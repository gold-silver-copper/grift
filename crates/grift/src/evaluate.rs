//! The evaluator: an explicit machine loop over a frame stack, and the Rust primitives.
//!
//! The machine's state is its frame stack, its argument stack, the endings waiting for a
//! cleanup to finish, where it is in the program and the current [`Mode`]. The collector runs
//! at the top of the loop and before a host's evaluation or call needs its first slot, with that
//! state, the root contexts and the new call's checked inputs as its roots. Allocation never
//! collects: Rust locals stay valid until the next run entry or machine safe point. The loop
//! checks for a stop just after each step. A run is over when its frames are gone. A host
//! function begins none: its [`HostCall`] has no way to begin one.
//!
//! A run ends one way: an error unwinds it. An error the host or the machine began is a limit,
//! whatever its kind is called, and two things follow: handlers pass it, and when a new error meets
//! one waiting, the rule in [`Grift::unwind`] decides. A stop is such an error. A force is a stop
//! that stays asked, so every cleanup it meets is cut before its first form.
//!
//! Two library forms have a native twin: `if!` and `sequence!` are the library's own operatives,
//! flagged, and run in Rust when their operands have the shape the twin handles; any other shape
//! runs the operative's body, so what a program sees never differs.

use crate::heap::kind::{Context, Pair};
use crate::heap::{Bindings, Operative, Origin, Ref, kind};
use crate::local::{Error, Host, K, Symbol, Trace, Value, boxed, push};
use crate::read::float_name;
use crate::{Answer, Entry, Grift, HostCall, collected, context_kind};

/// Declares the primitives: each one's variant and its name in the built-in context. How many
/// operands each takes is the pattern its arm of [`Grift::primitive`] takes them with. Those
/// `natives.grift` defines exist for speed only, and that definition is their meaning; the others
/// are what grift can't express.
macro_rules! primitives {
    ($($p:ident $name:literal;)*) => {
        /// The Rust primitives, each bound wrapped, as an applicative, until the library says otherwise.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum Primitive { $($p),* }

        impl Primitive {
            pub(crate) const ALL: &[Primitive] = &[$(Primitive::$p),*];

            /// The name the primitive is bound to in the built-in context.
            pub(crate) fn name(self) -> &'static str {
                match self { $(Primitive::$p => $name),* }
            }
        }
    };
}

primitives! {
    // Abstraction, wrapping and evaluation: the machine itself.
    Vau "vau!";
    Wrap "wrap";
    Unwrap "unwrap";
    Evaluate "evaluate";
    // Unwinding: machine frames.
    EvaluateWithCleanup "evaluate-with-cleanup";
    EvaluateWithHandler "evaluate-with-handler";
    Raise "raise";
    // Making, changing and listing bindings.
    Context "context";
    Bind "bind";
    Unbind "unbind";
    Names "names";
    // Making a pair; identity and representation.
    Pair "pair";
    Same "same?";
    TypeOf "type-of";
    // Checked integers and IEEE floats, and Rust's float functions (G-42); natives.grift defines
    // all but the first four and sine to natural-logarithm.
    Add "add";
    Subtract "subtract";
    Multiply "multiply";
    Divide "divide";
    Remainder "remainder";
    LessThan "less-than?";
    Power "power";
    SquareRoot "square-root";
    Sine "sine";
    Cosine "cosine";
    Tangent "tangent";
    Exponential "exponential";
    NaturalLogarithm "natural-logarithm";
    Floor "floor";
    Ceiling "ceiling";
    Round "round";
    Truncate "truncate";
    AbsoluteValue "absolute-value";
    // A string's or a bytes value's bytes, joining them, making names and floats from text, the
    // reader and the printer.
    ByteAt "byte-at";
    Join "join";
    StringToSymbol "string-to-symbol";
    StringToFloat "string-to-float";
    ReadForm "read-form";
    ValueToString "value-to-string";
    // natives.grift defines all of these.
    First "first";
    Rest "rest";
    Select "select";
    Lookup "lookup";
    BitwiseAnd "bitwise-and";
    BitwiseOr "bitwise-or";
    BitwiseExclusiveOr "bitwise-exclusive-or";
    ShiftLeft "shift-left";
    ShiftRight "shift-right";
    IntegerToFloat "integer-to-float";
    FloatToInteger "float-to-integer";
    ByteLength "byte-length";
    Slice "slice";
    CodePointToString "code-point-to-string";
    Search "search";
    IntegerToString "integer-to-string";
    StringToInteger "string-to-integer";
    // A bytes value's contents: the printer writes them and the reader reads them back.
    StringToBytes "string-to-bytes";
    BytesToString "bytes-to-string";
    ListToBytes "list-to-bytes";
    BytesToList "bytes-to-list";
}

/// A walk along a list's items inside one step, charged to [`Control::walk`], so a stop
/// ends it however long the list is. Every list walk goes through one: the lint on
/// [`Heap::pair_at`](crate::heap::Heap::pair_at) (`clippy.toml`) refuses another.
pub(crate) struct Items {
    rest: Value,
    walked: usize,
}

impl Items {
    pub(crate) fn new(list: Value) -> Items {
        Items {
            rest: list,
            walked: 0,
        }
    }

    /// The next item, or `None` once what is left isn't a pair.
    pub(crate) fn next<T>(&mut self, g: &Grift<T>) -> Result<Option<Value>, Error> {
        let Value::Pair(r) = self.rest else {
            return Ok(None);
        };
        g.control.walk(&mut self.walked)?;
        #[expect(clippy::disallowed_methods, reason = "the one list walk")]
        let (x, rest, _) = g.heap.pair_at(r)?;
        self.rest = rest;
        Ok(Some(x))
    }

    /// What is left: `()` after a proper list.
    pub(crate) fn rest(&self) -> Value {
        self.rest
    }
}

/// The library forms with a native twin.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Native {
    If,
    Sequence,
}

/// Every twin and the library operative it is the twin of. One left out here is never made, which
/// the compiler warns of.
pub(crate) const NATIVES: [(Native, &str); 2] =
    [(Native::If, "if!"), (Native::Sequence, "sequence!")];

