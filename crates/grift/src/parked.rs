//! Runs that wait. A host function answers "not yet", the run is set aside, and the host's loop goes
//! on until it resumes the run once, with a value or an error.
//!
//! An interpreter has one machine. Parking moves that whole machine into a heap object, which a
//! kept entry holds, so the collector sees a parked run's frames and arguments as it sees any
//! kept value; the interpreter's machine is then empty, for whatever runs next. Taking a run up
//! moves the machine back. The [`Parked`] handle says which: it is linear, so a run goes on at most
//! once by the type, and it carries its interpreter's identity as every value does.

use crate::evaluate::Machine;
use crate::heap::Object;
use crate::local::{self, K};
use crate::{Error, Grift, Owner, Primitive, Value};
use std::sync::Weak;

/// A host function that may answer later: [`Grift::parking_function`] makes one.
pub type ParkingFunction<T> = fn(&mut Grift<T>, &mut T, &[Value]) -> Result<Answer, Error>;

/// What a [`ParkingFunction`] answers.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    /// This value, as any host function answers.
    Now(Value),
    /// Not yet: the run is set aside, and [`Grift::resume`] gives this call its value or error.
    /// The value says what is being waited for, and [`Parked::request`] hands it to the host's
    /// loop: do the work there, once the run has really parked, for it may not. In a run that
    /// can't park ([`Grift::can_park`]) this call fails with `cannot-park`, naming the function; a
    /// stop that reached the run fails it with `interrupted`; and with no memory to park,
    /// `out-of-memory`.
    Later(Value),
}

/// A registered host function, of either kind.
pub(crate) enum Hosted<T> {
    Plain(crate::HostFunction<T>),
    Parking(ParkingFunction<T>),
}

impl<T> Clone for Hosted<T> {
    fn clone(&self) -> Hosted<T> {
        *self
    }
}

impl<T> Copy for Hosted<T> {}

/// How a run that may park came back to its host, when it didn't fail.
#[must_use]
#[derive(Debug)]
pub enum Ran {
    /// It ended with this value.
    Finished(Value),
    /// A host function answered [`Answer::Later`].
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
/// let wait = g.parking_function("wait", |_, _, args| Ok(Answer::Later(args[0].clone())))?;
/// g.define("wait", wait)?;
/// let top = g.top_level();
/// let Ran::Parked(parked) = g.start_in(&mut (), &top, "script", "(add 1 (wait 41))")? else {
///     panic!("it waits");
/// };
/// // The host's loop goes on, and other runs with it.
/// assert_eq!(g.evaluate(&mut (), "(add 2 2)")?, Value::Integer(4));
/// let asked = parked.request().clone();
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
/// # let wait = g.parking_function("wait", |_, _, _| Ok(Answer::Later(Value::Inert)))?;
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
    name: u32,
    request: Value,
}

impl Parked {
    /// What the call asked for in [`Answer::Later`]. The run holds it, so it stays valid for as long
    /// as this handle waits, through any collection.
    pub fn request(&self) -> &Value {
        &self.request
    }
}

