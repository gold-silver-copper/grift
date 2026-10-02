//! The evaluator: an explicit machine loop over heap-allocated frames.
//!
//! No Rust recursion grows with program depth. A tail call pushes no frame:
//! applying a combiner replaces the control register, so `eval` in tail
//! position of an operative body (and therefore every library form built
//! that way) is properly tail-recursive. Garbage is collected only at the
//! top of the loop, where every live value is reachable from the registers,
//! the frame chain and the roots, so primitives never observe a collection.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::ctx::Ctx;
use crate::error::{Error, Fault, GResult};
use crate::heap::{Frame, FrameKind, Object, Operative, SubCont};
use crate::prims;
use crate::value::{ObjId, PrimId, SymId, Value};

/// The control register.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Ctl {
    /// Evaluate an expression in an environment.
    Eval(Value, ObjId),
    /// Return a value to the current frame.
    Ret(Value),
    /// Apply a combiner to an operand tree.
    Apply(Value, Value, ObjId),
    /// Stop: the payload of an error that reached the host's prompt.
    HostErr(Value),
}

/// The machine: store, registers and roots.
#[derive(Clone, Debug)]
pub(crate) struct Machine {
    pub(crate) ctx: Ctx,
    /// The current continuation (top frame).
    pub(crate) k: Option<ObjId>,
    /// Values pinned by the host.
    pub(crate) pinned: Vec<Value>,
    /// Environments the host holds on to (ground, current top level, ...).
    pub(crate) env_roots: Vec<ObjId>,
    /// The environment holding the Rust primitives.
    pub(crate) prim_env: ObjId,
    /// The pre-made prompt for errors.
    pub(crate) error_prompt: ObjId,
    /// Scratch buffer for evaluated arguments.
    argbuf: Vec<Value>,
    /// Scratch buffer for parameter-tree bindings.
    bindbuf: Vec<(SymId, Value)>,
    /// Collect at the next safe point regardless of allocation volume.
    pub(crate) force_gc: bool,
    /// Machine steps taken (only counted with the `stats` feature).
    pub(crate) steps: u64,
}

impl Machine {
    pub(crate) fn new() -> Self {
        let mut ctx = Ctx::default();
        let prim_env = ctx.empty_env();
        let error_prompt = ctx.heap.alloc(Object::Prompt);
        let mut m = Machine {
            ctx,
            k: None,
            pinned: Vec::new(),
            env_roots: Vec::new(),
            prim_env,
            error_prompt,
            argbuf: Vec::new(),
            bindbuf: Vec::new(),
            force_gc: false,
            steps: 0,
        };
        m.prim_env = prims::install(&mut m);
        m
    }

    /// Run the machine from `ctl` until it returns, under a host error
    /// prompt.
    pub(crate) fn run(&mut self, ctl: Ctl) -> Result<Value, Error> {
        let host = self.push_frame(FrameKind::Prompt {
            prompt: self.error_prompt,
            host: true,
        });
        debug_assert_eq!(self.ctx.heap.frame(host).parent, None);
        let mut ctl = ctl;
        loop {
            #[cfg(feature = "stats")]
            {
                self.steps += 1;
            }
            if self.ctx.heap.since_gc >= self.ctx.heap.threshold || self.force_gc {
                self.force_gc = false;
                self.collect(ctl);
            }
            let step = match ctl {
                Ctl::Ret(v) => match self.k {
                    None => return Ok(v),
                    Some(f) => self.ret(f, v),
                },
                Ctl::Eval(e, env) => self.eval(e, env),
                Ctl::Apply(c, ops, env) => self.apply(c, ops, env),
                Ctl::HostErr(payload) => {
                    self.k = None;
                    return Err(self.decode_payload(payload));
                }
            };
            ctl = match step {
                Ok(c) => c,
                Err(fault) => self.raise(fault)?,
            };
        }
    }

    fn collect(&mut self, ctl: Ctl) {
        let mut roots: Vec<Value> = Vec::with_capacity(self.pinned.len() + 8);
        match ctl {
            Ctl::Eval(e, env) => {
                roots.push(e);
                roots.push(Value::Obj(env));
            }
            Ctl::Ret(v) | Ctl::HostErr(v) => roots.push(v),
            Ctl::Apply(c, ops, env) => {
                roots.push(c);
                roots.push(ops);
                roots.push(Value::Obj(env));
            }
        }
        roots.extend(self.k.map(Value::Obj));
        roots.extend(self.pinned.iter().copied());
        roots.extend(self.env_roots.iter().map(|&e| Value::Obj(e)));
        roots.push(Value::Obj(self.prim_env));
        roots.push(Value::Obj(self.error_prompt));
        self.ctx.heap.collect(roots);
    }

