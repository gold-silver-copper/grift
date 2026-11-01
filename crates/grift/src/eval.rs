//! The evaluator: an explicit machine loop over a frame stack, and the Rust primitives.
//!
//! The machine's state is its frame stack, its argument stack, the errors waiting for a
//! cleanup to finish, where it is in the program and the current [`Mode`]. The collector runs
//! only at the top of the loop, with exactly that state (plus the ground and top-level
//! environments) as its roots. Between two safe points Rust code may hold values in locals
//! freely: nothing is freed until the next safe point, and by then every local is gone. The
//! same safe point checks the host's interrupt flag.

use crate::heap::{Obj, Op};
use crate::{Error, Grift, Host, K, Ref, Symbol, Value, boxed, push};
use std::fmt::Write;

/// Declares the primitives: each one's variant, its name in the ground environment, and the
/// fewest and most operands it takes.
macro_rules! prims {
    ($($p:ident $name:literal $lo:literal $hi:expr;)*) => {
        /// The Rust primitives. All but `$vau` are bound wrapped, as applicatives.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Prim { $($p),* }

        impl Prim {
            pub(crate) const ALL: &[Prim] = &[$(Prim::$p),*];

            /// The name the primitive is bound to in the ground environment.
            pub fn name(self) -> &'static str {
                match self { $(Prim::$p => $name),* }
            }

            /// The fewest and most operands.
            fn arity(self) -> (usize, usize) {
                match self { $(Prim::$p => ($lo, $hi)),* }
            }
        }
    };
}

prims! {
    Vau "$vau" 3 3;
    Wrap "wrap" 1 1;
    Unwrap "unwrap" 1 1;
    Eval "eval" 2 2;
    MakeEnvironment "make-environment" 0 1;
    Bind "bind!" 3 3;
    Bindings "bindings" 1 1;
    Cons "cons" 2 2;
    Car "car" 1 1;
    Cdr "cdr" 1 1;
    Eq "eq?" 2 2;
    Select "select" 3 3;
    TypeOf "type-of" 1 1;
    Add "+" 2 2;
    Sub "-" 2 2;
    Mul "*" 2 2;
    Div "div" 2 2;
    Mod "mod" 2 2;
    Less "<?" 2 2;
    Error "error" 1 usize::MAX;
    EvalProtected "eval-protected" 3 3;
    Guard "guard" 2 2;
    Raise "raise" 1 1;
    StringLength "string-length" 1 1;
    StringByte "string-byte" 2 2;
    Substring "substring" 3 3;
    StringAppend "string-append" 0 usize::MAX;
    CodePointToString "code-point->string" 1 1;
    SymbolToString "symbol->string" 1 1;
    StringToSymbol "string->symbol" 1 1;
    NumberToString "number->string" 1 1;
}

/// What the machine does next.
#[derive(Clone, Copy)]
pub(crate) enum Mode {
    /// Evaluate an expression in an environment.
    Eval(Value, Ref),
    /// Deliver a value to the top frame.
    Return(Value),
    /// Call a host function with the arguments on the argument stack from `base`.
    Host(Host, usize),
}

/// A continuation frame: what to do with the value being returned. `at` is the combination
/// of the program the frame belongs to ([`Ref::NONE`] for none), where an error is reported.
#[derive(Clone, Copy)]
enum Frame {
    /// Evaluate the remaining forms of one `eval` call; with none left, pass the value on.
    /// `outer` is the location of the evaluation this one runs inside (a host function's
    /// call), kept here so the collector sees it until it is restored; `at` is the form
    /// being evaluated.
    Top {
        rest: Value,
        env: Ref,
        outer: Ref,
        at: Ref,
    },
    /// The value is the combiner of a combination with these operands.
    Combine { operands: Value, env: Ref, at: Ref },
    /// The value is an argument of `comb`; arguments so far are on the argument stack from
    /// `base`.
    Args {
        comb: Value,
        env: Ref,
        rest: Value,
        base: usize,
        at: Ref,
    },
    /// When the evaluation above ends, by a value or an error, call the combiner `cleanup` with
    /// no arguments in `env`; `args` is the argument stack's height to go back to.
    Cleanup {
        cleanup: Value,
        env: Ref,
        args: usize,
        at: Ref,
    },
    /// When an error leaves the evaluation above, call `handler` in `env` with it as a value
    /// instead; `args` is the argument stack's height to go back to. A value passes through.
    Guard {
        handler: Value,
        env: Ref,
        args: usize,
        at: Ref,
    },
    /// The cleanup above ran after this value: deliver it.
    Deliver(Value),
    /// The cleanup above ran after an error, the last one waiting: raise it again.
    Raise,
}

