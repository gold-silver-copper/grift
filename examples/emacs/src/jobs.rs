//! Shell commands that run beside the editor. grift runs one thing at a time, on the host's
//! thread, so nothing here touches an interpreter from another thread: a thread per command moves
//! bytes into the `Inbox`, and the host's loop reads them from the same place as keys.

use grift::Interrupter;
use std::collections::{BTreeMap, VecDeque};
use std::io::{self, PipeReader, Read};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, Stdio};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// C-g, the byte that interrupts.
pub const QUIT: u8 = 7;

/// How many bytes of output may wait for the editor. Past this a command's pipe fills and the
/// command itself waits, so a flood costs the editor no memory. It is also the most one event
/// holds, and a key typed during an event waits for it: grift takes about 1 ms per kilobyte.
const QUEUED_MAX: usize = 8 * 1024;

/// What the host's loop reads next.
pub enum Event {
    Key(u8),
    /// The terminal closed and every key has been read.
    Closed,
    Job(u64, JobEvent),
}

pub enum JobEvent {
    Output(String),
    /// The exit status.
    Ended(i64),
}

/// A job the host can still cancel.
#[derive(Default)]
struct Running {
    /// The process group of its command, while it has one.
    group: Option<u32>,
    cancelled: bool,
}

impl Running {
    fn cancel(&mut self) {
        self.cancelled = true;
        if let Some(group) = self
            .group
            .and_then(|g| rustix::process::Pid::from_raw(g as i32))
        {
            let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
        }
    }
}

#[derive(Default)]
struct State {
    keys: VecDeque<u8>,
    closed: bool,
    /// A key's run is going on: a C-g stops it. Otherwise grift is idle, drawing or cleaning up,
    /// and a C-g is a key, for what it stops must not be a cleanup or the drawing of a frame.
    stoppable: bool,
    events: VecDeque<(u64, JobEvent)>,
    queued: usize,
    running: BTreeMap<u64, Running>,
    next_id: u64,
}

/// Keys and job events, from every thread, for the host's loop.
#[derive(Default)]
pub struct Inbox {
    state: Mutex<State>,
    wake: Condvar,
    room: Condvar,
}

impl Inbox {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A byte from the terminal. A C-g stops a command that is busy, which is all it should do.
    /// Any other is a key like any other, unless the last one is still unread: then what grift is
    /// doing isn't ending, and is stopped, which settles both.
    pub fn typed(&self, b: u8, stop: &mut Interrupter) {
        let mut s = self.state();
        if b == QUIT && (s.stoppable || s.keys.contains(&QUIT)) && stop.stop() {
            s.keys.retain(|k| *k != QUIT);
            return;
        }
        s.keys.push_back(b);
        self.wake.notify_all();
    }

    /// Whether a C-g stops what grift does next; `next` sets it back.
    pub fn set_stoppable(&self, stoppable: bool) {
        self.state().stoppable = stoppable;
    }

    pub fn input_closed(&self) {
        self.state().closed = true;
        self.wake.notify_all();
    }

    /// The next key or job event: keys first, so output can't delay typing.
    pub fn next(&self) -> Event {
        let mut s = self.state();
        s.stoppable = false;
        loop {
            if let Some(event) = self.waiting(&mut s) {
                return event;
            }
            s = self.wake.wait(s).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// What `next` would give, if anything has come.
    pub fn ready(&self) -> Option<Event> {
        let mut s = self.state();
        s.stoppable = false;
        self.waiting(&mut s)
    }

    fn waiting(&self, s: &mut State) -> Option<Event> {
        if let Some(b) = s.keys.pop_front() {
            return Some(Event::Key(b));
        }
        self.next_job_event(s)
            .or_else(|| s.closed.then_some(Event::Closed))
    }

    fn next_job_event(&self, s: &mut State) -> Option<Event> {
        let (id, event) = s.events.pop_front()?;
        if let JobEvent::Output(text) = &event {
            s.queued = s.queued.saturating_sub(text.len());
            self.room.notify_all();
        }
        Some(Event::Job(id, event))
    }

    /// A new job's number.
    pub fn register(&self) -> u64 {
        let mut s = self.state();
        s.next_id += 1;
        let id = s.next_id;
        s.running.insert(id, Running::default());
        id
    }

    /// Names a job's process group. False if the job was cancelled already, and the group killed.
    fn set_group(&self, id: u64, group: Option<u32>) -> bool {
        let mut s = self.state();
        let Some(job) = s.running.get_mut(&id) else {
            return false;
        };
        job.group = group;
        if job.cancelled {
            job.cancel();
        }
        !job.cancelled
    }

    /// Kills a job's command. A job that has ended is left alone.
    pub fn cancel(&self, id: u64) {
        if let Some(job) = self.state().running.get_mut(&id) {
            job.cancel();
        }
        self.room.notify_all();
    }

    pub fn ended(&self, id: u64, status: i64) {
        let mut s = self.state();
        s.running.remove(&id);
        s.events.push_back((id, JobEvent::Ended(status)));
        self.wake.notify_all();
    }

    /// Queues output for the editor, waiting while too much is queued already.
    fn output(&self, id: u64, text: String) {
        let mut s = self.state();
        while s.queued >= QUEUED_MAX && !s.running.get(&id).is_some_and(|job| job.cancelled) {
            s = self.room.wait(s).unwrap_or_else(PoisonError::into_inner);
        }
        // A cancelled command's output is dropped: the pipe can hold a second of it.
        if s.running.get(&id).is_some_and(|job| job.cancelled) {
            return;
        }
        s.queued += text.len();
        match s.events.back_mut() {
            Some((last, JobEvent::Output(queued))) if *last == id => queued.push_str(&text),
            _ => s.events.push_back((id, JobEvent::Output(text))),
        }
        self.wake.notify_all();
    }

    /// Runs `command` as job `id` to its end: its output goes to the editor, stdout and stderr
    /// together. Its exit status, or 128 and the signal that killed it, as a shell says it.
    /// `None` if the job was cancelled before it started.
    pub fn run(&self, id: u64, command: &str) -> io::Result<Option<i64>> {
        let (reader, writer) = io::pipe()?;
        let child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer)
            .process_group(0)
            .spawn()?;
        if !self.set_group(id, Some(child.id())) {
            self.pump(id, reader, child)?;
            return Ok(None);
        }
        let status = self.pump(id, reader, child);
        self.set_group(id, None);
        status.map(Some)
    }

    fn pump(&self, id: u64, mut reader: PipeReader, mut child: Child) -> io::Result<i64> {
        let mut bytes = Vec::new();
        let mut chunk = [0u8; QUEUED_MAX];
        loop {
            let n = reader.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(chunk.get(..n).unwrap_or_default());
            let text = whole_characters(&mut bytes);
            if !text.is_empty() {
                self.output(id, text);
            }
        }
        if !bytes.is_empty() {
            self.output(id, String::from_utf8_lossy(&bytes).into_owned());
        }
        let status = child.wait()?;
        Ok(status
            .code()
            .or(status.signal().map(|s| 128 + s))
            .map_or(-1, i64::from))
    }
}

/// The text of `bytes`, leaving the start of a character that a later read will finish.
fn whole_characters(bytes: &mut Vec<u8>) -> String {
    let end = match std::str::from_utf8(bytes) {
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        _ => bytes.len(),
    };
    let rest = bytes.split_off(end);
    let text = String::from_utf8_lossy(bytes).into_owned();
    *bytes = rest;
    text
}