    pub(crate) fn push_frame(&mut self, kind: FrameKind) -> ObjId {
        let f = self.ctx.heap.alloc(Object::Frame(Frame {
            parent: self.k,
            kind,
        }));
        self.k = Some(f);
        f
    }

    /// Deliver a fault to the innermost error prompt.
    fn raise(&mut self, fault: Fault) -> Result<Ctl, Error> {
        let Some((prompt_frame, host)) = self.find_prompt(self.error_prompt) else {
            self.k = None;
            return Err(self.fault_to_error(&fault));
        };
        if host {
            self.k = None;
            return Err(self.fault_to_error(&fault));
        }
        let top = self.k.expect("a prompt frame was found");
        let sc = self.ctx.heap.alloc(Object::SubCont(SubCont {
            top,
            bottom: prompt_frame,
        }));
        let kind = self.ctx.symbol(&fault.kind);
        let message = self.ctx.string(&fault.message);
        let irritants = self.ctx.list(&fault.irritants);
        let payload = self.ctx.list(&[Value::Obj(sc), kind, message, irritants]);
        self.k = self.ctx.heap.frame(prompt_frame).parent;
        Ok(Ctl::Ret(payload))
    }

    /// Find the innermost frame delimited by `prompt`.
    pub(crate) fn find_prompt(&self, prompt: ObjId) -> Option<(ObjId, bool)> {
        let mut cur = self.k;
        while let Some(f) = cur {
            let frame = self.ctx.heap.frame(f);
            if let FrameKind::Prompt { prompt: p, host } = frame.kind
                && p == prompt
            {
                return Some((f, host));
            }
            cur = frame.parent;
        }
        None
    }

    pub(crate) fn fault_to_error(&self, fault: &Fault) -> Error {
        Error {
            kind: fault.kind.clone().into_owned(),
            message: fault.message.clone(),
            irritants: fault.irritants.iter().map(|&v| self.ctx.write(v)).collect(),
        }
    }

    /// Decode an error payload `(k kind message irritants)` that reached the
    /// host. Anything else delivered to the host's error prompt is reported
    /// as a `raise` of that value.
    fn decode_payload(&self, payload: Value) -> Error {
        let decoded = self.ctx.list_to_vec(payload).and_then(|items| {
            let [_, kind, message, irritants] = items.as_slice() else {
                return None;
            };
            Some(Error {
                kind: self.ctx.symbol_name(*kind)?.into(),
                message: self.ctx.as_str(*message)?.into(),
                irritants: self
                    .ctx
                    .list_to_vec(*irritants)?
                    .iter()
                    .map(|&v| self.ctx.write(v))
                    .collect(),
            })
        });
        decoded.unwrap_or_else(|| Error {
            kind: "raise".into(),
            message: "a non-error value reached the top level".into(),
            irritants: alloc::vec![self.ctx.write(payload)],
        })
    }

    fn eval(&mut self, expr: Value, env: ObjId) -> GResult<Ctl> {
        match expr {
            Value::Sym(s) => Ok(Ctl::Ret(self.ctx.lookup(env, s)?)),
            Value::Obj(id) => match *self.ctx.heap.get(id) {
                Object::Pair(head, operands) => match head {
                    Value::Sym(s) => {
                        let c = self.ctx.lookup(env, s)?;
                        self.check_sigil(s, c)?;
                        Ok(Ctl::Apply(c, operands, env))
                    }
                    _ if self.ctx.is_pair(head) => {
                        self.push_frame(FrameKind::Combine { operands, env });
                        Ok(Ctl::Eval(head, env))
                    }
                    // Any other head (e.g. a combiner placed into a
                    // constructed combination) evaluates to itself.
                    _ => Ok(Ctl::Apply(head, operands, env)),
                },
                _ => Ok(Ctl::Ret(expr)),
            },
            _ => Ok(Ctl::Ret(expr)),
        }
    }

    /// The sigil rule: a `$`-prefixed head symbol must name an operative,
    /// any other head symbol must not.
    fn check_sigil(&self, s: SymId, c: Value) -> GResult<()> {
        let dollar = self.ctx.syms.is_dollar(s);
        let operative = self.ctx.is_operative(c);
        if dollar == operative {
            return Ok(());
        }
        let name = self.ctx.syms.name(s);
        let msg = if dollar {
            format!(
                "{name} is called as an operative but is bound to {}",
                self.describe(c)
            )
        } else {
            format!("{name} is bound to an operative; operative names must start with $")
        };
        Err(Fault::new("sigil-violation", msg).with(Value::Sym(s)))
    }

