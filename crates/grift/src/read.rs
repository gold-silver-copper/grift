//! The reader: source text to forms, with an explicit stack of open lists.

use crate::evaluate::integer;
use crate::heap::Object;
use crate::{Error, Grift, K, Value, push};
use std::ops::Range;

/// A list being read: its first pair, its last pair, where a dotted tail stands, and the offset
/// of its `(`.
struct List {
    head: Value,
    last: Value,
    dot: Dot,
    start: usize,
}

impl List {
    fn new(start: usize) -> List {
        List {
            head: Value::EmptyList,
            last: Value::EmptyList,
            dot: Dot::No,
            start,
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
    /// The byte offset of the next character.
    at: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<char> {
        self.src.get(self.at..)?.chars().next()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        Some(c)
    }

    fn skip_space(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                ';' => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.next();
                    }
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
    /// Reads every form in `src` into a list, evaluating nothing. A `parse-error` carries why and
    /// the line and column: of the `(` or `"` left open, or of what didn't read.
    pub fn read(&mut self, src: &str) -> Result<Value, Error> {
        let mut forms = List::new(0);
        let mut at = 0;
        while let Some((form, place)) = self.read_form(src, at)? {
            self.add(&mut forms, form)?;
            at = place.end;
        }
        Ok(forms.head)
    }

    /// Reads the form after byte offset `from` of `src`, a character boundary: the form and where
    /// it is, or `None` if only space and comments are left.
    pub(crate) fn read_form(
        &mut self,
        src: &str,
        from: usize,
    ) -> Result<Option<(Value, Range<usize>)>, Error> {
        let mut cur = Cursor { src, at: from };
        // A first line starting with `#!` reads as a comment, so a script can run itself.
        if from == 0 && src.starts_with("#!") {
            cur.at = src.find('\n').unwrap_or(src.len());
        }
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
                    push(&mut open, List::new(begin))?;
                    continue;
                }
                ')' => match open.pop() {
                    Some(list) if list.dot != Dot::Tail => list.head,
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
                    let reason = if token.starts_with('#') {
                        K::UnknownConstant
                    } else {
                        K::BadNumber
                    };
                    self.atom(token).ok_or_else(|| fail(reason))??
                }
            };
            let Some(list) = open.last_mut() else {
                return Ok(Some((datum, start..cur.at)));
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
        let pair = self.pair(datum, Value::EmptyList)?;
        match list.last {
            Value::Pair(r) => self.set_rest(r, pair)?,
            _ => list.head = pair,
        }
        list.last = pair;
        Ok(())
    }

    /// Sets the rest of a pair the reader made and nobody else has seen yet.
    fn set_rest(&mut self, r: crate::Reference, v: Value) -> Result<(), Error> {
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
                .trim_start_matches(['+', '-'])
                .starts_with(|c: char| c.is_ascii_digit()) =>
            {
                match token.parse() {
                    Ok(n) => Value::Integer(n),
                    Err(_) if token.contains(['.', 'e', 'E']) => Value::Float(token.parse().ok()?),
                    Err(_) => return None,
                }
            }
            _ => return Some(self.symbol(token).map(Value::Symbol)),
        };
        Some(Ok(v))
    }

    /// A string after its opening quote, at offset `start`, with the escapes `\\`, `\"`, `\n`,
    /// `\t` and `\xHEX;` (a Unicode scalar value in hex).
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
                            (None, _) => return Err(parse_error(cur.src, start, K::UnclosedString)),
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
        self.string(&s)
    }
}
