//! Stopping a run from another thread. An interpreter and its interrupters share one word: the
//! number of the run in progress or else of the next to begin, whether it is in progress and may
//! park, and two bits a run is asked to stop by. Every handle names one run, the one it was taken
//! for, and everything here is one atomic operation on that word, so nothing can come between
//! looking at the run and asking it: a handle reaches its run while that run is in progress, and
//! never another. Ending a run, by parking too, moves to the next number and clears the rest. The
//! machine reads the word once a step and clears only [`STOP`], so a later stop asks again and a
//! force, which it never clears, is found by every check to the end of its run. No other memory is
//! published through the word, so every ordering is relaxed. Numbers are multiples of [`NEXT`]:
//! never repeated, at a run a nanosecond they wrap after thirty-six years.

use crate::{WALK_CHECK, evaluate::Prepared, parked::MayPark};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard};

const STOP: u64 = 1;
const FORCE: u64 = 2;
const ASKED: u64 = STOP | FORCE;
const RUNNING: u64 = 4;
const PARKS: u64 = 8;
const NEXT: u64 = 16;
/// The bits that don't say which run: a handle taken before its run began names it as well.
const NOT_NUMBER: u64 = ASKED | PARKS;

/// The word that ends the run `word` is, or is the number of.
fn after(word: u64) -> u64 {
    (word & !(NOT_NUMBER | RUNNING)).wrapping_add(NEXT)
}

/// What an interpreter shares with its interrupters and the handles of its parked runs.
#[derive(Default)]
pub(crate) struct Control {
    word: AtomicU64,
    /// The kept names of parked runs whose handles were dropped, for the host to find again.
    forgotten: Mutex<Vec<u32>>,
}

impl Control {
    /// Begins a run on the machine, which ends as the [`Run`] drops. It takes the proof that the
    /// heap was prepared for it ([`Prepared`]), so no run begins uncollected, and the right to
    /// park, if it has it, which ending the run takes back.
    pub fn begin(control: &Arc<Control>, _: Prepared, parks: Option<MayPark>) -> Run {
        control
            .word
            .fetch_or(RUNNING | parks.map_or(0, |_| PARKS), Relaxed);
        Run(Arc::clone(control))
    }

    /// Whether a run is in progress that may park: ending it, or parking it, takes the right back.
    pub fn may_park(&self) -> bool {
        self.word.load(Relaxed) & PARKS != 0
    }

    /// The run in progress, or the number of the next: it changes when a run ends.
    pub fn run(&self) -> u64 {
        self.word.load(Relaxed) & !NOT_NUMBER
    }

    /// Whether the run in progress is asked to stop. Read at every step.
    #[inline(always)]
    pub fn asked(&self) -> bool {
        self.word.load(Relaxed) & ASKED != 0
    }

    /// Whether a walk, now `walked` items long, must give up for a stop after `cost` more items:
    /// it asks once every [`WALK_CHECK`] items.
    pub fn charged(&self, walked: &mut usize, cost: usize) -> bool {
        let before = *walked;
        *walked = before.wrapping_add(cost);
        before / WALK_CHECK != *walked / WALK_CHECK && self.asked()
    }

    /// Replaces the word as `change` says, in one atomic step; whether it said to.
    fn update(&self, change: impl Fn(u64) -> Option<u64>) -> bool {
        let mut word = self.word.load(Relaxed);
        while let Some(new) = change(word) {
            match self.word.compare_exchange_weak(word, new, Relaxed, Relaxed) {
                Ok(_) => return true,
                Err(now) => word = now,
            }
        }
        false
    }

    /// Sets `bit`, in one atomic step, if `run` is in progress; whether it was.
    fn ask(&self, bit: u64, run: u64) -> bool {
        self.update(|word| (word & !NOT_NUMBER == run).then_some(word | bit))
    }

    /// The machine acts on the stops asked so far: they count as one.
    #[cold]
    pub fn acted(&self) {
        self.word.fetch_and(!STOP, Relaxed);
    }

