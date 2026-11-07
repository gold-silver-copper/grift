//! An Emacs-style editor whose editor is grift code. The host gives grift bytes, files and shell
//! commands only: a key as a byte, writes to the terminal, the terminal's size, reading and writing
//! a file, running a command. The buffer, redisplay, the keymap, the minibuffer and every command
//! live in `editor.grift`.
//!
//! The loop is the host's (`run`). A grift run that wants a key or a command's end parks, and the
//! loop goes on: it reads what comes next, keys and what the commands did (`jobs.rs`), hands it to
//! the run waiting for it, draws, and when nothing is waiting for a key starts the next
//! `command-step`. So every command is a run of its own, and one that waits holds up nothing.
//!
//! A thread reads the terminal while grift runs, so C-g stops a command that never returns: it
//! holds an `Interrupter`, and the command ends with the error `interrupted`.
//!
//! `grift-emacs [--init INIT.grift] [FILE]`

mod jobs;

use grift::{
    Answer, Error, Grift, HostFunction, Kept, Parked, ParkingFunction, Ran, ResumeError, Value,
};
use jobs::{Event, Inbox, JobEvent};
use std::fmt;
use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

const EDITOR: &str = include_str!("editor.grift");
const USAGE: &str = "usage: grift-emacs [--init INIT.grift] [FILE]";

/// The longest error text shown: an error may carry a whole buffer.
const ERROR_TEXT_MAX: usize = 400;

/// How long a cleanup may run once nobody can type C-g: the terminal has closed, or the editor leaves.
const CLEANUP_MAX: Duration = Duration::from_secs(2);

/// What the host functions work on.
struct Host {
    /// Terminal output, written when the host's loop is about to wait.
    out: Vec<u8>,
    /// The bytes typed, from the input thread, and what the jobs have to say.
    inbox: Arc<Inbox>,
    /// How many temporary paths this editor has named.
    temporaries: u64,
    /// Functions `(spawn f)` asked to run, each as a run of its own.
    spawned: Vec<Kept>,
    /// Jobs whose waiting runs `(job-abandon job)` asked to end.
    abandoned: Vec<i64>,
    /// `(quit-editor)` was called.
    quitting: bool,
}

impl Host {
    fn flush(&mut self) {
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(&self.out).and_then(|()| stdout.flush());
        self.out.clear();
    }
}

type G = Grift<Host>;

