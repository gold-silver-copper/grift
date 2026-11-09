//! The evaluator: an explicit machine loop over a frame stack, and the Rust primitives.
//!
//! The machine's state is its frame stack, its argument stack, the endings waiting for a
//! cleanup to finish, where it is in the program and the current [`Mode`]. The collector runs
//! at the top of the loop and before a host's evaluation or call needs its first slot, with that
//! state, the root contexts and the new call's checked inputs as its roots. Allocation never
//! collects: Rust locals stay valid until the next run entry or machine safe point. The loop
//! checks for a stop just after each step. A run is over when its frames are gone, and a run a
//! host function starts inside a run is a machine of its own, the caller's waiting whole, a root
//! of the collector, until it ends.
//!
//! A run ends one way: an error unwinds it. An error the host or the machine began is a limit,
//! whatever its kind is called, and two things follow: handlers pass it, and when a new error meets
//! one waiting, the rule in [`Grift::unwind`] decides. A stop is such an error. A force is a stop
//! that stays asked, so every cleanup it meets is cut before its first form.
//!
//! Two library forms have a native twin: `if!` and `sequence!` are the library's own operatives,
//! flagged, and run in Rust when their operands have the shape the twin handles; any other shape
//! runs the operative's body, so what a program sees never differs.

use crate::heap::{Bindings, Object, Operative, Origin, Reference, Seen};
use crate::interrupt::Control;
use crate::local::{Error, Host, K, K_NAMES, Symbol, Value, boxed, push};
use crate::read::float_name;
use crate::{Answer, Entry, Grift, collected, context_kind};
use std::sync::Arc;

/// Declares the primitives: each one's variant, its name in the built-in context, and the
/// fewest and most operands it takes. Those `natives.grift` defines exist for speed only, and that
/// definition is their meaning; the others are what grift can't express.
macro_rules! primitives {
    ($($p:ident $name:literal $lo:literal $hi:expr;)*) => {
        /// The Rust primitives, each bound wrapped, as an applicative, until the library says otherwise.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum Primitive { $($p),* }

        impl Primitive {
            pub(crate) const ALL: &[Primitive] = &[$(Primitive::$p),*];

            /// The name the primitive is bound to in the built-in context.
            pub(crate) fn name(self) -> &'static str {
                match self { $(Primitive::$p => $name),* }
            }

            /// The fewest and most operands.
            fn arity(self) -> (usize, usize) {
                match self { $(Primitive::$p => ($lo, $hi)),* }
            }
        }
    };
}

primitives! {
    // Abstraction, wrapping and evaluation: the machine itself.
    Vau "vau!" 2 usize::MAX;
    Wrap "wrap" 1 1;
    Unwrap "unwrap" 1 1;
    Evaluate "evaluate" 2 2;
    // Unwinding: machine frames.
    EvaluateWithCleanup "evaluate-with-cleanup" 3 3;
    EvaluateWithHandler "evaluate-with-handler" 3 3;
    Raise "raise" 1 1;
    // Making, changing and listing bindings.
    Context "context" 1 1;
    Bind "bind" 3 3;
    Unbind "unbind" 2 2;
    Names "names" 1 1;
    // Making a pair; identity and representation.
    Pair "pair" 2 2;
    Same "same?" 2 2;
    TypeOf "type-of" 1 1;
    // Checked integers and IEEE floats, and Rust's float functions (G-42).
    Add "add" 2 2;
    Subtract "subtract" 2 2;
    Multiply "multiply" 2 2;
    Divide "divide" 2 2;
    Remainder "remainder" 2 2;
    LessThan "less-than?" 2 2;
    Power "power" 2 2;
    SquareRoot "square-root" 1 1;
    Sine "sine" 1 1;
    Cosine "cosine" 1 1;
    Tangent "tangent" 1 1;
    Exponential "exponential" 1 1;
    NaturalLogarithm "natural-logarithm" 1 1;
    Floor "floor" 1 1;
    Ceiling "ceiling" 1 1;
    Round "round" 1 1;
    Truncate "truncate" 1 1;
    AbsoluteValue "absolute-value" 1 1;
    // A string's or a bytes value's bytes, joining them, making names and floats from text, the
    // reader and the printer.
    ByteAt "byte-at" 2 2;
    Join "join" 0 usize::MAX;
    StringToSymbol "string-to-symbol" 1 1;
    StringToFloat "string-to-float" 1 1;
    ReadForm "read-form" 2 2;
    ValueToString "value-to-string" 1 1;
    // What natives.grift defines.
    First "first" 1 1;
    Rest "rest" 1 1;
    Select "select" 3 3;
    Lookup "lookup" 3 3;
    BitwiseAnd "bitwise-and" 2 2;
    BitwiseOr "bitwise-or" 2 2;
    BitwiseExclusiveOr "bitwise-exclusive-or" 2 2;
    ShiftLeft "shift-left" 2 2;
    ShiftRight "shift-right" 2 2;
    IntegerToFloat "integer-to-float" 1 1;
    FloatToInteger "float-to-integer" 1 1;
    ByteLength "byte-length" 1 1;
    Slice "slice" 3 3;
    CodePointToString "code-point-to-string" 1 1;
    Search "search" 3 3;
    IntegerToString "integer-to-string" 1 1;
    StringToInteger "string-to-integer" 1 1;
    // A bytes value's contents: the printer writes them and the reader reads them back.
    StringToBytes "string-to-bytes" 1 1;
    BytesToString "bytes-to-string" 1 1;
    ListToBytes "list-to-bytes" 1 1;
    BytesToList "bytes-to-list" 1 1;
}

/// The library forms with a native twin.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Native {
    If,
    Sequence,
}

/// What the machine does next.
#[derive(Clone, Copy)]
pub(crate) enum Mode {
    /// Evaluate an expression in a context.
    Evaluate(Value, Reference),
    /// Deliver a value to the top frame.
    Return(Value),
    /// Call a host function with the arguments on the argument stack from `base`.
    Host(Host, usize),
}

/// A continuation frame: what to do with the value being returned, the context it happens in, and
/// the combination of the program it belongs to ([`Reference::NONE`] for none), where an error is
/// reported.
#[derive(Clone, Copy)]
struct Frame {
    what: What,
    context: Reference,
    at: Reference,
}

// The machine loop's data shape, checked as it compiles. Every step pushes or pops a frame and
// copies a mode, and a frame written field by field is read back whole: a bigger frame costs every
// program, and a field narrower than a word beside another narrow one makes the read wait for both
// writes to land (a store-forwarding stall: +21% on every program once, when `Base` was a `u32`
// and a `bool`). These catch a change of size or field width only. Code layout, inlining, branch
// shape and what the optimizer does with the loop they can't see: a speed run is the check for
// those.
const _: () = {
    // A frame: a `What`, its context and its combination, no padding.
    assert!(size_of::<Frame>() == 56 && align_of::<Frame>() == 8);
    assert!(size_of::<What>() == 40 && align_of::<What>() == 8);
    // Every field of every `What` is whole words: written whole, read back whole. A new variant or
    // field doesn't compile until `words` names it; a new variant then goes in the list below.
    const fn whole<T>(_: &T) -> bool {
        size_of::<T>().is_multiple_of(size_of::<usize>())
    }
    const fn words(what: &What) -> bool {
        match what {
            What::Forms { rest } | What::Body { rest } => whole(rest),
            What::Combine { operands, base } => whole(operands) && whole(base),
            What::Arguments {
                combiner,
                rest,
                base,
            } => whole(combiner) && whole(rest) && whole(base),
            What::Cleanup { cleanup: v, base } | What::Handler { handler: v, base } => {
                whole(v) && whole(base)
            }
            What::Branch {
                consequent,
                alternative,
            } => whole(consequent) && whole(alternative),
            What::Finish | What::Form => true,
        }
    }
    let (v, base) = (Value::Inert, Base(0));
    assert!(words(&What::Forms { rest: v }) && words(&What::Body { rest: v }));
    assert!(words(&What::Combine { operands: v, base }));
    assert!(words(&What::Arguments {
        combiner: v,
        rest: v,
        base,
    }));
    assert!(words(&What::Cleanup {
        cleanup: v,
        base: 0
    }));
    assert!(words(&What::Handler {
        handler: v,
        base: 0
    }));
    assert!(words(&What::Branch {
        consequent: v,
        alternative: v,
    }));
    // A mode is a value and a context, its tag in the value's spare tag values.
    assert!(size_of::<Mode>() == 24);
};