impl Drop for Parked {
    fn drop(&mut self) {
        if let Some(control) = self.owner.0.upgrade() {
            // Its park made the room, so this needs no memory and the run can always be found.
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

/// Why [`Grift::resume`] didn't take the run up, or what ended it.
#[derive(Debug)]
pub struct ResumeError {
    /// What went wrong.
    pub error: Error,
    /// The handle, if nothing was done and the run still waits: it was another interpreter's
    /// (`foreign-value`), this was inside a host call (`cannot-park`), or there was no memory to
    /// begin (`out-of-memory`). `None` if the run went on and ended with this error.
    pub parked: Option<Parked>,
}

impl From<ResumeError> for Error {
    /// The error alone, for `?`. A handle that came back with it goes where a dropped handle goes.
    fn from(e: ResumeError) -> Error {
        e.error
    }
}

impl<T> Grift<T> {
    /// As [`Grift::host_function`], for a function that may answer later. To a program it is any
    /// other call.
    pub fn parking_function(&mut self, name: &str, f: ParkingFunction<T>) -> Result<Value, Error> {
        self.hosted(name, Hosted::Parking(f))
    }

    /// [`Grift::call`], in a run that may park. Inside a host call it is a nested run, as `call` is,
    /// and a wait in it is `cannot-park`, since that call's Rust frames can't be set aside: a host
    /// function queues work that waits for its host's loop.
    pub fn start(&mut self, state: &mut T, f: &Value, arguments: &[Value]) -> Result<Ran, Error> {
        self.parkable(|g| {
            let f = g.prepare_call(f, arguments)?;
            let arguments = g.imported(arguments)?;
            g.call_local(Some(state), f, &arguments, true)
        })
    }

    /// [`Grift::evaluate_in`], in a run that may park, as [`Grift::start`] is.
    pub fn start_in(
        &mut self,
        state: &mut T,
        context: &Value,
        source: &str,
        src: &str,
    ) -> Result<Ran, Error> {
        self.parkable(|g| {
            let c = g.context_of(context, Primitive::Evaluate)?;
            g.prepare(std::iter::once(local::Value::Context(c)))?;
            let source = g.intern(source)?;
            let forms = g.read_local(src, Some(source))?;
            g.evaluate_forms(Some(state), forms, c, true)
        })
    }

    fn parkable(
        &mut self,
        go: impl FnOnce(&mut Self) -> Result<local::Value, local::Error>,
    ) -> Result<Ran, Error> {
        let done = go(self);
        self.ran(done).map_err(|e| self.export_error(e))
    }

    /// What a run came to: parked, if it left its kept name, or else what it returned.
    fn ran(&mut self, done: Result<local::Value, local::Error>) -> Result<Ran, local::Error> {
        match self.parked_as.take() {
            Some(name) => Ok(Ran::Parked(self.parked_handle(name))),
            None => done.map(|v| Ran::Finished(self.export(v))),
        }
    }

    /// A handle to the run parked under kept name `name`.
    fn parked_handle(&self, name: u32) -> Parked {
        let request = match self.kept_value(name) {
            Some(local::Value::Pair(fiber)) => match self.heap.get(fiber) {
                Ok(Object::Fiber(room)) => room.first().map(|m| m.request),
                _ => None,
            },
            _ => None,
        };
        Parked {
            owner: self.identity.clone(),
            name,
            request: self.export(request.unwrap_or(local::Value::Inert)),
        }
    }

    /// Takes up a parked run: the waiting call answers `answer` as a host function's return does,
    /// so a stale value or an error is raised there, and the run goes on until it ends or parks
    /// again. Each resume is a run of its own for stops. It makes no heap object to begin; refused
    /// the little it does need, the run waits still and the handle is back in
    /// [`ResumeError::parked`], as that type says.
    ///
    /// To give a run up, answer with a limit ([`Error::as_limit`]): nothing catches it, every
    /// `finally!` runs on the way out, and it is what the run ends with. A cleanup that waits is
    /// a run that waits like any other, which this same answer ends in turn.
    ///
    /// ```
    /// use grift::{Answer, Grift, Ran, ResumeError, Value};
    /// let mut g: Grift = Grift::new()?;
    /// let wait = g.parking_function("wait", |_, _, _| Ok(Answer::Later(Value::Inert)))?;
    /// g.define("wait", wait)?;
    /// let top = g.top_level();
    /// let src = "(define! here (current-context))
    ///            (define! closed #false)
    ///            (catch! (finally! (wait) (bind here (quote! closed) #true))
    ///                    e (bind here (quote! closed) (quote! caught)))";
    /// let Ran::Parked(parked) = g.start_in(&mut (), &top, "script", src)? else {
    ///     panic!("it waits");
    /// };
    /// let ending = g.error("given-up", &[]).as_limit();
    /// let Err(ResumeError { error, parked: None }) = g.resume(&mut (), parked, Err(ending)) else {
    ///     panic!("it ends");
    /// };
    /// assert!(error.is_limit() && g.symbol_name(&error.kind)? == "given-up");
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
        let machine = match self.claim(&parked, &answer) {
            Ok(machine) => machine,
            Err(e) => {
                return Err(ResumeError {
                    error: self.export_error(e),
                    parked: Some(parked),
                });
            }
        };
        parked.owner = Owner(Weak::new()); // Spent: nothing to find again.
        self.may_park = true;
        let done = self.wake(Some(state), machine, answer);
        self.ran(done).map_err(|e| ResumeError {
            error: self.export_error(e),
            parked: None,
        })
    }

    /// The parked run's machine, its fiber let go, and a new run begun, once nothing can refuse.
    fn claim(
        &mut self,
        parked: &Parked,
        answer: &Result<local::Value, local::Error>,
    ) -> Result<Machine, local::Error> {
        if !self.owns(&parked.owner) {
            return Err(local::Error::new(K::ForeignValue, &[]));
        }
        if self.control.run() != 0 {
            return Err(local::Error::new(K::CannotPark, &[]));
        }
        let (value, error, trace) = match answer {
            Ok(v) => (std::slice::from_ref(v), &[][..], &[][..]),
            Err(e) => (&[][..], e.values.as_slice(), e.trace.as_slice()),
        };
        self.prepare(value.iter().chain(error).chain(trace).copied())?;
        self.kept_free
            .try_reserve(1)
            .map_err(|_| local::Error::oom())?;
        let Some(local::Value::Pair(fiber)) = self.kept_value(parked.name) else {
            return Err(local::Error::new(K::StaleValue, &[]));
        };
        self.begin_run();
        let machine = match self.heap.get_mut(fiber)? {
            Object::Fiber(room) => room.pop(),
            _ => None,
        };
        let Some(machine) = machine else {
            self.end_run();
            return Err(local::Error::new(K::StaleValue, &[]));
        };
        self.clear_kept(parked.name);
        Ok(machine)
    }

    /// Whether a [`ParkingFunction`] called now may answer [`Answer::Later`]: not under
    /// [`Grift::evaluate`] or [`Grift::call`], and not in any run inside a host call.
    pub fn can_park(&self) -> bool {
        self.may_park
    }

    /// Room in the queue [`Grift::forgotten`] reads for every run that could wait in it, the one
    /// about to park too: no more runs are parked than there are kept names.
    pub(crate) fn room_to_forget(&self) -> Result<(), local::Error> {
        self.control
            .room_to_forget(self.kept.len() + 1)
            .map_err(|_| local::Error::oom())
    }

    /// How many runs are parked. Each holds what it refers to until it ends, so a host that sees this grow has forgotten some: [`Grift::forgotten`].
    pub fn parked(&self) -> usize {
        let fiber = |v: &&local::Value| match v {
            local::Value::Pair(r) => matches!(self.heap.get(*r), Ok(Object::Fiber(_))),
            _ => false,
        };
        self.kept.iter().flatten().filter(fiber).count()
    }

    /// A new handle for a parked run whose handle was dropped, one at a time: the only way to its
    /// cleanups.
    pub fn forgotten(&mut self) -> Option<Parked> {
        let name = self.control.forgotten().pop()?;
        Some(self.parked_handle(name))
    }
}