/// `(key)`: the next byte from the terminal, as an integer. The run parks, and the newest run
/// waiting here gets the next key.
fn key(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Answer, Error> {
    if !args.is_empty() {
        return Err(g.error("arity-error", args));
    }
    Ok(Answer::Later(Value::Inert))
}

/// `(job-wait job)`: the job's exit status, once it ends. The run parks, under the job's number.
/// It must not have ended: nothing would answer.
fn job_wait(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Answer, Error> {
    let [Value::Integer(id)] = args else {
        return Err(g.error("type-error", args));
    };
    Ok(Answer::Later(Value::Integer(*id)))
}

/// `(job-start command)`: runs the shell command beside the editor and gives the job's number.
fn job_start(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [command] = args else {
        return Err(g.error("arity-error", args));
    };
    let Some(command) = g.as_str(command)?.map(str::to_owned) else {
        return Err(g.error("type-error", args));
    };
    let id = h.inbox.register();
    let inbox = Arc::clone(&h.inbox);
    std::thread::spawn(move || {
        let status = inbox.run(id, &command).ok().flatten();
        inbox.ended(id, status.unwrap_or(-1));
    });
    Ok(job_value(id))
}

fn job_value(id: u64) -> Value {
    Value::Integer(i64::try_from(id).unwrap_or(i64::MAX))
}

/// `(job-cancel job)`: kills the job's command. Its `ended` comes as usual; an ended job's
/// number does nothing.
fn job_cancel(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [Value::Integer(id)] = args else {
        return Err(g.error("type-error", args));
    };
    h.inbox.cancel(u64::try_from(*id).unwrap_or(0));
    Ok(Value::Inert)
}

/// `(job-abandon job)`: ends the runs waiting for the job, through their cleanups, when the run
/// calling this has come back to the host: nothing can be taken up inside a host call.
fn job_abandon(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [Value::Integer(id)] = args else {
        return Err(g.error("type-error", args));
    };
    h.abandoned.push(*id);
    Ok(Value::Inert)
}

/// `(spawn f)`: calls `f` as a run of its own, beside this one, when this one has come back to
/// the host. Its error, if it ends with one, goes to the echo area.
fn spawn(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [f] = args else {
        return Err(g.error("arity-error", args));
    };
    h.spawned.push(g.keep(f.clone())?);
    Ok(Value::Inert)
}

/// `(quit-editor)`: the host's loop ends when this run comes back to it, abandoning what waits.
fn quit_editor(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    if !args.is_empty() {
        return Err(g.error("arity-error", args));
    }
    h.quitting = true;
    Ok(Value::Inert)
}

/// `(temporary-file)`: a new path in the system's temporary directory; no file is made.
fn temporary_file(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    if !args.is_empty() {
        return Err(g.error("arity-error", args));
    }
    h.temporaries += 1;
    let name = format!("grift-emacs-{}-{}", std::process::id(), h.temporaries);
    g.string(&std::env::temp_dir().join(name).to_string_lossy())
}

/// `(delete-file path)`: removes the file; one that isn't there is no error.
fn delete_file(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [path] = args else {
        return Err(g.error("arity-error", args));
    };
    let Some(name) = g.as_str(path)? else {
        return Err(g.error("type-error", args));
    };
    match std::fs::remove_file(name) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(file_error(g, path, &e)),
        _ => Ok(Value::Inert),
    }
}

/// `(terminal-write string ...)`: queues the strings for the terminal.
fn terminal_write(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    for v in args {
        match g.as_str(v)? {
            Some(s) => h.out.extend_from_slice(s.as_bytes()),
            None => return Err(g.error("type-error", std::slice::from_ref(v))),
        }
    }
    Ok(Value::Inert)
}

/// `(terminal-size)`: the terminal's rows and columns, as a list.
fn terminal_size(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    if !args.is_empty() {
        return Err(g.error("arity-error", args));
    }
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let cols = g.pair(Value::Integer(i64::from(cols)), Value::EmptyList)?;
    g.pair(Value::Integer(i64::from(rows)), cols)
}

fn file_error(g: &mut G, path: &Value, e: &io::Error) -> Error {
    match g.string(&e.to_string()) {
        Ok(why) => g.error("file-error", &[path.clone(), why]),
        Err(e) => e,
    }
}

/// `(read-file path)`: the file's text; `()` when it doesn't exist.
fn read_file(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [path] = args else {
        return Err(g.error("arity-error", args));
    };
    let Some(name) = g.as_str(path)?.map(str::to_owned) else {
        return Err(g.error("type-error", args));
    };
    match std::fs::read_to_string(&name) {
        Ok(s) => g.string(&s),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Value::EmptyList),
        Err(e) => Err(file_error(g, path, &e)),
    }
}

/// `(write-file path string)`: replaces the file's contents with the string's bytes.
fn write_file(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [path, contents] = args else {
        return Err(g.error("arity-error", args));
    };
    let (Some(name), Some(s)) = (g.as_str(path)?, g.as_str(contents)?) else {
        return Err(g.error("type-error", args));
    };
    match std::fs::write(name, s) {
        Ok(()) => Ok(Value::Inert),
        Err(e) => Err(file_error(g, path, &e)),
    }
}

/// A `fmt::Write` into a string of one line that stops at `ERROR_TEXT_MAX` bytes, then adds `...`.
struct Capped(String);

impl fmt::Write for Capped {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if self.0.len() + c.len_utf8() > ERROR_TEXT_MAX {
                self.0.push_str("...");
                return Err(fmt::Error);
            }
            self.0.push(if c == '\n' { ' ' } else { c });
        }
        Ok(())
    }
}

/// An error as its kind, values and trace, written out now: the values may be collected by
/// the next `evaluate`.
fn describe(g: &G, e: &Error) -> String {
    let mut out = Capped(String::new());
    let _ = g.describe(e, &mut out);
    out.0
}