#[derive(Clone, Copy)]
enum What {
    /// Evaluate the remaining forms of one `evaluate` call; with none left, pass the value on.
    Forms { rest: Value },
    /// The value is the combiner of a combination with these operands; `base` says whether the
    /// library made the combination.
    Combine { operands: Value, base: Base },
    /// The value is an argument of `combiner`; arguments so far are on the argument stack from
    /// `base`'s height, and `base` says whether the library made the combination.
    Arguments {
        combiner: Value,
        rest: Value,
        base: Base,
    },
    /// When the evaluation above ends, by a value or an error, call the combiner `cleanup` with
    /// no arguments; `base` is the argument stack's height to go back to.
    Cleanup { cleanup: Value, base: usize },
    /// When an error leaves the evaluation above, call `handler` with it as a value instead;
    /// `base` is the argument stack's height to go back to. A value passes through.
    Handler { handler: Value, base: usize },
    /// The cleanup above has ended: the ending it waited on, the last one saved, goes on.
    Finish,
    /// `if!`: the value is the test; evaluate one of these as a tail call.
    Branch {
        consequent: Value,
        alternative: Value,
    },
    /// `sequence!`: the forms left after the one evaluated, at least one.
    Body { rest: Value },
    /// A named outer form evaluated by grift: keep it as the boundary through its tail calls.
    Form,
}

/// An argument stack's height and whether the library made the combination, in one word. Written
/// whole, the frame reads back without waiting on two narrow writes, and stays 56 bytes.
#[derive(Clone, Copy)]
struct Base(usize);

impl Base {
    /// The top bit: a `Vec`'s length never reaches it.
    const LIBRARY: usize = 1 << (usize::BITS - 1);

    fn new(height: usize, library: bool) -> Base {
        Base(height | if library { Base::LIBRARY } else { 0 })
    }

    fn height(self) -> usize {
        self.0 & !Base::LIBRARY
    }

    fn library(self) -> bool {
        self.0 & Base::LIBRARY != 0
    }
}

impl Frame {
    fn values(self) -> [Value; 4] {
        let empty = Value::EmptyList;
        let (a, b) = match self.what {
            What::Forms { rest: v }
            | What::Combine { operands: v, .. }
            | What::Cleanup { cleanup: v, .. }
            | What::Handler { handler: v, .. }
            | What::Body { rest: v } => (v, empty),
            What::Arguments { combiner, rest, .. } => (combiner, rest),
            What::Branch {
                consequent,
                alternative,
            } => (consequent, alternative),
            What::Finish | What::Form => (empty, empty),
        };
        [a, b, Value::Context(self.context), Value::Pair(self.at)]
    }
}

/// A run's state, all of it: a run that parks moves this into the kept table, and what belongs to a
/// run but lives anywhere else stays behind when it does. An interpreter has one current, empty between runs.
pub(crate) struct Machine {
    frames: Vec<Frame>,
    arguments: Vec<Value>,
    /// One per `Cleanup` frame, its room to save the ending it meets, and one per `Finish` frame,
    /// that ending. Holding the room from the start, saving an ending can't fail.
    waiting: Vec<Result<Value, Error>>,
    /// The innermost combination of the program being evaluated, or [`Reference::NONE`]. A
    /// `Reference`, not a `Value`: this is written and read at every step.
    here: Reference,
    /// Whether the combination being combined is the library's: the pairs made now are too, so
    /// the code the library builds is the library's, and the code a program builds the program's.
    pub(crate) library: bool,
    /// Scratch for matching parameter trees.
    work: Vec<(Value, Value)>,
    /// What a parked run asked its host for, held for as long as it waits.
    pub request: Value,
    /// Whether this run may park: only one the host began to, with [`Grift::start`] or the like.
    pub(crate) may_park: bool,
    /// The trace the last handler got, and its combinations: raising that trace again takes them
    /// back, so a catch and raise doesn't walk the trace. Held until replaced or the run ends.
    handed: Option<(Value, Seen)>,
    /// Steps taken, for tests of what a form costs.
    #[cfg(test)]
    pub steps: u64,
}

/// What an ending holds for the collector: its value, or its error's values and trace.
pub(crate) fn ending_values(ending: &Result<Value, Error>) -> impl Iterator<Item = Value> {
    let (held, values, trace) = match ending {
        Ok(v) => ([*v, Value::Inert], &[][..], &[][..]),
        Err(e) => (
            [Value::Symbol(e.kind), e.shared],
            e.values.as_slice(),
            e.trace.as_slice(),
        ),
    };
    held.into_iter().chain(values.iter().chain(trace).copied())
}

impl Machine {
    /// What this machine holds for the collector, running or parked.
    pub(crate) fn roots(&self) -> impl Iterator<Item = Value> {
        let waiting = self.waiting.iter().flat_map(ending_values);
        self.frames
            .iter()
            .flat_map(|f| f.values())
            .chain(self.arguments.iter().copied())
            .chain(waiting)
            .chain([Value::Pair(self.here), self.request])
    }

    /// Empty again, keeping its room: a run ended without coming back here is over.
    pub(crate) fn clear(&mut self) {
        self.frames.clear();
        self.arguments.clear();
        self.waiting.clear();
        self.here = Reference::NONE;
        self.library = false;
        self.request = Value::Inert;
        self.may_park = false;
        self.handed = None;
    }

    /// How many objects a machine counts as, for when to collect: every collection walks it,
    /// running or parked.
    pub(crate) fn weight(&self) -> usize {
        let waiting = self.waiting.iter().map(|w| match w {
            Ok(_) => 1,
            Err(e) => 1 + e.values.len() + e.trace.len(),
        });
        1 + self.frames.len() + self.arguments.len() + waiting.sum::<usize>()
    }
}

#[cfg(test)]
impl Machine {
    /// The most frames there were at once.
    pub fn frames_high(&self) -> usize {
        self.frames.capacity()
    }

    /// Only this much room above the frames, as if no more could be had.
    pub fn leave_room(&mut self, room: usize) {
        self.frames.shrink_to(self.frames.len() + room);
    }
}

impl Default for Machine {
    fn default() -> Machine {
        Machine {
            frames: Vec::new(),
            arguments: Vec::new(),
            waiting: Vec::new(),
            here: Reference::NONE,
            library: false,
            work: Vec::new(),
            request: Value::Inert,
            may_park: false,
            handed: None,
            #[cfg(test)]
            steps: 0,
        }
    }
}

/// Held while a host function runs: if it doesn't return to the interpreter it was called from,
/// by a panic or by replacing it, that interpreter's run ends, and its next one clears its machine.
struct Hosting(Option<Arc<Control>>);

impl Drop for Hosting {
    fn drop(&mut self) {
        if let Some(c) = &self.0 {
            c.end();
        }
    }
}

pub(crate) fn type_error(v: Value) -> Error {
    Error::new(K::TypeError, &[v])
}

impl<T> Grift<T> {
    /// Evaluates the list `forms` in `context`, in order.
    pub(crate) fn evaluate_forms(
        &mut self,
        state: Option<&mut T>,
        forms: Value,
        context: Reference,
        parks: bool,
    ) -> Result<Value, Error> {
        let top = What::Forms { rest: forms };
        self.enter(state, top, context, Mode::Return(Value::Inert), parks)
    }

    /// [`Grift::call`], on this interpreter's values.
    pub(crate) fn call_local(
        &mut self,
        state: Option<&mut T>,
        f: Value,
        arguments: &[Value],
        parks: bool,
    ) -> Result<Value, Error> {
        let f = self.combiner_of(f)?;
        self.settled();
        let operands = self.list_local(arguments)?;
        let context = self.heap.context(None)?;
        // The run's own machine, whose argument stack starts empty.
        let call = What::Combine {
            operands,
            base: Base::new(0, false),
        };
        self.enter(state, call, context, Mode::Return(f), parks)
    }

    /// An applicative's underlying combiner; anything else is apply's `type-error`.
    fn combiner_of(&self, f: Value) -> Result<Value, Error> {
        match f {
            Value::Applicative(r) => self.unwrapped(r),
            _ => Err(type_error(f)),
        }
    }

    /// Runs the interpreter's machine from `mode` with one frame until its frames are gone, as a
    /// run of its own, for stops.
    fn enter(
        &mut self,
        state: Option<&mut T>,
        what: What,
        context: Reference,
        mode: Mode,
        parks: bool,
    ) -> Result<Value, Error> {
        self.begin_run();
        self.machine.may_park = parks;
        let result = self
            .frame(what, context, Reference::NONE)
            .and_then(|()| self.run(state, mode));
        // A machine that parked or was given up has nothing to go back to.
        self.machine.clear();
        self.control.end();
        result
    }

    /// Takes up parked `machine` with the host's answer, until it ends or parks again. Nothing is
    /// around it: the machine is its own, as it was when it began.
    pub(crate) fn wake(
        &mut self,
        state: Option<&mut T>,
        machine: Machine,
        answer: Result<Value, Error>,
    ) -> Result<Value, Error> {
        self.machine = machine;
        self.machine.request = Value::Inert;
        let mode = answer.map(Mode::Return).or_else(|e| self.unwind(e));
        let result = mode.and_then(|mode| self.run(state, mode));
        self.machine = Machine::default();
        self.control.end();
        result
    }