    fn describe(&self, v: Value) -> String {
        let name = self.ctx.type_name(v);
        let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        format!("{article} {name}")
    }

    /// Evaluate an operand that is not a combination.
    #[inline]
    fn eval_simple(&self, expr: Value, env: ObjId) -> GResult<Value> {
        match expr {
            Value::Sym(s) => self.ctx.lookup(env, s),
            _ => Ok(expr),
        }
    }

    fn apply(&mut self, c: Value, operands: Value, env: ObjId) -> GResult<Ctl> {
        match c {
            Value::Prim(id) => {
                if id == prims::VAU {
                    return prims::vau(self, operands, env);
                }
                let mut buf = core::mem::take(&mut self.argbuf);
                buf.clear();
                let ok = self.ctx.list_into(operands, &mut buf);
                let r = if ok {
                    prims::call(self, id, Value::Nil, &buf, env)
                } else {
                    Err(self.ctx.type_error("a proper list of operands", operands))
                };
                self.argbuf = buf;
                r
            }
            Value::Obj(id) => match *self.ctx.heap.get(id) {
                Object::Operative(op) => self.enter_operative(op, operands, None, env),
                Object::Applicative(inner) => self.eval_operands(inner, operands, env),
                Object::PrimData(..) | Object::Native(_) => {
                    let mut buf = core::mem::take(&mut self.argbuf);
                    buf.clear();
                    let ok = self.ctx.list_into(operands, &mut buf);
                    let r = if ok {
                        self.finish(c, &buf, env)
                    } else {
                        Err(self.ctx.type_error("a proper list of operands", operands))
                    };
                    self.argbuf = buf;
                    r
                }
                _ => Err(self.not_combiner(c)),
            },
            _ => Err(self.not_combiner(c)),
        }
    }

    fn not_combiner(&self, c: Value) -> Fault {
        Fault::new(
            "type-error",
            format!("{} is not a combiner", self.describe(c)),
        )
        .with(c)
    }

    /// Bind a compound operative's parameters and evaluate its body in tail
    /// position. Operands come either as a list (`operands`) or, for an
    /// applicative call, as a slice of evaluated arguments.
    fn enter_operative(
        &mut self,
        op: Operative,
        operands: Value,
        args: Option<&[Value]>,
        env: ObjId,
    ) -> GResult<Ctl> {
        let mut binds = core::mem::take(&mut self.bindbuf);
        binds.clear();
        let r = match args {
            Some(args) => self.ctx.match_ptree_slice(op.ptree, args, &mut binds),
            None => self.ctx.match_ptree(op.ptree, operands, true, &mut binds),
        };
        if let Err(e) = r {
            self.bindbuf = binds;
            return Err(e);
        }
        if let Value::Sym(e) = op.eparam {
            binds.push((e, Value::Obj(env)));
        }
        let local = self
            .ctx
            .extend_fresh(op.env, binds.drain(..).map(|(s, v)| (s, Some(v))));
        self.bindbuf = binds;
        Ok(Ctl::Eval(op.body, local))
    }

    /// Evaluate the operands of an applicative left to right, then apply
    /// the underlying combiner `inner` to the resulting arguments.
    fn eval_operands(&mut self, inner: Value, operands: Value, env: ObjId) -> GResult<Ctl> {
        let mut buf = core::mem::take(&mut self.argbuf);
        buf.clear();
        let mut rest = operands;
        let r = loop {
            match rest {
                Value::Nil => break self.finish(inner, &buf, env),
                _ => match self.ctx.pair(rest) {
                    Some((a, d)) if !self.ctx.is_pair(a) => match self.eval_simple(a, env) {
                        Ok(v) => {
                            buf.push(v);
                            rest = d;
                        }
                        Err(e) => break Err(e),
                    },
                    Some((a, d)) => {
                        let acc = buf.iter().fold(Value::Nil, |acc, &v| self.ctx.cons(v, acc));
                        self.push_frame(FrameKind::Args {
                            comb: inner,
                            rest: d,
                            acc,
                            env,
                        });
                        break Ok(Ctl::Eval(a, env));
                    }
                    None => {
                        break Err(self.ctx.type_error("a proper list of operands", operands));
                    }
                },
            }
        };
        self.argbuf = buf;
        r
    }