/// Calls the editor's function `name` with string arguments (`None` for `()`).
fn enter(g: &mut G, h: &mut Host, name: &str, args: &[Option<&str>]) -> Result<Value, Error> {
    let f = g.evaluate(h, name)?;
    let mut values = Vec::new();
    for a in args {
        values.push(match a {
            Some(s) => g.string(s)?,
            None => Value::EmptyList,
        });
    }
    g.call(h, &f, &values)
}

const HOST: [(&str, HostFunction<Host>); 11] = [
    ("job-start", job_start),
    ("job-cancel", job_cancel),
    ("job-abandon", job_abandon),
    ("spawn", spawn),
    ("quit-editor", quit_editor),
    ("temporary-file", temporary_file),
    ("delete-file", delete_file),
    ("terminal-write", terminal_write),
    ("terminal-size", terminal_size),
    ("read-file", read_file),
    ("write-file", write_file),
];

const PARKING: [(&str, ParkingFunction<Host>); 2] = [("key", key), ("job-wait", job_wait)];

/// The runs set aside, by what each waits for.
#[derive(Default)]
struct Runs {
    /// For a key, oldest first.
    keys: Vec<Parked>,
    /// For a job's end, with its number.
    jobs: Vec<(i64, Parked)>,
}

impl Runs {
    /// Where a run went: if it parked, among those waiting; if it failed, its error is reported in
    /// the echo area, and given back too, for whoever needed that run.
    fn settle(&mut self, g: &mut G, h: &mut Host, ran: Result<Ran, Error>) -> Option<String> {
        match ran {
            Ok(Ran::Finished(_)) => None,
            Ok(Ran::Parked(parked)) => {
                match parked.request() {
                    Value::Integer(id) => self.jobs.push((*id, parked)),
                    _ => self.keys.push(parked),
                }
                None
            }
            // That it was given up is no news.
            Err(e) if given_up(g, &e) => None,
            // With no run left reading keys, what the run left set for a while is the editor's to
            // put back. With one, the minibuffer may be that run's.
            Err(e) if self.keys.is_empty() => Some(report(g, h, "editor-recover", &e)),
            Err(e) => Some(report(g, h, "editor-report", &e)),
        }
    }

    /// Ends the runs waiting for job `id`, whose cleanups can't be stopped by one C-g.
    fn abandon_job(&mut self, g: &mut G, h: &mut Host, id: i64) {
        h.inbox.set_stoppable(false);
        for parked in self.take_job(id) {
            if let Some(again) = abandon(g, h, parked) {
                self.settle(g, h, Ok(Ran::Parked(again)));
            }
        }
    }

    /// The runs waiting for job `id`.
    fn take_job(&mut self, id: i64) -> Vec<Parked> {
        let (these, others) = std::mem::take(&mut self.jobs)
            .into_iter()
            .partition(|(job, _)| *job == id);
        self.jobs = others;
        these.into_iter().map(|(_, parked)| parked).collect()
    }

    /// Ends every run that waits, newest first, so each one's cleanups run.
    fn abandon_all(&mut self, g: &mut G, h: &mut Host) {
        h.inbox.set_stoppable(false);
        for parked in self.keys.drain(..).rev() {
            leave(g, h, parked);
        }
        for (_, parked) in self.jobs.drain(..).rev() {
            leave(g, h, parked);
        }
        // Any whose handle came back refused: each is tried once.
        for _ in 0..g.parked() {
            if let Some(parked) = g.forgotten() {
                leave(g, h, parked);
            }
        }
    }
}

/// Whether a run ended with this host's own limit for giving it up.
fn given_up(g: &G, e: &Error) -> bool {
    e.is_limit() && g.symbol_name(&e.kind).is_ok_and(|k| k == "abandoned")
}

/// Gives a parked run up: its waiting call answers a limit, which nothing catches, so its cleanups
/// run. That it was abandoned is no news; anything else that cut a cleanup is. A cleanup that waits
/// comes back, waiting.
fn abandon(g: &mut G, h: &mut Host, parked: Parked) -> Option<Parked> {
    let ending = g.error("abandoned", &[]).as_limit();
    let ResumeError { error, parked } = match g.resume(h, parked, Err(ending)) {
        Ok(Ran::Parked(again)) => return Some(again),
        Ok(Ran::Finished(_)) => return None,
        Err(e) => e,
    };
    if !given_up(g, &error) {
        report(g, h, "editor-report", &error);
    }
    parked
}