    /// Ends the run to park it, unless it was asked to stop.
    pub fn parks(&self) -> bool {
        self.update(|word| (word & ASKED == 0).then(|| after(word)))
    }

    /// Begins a run for host-side work, reading, printing or binding, unless one is in progress, so a stop
    /// reaches it; inside a run it is part of that run, and there is no guard. A stop already asked
    /// of the run it began is `interrupted` before it does anything. It takes no proof of a
    /// prepared heap: host-side work runs no machine, and a collection that is due waits for the
    /// next run on the machine to begin.
    pub fn host_run(control: &Arc<Control>) -> Result<Option<Run>, crate::local::Error> {
        let began = control.update(|word| (word & RUNNING == 0).then_some(word | RUNNING));
        let run = began.then(|| Run(Arc::clone(control)));
        match began && control.asked() {
            true => Err(crate::local::Error::interrupted()),
            false => Ok(run),
        }
    }

    /// The parked runs no handle holds. Nothing panics under the lock.
    pub fn forgotten(&self) -> MutexGuard<'_, Vec<u32>> {
        self.forgotten.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// The run in progress, owned: the only way to begin one is to get this, and the run ends as it
/// drops, however its owner leaves, a panic's unwinding included. Ending a run already over, as a
/// parked one is, keeps the number the next will have.
#[must_use]
pub(crate) struct Run(Arc<Control>);

impl Drop for Run {
    fn drop(&mut self) {
        self.0
            .update(|word| (word & RUNNING != 0).then(|| after(word)));
    }
}

/// Stops the one run of its [`Grift`](crate::Grift) it was taken for, from any thread: the run in
/// progress when [`Grift::interrupter`](crate::Grift::interrupter) made it, or, if none was, the
/// next to begin. Reading and printing the host asks for with no run in progress
/// ([`Grift::read`](crate::Grift::read), [`Grift::read_in`](crate::Grift::read_in),
/// [`Grift::write`](crate::Grift::write), [`Grift::describe`](crate::Grift::describe),
/// [`Grift::bind`](crate::Grift::bind)) are each a
/// run of their own, and reading a text to evaluate is part of the run that evaluates it. That run
/// ends with the error `interrupted` at its next step, or inside a long walk of a tree or a text. That error runs cleanups on its way out as any error does, but a stop can also
/// land in a cleanup that has begun, and cut it, even before its first form. A stop while its run
/// is not in progress does nothing and says so: before it begins, after it ends, while it is
/// parked, and in every other run, so no handle, however old, reaches a run its host didn't mean.
/// Stops waiting together count as one; a later stop of the same run, say of a cleanup that doesn't
/// end, interrupts it again. One that reached a run before it parked stops it there, its cleanups
/// then running, and they may park in turn. Each [`Grift::resume`](crate::Grift::resume) is a run
/// of its own: take a handle for it before resuming. [`Interrupter::force`] ends a run however its cleanups behave.
///
/// ```
/// let mut g: grift::Grift = grift::Grift::new()?;
/// // Taken between runs: it is for the next one.
/// let stopper = g.interrupter();
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
            // The run in progress, else the next to begin.
            run: control.run() | RUNNING,
        }
    }

    /// Stops this handle's run, if it is in progress; whether it was. `true` doesn't say the stop
    /// acted: a run about to end can still return its value.
    pub fn stop(&self) -> bool {
        self.control.ask(STOP, self.run)
    }

    /// Ends this handle's run, if it is in progress, at its next check, however its cleanups behave;
    /// whether it was. No stop need come first. Each cleanup it meets is cut before its first form,
    /// a call made for a host function's [`Answer::Call`](crate::Answer::Call) as any other, and
    /// the run ends with `interrupted`, whatever else fails on the way. `true` doesn't say it
    /// acted, as for a stop. [`Error::limit`] says the ending is real.
    ///
    /// [`Error::limit`]: crate::Error::limit
    pub fn force(&self) -> bool {
        self.control.ask(FORCE, self.run)
    }
}
