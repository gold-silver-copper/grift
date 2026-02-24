//! S-expression parser.
//!
//! Tokenizes and parses Lisp source text into arena-allocated values.

use grift_arena::{ArenaIndex, ArenaError, ArenaResult};

use crate::io::IoProvider;
use crate::lisp::Lisp;

/// A simple S-expression parser.
pub(crate) struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    /// Create a new parser for the given input string.
    pub fn new(input: &'a str) -> Self {
        Parser {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    /// Returns `true` if there is remaining input to parse.
    pub fn has_more(&mut self) -> bool {
        self.skip_whitespace();
        self.pos < self.input.len()
    }

    /// Parse one expression.
    pub fn parse<const N: usize, IO: IoProvider>(&mut self, lisp: &Lisp<N, IO>) -> ArenaResult<ArenaIndex> {
        self.skip_whitespace();
        if self.pos >= self.input.len() {
            return Ok(ArenaIndex::NIL);
        }

        match self.input[self.pos] {
            b'(' => {
                self.pos += 1;
                self.parse_list(lisp)
            }
            b'\'' => {
                self.pos += 1;
                self.parse_quote(lisp)
            }
            b'"' => {
                self.pos += 1;
                self.parse_string_literal(lisp)
            }
            b')' => Err(ArenaError::ParseError),
            _ => self.parse_atom(lisp),
        }
    }

    /// Skip whitespace and comments.
    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                b';' => {
                    while self.pos < self.input.len() && self.input[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    /// Parse a list: `(a b c)` → cons cells.
    fn parse_list<const N: usize, IO: IoProvider>(&mut self, lisp: &Lisp<N, IO>) -> ArenaResult<ArenaIndex> {
        self.skip_whitespace();

        if self.pos >= self.input.len() {
            return Err(ArenaError::ParseError);
        }

        if self.input[self.pos] == b')' {
            self.pos += 1;
            return Ok(ArenaIndex::NIL);
        }

        if self.peek_dot() {
            return Err(ArenaError::ParseError);
        }

        let car = self.parse(lisp)?;

        self.skip_whitespace();
        if self.peek_dot() {
            self.pos += 1; // skip the dot
            let cdr = self.parse(lisp)?;
            self.skip_whitespace();
            if self.pos < self.input.len() && self.input[self.pos] == b')' {
                self.pos += 1;
                return lisp.cons(car, cdr);
            }
            return Err(ArenaError::ParseError);
        }

        let cdr = self.parse_list(lisp)?;
        lisp.cons(car, cdr)
    }

    /// Check if current position is a dot separator (not a number like `.5`).
    fn peek_dot(&self) -> bool {
        self.input.get(self.pos) == Some(&b'.')
            && self.input.get(self.pos + 1)
                .is_none_or(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b')'))
    }

    /// Parse `'expr` → `(quote expr)`.
    fn parse_quote<const N: usize, IO: IoProvider>(&mut self, lisp: &Lisp<N, IO>) -> ArenaResult<ArenaIndex> {
        let expr = self.parse(lisp)?;
        let quote_sym = lisp.symbol("quote")?;
        let inner = lisp.cons(expr, ArenaIndex::NIL)?;
        lisp.cons(quote_sym, inner)
    }

    /// Parse a string literal `"..."`.
    fn parse_string_literal<const N: usize, IO: IoProvider>(
        &mut self,
        lisp: &Lisp<N, IO>,
    ) -> ArenaResult<ArenaIndex> {
        let start = self.pos;
        while self.pos < self.input.len() && self.input[self.pos] != b'"' {
            if self.input[self.pos] == b'\\' {
                self.pos += 1; // skip escaped char
            }
            self.pos += 1;
        }
        if self.pos >= self.input.len() {
            return Err(ArenaError::ParseError);
        }
        let end = self.pos;
        self.pos += 1; // skip closing quote

        let slice = &self.input[start..end];
        let s = core::str::from_utf8(slice).map_err(|_| ArenaError::ParseError)?;
        lisp.alloc_string(s)
    }

    /// Parse an atom: number, symbol, or boolean.
    fn parse_atom<const N: usize, IO: IoProvider>(&mut self, lisp: &Lisp<N, IO>) -> ArenaResult<ArenaIndex> {
        let start = self.pos;
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' | b'(' | b')' | b'"' | b';' => break,
                _ => self.pos += 1,
            }
        }

        let token = &self.input[start..self.pos];
        let s = core::str::from_utf8(token).map_err(|_| ArenaError::ParseError)?;

        match s {
            "#t" | "#true" => Ok(ArenaIndex::TRUE),
            "#f" | "#false" => Ok(ArenaIndex::FALSE),
            "#inert" => Ok(ArenaIndex::INERT),
            "#ignore" => Ok(ArenaIndex::IGNORE),
            _ => parse_integer(s)
                .map(|n| lisp.number(n))
                .unwrap_or_else(|| lisp.symbol(s)),
        }
    }
}

/// Parse an integer from a string slice without using std.
fn parse_integer(s: &str) -> Option<isize> {
    let bytes = s.as_bytes();
    let (&first, rest) = bytes.split_first()?;

    let (negative, digits) = match first {
        b'-' if !rest.is_empty() => (true, rest),
        b'+' if !rest.is_empty() => (false, rest),
        b'0'..=b'9' => (false, bytes),
        _ => return None,
    };

    let magnitude = digits.iter().try_fold(0isize, |acc, &b| {
        if !b.is_ascii_digit() { return None; }
        acc.checked_mul(10)?.checked_add((b - b'0') as isize)
    })?;

    Some(if negative { -magnitude } else { magnitude })
}
