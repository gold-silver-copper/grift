//! Stopping a run from another thread. An interpreter and its interrupters share one word: the
//! number of the run in progress or else of the next to begin, whether it is in progress, and two
//! bits a run is asked to stop by. Every handle names one run, the one it was taken for, and
//! everything here is one atomic operation on that word, so nothing can come between looking at
//! the run and asking it: a handle reaches its run while that run is in progress, and never
//! another. Ending a run moves to the next number and clears the rest. The machine reads the word
//! once a step and clears only [`STOP`], so a later stop asks again and a force, which it never
//! clears, is found by every check to the end of its run. No other memory is published through
//! the word, so every ordering is relaxed. Numbers are multiples of [`NEXT`]: never repeated, at a
//! run a nanosecond they wrap after seventy years.

use crate::Value;
use std::collections::{HashMap, TryReserveError};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard};

const STOP: u64 = 1;
const FORCE: u64 = 2;
const ASKED: u64 = STOP | FORCE;
const RUNNING: u64 = 4;
const NEXT: u64 = 8;

/// The word that ends the run `word` is, or is the number of.
fn after(word: u64) -> u64 {
    (word & !(ASKED | RUNNING)).wrapping_add(NEXT)
}

/// What an interpreter shares with its interrupters. Its allocation is also the interpreter's
/// identity: the values it hands out point to it.
#[derive(Default)]
pub(crate) struct Control {
    word: AtomicU64,
    /// What host handles change from any thread, under one lock.
    handles: Mutex<Handles>,
}

/// What host handles change from any thread.
#[derive(Default)]
pub(crate) struct Handles {
    /// The kept names of parked runs whose handles were dropped, and their requests, for the host
    /// to find again.
    pub forgotten: Vec<(u32, Value)>,
    /// How many host [`Symbol`](crate::Symbol)s hold each name, of those handed out: the
    /// collector's roots. A name is here only while a handle holds it.
    pub symbols: HashMap<u32, usize>,
}

impl Control {
    /// What host handles change. Nothing panics under the lock.
    pub fn handles(&self) -> MutexGuard<'_, Handles> {
        self.handles.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// One more host handle of name `id`, its count's room asked for if it is the first.
    pub fn hold(&self, id: u32) -> Result<(), TryReserveError> {
        let symbols = &mut self.handles().symbols;
        if !symbols.contains_key(&id) {
            symbols.try_reserve(1)?;
        }
        let count = symbols.entry(id).or_insert(0);
        *count = count.saturating_add(1);
        Ok(())
    }

    /// A clone of a host handle of name `id`, which is counted already, so this needs no memory.
    /// A count that reaches the most there is stays, the name held for good, rather than wrap.
    pub fn again(&self, id: u32) {
        if let Some(count) = self.handles().symbols.get_mut(&id) {
            *count = count.saturating_add(1);
        }
    }

    /// One host handle of name `id` fewer: with none left, the name is no longer a root.
    pub fn release(&self, id: u32) {
        let symbols = &mut self.handles().symbols;
        match symbols.get_mut(&id) {
            Some(count) if *count == usize::MAX => {}
            Some(count) if *count > 1 => *count -= 1,
            Some(_) => {
                symbols.remove(&id);
            }
            None => {}
        }
    }

    pub fn begin(&self) {
        self.word.fetch_or(RUNNING, Relaxed);
    }

    /// Ends the run in progress, if one is: a run already over keeps the number the next will have.
    pub fn end(&self) {
        self.update(|word| (word & RUNNING != 0).then(|| after(word)));
    }

    /// Whether a run is in progress.
    pub fn running(&self) -> bool {
        self.word.load(Relaxed) & RUNNING != 0
    }

    /// The run in progress, or the number of the next: it changes when a run ends.
    pub fn run(&self) -> u64 {
        self.word.load(Relaxed) & !ASKED
    }

    /// Whether the run in progress is asked to stop. Read at every step.
    #[inline(always)]
    pub fn asked(&self) -> bool {
        self.word.load(Relaxed) & ASKED != 0
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
        self.update(|word| (word & !ASKED == run).then_some(word | bit))
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
/// of its own: take a handle for it before resuming. [`Interrupter::force`] ends a run however its cleanups behave.
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
            // The run in progress, else the next to begin.
            run: control.run() | RUNNING,
        }
    }

    /// Stops this handle's run, if it is in progress; whether it was. `true` doesn't say the stop
    /// acted: a run about to end can still return its value.
    pub fn stop(&mut self) -> bool {
        self.control.ask(STOP, self.run)
    }

    /// Ends this handle's run, if it is in progress, at its next check, however its cleanups behave;
    /// whether it was. No stop need come first. Each cleanup it meets is cut before its first form,
    /// a call made for a host function's [`Answer::Call`](crate::Answer::Call) as any other, and
    /// the run ends with `interrupted`, whatever else fails on the way. `true` doesn't say it
    /// acted, as for a stop. [`Error::limit`] says the ending is real.
    ///
    /// [`Error::limit`]: crate::Error::limit
    pub fn force(&mut self) -> bool {
        self.control.ask(FORCE, self.run)
    }
}