    /// Continue evaluating operands after a frame returned: `acc` holds the
    /// values so far in reverse.
    fn continue_operands(
        &mut self,
        inner: Value,
        mut rest: Value,
        mut acc: Value,
        env: ObjId,
    ) -> GResult<Ctl> {
        loop {
            match rest {
                Value::Nil => break,
                _ => match self.ctx.pair(rest) {
                    Some((a, d)) if !self.ctx.is_pair(a) => {
                        let v = self.eval_simple(a, env)?;
                        acc = self.ctx.cons(v, acc);
                        rest = d;
                    }
                    Some((a, d)) => {
                        self.push_frame(FrameKind::Args {
                            comb: inner,
                            rest: d,
                            acc,
                            env,
                        });
                        return Ok(Ctl::Eval(a, env));
                    }
                    None => return Err(self.ctx.type_error("a proper list of operands", rest)),
                },
            }
        }
        let mut buf = core::mem::take(&mut self.argbuf);
        buf.clear();
        let mut cur = acc;
        while let Some((a, d)) = self.ctx.pair(cur) {
            buf.push(a);
            cur = d;
        }
        buf.reverse();
        let r = self.finish(inner, &buf, env);
        self.argbuf = buf;
        r
    }

    /// Apply the combiner underneath an applicative to evaluated `args`.
    pub(crate) fn finish(&mut self, c: Value, args: &[Value], env: ObjId) -> GResult<Ctl> {
        match c {
            Value::Prim(id) if id == prims::VAU => {
                let operands = self.ctx.list(args);
                prims::vau(self, operands, env)
            }
            Value::Prim(id) => prims::call(self, id, Value::Nil, args, env),
            Value::Obj(oid) => match *self.ctx.heap.get(oid) {
                Object::Operative(op) => self.enter_operative(op, Value::Nil, Some(args), env),
                Object::PrimData(pid, data) => prims::call(self, pid, data, args, env),
                Object::Native(n) => match (n.f)(&mut self.ctx, args) {
                    Ok(v) => Ok(Ctl::Ret(v)),
                    Err(e) => Err(self.error_to_fault(e)),
                },
                Object::Applicative(inner) => {
                    let operands = self.ctx.list(args);
                    Ok(Ctl::Apply(inner, operands, env))
                }
                _ => Err(self.not_combiner(c)),
            },
            _ => Err(self.not_combiner(c)),
        }
    }

    fn error_to_fault(&mut self, e: Error) -> Fault {
        let mut fault = Fault::new(e.kind, e.message);
        for irritant in &e.irritants {
            let s = self.ctx.string(irritant);
            fault.irritants.push(s);
        }
        fault
    }

    /// Call applicative `f` with evaluated `args`.
    pub(crate) fn call(&mut self, f: Value, args: &[Value], env: ObjId) -> GResult<Ctl> {
        match self.ctx.obj(f) {
            Some(Object::Applicative(inner)) => {
                let inner = *inner;
                self.finish(inner, args, env)
            }
            _ => Err(self.ctx.type_error("an applicative", f)),
        }
    }

    /// Return `v` to frame `f`.
    fn ret(&mut self, f: ObjId, v: Value) -> GResult<Ctl> {
        let frame = self.ctx.heap.frame(f);
        self.k = frame.parent;
        match frame.kind {
            FrameKind::Combine { operands, env } => Ok(Ctl::Apply(v, operands, env)),
            FrameKind::Args {
                comb,
                rest,
                acc,
                env,
            } => {
                let acc = self.ctx.cons(v, acc);
                self.continue_operands(comb, rest, acc, env)
            }
            FrameKind::InitRec { env, ptree } => {
                prims::init_recursive(self, env, ptree, v)?;
                Ok(Ctl::Ret(Value::Obj(env)))
            }
            FrameKind::Prompt { .. } => Ok(Ctl::Ret(v)),
            FrameKind::HostError => Ok(Ctl::HostErr(v)),
            FrameKind::CheckEnv => {
                if self.ctx.is_env(v) {
                    Ok(Ctl::Ret(v))
                } else {
                    Err(self
                        .ctx
                        .type_error("an environment from a declaration's transformer", v))
                }
            }
        }
    }

    /// Bind `name` to `value` in the primitive environment.
    pub(crate) fn bind_prim(&mut self, env: ObjId, name: &str, value: Value) -> ObjId {
        let s = self.ctx.sym(name);
        self.ctx.extend(env, [(s, Some(value))])
    }

    /// Wrap a primitive id as an applicative.
    pub(crate) fn prim_applicative(&mut self, id: PrimId) -> Value {
        Value::Obj(self.ctx.heap.alloc(Object::Applicative(Value::Prim(id))))
    }
}
