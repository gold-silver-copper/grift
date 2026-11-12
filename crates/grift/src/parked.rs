//! Runs that wait. A host function answers "not yet", the run is set aside, and the host's loop goes
//! on until it resumes the run once, with a value or an error.
//!
//! A run in progress has the interpreter's machine. Parking moves that whole machine into a kept entry, so the
//! collector sees a parked run's frames and arguments as it sees any kept value; the interpreter's
//! machine is then empty, for whatever runs next. Taking a run up moves the machine back. The [`Parked`] handle says which: it is linear, so a run goes on at most
//! once by the type, and it carries its interpreter's identity as every value does.

use crate::evaluate::{First, Machine, Prepared, ending_values};
use crate::interrupt::Control;
use crate::local;
use crate::{Entry, Error, Grift, Owner, Value};
use std::sync::{Arc, Weak};

/// How a run that may park came back to its host, when it didn't fail.
#[must_use]
#[derive(Debug)]
pub enum Ran {
    /// It ended with this value.
    Finished(Value),
    /// A host function answered [`Answer::Later`](crate::Answer::Later).
    Parked(Parked),
}

/// A run set aside. It is neither `Copy` nor `Clone`, and [`Grift::resume`] takes it by value, so a
/// run goes on at most once: a second resume doesn't compile. The run stays alive in the interpreter, holding everything it refers to, until then. A dropped handle is
/// not the end of the run, whose cleanups are still owed: [`Grift::forgotten`] gives a new one,
/// and [`Grift::parked`] counts what is waiting. If the interpreter is dropped first, its parked
/// runs go with it, their cleanups not run, as if the process had died, and handles to them do nothing.
///
/// ```
/// use grift::{Answer, Grift, Ran, Value};
/// let mut g: Grift = Grift::new()?;
/// let wait = g.host_function("wait", |_, _, args| Ok(Answer::Later(args[0])))?;
/// g.define("wait", wait)?;
/// let top = g.top_level();
/// let Ran::Parked(parked) = g.start_in(&mut (), &top, "script", "(add 1 (wait 41))")? else {
///     panic!("it waits");
/// };
/// // The host's loop goes on, and other runs with it.
/// assert_eq!(g.evaluate(&mut (), "(add 2 2)")?, Value::Integer(4));
/// let asked = *parked.request();
/// let ran = g.resume(&mut (), parked, Ok(asked))?;
/// assert!(matches!(ran, Ran::Finished(Value::Integer(42))));
/// # Ok::<(), grift::Error>(())
/// ```
///
/// A second resume doesn't compile:
///
/// ```compile_fail,E0382
/// # use grift::{Answer, Grift, Ran, Value};
/// # let mut g: Grift = Grift::new()?;
/// # let wait = g.host_function("wait", |_, _, _| Ok(Answer::Later(Value::Inert)))?;
/// # g.define("wait", wait)?;
/// # let top = g.top_level();
/// # let Ran::Parked(parked) = g.start_in(&mut (), &top, "script", "(wait)")? else { panic!() };
/// let first = g.resume(&mut (), parked, Ok(Value::Inert));
/// let second = g.resume(&mut (), parked, Ok(Value::Inert));
/// # Ok::<(), grift::Error>(())
/// ```
#[must_use]
#[derive(Debug)]
pub struct Parked {
    owner: Owner,
    /// Where a dropped handle leaves its name; nowhere once spent.
    control: Weak<Control>,
    name: u32,
    request: Value,
}

impl Parked {
    /// What the call asked for in [`Answer::Later`](crate::Answer::Later). The run holds it, so it
    /// stays valid for as long as this handle waits, through any collection.
    pub fn request(&self) -> &Value {
        &self.request
    }
}

impl Drop for Parked {
    fn drop(&mut self) {
        if let Some(control) = self.control.upgrade() {
            // Its park made the room, so this needs no memory and the run can always be found.
            #[expect(clippy::disallowed_methods, reason = "its park reserved this room")]
            control.forgotten().push(self.name);
        }
    }
}

