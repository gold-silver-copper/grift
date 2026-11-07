//! The reader: source text to forms, with an explicit stack of open lists.

use crate::evaluate::integer;
use crate::heap::{Object, Reference};
use crate::local::{Error, K, Symbol, Value, boxed, push};
use crate::{Grift, Position};
use std::ops::Range;

/// A list being read: its first pair, its last pair, where a dotted tail stands, and the offset
/// and line of its `(`.
struct List {
    head: Value,
    last: Value,
    dot: Dot,
    start: usize,
    line: u32,
}

impl List {
    fn new(start: usize, line: u32) -> List {
        List {
            head: Value::EmptyList,
            last: Value::EmptyList,
            dot: Dot::No,
            start,
            line,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Dot {
    No,
    /// After `.`, waiting for the tail.
    Tail,
    /// After the tail, waiting for `)`.
    Done,
}

struct Cursor<'a> {
    src: &'a str,
    /// The byte offset of the next character, and its line.
    at: usize,
    line: u32,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<char> {
        let &byte = self.src.as_bytes().get(self.at)?;
        if byte.is_ascii() {
            Some(char::from(byte))
        } else {
            self.src.get(self.at..)?.chars().next()
        }
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        if c == '\n' {
            self.line = self.line.saturating_add(1);
        }
        Some(c)
    }

    fn skip_space(&mut self) {
        while let Some(c) = self.peek() {
            let rest = self.src.get(self.at..).unwrap_or_default();
            match c {
                _ if c == ';' || rest.starts_with("#!") => {
                    self.at += rest.find('\n').unwrap_or(rest.len());
                }
                c if c.is_whitespace() => {}
                _ => return,
            }
            self.next();
        }
    }
}

/// A `parse-error` for `reason` at byte offset `at` of `src`: the reason, then its line and column,
/// counted from the start of the text only now, so reading form by form never recounts.
fn parse_error(src: &str, at: usize, reason: K) -> Error {
    let before = src.get(..at).unwrap_or_default();
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    Error::new(
        K::ParseError,
        &[Value::Symbol(reason.into()), integer(line), integer(column)],
    )
}

fn delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '"' | ';')
}

/// The float a `#` name stands for, for the reader and `string-to-float`.
pub(crate) fn float_name(token: &str) -> Option<f64> {
    match token {
        "#infinity" => Some(f64::INFINITY),
        "#-infinity" => Some(f64::NEG_INFINITY),
        "#not-a-number" => Some(f64::NAN),
        _ => None,
    }
}

impl<T> Grift<T> {
    /// Reads every form in `src` into a list. Under a `source` name, each combination's line goes
    /// in the positions table.
    pub(crate) fn read_local(&mut self, src: &str, source: Option<Symbol>) -> Result<Value, Error> {
        let mut forms = List::new(0, 1);
        let (mut at, mut line) = (0, 1);
        while let Some((form, place, after)) =
            self.read_form(src, at, line, source).or_else(|mut e| {
                if e.kind == K::ParseError.into()
                    && let Some(source) = source
                {
                    let name = self.source_string(source)?;
                    push(&mut e.trace, name)?;
                }
                Err(e)
            })?
        {
            if let (Some(_), Value::Pair(r)) = (source, form)
                && let Some(position) = self.positions.get_mut(&r.idx)
            {
                position.outer = true;
            }
            self.add(&mut forms, form)?;
            (at, line) = (place.end, after);
        }
        Ok(forms.head)
    }

    // Only malformed named input needs this allocation path; keep it out of the reader loop.
    #[cold]
    #[inline(never)]
    fn source_string(&mut self, source: Symbol) -> Result<Value, Error> {
        // Source ids are interned, below the disjoint host-kept ids at KEPT and above.
        if let Some(name) = self.find(self.kept, source)? {
            return Ok(name);
        }
        let text = boxed(self.name(source))?;
        let name = Value::String(self.heap.alloc(Object::String(text))?);
        self.bind_symbol(self.kept, source, name)?;
        Ok(name)
    }

