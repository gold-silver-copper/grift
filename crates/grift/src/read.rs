//! The reader: source text to a list of forms, with an explicit stack of open lists.

use crate::heap::Obj;
use crate::{Error, Grift, K, Value, push};
use alloc::string::String;
use alloc::vec::Vec;
use core::iter::Peekable;
use core::str::CharIndices;

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

impl Grift {
    /// Reads every form in `src` into a list. A `parse-error` carries the line and column
    /// where reading failed; an unfinished form fails at the end of the text.
    pub(crate) fn read(&mut self, src: &str) -> Result<Value, Error> {
        let mut cur = Cursor {
            src,
            chars: src.char_indices().peekable(),
            line: 1,
            col: 1,
        };
        let mut top = List {
            head: Value::Nil,
            last: Value::Nil,
            dot: Dot::No,
        };
        let mut open: Vec<List> = Vec::new();
        loop {
            cur.skip_space();
            let at = [Value::Int(cur.line), Value::Int(cur.col)];
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
                            head: Value::Nil,
                            last: Value::Nil,
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
                            Some(list) if list.dot == Dot::No && list.last != Value::Nil => {
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
                    let pair = self.cons(datum, Value::Nil)?;
                    match list.last {
                        Value::Pair(r) => self.set_cdr(r, pair)?,
                        _ => list.head = pair,
                    }
                    list.last = pair;
                }
                Dot::Tail => {
                    if let Value::Pair(r) = list.last {
                        self.set_cdr(r, datum)?;
                    }
                    list.dot = Dot::Done;
                }
                Dot::Done => return Err(fail()),
            }
        }
    }

    /// Sets the tail of a pair the reader made and nobody else has seen yet.
    fn set_cdr(&mut self, r: crate::Ref, v: Value) -> Result<(), Error> {
        if let Obj::Pair(_, d) = self.heap.get_mut(r)? {
            *d = v;
        }
        Ok(())
    }

    /// An atom: a constant, an integer or a symbol. `None` for a token that is neither.
    fn atom(&mut self, token: &str) -> Option<Result<Value, Error>> {
        let v = match token {
            "#t" => Value::True,
            "#f" => Value::False,
            "#inert" => Value::Inert,
            "#ignore" => Value::Ignore,
            _ if token.starts_with('#') => return None,
            _ if token
                .trim_start_matches(['+', '-'])
                .starts_with(|c: char| c.is_ascii_digit()) =>
            {
                Value::Int(token.parse().ok()?)
            }
            _ => return Some(self.symbol(token).map(Value::Sym)),
        };
        Some(Ok(v))
    }

    /// A string after its opening quote, with the escapes `\\`, `\"` and `\n`.
    fn read_string(&mut self, cur: &mut Cursor) -> Result<Value, Error> {
        let mut s = String::new();
        let fail =
            |cur: &Cursor| Error::new(K::ParseError, &[Value::Int(cur.line), Value::Int(cur.col)]);
        loop {
            let c = match cur.next() {
                Some((_, '"')) => break,
                Some((_, '\\')) => match cur.next() {
                    Some((_, '\\')) => '\\',
                    Some((_, '"')) => '"',
                    Some((_, 'n')) => '\n',
                    _ => return Err(fail(cur)),
                },
                Some((_, c)) => c,
                None => return Err(fail(cur)),
            };
            s.try_reserve(c.len_utf8()).map_err(|_| Error::oom())?;
            s.push(c);
        }
        // An exact copy, so boxing it never reallocates.
        let mut boxed = String::new();
        boxed.try_reserve_exact(s.len()).map_err(|_| Error::oom())?;
        boxed.push_str(&s);
        Ok(Value::Str(
            self.heap.alloc(Obj::Str(boxed.into_boxed_str()))?,
        ))
    }
}