impl Frame {
    fn values(self) -> [Value; 4] {
        let nil = Value::Nil;
        match self {
            Frame::Top {
                rest,
                env,
                outer,
                at,
            } => [rest, Value::Env(env), Value::Pair(outer), Value::Pair(at)],
            Frame::Combine { operands, env, at } => {
                [operands, Value::Env(env), Value::Pair(at), nil]
            }
            Frame::Args {
                comb,
                env,
                rest,
                at,
                ..
            } => [comb, Value::Env(env), rest, Value::Pair(at)],
            Frame::Cleanup {
                cleanup, env, at, ..
            } => [cleanup, Value::Env(env), Value::Pair(at), nil],
            Frame::Guard {
                handler, env, at, ..
            } => [handler, Value::Env(env), Value::Pair(at), nil],
            Frame::Deliver(v) => [v, nil, nil, nil],
            Frame::Raise => [nil; 4],
        }
    }

    /// The combination of the program the frame belongs to, or [`Ref::NONE`].
    fn at(self) -> Ref {
        match self {
            Frame::Top { at, .. }
            | Frame::Combine { at, .. }
            | Frame::Args { at, .. }
            | Frame::Cleanup { at, .. }
            | Frame::Guard { at, .. } => at,
            Frame::Deliver(_) | Frame::Raise => Ref::NONE,
        }
    }
}

pub(crate) struct Machine {
    frames: Vec<Frame>,
    args: Vec<Value>,
    /// Errors waiting for a cleanup to finish, one per `Raise` frame.
    raised: Vec<Error>,
    /// The innermost combination of the program being evaluated, or [`Ref::NONE`]. A `Ref`,
    /// not a `Value`: this is written and read at every step.
    here: Ref,
    /// Scratch for matching formals trees.
    work: Vec<(Value, Value)>,
    /// Steps taken, for tests of what a form costs.
    #[cfg(test)]
    pub steps: u64,
}

impl Default for Machine {
    fn default() -> Machine {
        Machine {
            frames: Vec::new(),
            args: Vec::new(),
            raised: Vec::new(),
            here: Ref::NONE,
            work: Vec::new(),
            #[cfg(test)]
            steps: 0,
        }
    }
}

fn type_error(p: Prim, v: Value) -> Error {
    Error::new(K::TypeError, &[Value::Prim(p), v])
}

#[inline]
fn boolean(b: bool) -> Value {
    if b { Value::True } else { Value::False }
}

impl<T> Grift<T> {
    /// Evaluates the list `forms` in `env`, in order. Reentrant: a host function may call
    /// `eval`, which runs on top of the current frames and leaves them as it found them.
    pub(crate) fn eval_forms(
        &mut self,
        state: Option<&mut T>,
        forms: Value,
        env: Ref,
    ) -> Result<Value, Error> {
        let outer = self.machine.here;
        let top = Frame::Top {
            rest: forms,
            env,
            outer,
            at: Ref::NONE,
        };
        self.start(state, &[top], Mode::Return(Value::Inert))
    }