/// A clone would be a second way to take up one run.
///
/// ```compile_fail
/// fn second(parked: &grift::Parked) -> grift::Parked {
///     parked.clone()
/// }
/// ```
#[cfg(doctest)]
struct ParkedIsNotClone;

/// `resume` uses the handle up: by value it is this type, the only reason a second one doesn't
/// compile.
///
/// ```
/// use grift::{Error, Grift, Parked, Ran, ResumeError, Value};
/// let _: fn(&mut Grift, &mut (), Parked, Result<Value, Error>) -> Result<Ran, ResumeError> =
///     Grift::resume;
/// ```
#[cfg(doctest)]
struct HandlesGoInByValue;

/// ```compile_fail
/// use grift::{Error, Grift, Parked, Ran, ResumeError, Value};
/// let _: fn(&mut Grift, &mut (), &Parked, Result<Value, Error>) -> Result<Ran, ResumeError> =
///     Grift::resume;
/// ```
#[cfg(doctest)]
struct ResumeTakesNoReference;

/// The right to park, alone with the one function that gives it, so no other code can make one.
mod right {
    use crate::{Error, Grift, Ran, local};

    /// The right to park, given to a run as it begins.
    pub(crate) struct MayPark(());

    impl<T> Grift<T> {
        /// Runs `run` with the right to park, then reads how it came back: parked, if it left its
        /// kept name, else what it returned.
        pub(super) fn started(
            &mut self,
            run: impl FnOnce(&mut Self, MayPark) -> Result<local::Value, local::Error>,
        ) -> Result<Ran, Error> {
            let done = run(self, MayPark(()));
            match self.parked_as.take() {
                Some(name) => self.out(self.parked_handle(name)).map(Ran::Parked),
                None => self.exported(done).map(Ran::Finished),
            }
        }
    }
}
pub(crate) use right::MayPark;

/// Why [`Grift::resume`] didn't take the run up, or what ended it.
#[derive(Debug)]
pub struct ResumeError {
    /// What went wrong.
    pub error: Error,
    /// The handle, if nothing was done and the run still waits: it was another interpreter's
    /// (`foreign-value`). `None` if the run went on and ended with this error.
    pub parked: Option<Parked>,
}

impl From<ResumeError> for Error {
    /// The error alone, for `?`. A handle that came back with it goes where a dropped handle goes.
    fn from(e: ResumeError) -> Error {
        e.error
    }
}

impl<T> Grift<T> {
    /// [`Grift::call`], in a run that may park.
    pub fn start(&mut self, state: &mut T, f: &Value, arguments: &[Value]) -> Result<Ran, Error> {
        self.started(|g, parks| g.run_call(state, f, arguments, Some(parks)))
    }

    /// [`Grift::evaluate_in`], in a run that may park, as [`Grift::start`] is.
    pub fn start_in(
        &mut self,
        state: &mut T,
        context: &Value,
        source: &str,
        src: &str,
    ) -> Result<Ran, Error> {
        self.started(|g, parks| {
            let c = g.context_of(context)?;
            g.run_text(Some(state), c, Some(source), src, false, Some(parks))
        })
    }

    /// A handle to the run parked under kept name `name`, or `stale-value` if none is.
    fn parked_handle(&self, name: u32) -> Result<Parked, local::Error> {
        let request = match self.kept.get(name as usize) {
            Some(Entry::Run(room)) => room.first().map(|m| m.request),
            _ => None,
        };
        let request = request.ok_or_else(local::Error::stale)?;
        Ok(Parked {
            owner: self.owner,
            control: Arc::downgrade(&self.control),
            name,
            request: self.export(request),
        })
    }

