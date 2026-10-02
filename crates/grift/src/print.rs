//! The printer: `write` (machine-readable) and `display` (human) modes.

use alloc::string::String;
use core::fmt::Write as _;

use crate::ctx::Ctx;
use crate::heap::Object;
use crate::value::Value;

/// Names of characters written as `#\name`.
pub(crate) const CHAR_NAMES: &[(&str, char)] = &[
    ("space", ' '),
    ("newline", '\n'),
    ("tab", '\t'),
    ("return", '\r'),
    ("null", '\0'),
    ("alarm", '\u{7}'),
    ("backspace", '\u{8}'),
    ("delete", '\u{7f}'),
    ("escape", '\u{1b}'),
];

impl Ctx {
    /// Append the representation of `v` to `out`.
    pub(crate) fn print(&self, v: Value, write: bool, out: &mut String) {
        match v {
            Value::Nil => out.push_str("()"),
            Value::Bool(true) => out.push_str("#t"),
            Value::Bool(false) => out.push_str("#f"),
            Value::Inert => out.push_str("#inert"),
            Value::Ignore => out.push_str("#ignore"),
            Value::Int(n) => {
                let _ = write!(out, "{n}");
            }
            Value::Char(c) => {
                if write {
                    write_char(c, out);
                } else {
                    out.push(c);
                }
            }
            Value::Sym(s) => out.push_str(self.syms.name(s)),
            Value::Prim(_) => out.push_str("#[operative]"),
            Value::Obj(id) => match self.heap.get(id) {
                Object::Pair(..) => self.print_list(v, write, out),
                Object::Str(s) => {
                    if write {
                        write_string(s, out);
                    } else {
                        out.push_str(s);
                    }
                }
                Object::Env(_) | Object::SmallEnv(_) => out.push_str("#[environment]"),
                Object::Operative(_) | Object::PrimData(..) | Object::Native(_) => {
                    out.push_str("#[operative]");
                }
                Object::Applicative(_) => out.push_str("#[applicative]"),
                Object::Encap(..) => out.push_str("#[encapsulation]"),
                Object::Declaration(_) => out.push_str("#[declaration]"),
                Object::Prompt => out.push_str("#[prompt]"),
                Object::SubCont(_) => out.push_str("#[sub-continuation]"),
                Object::Free(_) | Object::Log(_) | Object::Frame(_) => {
                    out.push_str("#[internal]");
                }
            },
        }
    }

    fn print_list(&self, mut v: Value, write: bool, out: &mut String) {
        out.push('(');
        let mut first = true;
        while let Some((a, d)) = self.pair(v) {
            if !first {
                out.push(' ');
            }
            first = false;
            self.print(a, write, out);
            v = d;
        }
        if v != Value::Nil {
            out.push_str(" . ");
            self.print(v, write, out);
        }
        out.push(')');
    }
}

fn write_char(c: char, out: &mut String) {
    out.push_str("#\\");
    if let Some((name, _)) = CHAR_NAMES.iter().find(|(_, ch)| *ch == c) {
        out.push_str(name);
    } else if c.is_control() || c.is_whitespace() {
        let _ = write!(out, "x{:x}", c as u32);
    } else {
        out.push(c);
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => {
                let _ = write!(out, "\\x{:x};", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