    /// Calls combiner `f` with `args` as Kernel's `apply` does: an applicative's underlying
    /// combiner gets the arguments as its operands, in a new empty environment; an operative
    /// gets them as they are. Host functions get `state`.
    pub fn call(&mut self, state: &mut T, f: Value, args: &[Value]) -> Result<Value, Error> {
        let f = match f {
            Value::App(r) => self.app(r)?,
            _ => f,
        };
        let operands = self.list(args)?;
        let env = self.heap.alloc(Obj::Env(None, Vec::new()))?;
        let frames = [
            Frame::Top {
                rest: Value::Nil,
                env,
                outer: self.machine.here,
                at: Ref::NONE,
            },
            Frame::Combine {
                operands,
                env,
                at: Ref::NONE,
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
        let (floor, args, raised, here) = (m.frames.len(), m.args.len(), m.raised.len(), m.here);
        let result = frames
            .iter()
            .try_for_each(|&f| push(&mut self.machine.frames, f))
            .and_then(|()| self.run(state, mode, floor));
        let m = &mut self.machine;
        m.frames.truncate(floor);
        m.args.truncate(args);
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
            Mode::Eval(Value::Sym(s), env) => Mode::Return(self.lookup(env, s)?),
            Mode::Eval(Value::Pair(r), env) => {
                let (combiner, operands, library) = match self.heap.get_code(r)? {
                    (Obj::Pair(a, d), library) => (*a, *d, library),
                    _ => return Err(Error::new(K::StaleValue, &[Value::Pair(r)])),
                };
                // A combination is the program's unless it is the library's code, or code built
                // around a combiner value (as the library builds it), not written.
                if !library && !is_combiner(combiner) {
                    self.machine.here = r;
                }
                // A combiner that needs no evaluation of its own is combined at once.
                if let Some(f) = self.at_once(combiner, env)? {
                    return self.combine(f, operands, env);
                }
                let at = self.machine.here;
                push(
                    &mut self.machine.frames,
                    Frame::Combine { operands, env, at },
                )?;
                Mode::Eval(combiner, env)
            }
            Mode::Eval(x, _) => Mode::Return(x),
            Mode::Return(v) => match self.machine.frames.pop() {
                None => Mode::Return(v),
                Some(Frame::Top {
                    rest: Value::Pair(r),
                    env,
                    outer,
                    ..
                }) => {
                    let (form, rest) = self.pair(r)?;
                    let at = match form {
                        Value::Pair(f) => f,
                        _ => Ref::NONE,
                    };
                    let top = Frame::Top {
                        rest,
                        env,
                        outer,
                        at,
                    };
                    push(&mut self.machine.frames, top)?;
                    self.machine.here = Ref::NONE;
                    Mode::Eval(form, env)
                }
                Some(Frame::Top { .. }) => Mode::Return(v),
                Some(Frame::Combine { operands, env, at }) => {
                    self.machine.here = at;
                    self.combine(v, operands, env)?
                }
                Some(Frame::Args {
                    comb,
                    env,
                    rest,
                    base,
                    at,
                }) => {
                    self.machine.here = at;
                    push(&mut self.machine.args, v)?;
                    self.next_arg(comb, env, rest, base)?
                }
                Some(Frame::Cleanup {
                    cleanup, env, at, ..
                }) => {
                    self.machine.here = at;
                    push(&mut self.machine.frames, Frame::Deliver(v))?;
                    self.call_thunk(cleanup, env, at)?
                }
                Some(Frame::Guard { at, .. }) => {
                    self.machine.here = at;
                    Mode::Return(v)
                }
                Some(Frame::Deliver(v)) => Mode::Return(v),
                Some(Frame::Raise) => {
                    // The error already says where it happened, or that it can't: not here.
                    self.machine.here = Ref::NONE;
                    return Err(self.machine.raised.pop().unwrap_or_else(Error::oom));
                }
            },
            Mode::Host(..) => return Err(Error::new(K::NotACombiner, &[])),
        })
    }

    /// An error stops the evaluation at `here`. Frames above `floor` are dropped down to the
    /// innermost cleanup or guard, adding where each was to the error. A cleanup then runs, the
    /// error waiting until it is done; a guard calls its handler with the error instead. With
    /// neither, the error goes to the caller. An error in a cleanup replaces the one waiting.
    fn unwind(&mut self, mut e: Error, floor: usize) -> Result<Mode, Error> {
        locate(&mut e, self.machine.here);
        while self.machine.frames.len() > floor {
            let Some(frame) = self.machine.frames.pop() else {
                break;
            };
            match frame {
                Frame::Cleanup {
                    cleanup,
                    env,
                    args,
                    at,
                } => {
                    locate(&mut e, at);
                    self.machine.args.truncate(args);
                    self.machine.here = at;
                    push(&mut self.machine.raised, e)?;
                    push(&mut self.machine.frames, Frame::Raise)?;
                    return self.call_thunk(cleanup, env, at);
                }
                // The host's limits hold: an interrupt or the allocator's refusal isn't caught.
                Frame::Guard {
                    handler,
                    env,
                    args,
                    at,
                } if e.code != K::Interrupted.into() && e.code != K::OutOfMemory.into() => {
                    self.machine.args.truncate(args);
                    self.machine.here = at;
                    match self.catch(&e, handler, env, args, at) {
                        Ok(mode) => return Ok(mode),
                        Err(oom) => e = oom,
                    }
                }
                Frame::Raise => drop(self.machine.raised.pop()),
                frame => locate(&mut e, frame.at()),
            }
        }
        Err(e)
    }

