//! Stopping a run from another thread. An interpreter and its interrupters share one word: the run
//! in progress, 0 for none, and two bits a run is asked to stop by. Everything here is one atomic
//! operation on that word, so nothing can come between looking at the run and asking it: a stop
//! reaches the run in progress as it returns, or none, and never a later one. A new run overwrites
//! the word, bits and all. The machine reads the word once a step and clears only [`STOP`], so a
//! later stop asks again and a force, which it never clears, is found by every check to the end of
//! its run. No other memory is published through the word, so every ordering is relaxed.

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

    /// Whether the run in progress is asked to stop. Read at every step.
    #[inline(always)]
    pub fn asked(&self) -> bool {
        self.word.load(Relaxed) & ASKED != 0
    }

    /// Sets `bit`, in one atomic step, if a run is in progress and it is one `wanted`; which it was.
    fn ask(&self, bit: u64, wanted: impl Fn(u64) -> bool) -> Option<u64> {
        let mut word = self.word.load(Relaxed);
        loop {
            let run = word & !ASKED;
            if run == 0 || !wanted(run) {
                return None;
            }
            let set = word | bit;
            match self.word.compare_exchange_weak(word, set, Relaxed, Relaxed) {
                Ok(_) => return Some(run),
                Err(now) => word = now,
            }
        }
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

/// Stops the run of its [`Grift`](crate::Grift) that is in progress, from any thread: the run
/// ends with the error `interrupted` at its next step, or inside a long walk of a tree. That error
/// runs cleanups on its way out as any error does, but a stop can also land in a cleanup that has
/// begun, and cut it, even before its first form. A stop with no run in progress does nothing, and
/// none reaches a run that starts after it returns. Stops waiting together count as one; a later
/// stop of the same run, say of a cleanup that doesn't end, interrupts it again. A parked run is
/// not in progress, so a stop then does nothing and says so; one that reached a run before it
/// parked stops it there, its cleanups then running, and they may park in turn; and each
/// [`Grift::resume`](crate::Grift::resume) is a run of its own. [`Interrupter::force`] ends a run
/// however its cleanups behave.
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
#[derive(Clone)]
pub struct Interrupter {
    control: Arc<Control>,
    /// The run this handle last reached, 0 for none, which is all its force may reach. A clone
    /// starts with its source's.
    reached: u64,
}

impl Interrupter {
    pub(crate) fn new(control: &Arc<Control>) -> Interrupter {
        Interrupter {
            control: Arc::clone(control),
            reached: 0,
        }
    }

    /// Stops the run in progress, if there is one; whether there was. `true` doesn't say the stop
    /// acted: a run about to end can still return its value. A watchdog whose deadline
    /// passed before its run began (while its source is read, say) asks again until one is, and
    /// no longer once the host knows its run ended: else it would stop the next.
    pub fn stop(&mut self) -> bool {
        self.ask(STOP, |_| true)
    }

    /// Ends the run this handle last reached by a stop or a force, or, if it has reached none, the
    /// run in progress: at its next check, however its cleanups behave. Each cleanup it meets is cut
    /// before its first form, at every level of the run, nothing a host function then evaluates
    /// runs, and the run ends with `interrupted`, whatever else fails on the way. Whether that run
    /// is in progress; so a handle that reached a run that is over forces nothing, ever, until a
    /// stop ties it to another, and a late force never reaches a later run. A handle that has
    /// reached nothing has the hazard a stop has: asked after its run ended, it reaches the next.
    /// `true` doesn't say it acted, as for a stop. [`Error::is_limit`] says the ending is real.
    ///
    /// [`Error::is_limit`]: crate::Error::is_limit
    pub fn force(&mut self) -> bool {
        let reached = self.reached;
        self.ask(FORCE, |run| reached == 0 || run == reached)
    }

    fn ask(&mut self, bit: u64, wanted: impl Fn(u64) -> bool) -> bool {
        let run = self.control.ask(bit, wanted);
        self.reached = run.unwrap_or(self.reached);
        run.is_some()
    }
}
