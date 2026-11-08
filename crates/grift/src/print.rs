//! The printer, with an explicit stack. Pairs can't form cycles (they are immutable, and a
//! pair only refers to older values), and a context prints only its names, so it ends. A report
//! prints the same way into a budget, a context there only as its count of names.

use crate::Grift;
use crate::heap::Object;
use crate::local::{Error, K, Value, push};
use std::fmt::{self, Write};

enum Item {
    Value(Value),
    /// The rest of a sequence after its first element, and the text written after it.
    Tail(Value, &'static str),
}

impl<T> Grift<T> {
    /// A new string of the text [`Grift::write`] writes for `v`.
    pub(crate) fn printed(&mut self, v: Value) -> Result<Value, Error> {
        let mut s = TextBuffer::new();
        // All this output can refuse for is memory to grow.
        self.print(v, &mut s).map_err(|e| {
            if e.kind == K::OutputError.into() {
                Error::oom()
            } else {
                e
            }
        })?;
        self.string_local(s.as_str())
    }

    /// What [`Grift::write`] writes. A value from the heap is live: nothing stale gets in. Each
    /// item and each byte written is a step of the walk, so a stop reaches a print that writes a
    /// great deal from a few items: many references to one long string, or one long bytes value.
    pub(crate) fn print(&self, v: Value, out: &mut dyn Write) -> Result<(), Error> {
        self.print_as(v, None, out)
    }

    /// [`Grift::print`], or with `report`, as a report prints `v`: a context as its count of
    /// names, a name with its control characters escaped, and the combination `report` holds, the
    /// line before's, as `...`.
    fn print_as(
        &self,
        v: Value,
        report: Option<Option<Value>>,
        out: &mut dyn Write,
    ) -> Result<(), Error> {
        let mut charged = Charged {
            grift: self,
            out,
            walked: 0,
            stopped: false,
        };
        let printed = self.print_charged(v, report, &mut charged);
        if charged.stopped {
            return Err(Error::interrupted());
        }
        printed
    }

    fn print_charged(
        &self,
        v: Value,
        report: Option<Option<Value>>,
        out: &mut Charged<'_, T>,
    ) -> Result<(), Error> {
        let mut todo = Vec::new();
        // The first item is held outside the stack, so writing an atom allocates nothing and an
        // `out-of-memory` error can still be printed.
        let mut first = Some(Item::Value(v));
        let shown = report.flatten();
        while let Some(item) = first.take().or_else(|| todo.pop()) {
            if self.walk_charged(&mut out.walked, 1) {
                return Err(Error::interrupted());
            }
            let v = match item {
                Item::Tail(Value::EmptyList, text) => {
                    out.write_str(text)?;
                    continue;
                }
                Item::Tail(Value::Pair(r), text) => {
                    let (element, rest) = self.pair_at(r)?;
                    out.write_char(' ')?;
                    push(&mut todo, Item::Tail(rest, text))?;
                    push(&mut todo, Item::Value(element))?;
                    continue;
                }
                Item::Tail(v, text) => {
                    out.write_str(" . ")?;
                    push(&mut todo, Item::Tail(Value::EmptyList, text))?;
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
                Value::Symbol(s) if report.is_some() => {
                    write_name(self.name(s), usize::MAX, out)?;
                }
                Value::Symbol(s) => out.write_str(self.name(s))?,
                Value::Primitive(p) => write!(out, "#[primitive {}]", p.name())?,
                Value::Host(h) => {
                    let name = self
                        .hosts
                        .get(h.0 as usize)
                        .map_or("", |&(s, _)| self.name(s));
                    out.write_str("#[host ")?;
                    match report {
                        Some(_) => write_name(name, usize::MAX, out).map(drop)?,
                        None => out.write_str(name)?,
                    }
                    out.write_char(']')?;
                }
                Value::Context(r) if report.is_some() => {
                    write!(out, "#[context {} names]", self.context_at(r)?.1.len())?;
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
                Value::Pair(_) if shown == Some(v) => {
                    out.write_str("...")?;
                }
                Value::Pair(r)
                | Value::String(r)
                | Value::Bytes(r)
                | Value::Operative(r)
                | Value::Applicative(r) => {
                    match self.heap.get(r) {
                        Ok(Object::Pair(a, b, _)) => {
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
                            push(&mut todo, Item::Tail(Value::EmptyList, " "))?;
                            push(&mut todo, Item::Value(op.parameters))?;
                        }
                        Ok(Object::Applicative(c)) => {
                            out.write_str("#[applicative ")?;
                            push(&mut todo, Item::Tail(Value::EmptyList, "]"))?;
                            push(&mut todo, Item::Value(*c))?;
                        }
                        _ => return Err(Error::new(K::StaleValue, &[v])),
                    }
                }
            }
        }
        Ok(())
    }
}

impl<T> Grift<T> {
    /// What [`Grift::describe`] writes, into `bytes` bytes of `out`. Nothing is copied: the error
    /// is printed from where it is, each value checked first.
    pub(crate) fn report(
        &self,
        e: &crate::Error,
        out: &mut dyn Write,
        bytes: usize,
    ) -> Result<ReportStatus, Error> {
        let kind = self.import_symbol(&e.kind)?;
        e.values
            .iter()
            .chain(&e.trace)
            .try_for_each(|v| self.import(v).map(drop))?;
        let mut budget = Budget {
            out,
            left: bytes,
            cut: false,
        };
        let written = self.write_report(e, kind, bytes / 4, &mut budget);
        match written {
            _ if budget.cut => Ok(ReportStatus::Truncated),
            Ok(true) => Ok(ReportStatus::Truncated),
            Ok(false) => Ok(ReportStatus::Complete),
            Err(e) => Err(e),
        }
    }

    /// The report: the kind and the innermost place, each cut to `share` bytes, the values, then
    /// the trace, each line's combination written whole but for the line before's. Whether a name
    /// was cut.
    fn write_report(
        &self,
        e: &crate::Error,
        kind: crate::local::Symbol,
        share: usize,
        out: &mut dyn Write,
    ) -> Result<bool, Error> {
        let trace = e.trace.iter().map(|v| self.import(v));
        let place = trace.clone().enumerate().find_map(|(i, at)| {
            let (source, line) = self.position_local(at.ok()?)?;
            Some((i, source, line))
        });
        let mut cut = write_name(self.name(kind), share, out)?;
        if let Some((_, source, line)) = place {
            out.write_str(" at ")?;
            cut |= write_name(source, share, out)?;
            write!(out, ":{line}")?;
        }
        for v in &e.values {
            out.write_char(' ')?;
            self.print_as(self.import(v)?, Some(None), out)?;
        }
        let mut before = None;
        for (i, at) in trace.enumerate() {
            let at = at?;
            out.write_str(if i == 0 { " in " } else { "\n  from " })?;
            self.print_as(at, Some(before), out)?;
            // The header has the innermost place: its line doesn't say it again.
            let placed = self
                .position_local(at)
                .filter(|_| place.is_none_or(|(p, ..)| p != i));
            if let Some((source, line)) = placed {
                out.write_str(" at ")?;
                write_name(source, usize::MAX, out)?;
                write!(out, ":{line}")?;
            }
            before = Some(at);
        }
        Ok(cut)
    }
}

/// Whether [`Grift::describe`] wrote the whole report. A sink's refusal is an error instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportStatus {
    /// The whole report was written.
    Complete,
    /// The budget cut it short, or cut a name in it with `...`.
    Truncated,
}

/// Text written fallibly: a [`fmt::Write`] that refuses what it has no room for, with
/// `fmt::Error`, where a `String` would abort the process. What it holds stays readable after a
/// refusal.
///
/// ```
/// use std::fmt::Write;
/// let mut text = grift::TextBuffer::new();
/// write!(text, "{} {}", 1, "two")?;
/// assert_eq!(text.as_str(), "1 two");
/// # Ok::<(), std::fmt::Error>(())
/// ```
#[derive(Debug, Default)]
pub struct TextBuffer(String);

impl TextBuffer {
    /// An empty buffer, which allocates nothing.
    pub fn new() -> TextBuffer {
        TextBuffer::default()
    }

