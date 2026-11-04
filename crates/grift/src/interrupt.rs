//! Stopping a run from another thread. Each outer run (an evaluation or call that isn't inside
//! another) gets the next number, its generation, published in the interpreter's [`Control`] while
//! it runs and 0 otherwise. A stop reads it and, if a run is in progress, leaves that number in
//! the stopping handle's own mailbox; the machine stops only for its own number. So a stop asked
//! for between runs does nothing, and a late one never reaches a later run. Everything is a load
//! or a store: no read-modify-write.

use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// What an interpreter shares with its interrupters. Its allocation is also the interpreter's
/// identity: the values it hands out point to it.
#[derive(Default)]
pub(crate) struct Control {
    /// The run in progress, 0 for none.
    active: AtomicU64,
    /// Set by a stop after it leaves its request, so the machine reads the mailboxes only then.
    asked: AtomicBool,
    /// The live interrupters' mailboxes. A stop never takes the lock: registering a handle and
    /// the machine's scan do, so a scan sees every handle linked before it began.
    mailboxes: Mutex<Vec<Arc<Mailbox>>>,
}

/// One interrupter's mailbox: the run its last stop was for and how many stops it has made, which
/// only that interrupter writes, and the last of those the machine acted on, which only the
/// machine writes.
#[derive(Default)]
struct Mailbox {
    requested: AtomicU64,
    count: AtomicU64,
    delivered: AtomicU64,
}

impl Mailbox {
    /// Whether a stop of `run` is waiting here that the machine hasn't acted on.
    fn waiting(&self, run: u64) -> bool {
        self.requested.load(Ordering::SeqCst) == run
            && self.count.load(Ordering::Acquire) != self.delivered.load(Ordering::Relaxed)
    }
}

impl Control {
    /// Publishes the run in progress. Release suffices: a stop that reads an older number stores
    /// a generation no later run has, so it can't reach one.
    pub fn begin(&self, run: u64) {
        self.active.store(run, Ordering::Release);
    }

    pub fn end(&self) {
        self.active.store(0, Ordering::Release);
    }

    /// The run published as in progress, 0 for none.
    pub fn active(&self) -> u64 {
        self.active.load(Ordering::Relaxed)
    }

    /// Whether a stop of `run` was asked for since the last one acted on. Read at every step: one
    /// load, unless one was.
    #[inline(always)]
    pub fn stopped(&self, run: u64) -> bool {
        self.asked.load(Ordering::Acquire) && self.requested(run)
    }

    #[cold]
    fn requested(&self, run: u64) -> bool {
        // Cleared before reading, so a stop that leaves its request after the read sets it again.
        self.asked.store(false, Ordering::SeqCst);
        let mut stopped = false;
        for m in self.mailboxes().iter() {
            if m.waiting(run) {
                // Every request waiting is answered by this one interruption.
                m.delivered
                    .store(m.count.load(Ordering::Acquire), Ordering::Relaxed);
                stopped = true;
            }
        }
        stopped
    }

    fn mailboxes(&self) -> MutexGuard<'_, Vec<Arc<Mailbox>>> {
        // Nothing panics while holding the lock, so it is never poisoned; if it were, the list
        // is still whole.
        self.mailboxes.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Lets go of the mailboxes no handle holds, but those with a stop still waiting for the run
    /// in progress, which a later sweep lets go.
    fn sweep(&self, list: &mut Vec<Arc<Mailbox>>, leaving: Option<&Arc<Mailbox>>) {
        let run = self.active();
        list.retain(|m| {
            let gone = leaving.is_some_and(|l| Arc::ptr_eq(l, m)) || Arc::strong_count(m) == 1;
            !gone || (run != 0 && m.waiting(run))
        });
    }
}

/// Stops the run of its [`Grift`](crate::Grift) that is in progress, from any thread: the run
/// ends with the error `interrupted` at its next step, and its cleanups run. A stop asked for
/// while no run is in progress does nothing, and one never reaches a run that starts after it
/// returns. Each stop is acted on once: the run's cleanups run, and a later stop of the same run,
/// say of a cleanup that doesn't end, interrupts it again. `Send`, not `Sync`: each clone is a
/// handle of its own, used by one thread at a time, with a mailbox of a few bytes while it lives.
///
/// ```
/// let mut g: grift::Grift = grift::Grift::new()?;
/// let mut stopper = g.interrupter();
/// let timer = std::thread::spawn(move || {
///     std::thread::sleep(std::time::Duration::from_millis(20));
///     stopper.stop();
/// });
/// let e = g.evaluate(&mut (), "(define! spin (lambda! () (spin))) (spin)").unwrap_err();
/// assert!(g.is_kind(&e, "interrupted")?);
/// timer.join().unwrap();
/// // The next run is not stopped.
/// assert_eq!(g.evaluate(&mut (), "(add 1 2)")?, grift::Value::Integer(3));
/// # Ok::<(), grift::Error>(())
/// ```
pub struct Interrupter {
    control: Arc<Control>,
    mailbox: Arc<Mailbox>,
    /// The stops this handle has made.
    sent: u64,
    not_shared: PhantomData<Cell<()>>,
}

impl Interrupter {
    /// A new handle with its own mailbox.
    pub(crate) fn new(control: &Arc<Control>) -> Interrupter {
        let mailbox = Arc::new(Mailbox::default());
        let mut list = control.mailboxes();
        control.sweep(&mut list, None);
        list.push(Arc::clone(&mailbox));
        drop(list);
        Interrupter {
            control: Arc::clone(control),
            mailbox,
            sent: 0,
            not_shared: PhantomData,
        }
    }

    /// Stops the run in progress, if there is one; whether there was. A watchdog whose deadline
    /// passed before its run began (while its source is read, say) asks again until one is.
    pub fn stop(&mut self) -> bool {
        let run = self.control.active.load(Ordering::Acquire);
        if run != 0 {
            self.sent += 1;
            self.mailbox.count.store(self.sent, Ordering::Release);
            self.mailbox.requested.store(run, Ordering::SeqCst);
            self.control.asked.store(true, Ordering::SeqCst);
        }
        run != 0
    }
}

impl Drop for Interrupter {
    fn drop(&mut self) {
        let mut list = self.control.mailboxes();
        self.control.sweep(&mut list, Some(&self.mailbox));
    }
}

impl Clone for Interrupter {
    /// Another handle, with a mailbox of its own.
    fn clone(&self) -> Interrupter {
        Interrupter::new(&self.control)
    }
}