/// The editor is leaving, so nothing will answer what a cleanup waits for: each wait gets the same
/// limit, for as long as a cleanup may take; past that the run goes with the process, as an endless
/// cleanup would.
fn leave(g: &mut G, h: &mut Host, mut parked: Parked) {
    let began = Instant::now();
    while began.elapsed() < CLEANUP_MAX {
        match abandon(g, h, parked) {
            Some(again) => parked = again,
            None => return,
        }
    }
}

/// Puts an error in the echo area, through the editor's own function `through`, which is given its
/// text and its kind and words it; its text.
fn report(g: &mut G, h: &mut Host, through: &str, e: &Error) -> String {
    let text = describe(g, e);
    // If that fails too, the next frame or command says so.
    let _ = g.string(&text).and_then(|shown| {
        let f = g.evaluate(h, through)?;
        g.call(h, &f, &[shown, Value::Symbol(e.kind.clone())])
    });
    text
}

/// Tells the editor what a job did: `(output job text)` or `(ended job status)`.
fn job_event(g: &mut G, h: &mut Host, id: u64, event: &JobEvent) -> Result<(), Error> {
    let f = g.evaluate(h, "job-event")?;
    let (what, value) = match event {
        JobEvent::Output(text) => ("output", g.string(text)?),
        JobEvent::Ended(status) => ("ended", Value::Integer(*status)),
    };
    let what = g.symbol(what)?;
    let event = g.list(&[Value::Symbol(what), job_value(id), value])?;
    g.call(h, &f, &[event]).map(drop)
}

/// Runs the editor until it quits or input ends. An error before the first key (the file can't
/// be read) ends it. An error in any run goes to the echo area, and the other runs go on. If the
/// editor's own code can't read a key or draw a frame, the host asks before trying again.
fn run(g: &mut G, h: &mut Host, file: Option<&str>, init: Option<&str>) -> Result<(), String> {
    if let Err(e) = enter(g, h, "editor-main", &[file, init]) {
        return Err(describe(g, &e));
    }
    let mut runs = Runs::default();
    while !h.quitting {
        for kept in std::mem::take(&mut h.spawned) {
            let ran = g.kept(&kept).and_then(|f| g.start(h, &f, &[]));
            runs.settle(g, h, ran);
            let _ = g.release(kept);
        }
        for id in std::mem::take(&mut h.abandoned) {
            runs.abandon_job(g, h, id);
        }
        h.inbox.set_stoppable(false);
        let mut broken = None;
        if runs.keys.is_empty() {
            let ran = enter_parking(g, h, "command-step");
            broken = runs.settle(g, h, ran);
            if runs.keys.is_empty() {
                broken.get_or_insert_with(|| "command-step read no key".to_string());
            }
        }
        if let Err(e) = enter(g, h, "redisplay", &[]) {
            broken = Some(describe(g, &e));
        }
        h.flush();
        if let Some(text) = &broken {
            let mut out = io::stdout().lock();
            let _ = write!(
                out,
                "\x1b[999;1H\x1b[0m\x1b[K{text} -- the editor itself fails: q quits, any other key retries"
            );
            let _ = out.flush();
        }
        // Work those runs queued goes on at once, after anything already typed.
        let queued = !h.spawned.is_empty() || !h.abandoned.is_empty();
        let event = match queued {
            true => h.inbox.ready(),
            false => Some(h.inbox.next()),
        };
        match event {
            None => {}
            Some(Event::Key(b'q')) if broken.is_some() => break,
            Some(Event::Key(_)) if broken.is_some() => {}
            Some(Event::Key(b)) => {
                if let Some(parked) = runs.keys.pop() {
                    h.inbox.set_stoppable(true);
                    let answer = Ok(Value::Integer(i64::from(b)));
                    let ran = g.resume(h, parked, answer).map_err(Error::from);
                    runs.settle(g, h, ran);
                }
            }
            Some(Event::Closed) => break,
            Some(Event::Job(id, event)) => {
                if let Err(e) = job_event(g, h, id, &event) {
                    report(g, h, "editor-report", &e);
                }
                if let JobEvent::Ended(status) = event {
                    let id = i64::try_from(id).unwrap_or(i64::MAX);
                    for parked in runs.take_job(id) {
                        let ran = g.resume(h, parked, Ok(Value::Integer(status)));
                        let ran = ran.map_err(Error::from);
                        runs.settle(g, h, ran);
                    }
                }
            }
        }
    }
    runs.abandon_all(g, h);
    Ok(())
}

