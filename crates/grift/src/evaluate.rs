//! The evaluator: an explicit machine loop over a frame stack, and the Rust primitives.
//!
//! The machine's state is its frame stack, its argument stack, the errors waiting for a
//! cleanup to finish, where it is in the program and the current [`Mode`]. The collector runs
//! only at the top of the loop, with exactly that state (plus the built-in and top-level
//! contexts) as its roots. Between two safe points Rust code may hold values in locals freely:
//! nothing is freed until the next safe point, and by then every local is gone. The same safe
//! point checks the host's interrupt flag.

use crate::heap::{Bindings, Object, Operative, Seen};
use crate::read::float_name;
use crate::{Error, Grift, Host, K, Reference, Symbol, Value, boxed, formatted, push};

/// Declares the primitives: each one's variant, its name in the built-in context, and the
/// fewest and most operands it takes.
macro_rules! primitives {
    ($($p:ident $name:literal $lo:literal $hi:expr;)*) => {
        /// The Rust primitives. All but `vau!` are bound wrapped, as applicatives.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Primitive { $($p),* }

        impl Primitive {
            pub(crate) const ALL: &[Primitive] = &[$(Primitive::$p),*];

            /// The name the primitive is bound to in the built-in context.
            pub fn name(self) -> &'static str {
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
    Vau "vau!" 3 3;
    Wrap "wrap" 1 1;
    Unwrap "unwrap" 1 1;
    Evaluate "evaluate" 2 2;
    EvaluateWithCleanup "evaluate-with-cleanup" 3 3;
    CallWithHandler "call-with-handler" 2 2;
    Raise "raise" 1 1;
    Context "context" 0 1;
    Bind "bind" 3 3;
    Names "names" 1 1;
    Pair "pair" 2 2;
    First "first" 1 1;
    Rest "rest" 1 1;
    Same "same?" 2 2;
    Select "select" 3 3;
    TypeOf "type-of" 1 1;
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
    IntegerToFloat "integer-to-float" 1 1;
    FloatToInteger "float-to-integer" 1 1;
    StringByteLength "string-byte-length" 1 1;
    StringByte "string-byte" 2 2;
    ByteSubstring "byte-substring" 3 3;
    StringAppend "string-append" 0 usize::MAX;
    CodePointToString "code-point-to-string" 1 1;
    SymbolToString "symbol-to-string" 1 1;
    StringToSymbol "string-to-symbol" 1 1;
    IntegerToString "integer-to-string" 1 1;
    StringToInteger "string-to-integer" 1 1;
    FloatToString "float-to-string" 1 1;
    StringToFloat "string-to-float" 1 1;
    ReadForm "read-form" 2 2;
    ValueToString "value-to-string" 1 1;
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

/// A continuation frame: what to do with the value being returned. `at` is the combination
/// of the program the frame belongs to ([`Reference::NONE`] for none), where an error is
/// reported.
#[derive(Clone, Copy)]
enum Frame {
    /// Evaluate the remaining forms of one `evaluate` call; with none left, pass the value on.
    /// `outer` is the location of the evaluation this one runs inside (a host function's
    /// call), kept here so the collector sees it until it is restored; `at` is the form
    /// being evaluated.
    Top {
        rest: Value,
        context: Reference,
        outer: Reference,
        at: Reference,
    },
    /// The value is the combiner of a combination with these operands.
    Combine {
        operands: Value,
        context: Reference,
        at: Reference,
    },
    /// The value is an argument of `combiner`; arguments so far are on the argument stack
    /// from `base`.
    Arguments {
        combiner: Value,
        context: Reference,
        rest: Value,
        base: usize,
        at: Reference,
    },
    /// When the evaluation above ends, by a value or an error, call the combiner `cleanup` with
    /// no arguments in `context`; `arguments` is the argument stack's height to go back to.
    Cleanup {
        cleanup: Value,
        context: Reference,
        arguments: usize,
        at: Reference,
    },
    /// When an error leaves the evaluation above, call `handler` in `context` with it as a
    /// value instead; `arguments` is the argument stack's height to go back to. A value passes
    /// through.
    Handler {
        handler: Value,
        context: Reference,
        arguments: usize,
        at: Reference,
    },
    /// The cleanup above ran after this value: deliver it.
    Deliver(Value),
    /// The cleanup above ran after an error, the last one waiting: raise it again.
    Raise,
}

impl Frame {
    fn values(self) -> [Value; 4] {
        let empty = Value::EmptyList;
        match self {
            Frame::Top {
                rest,
                context,
                outer,
                at,
            } => [
                rest,
                Value::Context(context),
                Value::Pair(outer),
                Value::Pair(at),
            ],
            Frame::Combine {
                operands,
                context,
                at,
            } => [operands, Value::Context(context), Value::Pair(at), empty],
            Frame::Arguments {
                combiner,
                context,
                rest,
                at,
                ..
            } => [combiner, Value::Context(context), rest, Value::Pair(at)],
            Frame::Cleanup {
                cleanup,
                context,
                at,
                ..
            } => [cleanup, Value::Context(context), Value::Pair(at), empty],
            Frame::Handler {
                handler,
                context,
                at,
                ..
            } => [handler, Value::Context(context), Value::Pair(at), empty],
            Frame::Deliver(v) => [v, empty, empty, empty],
            Frame::Raise => [empty; 4],
        }
    }

    /// The combination of the program the frame belongs to, or [`Reference::NONE`].
    fn at(self) -> Reference {
        match self {
            Frame::Top { at, .. }
            | Frame::Combine { at, .. }
            | Frame::Arguments { at, .. }
            | Frame::Cleanup { at, .. }
            | Frame::Handler { at, .. } => at,
            Frame::Deliver(_) | Frame::Raise => Reference::NONE,
        }
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

fn type_error(p: Primitive, v: Value) -> Error {
    Error::new(K::TypeError, &[Value::Primitive(p), v])
}

#[inline]
fn boolean(b: bool) -> Value {
    if b { Value::True } else { Value::False }
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
        let outer = self.machine.here;
        let top = Frame::Top {
            rest: forms,
            context,
            outer,
            at: Reference::NONE,
        };
        self.start(state, &[top], Mode::Return(Value::Inert))
    }

    /// Calls combiner `f` with `arguments` as Kernel's `apply` does: an applicative's
    /// underlying combiner gets the arguments as its operands, in a new empty context; an
    /// operative gets them as they are. Host functions get `state`.
    pub fn call(&mut self, state: &mut T, f: Value, arguments: &[Value]) -> Result<Value, Error> {
        let f = match f {
            Value::Applicative(r) => self.unwrapped(r)?,
            _ => f,
        };
        let operands = self.list(arguments)?;
        let context = self
            .heap
            .alloc(Object::Context(None, Bindings::default()))?;
        let frames = [
            Frame::Top {
                rest: Value::EmptyList,
                context,
                outer: self.machine.here,
                at: Reference::NONE,
            },
            Frame::Combine {
                operands,
                context,
                at: Reference::NONE,
            },
        ];
        self.start(Some(state), &frames, Mode::Return(f))
    }

    /// Runs the machine on top of the current frames from `mode`, with `frames` pushed, until
    /// they are all gone; then everything is as it was, whatever the outcome.
    fn start(
        &mut self,
        state: Option<&mut T>,
        frames: &[Frame],
        mode: Mode,
    ) -> Result<Value, Error> {
        let m = &mut self.machine;
        let (floor, arguments, raised, here) =
            (m.frames.len(), m.arguments.len(), m.raised.len(), m.here);
        let result = frames
            .iter()
            .try_for_each(|&f| push(&mut self.machine.frames, f))
            .and_then(|()| self.run(state, mode, floor));
        let m = &mut self.machine;
        m.frames.truncate(floor);
        m.arguments.truncate(arguments);
        m.raised.truncate(raised);
        m.here = here;
        result
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
                return Ok(v);
            }
            let next = self.safe_point(mode).and_then(|()| match mode {
                Mode::Host(h, base) => self.host(state.as_deref_mut(), h, base),
                _ => self.step(mode),
            });
            mode = match next {
                Ok(mode) => mode,
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
            self.collect(mode)?;
        }
        Ok(())
    }

    #[inline(always)]
    fn step(&mut self, mode: Mode) -> Result<Mode, Error> {
        Ok(match mode {
            Mode::Evaluate(Value::Symbol(s), context) => Mode::Return(self.lookup(context, s)?),
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
                let at = self.machine.here;
                push(
                    &mut self.machine.frames,
                    Frame::Combine {
                        operands,
                        context,
                        at,
                    },
                )?;
                Mode::Evaluate(combiner, context)
            }
            Mode::Evaluate(x, _) => Mode::Return(x),
            Mode::Return(v) => match self.machine.frames.pop() {
                None => Mode::Return(v),
                Some(Frame::Top {
                    rest: Value::Pair(r),
                    context,
                    outer,
                    ..
                }) => {
                    let (form, rest) = self.pair_at(r)?;
                    let at = match form {
                        Value::Pair(f) => f,
                        _ => Reference::NONE,
                    };
                    let top = Frame::Top {
                        rest,
                        context,
                        outer,
                        at,
                    };
                    push(&mut self.machine.frames, top)?;
                    self.machine.here = Reference::NONE;
                    Mode::Evaluate(form, context)
                }
                Some(Frame::Top { .. }) => Mode::Return(v),
                Some(Frame::Combine {
                    operands,
                    context,
                    at,
                }) => {
                    self.machine.here = at;
                    self.combine(v, operands, context)?
                }
                Some(Frame::Arguments {
                    combiner,
                    context,
                    rest,
                    base,
                    at,
                }) => {
                    self.machine.here = at;
                    push(&mut self.machine.arguments, v)?;
                    self.next_argument(combiner, context, rest, base)?
                }
                Some(Frame::Cleanup {
                    cleanup,
                    context,
                    at,
                    ..
                }) => {
                    self.machine.here = at;
                    push(&mut self.machine.frames, Frame::Deliver(v))?;
                    self.call_thunk(cleanup, context, at)?
                }
                Some(Frame::Handler { at, .. }) => {
                    self.machine.here = at;
                    Mode::Return(v)
                }
                Some(Frame::Deliver(v)) => Mode::Return(v),
                Some(Frame::Raise) => {
                    // The error already says where it happened, or that it can't: not here.
                    self.machine.here = Reference::NONE;
                    return Err(self.machine.raised.pop().unwrap_or_else(Error::oom));
                }
            },
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
            let Some(frame) = self.machine.frames.pop() else {
                break;
            };
            match frame {
                Frame::Cleanup {
                    cleanup,
                    context,
                    arguments,
                    at,
                } => {
                    locate(&mut e, at);
                    self.machine.arguments.truncate(arguments);
                    self.machine.here = at;
                    push(&mut self.machine.raised, e)?;
                    push(&mut self.machine.frames, Frame::Raise)?;
                    return self.call_thunk(cleanup, context, at);
                }
                Frame::Handler {
                    handler,
                    context,
                    arguments,
                    at,
                } if !limit(e.kind) => {
                    self.machine.arguments.truncate(arguments);
                    self.machine.here = at;
                    match self.handle(&e, handler, context, arguments, at) {
                        Ok(mode) => return Ok(mode),
                        Err(oom) => e = oom,
                    }
                }
                // The cleanup's error replaces the one waiting, unless that one is a limit.
                Frame::Raise => match self.machine.raised.pop() {
                    Some(waiting) if limit(waiting.kind) && !limit(e.kind) => e = waiting,
                    _ => {}
                },
                frame => locate(&mut e, frame.at()),
            }
        }
        Err(e)
    }

    /// Calls a handler with the error as a new context, with no parent, binding `kind`,
    /// `values` and `trace`.
    fn handle(
        &mut self,
        e: &Error,
        handler: Value,
        context: Reference,
        base: usize,
        at: Reference,
    ) -> Result<Mode, Error> {
        let (values, trace) = (self.list(&e.values)?, self.list(&e.trace)?);
        let mut fields = Bindings::default();
        fields.try_reserve(3).map_err(|_| Error::oom())?;
        fields.insert(K::Kind.into(), Value::Symbol(e.kind));
        fields.insert(K::Values.into(), values);
        fields.insert(K::Trace.into(), trace);
        let v = Value::Context(self.heap.alloc(Object::Context(None, fields))?);
        let combiner = match handler {
            Value::Applicative(r) => self.unwrapped(r)?,
            h => h,
        };
        let rest = Value::EmptyList;
        push(
            &mut self.machine.frames,
            Frame::Arguments {
                combiner,
                context,
                rest,
                base,
                at,
            },
        )?;
        Ok(Mode::Return(v))
    }

    fn collect(&mut self, mode: Mode) -> Result<(), Error> {
        let current = match mode {
            Mode::Evaluate(x, context) => [x, Value::Context(context)],
            Mode::Return(v) => [v, Value::EmptyList],
            Mode::Host(..) => [Value::EmptyList, Value::EmptyList],
        };
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
        arguments.extend_from_slice(given);
        let v = f(self, state, &arguments);
        self.machine.arguments.truncate(base);
        Ok(Mode::Return(v?))
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
                let at = self.machine.here;
                push(
                    &mut self.machine.frames,
                    Frame::Arguments {
                        combiner,
                        context,
                        rest,
                        base,
                        at,
                    },
                )?;
                Ok(Mode::Evaluate(x, context))
            }
            Value::EmptyList => self.apply(combiner, context, base),
            _ => Err(Error::new(K::TypeError, &[combiner, rest])),
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
                let mut list = Value::EmptyList;
                while self.machine.arguments.len() > base {
                    let x = self.machine.arguments.pop().unwrap_or(Value::EmptyList);
                    list = self.pair(x, list)?;
                }
                self.combine(combiner, list, context)
            }
        }
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
        // The bindings are made first, with room for all of them, then the context.
        let mut bindings = Bindings::default();
        bindings.try_reserve(op.size).map_err(|_| Error::oom())?;
        self.match_tree(&mut bindings, op.parameters, operands)?;
        if let Value::Symbol(s) = op.context_parameter {
            bindings.insert(s, Value::Context(context));
        }
        let local = self
            .heap
            .alloc(Object::Context(Some(op.context), bindings))?;
        Ok(Mode::Evaluate(op.body, local))
    }

    fn primitive(&mut self, p: Primitive, context: Reference, base: usize) -> Result<Mode, Error> {
        use Primitive as P;
        let given = self.machine.arguments.get(base..).unwrap_or_default();
        let (lo, hi) = p.arity();
        if given.len() < lo || given.len() > hi {
            let mut values = Vec::new();
            values
                .try_reserve_exact(given.len() + 1)
                .map_err(|_| Error::oom())?;
            values.push(Value::Primitive(p));
            values.extend_from_slice(given);
            return Err(Error {
                kind: K::ArityError.into(),
                values,
                trace: Vec::new(),
                seen: Seen::default(),
            });
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
                    size,
                };
                Value::Operative(self.heap.alloc(Object::Operative(op))?)
            }
            P::Wrap => match x {
                Value::Operative(_)
                | Value::Applicative(_)
                | Value::Primitive(_)
                | Value::Host(_) => Value::Applicative(self.heap.alloc(Object::Applicative(x))?),
                _ => return Err(type_error(p, x)),
            },
            P::Unwrap => match x {
                Value::Applicative(r) => self.unwrapped(r)?,
                _ => return Err(type_error(p, x)),
            },
            P::Evaluate => match y {
                Value::Context(c) => return Ok(Mode::Evaluate(x, c)),
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
                    self.check_parameters(y, Value::Ignore)?;
                    let Object::Context(_, bindings) = self.heap.get_mut(c)? else {
                        return Err(Error::new(K::StaleValue, &[x]));
                    };
                    let mut bindings = std::mem::take(bindings);
                    let matched = self.match_tree(&mut bindings, y, z);
                    if let Object::Context(_, b) = self.heap.get_mut(c)? {
                        *b = bindings;
                    }
                    matched?;
                    Value::Inert
                }
                _ => return Err(type_error(p, x)),
            },
            P::Names => match x {
                Value::Context(c) => {
                    let names = self.names_of(c)?;
                    self.list(&names)?
                }
                _ => return Err(type_error(p, x)),
            },
            P::Pair => self.pair(x, y)?,
            P::First | P::Rest => match x {
                Value::Pair(r) => {
                    let (a, b) = self.pair_at(r)?;
                    if p == P::First { a } else { b }
                }
                _ => return Err(type_error(p, x)),
            },
            P::Same => boolean(self.same(x, y)),
            P::Select => match x {
                Value::True => y,
                Value::False => z,
                _ => return Err(type_error(p, x)),
            },
            P::TypeOf => Value::Symbol(type_of(x).into()),
            P::Add | P::Subtract | P::Multiply | P::Divide | P::Remainder | P::LessThan => {
                arithmetic(p, x, y)?
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
                    let (arguments, at) = (base, self.machine.here);
                    let cleanup = Frame::Cleanup {
                        cleanup: z,
                        context: c,
                        arguments,
                        at,
                    };
                    push(&mut self.machine.frames, cleanup)?;
                    return Ok(Mode::Evaluate(x, c));
                }
                _ => return Err(type_error(p, y)),
            },
            P::CallWithHandler => {
                if let Some(&v) = [x, y].iter().find(|&&v| !is_combiner(v)) {
                    return Err(type_error(p, v));
                }
                let at = self.machine.here;
                let handler = Frame::Handler {
                    handler: y,
                    context,
                    arguments: base,
                    at,
                };
                push(&mut self.machine.frames, handler)?;
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
            P::StringByteLength => {
                Value::Integer(i64::try_from(self.text(p, x)?.len()).unwrap_or(i64::MAX))
            }
            P::StringByte => {
                let (s, i) = (self.text(p, x)?, integer_of(p, y)?);
                let byte = usize::try_from(i).ok().and_then(|i| s.as_bytes().get(i));
                let byte =
                    byte.ok_or_else(|| Error::new(K::RangeError, &[Value::Primitive(p), x, y]))?;
                Value::Integer(i64::from(*byte))
            }
            P::ByteSubstring => {
                let range = (
                    usize::try_from(integer_of(p, y)?),
                    usize::try_from(integer_of(p, z)?),
                );
                let part = match range {
                    (Ok(a), Ok(b)) => self.text(p, x)?.get(a..b),
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
            P::CodePointToString => {
                let c = u32::try_from(integer_of(p, x)?)
                    .ok()
                    .and_then(char::from_u32);
                let c = c.ok_or_else(|| Error::new(K::RangeError, &[Value::Primitive(p), x]))?;
                self.string(c.encode_utf8(&mut [0; 4]))?
            }
            P::SymbolToString => match x {
                Value::Symbol(s) => {
                    let name = boxed(self.name(s))?;
                    Value::String(self.heap.alloc(Object::String(name))?)
                }
                _ => return Err(type_error(p, x)),
            },
            P::StringToSymbol => {
                let name = boxed(self.text(p, x)?)?;
                Value::Symbol(self.symbol(&name)?)
            }
            P::IntegerToString => {
                let s = formatted(format_args!("{}", integer_of(p, x)?))?;
                Value::String(self.heap.alloc(Object::String(s))?)
            }
            P::FloatToString => {
                float_of(p, x)?;
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
                    .map(|i| self.read_form(&text, i));
                if let Object::String(s) = self.heap.get_mut(r)? {
                    *s = text;
                }
                let read =
                    read.ok_or_else(|| Error::new(K::RangeError, &[Value::Primitive(p), x, y]))?;
                match read? {
                    Some((form, next)) => {
                        let next = i64::try_from(next).map_err(|_| Error::oom())?;
                        self.pair(form, Value::Integer(next))?
                    }
                    None => Value::EmptyList,
                }
            }
        };
        Ok(Mode::Return(v))
    }

    /// Calls combiner `f` with no arguments in `context`.
    fn call_thunk(&mut self, f: Value, context: Reference, at: Reference) -> Result<Mode, Error> {
        let operands = Value::EmptyList;
        push(
            &mut self.machine.frames,
            Frame::Combine {
                operands,
                context,
                at,
            },
        )?;
        Ok(Mode::Return(f))
    }

    /// The error an error context describes: one `error` made, or a handler got.
    fn error_of(&self, e: Value) -> Result<Error, Error> {
        let bad = || type_error(Primitive::Raise, e);
        let Value::Context(r) = e else {
            return Err(bad());
        };
        let Object::Context(_, fields) = self.heap.get(r)? else {
            return Err(bad());
        };
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
    fn items(&self, mut list: Value, bad: impl Fn() -> Error) -> Result<Vec<Value>, Error> {
        let mut v = Vec::new();
        while let Some((x, rest)) = self.as_pair(list) {
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
    fn same(&self, x: Value, y: Value) -> bool {
        match (x, y) {
            (Value::String(_), Value::String(_)) => self.as_str(x) == self.as_str(y),
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            _ => x == y,
        }
    }

    /// The contents of string `v`, or a type error naming `p`.
    fn text(&self, p: Primitive, v: Value) -> Result<&str, Error> {
        self.as_str(v).ok_or_else(|| type_error(p, v))
    }

    /// A new list of `xs`.
    fn list(&mut self, xs: &[Value]) -> Result<Value, Error> {
        xs.iter()
            .rev()
            .try_fold(Value::EmptyList, |l, &x| self.pair(x, l))
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
        let result = self.match_with(&mut work, bindings, tree, value);
        self.machine.work = work;
        result
    }

    fn match_with(
        &self,
        work: &mut Vec<(Value, Value)>,
        bindings: &mut Bindings,
        tree: Value,
        value: Value,
    ) -> Result<(), Error> {
        let mismatch = || Error::new(K::ArityError, &[tree, value]);
        push(work, (tree, value))?;
        while let Some((t, v)) = work.pop() {
            match (t, v) {
                (Value::Symbol(s), _) => {
                    bindings.try_reserve(1).map_err(|_| Error::oom())?;
                    bindings.insert(s, v);
                }
                (Value::Ignore, _) | (Value::EmptyList, Value::EmptyList) => {}
                (Value::Pair(t), Value::Pair(v)) => {
                    let ((ta, tb), (va, vb)) = (self.pair_at(t)?, self.pair_at(v)?);
                    push(work, (tb, vb))?;
                    push(work, (ta, va))?;
                }
                _ => return Err(mismatch()),
            }
        }
        Ok(())
    }

    /// Binds `s` in `context` itself, replacing an existing binding there.
    pub(crate) fn bind_symbol(
        &mut self,
        context: Reference,
        s: Symbol,
        v: Value,
    ) -> Result<(), Error> {
        let Object::Context(_, bindings) = self.heap.get_mut(context)? else {
            return Err(Error::new(K::StaleValue, &[Value::Context(context)]));
        };
        bindings.try_reserve(1).map_err(|_| Error::oom())?;
        bindings.insert(s, v);
        Ok(())
    }

    fn lookup(&self, mut context: Reference, s: Symbol) -> Result<Value, Error> {
        loop {
            let Object::Context(parent, bindings) = self.heap.get(context)? else {
                return Err(Error::new(K::StaleValue, &[Value::Context(context)]));
            };
            if let Some(&v) = bindings.get(&s) {
                return Ok(v);
            }
            context = parent.ok_or_else(|| Error::new(K::UnboundSymbol, &[Value::Symbol(s)]))?;
        }
    }

    /// The symbols bound in `context` itself, sorted by name.
    pub(crate) fn names_of(&self, context: Reference) -> Result<Vec<Value>, Error> {
        let Object::Context(_, bindings) = self.heap.get(context)? else {
            return Err(Error::new(K::StaleValue, &[Value::Context(context)]));
        };
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
        Value::True | Value::False => K::Boolean,
        Value::Inert => K::Inert,
        Value::Ignore => K::Ignore,
        Value::Integer(_) => K::Integer,
        Value::Float(_) => K::Float,
        Value::Symbol(_) => K::Symbol,
        Value::Pair(_) => K::Pair,
        Value::String(_) => K::String,
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
            _ => return Ok(boolean(a < b)),
        },
        (Value::Float(a), Value::Float(b)) => {
            return Ok(Value::Float(match p {
                P::Add => a + b,
                P::Subtract => a - b,
                P::Multiply => a * b,
                P::Divide => a / b,
                P::Remainder => a % b,
                _ => return Ok(boolean(a < b)),
            }));
        }
        (Value::Integer(_) | Value::Float(_), _) => return Err(type_error(p, y)),
        _ => return Err(type_error(p, x)),
    };
    r.map(Value::Integer)
        .ok_or_else(|| Error::new(K::Overflow, &[Value::Primitive(p), x, y]))
}