    /// Sets the run in progress aside: its whole machine moves into a kept entry, and this
    /// interpreter's machine is empty again. Everything that can fail comes first, so a run that
    /// can't park goes on where it was, with the error, and that it was asked to stop is the last
    /// of those; filling the entry comes after them all, so there is nothing to undo. Its kept
    /// name is left for [`Grift::ran`].
    fn park(&mut self, request: Value) -> Result<(), Error> {
        if !self.machine.may_park {
            return Err(Error::new(K::CannotPark, &[]));
        }
        let mut room = Vec::new();
        room.try_reserve_exact(1).map_err(|_| Error::oom())?;
        let name = self.free_name()?;
        self.room_to_forget()?;
        let asked = self.export(request)?;
        if !self.control.parks() {
            return Err(Error::interrupted());
        }
        self.machine.request = request;
        room.push(std::mem::take(&mut self.machine));
        self.fill_kept(name, Entry::Run(room));
        self.parked_as = Some((name, asked));
        Ok(())
    }

    fn frame(&mut self, what: What, context: Reference, at: Reference) -> Result<(), Error> {
        push(&mut self.machine.frames, Frame { what, context, at })
    }

    fn run(&mut self, mut state: Option<&mut T>, mut mode: Mode) -> Result<Value, Error> {
        loop {
            #[cfg(test)]
            {
                self.machine.steps += 1;
            }
            if let Mode::Return(v) = mode
                && self.machine.frames.is_empty()
            {
                return Ok(v);
            }
            let next = self.safe_point(mode).and_then(|()| match mode {
                Mode::Host(h, base) => self.host(state.as_deref_mut(), h, base),
                _ => self.step(mode),
            });
            // A stop that came during the step, however long, is how the step ended, whatever it
            // would have answered: the cleanups then run.
            mode = match next {
                _ if self.control.asked() => {
                    self.control.acted();
                    self.unwind(Error::interrupted())?
                }
                Ok(mode) => mode,
                Err(e) => self.unwind(e)?,
            };
        }
    }

    /// The safe point: the machine's state is all there is, so the collector may run.
    #[inline]
    fn safe_point(&mut self, mode: Mode) -> Result<(), Error> {
        if self.heap.due() {
            let current = match mode {
                Mode::Evaluate(x, context) => [x, Value::Context(context)],
                Mode::Return(v) => [v, Value::EmptyList],
                Mode::Host(..) => [Value::EmptyList, Value::EmptyList],
            };
            self.collect(current.into_iter());
        }
        Ok(())
    }

    #[inline(always)]
    fn step(&mut self, mode: Mode) -> Result<Mode, Error> {
        Ok(match mode {
            Mode::Evaluate(Value::Symbol(s), context) => Mode::Return(self.value_of(context, s)?),
            Mode::Evaluate(Value::Pair(r), context) => {
                let Object::Pair(combiner, operands, origin) = *self.heap.get(r)? else {
                    return Err(Error::new(K::StaleValue, &[Value::Pair(r)]));
                };
                // A combination is the program's unless the library made it.
                if !origin.library {
                    self.machine.here = r;
                    if origin.outer {
                        self.hold_form(r)?;
                    }
                }
                let combine = What::Combine {
                    operands,
                    base: Base::new(self.machine.arguments.len(), origin.library),
                };
                self.frame(combine, context, self.machine.here)?;
                Mode::Evaluate(combiner, context)
            }
            Mode::Evaluate(x, _) => Mode::Return(x),
            Mode::Return(v) => {
                let Some(Frame { what, context, at }) = self.machine.frames.pop() else {
                    return Ok(Mode::Return(v));
                };
                match what {
                    What::Forms {
                        rest: Value::Pair(r),
                    } => {
                        let (form, rest) = self.pair_at(r)?;
                        let at = match form {
                            Value::Pair(f) if !self.origin_at(f)?.library => f,
                            _ => Reference::NONE,
                        };
                        self.frame(What::Forms { rest }, context, at)?;
                        self.machine.here = Reference::NONE;
                        Mode::Evaluate(form, context)
                    }
                    What::Forms { .. } | What::Form => Mode::Return(v),
                    What::Combine { operands, base } => {
                        self.machine.here = at;
                        self.machine.library = base.library();
                        self.combine(v, operands, context)?
                    }
                    What::Arguments {
                        combiner,
                        rest,
                        base,
                    } => {
                        self.machine.here = at;
                        push(&mut self.machine.arguments, v)?;
                        self.next_argument(combiner, context, rest, base)?
                    }
                    What::Cleanup { cleanup, .. } => {
                        self.machine.here = at;
                        self.start_cleanup(Ok(v), cleanup, context, at)?
                    }
                    What::Handler { .. } => {
                        self.machine.here = at;
                        Mode::Return(v)
                    }
                    What::Branch {
                        consequent,
                        alternative,
                    } => {
                        self.machine.here = at;
                        match v {
                            Value::Boolean(true) => Mode::Evaluate(consequent, context),
                            Value::Boolean(false) => Mode::Evaluate(alternative, context),
                            _ => return Err(type_error(v)),
                        }
                    }
                    What::Body { rest } => {
                        self.machine.here = at;
                        self.next_form(rest, context, at)?
                    }
                    // A cleanup's install gave its `Finish` this entry, so the fallback never acts.
                    What::Finish => match self.machine.waiting.pop().unwrap_or(Ok(v)) {
                        Ok(v) => Mode::Return(v),
                        Err(e) => {
                            // It already says where it happened, or that it can't: not here.
                            self.machine.here = Reference::NONE;
                            return Err(e);
                        }
                    },
                }
            }
            Mode::Host(..) => return Err(Error::new(K::NotACombiner, &[])),
        })
    }

    /// All routes into a named outer form share this boundary, including native control flow.
    /// A tail form replaces the finished one; the host's own forms frame stays as it was.
    fn hold_form(&mut self, form: Reference) -> Result<(), Error> {
        match self.machine.frames.last_mut() {
            Some(Frame {
                what: What::Form,
                at,
                ..
            }) => *at = form,
            Some(Frame {
                what: What::Forms { .. },
                at,
                ..
            }) if *at == form => {}
            _ => self.frame(What::Form, Reference::NONE, form)?,
        }
        Ok(())
    }

    /// An error stops the evaluation at `here`. Frames are dropped down to the innermost cleanup
    /// or handler, adding where each was to the error's trace. A cleanup then
    /// runs, the error waiting until it is done; a handler, unless the error is a limit, is called
    /// with it instead. What can't be called fails there like any call, with that error, and the
    /// next frame goes on from it. With neither left, the error goes to the caller.
    /// An error that began as refused memory: the next safe point collects, and the spare room is
    /// given up for what reports it. Once: whoever acts on it clears the mark.
    fn acting_on_refusal(&mut self, e: &mut Error) {
        if std::mem::take(&mut e.refused) {
            self.heap.request_collection();
            self.spare = Vec::new();
        }
    }

    pub(crate) fn unwind(&mut self, mut e: Error) -> Result<Mode, Error> {
        // The garbage is what the cleanups will run on, and the host's next run: the next safe
        // point collects, with the frames dropped here let go of. Once: what is left to free comes
        // a level at a time, and a collection walks all the levels left. A cleanup refused memory
        // is a new refusal, and asks again.
        self.acting_on_refusal(&mut e);
        locate(&mut e, self.machine.here);
        while let Some(Frame { what, context, at }) = self.machine.frames.pop() {
            match what {
                What::Cleanup { cleanup, base } => {
                    locate(&mut e, at);
                    self.machine.arguments.truncate(base);
                    self.machine.here = at;
                    return self.start_cleanup(Err(e), cleanup, context, at);
                }
                What::Handler { handler, base } if !e.limit => {
                    self.machine.arguments.truncate(base);
                    self.machine.here = at;
                    match self.handle(e, handler, context) {
                        Ok(mode) => return Ok(mode),
                        Err(mut failed) => {
                            self.acting_on_refusal(&mut failed);
                            e = failed;
                            locate(&mut e, at);
                        }
                    }
                }
                What::Finish => {
                    // The one rule: the newest reason wins, unless a program's would replace a
                    // limit.
                    if let Some(Err(waiting)) = self.machine.waiting.pop()
                        && waiting.limit
                        && !e.limit
                    {
                        e = waiting;
                    }
                }
                _ => locate(&mut e, at),
            }
        }
        Err(e)
    }

    /// A popped `Cleanup` frame becomes a `Finish` frame and a call of the cleanup, and its room in
    /// `waiting` this ending. Its install made all that room, so a cleanup's start doesn't fail: the
    /// next check comes first, and a refusal after it is the cleanup's first step failing. The
    /// call's frame goes on the machine's usual way, a plain `push` here costing every step 25%.
    fn start_cleanup(
        &mut self,
        ending: Result<Value, Error>,
        cleanup: Value,
        context: Reference,
        at: Reference,
    ) -> Result<Mode, Error> {
        let m = &mut self.machine;
        m.waiting.pop();
        m.waiting.push(ending);
        #[cfg(test)]
        assert!(
            m.frames.len() + 2 <= m.frames.capacity(),
            "its install made the room"
        );
        m.frames.push(Frame {
            what: What::Finish,
            context: Reference::NONE,
            at: Reference::NONE,
        });
        let call = What::Combine {
            operands: Value::EmptyList,
            base: Base::new(self.machine.arguments.len(), false),
        };
        self.frame(call, context, at)?;
        Ok(Mode::Return(cleanup))
    }

