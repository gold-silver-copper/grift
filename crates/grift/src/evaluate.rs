//! The evaluator: an explicit machine loop over a frame stack, and the Rust primitives.
//!
//! The machine's state is its frame stack, its argument stack, the errors waiting for a
//! cleanup to finish, where it is in the program and the current [`Mode`]. The collector runs
//! at the top of the loop and before a host's evaluation or call needs its first slot, with that
//! state, the root contexts and the new call's checked inputs as its roots. Allocation never
//! collects: Rust locals stay valid until the next run entry or machine safe point. The loop's
//! safe point also checks for a stop.
//!
//! Three library forms have a native twin: `if!`, `sequence!` and `string-search` are the
//! library's own operatives, flagged, and run in Rust when their operands have the shape the twin
//! handles; any other shape runs the operative's body, so what a program sees never differs.

use crate::Grift;
use crate::collected;
use crate::heap::{Bindings, Object, Operative, Reference, Seen};
use crate::interrupt::Control;
use crate::local::{Error, Host, K, Symbol, Value, boxed, push};
use crate::read::float_name;
use std::sync::Arc;

/// Declares the primitives: each one's variant, its name in the built-in context, and the
/// fewest and most operands it takes. The core ones are what grift can't express; each native
/// one exists for speed only, and its meaning is its grift definition in `natives.grift`.
macro_rules! primitives {
    (core { $($p:ident $name:literal $lo:literal $hi:expr;)* }
     native { $($q:ident $qname:literal $qlo:literal $qhi:expr;)* }) => {
        /// The Rust primitives. All but `vau!` are bound wrapped, as applicatives.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Primitive { $($p,)* $($q),* }

        impl Primitive {
            pub(crate) const ALL: &[Primitive] = &[$(Primitive::$p,)* $(Primitive::$q),*];

            /// The natives, each with a grift definition.
            #[cfg(test)]
            pub(crate) const NATIVE: &[Primitive] = &[$(Primitive::$q),*];

            /// The name the primitive is bound to in the built-in context.
            pub fn name(self) -> &'static str {
                match self { $(Primitive::$p => $name,)* $(Primitive::$q => $qname),* }
            }

            /// The fewest and most operands.
            fn arity(self) -> (usize, usize) {
                match self { $(Primitive::$p => ($lo, $hi),)* $(Primitive::$q => ($qlo, $qhi)),* }
            }
        }
    };
}

primitives! {
core {
    // Abstraction, wrapping and evaluation: the machine itself.
    Vau "vau!" 3 3;
    Wrap "wrap" 1 1;
    Unwrap "unwrap" 1 1;
    Evaluate "evaluate" 2 2;
    // Unwinding: machine frames.
    EvaluateWithCleanup "evaluate-with-cleanup" 3 3;
    CallWithHandler "call-with-handler" 2 2;
    Raise "raise" 1 1;
    // Making, changing and listing bindings.
    Context "context" 0 1;
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
    // A string's bytes, making text, names and floats from it, the reader and the printer.
    StringByte "string-byte" 2 2;
    StringAppend "string-append" 0 usize::MAX;
    StringToSymbol "string-to-symbol" 1 1;
    StringToFloat "string-to-float" 1 1;
    ReadForm "read-form" 2 2;
    ValueToString "value-to-string" 1 1;
}
native {
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
    StringByteLength "string-byte-length" 1 1;
    ByteSubstring "byte-substring" 3 3;
    CodePointToString "code-point-to-string" 1 1;
    IntegerToString "integer-to-string" 1 1;
    StringToInteger "string-to-integer" 1 1;
    // A bytes value's contents: the printer writes them and the reader reads them back.
    BytesLength "bytes-length" 1 1;
    BytesByte "bytes-byte" 2 2;
    BytesSlice "bytes-slice" 3 3;
    BytesAppend "bytes-append" 0 usize::MAX;
    StringToBytes "string-to-bytes" 1 1;
    BytesToString "bytes-to-string" 1 1;
    ListToBytes "list-to-bytes" 1 1;
    BytesToList "bytes-to-list" 1 1;
}
}

/// The library forms with a native twin.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Native {
    If,
    Sequence,
    StringSearch,
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

#[derive(Clone, Copy)]
enum What {
    /// Evaluate the remaining forms of one `evaluate` call; with none left, pass the value on.
    Forms { rest: Value },
    /// The value is the combiner of a combination with these operands.
    Combine { operands: Value },
    /// The value is an argument of `combiner`; arguments so far are on the argument stack from
    /// `base`.
    Arguments {
        combiner: Value,
        rest: Value,
        base: usize,
    },
    /// When the evaluation above ends, by a value or an error, call the combiner `cleanup` with
    /// no arguments; `base` is the argument stack's height to go back to.
    Cleanup { cleanup: Value, base: usize },
    /// When an error leaves the evaluation above, call `handler` with it as a value instead;
    /// `base` is the argument stack's height to go back to. A value passes through.
    Handler { handler: Value, base: usize },
    /// The cleanup above ran after this value: deliver it.
    Deliver(Value),
    /// The cleanup above ran after an error, the last one waiting: raise it again.
    Raise,
    /// Native `if!`: the value is the test; evaluate one of these as a tail call.
    Branch {
        consequent: Value,
        alternative: Value,
    },
    /// Native `sequence!`: the forms left after the one evaluated, at least one.
    Body { rest: Value },
    /// A named outer form evaluated by grift: keep it as the boundary through its tail calls.
    Form,
}

impl Frame {
    fn values(self) -> [Value; 4] {
        let empty = Value::EmptyList;
        let (a, b) = match self.what {
            What::Forms { rest: v }
            | What::Combine { operands: v }
            | What::Cleanup { cleanup: v, .. }
            | What::Handler { handler: v, .. }
            | What::Deliver(v)
            | What::Body { rest: v } => (v, empty),
            What::Arguments { combiner, rest, .. } => (combiner, rest),
            What::Branch {
                consequent,
                alternative,
            } => (consequent, alternative),
            What::Raise | What::Form => (empty, empty),
        };
        [a, b, Value::Context(self.context), Value::Pair(self.at)]
    }
}

