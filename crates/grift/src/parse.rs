//! S-expression parser.
//!
//! Tokenizes and parses Lisp source text into arena-allocated values.

use grift_arena::{ArenaIndex, ArenaError, ArenaResult};

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

    /// Parse one expression.
    pub fn parse<const N: usize>(&mut self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        self.skip_whitespace();
        if self.pos >= self.input.len() {
            return lisp.nil();
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
            b')' => Err(ArenaError::InvalidIndex),
            _ => self.parse_atom(lisp),
        }
    }

    /// Skip whitespace and comments.
    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                b';' => {
                    // Skip line comment
                    while self.pos < self.input.len() && self.input[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    /// Parse a list: `(a b c)` → cons cells.
    fn parse_list<const N: usize>(&mut self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        self.skip_whitespace();

        if self.pos >= self.input.len() {
            return Err(ArenaError::InvalidIndex);
        }

        if self.input[self.pos] == b')' {
            self.pos += 1;
            return lisp.nil();
        }

        // Check for dotted pair notation
        if self.peek_dot() {
            return Err(ArenaError::InvalidIndex);
        }

        let car = self.parse(lisp)?;

        // Check for dot (improper list)
        self.skip_whitespace();
        if self.peek_dot() {
            self.pos += 1; // skip the dot
            let cdr = self.parse(lisp)?;
            self.skip_whitespace();
            if self.pos < self.input.len() && self.input[self.pos] == b')' {
                self.pos += 1;
                return lisp.cons(car, cdr);
            }
            return Err(ArenaError::InvalidIndex);
        }

        let cdr = self.parse_list(lisp)?;
        lisp.cons(car, cdr)
    }

    /// Check if current position is a dot separator (not a number like `.5`).
    fn peek_dot(&self) -> bool {
        if self.pos >= self.input.len() {
            return false;
        }
        if self.input[self.pos] != b'.' {
            return false;
        }
        // A dot followed by whitespace or ')' or EOF is a dot separator
        let next = self.pos + 1;
        next >= self.input.len()
            || self.input[next] == b' '
            || self.input[next] == b'\t'
            || self.input[next] == b'\n'
            || self.input[next] == b'\r'
            || self.input[next] == b')'
    }

    /// Parse `'expr` → `(quote expr)`.
    fn parse_quote<const N: usize>(&mut self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        let expr = self.parse(lisp)?;
        let quote_sym = lisp.symbol("quote")?;
        let nil = lisp.nil()?;
        let inner = lisp.cons(expr, nil)?;
        lisp.cons(quote_sym, inner)
    }

    /// Parse a string literal `"..."`.
    fn parse_string_literal<const N: usize>(
        &mut self,
        lisp: &Lisp<N>,
    ) -> ArenaResult<ArenaIndex> {
        let start = self.pos;
        while self.pos < self.input.len() && self.input[self.pos] != b'"' {
            if self.input[self.pos] == b'\\' {
                self.pos += 1; // skip escaped char
            }
            self.pos += 1;
        }
        if self.pos >= self.input.len() {
            return Err(ArenaError::InvalidIndex);
        }
        let end = self.pos;
        self.pos += 1; // skip closing quote

        let slice = &self.input[start..end];
        // Convert to &str (we know input is valid UTF-8)
        let s = core::str::from_utf8(slice).map_err(|_| ArenaError::InvalidIndex)?;
        lisp.alloc_string(s)
    }

    /// Parse an atom: number, symbol, or boolean.
    fn parse_atom<const N: usize>(&mut self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        let start = self.pos;
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' | b'(' | b')' | b'"' | b';' => break,
                _ => self.pos += 1,
            }
        }

        let token = &self.input[start..self.pos];
        let s = core::str::from_utf8(token).map_err(|_| ArenaError::InvalidIndex)?;

        // Check for booleans
        if s == "#t" || s == "#true" {
            return lisp.boolean(true);
        }
        if s == "#f" || s == "#false" {
            return lisp.boolean(false);
        }

        // Try to parse as number
        if let Some(n) = parse_integer(s) {
            return lisp.number(n);
        }

        // Otherwise it's a symbol
        lisp.symbol(s)
    }
}

/// Parse an integer from a string slice without using std.
fn parse_integer(s: &str) -> Option<isize> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let (negative, start) = if bytes[0] == b'-' {
        if bytes.len() == 1 {
            return None; // just "-"
        }
        (true, 1)
    } else if bytes[0] == b'+' {
        if bytes.len() == 1 {
            return None; // just "+"
        }
        (false, 1)
    } else {
        (false, 0)
    };

    let mut result: isize = 0;
    for &b in &bytes[start..] {
        if b < b'0' || b > b'9' {
            return None;
        }
        result = result.checked_mul(10)?;
        result = result.checked_add((b - b'0') as isize)?;
    }

    if negative {
        Some(-result)
    } else {
        Some(result)
    }
}
