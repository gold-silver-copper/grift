//! The evaluator: an explicit machine loop over a frame stack, and the Rust primitives.
//!
//! The machine's state is its frame stack, its argument stack and the current [`Mode`]. The
//! collector runs only at the top of the loop, with exactly that state (plus the ground and
//! top-level environments) as its roots. Between two safe points Rust code may hold values in
//! locals freely: nothing is freed until the next safe point, and by then every local is gone.

use crate::heap::{Obj, Op};
use crate::{Error, Grift, K, Ref, Symbol, Value, push};
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
}

impl Prim {
    pub(crate) const ALL: [Prim; 19] = [
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
            Prim::Vau | Prim::Bind | Prim::Select => (3, 3),
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
}

/// A continuation frame: what to do with the value being returned.
#[derive(Clone, Copy)]
enum Frame {
    /// Evaluate the remaining top-level forms of one `eval_in` call.
    Top { rest: Value, env: Ref },
    /// The value is the combiner of a combination with these operands.
    Combine { operands: Value, env: Ref },
    /// The value is an argument of `comb`; arguments so far are on the argument stack from
    /// `base`.
    Args {
        comb: Value,
        env: Ref,
        rest: Value,
        base: usize,
    },
}

impl Frame {
    fn values(self) -> [Value; 3] {
        match self {
            Frame::Top { rest, env } => [rest, Value::Env(env), Value::Nil],
            Frame::Combine { operands, env } => [operands, Value::Env(env), Value::Nil],
            Frame::Args {
                comb, env, rest, ..
            } => [comb, Value::Env(env), rest],
        }
    }
}

#[derive(Default)]
pub(crate) struct Machine {
    frames: Vec<Frame>,
    args: Vec<Value>,
    /// Scratch for matching formals trees.
    work: Vec<(Value, Value)>,
}

fn type_error(p: Prim, v: Value) -> Error {
    Error::new(K::TypeError, &[Value::Prim(p), v])
}

fn boolean(b: bool) -> Value {
    if b { Value::True } else { Value::False }
}

impl Grift {
    /// Reads `src`, then evaluates its forms in `env`. Reentrant: a host function may call
    /// `eval`, which runs on top of the current frames and leaves them as it found them.
    pub(crate) fn eval_in(&mut self, src: &str, env: Ref) -> Result<Value, Error> {
        let forms = self.read(src)?;
        let (frames, args) = (self.machine.frames.len(), self.machine.args.len());
        let result = push(&mut self.machine.frames, Frame::Top { rest: forms, env })
            .and_then(|()| self.run(Mode::Return(Value::Inert)));
        if result.is_err() {
            self.machine.frames.truncate(frames);
            self.machine.args.truncate(args);
        }
        result
    }

    fn run(&mut self, mut mode: Mode) -> Result<Value, Error> {
        loop {
            // The safe point: the machine's state is all there is.
            if self.heap.due() {
                self.collect(mode)?;
            }
            mode = match mode {
                Mode::Eval(Value::Sym(s), env) => Mode::Return(self.lookup(env, s)?),
                Mode::Eval(Value::Pair(r), env) => {
                    let (combiner, operands) = self.pair(r)?;
                    push(&mut self.machine.frames, Frame::Combine { operands, env })?;
                    Mode::Eval(combiner, env)
                }
                Mode::Eval(x, _) => Mode::Return(x),
                Mode::Return(v) => match self.machine.frames.pop() {
                    None => return Ok(v),
                    Some(Frame::Top {
                        rest: Value::Pair(r),
                        env,
                    }) => {
                        let (form, rest) = self.pair(r)?;
                        push(&mut self.machine.frames, Frame::Top { rest, env })?;
                        Mode::Eval(form, env)
                    }
                    Some(Frame::Top { .. }) => return Ok(v),
                    Some(Frame::Combine { operands, env }) => self.combine(v, operands, env)?,
                    Some(Frame::Args {
                        comb,
                        env,
                        rest,
                        base,
                    }) => {
                        push(&mut self.machine.args, v)?;
                        self.next_arg(comb, env, rest, base)?
                    }
                },
            };
        }
    }

    fn collect(&mut self, mode: Mode) -> Result<(), Error> {
        let current = match mode {
            Mode::Eval(x, env) => [x, Value::Env(env)],
            Mode::Return(v) => [v, Value::Nil],
        };
        let m = &self.machine;
        let roots = m
            .frames
            .iter()
            .flat_map(|f| f.values())
            .chain(m.args.iter().copied());
        let roots = roots
            .chain(current)
            .chain([Value::Env(self.ground), Value::Env(self.top)]);
        self.heap.collect(roots)
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
            Value::Op(r) => self.call(r, operands, env),
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
                push(
                    &mut self.machine.frames,
                    Frame::Args {
                        comb,
                        env,
                        rest,
                        base,
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
            Value::Host(h) => {
                let f = self.hosts.get(h.0 as usize).map(|&(_, f)| f);
                let f = f.ok_or_else(|| Error::new(K::NotACombiner, &[comb]))?;
                // The arguments stay on the argument stack, rooted, while the host runs.
                let given = self.machine.args.get(base..).unwrap_or_default();
                let mut args = Vec::new();
                args.try_reserve_exact(given.len())
                    .map_err(|_| Error::oom())?;
                args.extend_from_slice(given);
                let v = f(self, &args);
                self.machine.args.truncate(base);
                Ok(Mode::Return(v?))
            }
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
    fn call(&mut self, r: Ref, operands: Value, env: Ref) -> Result<Mode, Error> {
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
                return Err(Error { code, values });
            }
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

    pub(crate) fn cons(&mut self, a: Value, d: Value) -> Result<Value, Error> {
        Ok(Value::Pair(self.heap.alloc(Obj::Pair(a, d))?))
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
