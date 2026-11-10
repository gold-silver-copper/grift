//! The printer, with an explicit stack. Pairs can't form cycles (they are immutable, and a
//! pair only refers to older values), and a context prints only its names, so it ends. A report
//! prints the same way into a budget, a context there only as its count of names and data nested
//! too deep for its stack as `...`.

use crate::Grift;
use crate::heap::Object;
use crate::local::{Error, K, Value, push};
use std::fmt::{self, Write};

/// The least budget a report cut to fit keeps both its kind and its place in: past the two
/// quarter shares for the names, half the budget holds ` at `, `:` and a line of 10 digits.
const REPORT_FLOOR: usize = 2 * (" at :".len() + 10);

/// A sink that takes everything and keeps nothing: a report's first pass, which measures.
struct Discard;

impl Write for Discard {
    fn write_str(&mut self, _: &str) -> fmt::Result {
        Ok(())
    }
}

/// How many items a report's printer holds before it writes what it would open as `...`: data
/// nested through its first element costs an item a level, so this, not the data, bounds the
/// printer's memory, about 2 KiB.
const REPORT_DEPTH: usize = 32;

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
        let mut out = Paced::new(self, out, usize::MAX);
        let printed = self.print_into(v, None, &mut out);
        out.ending(printed)
    }

    /// Prints `v` into `out`, or with `report`, as a report prints it: a context as its count of
    /// names, a name with its control characters escaped, and the combination `report` holds, the
    /// line before's, as `...`.
    fn print_into(
        &self,
        v: Value,
        report: Option<Option<Value>>,
        out: &mut Paced<'_, T>,
    ) -> Result<(), Error> {
        let mut todo = Vec::new();
        // The first item is held outside the stack, so writing an atom allocates nothing and an
        // `out-of-memory` error can still be printed.
        let mut first = Some(Item::Value(v));
        let shown = report.flatten();
        while let Some(item) = first.take().or_else(|| todo.pop()) {
            if out.charge(1) {
                return Err(Error::interrupted());
            }
            let v = match item {
                Item::Tail(Value::EmptyList, text) => {
                    out.write_str(text)?;
                    continue;
                }
                Item::Tail(Value::Pair(r), text) => {
                    #[expect(clippy::disallowed_methods, reason = "the printer's loop is charged")]
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
                Value::Symbol(s) if report.is_some() => write_name(self.name(s), usize::MAX, out)?,
                Value::Symbol(s) => out.write_str(self.name(s))?,
                Value::Primitive(p) => write!(out, "#[primitive {}]", p.name())?,
                Value::Host(h) => {
                    let name = self
                        .hosts
                        .get(h.0 as usize)
                        .map_or("", |&(s, _)| self.name(s));
                    out.write_str("#[host ")?;
                    match report {
                        Some(_) => write_name(name, usize::MAX, out)?,
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
                Value::Pair(_) | Value::Operative(_) | Value::Applicative(_)
                    if report.is_some() && todo.len() >= REPORT_DEPTH =>
                {
                    out.left_out = true;
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
    /// is printed from where it is, each value checked first. All of its work, every check, item
    /// and byte, is one walk, which a stop ends.
    pub(crate) fn report(
        &self,
        e: &crate::Error,
        out: &mut dyn Write,
        bytes: usize,
    ) -> Result<ReportStatus, Error> {
        // A first pass writes nothing, to learn whether the whole report fits: only one that doesn't
        // has its names cut, to keep the place in, and only from `REPORT_FLOOR` bytes up, where
        // both fit; below, the kind comes first, as much of it as fits.
        let mut discard = Discard;
        let mut probe = Paced::new(self, &mut discard, bytes);
        let probed = self.write_report(e, usize::MAX, &mut probe);
        probe.ending(probed)?;
        let share = match probe.full && bytes >= REPORT_FLOOR {
            true => bytes / 4,
            false => usize::MAX,
        };
        let mut out = Paced::new(self, out, bytes);
        out.walked = probe.walked;
        let written = self.write_report(e, share, &mut out);
        out.ending(written)?;
        Ok(match out.left_out {
            true => ReportStatus::Truncated,
            false => ReportStatus::Complete,
        })
    }

    /// The report: the kind and the innermost place, each cut to `share` bytes, the values, then
    /// the trace, each line's combination written whole but for the line before's. With no place
    /// to keep in, the kind isn't cut to a share.
    fn write_report(
        &self,
        e: &crate::Error,
        share: usize,
        out: &mut Paced<'_, T>,
    ) -> Result<(), Error> {
        let kind = self.import_symbol(&e.kind)?;
        for v in e.values.iter().chain(&e.trace) {
            if out.charge(1) {
                return Err(Error::interrupted());
            }
            self.import(v)?;
        }
        let trace = e.trace.iter().map(|v| self.import(v));
        let mut place = None;
        for (i, at) in trace.clone().enumerate() {
            if out.charge(1) {
                return Err(Error::interrupted());
            }
            if let Some((source, line)) = self.position_local(at?) {
                place = Some((i, source, line));
                break;
            }
        }
        let kind_share = if place.is_some() { share } else { usize::MAX };
        write_name(self.name(kind), kind_share, out)?;
        if let Some((_, source, line)) = place {
            out.write_str(" at ")?;
            write_name(source, share, out)?;
            write!(out, ":{line}")?;
        }
        for v in &e.values {
            out.write_char(' ')?;
            self.print_into(self.import(v)?, Some(None), out)?;
        }
        let mut before = None;
        for (i, at) in trace.enumerate() {
            let at = at?;
            out.write_str(if i == 0 { " in " } else { "\n  from " })?;
            self.print_into(at, Some(before), out)?;
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
        Ok(())
    }
}

/// Whether [`Grift::describe`] wrote the whole report. A sink's refusal is an error instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportStatus {
    /// The whole report was written.
    Complete,
    /// Something was left out: by the budget, a name cut with `...`, or data nested too deep.
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
        #[expect(clippy::disallowed_methods, reason = "reserved text.len() above")]
        self.0.push_str(text);
        Ok(())
    }
}

/// Writes `name` as a report does: its control characters escaped as in a string literal, so no
/// name can begin a line of a report, and cut with `...` if it is longer than `share` bytes.
fn write_name<T>(name: &str, share: usize, out: &mut Paced<'_, T>) -> fmt::Result {
    let width = |c: char| match c.is_control() {
        // `\x` and `;` around the hex digits.
        true => {
            3 + (u32::BITS - u32::from(c).leading_zeros())
                .div_ceil(4)
                .max(1) as usize
        }
        false => c.len_utf8(),
    };
    // Whether it fits, looking no further in than `share` bytes.
    let mut seen = 0;
    let mut whole = true;
    for c in name.chars() {
        seen += width(c);
        whole = seen <= share;
        if !whole {
            break;
        }
        if out.charge(1) {
            return Err(fmt::Error);
        }
    }
    let room = if whole {
        share
    } else {
        share.saturating_sub(3)
    };
    let mut used = 0;
    for c in name.chars() {
        used += width(c);
        if used > room {
            out.left_out = true;
            return out.write_str("...");
        }
        match c.is_control() {
            true => write!(out, "\\x{:x};", u32::from(c))?,
            false => out.write_char(c)?,
        }
    }
    Ok(())
}

/// The printer's output. It charges every item and byte to one walk, which a stop ends, and it
/// passes on `left` more bytes, the last piece cut at a character boundary, then refuses.
struct Paced<'a, T> {
    grift: &'a Grift<T>,
    out: &'a mut dyn Write,
    walked: usize,
    left: usize,
    /// A stop was found.
    stopped: bool,
    /// `left` ran out.
    full: bool,
    /// Something was left out: by `full`, or by a name cut.
    left_out: bool,
}

impl<'a, T> Paced<'a, T> {
    fn new(grift: &'a Grift<T>, out: &'a mut dyn Write, left: usize) -> Paced<'a, T> {
        Paced {
            grift,
            out,
            walked: 0,
            left,
            stopped: false,
            full: false,
            left_out: false,
        }
    }

    /// Charges `cost` items of work; whether a stop was found.
    fn charge(&mut self, cost: usize) -> bool {
        self.stopped |= self.grift.walk_charged(&mut self.walked, cost);
        self.stopped
    }

    /// What a print came to: a stop found is `interrupted`, running out of room is no error, and
    /// a refusal of the host's sink is.
    fn ending(&self, printed: Result<(), Error>) -> Result<(), Error> {
        match printed {
            _ if self.stopped => Err(Error::interrupted()),
            Err(_) if self.full => Ok(()),
            printed => printed,
        }
    }
}

impl<T> Write for Paced<'_, T> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.charge(text.len()) {
            return Err(fmt::Error);
        }
        if let Some(left) = self.left.checked_sub(text.len()) {
            self.left = left;
            return self.out.write_str(text);
        }
        // `left` is inside `text`, and a boundary is at most three bytes back, 0 being one.
        let end = (0..=self.left).rev().find(|&i| text.is_char_boundary(i));
        self.out
            .write_str(text.get(..end.unwrap_or(0)).unwrap_or_default())?;
        (self.left, self.full, self.left_out) = (0, true, true);
        Err(fmt::Error)
    }
}

/// Writes `text` as one quoted grift string literal, with the same escapes as [`Grift::write`]
/// and the reader. Needs no interpreter and allocates nothing; an output failure is `fmt::Error`.
///
/// ```
/// let mut out = grift::TextBuffer::new();
/// grift::write_string("a\n\"b", &mut out)?;
/// assert_eq!(out.as_str(), "\"a\\n\\\"b\"");
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