/// What the machine does next.
#[derive(Clone, Copy)]
pub(crate) enum Mode {
    /// Evaluate an expression in a context.
    Evaluate(Value, Ref<Context>),
    /// Deliver a value to the top frame.
    Return(Value),
    /// Call a host function with the arguments on the argument stack from `base`.
    Host(Host, usize),
}

/// A continuation frame: what to do with the value being returned, the context it happens in, and
/// the combination of the program it belongs to ([`Ref::NONE`] for none), where an error is
/// reported.
#[derive(Clone, Copy)]
struct Frame {
    what: What,
    context: Ref<Context>,
    at: Ref<Pair>,
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
            What::Cleanup { cleanup: v, base } => whole(v) && whole(base),
            What::Handler { handler: v, base } => whole(v) && whole(base),
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
        base: Base(0)
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
    /// `base` is the argument stack's height to go back to, and whether the library made the
    /// combination that set the handler, as the handler's call is then. A value passes through.
    Handler { handler: Value, base: Base },
    /// The cleanup above has ended: the ending it waited on, the last one saved, goes on.
    Finish,
    /// `if!`: the value is the test; evaluate one of these as a tail call.
    Branch {
        consequent: Value,
        alternative: Value,
    },
    /// An operative's body, `sequence!`'s too: the forms left after the one evaluated, at least one.
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
            // A base is a height and a mark, never a value.
            What::Forms { rest: v }
            | What::Combine {
                operands: v,
                base: _,
            }
            | What::Cleanup {
                cleanup: v,
                base: _,
            }
            | What::Handler {
                handler: v,
                base: _,
            }
            | What::Body { rest: v } => (v, empty),
            What::Arguments {
                combiner,
                rest,
                base: _,
            } => (combiner, rest),
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
    /// One per `Finish` frame: `#inert` while its cleanup waits under a `Cleanup` frame, then the
    /// ending that started it. Holding the room from the start, saving an ending can't fail.
    waiting: Vec<Result<Value, Error>>,
    /// The innermost combination of the program being evaluated, or [`Ref::NONE`]. A
    /// `Ref`, not a `Value`: this is written and read at every step.
    here: Ref<Pair>,
    /// Whether the combination being combined is the library's: the pairs made now are too, so
    /// the code the library builds is the library's, and the code a program builds the program's.
    pub(crate) library: bool,
    /// Scratch for matching parameter trees.
    work: Vec<(Value, Value)>,
    /// What a parked run asked its host for, held for as long as it waits.
    pub request: Value,
    /// The trace the last handler got, as one shared list: raising that list again takes it back
    /// with its combinations, so a catch and raise doesn't walk it. Held until replaced or the run
    /// ends.
    handed: Option<Trace>,
}

/// What an ending holds for the collector: its value, or its error's values and trace.
pub(crate) fn ending_values(ending: &Result<Value, Error>) -> impl Iterator<Item = Value> {
    let (value, values, trace) = match ending {
        Ok(v) => (*v, &[][..], None),
        Err(e) => (Value::Symbol(e.kind), e.values.as_slice(), Some(&e.trace)),
    };
    let trace = trace.into_iter().flat_map(Trace::roots);
    std::iter::once(value)
        .chain(values.iter().copied())
        .chain(trace)
}

impl Machine {
    /// What this machine holds for the collector, running or parked.
    /// Every field is named in `roots`, `clear` and `tidy`, so a new one doesn't compile until it
    /// is classed in each.
    pub(crate) fn roots(&self) -> impl Iterator<Item = Value> {
        let Machine {
            frames,
            arguments,
            waiting,
            here,
            request,
            library: _,
            // Not roots. `handed` names a trace only to know it again: a trace collected since
            // can't match, as its slot's generation moved on. `work` is cleared before each use, so
            // what an error leaves in it is never read.
            handed: _,
            work: _,
        } = self;
        (frames.iter().flat_map(|f| f.values()))
            .chain(arguments.iter().copied())
            .chain(waiting.iter().flat_map(ending_values))
            .chain([Value::Pair(*here), *request])
    }

    /// Empty again, keeping its room: a run ended without coming back here is over.
    pub(crate) fn clear(&mut self) {
        let Machine {
            frames,
            arguments,
            waiting,
            here,
            library,
            request,
            handed,
            work: _,
        } = self;
        frames.clear();
        arguments.clear();
        waiting.clear();
        (*here, *library, *request, *handed) = (Ref::NONE, false, Value::Inert, None);
    }

    /// Gives back the room of stacks that were once much deeper than they are.
    pub(crate) fn tidy(&mut self) {
        let Machine {
            frames,
            arguments,
            waiting,
            work,
            here: _,
            library: _,
            request: _,
            handed: _,
        } = self;
        crate::heap::tidy(frames);
        crate::heap::tidy(arguments);
        crate::heap::tidy(waiting);
        crate::heap::tidy(work);
    }
}

impl Default for Machine {
    fn default() -> Machine {
        Machine {
            frames: Vec::new(),
            arguments: Vec::new(),
            waiting: Vec::new(),
            here: Ref::NONE,
            library: false,
            work: Vec::new(),
            request: Value::Inert,
            handed: None,
        }
    }
}

/// That [`Grift::prepare`] came first: only it makes one, and the run on the machine takes it,
/// so every run begins on a prepared heap.
pub(crate) struct Prepared(());
pub(crate) use whole::OnMachine;

pub(crate) fn type_error(v: Value) -> Error {
    Error::new(K::TypeError, &[v])
}

/// What a run begins with, which makes its first frames once it has begun.
#[expect(clippy::large_enum_variant, reason = "one per run, matched at once")]
pub(crate) enum First<'a> {
    /// Reads `src` (of `source`), and evaluates its forms in `context`; `library` marks a host's own
    /// grift code. Reading is the run's first part, so a stop reaches it too.
    Text {
        context: Ref<Context>,
        source: Option<Symbol>,
        src: &'a str,
        library: bool,
    },
    /// Calls applicative `f` with `arguments` as `apply` does.
    Call { f: Value, arguments: &'a [Value] },
    /// Takes up parked `machine`, as it was when it began, its waiting call answering `answer`.
    Resume {
        machine: Machine,
        answer: Result<Value, Error>,
    },
}

/// A whole run on the machine, alone with the proof [`Control::begin`] takes, so no other code can
/// begin one.
mod whole {
    use super::*;
    use crate::{interrupt::Control, parked::MayPark};

