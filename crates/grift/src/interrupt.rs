//! Stopping a run from another thread. Each outer run (an evaluation or call that isn't inside
//! another) gets the next number, its generation, published in the interpreter's [`Control`] while
//! it runs and 0 otherwise. A stop reads it and, if a run is in progress, leaves that number in
//! the stopping handle's own mailbox; the machine stops only for its own number. So a stop asked
//! for between runs does nothing, and a late one never reaches a later run. Everything is a load
//! or a store: no read-modify-write.

use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

/// What an interpreter shares with its interrupters. Its allocation is also the interpreter's
/// identity: the values it hands out point to it.
#[derive(Default)]
pub(crate) struct Control {
    /// The run in progress, 0 for none.
    active: AtomicU64,
    /// Set by a stop after it leaves its number, so the machine reads the mailboxes only then.
    asked: AtomicBool,
    /// The first interrupter's mailbox; each links to the next.
    first: OnceLock<Arc<Mailbox>>,
}

/// One interrupter's mailbox: the run it last asked to stop. Only that interrupter writes it.
#[derive(Default)]
struct Mailbox {
    requested: AtomicU64,
    next: OnceLock<Arc<Mailbox>>,
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

    /// Whether a stop of `run` was asked for. Read at every step: one load, unless one was.
    #[inline(always)]
    pub fn stopped(&self, run: u64) -> bool {
        self.asked.load(Ordering::Acquire) && self.requested(run)
    }

    #[cold]
    fn requested(&self, run: u64) -> bool {
        // Cleared before reading, so a stop that leaves its number after the read sets it again.
        self.asked.store(false, Ordering::SeqCst);
        let mut link = self.first.get();
        while let Some(m) = link {
            if m.requested.load(Ordering::SeqCst) == run {
                return true;
            }
            link = m.next.get();
        }
        false
    }
}

/// Drops a chain of mailboxes one at a time, not by recursion that grows with its length.
fn unlink(mut next: Option<Arc<Mailbox>>) {
    while let Some(m) = next {
        next = Arc::try_unwrap(m).ok().and_then(|mut m| m.next.take());
    }
}

impl Drop for Control {
    fn drop(&mut self) {
        unlink(self.first.take());
    }
}

impl Drop for Mailbox {
    fn drop(&mut self) {
        unlink(self.next.take());
    }
}

/// Stops the run of its [`Grift`](crate::Grift) that is in progress, from any thread: the run
/// ends with the error `interrupted` at its next step, and its cleanups run. A stop asked for
/// while no run is in progress does nothing, and one never reaches a run that starts after it
/// returns. `Send`, not `Sync`: each clone is a handle of its own, used by one thread at a time.
/// Each clone keeps a mailbox of a few bytes until the interpreter and every clone are dropped.
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
    not_shared: PhantomData<Cell<()>>,
}

impl Interrupter {
    /// A new handle with its own mailbox, linked at the end of the chain.
    pub(crate) fn new(control: &Arc<Control>) -> Interrupter {
        let mailbox = Arc::new(Mailbox::default());
        let mut link = &control.first;
        let mut adding = Arc::clone(&mailbox);
        loop {
            match link.get() {
                Some(m) => link = &m.next,
                // Another thread may link one first: then go on past it.
                None => match link.set(adding) {
                    Ok(()) => break,
                    Err(back) => adding = back,
                },
            }
        }
        Interrupter {
            control: Arc::clone(control),
            mailbox,
            not_shared: PhantomData,
        }
    }

    /// Stops the run in progress, if there is one.
    pub fn stop(&mut self) {
        let run = self.control.active.load(Ordering::Acquire);
        if run != 0 {
            self.mailbox.requested.store(run, Ordering::SeqCst);
            self.control.asked.store(true, Ordering::SeqCst);
        }
    }
}

impl Clone for Interrupter {
    /// Another handle, with a mailbox of its own.
    fn clone(&self) -> Interrupter {
        Interrupter::new(&self.control)
    }
}

impl std::fmt::Debug for Interrupter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Interrupter")
    }
}