    /// Reads the form after byte offset `from` of `src`, a character boundary on line `line`: the
    /// form, where it is and the line it ends on, or `None` if only space and comments are left.
    pub(crate) fn read_form(
        &mut self,
        src: &str,
        from: usize,
        line: u32,
        source: Option<Symbol>,
    ) -> Result<Option<(Value, Range<usize>, u32)>, Error> {
        let mut cur = Cursor {
            src,
            at: from,
            line,
        };
        cur.skip_space();
        let start = cur.at;
        let mut open: Vec<List> = Vec::new();
        loop {
            cur.skip_space();
            let begin = cur.at;
            let fail = |reason| parse_error(src, begin, reason);
            let Some(c) = cur.next() else {
                return match open.last() {
                    None => Ok(None),
                    Some(list) => Err(parse_error(src, list.start, K::UnclosedList)),
                };
            };
            let datum = match c {
                '(' => {
                    push(&mut open, List::new(begin, cur.line))?;
                    continue;
                }
                ')' => match open.pop() {
                    Some(list) if list.dot != Dot::Tail => {
                        if let (Some(source), Value::Pair(r)) = (source, list.head) {
                            self.place(r, source, list.line)?;
                        }
                        list.head
                    }
                    Some(_) => return Err(fail(K::MisplacedDot)),
                    None => return Err(fail(K::UnmatchedParenthesis)),
                },
                '"' => self.read_string(&mut cur, begin)?,
                _ => {
                    while cur.peek().is_some_and(|c| !delimiter(c)) {
                        cur.next();
                    }
                    let token = src.get(begin..cur.at).unwrap_or_default();
                    if token == "." {
                        match open.last_mut() {
                            Some(list) if list.dot == Dot::No && list.last != Value::EmptyList => {
                                list.dot = Dot::Tail;
                                continue;
                            }
                            _ => return Err(fail(K::MisplacedDot)),
                        }
                    }
                    if token == "#bytes" && cur.peek() == Some('(') {
                        let open = cur.at;
                        cur.next();
                        self.read_bytes(&mut cur, open)?
                    } else {
                        let reason = if token.starts_with('#') {
                            K::UnknownConstant
                        } else {
                            K::BadNumber
                        };
                        self.atom(token).ok_or_else(|| fail(reason))??
                    }
                }
            };
            let Some(list) = open.last_mut() else {
                return Ok(Some((datum, start..cur.at, cur.line)));
            };
            if list.dot == Dot::Done {
                return Err(fail(K::MisplacedDot));
            }
            self.add(list, datum)?;
        }
    }

    /// Adds `datum` to the end of a list being read: an element, or its tail after a `.`.
    fn add(&mut self, list: &mut List, datum: Value) -> Result<(), Error> {
        if list.dot == Dot::Tail {
            if let Value::Pair(r) = list.last {
                self.set_rest(r, datum)?;
            }
            list.dot = Dot::Done;
            return Ok(());
        }
        let pair = self.pair_local(datum, Value::EmptyList)?;
        match list.last {
            Value::Pair(r) => self.set_rest(r, pair)?,
            _ => list.head = pair,
        }
        list.last = pair;
        Ok(())
    }

    /// Records that combination `r` was read from `source` at `line`; a reused slot's entry is
    /// replaced, so the table stays within the heap's size.
    fn place(&mut self, r: Reference, source: Symbol, line: u32) -> Result<(), Error> {
        self.positions.try_reserve(1).map_err(|_| Error::oom())?;
        self.positions.insert(
            r.idx,
            Position {
                generation: r.generation,
                source,
                line,
                outer: false,
            },
        );
        Ok(())
    }

    /// Sets the rest of a pair the reader made and nobody else has seen yet.
    fn set_rest(&mut self, r: Reference, v: Value) -> Result<(), Error> {
        if let Object::Pair(_, d) = self.heap.get_mut(r)? {
            *d = v;
        }
        Ok(())
    }