    /// Calls a handler with the error as a new context, with no parent, binding `kind`,
    /// `values` and `trace`. The trace is the error's shared list with the places added since on
    /// its front, so this costs those places only.
    fn handle(&mut self, e: Error, handler: Value, context: Reference) -> Result<Mode, Error> {
        let values = self.list_local(&e.values)?;
        let trace = (e.trace.iter()).try_fold(e.shared, |l, &x| self.pair_local(x, l))?;
        let mut fields = Bindings::default();
        fields.try_reserve(3).map_err(|_| Error::oom())?;
        fields.insert(K::Kind.into(), Value::Symbol(e.kind));
        fields.insert(K::Values.into(), values);
        fields.insert(K::Trace.into(), trace);
        let v = Value::Context(self.heap.alloc(Object::Context(None, fields))?);
        let operands = self.list_local(&[v])?;
        self.machine.handed = Some((trace, e.seen));
        self.combine(handler, operands, context)
    }

    /// Every way into a run comes here first. One inside a host call is refused, `cannot-nest`: it
    /// would stack the program's depth on the host function's Rust frame, where a host function
    /// answers [`Answer::Call`] instead. Then, as the run may need slots before reaching the
    /// machine loop, collect first if due, with its checked inputs as roots.
    pub(crate) fn prepare(&mut self, current: impl Iterator<Item = Value>) -> Result<(), Error> {
        if self.control.running() {
            return Err(Error::new(K::CannotNest, &[]));
        }
        if self.heap.due() {
            self.collect(current);
        }
        Ok(())
    }

    // Root iterators and marking temporaries belong only to collection.
    #[inline(never)]
    fn collect(&mut self, current: impl Iterator<Item = Value>) {
        // What the collection walks outside the heap: this machine, and the whole kept table.
        let parked: usize = self.kept.iter().map(Entry::weight).sum();
        let walked = parked + self.kept.len() + self.machine.weight();
        let kept = self.kept.iter().flat_map(Entry::roots);
        let roots = self
            .machine
            .roots()
            .chain(current)
            .chain(kept)
            .chain([Value::Context(self.built_in), Value::Context(self.top)]);
        // Names live as heap objects: the known ones, host functions' and those a host holds are
        // roots too, and the spelling index, which the collection walks, keeps only live ones.
        let handles = self.control.handles();
        let held = &handles.symbols;
        let known = (0..).zip(K_NAMES).map(|(id, _)| Symbol(id));
        let hosts = self.hosts.iter().map(|&(name, _)| name);
        let names = known.chain(held.keys().map(|&id| Symbol(id))).chain(hosts);
        let walked = walked + self.symbols.capacity() + held.len() + self.hosts.len();
        self.heap
            .collect(roots.chain(names.map(Value::Symbol)), walked);
        drop(handles);
        self.tidy_kept();
        self.tidy_symbols();
    }

    /// Calls host function `h` with the arguments on the argument stack from `base`, popping
    /// them. Only the library runs with no state, and it calls no host function.
    fn host(&mut self, state: Option<&mut T>, h: Host, base: usize) -> Result<Mode, Error> {
        let f = self.hosts.get(h.0 as usize).map(|&(_, f)| f);
        let (Some(f), Some(state)) = (f, state) else {
            return Err(Error::new(K::NotACombiner, &[Value::Host(h)]));
        };
        // The arguments stay on the argument stack, rooted, while the host runs.
        let given = self.machine.arguments.get(base..).unwrap_or_default();
        let arguments = self.export_all(given)?;
        let mut hosting = Hosting(Some(Arc::clone(&self.control)));
        let v = f(self, state, &arguments);
        // Replaced: the run is over, and what is on this machine is no part of it. The replaced
        // interpreter's run ends as `hosting` drops. Nothing else ends a run in a host call, which
        // begins none.
        let same = hosting
            .0
            .as_ref()
            .is_some_and(|c| Arc::ptr_eq(c, &self.control));
        if !same {
            self.control.end();
            self.settled();
            return Err(Error::new(K::ForeignValue, &[]));
        }
        hosting.0 = None;
        self.machine.arguments.truncate(base);
        self.answered(v)
    }

    /// What a host function answered, checked as everything the host hands in is. Apart from
    /// [`Grift::host`], so that its temporaries stay off the machine loop's stack.
    #[inline(never)]
    fn answered(&mut self, v: Result<Answer, crate::Error>) -> Result<Mode, Error> {
        match v {
            Ok(Answer::Now(v)) => Ok(Mode::Return(self.import(&v)?)),
            // Parked, the machine is empty, so this value ends the loop.
            Ok(Answer::Later(request)) => {
                let request = self.import(&request)?;
                self.park(request).map(|()| Mode::Return(Value::Inert))
            }
            Ok(Answer::Call {
                function,
                arguments,
                then,
            }) => {
                let function = self.import(&function)?;
                let arguments = self.imported(&arguments)?;
                let then = match then {
                    Some((f, saved)) => Some((self.import(&f)?, self.imported(&saved)?)),
                    None => None,
                };
                self.call_on_machine(function, &arguments, then)
            }
            Err(e) => Err(self.incoming_error(&e)),
        }
    }

    /// [`Answer::Call`]: the frames of a call of `function`, as [`Grift::call_local`] builds them,
    /// and of `then`, each in a new empty context of its own.
    /// The frame of an argument still to come is all a `then` needs: the saved values are the
    /// arguments so far and the call's value is the last, so the machine has no new kind of frame.
    fn call_on_machine(
        &mut self,
        function: Value,
        arguments: &[Value],
        then: Option<(Value, Vec<Value>)>,
    ) -> Result<Mode, Error> {
        let function = self.combiner_of(function)?;
        let operands = self.list_local(arguments)?;
        let at = self.machine.here;
        if let Some((then, saved)) = then {
            let combiner = self.combiner_of(then)?;
            let context = self.heap.context(None)?;
            let base = Base::new(self.machine.arguments.len(), false);
            for v in saved {
                push(&mut self.machine.arguments, v)?;
            }
            let rest = Value::EmptyList;
            self.frame(
                What::Arguments {
                    combiner,
                    rest,
                    base,
                },
                context,
                at,
            )?;
        }
        let context = self.heap.context(None)?;
        let base = Base::new(self.machine.arguments.len(), false);
        self.frame(What::Combine { operands, base }, context, at)?;
        Ok(Mode::Return(function))
    }

    /// Combines a combiner with unevaluated operands.
    fn combine(&mut self, f: Value, operands: Value, context: Reference) -> Result<Mode, Error> {
        match f {
            Value::Applicative(r) => {
                let mut inner = self.unwrapped(r)?;
                // With no operands, every layer of wrapping evaluates nothing: peel them here
                // rather than recursing once per layer.
                while let (Value::Applicative(r), Value::EmptyList) = (inner, operands) {
                    inner = self.unwrapped(r)?;
                }
                let base = Base::new(self.machine.arguments.len(), self.machine.library);
                self.next_argument(inner, context, operands, base)
            }
            Value::Operative(r) => self.call_compound(r, operands, context),
            Value::Primitive(_) | Value::Host(_) => self.spread(f, operands, context),
            _ => Err(Error::new(K::NotACombiner, &[f])),
        }
    }

    /// Applies a primitive or a host function to its operands as they are.
    fn spread(&mut self, f: Value, operands: Value, context: Reference) -> Result<Mode, Error> {
        let base = self.machine.arguments.len();
        let mut rest = operands;
        while let Value::Pair(r) = rest {
            let (x, next) = self.pair_at(r)?;
            push(&mut self.machine.arguments, x)?;
            rest = next;
        }
        if rest != Value::EmptyList {
            return Err(Error::new(K::TypeError, &[f, operands]));
        }
        self.apply(f, context, base)
    }

    /// Evaluates the next operand of an applicative, or applies its combiner once none is left.
    fn next_argument(
        &mut self,
        combiner: Value,
        context: Reference,
        rest: Value,
        base: Base,
    ) -> Result<Mode, Error> {
        match rest {
            Value::Pair(r) => {
                let (x, rest) = self.pair_at(r)?;
                let arguments = What::Arguments {
                    combiner,
                    rest,
                    base,
                };
                self.frame(arguments, context, self.machine.here)?;
                Ok(Mode::Evaluate(x, context))
            }
            Value::EmptyList => {
                self.machine.library = base.library();
                self.apply(combiner, context, base.height())
            }
            _ => Err(type_error(rest)),
        }
    }

