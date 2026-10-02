//! Immutable environments and parameter-tree matching.

use alloc::format;
use alloc::vec::Vec;

use crate::ctx::Ctx;
use crate::error::{Fault, GResult};
use crate::heap::{Env, Log, Object, SMALL_ENV, SmallEnv};
use crate::value::{ObjId, SymId, Value};

/// Number of entries in the lookup cache (a power of two).
pub(crate) const LOOKUP_CACHE_SIZE: usize = 4096;

/// An unused lookup-cache entry: no environment has this id.
pub(crate) const EMPTY_CACHE_ENTRY: (u32, u32, Value) = (u32::MAX, u32::MAX, Value::Nil);

#[inline]
fn cache_slot(env: ObjId, sym: SymId) -> usize {
    let h = env.0.wrapping_mul(0x9E37_79B1) ^ sym.0.wrapping_mul(0x85EB_CA6B);
    (h >> 4) as usize & (LOOKUP_CACHE_SIZE - 1)
}

impl Ctx {
    /// A fresh empty environment with no parent.
    pub(crate) fn empty_env(&mut self) -> ObjId {
        let log = self.heap.alloc(Object::Log(Log::new(None, Vec::new())));
        self.heap.alloc(Object::Env(Env { log, len: 0 }))
    }

    /// A new child of `parent` holding `entries`.
    ///
    /// If `parent`'s view ends at the end of its log the entries are
    /// appended to that log (no existing view can see them); otherwise a
    /// new log is started.
    pub(crate) fn extend(
        &mut self,
        parent: ObjId,
        entries: impl IntoIterator<Item = (SymId, Option<Value>)>,
    ) -> ObjId {
        if let Object::Env(Env { log, len }) = *self.heap.get(parent)
            && self.heap.log(log).entries.len() == len as usize
        {
            let l = self.heap.log_mut(log);
            l.append(entries);
            let len = u32::try_from(l.entries.len()).expect("environment too large");
            self.heap.alloc(Object::Env(Env { log, len }))
        } else {
            let mut v = self.heap.entry_vec();
            v.extend(entries);
            let l = Log::new(Some(parent), v);
            let len = u32::try_from(l.entries.len()).expect("environment too large");
            let log = self.heap.alloc(Object::Log(l));
            self.heap.alloc(Object::Env(Env { log, len }))
        }
    }

    /// The environment of an operative call: a child of `parent` binding
    /// `binds` and, optionally, the environment parameter to `caller`.
    pub(crate) fn call_env(
        &mut self,
        parent: ObjId,
        binds: &[(SymId, Value)],
        eparam: Option<(SymId, ObjId)>,
    ) -> ObjId {
        if binds.len() <= SMALL_ENV {
            let mut env = SmallEnv {
                parent,
                eparam,
                len: binds.len() as u8,
                syms: [SymId(0); SMALL_ENV],
                vals: [Value::Nil; SMALL_ENV],
            };
            for (i, &(s, v)) in binds.iter().enumerate() {
                env.syms[i] = s;
                env.vals[i] = v;
            }
            return self.heap.alloc(Object::SmallEnv(env));
        }
        let caller = eparam.map(|(s, e)| (s, Some(Value::Obj(e))));
        self.extend_fresh(
            parent,
            binds.iter().map(|&(s, v)| (s, Some(v))).chain(caller),
        )
    }

    /// A new child of `parent` holding `entries`, always in a fresh log.
    /// Used for operative calls, whose frames are short-lived and must not
    /// grow a shared log.
    pub(crate) fn extend_fresh(
        &mut self,
        parent: ObjId,
        entries: impl IntoIterator<Item = (SymId, Option<Value>)>,
    ) -> ObjId {
        let mut v = self.heap.entry_vec();
        v.extend(entries);
        let len = u32::try_from(v.len()).expect("environment too large");
        let log = self.heap.alloc(Object::Log(Log::new(Some(parent), v)));
        self.heap.alloc(Object::Env(Env { log, len }))
    }

    /// Look `sym` up in `env`.
    ///
    /// The first frame (usually a call's own bindings) is searched
    /// directly. Below it, results are memoized per (environment, symbol):
    /// environments never change, so a resolved binding stays valid. The
    /// cache is cleared at every collection, since object ids are reused.
    pub(crate) fn lookup(&mut self, env: ObjId, sym: SymId) -> GResult<Value> {
        let parent = match self.heap.get(env) {
            Object::SmallEnv(e) => {
                if let Some(v) = e.find(sym) {
                    return Ok(v);
                }
                e.parent
            }
            Object::Env(Env { log, len }) => {
                let l = self.heap.log(*log);
                if let Some(i) = l.find(sym, *len) {
                    return l.entries[i].1.ok_or_else(|| self.uninitialized(sym));
                }
                match l.parent {
                    Some(p) => p,
                    None => return Err(self.unbound(sym)),
                }
            }
            other => unreachable!("expected environment, found {other:?}"),
        };
        let slot = cache_slot(parent, sym);
        let (e, s, v) = self.lookup_cache[slot];
        if e == parent.0 && s == sym.0 {
            return Ok(v);
        }
        let v = self.resolve(parent, sym)?;
        self.lookup_cache[slot] = (parent.0, sym.0, v);
        Ok(v)
    }

