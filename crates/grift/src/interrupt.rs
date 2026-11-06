//! Stopping a run from another thread. Each outer run (an evaluation or call that isn't inside
//! another) gets the next number, its generation, published in the interpreter's [`Control`] while
//! it runs and 0 otherwise. A stop reads it and, if a run is in progress, leaves that number in
//! the stopping handle's own mailbox; the machine stops only for its own number. So a stop asked
//! for between runs does nothing, and a late one never reaches a later run. A stop is a load and
//! a few stores, never a lock or a read-modify-write; the machine reads one flag a step, and only
//! once a stop has set it takes the lock and reads the mailboxes.
//!
//! A run that parks goes on later, so a stop must not be lost in the moment it parks: a stop first
//! says it is stopping and then reads the run, and a run about to park first unpublishes itself
//! and then looks for stops. Whichever order they come in, one sees the other: [`Control::parks`].

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
    /// The kept names of parked runs whose handles were dropped, for the host to find again.
    forgotten: Mutex<Vec<u32>>,
}

/// One interrupter's mailbox: the run its last stop was for and how many stops it has made, which
/// only that interrupter writes, and the last of those the machine acted on, which only the
/// machine writes.
#[derive(Default)]
struct Mailbox {
    requested: AtomicU64,
    count: AtomicU64,
    delivered: AtomicU64,
    /// Set for as long as the handle's stop takes.
    stopping: AtomicBool,
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

    /// Unpublishes `run` to park it, unless something was asked: then `run` goes on being the run
    /// in progress, and what was asked is the machine's to judge, as at any check. A stop that read
    /// `run` has said it is stopping before this unpublishes it, so the wait below sees it, and
    /// that wait is for a few stores, as the lock is. One that reads 0 does nothing and says so. The
    /// lock comes first so that no handle lets go of a waiting mailbox while no run is published.
    pub fn parks(&self, run: u64) -> bool {
        let list = self.mailboxes();
        self.active.store(0, Ordering::SeqCst);
        for m in list.iter() {
            while m.stopping.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
        }
        let asked = self.asked.load(Ordering::SeqCst);
        if asked {
            self.active.store(run, Ordering::SeqCst);
        }
        !asked
    }

    /// Whether a stop of `run` was asked for since the last one acted on. Read at every step: one
    /// load, unless one was.
    #[inline(always)]
    pub fn stopped(&self, run: u64) -> bool {
        self.asked.load(Ordering::Acquire) && self.requested(run)
    }

    #[cold]
    fn requested(&self, run: u64) -> bool {
        // With no run in progress, never: a mailbox a stop is still writing may hold its new count
        // beside 0. The flag stays for the next run to read.
        if run == 0 {
            return false;
        }
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

    /// The parked runs no handle holds. As with the mailboxes, nothing panics under the lock.
    pub fn forgotten(&self) -> MutexGuard<'_, Vec<u32>> {
        self.forgotten.lock().unwrap_or_else(|p| p.into_inner())
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
/// say of a cleanup that doesn't end, interrupts it again. A parked run is not in progress, so a
/// stop then does nothing and says so; one that reached a run as it parked stops it there, its
/// cleanups then running, and they may park in turn; and each [`Grift::resume`](crate::Grift::resume)
/// is a run of its own. `Send`, not `Sync`: each clone is a
/// handle of its own, used by one thread at a time, with a mailbox of a few bytes while it lives.
///
/// ```
/// let mut g: grift::Grift = grift::Grift::new()?;
/// let mut stopper = g.interrupter();
/// let timer = std::thread::spawn(move || {
///     std::thread::sleep(std::time::Duration::from_millis(20));
///     // A stop before the run has begun does nothing: ask until it has. This run never ends by
///     // itself; a watchdog whose run may end stops asking once the host knows it has.
///     while !stopper.stop() {
///         std::thread::sleep(std::time::Duration::from_millis(1));
///     }
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
    /// passed before its run began (while its source is read, say) asks again until one is, and
    /// no longer once the host knows its run ended: else it would stop the next.
    pub fn stop(&mut self) -> bool {
        let run = self.stop_begun();
        self.stop_landed(run);
        run != 0
    }

    /// A stop's first half: it says it is stopping, and reads the run, which is what it will stop.
    pub(crate) fn stop_begun(&mut self) -> u64 {
        self.mailbox.stopping.store(true, Ordering::SeqCst);
        self.control.active.load(Ordering::SeqCst)
    }

    /// A stop's second half, which tests can hold apart from the first.
    pub(crate) fn stop_landed(&mut self, run: u64) {
        if run != 0 {
            self.sent += 1;
            self.mailbox.count.store(self.sent, Ordering::Release);
            self.mailbox.requested.store(run, Ordering::SeqCst);
            self.control.asked.store(true, Ordering::SeqCst);
        }
        self.mailbox.stopping.store(false, Ordering::SeqCst);
    }

    /// A stop's first store alone, as a stop preempted before it stores its run is seen: for
    /// tests of that interleaving.
    #[cfg(test)]
    pub(crate) fn stop_halfway(&mut self) {
        self.sent += 1;
        self.mailbox.count.store(self.sent, Ordering::Release);
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
