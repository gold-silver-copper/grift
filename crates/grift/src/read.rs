//! The reader: source text to forms, with an explicit stack of open lists.

use crate::evaluate::integer;
use crate::heap::Origin;
use crate::interrupt::Control;
use crate::local::{Error, K, Symbol, Value, push};
use crate::{Grift, TextBuffer, WALK_CHECK};
use std::fmt::Write;
use std::ops::Range;
use std::sync::Arc;

/// A list being read: where its items begin on the reader's stack of items, its tail after a `.`,
/// where a dotted tail stands, and the offset and line of its `(`. A `#bytes(` literal is a list
/// whose elements go in `bytes` instead. Its pairs are made as it closes, so none ever changes.
struct List {
    from: usize,
    tail: Value,
    dot: Dot,
    start: usize,
    line: u32,
    bytes: Option<Vec<u8>>,
}

impl List {
    fn new(from: usize, start: usize, line: u32) -> List {
        List {
            from,
            tail: Value::EmptyList,
            dot: Dot::No,
            start,
            line,
            bytes: None,
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

/// Where the reader is in its text. Each byte it moves past is a step of the walk `walked`, which
/// a stop ends: inside one long string, token or comment too.
struct Cursor<'a> {
    src: &'a str,
    /// The text's name, if it has one, for a `parse-error` to carry.
    source: Option<Symbol>,
    /// The byte offset of the next character, and its line.
    at: usize,
    line: u32,
    walked: &'a mut usize,
    control: Arc<Control>,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<char> {
        self.src.get(self.at..)?.chars().next()
    }

    fn next(&mut self) -> Result<Option<char>, Error> {
        let Some(c) = self.peek() else {
            return Ok(None);
        };
        self.skip(c.len_utf8())?;
        if c == '\n' {
            self.line = self.line.saturating_add(1);
        }
        Ok(Some(c))
    }

    /// Moves past `bytes` bytes of the line, charging them to the walk.
    fn skip(&mut self, bytes: usize) -> Result<(), Error> {
        self.at += bytes;
        match self.control.charged(self.walked, bytes) {
            true => Err(Error::interrupted()),
            false => Ok(()),
        }
    }

    /// Moves to the end of the comment here, its line's end. Its end is found at memory speed, as
    /// a copy is, a check interval of the walk at a time, each charged before the next is searched.
    fn skip_comment(&mut self) -> Result<(), Error> {
        loop {
            let rest = self.src.get(self.at..).unwrap_or_default();
            let mut window = rest.len().min(WALK_CHECK);
            while !rest.is_char_boundary(window) {
                window -= 1;
            }
            let end = rest.get(..window).and_then(|w| w.find('\n'));
            self.skip(end.unwrap_or(window))?;
            if end.is_some() || window == rest.len() {
                return Ok(());
            }
        }
    }

    fn skip_space(&mut self) -> Result<(), Error> {
        while let Some(c) = self.peek() {
            let rest = self.src.get(self.at..).unwrap_or_default();
            match c {
                _ if c == ';' || rest.starts_with("#!") => {
                    self.skip_comment()?;
                    continue;
                }
                c if c.is_whitespace() => {}
                _ => return Ok(()),
            }
            self.next()?;
        }
        Ok(())
    }
}

/// A `parse-error` for `reason` at byte offset `at` of `src`: the reason, then its line and column,
/// counted from the start of the text only now, so reading form by form never recounts, then the
/// text's `source` if it has a name. Every one is made here, so none can miss its source.
fn parse_error(src: &str, at: usize, reason: K, source: Option<Symbol>) -> Error {
    let before = src.get(..at).unwrap_or_default();
    let line = integer(before.matches('\n').count() + 1);
    let column = integer(before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1);
    let reason = Value::Symbol(reason.into());
    match source {
        Some(s) => Error::new(K::ParseError, &[reason, line, column, Value::Symbol(s)]),
        None => Error::new(K::ParseError, &[reason, line, column]),
    }
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
    /// Reads every form in `src` into a list: the library's code if `library`, else the program's.
    /// Under a `source` name, each combination's first pair says its source and line.
    pub(crate) fn read_local(
        &mut self,
        src: &str,
        source: Option<Symbol>,
        library: bool,
    ) -> Result<Value, Error> {
        let mut forms = Vec::new();
        let (mut at, mut line, mut walked) = (0, 1, 0);
        while let Some((form, place, after)) =
            self.read_form(src, at, line, source, &mut walked, library)?
        {
            push(&mut forms, form)?;
            (at, line) = (place.end, after);
        }
        self.close(
            &mut forms,
            &List::new(0, 0, 1),
            None,
            false,
            library,
            &mut walked,
        )
    }

    /// Reads the form after byte offset `from` of `src`, a character boundary on line `line`: the
    /// form, where it is and the line it ends on, or `None` if only space and comments are left.
    /// Each byte read is a step of the walk `walked`, which a stop ends. The form is the library's
    /// code if `library`, else the program's.
    pub(crate) fn read_form(
        &mut self,
        src: &str,
        from: usize,
        line: u32,
        source: Option<Symbol>,
        walked: &mut usize,
        library: bool,
    ) -> Result<Option<(Value, Range<usize>, u32)>, Error> {
        let mut cur = Cursor {
            src,
            source,
            at: from,
            line,
            walked,
            control: Arc::clone(&self.control),
        };
        cur.skip_space()?;
        let start = cur.at;
        let (mut open, mut items): (Vec<List>, _) = (Vec::new(), Vec::new());
        loop {
            cur.skip_space()?;
            let begin = cur.at;
            let fail = |reason| parse_error(src, begin, reason, source);
            let Some(c) = cur.next()? else {
                return match open.last() {
                    None => Ok(None),
                    Some(list) => Err(parse_error(src, list.start, K::UnclosedList, source)),
                };
            };
            let datum = match c {
                '(' => {
                    push(&mut open, List::new(items.len(), begin, cur.line))?;
                    continue;
                }
                ')' => match open.pop() {
                    Some(List {
                        bytes: Some(bytes), ..
                    }) => self.bytes_local(bytes)?,
                    Some(list) if list.dot != Dot::Tail => {
                        let place = source.map(|s| (s, list.line));
                        let outer = open.is_empty();
                        self.close(&mut items, &list, place, outer, library, cur.walked)?
                    }
                    Some(_) => return Err(fail(K::MisplacedDot)),
                    None => return Err(fail(K::UnmatchedParenthesis)),
                },
                '"' => self.read_string(&mut cur, begin)?,
                _ => {
                    while cur.peek().is_some_and(|c| !delimiter(c)) {
                        cur.next()?;
                    }
                    let token = src.get(begin..cur.at).unwrap_or_default();
                    if token == "." {
                        match open.last_mut() {
                            Some(list) if list.dot == Dot::No && items.len() > list.from => {
                                list.dot = Dot::Tail;
                                continue;
                            }
                            _ => return Err(fail(K::MisplacedDot)),
                        }
                    }
                    if token == "#bytes" && cur.peek() == Some('(') {
                        let list = List {
                            bytes: Some(Vec::new()),
                            ..List::new(items.len(), cur.at, cur.line)
                        };
                        push(&mut open, list)?;
                        cur.next()?;
                        continue;
                    }
                    self.atom(token, fail)?
                }
            };
            let Some(list) = open.last_mut() else {
                return Ok(Some((datum, start..cur.at, cur.line)));
            };
            if list.dot == Dot::Done {
                return Err(fail(K::MisplacedDot));
            }
            match &mut list.bytes {
                Some(bytes) => {
                    let byte = match datum {
                        Value::Integer(n) => u8::try_from(n).ok(),
                        _ => None,
                    };
                    push(bytes, byte.ok_or_else(|| fail(K::BadByte))?)?;
                }
                None if list.dot == Dot::Tail => (list.tail, list.dot) = (datum, Dot::Done),
                None => push(&mut items, datum)?,
            }
        }
    }

    /// The pairs of `list`, its items the last of `items`, which it takes, ending in its tail. Under
    /// a source name its first pair says where it was read, and whether it is an outer form. Each
    /// pair is a step of the walk `walked`, as each byte read is, so a stop ends a long list's too.
    fn close(
        &mut self,
        items: &mut Vec<Value>,
        list: &List,
        place: Option<(Symbol, u32)>,
        outer: bool,
        library: bool,
        walked: &mut usize,
    ) -> Result<Value, Error> {
        let mut v = list.tail;
        for (i, &x) in items.iter().enumerate().skip(list.from).rev() {
            self.control.walk(walked)?;
            let first = i == list.from;
            let origin = Origin {
                place: place.filter(|_| first),
                library,
                outer: outer && first && place.is_some(),
            };
            v = Value::Pair(self.heap.make((x, v, origin))?);
        }
        items.truncate(list.from);
        Ok(v)
    }

    /// An atom: a constant, a number or a symbol. `fail` makes the error for a token that is none
    /// of them. A number starts with a digit, after an optional sign: an integer if it is one, else
    /// a float if it has a `.` or an exponent, as Rust reads them.
    fn atom(&mut self, token: &str, fail: impl Fn(K) -> Error) -> Result<Value, Error> {
        let v = match token {
            "#true" => Value::Boolean(true),
            "#false" => Value::Boolean(false),
            "#inert" => Value::Inert,
            "#ignore" => Value::Ignore,
            _ if token.starts_with('#') => {
                Value::Float(float_name(token).ok_or_else(|| fail(K::UnknownConstant))?)
            }
            _ if token
                .strip_prefix(['+', '-'])
                .unwrap_or(token)
                .starts_with(|c: char| c.is_ascii_digit()) =>
            {
                match token.parse() {
                    Ok(n) => Value::Integer(n),
                    Err(_) if token.contains(['.', 'e', 'E']) => {
                        Value::Float(token.parse().map_err(|_| fail(K::BadNumber))?)
                    }
                    Err(_) => return Err(fail(K::BadNumber)),
                }
            }
            _ => Value::Symbol(self.heap.intern(token)?),
        };
        Ok(v)
    }

    /// A string after its opening quote, at offset `start`, with the escapes `\\`, `\"`, `\n`,
    /// `\r`, `\t` and `\xHEX;` (a Unicode scalar value in hex).
    fn read_string(&mut self, cur: &mut Cursor, start: usize) -> Result<Value, Error> {
        let (src, source) = (cur.src, cur.source);
        let unclosed = || parse_error(src, start, K::UnclosedString, source);
        let mut s = TextBuffer::new();
        loop {
            let escape = cur.at;
            let c = match cur.next()?.ok_or_else(unclosed)? {
                '"' => break,
                '\\' => match cur.next()?.ok_or_else(unclosed)? {
                    '\\' => '\\',
                    '"' => '"',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'x' => {
                        let begin = cur.at;
                        while cur.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                            cur.next()?;
                        }
                        let hex = src.get(begin..cur.at).unwrap_or_default();
                        let c = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
                        match (cur.next()?.ok_or_else(unclosed)?, c) {
                            (';', Some(c)) => c,
                            _ => return Err(parse_error(src, escape, K::BadEscape, source)),
                        }
                    }
                    _ => return Err(parse_error(src, escape, K::BadEscape, source)),
                },
                c => c,
            };
            // All a text buffer refuses for is memory to grow.
            s.write_char(c).map_err(|_| Error::oom())?;
        }
        self.string_local(s.as_str())
    }
}
