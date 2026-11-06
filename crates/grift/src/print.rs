//! The printer, with an explicit stack. Pairs can't form cycles (they are immutable, and a
//! pair only refers to older values), and a context prints only its names, so it ends.

use crate::Grift;
use crate::heap::Object;
use crate::local::{Error, Growing, Value};
use std::fmt::{self, Write};

enum Item {
    Value(Value),
    /// The rest of a list after its first element.
    Tail(Value),
    Text(&'static str),
}

impl<T> Grift<T> {
    /// A new string of the text [`Grift::write`] writes for `v`.
    pub(crate) fn printed(&mut self, v: Value) -> Result<Value, Error> {
        let mut s = Growing::default();
        self.print(v, &mut s).map_err(|_| Error::oom())?;
        self.string_local(&s.0)
    }

    /// What [`Grift::write`] writes. A value from the heap is live: nothing stale gets in.
    pub(crate) fn print(&self, v: Value, out: &mut dyn Write) -> fmt::Result {
        let mut todo = Vec::new();
        let push = |todo: &mut Vec<Item>, it| {
            todo.try_reserve(1).map_err(|_| fmt::Error)?;
            todo.push(it);
            Ok(())
        };
        // The first item is held outside the stack, so writing an atom allocates nothing and an
        // `out-of-memory` error can still be printed.
        let mut first = Some(Item::Value(v));
        let mut walked = 0;
        while let Some(item) = first.take().or_else(|| todo.pop()) {
            if self.walk_stopped(&mut walked) {
                return Err(fmt::Error);
            }
            let opens = matches!(item, Item::Value(Value::Pair(_)));
            let v = match item {
                Item::Text(s) => {
                    out.write_str(s)?;
                    continue;
                }
                Item::Tail(Value::EmptyList) => {
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
                Value::EmptyList => out.write_str("()")?,
                Value::Boolean(true) => out.write_str("#true")?,
                Value::Boolean(false) => out.write_str("#false")?,
                Value::Inert => out.write_str("#inert")?,
                Value::Ignore => out.write_str("#ignore")?,
                Value::Integer(n) => write!(out, "{n}")?,
                Value::Float(f) if f.is_nan() => out.write_str("#not-a-number")?,
                Value::Float(f) if f == f64::INFINITY => out.write_str("#infinity")?,
                Value::Float(f) if f == f64::NEG_INFINITY => out.write_str("#-infinity")?,
                // Rust's shortest form that reads back, always with a `.` or an exponent.
                Value::Float(f) => write!(out, "{f:?}")?,
                Value::Symbol(s) => out.write_str(self.name(s))?,
                Value::Primitive(p) => write!(out, "#[primitive {}]", p.name())?,
                Value::Host(h) => {
                    let name = self
                        .hosts
                        .get(h.0 as usize)
                        .map_or("", |&(s, _)| self.name(s));
                    write!(out, "#[host {name}]")?;
                }
                Value::Context(r) => {
                    let names = self.names_of(r).map_err(|_| fmt::Error)?;
                    out.write_str("#[context")?;
                    for name in names {
                        out.write_char(' ')?;
                        self.print(name, out)?;
                    }
                    out.write_char(']')?;
                }
                Value::Pair(r)
                | Value::String(r)
                | Value::Bytes(r)
                | Value::Operative(r)
                | Value::Applicative(r) => {
                    match self.heap.get(r) {
                        Ok(Object::Pair(a, b)) => {
                            if opens {
                                out.write_char('(')?;
                            }
                            push(&mut todo, Item::Tail(*b))?;
                            push(&mut todo, Item::Value(*a))?;
                        }
                        Ok(Object::String(s)) => crate::write_string(s, out)?,
                        // The literal: each byte as the integer `bytes-byte` gives for it.
                        Ok(Object::Bytes(b)) => {
                            out.write_str("#bytes(")?;
                            for (i, byte) in b.iter().enumerate() {
                                if i > 0 {
                                    out.write_char(' ')?;
                                }
                                write!(out, "{byte}")?;
                            }
                            out.write_char(')')?;
                        }
                        Ok(Object::Operative(op)) => {
                            out.write_str("#[operative ")?;
                            let parts = [op.parameters, op.context_parameter, op.body];
                            push(&mut todo, Item::Text("]"))?;
                            for (i, part) in parts.into_iter().enumerate().rev() {
                                push(&mut todo, Item::Value(part))?;
                                if i > 0 {
                                    push(&mut todo, Item::Text(" "))?;
                                }
                            }
                        }
                        Ok(Object::Applicative(c)) => {
                            out.write_str("#[applicative ")?;
                            push(&mut todo, Item::Text("]"))?;
                            push(&mut todo, Item::Value(*c))?;
                        }
                        _ => return Err(fmt::Error),
                    }
                }
            }
        }
        Ok(())
    }
}