    /// Applies a combiner to the arguments on the argument stack from `base`, popping them.
    fn apply(&mut self, combiner: Value, context: Reference, base: usize) -> Result<Mode, Error> {
        match combiner {
            Value::Primitive(p) => {
                let mode = self.primitive(p, context, base);
                self.machine.arguments.truncate(base);
                mode
            }
            Value::Host(h) => Ok(Mode::Host(h, base)),
            _ => {
                let list = self.arguments_list(base)?;
                self.combine(combiner, list, context)
            }
        }
    }

    /// The arguments on the argument stack from `base` as a new list, popping them.
    fn arguments_list(&mut self, base: usize) -> Result<Value, Error> {
        let mut list = Value::EmptyList;
        while self.machine.arguments.len() > base {
            let x = self.machine.arguments.pop().unwrap_or(Value::EmptyList);
            list = self.pair_local(x, list)?;
        }
        Ok(list)
    }

    /// Calls a compound operative: a child of the context it was made in, with the parameters
    /// bound to the operands and the context parameter to the caller's context.
    fn call_compound(
        &mut self,
        r: Reference,
        operands: Value,
        context: Reference,
    ) -> Result<Mode, Error> {
        let op = match self.heap.get(r)? {
            Object::Operative(op) => *op,
            _ => return Err(Error::new(K::StaleValue, &[Value::Operative(r)])),
        };
        if let Some(native) = op.native
            && let Some(mode) = self.native(native, operands, context)?
        {
            return Ok(mode);
        }
        // The bindings are made first, with room for all of them, then the context.
        let mut bindings = Bindings::default();
        bindings
            .try_reserve(op.size as usize)
            .map_err(|_| Error::oom())?;
        self.match_tree(&mut bindings, op.parameters, operands)?;
        if let Value::Symbol(s) = op.context_parameter {
            bindings.insert(s, Value::Context(context));
        }
        let local = self
            .heap
            .alloc(Object::Context(Some(op.context), bindings))?;
        self.next_form(op.body, local, self.machine.here)
    }

    /// Runs a native twin on `operands`, as the library's operative would with them; `None` for
    /// a shape the twin leaves to the operative's body.
    fn native(
        &mut self,
        native: Native,
        operands: Value,
        context: Reference,
    ) -> Result<Option<Mode>, Error> {
        let at = self.machine.here;
        let mode = match native {
            Native::If => {
                let Some([test, consequent, alternative]) = self.three(operands)? else {
                    return Ok(None);
                };
                let branch = What::Branch {
                    consequent,
                    alternative,
                };
                self.frame(branch, context, at)?;
                Mode::Evaluate(test, context)
            }
            Native::Sequence => {
                let mut rest = operands;
                while let Value::Pair(r) = rest {
                    rest = self.pair_at(r)?.1;
                }
                if rest != Value::EmptyList {
                    return Ok(None);
                }
                self.next_form(operands, context, at)?
            }
        };
        Ok(Some(mode))
    }

    #[inline(always)]
    fn three(&self, list: Value) -> Result<Option<[Value; 3]>, Error> {
        let mut items = [Value::EmptyList; 3];
        let mut rest = list;
        for item in &mut items {
            let Value::Pair(r) = rest else {
                return Ok(None);
            };
            (*item, rest) = self.pair_at(r)?;
        }
        Ok((rest == Value::EmptyList).then_some(items))
    }

    /// Flags the library's operatives that have a native twin.
    pub(crate) fn install_natives(&mut self) -> Result<(), Error> {
        for (name, native) in [("if!", Native::If), ("sequence!", Native::Sequence)] {
            let found = self
                .intern(name)
                .and_then(|s| self.find(self.built_in, s))?;
            let Some(Value::Operative(r)) = found else {
                return Err(Error::new(K::TypeError, &[found.unwrap_or(Value::Inert)]));
            };
            if let Object::Operative(op) = self.heap.get_mut(r)? {
                op.native = Some(native);
            }
        }
        Ok(())
    }

    /// Evaluates the first of `forms`, a proper list, the last as a tail call; none is `#inert`.
    fn next_form(
        &mut self,
        forms: Value,
        context: Reference,
        at: Reference,
    ) -> Result<Mode, Error> {
        let Value::Pair(r) = forms else {
            return Ok(Mode::Return(Value::Inert));
        };
        let (form, rest) = self.pair_at(r)?;
        if rest != Value::EmptyList {
            self.frame(What::Body { rest }, context, at)?;
        }
        Ok(Mode::Evaluate(form, context))
    }

    /// The names `context` binds, sorted, as a list. Apart from [`Grift::primitive`], so that it
    /// doesn't change how the common primitives inline.
    #[inline(never)]
    fn names_list(&mut self, context: Reference) -> Result<Value, Error> {
        let names = self.names_of(context)?;
        names.iter().rev().try_fold(Value::EmptyList, |list, &s| {
            self.pair_local(Value::Symbol(s), list)
        })
    }

    fn primitive(&mut self, p: Primitive, context: Reference, base: usize) -> Result<Mode, Error> {
        use Primitive as P;
        let given = self.machine.arguments.get(base..).unwrap_or_default();
        let (lo, hi) = p.arity();
        if given.len() < lo || given.len() > hi {
            return Err(self.arity_error(base));
        }
        let mut a = [Value::EmptyList; 3];
        a.iter_mut().zip(given).for_each(|(d, s)| *d = *s);
        let [x, y, z] = a;
        let v = match p {
            P::Vau => {
                let size = self.check_parameters(x, y)?;
                let body = self.arguments_list(base + 2)?;
                let op = Operative {
                    parameters: x,
                    context_parameter: y,
                    body,
                    context,
                    size: u32::try_from(size).map_err(|_| Error::oom())?,
                    native: None,
                };
                Value::Operative(self.heap.alloc(Object::Operative(op))?)
            }
            P::Wrap if !is_combiner(x) => return Err(type_error(x)),
            P::Wrap => Value::Applicative(self.heap.alloc(Object::Applicative(x))?),
            P::Unwrap => match x {
                Value::Applicative(r) => self.unwrapped(r)?,
                _ => return Err(type_error(x)),
            },
            P::Evaluate => return Ok(Mode::Evaluate(x, context_kind(y)?)),
            P::Context => {
                let parent = match x {
                    Value::Context(c) => Some(c),
                    Value::EmptyList => None,
                    _ => return Err(type_error(x)),
                };
                Value::Context(self.heap.context(parent)?)
            }
            P::Bind => {
                self.bind_tree(context_kind(x)?, y, z)?;
                Value::Inert
            }
            P::Unbind | P::Lookup => {
                let (Value::Context(c), Value::Symbol(s)) = (x, y) else {
                    let bad = if matches!(x, Value::Context(_)) { y } else { x };
                    return Err(type_error(bad));
                };
                match p {
                    P::Unbind => Value::Boolean(self.bindings_mut(c)?.remove(&s).is_some()),
                    _ => self.find(c, s)?.unwrap_or(z),
                }
            }
            P::Names => self.names_list(context_kind(x)?)?,
            P::Pair => self.pair_local(x, y)?,
            P::First | P::Rest => match x {
                Value::Pair(r) => {
                    let (a, b) = self.pair_at(r)?;
                    if p == P::First { a } else { b }
                }
                _ => return Err(type_error(x)),
            },
            P::Same => Value::Boolean(self.same(x, y)?),
            P::Select => match x {
                Value::Boolean(true) => y,
                Value::Boolean(false) => z,
                _ => return Err(type_error(x)),
            },
            P::TypeOf => Value::Symbol(type_of(x).into()),
            P::Add | P::Subtract | P::Multiply | P::Divide | P::Remainder | P::LessThan => {
                self.arithmetic(p, x, y)?
            }
            P::BitwiseAnd | P::BitwiseOr | P::BitwiseExclusiveOr | P::ShiftLeft | P::ShiftRight => {
                bitwise(p, x, y)?
            }
            P::Power => Value::Float(float_of(x)?.powf(float_of(y)?)),
            P::SquareRoot => Value::Float(float_of(x)?.sqrt()),
            P::Sine => Value::Float(float_of(x)?.sin()),
            P::Cosine => Value::Float(float_of(x)?.cos()),
            P::Tangent => Value::Float(float_of(x)?.tan()),
            P::Exponential => Value::Float(float_of(x)?.exp()),
            P::NaturalLogarithm => Value::Float(float_of(x)?.ln()),
            P::Floor => Value::Float(float_of(x)?.floor()),
            P::Ceiling => Value::Float(float_of(x)?.ceil()),
            P::Round => Value::Float(float_of(x)?.round()),
            P::Truncate => Value::Float(float_of(x)?.trunc()),
            P::AbsoluteValue => Value::Float(float_of(x)?.abs()),
            P::IntegerToFloat => Value::Float(integer_of(x)? as f64),
            // Rust's `as`: saturating, and not-a-number becomes 0.
            P::FloatToInteger => Value::Integer(float_of(x)? as i64),
            P::EvaluateWithCleanup | P::EvaluateWithHandler if !is_combiner(z) => {
                return Err(type_error(z));
            }
            P::EvaluateWithCleanup | P::EvaluateWithHandler => {
                let c = context_kind(y)?;
                let at = self.machine.here;
                if p == P::EvaluateWithHandler {
                    self.frame(What::Handler { handler: z, base }, c, at)?;
                } else {
                    let m = &mut self.machine;
                    m.waiting.try_reserve(1).map_err(|_| Error::oom())?;
                    m.frames.try_reserve(2).map_err(|_| Error::oom())?;
                    self.frame(What::Cleanup { cleanup: z, base }, c, at)?;
                    self.machine.waiting.push(Ok(Value::Inert));
                }
                return Ok(Mode::Evaluate(x, c));
            }
            P::Raise => {
                return Err(self.error_of(x)?);
            }
            P::ByteAt
            | P::Join
            | P::ByteLength
            | P::Slice
            | P::Search
            | P::StringToBytes
            | P::BytesToString
            | P::ListToBytes
            | P::BytesToList => self.sequence_primitive(p, [x, y, z], base)?,
            P::CodePointToString => {
                let c = u32::try_from(integer_of(x)?).ok().and_then(char::from_u32);
                let c = c.ok_or_else(|| Error::new(K::RangeError, &[x]))?;
                self.string_local(c.encode_utf8(&mut [0; 4]))?
            }
            P::StringToSymbol => {
                let name = boxed(self.text(x)?)?;
                Value::Symbol(self.intern(&name)?)
            }
            P::IntegerToString => {
                integer_of(x)?;
                self.printed(x)?
            }
            P::ValueToString => self.printed(x)?,
            P::StringToInteger | P::StringToFloat => {
                let s = self.text(x)?;
                let n = match p {
                    P::StringToInteger => s.parse().ok().map(Value::Integer),
                    _ => float_name(s).or_else(|| s.parse().ok()).map(Value::Float),
                };
                n.ok_or_else(|| Error::new(K::ParseError, &[x]))?
            }
            P::ReadForm => {
                let Value::String(r) = x else {
                    return Err(type_error(x));
                };
                let offset = usize::try_from(integer_of(y)?).ok();
                // The text is lent to the reader, not copied: a program reads a long text one
                // form at a time.
                let Object::String(text) = self.heap.get_mut(r)? else {
                    return Err(Error::new(K::StaleValue, &[x]));
                };
                let text = std::mem::take(text);
                let read = offset
                    .filter(|&i| text.is_char_boundary(i))
                    .map(|i| self.read_form(&text, i, 1, None));
                if let Object::String(s) = self.heap.get_mut(r)? {
                    *s = text;
                }
                let read = read.ok_or_else(|| Error::new(K::RangeError, &[x, y]))?;
                match read? {
                    Some((form, place, _)) => {
                        self.list_local(&[form, integer(place.start), integer(place.end)])?
                    }
                    None => Value::EmptyList,
                }
            }
        };
        Ok(Mode::Return(v))
    }

