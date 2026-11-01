//! The evaluator: an explicit machine loop over a frame stack, and the Rust primitives.
//!
//! The machine's state is its frame stack, its argument stack, the errors waiting for a
//! cleanup to finish, where it is in the program and the current [`Mode`]. The collector runs
//! only at the top of the loop, with exactly that state (plus the ground and top-level
//! environments) as its roots. Between two safe points Rust code may hold values in locals
//! freely: nothing is freed until the next safe point, and by then every local is gone. The
//! same safe point checks the host's interrupt flag.

use crate::heap::{Obj, Op};
use crate::{Error, Grift, Host, K, Ref, Symbol, Value, push};
use alloc::vec::Vec;

/// The Rust primitives. All but `$vau` are bound wrapped, as applicatives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prim {
    Vau,
    Wrap,
    Unwrap,
    Eval,
    MakeEnvironment,
    Bind,
    Bindings,
    Cons,
    Car,
    Cdr,
    Eq,
    Select,
    TypeOf,
    Add,
    Sub,
    Mul,
    Div,
    Less,
    Error,
    EvalProtected,
}

impl Prim {
    pub(crate) const ALL: [Prim; 20] = [
        Prim::Vau,
        Prim::Wrap,
        Prim::Unwrap,
        Prim::Eval,
        Prim::MakeEnvironment,
        Prim::Bind,
        Prim::Bindings,
        Prim::Cons,
        Prim::Car,
        Prim::Cdr,
        Prim::Eq,
        Prim::Select,
        Prim::TypeOf,
        Prim::Add,
        Prim::Sub,
        Prim::Mul,
        Prim::Div,
        Prim::Less,
        Prim::Error,
        Prim::EvalProtected,
    ];

    /// The name the primitive is bound to in the ground environment.
    pub fn name(self) -> &'static str {
        match self {
            Prim::Vau => "$vau",
            Prim::Wrap => "wrap",
            Prim::Unwrap => "unwrap",
            Prim::Eval => "eval",
            Prim::MakeEnvironment => "make-environment",
            Prim::Bind => "bind!",
            Prim::Bindings => "bindings",
            Prim::Cons => "cons",
            Prim::Car => "car",
            Prim::Cdr => "cdr",
            Prim::Eq => "eq?",
            Prim::Select => "select",
            Prim::TypeOf => "type-of",
            Prim::Add => "+",
            Prim::Sub => "-",
            Prim::Mul => "*",
            Prim::Div => "div",
            Prim::Less => "<?",
            Prim::Error => "error",
            Prim::EvalProtected => "eval-protected",
        }
    }

    /// The fewest and most operands.
    fn arity(self) -> (usize, usize) {
        match self {
            Prim::MakeEnvironment => (0, 1),
            Prim::Wrap | Prim::Unwrap | Prim::Bindings | Prim::Car | Prim::Cdr | Prim::TypeOf => {
                (1, 1)
            }
            Prim::Error => (1, usize::MAX),
            Prim::Vau | Prim::Bind | Prim::Select | Prim::EvalProtected => (3, 3),
            _ => (2, 2),
        }
    }
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
    Top { rest: Value, env: Ref },
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
    /// When the evaluation above ends, by a value or an error, evaluate `cleanup` in `env`;
    /// `args` is the argument stack's height to go back to.
    Cleanup {
        cleanup: Value,
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
            Frame::Top { rest, env } => [rest, Value::Env(env), nil, nil],
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
            Frame::Deliver(v) => [v, nil, nil, nil],
            Frame::Raise => [nil; 4],
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
}

impl Default for Machine {
    fn default() -> Machine {
        Machine {
            frames: Vec::new(),
            args: Vec::new(),
            raised: Vec::new(),
            here: Ref::NONE,
            work: Vec::new(),
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
        let top = Frame::Top { rest: forms, env };
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
        let mut operands = Value::Nil;
        for &a in args.iter().rev() {
            operands = self.cons(a, operands)?;
        }
        let env = self.heap.alloc(Obj::Env(None, Vec::new()))?;
        let frames = [
            Frame::Top {
                rest: Value::Nil,
                env,
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
                let (combiner, operands) = self.pair(r)?;
                // A combination is the program's unless it is the library's source, or code
                // built around a combiner value (as the library builds it), not written.
                if !self.heap.in_library(r) && !is_combiner(combiner) {
                    self.machine.here = r;
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
                }) => {
                    let (form, rest) = self.pair(r)?;
                    push(&mut self.machine.frames, Frame::Top { rest, env })?;
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
                    Mode::Eval(cleanup, env)
                }
                Some(Frame::Deliver(v)) => Mode::Return(v),
                Some(Frame::Raise) => {
                    return Err(self.machine.raised.pop().unwrap_or_else(Error::oom));
                }
            },
            Mode::Host(..) => return Err(Error::new(K::NotACombiner, &[])),
        })
    }

    /// An error stops the evaluation at `here`. Frames above `floor` are dropped down to the
    /// innermost cleanup, which then runs, the error waiting until it is done; with none, the
    /// error goes to the caller. An error in a cleanup replaces the one waiting.
    fn unwind(&mut self, mut e: Error, floor: usize) -> Result<Mode, Error> {
        if e.at.is_none() && self.machine.here != Ref::NONE {
            e.at = Some(Value::Pair(self.machine.here));
        }
        while self.machine.frames.len() > floor {
            match self.machine.frames.pop() {
                Some(Frame::Cleanup {
                    cleanup,
                    env,
                    args,
                    at,
                }) => {
                    self.machine.args.truncate(args);
                    self.machine.here = at;
                    push(&mut self.machine.raised, e)?;
                    push(&mut self.machine.frames, Frame::Raise)?;
                    return Ok(Mode::Eval(cleanup, env));
                }
                Some(Frame::Raise) => drop(self.machine.raised.pop()),
                _ => {}
            }
        }
        Err(e)
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
            .flat_map(|e| e.values.iter().copied().chain(e.at));
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
    fn next_arg(&mut self, comb: Value, env: Ref, rest: Value, base: usize) -> Result<Mode, Error> {
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
                at: None,
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
            Prim::Add | Prim::Sub | Prim::Mul | Prim::Div | Prim::Less => arith(p, x, y)?,
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
                    at: None,
                });
            }
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
        };
        Ok(Mode::Return(v))
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
        let mut work = core::mem::take(&mut self.machine.work);
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

/// Checked integer arithmetic. `div` rounds toward negative infinity.
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
        Prim::Div if b == 0 => return Err(Error::new(K::DivisionByZero, &[x, y])),
        Prim::Div => a.checked_div(b).and_then(|q| match a.checked_rem(b) {
            Some(r) if r != 0 && (r < 0) != (b < 0) => q.checked_sub(1),
            _ => Some(q),
        }),
        _ => return Ok(boolean(a < b)),
    };
    r.map(Value::Int)
        .ok_or_else(|| Error::new(K::Overflow, &[Value::Prim(p), x, y]))
}