    /// Takes up a parked run: the waiting call answers `answer` as a host function's return does,
    /// so a stale value or an error is raised there, and the run goes on until it ends or parks
    /// again. Each resume is a run of its own for stops. Beginning needs no memory; a refused
    /// resume leaves the run waiting and the handle in [`ResumeError::parked`], as that type says.
    ///
    /// To give a run up, answer with a limit ([`Error::limit`]): nothing catches it, every
    /// `finally!` runs on the way out, and it is what the run ends with. A cleanup that waits is
    /// a run that waits like any other, which this same answer ends in turn.
    ///
    /// ```
    /// use grift::{Answer, Grift, Ran, ResumeError, Value};
    /// let mut g: Grift = Grift::new()?;
    /// let wait = g.host_function("wait", |_, _, _| Ok(Answer::Later(Value::Inert)))?;
    /// g.define("wait", wait)?;
    /// let top = g.top_level();
    /// let src = "(define! here (current-context))
    ///            (define! closed #false)
    ///            (catch! (finally! (wait) (bind here (quote! closed) #true))
    ///                    e (bind here (quote! closed) (quote! caught)))";
    /// let Ran::Parked(parked) = g.start_in(&mut (), &top, "script", src)? else {
    ///     panic!("it waits");
    /// };
    /// let ending = g.limit("given-up", &[]);
    /// let Err(ResumeError { error, parked: None }) = g.resume(&mut (), parked, Err(ending)) else {
    ///     panic!("it ends");
    /// };
    /// assert!(error.limit && g.symbol_name(&error.kind)? == "given-up");
    /// assert_eq!(g.evaluate(&mut (), "closed")?, Value::Boolean(true));
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn resume(
        &mut self,
        state: &mut T,
        mut parked: Parked,
        answer: Result<Value, Error>,
    ) -> Result<Ran, ResumeError> {
        let answer = match &answer {
            Ok(v) => self.import(v),
            Err(e) => Err(self.incoming_error(e)),
        };
        let (machine, ready) = match self.claim(&parked, &answer) {
            Ok(claimed) => claimed,
            Err(e) => {
                return Err(ResumeError {
                    error: self.export_error(e),
                    parked: Some(parked),
                });
            }
        };
        parked.control = Weak::new(); // Spent: nothing to find again.
        let first = First::Resume { machine, answer };
        self.started(|g, parks| g.run_on_machine(ready, Some(parks), Some(state), first))
            .map_err(|error| ResumeError {
                error,
                parked: None,
            })
    }

    /// The parked run's machine, its entry freed, and the heap prepared: nothing after can refuse.
    fn claim(
        &mut self,
        parked: &Parked,
        answer: &Result<local::Value, local::Error>,
    ) -> Result<(Machine, Prepared), local::Error> {
        self.owns(&parked.owner)?;
        let ready = self.prepare(ending_values(answer), []);
        // A handle is linear and of this interpreter, so its name is still that run.
        let Some(Entry::Run(mut room)) = self.clear_kept(parked.name) else {
            return Err(local::Error::stale());
        };
        Ok((room.pop().ok_or_else(local::Error::stale)?, ready))
    }

    /// Whether a host function called now may answer [`Answer::Later`](crate::Answer::Later): not
    /// under [`Grift::evaluate`] or [`Grift::call`], nor with no run in progress.
    pub fn can_park(&self) -> bool {
        self.control.may_park()
    }

    /// Room in the queue [`Grift::forgotten`] reads for every run that could wait in it, the one
    /// about to park too, whose entry is already made: no more runs are parked than there are kept
    /// names.
    pub(crate) fn room_to_forget(&self) -> Result<(), local::Error> {
        let mut forgotten = self.control.forgotten();
        let more = self.kept.len().saturating_sub(forgotten.len());
        Ok(forgotten.try_reserve(more)?)
    }

    /// How many runs are parked. Each holds what it refers to until it ends, so a host that sees this grow has forgotten some: [`Grift::forgotten`].
    pub fn parked(&self) -> usize {
        let run = |e: &&Entry| matches!(e, Entry::Run(_));
        self.kept.iter().filter(run).count()
    }

    /// A new handle for a parked run whose handle was dropped, one at a time: the only way to its
    /// cleanups.
    pub fn forgotten(&mut self) -> Option<Parked> {
        let name = self.control.forgotten().pop()?;
        self.parked_handle(name).ok()
    }
}