    /// Look `sym` up by walking the environment chain from `env`.
    fn resolve(&self, env: ObjId, sym: SymId) -> GResult<Value> {
        let mut cur = env;
        loop {
            match self.heap.get(cur) {
                Object::SmallEnv(e) => {
                    if let Some(v) = e.find(sym) {
                        return Ok(v);
                    }
                    cur = e.parent;
                }
                Object::Env(Env { log, len }) => {
                    let l = self.heap.log(*log);
                    if let Some(i) = l.find(sym, *len) {
                        return l.entries[i].1.ok_or_else(|| self.uninitialized(sym));
                    }
                    match l.parent {
                        Some(p) => cur = p,
                        None => return Err(self.unbound(sym)),
                    }
                }
                other => unreachable!("expected environment, found {other:?}"),
            }
        }
    }

    /// Forget every memoized lookup.
    pub(crate) fn clear_lookup_cache(&mut self) {
        self.lookup_cache.fill(EMPTY_CACHE_ENTRY);
    }

    fn uninitialized(&self, sym: SymId) -> Fault {
        Fault::new(
            "uninitialized-binding",
            format!(
                "{} was referenced before its definition finished",
                self.syms.name(sym)
            ),
        )
        .with(Value::Sym(sym))
    }

    fn unbound(&self, sym: SymId) -> Fault {
        Fault::new(
            "unbound-symbol",
            format!("unbound symbol {}", self.syms.name(sym)),
        )
        .with(Value::Sym(sym))
    }

    /// Validate a parameter tree and append its symbols, in matching order,
    /// to `out`. Valid trees are built from symbols, `#ignore`, `()` and
    /// pairs, with no symbol occurring twice.
    pub(crate) fn ptree_symbols(&self, ptree: Value, out: &mut Vec<SymId>) -> GResult<()> {
        let start = out.len();
        self.collect_ptree(ptree, out)?;
        let syms = &out[start..];
        for (i, s) in syms.iter().enumerate() {
            if syms[..i].contains(s) {
                return Err(Fault::new(
                    "type-error",
                    format!(
                        "symbol {} occurs twice in a parameter tree",
                        self.syms.name(*s)
                    ),
                )
                .with(ptree));
            }
        }
        Ok(())
    }

    fn collect_ptree(&self, mut ptree: Value, out: &mut Vec<SymId>) -> GResult<()> {
        loop {
            match ptree {
                Value::Sym(s) => {
                    out.push(s);
                    return Ok(());
                }
                Value::Ignore | Value::Nil => return Ok(()),
                _ => match self.pair(ptree) {
                    Some((a, d)) => {
                        self.collect_ptree(a, out)?;
                        ptree = d;
                    }
                    None => {
                        return Err(self
                            .type_error("a parameter tree (symbols, #ignore, (), pairs)", ptree));
                    }
                },
            }
        }
    }

    /// Validate an environment parameter: a symbol not in `ptree_syms`, or
    /// `#ignore`.
    pub(crate) fn check_eparam(&self, eparam: Value, ptree_syms: &[SymId]) -> GResult<()> {
        match eparam {
            Value::Ignore => Ok(()),
            Value::Sym(s) if !ptree_syms.contains(&s) => Ok(()),
            Value::Sym(s) => Err(Fault::new(
                "type-error",
                format!(
                    "environment parameter {} also occurs in the parameter tree",
                    self.syms.name(s)
                ),
            )
            .with(eparam)),
            _ => Err(self.type_error("a symbol or #ignore as environment parameter", eparam)),
        }
    }

    /// Match a validated parameter tree against `value`, appending the
    /// bindings (in the same order as [`Ctx::ptree_symbols`]) to `out`.
    ///
    /// `spine` is true when `value` is an operand list: shape mismatches
    /// along its spine are arity errors, mismatches inside elements are
    /// type errors.
    pub(crate) fn match_ptree(
        &self,
        mut ptree: Value,
        mut value: Value,
        spine: bool,
        out: &mut Vec<(SymId, Value)>,
    ) -> GResult<()> {
        loop {
            match ptree {
                Value::Sym(s) => {
                    out.push((s, value));
                    return Ok(());
                }
                Value::Ignore => return Ok(()),
                Value::Nil => {
                    return if value == Value::Nil {
                        Ok(())
                    } else {
                        Err(self.mismatch(ptree, value, spine))
                    };
                }
                _ => {
                    let (pa, pd) = self.pair(ptree).expect("validated parameter tree");
                    let Some((va, vd)) = self.pair(value) else {
                        return Err(self.mismatch(ptree, value, spine));
                    };
                    self.match_ptree(pa, va, false, out)?;
                    ptree = pd;
                    value = vd;
                }
            }
        }
    }

    /// Match a validated parameter tree against the list whose elements are
    /// `args`, without building that list unless a symbol binds a tail.
    pub(crate) fn match_ptree_slice(
        &mut self,
        mut ptree: Value,
        args: &[Value],
        out: &mut Vec<(SymId, Value)>,
    ) -> GResult<()> {
        let mut i = 0;
        loop {
            match ptree {
                Value::Sym(s) => {
                    let tail = self.list(&args[i..]);
                    out.push((s, tail));
                    return Ok(());
                }
                Value::Ignore => return Ok(()),
                Value::Nil => {
                    return if i == args.len() {
                        Ok(())
                    } else {
                        let rest = self.list(&args[i..]);
                        Err(self.mismatch(ptree, rest, true))
                    };
                }
                _ => {
                    let (pa, pd) = self.pair(ptree).expect("validated parameter tree");
                    if i == args.len() {
                        return Err(self.mismatch(ptree, Value::Nil, true));
                    }
                    self.match_ptree(pa, args[i], false, out)?;
                    i += 1;
                    ptree = pd;
                }
            }
        }
    }

    fn mismatch(&self, ptree: Value, value: Value, spine: bool) -> Fault {
        Fault::new(
            if spine { "arity-error" } else { "type-error" },
            "value does not match parameter tree",
        )
        .with(ptree)
        .with(value)
    }
}
