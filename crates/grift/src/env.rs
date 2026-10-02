//! Immutable environments and parameter-tree matching.

use alloc::format;
use alloc::vec::Vec;

use crate::ctx::Ctx;
use crate::error::{Fault, GResult};
use crate::heap::{Env, Log, Object};
use crate::value::{ObjId, SymId, Value};

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
        let Env { log, len } = self.heap.env(parent);
        if self.heap.log(log).entries.len() == len as usize {
            let l = self.heap.log_mut(log);
            l.append(entries);
            let len = u32::try_from(l.entries.len()).expect("environment too large");
            self.heap.alloc(Object::Env(Env { log, len }))
        } else {
            let l = Log::new(Some(parent), entries.into_iter().collect());
            let len = u32::try_from(l.entries.len()).expect("environment too large");
            let log = self.heap.alloc(Object::Log(l));
            self.heap.alloc(Object::Env(Env { log, len }))
        }
    }

    /// Look `sym` up in `env`.
    pub(crate) fn lookup(&self, env: ObjId, sym: SymId) -> GResult<Value> {
        let mut cur = env;
        loop {
            let Env { log, len } = self.heap.env(cur);
            let l = self.heap.log(log);
            if let Some(i) = l.find(sym, len) {
                return l.entries[i].1.ok_or_else(|| {
                    Fault::new(
                        "uninitialized-binding",
                        format!(
                            "{} was referenced before its definition finished",
                            self.syms.name(sym)
                        ),
                    )
                    .with(Value::Sym(sym))
                });
            }
            match l.parent {
                Some(p) => cur = p,
                None => {
                    return Err(Fault::new(
                        "unbound-symbol",
                        format!("unbound symbol {}", self.syms.name(sym)),
                    )
                    .with(Value::Sym(sym)));
                }
            }
        }
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
    pub(crate) fn match_ptree(
        &self,
        mut ptree: Value,
        mut value: Value,
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
                        Err(self.mismatch(ptree, value))
                    };
                }
                _ => {
                    let (pa, pd) = self.pair(ptree).expect("validated parameter tree");
                    let Some((va, vd)) = self.pair(value) else {
                        return Err(self.mismatch(ptree, value));
                    };
                    self.match_ptree(pa, va, out)?;
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
                        Err(self.mismatch(ptree, rest))
                    };
                }
                _ => {
                    let (pa, pd) = self.pair(ptree).expect("validated parameter tree");
                    if i == args.len() {
                        return Err(self.mismatch(ptree, Value::Nil));
                    }
                    self.match_ptree(pa, args[i], out)?;
                    i += 1;
                    ptree = pd;
                }
            }
        }
    }

    fn mismatch(&self, ptree: Value, value: Value) -> Fault {
        Fault::new(
            "arity-error",
            format!(
                "{} does not match parameter tree {}",
                self.write(value),
                self.write(ptree)
            ),
        )
        .with(ptree)
        .with(value)
    }
}
