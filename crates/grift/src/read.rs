//! The reader: source text to a list of forms, with an explicit stack of open lists.

use crate::heap::Object;
use crate::{Error, Grift, K, Value, push};
use std::iter::Peekable;
use std::str::CharIndices;

/// A list being read: its first pair, its last pair, and where a dotted tail stands.
struct List {
    head: Value,
    last: Value,
    dot: Dot,
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
    chars: Peekable<CharIndices<'a>>,
    line: i64,
    col: i64,
}

impl Cursor<'_> {
    fn next(&mut self) -> Option<(usize, char)> {
        let (i, c) = self.chars.next()?;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some((i, c))
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|&(_, c)| c)
    }

    fn end(&mut self) -> usize {
        self.chars.peek().map_or(self.src.len(), |&(i, _)| i)
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

fn delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '"' | ';')
}

impl<T> Grift<T> {
    /// Reads every form in `src` into a list, evaluating nothing. A `parse-error` carries the
    /// line and column where reading failed; an unfinished form fails at the end of the text.
    pub fn read(&mut self, src: &str) -> Result<Value, Error> {
        let mut cur = Cursor {
            src,
            chars: src.char_indices().peekable(),
            line: 1,
            col: 1,
        };
        let mut top = List {
            head: Value::EmptyList,
            last: Value::EmptyList,
            dot: Dot::No,
        };
        let mut open: Vec<List> = Vec::new();
        loop {
            cur.skip_space();
            let at = [Value::Integer(cur.line), Value::Integer(cur.col)];
            let fail = || Error::new(K::ParseError, &at);
            let Some((start, c)) = cur.next() else {
                return if open.is_empty() {
                    Ok(top.head)
                } else {
                    Err(fail())
                };
            };
            let datum = match c {
                '(' => {
                    push(
                        &mut open,
                        List {
                            head: Value::EmptyList,
                            last: Value::EmptyList,
                            dot: Dot::No,
                        },
                    )?;
                    continue;
                }
                ')' => match open.pop() {
                    Some(list) if list.dot != Dot::Tail => list.head,
                    _ => return Err(fail()),
                },
                '"' => self.read_string(&mut cur)?,
                _ => {
                    while cur.peek().is_some_and(|c| !delimiter(c)) {
                        cur.next();
                    }
                    let token = src.get(start..cur.end()).unwrap_or_default();
                    if token == "." {
                        match open.last_mut() {
                            Some(list) if list.dot == Dot::No && list.last != Value::EmptyList => {
                                list.dot = Dot::Tail;
                                continue;
                            }
                            _ => return Err(fail()),
                        }
                    }
                    self.atom(token).ok_or_else(fail)??
                }
            };
            let list = open.last_mut().unwrap_or(&mut top);
            match list.dot {
                Dot::No => {
                    let pair = self.pair(datum, Value::EmptyList)?;
                    match list.last {
                        Value::Pair(r) => self.set_rest(r, pair)?,
                        _ => list.head = pair,
                    }
                    list.last = pair;
                }
                Dot::Tail => {
                    if let Value::Pair(r) = list.last {
                        self.set_rest(r, datum)?;
                    }
                    list.dot = Dot::Done;
                }
                Dot::Done => return Err(fail()),
            }
        }
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
            "#true" => Value::True,
            "#false" => Value::False,
            "#inert" => Value::Inert,
            "#ignore" => Value::Ignore,
            "#infinity" => Value::Float(f64::INFINITY),
            "#-infinity" => Value::Float(f64::NEG_INFINITY),
            "#not-a-number" => Value::Float(f64::NAN),
            _ if token.starts_with('#') => return None,
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

    /// A string after its opening quote, with the escapes `\\`, `\"`, `\n`, `\t` and `\xHEX;`
    /// (a Unicode scalar value in hex).
    fn read_string(&mut self, cur: &mut Cursor) -> Result<Value, Error> {
        let mut s = String::new();
        let fail = |cur: &Cursor| {
            Error::new(
                K::ParseError,
                &[Value::Integer(cur.line), Value::Integer(cur.col)],
            )
        };
        loop {
            let c = match cur.next() {
                Some((_, '"')) => break,
                Some((_, '\\')) => match cur.next() {
                    Some((_, '\\')) => '\\',
                    Some((_, '"')) => '"',
                    Some((_, 'n')) => '\n',
                    Some((_, 't')) => '\t',
                    Some((at, 'x')) => {
                        while cur.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                            cur.next();
                        }
                        let hex = cur.src.get(at + 1..cur.end()).unwrap_or_default();
                        let c = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
                        match (cur.next(), c) {
                            (Some((_, ';')), Some(c)) => c,
                            _ => return Err(fail(cur)),
                        }
                    }
                    _ => return Err(fail(cur)),
                },
                Some((_, c)) => c,
                None => return Err(fail(cur)),
            };
            s.try_reserve(c.len_utf8()).map_err(|_| Error::oom())?;
            s.push(c);
        }
        self.string(&s)
    }
}