    /// The arguments from `base` don't fit the primitive: an `arity-error` carrying them as one
    /// list. Cold and apart, so that the check costs the machine's loop nothing more.
    #[cold]
    #[inline(never)]
    fn arity_error(&mut self, base: usize) -> Error {
        match self.arguments_list(base) {
            Ok(operands) => Error::new(K::ArityError, &[operands]),
            Err(e) => e,
        }
    }

    /// The primitives on strings and bytes, apart from [`Grift::primitive`] so that its common cases
    /// stay small enough for the compiler to inline it into the machine's loop. A string or a bytes
    /// value is a sequence of bytes: the first argument's kind is the answer's, and a string is cut
    /// only on character boundaries.
    #[inline(never)]
    fn sequence_primitive(
        &mut self,
        p: Primitive,
        [x, y, z]: [Value; 3],
        base: usize,
    ) -> Result<Value, Error> {
        use Primitive as P;
        Ok(match p {
            P::ByteLength => integer(self.octets(x)?.len()),
            P::ByteAt => {
                let (b, i) = (self.octets(x)?, integer_of(y)?);
                let byte = usize::try_from(i).ok().and_then(|i| b.get(i));
                let byte = byte.ok_or_else(|| Error::new(K::RangeError, &[x, y]))?;
                Value::Integer(i64::from(*byte))
            }
            P::Slice => {
                self.octets(x)?;
                let range = (
                    usize::try_from(integer_of(y)?),
                    usize::try_from(integer_of(z)?),
                );
                let part = match (range, x) {
                    ((Ok(a), Ok(b)), Value::String(_)) => self
                        .text(x)?
                        .get(a..b)
                        .map(|t| boxed(t).map(Object::String)),
                    ((Ok(a), Ok(b)), _) => self
                        .bytes_of(x)?
                        .get(a..b)
                        .map(|b| collected(b.iter().copied()).map(Object::Bytes)),
                    _ => None,
                };
                let part = part.ok_or_else(|| Error::new(K::RangeError, &[x, y, z]))??;
                Self::sequence_of(matches!(x, Value::Bytes(_)), self.heap.alloc(part)?)
            }
            P::Join => {
                let given = self.machine.arguments.get(base..).unwrap_or_default();
                let bytes = matches!(given.first(), Some(Value::Bytes(_)));
                let mut len = 0usize;
                for &v in given {
                    if matches!(v, Value::Bytes(_)) != bytes {
                        return Err(type_error(v));
                    }
                    len = len
                        .checked_add(self.octets(v)?.len())
                        .ok_or_else(Error::oom)?;
                }
                // Exactly the length, so boxing a string never reallocates.
                let made = if bytes {
                    let mut joined = Vec::new();
                    joined.try_reserve_exact(len).map_err(|_| Error::oom())?;
                    for &v in given {
                        joined.extend_from_slice(self.bytes_of(v)?);
                    }
                    Object::Bytes(joined)
                } else {
                    let mut joined = String::new();
                    joined.try_reserve_exact(len).map_err(|_| Error::oom())?;
                    for &v in given {
                        joined.push_str(self.text(v)?);
                    }
                    Object::String(joined.into_boxed_str())
                };
                Self::sequence_of(bytes, self.heap.alloc(made)?)
            }
            P::Search => {
                let (text, pattern) = (self.octets(x)?, self.octets(y)?);
                if std::mem::discriminant(&x) != std::mem::discriminant(&y) {
                    return Err(type_error(y));
                }
                let bad = || Error::new(K::RangeError, &[x, y, z]);
                let start = usize::try_from(integer_of(z)?).map_err(|_| bad())?;
                // A string starts on a character boundary; then a valid pattern in valid text
                // matches only on character boundaries.
                let rest = match x {
                    Value::String(_) => self.text(x)?.get(start..).map(str::as_bytes),
                    _ => text.get(start..),
                };
                let at = self.search(rest.ok_or_else(bad)?, pattern)?;
                at.map_or(Value::EmptyList, |at| integer(start + at))
            }
            P::StringToBytes => {
                let b = collected(self.text(x)?.bytes())?;
                self.bytes_local(b)?
            }
            P::BytesToString => {
                let text = std::str::from_utf8(self.bytes_of(x)?).map_err(|e| {
                    let at = integer(e.valid_up_to());
                    Error::new(K::EncodingError, &[x, at])
                })?;
                let text = boxed(text)?;
                Value::String(self.heap.alloc(Object::String(text))?)
            }
            P::ListToBytes => {
                let mut bytes = Vec::new();
                let mut rest = x;
                while let Value::Pair(r) = rest {
                    let (v, next) = self.pair_at(r)?;
                    let at = integer(bytes.len());
                    let byte = match v {
                        Value::Integer(n) => {
                            u8::try_from(n).map_err(|_| Error::new(K::RangeError, &[v, at]))?
                        }
                        _ => return Err(Error::new(K::TypeError, &[v, at])),
                    };
                    push(&mut bytes, byte)?;
                    rest = next;
                }
                if rest != Value::EmptyList {
                    let at = integer(bytes.len());
                    return Err(Error::new(K::TypeError, &[rest, at]));
                }
                self.bytes_local(bytes)?
            }
            P::BytesToList => {
                let b = collected(self.bytes_of(x)?.iter().copied())?;
                b.iter().rev().try_fold(Value::EmptyList, |list, &byte| {
                    self.pair_local(Value::Integer(i64::from(byte)), list)
                })?
            }
            _ => return Err(type_error(x)),
        })
    }

    /// A bytes value or a string for object `r`.
    fn sequence_of(bytes: bool, r: Reference) -> Value {
        if bytes {
            Value::Bytes(r)
        } else {
            Value::String(r)
        }
    }

