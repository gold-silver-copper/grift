//! Stopping a run from another thread. An interpreter and its interrupters share one word: the run
//! in progress, 0 for none, and two bits a run is asked to stop by. Every handle names one run, the
//! one it was taken for, and everything here is one atomic operation on that word, so nothing can
//! come between looking at the run and asking it: a handle reaches its run while that run is in
//! progress, and never another. A new run overwrites the word, bits and all. The machine reads the
//! word once a step and clears only [`STOP`], so a later stop asks again and a force, which it
//! never clears, is found by every check to the end of its run. No other memory is published
//! through the word, so every ordering is relaxed.

use std::collections::TryReserveError;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard};

const STOP: u64 = 1;
const FORCE: u64 = 2;
const ASKED: u64 = STOP | FORCE;

/// What an interpreter shares with its interrupters. Its allocation is also the interpreter's
/// identity: the values it hands out point to it.
#[derive(Default)]
pub(crate) struct Control {
    word: AtomicU64,
    /// The last run's number, a multiple of 4 that leaves the two bits free. Never repeated: at a
    /// run a nanosecond it wraps after a century.
    runs: AtomicU64,
    /// The kept names of parked runs whose handles were dropped, for the host to find again.
    forgotten: Mutex<Vec<u32>>,
}

impl Control {
    pub fn begin(&self) {
        let run = self.runs.fetch_add(ASKED + 1, Relaxed);
        self.word.store(run.wrapping_add(ASKED + 1), Relaxed);
    }

    pub fn end(&self) {
        self.word.store(0, Relaxed);
    }

    /// The run in progress, 0 for none.
    pub fn run(&self) -> u64 {
        self.word.load(Relaxed) & !ASKED
    }

    /// The run a handle taken now is for: the one in progress, else the next to begin.
    fn meant(&self) -> u64 {
        match self.run() {
            0 => self.runs.load(Relaxed).wrapping_add(ASKED + 1),
            run => run,
        }
    }

    /// Whether the run in progress is asked to stop. Read at every step.
    #[inline(always)]
    pub fn asked(&self) -> bool {
        self.word.load(Relaxed) & ASKED != 0
    }

    /// Sets `bit`, in one atomic step, if `run` is in progress; whether it was.
    fn ask(&self, bit: u64, run: u64) -> bool {
        let mut word = self.word.load(Relaxed);
        while word & !ASKED == run {
            match self
                .word
                .compare_exchange_weak(word, word | bit, Relaxed, Relaxed)
            {
                Ok(_) => return true,
                Err(now) => word = now,
            }
        }
        false
    }

    /// The machine acts on the stops asked so far: they count as one.
    #[cold]
    pub fn acted(&self) {
        self.word.fetch_and(!STOP, Relaxed);
    }

    /// Unpublishes the run to park it, unless it was asked to stop.
    pub fn parks(&self) -> bool {
        let run = self.run();
        self.word.compare_exchange(run, 0, Relaxed, Relaxed).is_ok()
    }

    /// The parked runs no handle holds. Nothing panics under the lock.
    pub fn forgotten(&self) -> MutexGuard<'_, Vec<u32>> {
        self.forgotten.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Room for `runs` in that queue, so that adding one up to there needs no memory.
    pub fn room_to_forget(&self, runs: usize) -> Result<(), TryReserveError> {
        let mut forgotten = self.forgotten();
        let more = runs.saturating_sub(forgotten.len());
        forgotten.try_reserve(more)
    }
}

/// Stops the one run of its [`Grift`](crate::Grift) it was taken for, from any thread: the run in
/// progress when [`Grift::interrupter`](crate::Grift::interrupter) made it, or, if none was, the
/// next to begin. That run ends with the error `interrupted` at its next step, or inside a long
/// walk of a tree. That error runs cleanups on its way out as any error does, but a stop can also
/// land in a cleanup that has begun, and cut it, even before its first form. A stop while its run
/// is not in progress does nothing and says so: before it begins, after it ends, while it is
/// parked, and in every other run, so no handle, however old, reaches a run its host didn't mean.
/// Stops waiting together count as one; a later stop of the same run, say of a cleanup that doesn't
/// end, interrupts it again. One that reached a run before it parked stops it there, its cleanups
/// then running, and they may park in turn. Each [`Grift::resume`](crate::Grift::resume) is a run
/// of its own: take a handle for it before resuming. A run inside a host call is part of the run
/// that made the call. [`Interrupter::force`] ends a run however its cleanups behave.
///
/// ```
/// let mut g: grift::Grift = grift::Grift::new()?;
/// // Taken between runs: it is for the next one.
/// let mut stopper = g.interrupter();
/// let timer = std::thread::spawn(move || {
///     std::thread::sleep(std::time::Duration::from_millis(20));
///     // A stop before its run has begun does nothing: ask until it has. Once that run is over,
///     // this handle reaches nothing, so a watchdog may go on asking.
///     while !stopper.stop() {
///         std::thread::sleep(std::time::Duration::from_millis(1));
///     }
/// });
/// let e = g.evaluate(&mut (), "(define! spin (lambda! () (spin))) (spin)").unwrap_err();
/// assert_eq!(g.symbol_name(&e.kind)?, "interrupted");
/// timer.join().unwrap();
/// // The next run is not stopped.
/// assert_eq!(g.evaluate(&mut (), "(add 1 2)")?, grift::Value::Integer(3));
/// # Ok::<(), grift::Error>(())
/// ```
#[derive(Clone)]
pub struct Interrupter {
    control: Arc<Control>,
    /// The run this handle is for, by its number. A clone is for the same run.
    run: u64,
}

impl Interrupter {
    pub(crate) fn new(control: &Arc<Control>) -> Interrupter {
        Interrupter {
            control: Arc::clone(control),
            run: control.meant(),
        }
    }

    /// Stops this handle's run, if it is in progress; whether it was. `true` doesn't say the stop
    /// acted: a run about to end can still return its value.
    pub fn stop(&mut self) -> bool {
        self.control.ask(STOP, self.run)
    }

    /// Ends this handle's run, if it is in progress, at its next check, however its cleanups behave;
    /// whether it was. No stop need come first. Each cleanup it meets is cut before its first form,
    /// at every level of the run, nothing a host function then evaluates runs past the one
    /// primitive it may call directly, and the run ends with `interrupted`, whatever else fails on
    /// the way. `true` doesn't say it acted, as for a stop. [`Error::limit`] says the ending is
    /// real.
    ///
    /// [`Error::limit`]: crate::Error::limit
    pub fn force(&mut self) -> bool {
        self.control.ask(FORCE, self.run)
    }
}