pub(crate) struct Machine {
    frames: Vec<Frame>,
    arguments: Vec<Value>,
    /// Errors waiting for a cleanup to finish, one per `Raise` frame.
    raised: Vec<Error>,
    /// The innermost combination of the program being evaluated, or [`Reference::NONE`]. A
    /// `Reference`, not a `Value`: this is written and read at every step.
    here: Reference,
    /// Scratch for matching parameter trees.
    work: Vec<(Value, Value)>,
    /// Steps taken, for tests of what a form costs.
    #[cfg(test)]
    pub steps: u64,
}

#[cfg(test)]
impl Machine {
    /// The most frames there were at once.
    pub fn frames_high(&self) -> usize {
        self.frames.capacity()
    }
}

impl Default for Machine {
    fn default() -> Machine {
        Machine {
            frames: Vec::new(),
            arguments: Vec::new(),
            raised: Vec::new(),
            here: Reference::NONE,
            work: Vec::new(),
            #[cfg(test)]
            steps: 0,
        }
    }
}

/// Held while a host function runs: if it doesn't return to the interpreter it was called from,
/// by a panic or by replacing it, that interpreter's run ends.
struct Hosting(Option<Arc<Control>>);

impl Drop for Hosting {
    fn drop(&mut self) {
        if let Some(c) = &self.0 {
            c.end();
        }
    }
}

pub(crate) fn type_error(p: Primitive, v: Value) -> Error {
    Error::new(K::TypeError, &[Value::Primitive(p), v])
}

impl<T> Grift<T> {
    /// Evaluates the list `forms` in `context`, in order. Reentrant: a host function may call
    /// `evaluate`, which runs on top of the current frames and leaves them as it found them.
    pub(crate) fn evaluate_forms(
        &mut self,
        state: Option<&mut T>,
        forms: Value,
        context: Reference,
    ) -> Result<Value, Error> {
        let top = What::Forms { rest: forms };
        self.start(state, &[(top, context)], Mode::Return(Value::Inert))
    }

    /// [`Grift::call`], on this interpreter's values.
    pub(crate) fn call_local(
        &mut self,
        state: Option<&mut T>,
        f: Value,
        arguments: &[Value],
    ) -> Result<Value, Error> {
        self.prepare(std::iter::once(f).chain(arguments.iter().copied()))?;
        let f = match f {
            Value::Applicative(r) => self.unwrapped(r)?,
            _ => f,
        };
        let operands = self.list_local(arguments)?;
        let context = self
            .heap
            .alloc(Object::Context(None, Bindings::default()))?;
        let call = What::Combine { operands };
        self.start(state, &[(call, context)], Mode::Return(f))
    }

    /// Runs the machine on top of the current frames from `mode`, with `frames` pushed, until
    /// they are all gone; then everything is as it was, whatever the outcome. The first frame
    /// pushed holds where the machine was, so the collector sees it until it is restored. An
    /// outer run gets its generation, for stops.
    fn start(
        &mut self,
        state: Option<&mut T>,
        frames: &[(What, Reference)],
        mode: Mode,
    ) -> Result<Value, Error> {
        let outer = self.begin_run()?;
        // A host function may replace the interpreter or abandon this run: then the machine
        // state is gone and nothing is restored into another's.
        let (owner, run) = (Arc::as_ptr(&self.control), self.run);
        let m = &mut self.machine;
        let (base, arguments, raised, here) =
            (m.frames.len(), m.arguments.len(), m.raised.len(), m.here);
        let held = What::Deliver(Value::EmptyList);
        let result = self.frame(held, Reference::NONE, here).and_then(|()| {
            frames
                .iter()
                .try_for_each(|&(what, context)| self.frame(what, context, Reference::NONE))
        });
        let result = result.and_then(|()| self.run(state, mode, base + 1));
        if !std::ptr::eq(owner, Arc::as_ptr(&self.control)) || self.run != run {
            return result;
        }
        let m = &mut self.machine;
        m.frames.truncate(base);
        m.arguments.truncate(arguments);
        m.raised.truncate(raised);
        m.here = here;
        if outer {
            self.end_run();
        }
        result
    }

    fn frame(&mut self, what: What, context: Reference, at: Reference) -> Result<(), Error> {
        push(&mut self.machine.frames, Frame { what, context, at })
    }

    fn run(
        &mut self,
        mut state: Option<&mut T>,
        mut mode: Mode,
        floor: usize,
    ) -> Result<Value, Error> {
        loop {
            #[cfg(test)]
            {
                self.machine.steps += 1;
            }
            if let Mode::Return(v) = mode
                && self.machine.frames.len() <= floor
            {
                // The last step may have been a long native or host call, with no safe point
                // after it: a stop that came during it stops the run here.
                if !self.interrupted() {
                    return Ok(v);
                }
                mode = self.unwind(Error::new(K::Interrupted, &[]), floor)?;
                continue;
            }
            let next = self.safe_point(mode).and_then(|()| match mode {
                Mode::Host(h, base) => self.host(state.as_deref_mut(), h, base),
                _ => self.step(mode),
            });
            mode = match next {
                Ok(mode) => mode,
                // So too a stop that came during a step that failed, even with a limit: the
                // stop is that failure, and the cleanups then run.
                Err(_) if self.interrupted() => {
                    self.unwind(Error::new(K::Interrupted, &[]), floor)?
                }
                Err(e) => self.unwind(e, floor)?,
            };
        }
    }