    /// The error an error context describes: one `error` made, or a handler got.
    fn error_of(&mut self, e: Value) -> Result<Error, Error> {
        let bad = || type_error(e);
        let Value::Context(r) = e else {
            return Err(bad());
        };
        let field = |k: K| self.find(r, k.into())?.ok_or_else(bad);
        let Value::Symbol(kind) = field(K::Kind)? else {
            return Err(bad());
        };
        let (values, trace) = (field(K::Values)?, field(K::Trace)?);
        let values = self.items(values, e)?;
        // The trace the last handler got comes back whole, its combinations with it.
        if let Some((_, seen)) = self.machine.handed.take_if(|(h, _)| *h == trace) {
            return Ok(Error {
                shared: trace,
                seen,
                ..Error::from_parts(kind, values, Vec::new())?
            });
        }
        let mut given = self.items(trace, e)?;
        given.reverse();
        Error::from_parts(kind, values, given)
    }

    /// The elements of a proper list; anything else is a `type-error` naming `culprit`.
    pub(crate) fn items(&self, mut list: Value, culprit: Value) -> Result<Vec<Value>, Error> {
        let mut v = Vec::new();
        while let Value::Pair(r) = list {
            let (x, rest) = self.pair_at(r)?;
            push(&mut v, x)?;
            list = rest;
        }
        if list == Value::EmptyList {
            Ok(v)
        } else {
            Err(type_error(culprit))
        }
    }

    /// Whether `x` and `y` are the same value: equal integers, symbols, strings and bytes, floats
    /// with the same bits, and otherwise the same object.
    fn same(&self, x: Value, y: Value) -> Result<bool, Error> {
        Ok(match (x, y) {
            (Value::String(_), Value::String(_)) => self.text(x)? == self.text(y)?,
            (Value::Bytes(_), Value::Bytes(_)) => self.bytes_of(x)? == self.bytes_of(y)?,
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            _ => x == y,
        })
    }

    /// Arithmetic and comparison, as Rust has them, on two integers or two floats, and `less-than?`
    /// on two strings or two bytes values, in byte order. Integer arithmetic is checked: `divide` and
    /// `remainder` are Rust's `/` and `%`, truncating, and fail where Rust's would panic. Float
    /// arithmetic is IEEE.
    #[inline]
    fn arithmetic(&self, p: Primitive, x: Value, y: Value) -> Result<Value, Error> {
        use Primitive as P;
        let r = match (x, y) {
            (Value::Integer(a), Value::Integer(b)) => match p {
                P::Add => a.checked_add(b),
                P::Subtract => a.checked_sub(b),
                P::Multiply => a.checked_mul(b),
                P::Divide | P::Remainder if b == 0 => {
                    return Err(Error::new(K::DivisionByZero, &[x, y]));
                }
                P::Divide => a.checked_div(b),
                P::Remainder => a.checked_rem(b),
                _ => return Ok(Value::Boolean(a < b)),
            },
            (Value::Float(a), Value::Float(b)) => {
                return Ok(Value::Float(match p {
                    P::Add => a + b,
                    P::Subtract => a - b,
                    P::Multiply => a * b,
                    P::Divide => a / b,
                    P::Remainder => a % b,
                    _ => return Ok(Value::Boolean(a < b)),
                }));
            }
            (Value::Integer(_) | Value::Float(_), _) => return Err(type_error(y)),
            (Value::String(_), Value::String(_)) | (Value::Bytes(_), Value::Bytes(_))
                if p == P::LessThan =>
            {
                return Ok(Value::Boolean(self.octets(x)? < self.octets(y)?));
            }
            _ => return Err(type_error(x)),
        };
        r.map(Value::Integer)
            .ok_or_else(|| Error::new(K::Overflow, &[x, y]))
    }

    /// The contents of string `v`, or a type error.
    pub(crate) fn text(&self, v: Value) -> Result<&str, Error> {
        match v {
            Value::String(r) => match self.heap.get(r)? {
                Object::String(s) => Ok(s),
                _ => Err(Error::new(K::StaleValue, &[v])),
            },
            _ => Err(type_error(v)),
        }
    }

    /// The bytes of string or bytes value `v`, or a type error.
    pub(crate) fn octets(&self, v: Value) -> Result<&[u8], Error> {
        match v {
            Value::String(_) => self.text(v).map(str::as_bytes),
            _ => self.bytes_of(v),
        }
    }

    /// The contents of bytes value `v`, or a type error.
    pub(crate) fn bytes_of(&self, v: Value) -> Result<&[u8], Error> {
        match v {
            Value::Bytes(r) => match self.heap.get(r)? {
                Object::Bytes(b) => Ok(b),
                _ => Err(Error::new(K::StaleValue, &[v])),
            },
            _ => Err(type_error(v)),
        }
    }

    /// A new list of `xs`.
    pub(crate) fn list_local(&mut self, xs: &[Value]) -> Result<Value, Error> {
        xs.iter()
            .rev()
            .try_fold(Value::EmptyList, |l, &x| self.pair_local(x, l))
    }

    /// A new pair, the library's if the combination being combined is.
    pub(crate) fn pair_local(&mut self, a: Value, b: Value) -> Result<Value, Error> {
        let origin = Origin {
            place: None,
            library: self.machine.library,
            outer: false,
        };
        Ok(Value::Pair(self.heap.alloc(Object::Pair(a, b, origin))?))
    }

    pub(crate) fn string_local(&mut self, s: &str) -> Result<Value, Error> {
        let s = boxed(s)?;
        Ok(Value::String(self.heap.alloc(Object::String(s))?))
    }

    pub(crate) fn bytes_local(&mut self, b: Vec<u8>) -> Result<Value, Error> {
        Ok(Value::Bytes(self.heap.alloc(Object::Bytes(b))?))
    }

    /// Checks a parameter tree by matching it to itself, and a context parameter, `#ignore` or a
    /// symbol not in the tree. The number of bindings a call makes.
    pub(crate) fn check_parameters(
        &mut self,
        tree: Value,
        context_parameter: Value,
    ) -> Result<usize, Error> {
        let mut bound = Bindings::default();
        self.match_tree(&mut bound, tree, tree)?;
        match context_parameter {
            Value::Symbol(s) if !bound.contains_key(&s) => Ok(bound.len() + 1),
            Value::Ignore => Ok(bound.len()),
            _ => Err(Error::new(K::ParameterError, &[context_parameter])),
        }
    }

    /// Matches a parameter tree to a value, binding its symbols in `bindings`, which it expects to
    /// have none of them: a symbol bound twice is a `parameter-error`, as is a leaf of the tree
    /// other than a symbol, `#ignore` and `()`.
    pub(crate) fn match_tree(
        &mut self,
        bindings: &mut Bindings,
        tree: Value,
        value: Value,
    ) -> Result<(), Error> {
        let mut work = std::mem::take(&mut self.machine.work);
        work.clear();
        let mut result = push(&mut work, (tree, value));
        let mut walked = 0;
        while let (Ok(()), Some((t, v))) = (&result, work.pop()) {
            result = match (t, v) {
                _ if self.walk_stopped(&mut walked) => Err(Error::interrupted()),
                (Value::Symbol(s), _) => match bindings.try_reserve(1) {
                    Ok(()) if bindings.insert(s, v).is_none() => Ok(()),
                    Ok(()) => Err(Error::new(K::ParameterError, &[tree])),
                    Err(_) => Err(Error::oom()),
                },
                (Value::Ignore, _) | (Value::EmptyList, Value::EmptyList) => Ok(()),
                (Value::Pair(t), Value::Pair(v)) => self.pair_at(t).and_then(|(ta, tb)| {
                    let (va, vb) = self.pair_at(v)?;
                    push(&mut work, (tb, vb))?;
                    push(&mut work, (ta, va))
                }),
                (Value::Pair(_) | Value::EmptyList, _) => Err(Error::new(K::ArityError, &[value])),
                _ => Err(Error::new(K::ParameterError, &[tree])),
            };
        }
        self.machine.work = work;
        result
    }

    /// Matches parameter tree `tree` to `value`, binding its symbols in context `c` itself, all of
    /// them or, if it fails, none.
    pub(crate) fn bind_tree(
        &mut self,
        c: Reference,
        tree: Value,
        value: Value,
    ) -> Result<(), Error> {
        let mut matched = Bindings::default();
        self.match_tree(&mut matched, tree, value)?;
        let bindings = self.bindings_mut(c)?;
        bindings
            .try_reserve(matched.len())
            .map_err(|_| Error::oom())?;
        bindings.extend(matched);
        Ok(())
    }

    /// Binds `s` in `context` itself, replacing an existing binding there.
    pub(crate) fn bind_symbol(
        &mut self,
        context: Reference,
        s: Symbol,
        v: Value,
    ) -> Result<(), Error> {
        let bindings = self.bindings_mut(context)?;
        bindings.try_reserve(1).map_err(|_| Error::oom())?;
        bindings.insert(s, v);
        Ok(())
    }