    /// That a run begins in [`Grift::run_on_machine`], on a prepared heap.
    pub(crate) struct OnMachine(Prepared);

    impl<T> Grift<T> {
        /// A whole run: begun with the machine clear, its first frames made, run, cleared, and
        /// ended as `run` drops (a host function's panic too). `parks` is the right to park.
        pub(crate) fn run_on_machine(
            &mut self,
            ready: Prepared,
            parks: Option<MayPark>,
            state: Option<&mut T>,
            first: First,
        ) -> Result<Value, Error> {
            // Refused, this run goes without: an `out-of-memory` trace can miss places.
            let _ = self.spare.try_reserve_exact(crate::SPARE);
            self.machine.clear();
            let run = Control::begin(&self.control, OnMachine(ready), parks);
            let mode = match first {
                First::Text {
                    context,
                    source,
                    src,
                    library,
                } => self.read_local(src, source, library).and_then(|forms| {
                    self.frame(What::Forms { rest: forms }, context, Ref::NONE)?;
                    Ok(Mode::Return(Value::Inert))
                }),
                First::Call { f, arguments } => self.call_on_machine(f, arguments, None),
                First::Resume { machine, answer } => {
                    self.machine = machine;
                    self.machine.request = Value::Inert;
                    answer.map(Mode::Return).or_else(|e| self.unwind(e))
                }
            };
            let result = mode.and_then(|mode| self.run(state, mode));
            // A machine that parked or was given up has nothing to go back to.
            self.machine.clear();
            drop(run);
            result
        }
    }
}

impl<T> Grift<T> {
    /// An applicative's underlying combiner; anything else is apply's `type-error`.
    pub(crate) fn combiner_of(&self, f: Value) -> Result<Value, Error> {
        match f {
            Value::Applicative(r) => self.heap.read(r).copied(),
            _ => Err(type_error(f)),
        }
    }