    /// The text written so far.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Write for TextBuffer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0.try_reserve(text.len()).map_err(|_| fmt::Error)?;
        self.0.push_str(text);
        Ok(())
    }
}

/// A report's sink: it passes on `left` more bytes, the last piece cut at a character boundary,
/// then refuses, and says it was cut.
struct Budget<'a> {
    out: &'a mut dyn Write,
    left: usize,
    cut: bool,
}

impl Write for Budget<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if let Some(left) = self.left.checked_sub(text.len()) {
            self.left = left;
            return self.out.write_str(text);
        }
        // `left` is inside `text`, and a boundary is at most three bytes back, 0 being one.
        let end = (0..=self.left).rev().find(|&i| text.is_char_boundary(i));
        self.out
            .write_str(text.get(..end.unwrap_or(0)).unwrap_or_default())?;
        (self.left, self.cut) = (0, true);
        Err(fmt::Error)
    }
}

/// Writes `name` as a report does: its control characters escaped as in a string literal, so no
/// name can begin a line of a report, and cut with `...` if it is longer than `share` bytes.
/// Whether it was cut.
fn write_name(name: &str, share: usize, out: &mut dyn Write) -> Result<bool, fmt::Error> {
    let width = |c: char| match c.is_control() {
        // `\x` and `;` around the hex digits.
        true => {
            3 + (u32::BITS - u32::from(c).leading_zeros())
                .div_ceil(4)
                .max(1) as usize
        }
        false => c.len_utf8(),
    };
    let whole = name.chars().map(width).sum::<usize>() <= share;
    let room = if whole {
        share
    } else {
        share.saturating_sub(3)
    };
    let mut used = 0;
    for c in name.chars() {
        used += width(c);
        if used > room {
            out.write_str("...")?;
            return Ok(true);
        }
        match c.is_control() {
            true => write!(out, "\\x{:x};", u32::from(c))?,
            false => out.write_char(c)?,
        }
    }
    Ok(false)
}

/// The printer's output, which charges every byte written to its walk and refuses it once a stop
/// is found.
struct Charged<'a, T> {
    grift: &'a Grift<T>,
    out: &'a mut dyn Write,
    walked: usize,
    stopped: bool,
}

impl<T> Write for Charged<'_, T> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.grift.walk_charged(&mut self.walked, text.len()) {
            self.stopped = true;
            return Err(fmt::Error);
        }
        self.out.write_str(text)
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