    /// An atom: a constant, a number or a symbol. `None` for a token that is none of them. A
    /// number starts with a digit, after an optional sign: an integer if it is one, else a float
    /// if it has a `.` or an exponent, as Rust reads them.
    fn atom(&mut self, token: &str) -> Option<Result<Value, Error>> {
        let v = match token {
            "#true" => Value::Boolean(true),
            "#false" => Value::Boolean(false),
            "#inert" => Value::Inert,
            "#ignore" => Value::Ignore,
            _ if token.starts_with('#') => Value::Float(float_name(token)?),
            _ if token
                .strip_prefix(['+', '-'])
                .unwrap_or(token)
                .starts_with(|c: char| c.is_ascii_digit()) =>
            {
                match token.parse() {
                    Ok(n) => Value::Integer(n),
                    Err(_) if token.contains(['.', 'e', 'E']) => Value::Float(token.parse().ok()?),
                    Err(_) => return None,
                }
            }
            _ => return Some(self.intern(token).map(Value::Symbol)),
        };
        Some(Ok(v))
    }

    /// The bytes of a `#bytes(...)` literal after its `(`, at offset `open`: integers 0 to 255,
    /// as the reader reads an integer, with space and comments between them, up to the `)`.
    fn read_bytes(&mut self, cur: &mut Cursor, open: usize) -> Result<Value, Error> {
        let src = cur.src;
        let mut bytes = Vec::new();
        loop {
            cur.skip_space();
            let begin = cur.at;
            match cur.next() {
                None => return Err(parse_error(src, open, K::UnclosedList)),
                Some(')') => break,
                Some(c) if delimiter(c) => return Err(parse_error(src, begin, K::BadByte)),
                Some(_) => {
                    while cur.peek().is_some_and(|c| !delimiter(c)) {
                        cur.next();
                    }
                    let token = src.get(begin..cur.at).unwrap_or_default();
                    match token.parse::<i64>().ok().and_then(|n| u8::try_from(n).ok()) {
                        Some(byte) => push(&mut bytes, byte)?,
                        None if token == "." => {
                            return Err(parse_error(src, begin, K::MisplacedDot));
                        }
                        None => return Err(parse_error(src, begin, K::BadByte)),
                    }
                }
            }
        }
        self.bytes_local(bytes)
    }

    /// A string after its opening quote, at offset `start`, with the escapes `\\`, `\"`, `\n`,
    /// `\r`, `\t` and `\xHEX;` (a Unicode scalar value in hex).
    fn read_string(&mut self, cur: &mut Cursor, start: usize) -> Result<Value, Error> {
        let mut s = String::new();
        loop {
            let escape = cur.at;
            let c = match cur.next() {
                Some('"') => break,
                Some('\\') => match cur.next() {
                    Some('\\') => '\\',
                    Some('"') => '"',
                    Some('n') => '\n',
                    Some('r') => '\r',
                    Some('t') => '\t',
                    Some('x') => {
                        let begin = cur.at;
                        while cur.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                            cur.next();
                        }
                        let hex = cur.src.get(begin..cur.at).unwrap_or_default();
                        let c = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
                        match (cur.next(), c) {
                            (Some(';'), Some(c)) => c,
                            (None, _) => {
                                return Err(parse_error(cur.src, start, K::UnclosedString));
                            }
                            _ => return Err(parse_error(cur.src, escape, K::BadEscape)),
                        }
                    }
                    Some(_) => return Err(parse_error(cur.src, escape, K::BadEscape)),
                    None => return Err(parse_error(cur.src, start, K::UnclosedString)),
                },
                Some(c) => c,
                None => return Err(parse_error(cur.src, start, K::UnclosedString)),
            };
            s.try_reserve(c.len_utf8()).map_err(|_| Error::oom())?;
            s.push(c);
        }
        self.string_local(&s)
    }
}