/// Starts the editor's function `name`, with no arguments, as a run that may park.
fn enter_parking(g: &mut G, h: &mut Host, name: &str) -> Result<Ran, Error> {
    let f = g.evaluate(h, name)?;
    g.start(h, &f, &[])
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut init, mut file) = (None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--init" if init.is_none() => match args.next() {
                Some(path) => init = Some(path),
                None => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            },
            _ if a.starts_with('-') || file.is_some() => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
            _ => file = Some(a),
        }
    }
    let init = match init.map(std::fs::read_to_string).transpose() {
        Ok(init) => init,
        Err(e) => {
            eprintln!("grift-emacs: --init: {e}");
            return ExitCode::from(2);
        }
    };
    let inbox = Arc::new(Inbox::default());
    let mut h = Host {
        out: Vec::new(),
        inbox: Arc::clone(&inbox),
        temporaries: 0,
        spawned: Vec::new(),
        abandoned: Vec::new(),
        quitting: false,
    };
    let mut g: G = match Grift::new() {
        Ok(g) => g,
        Err(_) => {
            eprintln!("grift-emacs: grift failed to start");
            return ExitCode::FAILURE;
        }
    };
    let mut stop = g.interrupter();
    for (name, f) in HOST {
        if g.host_function(name, f)
            .and_then(|f| g.define(name, f))
            .is_err()
        {
            eprintln!("grift-emacs: out of memory");
            return ExitCode::FAILURE;
        }
    }
    for (name, f) in PARKING {
        if g.parking_function(name, f)
            .and_then(|f| g.define(name, f))
            .is_err()
        {
            eprintln!("grift-emacs: out of memory");
            return ExitCode::FAILURE;
        }
    }
    if let Err(e) = g.evaluate(&mut h, EDITOR) {
        eprintln!("grift-emacs: editor.grift: {}", describe(&g, &e));
        return ExitCode::FAILURE;
    }
    // An init file that fails is reported in the echo area, not fatal.
    let init = match init.map(|src| g.evaluate(&mut h, &src)) {
        Some(Err(e)) => format!("init: {}", describe(&g, &e)),
        _ => String::new(),
    };
    if let Err(e) = crossterm::terminal::enable_raw_mode() {
        eprintln!("grift-emacs: the terminal: {e}");
        return ExitCode::FAILURE;
    }
    // A closed terminal is the end of input, so the cleanups run: its SIGHUP would end the
    // process first. A handler of any kind is enough for the read below to fail instead.
    if let Err(e) = signal_hook::flag::register(signal_hook::consts::SIGHUP, Arc::default()) {
        eprintln!("grift-emacs: SIGHUP: {e}");
        return ExitCode::FAILURE;
    }
    // The input thread: every byte typed goes to the host's loop, or stops what grift is busy with.
    // Once the input ends no C-g can come, so it keeps stopping what runs, which may be a command
    // that never returns to that loop or a cleanup on the way out. It ends when the program exits.
    std::thread::spawn(move || {
        let mut b = [0u8];
        while let Ok(1) = io::stdin().lock().read(&mut b) {
            inbox.typed(b[0], &mut stop);
        }
        inbox.input_closed();
        loop {
            std::thread::sleep(CLEANUP_MAX);
            stop.stop();
        }
    });
    // The alternate screen, with lines clipped at the right edge instead of wrapped.
    print!("\x1b[?1049h\x1b[?7l");
    let ran = run(&mut g, &mut h, file.as_deref(), Some(&init));
    h.flush();
    print!("\x1b[?7h\x1b[?1049l");
    let _ = io::stdout().flush();
    let _ = crossterm::terminal::disable_raw_mode();
    match ran {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("grift-emacs: {e}");
            ExitCode::FAILURE
        }
    }
}