    /// Sets the run in progress aside: its whole machine moves into a kept entry, and this
    /// interpreter's machine is empty again. Everything that can fail comes first, so a run that
    /// can't park goes on where it was, with the error, and that it was asked to stop is the last
    /// of those; filling the entry comes after them all, so there is nothing to undo. Its kept
    /// name is left for [`Grift::started`].
    fn park(&mut self, request: Value) -> Result<(), Error> {
        if !self.control.may_park() {
            return Err(Error::new(K::CannotPark, &[]));
        }
        let mut room = Vec::new();
        room.try_reserve_exact(1)?;
        let name = self.free_name()?;
        self.room_to_forget()?;
        if !self.control.parks() {
            return Err(Error::interrupted());
        }
        self.machine.request = request;
        #[expect(
            clippy::disallowed_methods,
            reason = "room was reserved above, before anything could fail"
        )]
        room.push(std::mem::take(&mut self.machine));
        self.fill_kept(name, Entry::Run(room));
        self.parked_as = Some(name);
        Ok(())
    }

    fn frame(&mut self, what: What, context: Ref<Context>, at: Ref<Pair>) -> Result<(), Error> {
        push(&mut self.machine.frames, Frame { what, context, at })
    }

    fn run(&mut self, mut state: Option<&mut T>, mut mode: Mode) -> Result<Value, Error> {
        loop {
            #[cfg(test)]
            {
                self.heap.probe.steps += 1;
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
            self.collect(current.into_iter(), []);
        }
        Ok(())
    }

    #[inline(always)]
    fn step(&mut self, mode: Mode) -> Result<Mode, Error> {
        Ok(match mode {
            Mode::Evaluate(Value::Symbol(s), context) => Mode::Return(self.value_of(context, s)?),
            Mode::Evaluate(Value::Pair(r), context) => {
                #[expect(clippy::disallowed_methods, reason = "one combination per step")]
                let (combiner, operands, origin) = self.heap.pair_at(r)?;
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
                        #[expect(clippy::disallowed_methods, reason = "one form per step")]
                        let (form, rest, _) = self.heap.pair_at(r)?;
                        #[expect(clippy::disallowed_methods, reason = "a form's origin")]
                        let at = match form {
                            Value::Pair(f) if !self.heap.pair_at(f)?.2.library => f,
                            _ => Ref::NONE,
                        };
                        self.frame(What::Forms { rest }, context, at)?;
                        self.machine.here = Ref::NONE;
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
                            self.machine.here = Ref::NONE;
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
    fn hold_form(&mut self, form: Ref<Pair>) -> Result<(), Error> {
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
            _ => self.frame(What::Form, Ref::NONE, form)?,
        }
        Ok(())
    }

    /// An error that began as refused memory: the next safe point collects, and the spare room is
    /// given up for what reports it. Once: whoever acts on it clears the mark.
    fn acting_on_refusal(&mut self, e: &mut Error) {
        if e.take_refusal() {
            self.heap.request_collection();
            self.spare = Vec::new();
        }
    }

    /// An error stops the evaluation at `here`. Frames are dropped down to the innermost cleanup
    /// or handler, adding where each was to the error's trace. A cleanup then
    /// runs, the error waiting until it is done; a handler, unless the error is a limit, is called
    /// with it instead. What can't be called fails there like any call, with that error, and the
    /// next frame goes on from it. With neither left, the error goes to the caller.
    pub(crate) fn unwind(&mut self, mut e: Error) -> Result<Mode, Error> {
        // The garbage is what the cleanups will run on, and the host's next run: the next safe
        // point collects, with the frames dropped here let go of. Once: what is left to free comes
        // a level at a time, and a collection walks all the levels left. A cleanup refused memory
        // is a new refusal, and asks again.
        self.acting_on_refusal(&mut e);
        e.trace.add(self.machine.here);
        while let Some(Frame { what, context, at }) = self.machine.frames.pop() {
            match what {
                What::Cleanup { cleanup, base } => {
                    e.trace.add(at);
                    self.machine.arguments.truncate(base);
                    self.machine.here = at;
                    return self.start_cleanup(Err(e), cleanup, context, at);
                }
                What::Handler { handler, base } if !e.limit() => {
                    self.machine.arguments.truncate(base.height());
                    self.machine.here = at;
                    self.machine.library = base.library();
                    match self.handle(e, handler, context) {
                        Ok(mode) => return Ok(mode),
                        Err(mut failed) => {
                            self.acting_on_refusal(&mut failed);
                            e = failed;
                            e.trace.add(at);
                        }
                    }
                }
                What::Finish => {
                    // The one rule: the newest reason wins, unless a program's would replace a
                    // limit.
                    if let Some(Err(waiting)) = self.machine.waiting.pop()
                        && waiting.limit()
                        && !e.limit()
                    {
                        e = waiting;
                    }
                }
                _ => e.trace.add(at),
            }
        }
        Err(e)
    }

    /// A popped `Cleanup` frame becomes a call of the cleanup above the `Finish` its install put
    /// under it, whose `waiting` entry, written in place, holds this ending. The call fills the
    /// room just freed: a refusal after the next check is the cleanup's first step failing.
    fn start_cleanup(
        &mut self,
        ending: Result<Value, Error>,
        cleanup: Value,
        context: Ref<Context>,
        at: Ref<Pair>,
    ) -> Result<Mode, Error> {
        if let Some(waiting) = self.machine.waiting.last_mut() {
            *waiting = ending;
        }
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
    fn handle(&mut self, e: Error, handler: Value, context: Ref<Context>) -> Result<Mode, Error> {
        let library = self.machine.library;
        let values = self.list_of(e.values.iter().copied(), library)?;
        let handed = e.trace.onto(|x, l| self.pair_local(x, l, library))?;
        let record = self.heap.make((None, Bindings::default()))?;
        let fields = [
            (K::Kind.into(), Value::Symbol(e.kind)),
            (K::Values.into(), values),
            (K::Trace.into(), handed.parts().0),
        ];
        self.heap.bind(record, fields)?;
        let operands = self.pair_local(Value::Context(record), Value::EmptyList, library)?;
        self.machine.handed = Some(handed);
        self.combine(handler, operands, context)
    }

    /// Every way into a run comes here first, as beginning one takes what this gives: as
    /// the run may need slots before reaching the machine loop, collect first if due, or after a
    /// peak, with its checked inputs as roots.
    pub(crate) fn prepare(
        &mut self,
        current: impl Iterator<Item = Value>,
        held: impl IntoIterator<Item = Ref>,
    ) -> Prepared {
        if self.heap.due_before_run() {
            self.collect(current, held);
        }
        Prepared(())
    }

    // Root iterators and marking temporaries belong only to collection.
    #[inline(never)]
    fn collect(
        &mut self,
        current: impl Iterator<Item = Value>,
        held: impl IntoIterator<Item = Ref>,
    ) {
        // Names are heap objects: host functions' are roots too.
        let roots = (self.machine.roots().chain(current))
            .chain(self.kept.iter().flat_map(Entry::roots))
            .chain([Value::Context(self.built_in), Value::Context(self.top)])
            .chain(self.hosts.iter().map(|&(name, _)| Value::Symbol(name)));
        self.heap.collect(roots, held.into_iter());
        self.tidy_kept();
        self.machine.tidy();
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
        let arguments = collected(given.iter().map(|&v| self.export(v)))?;
        let v = f(&mut HostCall::from(&mut *self), state, &arguments);
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

    /// The frames of a call of `function`, as [`Grift::call`] and [`Answer::Call`] make them, and
    /// of `then`, each in a new empty context of its own.
    /// The frame of an argument still to come is all a `then` needs: the saved values are the
    /// arguments so far and the call's value is the last, so the machine has no new kind of frame.
    fn call_on_machine(
        &mut self,
        function: Value,
        arguments: &[Value],
        then: Option<(Value, Vec<Value>)>,
    ) -> Result<Mode, Error> {
        let function = self.combiner_of(function)?;
        let operands = self.list_of(arguments.iter().copied(), false)?;
        let at = self.machine.here;
        if let Some((then, saved)) = then {
            let combiner = self.combiner_of(then)?;
            let context = self.heap.make((None, Bindings::default()))?;
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
        let context = self.heap.make((None, Bindings::default()))?;
        let base = Base::new(self.machine.arguments.len(), false);
        self.frame(What::Combine { operands, base }, context, at)?;
        Ok(Mode::Return(function))
    }

    /// Combines a combiner with unevaluated operands.
    fn combine(&mut self, f: Value, operands: Value, context: Ref<Context>) -> Result<Mode, Error> {
        match f {
            Value::Applicative(r) => {
                let mut inner = *self.heap.read(r)?;
                // With no operands, every layer of wrapping evaluates nothing: peel them here
                // rather than recursing once per layer.
                while let (Value::Applicative(r), Value::EmptyList) = (inner, operands) {
                    inner = *self.heap.read(r)?;
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
    fn spread(&mut self, f: Value, operands: Value, context: Ref<Context>) -> Result<Mode, Error> {
        let base = self.machine.arguments.len();
        let mut items = Items::new(operands);
        while let Some(x) = items.next(self)? {
            push(&mut self.machine.arguments, x)?;
        }
        if items.rest() != Value::EmptyList {
            return Err(Error::new(K::TypeError, &[f, operands]));
        }
        self.apply(f, context, base)
    }

    /// Evaluates the next operand of an applicative, or applies its combiner once none is left.
    fn next_argument(
        &mut self,
        combiner: Value,
        context: Ref<Context>,
        rest: Value,
        base: Base,
    ) -> Result<Mode, Error> {
        match rest {
            Value::Pair(r) => {
                #[expect(clippy::disallowed_methods, reason = "one operand per step")]
                let (x, rest, _) = self.heap.pair_at(r)?;
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
    fn apply(
        &mut self,
        combiner: Value,
        context: Ref<Context>,
        base: usize,
    ) -> Result<Mode, Error> {
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
        while self.machine.arguments.len() > base
            && let Some(x) = self.machine.arguments.pop()
        {
            list = self.pair_local(x, list, self.machine.library)?;
        }
        Ok(list)
    }

    /// Calls a compound operative: a child of the context it was made in, with the parameters
    /// bound to the operands and the context parameter to the caller's context.
    fn call_compound(
        &mut self,
        r: Ref<kind::Operative>,
        operands: Value,
        context: Ref<Context>,
    ) -> Result<Mode, Error> {
        let op = *self.heap.read(r)?;
        if let Some(native) = op.native
            && let Some(mode) = self.native(native, operands, context)?
        {
            return Ok(mode);
        }
        // The bindings are made first, with room for all of them, then the context.
        let mut bindings = Bindings::default();
        bindings.try_reserve(op.size as usize)?;
        self.match_tree(&mut bindings, op.parameters, operands)?;
        if let Some(s) = op.context_parameter {
            #[expect(
                clippy::disallowed_methods,
                reason = "reserved the operative's size above"
            )]
            bindings.insert(s, Value::Context(context));
        }
        let local = self.heap.make((Some(op.context), bindings))?;
        self.next_form(op.body, local, self.machine.here)
    }

    /// Runs a native twin on `operands`, as the library's operative would with them; `None` for
    /// a shape the twin leaves to the operative's body.
    fn native(
        &mut self,
        native: Native,
        operands: Value,
        context: Ref<Context>,
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
                let mut items = Items::new(operands);
                while items.next(self)?.is_some() {}
                if items.rest() != Value::EmptyList {
                    return Ok(None);
                }
                self.next_form(operands, context, at)?
            }
        };
        Ok(Some(mode))
    }

    #[inline(always)]
    #[expect(clippy::disallowed_methods, reason = "three items at most")]
    fn three(&self, list: Value) -> Result<Option<[Value; 3]>, Error> {
        let mut items = [Value::EmptyList; 3];
        let mut rest = list;
        for item in &mut items {
            let Value::Pair(r) = rest else {
                return Ok(None);
            };
            (*item, rest, _) = self.heap.pair_at(r)?;
        }
        Ok((rest == Value::EmptyList).then_some(items))
    }

    /// Flags the library's operatives that have a native twin.
    pub(crate) fn install_natives(&mut self) -> Result<(), Error> {
        for (native, name) in NATIVES {
            let found = self
                .heap
                .intern(name)
                .and_then(|s| self.value_of(self.built_in, s))?;
            let Value::Operative(r) = found else {
                return Err(type_error(found));
            };
            self.heap.install(r, Some(native))?;
        }
        Ok(())
    }

    /// Evaluates the first of `forms`, a proper list, the last as a tail call; none is `#inert`.
    fn next_form(
        &mut self,
        forms: Value,
        context: Ref<Context>,
        at: Ref<Pair>,
    ) -> Result<Mode, Error> {
        let Value::Pair(r) = forms else {
            return Ok(Mode::Return(Value::Inert));
        };
        #[expect(clippy::disallowed_methods, reason = "one form per step")]
        let (form, rest, _) = self.heap.pair_at(r)?;
        if rest != Value::EmptyList {
            self.frame(What::Body { rest }, context, at)?;
        }
        Ok(Mode::Evaluate(form, context))
    }

    /// The names `context` binds, sorted, as a list. Apart from [`Grift::primitive`], so that it
    /// doesn't change how the common primitives inline.
    #[inline(never)]
    fn names_list(&mut self, context: Ref<Context>) -> Result<Value, Error> {
        let names = self.names_of(context)?;
        self.list_of(names.into_iter().map(Value::Symbol), self.machine.library)
    }

    /// A new list of `items`, built from the last, its pairs the library's if `library`: charged
    /// like [`Items`], so a stop ends it. Every list Rust builds from values it holds is built here.
    fn list_of(
        &mut self,
        items: impl DoubleEndedIterator<Item = Value>,
        library: bool,
    ) -> Result<Value, Error> {
        let mut walked = 0;
        items.rev().try_fold(Value::EmptyList, |list, x| {
            self.control.walk(&mut walked)?;
            self.pair_local(x, list, library)
        })
    }

    /// The arguments from `base`, exactly as many as the arm's pattern names, or else the
    /// `arity-error`: each primitive's count is the pattern it takes them with.
    #[inline]
    fn arguments<const N: usize>(&mut self, base: usize) -> Result<[Value; N], Error> {
        let given = self.machine.arguments.get(base..).unwrap_or_default();
        <[Value; N]>::try_from(given).map_err(|_| self.arity_error(base))
    }

    fn primitive(
        &mut self,
        p: Primitive,
        context: Ref<Context>,
        base: usize,
    ) -> Result<Mode, Error> {
        use Primitive as P;
        let v = match p {
            P::Vau => {
                let given = self.machine.arguments.get(base..).unwrap_or_default();
                let Some(&[x, y]) = given.first_chunk() else {
                    return Err(self.arity_error(base));
                };
                let (size, context_parameter) = self.check_parameters(x, y)?;
                let body = self.arguments_list(base + 2)?;
                let op = Operative {
                    parameters: x,
                    context_parameter,
                    body,
                    context,
                    size: u32::try_from(size).map_err(|_| Error::oom())?,
                    native: None,
                };
                Value::Operative(self.heap.make(op)?)
            }
            P::Wrap => {
                let [x] = self.arguments(base)?;
                if !is_combiner(x) {
                    return Err(type_error(x));
                }
                Value::Applicative(self.heap.make(x)?)
            }
            P::Unwrap => match self.arguments(base)? {
                [Value::Applicative(r)] => *self.heap.read(r)?,
                [x] => return Err(type_error(x)),
            },
            P::Evaluate => {
                let [x, y] = self.arguments(base)?;
                return Ok(Mode::Evaluate(x, context_kind(y)?));
            }
            P::Context => {
                let parent = match self.arguments(base)? {
                    [Value::Context(c)] => Some(c),
                    [Value::EmptyList] => None,
                    [x] => return Err(type_error(x)),
                };
                Value::Context(self.heap.make((parent, Bindings::default()))?)
            }
            P::Bind => {
                let [x, y, z] = self.arguments(base)?;
                self.bind_tree(context_kind(x)?, y, z)?;
                Value::Inert
            }
            P::Unbind => match self.arguments(base)? {
                [Value::Context(c), Value::Symbol(s)] => Value::Boolean(self.heap.unbind(c, s)?),
                [Value::Context(_), y] | [y, _] => return Err(type_error(y)),
            },
            P::Lookup => match self.arguments(base)? {
                [Value::Context(c), Value::Symbol(s), z] => self.find(c, s)?.unwrap_or(z),
                [Value::Context(_), y, _] | [y, _, _] => return Err(type_error(y)),
            },
            P::Names => {
                let [x] = self.arguments(base)?;
                self.names_list(context_kind(x)?)?
            }
            P::Pair => {
                let [x, y] = self.arguments(base)?;
                self.pair_local(x, y, self.machine.library)?
            }
            P::First | P::Rest => match self.arguments(base)? {
                [Value::Pair(r)] => {
                    #[expect(clippy::disallowed_methods, reason = "one pair: first or rest")]
                    let (a, b, _) = self.heap.pair_at(r)?;
                    if p == P::First { a } else { b }
                }
                [x] => return Err(type_error(x)),
            },
            P::Same => {
                let [x, y] = self.arguments(base)?;
                Value::Boolean(self.same(x, y)?)
            }
            P::Select => match self.arguments(base)? {
                [Value::Boolean(true), y, _] => y,
                [Value::Boolean(false), _, z] => z,
                [x, _, _] => return Err(type_error(x)),
            },
            P::TypeOf => {
                let [x] = self.arguments(base)?;
                Value::Symbol(type_of(x).into())
            }
            P::Add | P::Subtract | P::Multiply | P::Divide | P::Remainder | P::LessThan => {
                let [x, y] = self.arguments(base)?;
                self.arithmetic(p, x, y)?
            }
            P::BitwiseAnd | P::BitwiseOr | P::BitwiseExclusiveOr | P::ShiftLeft | P::ShiftRight => {
                let [x, y] = self.arguments(base)?;
                bitwise(p, x, y)?
            }
            P::Power => match self.arguments(base)? {
                [x @ Value::Integer(_), y @ Value::Integer(b)] if b < 0 => {
                    return Err(Error::new(K::RangeError, &[x, y]));
                }
                [x @ Value::Integer(a), y @ Value::Integer(b)] => {
                    // Past u32's exponents only 0 and ±1 have powers that fit, and those the
                    // exponent's parity decides, as it does for the largest u32 of that parity.
                    let b = u32::try_from(b).unwrap_or(if b & 1 == 0 {
                        u32::MAX - 1
                    } else {
                        u32::MAX
                    });
                    let n = a.checked_pow(b);
                    Value::Integer(n.ok_or_else(|| Error::new(K::Overflow, &[x, y]))?)
                }
                [x, y] => Value::Float(float_of(x)?.powf(float_of(y)?)),
            },
            P::SquareRoot
            | P::Sine
            | P::Cosine
            | P::Tangent
            | P::Exponential
            | P::NaturalLogarithm
            | P::Floor
            | P::Ceiling
            | P::Round
            | P::Truncate => {
                let [x] = self.arguments(base)?;
                let f = float_of(x)?;
                Value::Float(match p {
                    P::SquareRoot => f.sqrt(),
                    P::Sine => f.sin(),
                    P::Cosine => f.cos(),
                    P::Tangent => f.tan(),
                    P::Exponential => f.exp(),
                    P::NaturalLogarithm => f.ln(),
                    P::Floor => f.floor(),
                    P::Ceiling => f.ceil(),
                    P::Round => f.round(),
                    _ => f.trunc(),
                })
            }
            P::AbsoluteValue => match self.arguments(base)? {
                [x @ Value::Integer(a)] => Value::Integer(
                    a.checked_abs()
                        .ok_or_else(|| Error::new(K::Overflow, &[x]))?,
                ),
                [x] => Value::Float(float_of(x)?.abs()),
            },
            P::IntegerToFloat => {
                let [x] = self.arguments(base)?;
                Value::Float(integer_of(x)? as f64)
            }
            // Rust's `as`: saturating, and not-a-number becomes 0.
            P::FloatToInteger => {
                let [x] = self.arguments(base)?;
                Value::Integer(float_of(x)? as i64)
            }
            P::EvaluateWithCleanup | P::EvaluateWithHandler => {
                let [x, y, z] = self.arguments(base)?;
                if !is_combiner(z) {
                    return Err(type_error(z));
                }
                let c = context_kind(y)?;
                let at = self.machine.here;
                if p == P::EvaluateWithHandler {
                    let base = Base::new(base, self.machine.library);
                    self.frame(What::Handler { handler: z, base }, c, at)?;
                } else {
                    // Under it go its `Finish` and that frame's entry, never one without the
                    // other: all the room comes first, and nothing after it can fail.
                    let m = &mut self.machine;
                    m.frames.try_reserve(2)?;
                    m.waiting.try_reserve(1)?;
                    let cleanup = What::Cleanup { cleanup: z, base };
                    let frames = [(What::Finish, Ref::NONE, Ref::NONE), (cleanup, c, at)];
                    #[expect(clippy::disallowed_methods, reason = "reserved all three above")]
                    {
                        m.waiting.push(Ok(Value::Inert));
                        for (what, context, at) in frames {
                            m.frames.push(Frame { what, context, at });
                        }
                    }
                }
                return Ok(Mode::Evaluate(x, c));
            }
            P::Raise => {
                let [x] = self.arguments(base)?;
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
            | P::BytesToList => self.sequence_primitive(p, base)?,
            P::CodePointToString => {
                let [x] = self.arguments(base)?;
                let c = u32::try_from(integer_of(x)?).ok().and_then(char::from_u32);
                let c = c.ok_or_else(|| Error::new(K::RangeError, &[x]))?;
                self.string_local(c.encode_utf8(&mut [0; 4]))?
            }
            P::StringToSymbol => {
                let [x] = self.arguments(base)?;
                let name = boxed(self.text(x)?)?;
                Value::Symbol(self.heap.intern(&name)?)
            }
            P::IntegerToString => {
                let [x] = self.arguments(base)?;
                integer_of(x)?;
                self.printed(x)?
            }
            P::ValueToString => {
                let [x] = self.arguments(base)?;
                self.printed(x)?
            }
            P::StringToInteger | P::StringToFloat => {
                let [x] = self.arguments(base)?;
                let s = self.text(x)?;
                let n = match p {
                    P::StringToInteger => s.parse().ok().map(Value::Integer),
                    _ => float_name(s).or_else(|| s.parse().ok()).map(Value::Float),
                };
                n.ok_or_else(|| Error::new(K::ParseError, &[x]))?
            }
            P::ReadForm => {
                let [x, y] = self.arguments(base)?;
                self.read_form_at(x, y)?
            }
        };
        Ok(Mode::Return(v))
    }

    /// `read-form`'s work: the form of string `x` after byte offset `y` and where it is. Apart from
    /// `primitive`, so its size costs the machine's loop nothing.
    #[inline(never)]
    fn read_form_at(&mut self, x: Value, y: Value) -> Result<Value, Error> {
        let Value::String(r) = x else {
            return Err(type_error(x));
        };
        let offset = usize::try_from(integer_of(y)?).ok();
        // The text is lent to the reader, not copied: a program reads a long text one
        // form at a time.
        let read = self.lend_text(r, |g, text| {
            offset
                .filter(|&i| text.is_char_boundary(i))
                .map(|i| g.read_form(text, i, 1, None, &mut 0, g.machine.library))
        })?;
        let read = read.ok_or_else(|| Error::new(K::RangeError, &[x, y]))?;
        Ok(match read? {
            Some((form, place, _)) => {
                let found = [form, integer(place.start), integer(place.end)];
                self.list_of(found.into_iter(), self.machine.library)?
            }
            None => Value::EmptyList,
        })
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
    fn sequence_primitive(&mut self, p: Primitive, base: usize) -> Result<Value, Error> {
        use Primitive as P;
        Ok(match p {
            P::ByteLength => {
                let [x] = self.arguments(base)?;
                integer(self.octets(x)?.len())
            }
            P::ByteAt => {
                let [x, y] = self.arguments(base)?;
                let (b, i) = (self.octets(x)?, integer_of(y)?);
                let byte = usize::try_from(i).ok().and_then(|i| b.get(i));
                let byte = byte.ok_or_else(|| Error::new(K::RangeError, &[x, y]))?;
                Value::Integer(i64::from(*byte))
            }
            P::Slice => {
                let [x, y, z] = self.arguments(base)?;
                self.octets(x)?;
                let range = (
                    usize::try_from(integer_of(y)?),
                    usize::try_from(integer_of(z)?),
                );
                let bad = || Error::new(K::RangeError, &[x, y, z]);
                let (Ok(a), Ok(b)) = range else {
                    return Err(bad());
                };
                match x {
                    Value::String(_) => {
                        let part = boxed(self.text(x)?.get(a..b).ok_or_else(bad)?)?;
                        Value::String(self.heap.make(part)?)
                    }
                    _ => {
                        let part = self.bytes_of(x)?.get(a..b).ok_or_else(bad)?;
                        let part = collected(part.iter().copied())?;
                        self.bytes_local(part)?
                    }
                }
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
                if bytes {
                    let mut joined = Vec::new();
                    joined.try_reserve_exact(len)?;
                    for &v in given {
                        #[expect(
                            clippy::disallowed_methods,
                            reason = "reserved the exact joined length above"
                        )]
                        joined.extend_from_slice(self.bytes_of(v)?);
                    }
                    self.bytes_local(joined)?
                } else {
                    let mut joined = String::new();
                    joined.try_reserve_exact(len)?;
                    for &v in given {
                        #[expect(
                            clippy::disallowed_methods,
                            reason = "reserved the exact joined length above"
                        )]
                        joined.push_str(self.text(v)?);
                    }
                    Value::String(self.heap.make(joined.into_boxed_str())?)
                }
            }
            P::Search => {
                let [x, y, z] = self.arguments(base)?;
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
                let [x] = self.arguments(base)?;
                let b = collected(self.text(x)?.bytes())?;
                self.bytes_local(b)?
            }
            P::BytesToString => {
                let [x] = self.arguments(base)?;
                let text = std::str::from_utf8(self.bytes_of(x)?).map_err(|e| {
                    let at = integer(e.valid_up_to());
                    Error::new(K::EncodingError, &[x, at])
                })?;
                let text = boxed(text)?;
                Value::String(self.heap.make(text)?)
            }
            P::ListToBytes => {
                let [x] = self.arguments(base)?;
                let mut bytes = Vec::new();
                let mut items = Items::new(x);
                while let Some(v) = items.next(self)? {
                    let at = integer(bytes.len());
                    let byte = match v {
                        Value::Integer(n) => {
                            u8::try_from(n).map_err(|_| Error::new(K::RangeError, &[v, at]))?
                        }
                        _ => return Err(Error::new(K::TypeError, &[v, at])),
                    };
                    push(&mut bytes, byte)?;
                }
                if items.rest() != Value::EmptyList {
                    let at = integer(bytes.len());
                    return Err(Error::new(K::TypeError, &[items.rest(), at]));
                }
                self.bytes_local(bytes)?
            }
            // `bytes-to-list`.
            _ => {
                let [x] = self.arguments(base)?;
                let b = collected(self.bytes_of(x)?.iter().copied())?;
                let items = b.into_iter().map(|byte| Value::Integer(i64::from(byte)));
                self.list_of(items, self.machine.library)?
            }
        })
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
        if let Some(handed) = self.machine.handed.take_if(|h| h.parts().0 == trace) {
            return Ok(Error::from_parts(kind, values, handed));
        }
        let mut given = self.items(trace, e)?;
        given.reverse();
        Ok(Error::from_parts(kind, values, Trace::given(given)?))
    }

    /// The elements of a proper list; anything else is a `type-error` naming `culprit`.
    pub(crate) fn items(&self, list: Value, culprit: Value) -> Result<Vec<Value>, Error> {
        let mut v = Vec::new();
        let mut items = Items::new(list);
        while let Some(x) = items.next(self)? {
            push(&mut v, x)?;
        }
        if items.rest() == Value::EmptyList {
            Ok(v)
        } else {
            Err(type_error(culprit))
        }
    }

    /// Whether `x` and `y` are the same value: equal integers, symbols, strings and bytes, floats
    /// with the same bits or both not-a-number (a not-a-number's bits are the machine's, not the
    /// program's), and otherwise the same object.
    fn same(&self, x: Value, y: Value) -> Result<bool, Error> {
        Ok(match (x, y) {
            (Value::String(_), Value::String(_)) => self.text(x)? == self.text(y)?,
            (Value::Bytes(_), Value::Bytes(_)) => self.bytes_of(x)? == self.bytes_of(y)?,
            (Value::Float(a), Value::Float(b)) => {
                a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
            }
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
            Value::String(r) => self.heap.read(r).map(|s| &**s),
            _ => Err(type_error(v)),
        }
    }

    /// The bytes of string or bytes value `v`, or a type error.
    fn octets(&self, v: Value) -> Result<&[u8], Error> {
        match v {
            Value::String(_) => self.text(v).map(str::as_bytes),
            _ => self.bytes_of(v),
        }
    }

    /// The contents of bytes value `v`, or a type error.
    pub(crate) fn bytes_of(&self, v: Value) -> Result<&[u8], Error> {
        match v {
            Value::Bytes(r) => self.heap.read(r).map(Vec::as_slice),
            _ => Err(type_error(v)),
        }
    }

    /// A new pair, the library's if `library`: what the machine makes is the library's if the
    /// combination being combined is, what the host makes is the program's, and the reader says
    /// whose text it reads.
    pub(crate) fn pair_local(&mut self, a: Value, b: Value, library: bool) -> Result<Value, Error> {
        let origin = Origin {
            place: None,
            library,
            outer: false,
        };
        Ok(Value::Pair(self.heap.make((a, b, origin))?))
    }

    pub(crate) fn string_local(&mut self, s: &str) -> Result<Value, Error> {
        let s = boxed(s)?;
        Ok(Value::String(self.heap.make(s)?))
    }

    pub(crate) fn bytes_local(&mut self, b: Vec<u8>) -> Result<Value, Error> {
        Ok(Value::Bytes(self.heap.make(b)?))
    }

    /// Checks a parameter tree by matching it to itself, and a context parameter, `#ignore` or a
    /// symbol not in the tree. The number of bindings a call makes, and the context parameter's
    /// symbol unless it is `#ignore`.
    fn check_parameters(
        &mut self,
        tree: Value,
        context_parameter: Value,
    ) -> Result<(usize, Option<Symbol>), Error> {
        let mut bound = Bindings::default();
        self.match_tree(&mut bound, tree, tree)?;
        match context_parameter {
            Value::Symbol(s) if !bound.contains_key(&s) => Ok((bound.len() + 1, Some(s))),
            Value::Ignore => Ok((bound.len(), None)),
            _ => Err(Error::new(K::ParameterError, &[context_parameter])),
        }
    }

    /// Matches a parameter tree to a value, binding its symbols in `bindings`, which it expects to
    /// have none of them: a symbol bound twice is a `parameter-error`, as is a leaf of the tree
    /// other than a symbol, `#ignore` and `()`.
    fn match_tree(
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
            result = self.control.walk(&mut walked).and_then(|()| match (t, v) {
                #[expect(clippy::disallowed_methods, reason = "reserved 1 in the arm")]
                (Value::Symbol(s), _) => match bindings.try_reserve(1) {
                    Ok(()) if bindings.insert(s, v).is_none() => Ok(()),
                    Ok(()) => Err(Error::new(K::ParameterError, &[tree])),
                    Err(_) => Err(Error::oom()),
                },
                (Value::Ignore, _) | (Value::EmptyList, Value::EmptyList) => Ok(()),
                #[expect(clippy::disallowed_methods, reason = "a tree walk charged above")]
                (Value::Pair(t), Value::Pair(v)) => self.heap.pair_at(t).and_then(|(ta, tb, _)| {
                    let (va, vb, _) = self.heap.pair_at(v)?;
                    push(&mut work, (tb, vb))?;
                    push(&mut work, (ta, va))
                }),
                (Value::Pair(_) | Value::EmptyList, _) => Err(Error::new(K::ArityError, &[value])),
                _ => Err(Error::new(K::ParameterError, &[tree])),
            });
        }
        self.machine.work = work;
        result
    }

    /// Matches parameter tree `tree` to `value`, binding its symbols in context `c` itself, all of
    /// them or, if it fails, none.
    pub(crate) fn bind_tree(
        &mut self,
        c: Ref<Context>,
        tree: Value,
        value: Value,
    ) -> Result<(), Error> {
        let mut matched = Bindings::default();
        self.match_tree(&mut matched, tree, value)?;
        self.heap.bind(c, matched)
    }

    fn value_of(&self, context: Ref<Context>, s: Symbol) -> Result<Value, Error> {
        self.find(context, s)?
            .ok_or_else(|| Error::new(K::UnboundSymbol, &[Value::Symbol(s)]))
    }

    /// The binding of `s` in `context` or the nearest parent that has one.
    pub(crate) fn find(
        &self,
        mut context: Ref<Context>,
        s: Symbol,
    ) -> Result<Option<Value>, Error> {
        loop {
            let (parent, bindings) = self.heap.read(context)?;
            if let Some(&v) = bindings.get(&s) {
                return Ok(Some(v));
            }
            let Some(parent) = parent else {
                return Ok(None);
            };
            context = *parent;
        }
    }

    /// The symbols bound in `context` itself, sorted by name.
    pub(crate) fn names_of(&self, context: Ref<Context>) -> Result<Vec<Symbol>, Error> {
        let (_, bindings) = self.heap.read(context)?;
        let mut names = collected(bindings.keys().copied())?;
        names.sort_unstable_by(|&a, &b| self.name(a).cmp(self.name(b)));
        Ok(names)
    }

    /// Runs primitive `p` on `args` in `context` with no machine step around it, for the tests.
    #[cfg(test)]
    pub(crate) fn call_primitive(
        &mut self,
        p: Primitive,
        args: &[Value],
        context: Ref<Context>,
    ) -> Result<Mode, Error> {
        let base = self.machine.arguments.len();
        for &a in args {
            push(&mut self.machine.arguments, a)?;
        }
        self.apply(Value::Primitive(p), context, base)
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
/// a stop reaches it in every phase through [`Control::walk`]. Strings search through it too.
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
            self.control.walk(&mut walked)?;
            held |= 1 << (b & 63);
        }
        let (mut at, mut memory) = (0, 0);
        while let Some(window) = text.get(at..at + n) {
            if window.last().is_some_and(|b| held >> (b & 63) & 1 == 0) {
                self.control.walk(&mut walked)?;
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
            self.control.walk(walked)?;
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
            self.control.walk(walked)?;
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
