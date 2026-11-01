//! The printer, with an explicit stack. Pairs can't form cycles (they are immutable, and a
//! pair only refers to older values), and environments are printed opaquely, so it ends.

use crate::heap::Obj;
use crate::{Grift, Value};
use std::fmt::{self, Write};

enum Item {
    Value(Value),
    /// The rest of a list after its first element.
    Tail(Value),
    Text(&'static str),
}

impl<T> Grift<T> {
    /// Writes `v` as grift would read it back, where it can be read back. Combiners show their
    /// parameters and body: `#[operative (x) #ignore x]`, `#[applicative #[primitive car]]`.
    pub fn write(&self, v: Value, out: &mut dyn Write) -> fmt::Result {
        let mut todo = Vec::new();
        let push = |todo: &mut Vec<Item>, it| {
            todo.try_reserve(1).map_err(|_| fmt::Error)?;
            todo.push(it);
            Ok(())
        };
        // The first item is held outside the stack, so writing an atom allocates nothing and an
        // `out-of-memory` error can still be printed.
        let mut first = Some(Item::Value(v));
        while let Some(item) = first.take().or_else(|| todo.pop()) {
            let opens = matches!(item, Item::Value(Value::Pair(_)));
            let v = match item {
                Item::Text(s) => {
                    out.write_str(s)?;
                    continue;
                }
                Item::Tail(Value::Nil) => {
                    out.write_char(')')?;
                    continue;
                }
                Item::Tail(Value::Pair(r)) => {
                    out.write_char(' ')?;
                    Value::Pair(r)
                }
                Item::Tail(v) => {
                    out.write_str(" . ")?;
                    push(&mut todo, Item::Text(")"))?;
                    v
                }
                Item::Value(v) => v,
            };
            match v {
                Value::Nil => out.write_str("()")?,
                Value::True => out.write_str("#t")?,
                Value::False => out.write_str("#f")?,
                Value::Inert => out.write_str("#inert")?,
                Value::Ignore => out.write_str("#ignore")?,
                Value::Int(n) => write!(out, "{n}")?,
                Value::Sym(s) => out.write_str(self.names.get(s.0 as usize).map_or("", |n| n))?,
                Value::Env(_) => out.write_str("#[environment]")?,
                Value::Prim(p) => write!(out, "#[primitive {}]", p.name())?,
                Value::Host(h) => {
                    out.write_str("#[host ")?;
                    let name = self.hosts.get(h.0 as usize).map(|&(s, _)| Value::Sym(s));
                    push(&mut todo, Item::Text("]"))?;
                    push(&mut todo, Item::Value(name.unwrap_or(Value::Nil)))?;
                }
                Value::Pair(r) | Value::Str(r) | Value::Op(r) | Value::App(r) => {
                    match self.heap.get(r) {
                        Ok(Obj::Pair(a, d)) => {
                            if opens {
                                out.write_char('(')?;
                            }
                            push(&mut todo, Item::Tail(*d))?;
                            push(&mut todo, Item::Value(*a))?;
                        }
                        Ok(Obj::Str(s)) => {
                            out.write_char('"')?;
                            for c in s.chars() {
                                match c {
                                    '"' => out.write_str("\\\"")?,
                                    '\\' => out.write_str("\\\\")?,
                                    '\n' => out.write_str("\\n")?,
                                    '\t' => out.write_str("\\t")?,
                                    c if c.is_control() => write!(out, "\\x{:x};", u32::from(c))?,
                                    c => out.write_char(c)?,
                                }
                            }
                            out.write_char('"')?;
                        }
                        Ok(Obj::Op(op)) => {
                            out.write_str("#[operative ")?;
                            let parts = [op.formals, op.eformal, op.body];
                            push(&mut todo, Item::Text("]"))?;
                            for (i, part) in parts.into_iter().enumerate().rev() {
                                push(&mut todo, Item::Value(part))?;
                                if i > 0 {
                                    push(&mut todo, Item::Text(" "))?;
                                }
                            }
                        }
                        Ok(Obj::App(c)) => {
                            out.write_str("#[applicative ")?;
                            push(&mut todo, Item::Text("]"))?;
                            push(&mut todo, Item::Value(*c))?;
                        }
                        _ => out.write_str("#[stale]")?,
                    }
                }
            }
        }
        Ok(())
    }
}
