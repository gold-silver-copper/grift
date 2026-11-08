//! The printer, with an explicit stack. Pairs can't form cycles (they are immutable, and a
//! pair only refers to older values), and a context prints only its names, so it ends.

use crate::Grift;
use crate::heap::Object;
use crate::local::{Error, Growing, K, Value, push};
use std::fmt::{self, Write};

enum Item {
    Value(Value),
    /// The rest of a sequence after its first element, and the bracket that closes it.
    Tail(Value, &'static str),
    Text(&'static str),
}

/// Why printing stopped.
pub(crate) enum Unprinted {
    /// The output refused the text.
    Output,
    /// A stop, or no memory for the printer's own stack.
    Value(Error),
}

impl From<fmt::Error> for Unprinted {
    fn from(_: fmt::Error) -> Unprinted {
        Unprinted::Output
    }
}

impl From<Error> for Unprinted {
    fn from(e: Error) -> Unprinted {
        Unprinted::Value(e)
    }
}

impl<T> Grift<T> {
    /// A new string of the text [`Grift::write`] writes for `v`.
    pub(crate) fn printed(&mut self, v: Value) -> Result<Value, Error> {
        let mut s = Growing::default();
        self.print(v, &mut s).map_err(|why| match why {
            Unprinted::Output => Error::oom(),
            Unprinted::Value(e) => e,
        })?;
        self.string_local(&s.0)
    }

    /// What [`Grift::write`] writes. A value from the heap is live: nothing stale gets in.
    pub(crate) fn print(&self, v: Value, out: &mut dyn Write) -> Result<(), Unprinted> {
        let mut todo = Vec::new();
        // The first item is held outside the stack, so writing an atom allocates nothing and an
        // `out-of-memory` error can still be printed.
        let mut first = Some(Item::Value(v));
        let mut walked = 0;
        while let Some(item) = first.take().or_else(|| todo.pop()) {
            if self.walk_stopped(&mut walked) {
                return Err(Error::interrupted().into());
            }
            let v = match item {
                Item::Text(s) => {
                    out.write_str(s)?;
                    continue;
                }
                Item::Tail(Value::EmptyList, closer) => {
                    out.write_str(closer)?;
                    continue;
                }
                Item::Tail(Value::Pair(r), closer) => {
                    let Ok(Object::Pair(element, rest)) = self.heap.get(r) else {
                        return Err(Error::new(K::StaleValue, &[Value::Pair(r)]).into());
                    };
                    out.write_char(' ')?;
                    push(&mut todo, Item::Tail(*rest, closer))?;
                    push(&mut todo, Item::Value(*element))?;
                    continue;
                }
                Item::Tail(v, closer) => {
                    out.write_str(" . ")?;
                    push(&mut todo, Item::Text(closer))?;
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
                    let names = self.names_of(r)?;
                    out.write_str("#[context")?;
                    for name in names {
                        out.write_char(' ')?;
                        out.write_str(self.name(name))?;
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
                            out.write_char('(')?;
                            push(&mut todo, Item::Tail(*b, ")"))?;
                            push(&mut todo, Item::Value(*a))?;
                        }
                        Ok(Object::String(s)) => write_string(s, out)?,
                        // The literal: each byte as the integer `byte-at` gives for it.
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
                            push(&mut todo, Item::Tail(op.body, "]"))?;
                            push(&mut todo, Item::Value(op.context_parameter))?;
                            push(&mut todo, Item::Text(" "))?;
                            push(&mut todo, Item::Value(op.parameters))?;
                        }
                        Ok(Object::Applicative(c)) => {
                            out.write_str("#[applicative ")?;
                            push(&mut todo, Item::Text("]"))?;
                            push(&mut todo, Item::Value(*c))?;
                        }
                        _ => return Err(Error::new(K::StaleValue, &[v]).into()),
                    }
                }
            }
        }
        Ok(())
    }
}

/// Writes `text` as one quoted grift string literal, with the same escapes as [`Grift::write`]
/// and the reader. Needs no interpreter and allocates nothing; an output failure is `fmt::Error`.
///
/// ```
/// let mut out = String::new();
/// grift::write_string("a\n\"b", &mut out)?;
/// assert_eq!(out, "\"a\\n\\\"b\"");
/// # Ok::<(), std::fmt::Error>(())
/// ```
pub fn write_string(text: &str, out: &mut dyn fmt::Write) -> fmt::Result {
    out.write_char('"')?;
    for c in text.chars() {
        match c {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            '\n' => out.write_str("\\n")?,
            '\r' => out.write_str("\\r")?,
            '\t' => out.write_str("\\t")?,
            c if c.is_control() => write!(out, "\\x{:x};", u32::from(c))?,
            c => out.write_char(c)?,
        }
    }
    out.write_char('"')
}