    /// The safe point: the machine's state is all there is, so the collector may run, and
    /// stopping here leaves nothing half done.
    #[inline(always)]
    fn safe_point(&mut self, mode: Mode) -> Result<(), Error> {
        if self.interrupted() {
            return Err(Error::new(K::Interrupted, &[]));
        }
        if self.heap.due() {
            let current = match mode {
                Mode::Evaluate(x, context) => [x, Value::Context(context)],
                Mode::Return(v) => [v, Value::EmptyList],
                Mode::Host(..) => [Value::EmptyList, Value::EmptyList],
            };
            self.collect(current.into_iter())?;
        }
        Ok(())
    }

    #[inline(always)]
    fn step(&mut self, mode: Mode) -> Result<Mode, Error> {
        Ok(match mode {
            Mode::Evaluate(Value::Symbol(s), context) => Mode::Return(self.value_of(context, s)?),
            Mode::Evaluate(Value::Pair(r), context) => {
                let (combiner, operands, library) = match self.heap.get_code(r)? {
                    (Object::Pair(a, b), library) => (*a, *b, library),
                    _ => return Err(Error::new(K::StaleValue, &[Value::Pair(r)])),
                };
                // A combination is the program's unless it is the library's code, or code built
                // around a combiner value (as the library builds it), not written.
                if !library && !is_combiner(combiner) {
                    self.machine.here = r;
                }
                self.frame(What::Combine { operands }, context, self.machine.here)?;
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
                            Value::Pair(f) => f,
                            _ => Reference::NONE,
                        };
                        self.frame(What::Forms { rest }, context, at)?;
                        self.machine.here = Reference::NONE;
                        Mode::Evaluate(form, context)
                    }
                    What::Forms { .. } | What::Form => Mode::Return(v),
                    What::Combine { operands } => {
                        self.machine.here = at;
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
                        self.frame(What::Deliver(v), Reference::NONE, Reference::NONE)?;
                        self.call_thunk(cleanup, context, at)?
                    }
                    What::Handler { .. } => {
                        self.machine.here = at;
                        Mode::Return(v)
                    }
                    What::Deliver(v) => Mode::Return(v),
                    What::Branch {
                        consequent,
                        alternative,
                    } => {
                        self.machine.here = at;
                        match v {
                            Value::Boolean(true) => Mode::Evaluate(consequent, context),
                            Value::Boolean(false) => Mode::Evaluate(alternative, context),
                            _ => return Err(type_error(Primitive::Select, v)),
                        }
                    }
                    What::Body { rest } => {
                        self.machine.here = at;
                        self.next_form(rest, context, at)?
                    }
                    What::Raise => {
                        // The error already says where it happened, or that it can't: not here.
                        self.machine.here = Reference::NONE;
                        return Err(self.machine.raised.pop().unwrap_or_else(Error::oom));
                    }
                }
            }
            Mode::Host(..) => return Err(Error::new(K::NotACombiner, &[])),
        })
    }

    /// An error stops the evaluation at `here`. Frames above `floor` are dropped down to the
    /// innermost cleanup or handler, adding where each was to the error's trace. A cleanup then
    /// runs, the error waiting until it is done; a handler is called with the error instead.
    /// With neither, the error goes to the caller. An error in a cleanup replaces the one
    /// waiting.
    fn unwind(&mut self, mut e: Error, floor: usize) -> Result<Mode, Error> {
        locate(&mut e, self.machine.here);
        while self.machine.frames.len() > floor {
            let Some(Frame { what, context, at }) = self.machine.frames.pop() else {
                break;
            };
            match what {
                What::Cleanup { cleanup, base } => {
                    locate(&mut e, at);
                    self.machine.arguments.truncate(base);
                    self.machine.here = at;
                    push(&mut self.machine.raised, e)?;
                    self.frame(What::Raise, Reference::NONE, Reference::NONE)?;
                    return self.call_thunk(cleanup, context, at);
                }
                What::Handler { handler, base } if !limit(e.kind) => {
                    self.machine.arguments.truncate(base);
                    self.machine.here = at;
                    match self.handle(&e, handler, context) {
                        Ok(mode) => return Ok(mode),
                        // Calling it failed, here: that error goes on instead.
                        Err(failed) => {
                            e = failed;
                            locate(&mut e, at);
                        }
                    }
                }
                // The cleanup's error replaces the one waiting, unless that one is a limit.
                What::Raise => match self.machine.raised.pop() {
                    Some(waiting) if limit(waiting.kind) && !limit(e.kind) => e = waiting,
                    _ => {}
                },
                _ => locate(&mut e, at),
            }
        }
        Err(e)
    }

    /// Calls a handler with the error as a new context, with no parent, binding `kind`,
    /// `values` and `trace`.
    fn handle(&mut self, e: &Error, handler: Value, context: Reference) -> Result<Mode, Error> {
        let (values, trace) = (self.list_local(&e.values)?, self.list_local(&e.trace)?);
        let mut fields = Bindings::default();
        fields.try_reserve(3).map_err(|_| Error::oom())?;
        fields.insert(K::Kind.into(), Value::Symbol(e.kind));
        fields.insert(K::Values.into(), values);
        fields.insert(K::Trace.into(), trace);
        let v = Value::Context(self.heap.alloc(Object::Context(None, fields))?);
        let operands = self.list_local(&[v])?;
        self.combine(handler, operands, context)
    }

    /// A host's next run may need slots before reaching the machine loop. Collect here first,
    /// with its checked inputs and the suspended machine as roots; never during allocation.
    pub(crate) fn prepare(&mut self, current: impl Iterator<Item = Value>) -> Result<(), Error> {
        if self.run != 0 && self.control.active() != self.run {
            self.abandon();
        }
        if self.heap.due() {
            self.collect(current)?;
        }
        Ok(())
    }

    fn collect(&mut self, current: impl Iterator<Item = Value>) -> Result<(), Error> {
        let m = &self.machine;
        let raised = m
            .raised
            .iter()
            .flat_map(|e| e.values.iter().chain(&e.trace).copied());
        let roots = m
            .frames
            .iter()
            .flat_map(|f| f.values())
            .chain(m.arguments.iter().copied())
            .chain(raised);
        let roots = roots.chain(current).chain([
            Value::Pair(m.here),
            Value::Context(self.built_in),
            Value::Context(self.top),
            Value::Context(self.kept),
            #[cfg(test)]
            Value::Context(self.definitions),
        ]);
        self.heap.collect(roots)
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
        let mut arguments = Vec::new();
        arguments
            .try_reserve_exact(given.len())
            .map_err(|_| Error::oom())?;
        arguments.extend(given.iter().map(|&v| self.export(v)));
        let run = self.run;
        let mut hosting = Hosting(Some(Arc::clone(&self.control)));
        let v = f(self, state, &arguments);
        // Replaced, or restored after a run inside was abandoned: this run is over, and its
        // control says so (as it does if `f` panics).
        let same = hosting
            .0
            .as_ref()
            .is_some_and(|c| Arc::ptr_eq(c, &self.control) && self.run == run && c.active() == run);
        if !same {
            self.abandon();
            return Err(Error::new(K::ForeignValue, &[]));
        }
        hosting.0 = None;
        self.machine.arguments.truncate(base);
        // What comes back is checked as everything the host hands in is.
        match v {
            Ok(v) => Ok(Mode::Return(self.import(&v)?)),
            Err(e) => Err(self.import_error(&e).unwrap_or_else(|failed| failed)),
        }
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
                let base = self.machine.arguments.len();
                self.next_argument(inner, context, operands, base)
            }
            Value::Operative(r) => self.call_compound(r, operands, context),
            Value::Primitive(_) | Value::Host(_) => {
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
            _ => Err(Error::new(K::NotACombiner, &[f])),
        }
    }

    /// Evaluates the next operand of an applicative, or applies its combiner once none is left.
    fn next_argument(
        &mut self,
        combiner: Value,
        context: Reference,
        rest: Value,
        base: usize,
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
            Value::EmptyList => self.apply(combiner, context, base),
            _ => Err(Error::new(K::TypeError, &[combiner, rest])),
        }
    }

    /// Applies a combiner to the arguments on the argument stack from `base`, popping them.
    fn apply(&mut self, combiner: Value, context: Reference, base: usize) -> Result<Mode, Error> {
        match combiner {
            #[cfg(test)]
            Value::Primitive(p) if !self.natives && self.definition(p)?.is_some() => {
                let list = self.arguments_list(base)?;
                let definition = self.definition(p)?.unwrap_or(combiner);
                self.combine(definition, list, context)
            }
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
        let stale = || Error::new(K::StaleValue, &[Value::Operative(r)]);
        let op = match self.heap.get(r)? {
            Object::Operative(op) => match op.native {
                Some(native) if self.natives_on() => Err(native),
                _ => Ok(*op),
            },
            _ => return Err(stale()),
        };
        let op = match op {
            Ok(op) => op,
            Err(native) => match self.native(native, operands, context)? {
                Some(mode) => return Ok(mode),
                None => match self.heap.get(r)? {
                    Object::Operative(op) => *op,
                    _ => return Err(stale()),
                },
            },
        };
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
        Ok(Mode::Evaluate(op.body, local))
    }

    /// Runs a native twin on `operands`, as the library's operative would with them; `None` for
    /// a shape it doesn't handle, which the operative's body then runs, errors and all.
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
                match (operands, rest) {
                    (Value::EmptyList, _) => Mode::Return(Value::Inert),
                    (_, Value::EmptyList) => self.next_form(operands, context, at)?,
                    _ => return Ok(None),
                }
            }
            Native::StringSearch => {
                let Some([Value::String(t), Value::String(p), Value::Integer(i)]) =
                    self.three(operands)?
                else {
                    return Ok(None);
                };
                let (Object::String(t), Object::String(p)) = (self.heap.get(t)?, self.heap.get(p)?)
                else {
                    return Ok(None);
                };
                // A start out of range or off a character boundary is the body's range-error.
                let Some((start, after)) =
                    usize::try_from(i).ok().and_then(|i| Some((i, t.get(i..)?)))
                else {
                    return Ok(None);
                };
                let found = after.find(&**p).map(|at| integer(start + at));
                Mode::Return(found.unwrap_or(Value::EmptyList))
            }
        };
        Ok(Some(mode))
    }

    /// The elements of a list of exactly three; `None` for anything else.
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

    /// Evaluates the first of `forms`, a non-empty proper list; the last one as a tail call.
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

    /// Whether native twins and native primitives run; tests turn them off, to run their grift
    /// definitions instead and compare.
    #[inline(always)]
    fn natives_on(&self) -> bool {
        #[cfg(test)]
        return self.natives;
        #[cfg(not(test))]
        true
    }

    /// Flags the library's operatives that have a native twin.
    pub(crate) fn install_natives(&mut self) -> Result<(), Error> {
        let twins = [
            ("if!", Native::If),
            ("sequence!", Native::Sequence),
            ("string-search", Native::StringSearch),
        ];
        for (name, native) in twins {
            let found = self
                .intern(name)
                .and_then(|s| self.find(self.built_in, s))?;
            let found = match found {
                Some(Value::Applicative(r)) => self.unwrapped(r)?,
                Some(v) => v,
                None => Value::Inert,
            };
            let Value::Operative(r) = found else {
                return Err(Error::new(K::TypeError, &[found]));
            };
            if let Object::Operative(op) = self.heap.get_mut(r)? {
                op.native = Some(native);
            }
        }
        Ok(())
    }

    fn primitive(&mut self, p: Primitive, context: Reference, base: usize) -> Result<Mode, Error> {
        use Primitive as P;
        let given = self.machine.arguments.get(base..).unwrap_or_default();
        let (lo, hi) = p.arity();
        if given.len() < lo || given.len() > hi {
            let mut e = Error::new(K::ArityError, given);
            push(&mut e.values, Value::Primitive(p))?;
            e.values.rotate_right(1);
            return Err(e);
        }
        let n = given.len();
        let mut a = [Value::EmptyList; 3];
        a.iter_mut().zip(given).for_each(|(d, s)| *d = *s);
        let [x, y, z] = a;
        let v = match p {
            P::Vau => {
                let size = self.check_parameters(x, y)?;
                let op = Operative {
                    parameters: x,
                    context_parameter: y,
                    body: z,
                    context,
                    size: u32::try_from(size).map_err(|_| Error::oom())?,
                    native: None,
                };
                Value::Operative(self.heap.alloc(Object::Operative(op))?)
            }
            P::Wrap if !is_combiner(x) => return Err(type_error(p, x)),
            P::Wrap => Value::Applicative(self.heap.alloc(Object::Applicative(x))?),
            P::Unwrap => match x {
                Value::Applicative(r) => self.unwrapped(r)?,
                _ => return Err(type_error(p, x)),
            },
            P::Evaluate => match y {
                Value::Context(c) => {
                    if let Value::Pair(at) = x
                        && self
                            .positions
                            .get(&at.idx)
                            .is_some_and(|p| p.generation == at.generation && p.outer)
                        && !self.heap.get_code(at)?.1
                        && !self
                            .machine
                            .frames
                            .last()
                            .is_some_and(|f| matches!(f.what, What::Form))
                    {
                        self.frame(What::Form, Reference::NONE, at)?;
                    }
                    return Ok(Mode::Evaluate(x, c));
                }
                _ => return Err(type_error(p, y)),
            },
            P::Context => {
                let parent = match x {
                    Value::Context(c) if n == 1 => Some(c),
                    _ if n == 0 => None,
                    _ => return Err(type_error(p, x)),
                };
                Value::Context(
                    self.heap
                        .alloc(Object::Context(parent, Bindings::default()))?,
                )
            }
            P::Bind => match x {
                Value::Context(c) => {
                    self.bind_tree(c, y, z)?;
                    Value::Inert
                }
                _ => return Err(type_error(p, x)),
            },
            P::Unbind | P::Lookup => {
                let (Value::Context(c), Value::Symbol(s)) = (x, y) else {
                    let bad = if matches!(x, Value::Context(_)) { y } else { x };
                    return Err(type_error(p, bad));
                };
                match p {
                    P::Unbind => Value::Boolean(self.bindings_mut(c)?.remove(&s).is_some()),
                    _ => self.find(c, s)?.unwrap_or(z),
                }
            }
            P::Names => match x {
                Value::Context(c) => {
                    let names = self.names_of(c)?;
                    self.list_local(&names)?
                }
                _ => return Err(type_error(p, x)),
            },
            P::Pair => self.pair_local(x, y)?,
            P::First | P::Rest => match x {
                Value::Pair(r) => {
                    let (a, b) = self.pair_at(r)?;
                    if p == P::First { a } else { b }
                }
                _ => return Err(type_error(p, x)),
            },
            P::Same => Value::Boolean(self.same(x, y)?),
            P::Select => match x {
                Value::Boolean(true) => y,
                Value::Boolean(false) => z,
                _ => return Err(type_error(p, x)),
            },
            P::TypeOf => Value::Symbol(type_of(x).into()),
            P::Add | P::Subtract | P::Multiply | P::Divide | P::Remainder | P::LessThan => {
                arithmetic(p, x, y)?
            }
            P::BitwiseAnd | P::BitwiseOr | P::BitwiseExclusiveOr | P::ShiftLeft | P::ShiftRight => {
                bitwise(p, x, y)?
            }
            P::Power => Value::Float(float_of(p, x)?.powf(float_of(p, y)?)),
            P::SquareRoot => Value::Float(float_of(p, x)?.sqrt()),
            P::Sine => Value::Float(float_of(p, x)?.sin()),
            P::Cosine => Value::Float(float_of(p, x)?.cos()),
            P::Tangent => Value::Float(float_of(p, x)?.tan()),
            P::Exponential => Value::Float(float_of(p, x)?.exp()),
            P::NaturalLogarithm => Value::Float(float_of(p, x)?.ln()),
            P::Floor => Value::Float(float_of(p, x)?.floor()),
            P::Ceiling => Value::Float(float_of(p, x)?.ceil()),
            P::Round => Value::Float(float_of(p, x)?.round()),
            P::Truncate => Value::Float(float_of(p, x)?.trunc()),
            P::AbsoluteValue => Value::Float(float_of(p, x)?.abs()),
            P::IntegerToFloat => Value::Float(integer_of(p, x)? as f64),
            // Rust's `as`: saturating, and not-a-number becomes 0.
            P::FloatToInteger => Value::Integer(float_of(p, x)? as i64),
            P::EvaluateWithCleanup if !is_combiner(z) => return Err(type_error(p, z)),
            P::EvaluateWithCleanup => match y {
                Value::Context(c) => {
                    let cleanup = What::Cleanup { cleanup: z, base };
                    self.frame(cleanup, c, self.machine.here)?;
                    return Ok(Mode::Evaluate(x, c));
                }
                _ => return Err(type_error(p, y)),
            },
            P::CallWithHandler => {
                if let Some(&v) = [x, y].iter().find(|&&v| !is_combiner(v)) {
                    return Err(type_error(p, v));
                }
                let at = self.machine.here;
                self.frame(What::Handler { handler: y, base }, context, at)?;
                return self.call_thunk(x, context, at);
            }
            P::Raise => {
                let e = self.error_of(x)?;
                // A new error happens here; a caught one keeps where it happened, and the raise
                // itself isn't one of its locations.
                if !e.trace.is_empty() {
                    self.machine.here = Reference::NONE;
                }
                return Err(e);
            }
            P::StringByteLength => integer(self.text(p, x)?.len()),
            P::StringByte => {
                let (s, i) = (self.text(p, x)?, integer_of(p, y)?);
                let byte = usize::try_from(i).ok().and_then(|i| s.as_bytes().get(i));
                let byte =
                    byte.ok_or_else(|| Error::new(K::RangeError, &[Value::Primitive(p), x, y]))?;
                Value::Integer(i64::from(*byte))
            }
            P::ByteSubstring => {
                let s = self.text(p, x)?;
                let range = (
                    usize::try_from(integer_of(p, y)?),
                    usize::try_from(integer_of(p, z)?),
                );
                let part = match range {
                    (Ok(a), Ok(b)) => s.get(a..b),
                    _ => None,
                };
                let part =
                    part.ok_or_else(|| Error::new(K::RangeError, &[Value::Primitive(p), x, y, z]))?;
                let part = boxed(part)?;
                Value::String(self.heap.alloc(Object::String(part))?)
            }
            P::StringAppend => {
                let given = self.machine.arguments.get(base..).unwrap_or_default();
                let mut len = 0usize;
                for &v in given {
                    len = len.saturating_add(self.text(p, v)?.len());
                }
                // Exactly the length, so boxing it never reallocates.
                let mut s = String::new();
                s.try_reserve_exact(len).map_err(|_| Error::oom())?;
                for &v in given {
                    s.push_str(self.text(p, v)?);
                }
                Value::String(self.heap.alloc(Object::String(s.into_boxed_str()))?)
            }
            P::BytesLength
            | P::BytesByte
            | P::BytesSlice
            | P::BytesAppend
            | P::StringToBytes
            | P::BytesToString
            | P::ListToBytes
            | P::BytesToList => self.bytes_primitive(p, [x, y, z], base)?,
            P::CodePointToString => {
                let c = u32::try_from(integer_of(p, x)?)
                    .ok()
                    .and_then(char::from_u32);
                let c = c.ok_or_else(|| Error::new(K::RangeError, &[Value::Primitive(p), x]))?;
                self.string_local(c.encode_utf8(&mut [0; 4]))?
            }
            P::StringToSymbol => {
                let name = boxed(self.text(p, x)?)?;
                Value::Symbol(self.intern(&name)?)
            }
            P::IntegerToString => {
                integer_of(p, x)?;
                self.printed(x)?
            }
            P::ValueToString => self.printed(x)?,
            P::StringToInteger | P::StringToFloat => {
                let s = self.text(p, x)?;
                let n = match p {
                    P::StringToInteger => s.parse().ok().map(Value::Integer),
                    _ => float_name(s).or_else(|| s.parse().ok()).map(Value::Float),
                };
                n.ok_or_else(|| Error::new(K::ParseError, &[Value::Primitive(p), x]))?
            }
            P::ReadForm => {
                let Value::String(r) = x else {
                    return Err(type_error(p, x));
                };
                let offset = usize::try_from(integer_of(p, y)?).ok();
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
                let read =
                    read.ok_or_else(|| Error::new(K::RangeError, &[Value::Primitive(p), x, y]))?;
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

    /// The primitives on bytes, apart from [`Grift::primitive`] so that its common cases stay
    /// small enough for the compiler to inline it into the machine's loop.
    #[inline(never)]
    fn bytes_primitive(
        &mut self,
        p: Primitive,
        [x, y, z]: [Value; 3],
        base: usize,
    ) -> Result<Value, Error> {
        use Primitive as P;
        Ok(match p {
            P::BytesLength => integer(self.bytes_of(p, x)?.len()),
            P::BytesByte => {
                let (b, i) = (self.bytes_of(p, x)?, integer_of(p, y)?);
                let byte = usize::try_from(i).ok().and_then(|i| b.get(i));
                let byte =
                    byte.ok_or_else(|| Error::new(K::RangeError, &[Value::Primitive(p), x, y]))?;
                Value::Integer(i64::from(*byte))
            }
            P::BytesSlice => {
                let b = self.bytes_of(p, x)?;
                let range = (
                    usize::try_from(integer_of(p, y)?),
                    usize::try_from(integer_of(p, z)?),
                );
                let part = match range {
                    (Ok(start), Ok(end)) => b.get(start..end),
                    _ => None,
                };
                let part =
                    part.ok_or_else(|| Error::new(K::RangeError, &[Value::Primitive(p), x, y, z]))?;
                self.bytes_local(collected(part.iter().copied())?)?
            }
            P::BytesAppend => {
                let given = self.machine.arguments.get(base..).unwrap_or_default();
                let mut len = 0usize;
                for &v in given {
                    len = len
                        .checked_add(self.bytes_of(p, v)?.len())
                        .ok_or_else(Error::oom)?;
                }
                let mut joined = Vec::new();
                joined.try_reserve_exact(len).map_err(|_| Error::oom())?;
                for &v in given {
                    joined.extend_from_slice(self.bytes_of(p, v)?);
                }
                self.bytes_local(joined)?
            }
            P::StringToBytes => {
                let b = collected(self.text(p, x)?.bytes())?;
                self.bytes_local(b)?
            }
            P::BytesToString => {
                let text = std::str::from_utf8(self.bytes_of(p, x)?).map_err(|e| {
                    let at = integer(e.valid_up_to());
                    Error::new(K::EncodingError, &[Value::Primitive(p), x, at])
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
                        Value::Integer(n) => u8::try_from(n).map_err(|_| {
                            Error::new(K::RangeError, &[Value::Primitive(p), v, at])
                        })?,
                        _ => return Err(Error::new(K::TypeError, &[Value::Primitive(p), v, at])),
                    };
                    push(&mut bytes, byte)?;
                    rest = next;
                }
                if rest != Value::EmptyList {
                    let at = integer(bytes.len());
                    return Err(Error::new(K::TypeError, &[Value::Primitive(p), rest, at]));
                }
                self.bytes_local(bytes)?
            }
            P::BytesToList => {
                let b = collected(self.bytes_of(p, x)?.iter().copied())?;
                b.iter().rev().try_fold(Value::EmptyList, |list, &byte| {
                    self.pair_local(Value::Integer(i64::from(byte)), list)
                })?
            }
            _ => return Err(type_error(p, x)),
        })
    }

    /// Calls combiner `f` with no arguments in `context`.
    fn call_thunk(&mut self, f: Value, context: Reference, at: Reference) -> Result<Mode, Error> {
        let operands = Value::EmptyList;
        self.frame(What::Combine { operands }, context, at)?;
        Ok(Mode::Return(f))
    }

    /// The error an error context describes: one `error` made, or a handler got.
    fn error_of(&self, e: Value) -> Result<Error, Error> {
        let bad = || type_error(Primitive::Raise, e);
        let Value::Context(r) = e else {
            return Err(bad());
        };
        let (_, fields) = self.context_at(r)?;
        let field = |k: K| fields.get(&k.into()).copied().ok_or_else(bad);
        let Value::Symbol(kind) = field(K::Kind)? else {
            return Err(bad());
        };
        let (values, trace) = (field(K::Values)?, field(K::Trace)?);
        let (values, trace) = (self.items(values, bad)?, self.items(trace, bad)?);
        let mut seen = Seen::default();
        seen.try_reserve(trace.len()).map_err(|_| Error::oom())?;
        seen.extend(trace.iter().filter_map(|&v| match v {
            Value::Pair(r) => Some(r),
            _ => None,
        }));
        Ok(Error {
            kind,
            values,
            trace,
            seen,
        })
    }

    /// The elements of a proper list; `bad()` for anything else.
    pub(crate) fn items(
        &self,
        mut list: Value,
        bad: impl Fn() -> Error,
    ) -> Result<Vec<Value>, Error> {
        let mut v = Vec::new();
        while let Value::Pair(r) = list {
            let (x, rest) = self.pair_at(r)?;
            push(&mut v, x)?;
            list = rest;
        }
        if list == Value::EmptyList {
            Ok(v)
        } else {
            Err(bad())
        }
    }

    /// Whether `x` and `y` are the same value: equal integers, symbols and strings, floats
    /// with the same bits, and otherwise the same object.
    fn same(&self, x: Value, y: Value) -> Result<bool, Error> {
        Ok(match (x, y) {
            (Value::String(_), Value::String(_)) => {
                self.text(Primitive::Same, x)? == self.text(Primitive::Same, y)?
            }
            (Value::Bytes(_), Value::Bytes(_)) => {
                self.bytes_of(Primitive::Same, x)? == self.bytes_of(Primitive::Same, y)?
            }
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            _ => x == y,
        })
    }

    /// The contents of string `v`, or a type error naming `p`.
    pub(crate) fn text(&self, p: Primitive, v: Value) -> Result<&str, Error> {
        match v {
            Value::String(r) => match self.heap.get(r)? {
                Object::String(s) => Ok(s),
                _ => Err(Error::new(K::StaleValue, &[v])),
            },
            _ => Err(type_error(p, v)),
        }
    }

    /// The contents of bytes value `v`, or a type error naming `p`.
    pub(crate) fn bytes_of(&self, p: Primitive, v: Value) -> Result<&[u8], Error> {
        match v {
            Value::Bytes(r) => match self.heap.get(r)? {
                Object::Bytes(b) => Ok(b),
                _ => Err(Error::new(K::StaleValue, &[v])),
            },
            _ => Err(type_error(p, v)),
        }
    }

    /// A new list of `xs`.
    pub(crate) fn list_local(&mut self, xs: &[Value]) -> Result<Value, Error> {
        xs.iter()
            .rev()
            .try_fold(Value::EmptyList, |l, &x| self.pair_local(x, l))
    }

    pub(crate) fn pair_local(&mut self, a: Value, b: Value) -> Result<Value, Error> {
        Ok(Value::Pair(self.heap.alloc(Object::Pair(a, b))?))
    }

    pub(crate) fn string_local(&mut self, s: &str) -> Result<Value, Error> {
        let s = boxed(s)?;
        Ok(Value::String(self.heap.alloc(Object::String(s))?))
    }

    pub(crate) fn bytes_local(&mut self, b: Vec<u8>) -> Result<Value, Error> {
        Ok(Value::Bytes(self.heap.alloc_bytes(b)?))
    }

    /// Checks a parameter tree: symbols, `#ignore`, `()` and pairs, no symbol twice, and a
    /// context parameter that is `#ignore` or a symbol not in the tree. The number of bindings
    /// a call makes.
    fn check_parameters(&mut self, tree: Value, context_parameter: Value) -> Result<usize, Error> {
        let bad = |v: Value| Error::new(K::ParameterError, &[v]);
        let mut seen: Seen<Symbol> = Seen::default();
        let mut todo = Vec::new();
        push(&mut todo, tree)?;
        while let Some(t) = todo.pop() {
            match t {
                Value::Symbol(s) => {
                    seen.try_reserve(1).map_err(|_| Error::oom())?;
                    if !seen.insert(s) {
                        return Err(bad(tree));
                    }
                }
                Value::Ignore | Value::EmptyList => {}
                Value::Pair(r) => {
                    let (a, b) = self.pair_at(r)?;
                    push(&mut todo, b)?;
                    push(&mut todo, a)?;
                }
                _ => return Err(bad(tree)),
            }
        }
        match context_parameter {
            Value::Symbol(s) if !seen.contains(&s) => Ok(seen.len() + 1),
            Value::Ignore => Ok(seen.len()),
            _ => Err(bad(context_parameter)),
        }
    }

    /// Matches a checked parameter tree to a value, binding its symbols in `bindings`.
    fn match_tree(
        &mut self,
        bindings: &mut Bindings,
        tree: Value,
        value: Value,
    ) -> Result<(), Error> {
        let mut work = std::mem::take(&mut self.machine.work);
        work.clear();
        let mut result = push(&mut work, (tree, value));
        while let (Ok(()), Some((t, v))) = (&result, work.pop()) {
            result = match (t, v) {
                (Value::Symbol(s), _) => bindings
                    .try_reserve(1)
                    .map(|()| {
                        bindings.insert(s, v);
                    })
                    .map_err(|_| Error::oom()),
                (Value::Ignore, _) | (Value::EmptyList, Value::EmptyList) => Ok(()),
                (Value::Pair(t), Value::Pair(v)) => self.pair_at(t).and_then(|(ta, tb)| {
                    let (va, vb) = self.pair_at(v)?;
                    push(&mut work, (tb, vb))?;
                    push(&mut work, (ta, va))
                }),
                _ => Err(Error::new(K::ArityError, &[tree, value])),
            };
        }
        self.machine.work = work;
        result
    }

    /// Matches parameter tree `tree` to `value`, binding its symbols in context `c` itself.
    pub(crate) fn bind_tree(
        &mut self,
        c: Reference,
        tree: Value,
        value: Value,
    ) -> Result<(), Error> {
        self.check_parameters(tree, Value::Ignore)?;
        let mut bindings = std::mem::take(self.bindings_mut(c)?);
        let matched = self.match_tree(&mut bindings, tree, value);
        *self.bindings_mut(c)? = bindings;
        matched
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
    pub(crate) fn names_of(&self, context: Reference) -> Result<Vec<Value>, Error> {
        let (_, bindings) = self.context_at(context)?;
        let mut names = Vec::new();
        names
            .try_reserve_exact(bindings.len())
            .map_err(|_| Error::oom())?;
        names.extend(bindings.keys().map(|&s| Value::Symbol(s)));
        names.sort_unstable_by(|a, b| match (a, b) {
            (Value::Symbol(a), Value::Symbol(b)) => self.name(*a).cmp(self.name(*b)),
            _ => std::cmp::Ordering::Equal,
        });
        Ok(names)
    }

    pub(crate) fn pair_at(&self, r: Reference) -> Result<(Value, Value), Error> {
        match self.heap.get(r)? {
            Object::Pair(a, b) => Ok((*a, *b)),
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

/// Whether an error is a limit set by the host or the machine, which no handler catches.
fn limit(kind: Symbol) -> bool {
    kind == K::Interrupted.into() || kind == K::OutOfMemory.into()
}

/// Adds program combination `r` to the error's trace, unless it is there already. With no
/// memory to spare the trace stays as it is: the error matters more than its last location.
fn locate(e: &mut Error, r: Reference) {
    if r != Reference::NONE
        && e.seen.try_reserve(1).is_ok()
        && e.trace.try_reserve(1).is_ok()
        && e.seen.insert(r)
    {
        e.trace.push(Value::Pair(r));
    }
}

/// The bitwise operations, as Rust's `&`, `|`, `^`, `<<` and `>>` on `i64`: a shift by an amount
/// outside 0 to 63 is `overflow`, where Rust panics, and the bits shifted out are lost, as in Rust.
fn bitwise(p: Primitive, x: Value, y: Value) -> Result<Value, Error> {
    use Primitive as P;
    let (a, b) = (integer_of(p, x)?, integer_of(p, y)?);
    let amount = u32::try_from(b).ok();
    let r = match p {
        P::BitwiseAnd => Some(a & b),
        P::BitwiseOr => Some(a | b),
        P::BitwiseExclusiveOr => Some(a ^ b),
        P::ShiftLeft => amount.and_then(|n| a.checked_shl(n)),
        _ => amount.and_then(|n| a.checked_shr(n)),
    };
    r.map(Value::Integer)
        .ok_or_else(|| Error::new(K::Overflow, &[Value::Primitive(p), x, y]))
}

/// A length or offset as a grift integer.
pub(crate) fn integer(n: usize) -> Value {
    Value::Integer(i64::try_from(n).unwrap_or(i64::MAX))
}

fn integer_of(p: Primitive, v: Value) -> Result<i64, Error> {
    match v {
        Value::Integer(n) => Ok(n),
        _ => Err(type_error(p, v)),
    }
}

fn float_of(p: Primitive, v: Value) -> Result<f64, Error> {
    match v {
        Value::Float(f) => Ok(f),
        _ => Err(type_error(p, v)),
    }
}

#[inline]
fn is_combiner(v: Value) -> bool {
    matches!(
        v,
        Value::Operative(_) | Value::Applicative(_) | Value::Primitive(_) | Value::Host(_)
    )
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

/// Arithmetic and comparison, as Rust has them, on two integers or two floats. Integer
/// arithmetic is checked: `divide` and `remainder` are Rust's `/` and `%`, truncating, and fail
/// where Rust's would panic. Float arithmetic is IEEE.
#[inline]
fn arithmetic(p: Primitive, x: Value, y: Value) -> Result<Value, Error> {
    use Primitive as P;
    let r = match (x, y) {
        (Value::Integer(a), Value::Integer(b)) => match p {
            P::Add => a.checked_add(b),
            P::Subtract => a.checked_sub(b),
            P::Multiply => a.checked_mul(b),
            P::Divide | P::Remainder if b == 0 => {
                return Err(Error::new(K::DivisionByZero, &[Value::Primitive(p), x, y]));
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
        (Value::Integer(_) | Value::Float(_), _) => return Err(type_error(p, y)),
        _ => return Err(type_error(p, x)),
    };
    r.map(Value::Integer)
        .ok_or_else(|| Error::new(K::Overflow, &[Value::Primitive(p), x, y]))
}