    pub(crate) fn bindings_mut(&mut self, context: Reference) -> Result<&mut Bindings, Error> {
        match self.heap.get_mut(context)? {
            Object::Context(_, bindings) => Ok(bindings),
            _ => Err(Error::new(K::StaleValue, &[Value::Context(context)])),
        }
    }

    pub(crate) fn context_at(
        &self,
        context: Reference,
    ) -> Result<(Option<Reference>, &Bindings), Error> {
        match self.heap.get(context)? {
            Object::Context(parent, bindings) => Ok((*parent, bindings)),
            _ => Err(Error::new(K::StaleValue, &[Value::Context(context)])),
        }
    }

    fn value_of(&self, context: Reference, s: Symbol) -> Result<Value, Error> {
        self.find(context, s)?
            .ok_or_else(|| Error::new(K::UnboundSymbol, &[Value::Symbol(s)]))
    }

    /// The binding of `s` in `context` or the nearest parent that has one.
    pub(crate) fn find(&self, mut context: Reference, s: Symbol) -> Result<Option<Value>, Error> {
        loop {
            let (parent, bindings) = self.context_at(context)?;
            if let Some(&v) = bindings.get(&s) {
                return Ok(Some(v));
            }
            let Some(parent) = parent else {
                return Ok(None);
            };
            context = parent;
        }
    }

    /// The symbols bound in `context` itself, sorted by name.
    pub(crate) fn names_of(&self, context: Reference) -> Result<Vec<Symbol>, Error> {
        let (_, bindings) = self.context_at(context)?;
        let mut names = collected(bindings.keys().copied())?;
        names.sort_unstable_by(|&a, &b| self.name(a).cmp(self.name(b)));
        Ok(names)
    }

    pub(crate) fn pair_at(&self, r: Reference) -> Result<(Value, Value), Error> {
        match self.heap.get(r)? {
            Object::Pair(a, b, _) => Ok((*a, *b)),
            _ => Err(Error::new(K::StaleValue, &[Value::Pair(r)])),
        }
    }

    pub(crate) fn origin_at(&self, r: Reference) -> Result<Origin, Error> {
        match self.heap.get(r)? {
            Object::Pair(_, _, origin) => Ok(*origin),
            _ => Err(Error::new(K::StaleValue, &[Value::Pair(r)])),
        }
    }

    pub(crate) fn unwrapped(&self, r: Reference) -> Result<Value, Error> {
        match self.heap.get(r)? {
            Object::Applicative(c) => Ok(*c),
            _ => Err(Error::new(K::StaleValue, &[Value::Applicative(r)])),
        }
    }
}

/// Adds program combination `r` to the error's trace, unless it is there already. With no
/// memory to spare the trace stays as it is: the error matters more than its last location.
fn locate(e: &mut Error, r: Reference) {
    if r != Reference::NONE
        && e.seen.try_reserve(1).is_ok()
        && e.trace.try_reserve(1).is_ok()
        && e.seen.insert(r.idx)
    {
        e.trace.push(Value::Pair(r));
    }
}

/// The bitwise operations, as Rust's `&`, `|`, `^`, `<<` and `>>` on `i64`: a shift by an amount
/// outside 0 to 63 is `overflow`, where Rust panics, and the bits shifted out are lost, as in Rust.
fn bitwise(p: Primitive, x: Value, y: Value) -> Result<Value, Error> {
    use Primitive as P;
    let (a, b) = (integer_of(x)?, integer_of(y)?);
    let amount = u32::try_from(b).ok();
    let r = match p {
        P::BitwiseAnd => Some(a & b),
        P::BitwiseOr => Some(a | b),
        P::BitwiseExclusiveOr => Some(a ^ b),
        P::ShiftLeft => amount.and_then(|n| a.checked_shl(n)),
        _ => amount.and_then(|n| a.checked_shr(n)),
    };
    r.map(Value::Integer)
        .ok_or_else(|| Error::new(K::Overflow, &[x, y]))
}

/// The Two-Way search (Crochemore and Perrin, 1991), as in Rust's `str::find`, CPython and glibc:
/// linear time in the text and the pattern, no memory. Every byte it reads is an item of a walk, so
/// a stop reaches it in every phase through [`Grift::walk_stopped`]. Strings search through it too.
impl<T> Grift<T> {
    /// Where `p` first is in `text`; an empty pattern is at 0.
    pub(crate) fn search(&self, text: &[u8], p: &[u8]) -> Result<Option<usize>, Error> {
        let n = p.len();
        if n == 0 {
            return Ok(Some(0));
        }
        let mut walked = 0;
        // The later of the two maximal suffixes splits the pattern at a critical position.
        let (split, period) = self
            .maximal_suffix(p, false, &mut walked)?
            .max(self.maximal_suffix(p, true, &mut walked)?);
        // A left part that repeats in the right: the bytes a shift keeps needn't be read again.
        let shifted = p.get(period..).unwrap_or_default();
        let (shift, kept) = if self
            .difference(0..split, p, shifted, &mut walked)?
            .is_none()
        {
            (period, n - period)
        } else {
            (split.max(n - split) + 1, 0)
        };
        // Which bytes, by their low six bits, the pattern holds: a window ending in another one
        // can't overlap a match.
        let mut held = 0u64;
        for &b in p {
            if self.walk_stopped(&mut walked) {
                return Err(Error::interrupted());
            }
            held |= 1 << (b & 63);
        }
        let (mut at, mut memory) = (0, 0);
        while let Some(window) = text.get(at..at + n) {
            if window.last().is_some_and(|b| held >> (b & 63) & 1 == 0) {
                if self.walk_stopped(&mut walked) {
                    return Err(Error::interrupted());
                }
                (at, memory) = (at + n, 0);
            } else if let Some(i) = self.difference(split.max(memory)..n, p, window, &mut walked)? {
                (at, memory) = (at + i + 1 - split, 0);
            } else if self
                .difference((memory..split).rev(), p, window, &mut walked)?
                .is_some()
            {
                (at, memory) = (at + shift, kept);
            } else {
                return Ok(Some(at));
            }
        }
        Ok(None)
    }

    /// The first place, in the order of `places`, where `a` and `b` differ.
    fn difference(
        &self,
        places: impl Iterator<Item = usize>,
        a: &[u8],
        b: &[u8],
        walked: &mut usize,
    ) -> Result<Option<usize>, Error> {
        for i in places {
            if self.walk_stopped(walked) {
                return Err(Error::interrupted());
            }
            if a.get(i) != b.get(i) {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }

    /// Where the maximal suffix of `p` begins, and its period; `reversed` flips the byte order.
    fn maximal_suffix(
        &self,
        p: &[u8],
        reversed: bool,
        walked: &mut usize,
    ) -> Result<(usize, usize), Error> {
        let (mut left, mut right, mut offset, mut period) = (0, 1, 0, 1);
        while let (Some(&a), Some(&b)) = (p.get(right + offset), p.get(left + offset)) {
            if self.walk_stopped(walked) {
                return Err(Error::interrupted());
            }
            if a == b {
                offset += 1;
                if offset == period {
                    (right, offset) = (right + period, 0);
                }
            } else if (a < b) != reversed {
                (right, offset) = (right + offset + 1, 0);
                period = right - left;
            } else {
                (left, right, offset, period) = (right, right + 1, 0, 1);
            }
        }
        Ok((left, period))
    }
}

#[cfg(test)]
impl<T> Grift<T> {
    /// The primitive `search` from the start of `text`, with no step of the machine around it.
    pub(crate) fn search_directly(&mut self, text: Value, pattern: Value) -> Result<Value, Error> {
        self.sequence_primitive(Primitive::Search, [text, pattern, Value::Integer(0)], 0)
    }
}

/// A length or offset as a grift integer.
pub(crate) fn integer(n: usize) -> Value {
    Value::Integer(i64::try_from(n).unwrap_or(i64::MAX))
}

fn integer_of(v: Value) -> Result<i64, Error> {
    match v {
        Value::Integer(n) => Ok(n),
        _ => Err(type_error(v)),
    }
}

fn float_of(v: Value) -> Result<f64, Error> {
    match v {
        Value::Float(f) => Ok(f),
        _ => Err(type_error(v)),
    }
}

#[inline]
fn is_combiner(v: Value) -> bool {
    matches!(type_of(v), K::Operative | K::Applicative)
}

fn type_of(v: Value) -> K {
    match v {
        Value::EmptyList => K::EmptyList,
        Value::Boolean(_) => K::Boolean,
        Value::Inert => K::Inert,
        Value::Ignore => K::Ignore,
        Value::Integer(_) => K::Integer,
        Value::Float(_) => K::Float,
        Value::Symbol(_) => K::Symbol,
        Value::Pair(_) => K::Pair,
        Value::String(_) => K::String,
        Value::Bytes(_) => K::Bytes,
        Value::Context(_) => K::Context,
        Value::Operative(_) | Value::Primitive(_) | Value::Host(_) => K::Operative,
        Value::Applicative(_) => K::Applicative,
    }
}