    /// Calls a guard's handler with the error as the value `(code values at)`.
    fn catch(
        &mut self,
        e: &Error,
        handler: Value,
        env: Ref,
        base: usize,
        at: Ref,
    ) -> Result<Mode, Error> {
        let (values, chain) = (self.list(&e.values)?, self.list(&e.at)?);
        let v = self.list(&[Value::Sym(e.code), values, chain])?;
        let comb = match handler {
            Value::App(r) => self.app(r)?,
            h => h,
        };
        let rest = Value::Nil;
        push(
            &mut self.machine.frames,
            Frame::Args {
                comb,
                env,
                rest,
                base,
                at,
            },
        )?;
        Ok(Mode::Return(v))
    }

    fn collect(&mut self, mode: Mode) -> Result<(), Error> {
        let current = match mode {
            Mode::Eval(x, env) => [x, Value::Env(env)],
            Mode::Return(v) => [v, Value::Nil],
            Mode::Host(..) => [Value::Nil, Value::Nil],
        };
        let m = &self.machine;
        let raised = m
            .raised
            .iter()
            .flat_map(|e| e.values.iter().chain(&e.at).copied());
        let roots = m
            .frames
            .iter()
            .flat_map(|f| f.values())
            .chain(m.args.iter().copied())
            .chain(raised);
        let roots = roots.chain(current).chain([
            Value::Pair(m.here),
            Value::Env(self.ground),
            Value::Env(self.top),
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
        let given = self.machine.args.get(base..).unwrap_or_default();
        let mut args = Vec::new();
        args.try_reserve_exact(given.len())
            .map_err(|_| Error::oom())?;
        args.extend_from_slice(given);
        let v = f(self, state, &args);
        self.machine.args.truncate(base);
        Ok(Mode::Return(v?))
    }

    /// Combines a combiner with unevaluated operands.
    fn combine(&mut self, f: Value, operands: Value, env: Ref) -> Result<Mode, Error> {
        match f {
            Value::App(r) => {
                let mut inner = self.app(r)?;
                // With no operands, every layer of wrapping evaluates nothing: peel them here
                // rather than recursing once per layer.
                while let (Value::App(r), Value::Nil) = (inner, operands) {
                    inner = self.app(r)?;
                }
                self.next_arg(inner, env, operands, self.machine.args.len())
            }
            Value::Op(r) => self.call_compound(r, operands, env),
            Value::Prim(_) | Value::Host(_) => {
                let base = self.machine.args.len();
                let mut rest = operands;
                while let Value::Pair(r) = rest {
                    let (x, next) = self.pair(r)?;
                    push(&mut self.machine.args, x)?;
                    rest = next;
                }
                if rest != Value::Nil {
                    return Err(Error::new(K::TypeError, &[f, operands]));
                }
                self.apply(f, env, base)
            }
            _ => Err(Error::new(K::NotACombiner, &[f])),
        }
    }

    /// Evaluates the next operand of an applicative, or applies its combiner once none is left.
    /// Operands that are symbols or constants are evaluated here, without a step of their own.
    fn next_arg(&mut self, comb: Value, env: Ref, rest: Value, base: usize) -> Result<Mode, Error> {
        let mut rest = rest;
        while let Value::Pair(r) = rest {
            let (x, next) = self.pair(r)?;
            match self.at_once(x, env)? {
                Some(v) => push(&mut self.machine.args, v)?,
                None => break,
            }
            rest = next;
        }
        match rest {
            Value::Pair(r) => {
                let (x, rest) = self.pair(r)?;
                let at = self.machine.here;
                push(
                    &mut self.machine.frames,
                    Frame::Args {
                        comb,
                        env,
                        rest,
                        base,
                        at,
                    },
                )?;
                Ok(Mode::Eval(x, env))
            }
            Value::Nil => self.apply(comb, env, base),
            _ => Err(Error::new(K::TypeError, &[comb, rest])),
        }
    }

    /// The value of `x` in `env` if it is a symbol or evaluates to itself; `None` for a
    /// combination.
    #[inline(always)]
    fn at_once(&self, x: Value, env: Ref) -> Result<Option<Value>, Error> {
        match x {
            Value::Pair(_) => Ok(None),
            Value::Sym(s) => self.lookup(env, s).map(Some),
            x => Ok(Some(x)),
        }
    }

    /// Applies a combiner to the arguments on the argument stack from `base`, popping them.
    fn apply(&mut self, comb: Value, env: Ref, base: usize) -> Result<Mode, Error> {
        match comb {
            Value::Prim(p) => {
                let mode = self.prim(p, env, base);
                self.machine.args.truncate(base);
                mode
            }
            Value::Host(h) => Ok(Mode::Host(h, base)),
            _ => {
                let mut list = Value::Nil;
                while self.machine.args.len() > base {
                    let x = self.machine.args.pop().unwrap_or(Value::Nil);
                    list = self.cons(x, list)?;
                }
                self.combine(comb, list, env)
            }
        }
    }

    /// Calls a compound operative: a child of its static environment, with the formals bound
    /// to the operands and the environment formal to the caller's environment.
    fn call_compound(&mut self, r: Ref, operands: Value, env: Ref) -> Result<Mode, Error> {
        let op = match self.heap.get(r)? {
            Obj::Op(op) => *op,
            _ => return Err(Error::new(K::StaleValue, &[Value::Op(r)])),
        };
        let local = self.heap.alloc(Obj::Env(Some(op.env), Vec::new()))?;
        self.match_tree(local, op.formals, operands)?;
        if let Value::Sym(s) = op.eformal {
            self.bind_symbol(local, s, Value::Env(env))?;
        }
        Ok(Mode::Eval(op.body, local))
    }

    fn prim(&mut self, p: Prim, env: Ref, base: usize) -> Result<Mode, Error> {
        let given = self.machine.args.get(base..).unwrap_or_default();
        let (lo, hi) = p.arity();
        if given.len() < lo || given.len() > hi {
            let mut values = Vec::new();
            values
                .try_reserve_exact(given.len() + 1)
                .map_err(|_| Error::oom())?;
            values.push(Value::Prim(p));
            values.extend_from_slice(given);
            return Err(Error {
                code: K::ArityError.into(),
                values,
                at: Vec::new(),
            });
        }
        let n = given.len();
        let mut a = [Value::Nil; 3];
        a.iter_mut().zip(given).for_each(|(d, s)| *d = *s);
        let [x, y, z] = a;
        let v = match p {
            Prim::Vau => {
                self.check_formals(x, y)?;
                let op = Op {
                    formals: x,
                    eformal: y,
                    body: z,
                    env,
                };
                Value::Op(self.heap.alloc(Obj::Op(op))?)
            }
            Prim::Wrap => match x {
                Value::Op(_) | Value::App(_) | Value::Prim(_) | Value::Host(_) => {
                    Value::App(self.heap.alloc(Obj::App(x))?)
                }
                _ => return Err(type_error(p, x)),
            },
            Prim::Unwrap => match x {
                Value::App(r) => self.app(r)?,
                _ => return Err(type_error(p, x)),
            },
            Prim::Eval => match y {
                Value::Env(e) => return Ok(Mode::Eval(x, e)),
                _ => return Err(type_error(p, y)),
            },
            Prim::MakeEnvironment => {
                let parent = match x {
                    Value::Env(e) if n == 1 => Some(e),
                    _ if n == 0 => None,
                    _ => return Err(type_error(p, x)),
                };
                Value::Env(self.heap.alloc(Obj::Env(parent, Vec::new()))?)
            }
            Prim::Bind => match x {
                Value::Env(e) => {
                    self.check_formals(y, Value::Ignore)?;
                    self.match_tree(e, y, z)?;
                    Value::Inert
                }
                _ => return Err(type_error(p, x)),
            },
            Prim::Bindings => match x {
                Value::Env(e) => self.bindings(e)?,
                _ => return Err(type_error(p, x)),
            },
            Prim::Cons => self.cons(x, y)?,
            Prim::Car | Prim::Cdr => match x {
                Value::Pair(r) => {
                    let (a, d) = self.pair(r)?;
                    if p == Prim::Car { a } else { d }
                }
                _ => return Err(type_error(p, x)),
            },
            Prim::Eq => boolean(match (x, y) {
                (Value::Str(_), Value::Str(_)) => self.string(x) == self.string(y),
                _ => x == y,
            }),
            Prim::Select => match x {
                Value::True => y,
                Value::False => z,
                _ => return Err(type_error(p, x)),
            },
            Prim::TypeOf => Value::Sym(type_of(x).into()),
            Prim::Add | Prim::Sub | Prim::Mul | Prim::Div | Prim::Mod | Prim::Less => {
                arith(p, x, y)?
            }
            Prim::Error => {
                let Value::Sym(code) = x else {
                    return Err(type_error(p, x));
                };
                let mut values = Vec::new();
                let rest = self.machine.args.get(base + 1..).unwrap_or_default();
                values
                    .try_reserve_exact(rest.len())
                    .map_err(|_| Error::oom())?;
                values.extend_from_slice(rest);
                return Err(Error {
                    code,
                    values,
                    at: Vec::new(),
                });
            }
            Prim::EvalProtected if !is_combiner(z) => return Err(type_error(p, z)),
            Prim::EvalProtected => match y {
                Value::Env(e) => {
                    let (args, at) = (base, self.machine.here);
                    let cleanup = Frame::Cleanup {
                        cleanup: z,
                        env: e,
                        args,
                        at,
                    };
                    push(&mut self.machine.frames, cleanup)?;
                    return Ok(Mode::Eval(x, e));
                }
                _ => return Err(type_error(p, y)),
            },
            Prim::Guard => {
                if let Some(&v) = [x, y].iter().find(|&&v| !is_combiner(v)) {
                    return Err(type_error(p, v));
                }
                let at = self.machine.here;
                let guard = Frame::Guard {
                    handler: y,
                    env,
                    args: base,
                    at,
                };
                push(&mut self.machine.frames, guard)?;
                return self.call_thunk(x, env, at);
            }
            Prim::Raise => return Err(self.caught(x)?),
            Prim::StringLength => {
                Value::Int(i64::try_from(self.text(p, x)?.len()).unwrap_or(i64::MAX))
            }
            Prim::StringByte => {
                let (s, i) = (self.text(p, x)?, int_of(p, y)?);
                let byte = usize::try_from(i).ok().and_then(|i| s.as_bytes().get(i));
                let byte =
                    byte.ok_or_else(|| Error::new(K::RangeError, &[Value::Prim(p), x, y]))?;
                Value::Int(i64::from(*byte))
            }
            Prim::Substring => {
                let range = (
                    usize::try_from(int_of(p, y)?),
                    usize::try_from(int_of(p, z)?),
                );
                let part = match range {
                    (Ok(a), Ok(b)) => self.text(p, x)?.get(a..b),
                    _ => None,
                };
                let part =
                    part.ok_or_else(|| Error::new(K::RangeError, &[Value::Prim(p), x, y, z]))?;
                let part = boxed(part)?;
                Value::Str(self.heap.alloc(Obj::Str(part))?)
            }
            Prim::StringAppend => {
                let given = self.machine.args.get(base..).unwrap_or_default();
                let mut s = String::new();
                for &v in given {
                    let t = self.text(p, v)?;
                    s.try_reserve(t.len()).map_err(|_| Error::oom())?;
                    s.push_str(t);
                }
                Value::Str(self.heap.alloc(Obj::Str(s.into_boxed_str()))?)
            }
            Prim::CodePointToString => {
                let c = u32::try_from(int_of(p, x)?).ok().and_then(char::from_u32);
                let c = c.ok_or_else(|| Error::new(K::RangeError, &[Value::Prim(p), x]))?;
                let s = boxed(c.encode_utf8(&mut [0; 4]))?;
                Value::Str(self.heap.alloc(Obj::Str(s))?)
            }
            Prim::SymbolToString => match x {
                Value::Sym(s) => {
                    let name = boxed(self.names.get(s.0 as usize).map_or("", |n| n))?;
                    Value::Str(self.heap.alloc(Obj::Str(name))?)
                }
                _ => return Err(type_error(p, x)),
            },
            Prim::StringToSymbol => {
                let name = boxed(self.text(p, x)?)?;
                Value::Sym(self.symbol(&name)?)
            }
            Prim::NumberToString => {
                let mut s = String::new();
                s.try_reserve(20).map_err(|_| Error::oom())?;
                // An i64 has at most 20 characters, so this never grows the string.
                write!(s, "{}", int_of(p, x)?).map_err(|_| Error::oom())?;
                Value::Str(self.heap.alloc(Obj::Str(s.into_boxed_str()))?)
            }
        };
        Ok(Mode::Return(v))
    }

    /// Calls combiner `f` with no arguments in `env`.
    fn call_thunk(&mut self, f: Value, env: Ref, at: Ref) -> Result<Mode, Error> {
        let operands = Value::Nil;
        push(
            &mut self.machine.frames,
            Frame::Combine { operands, env, at },
        )?;
        Ok(Mode::Return(f))
    }

    /// The error a guard caught, from the list `(code values at)` its handler got, as it was.
    fn caught(&self, e: Value) -> Result<Error, Error> {
        let bad = || type_error(Prim::Raise, e);
        let mut parts = [Value::Nil; 3];
        let mut rest = e;
        for part in &mut parts {
            (*part, rest) = self.uncons(rest).ok_or_else(bad)?;
        }
        let ([Value::Sym(code), values, at], Value::Nil) = (parts, rest) else {
            return Err(bad());
        };
        let (values, at) = (self.items(values, bad)?, self.items(at, bad)?);
        Ok(Error { code, values, at })
    }

    /// The elements of a proper list; `bad()` for anything else.
    fn items(&self, mut list: Value, bad: impl Fn() -> Error) -> Result<Vec<Value>, Error> {
        let mut v = Vec::new();
        while let Some((x, rest)) = self.uncons(list) {
            push(&mut v, x)?;
            list = rest;
        }
        if list == Value::Nil {
            Ok(v)
        } else {
            Err(bad())
        }
    }

    /// The contents of string `v`, or a type error naming `p`.
    fn text(&self, p: Prim, v: Value) -> Result<&str, Error> {
        self.string(v).ok_or_else(|| type_error(p, v))
    }

    /// A new list of `xs`.
    fn list(&mut self, xs: &[Value]) -> Result<Value, Error> {
        xs.iter()
            .rev()
            .try_fold(Value::Nil, |l, &x| self.cons(x, l))
    }

    /// Checks a formals tree: symbols, `#ignore`, `()` and pairs, no symbol twice, and an
    /// environment formal that is `#ignore` or a symbol not in the tree.
    fn check_formals(&mut self, formals: Value, eformal: Value) -> Result<(), Error> {
        let bad = |v: Value| Error::new(K::BadFormals, &[v]);
        let mut seen: Vec<Symbol> = Vec::new();
        let mut todo = Vec::new();
        push(&mut todo, formals)?;
        while let Some(t) = todo.pop() {
            match t {
                Value::Sym(s) if seen.contains(&s) => return Err(bad(formals)),
                Value::Sym(s) => push(&mut seen, s)?,
                Value::Ignore | Value::Nil => {}
                Value::Pair(r) => {
                    let (a, d) = self.pair(r)?;
                    push(&mut todo, d)?;
                    push(&mut todo, a)?;
                }
                _ => return Err(bad(formals)),
            }
        }
        match eformal {
            Value::Sym(s) if !seen.contains(&s) => Ok(()),
            Value::Ignore => Ok(()),
            _ => Err(bad(eformal)),
        }
    }

    /// Matches a checked formals tree to a value, binding its symbols in `env`.
    fn match_tree(&mut self, env: Ref, formals: Value, value: Value) -> Result<(), Error> {
        let mut work = std::mem::take(&mut self.machine.work);
        work.clear();
        let result = self.match_with(&mut work, env, formals, value);
        self.machine.work = work;
        result
    }

    fn match_with(
        &mut self,
        work: &mut Vec<(Value, Value)>,
        env: Ref,
        formals: Value,
        value: Value,
    ) -> Result<(), Error> {
        let mismatch = || Error::new(K::ArityError, &[formals, value]);
        push(work, (formals, value))?;
        while let Some((t, v)) = work.pop() {
            match (t, v) {
                (Value::Sym(s), _) => self.bind_symbol(env, s, v)?,
                (Value::Ignore, _) | (Value::Nil, Value::Nil) => {}
                (Value::Pair(t), Value::Pair(v)) => {
                    let ((ta, td), (va, vd)) = (self.pair(t)?, self.pair(v)?);
                    push(work, (td, vd))?;
                    push(work, (ta, va))?;
                }
                _ => return Err(mismatch()),
            }
        }
        Ok(())
    }

    /// Binds `s` in `env` itself, replacing an existing binding there.
    pub(crate) fn bind_symbol(&mut self, env: Ref, s: Symbol, v: Value) -> Result<(), Error> {
        let Obj::Env(_, binds) = self.heap.get_mut(env)? else {
            return Err(Error::new(K::StaleValue, &[Value::Env(env)]));
        };
        match binds.iter_mut().find(|(k, _)| *k == s) {
            Some(b) => b.1 = v,
            None => push(binds, (s, v))?,
        }
        Ok(())
    }

    fn lookup(&self, mut env: Ref, s: Symbol) -> Result<Value, Error> {
        loop {
            let Obj::Env(parent, binds) = self.heap.get(env)? else {
                return Err(Error::new(K::StaleValue, &[Value::Env(env)]));
            };
            if let Some(&(_, v)) = binds.iter().find(|(k, _)| *k == s) {
                return Ok(v);
            }
            env = parent.ok_or_else(|| Error::new(K::UnboundSymbol, &[Value::Sym(s)]))?;
        }
    }

    /// The symbols bound in `env` itself, in definition order.
    fn bindings(&mut self, env: Ref) -> Result<Value, Error> {
        let mut list = Value::Nil;
        let mut i = usize::MAX;
        loop {
            let Obj::Env(_, binds) = self.heap.get(env)? else {
                return Err(Error::new(K::StaleValue, &[Value::Env(env)]));
            };
            i = i.min(binds.len());
            let Some(prev) = i.checked_sub(1) else {
                return Ok(list);
            };
            let Some(&(s, _)) = binds.get(prev) else {
                return Ok(list);
            };
            i = prev;
            list = self.cons(Value::Sym(s), list)?;
        }
    }

    pub(crate) fn pair(&self, r: Ref) -> Result<(Value, Value), Error> {
        match self.heap.get(r)? {
            Obj::Pair(a, d) => Ok((*a, *d)),
            _ => Err(Error::new(K::StaleValue, &[Value::Pair(r)])),
        }
    }

    pub(crate) fn app(&self, r: Ref) -> Result<Value, Error> {
        match self.heap.get(r)? {
            Obj::App(c) => Ok(*c),
            _ => Err(Error::new(K::StaleValue, &[Value::App(r)])),
        }
    }
}

/// Adds program combination `r` to the error's locations, unless it is there already. With no
/// memory to spare the list stays as it is: the error matters more than its last location.
fn locate(e: &mut Error, r: Ref) {
    let v = Value::Pair(r);
    if r != Ref::NONE && !e.at.contains(&v) && e.at.try_reserve(1).is_ok() {
        e.at.push(v);
    }
}

fn int_of(p: Prim, v: Value) -> Result<i64, Error> {
    match v {
        Value::Int(n) => Ok(n),
        _ => Err(type_error(p, v)),
    }
}

#[inline]
fn is_combiner(v: Value) -> bool {
    matches!(
        v,
        Value::Op(_) | Value::App(_) | Value::Prim(_) | Value::Host(_)
    )
}

fn type_of(v: Value) -> K {
    match v {
        Value::Nil => K::Null,
        Value::True | Value::False => K::Boolean,
        Value::Inert => K::Inert,
        Value::Ignore => K::Ignore,
        Value::Int(_) => K::Integer,
        Value::Sym(_) => K::Symbol,
        Value::Pair(_) => K::Pair,
        Value::Str(_) => K::String,
        Value::Env(_) => K::Environment,
        Value::Op(_) | Value::Prim(_) | Value::Host(_) => K::Operative,
        Value::App(_) => K::Applicative,
    }
}

/// Checked integer arithmetic. `div` rounds toward negative infinity; `mod` is what it leaves,
/// and never overflows: `(mod -9223372036854775808 -1)` is 0.
#[inline]
fn arith(p: Prim, x: Value, y: Value) -> Result<Value, Error> {
    let (Value::Int(a), Value::Int(b)) = (x, y) else {
        return Err(type_error(
            p,
            if matches!(x, Value::Int(_)) { y } else { x },
        ));
    };
    let r = match p {
        Prim::Add => a.checked_add(b),
        Prim::Sub => a.checked_sub(b),
        Prim::Mul => a.checked_mul(b),
        Prim::Div | Prim::Mod if b == 0 => return Err(Error::new(K::DivisionByZero, &[x, y])),
        Prim::Div => a.checked_div(b).and_then(|q| match a.checked_rem(b) {
            Some(r) if r != 0 && (r < 0) != (b < 0) => q.checked_sub(1),
            _ => Some(q),
        }),
        // A remainder with the sign of b: r and b then have opposite signs, so r + b fits.
        Prim::Mod => Some(match a.wrapping_rem(b) {
            r if r != 0 && (r < 0) != (b < 0) => r + b,
            r => r,
        }),
        _ => return Ok(boolean(a < b)),
    };
    r.map(Value::Int)
        .ok_or_else(|| Error::new(K::Overflow, &[Value::Prim(p), x, y]))
}
