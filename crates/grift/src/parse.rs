//! S-expression parser.
//!
//! Defines the [`CharSource`] trait and a single generic parser that works
//! with any character source: byte slices or CharPair chains.
//!
//! Two `CharSource` implementations cover all parsing needs:
//! - [`SliceSource`] for `&str` / `&[u8]` input (file contents, `eval`)
//! - [`ChainSource`] for parsing existing `CharPair` chains (`raw-read-string`)
//!
//! The parser is recursive-descent with support for:
//! - Proper and dotted lists: `(a b c)`, `(a . b)`
//! - Quote shorthand: `'x` → `(quote x)`
//! - String literals with escape sequences: `"hello\n"`
//! - Atoms: booleans (`#t`, `#f`), `#inert`, `#ignore`, integers, symbols
//! - Line comments: `; comment`

use crate::arena::{Arena, ArenaError, ArenaIndex, ArenaResult};

use crate::lisp::Lisp;
use crate::value::Value;

// ── CharSource trait ──────────────────────────────────────────────

/// Abstraction over character input sources for the parser.
pub(crate) trait CharSource {
    /// Read and consume the next character, or `None` at end-of-input.
    fn read_char(&mut self) -> Option<char>;
    /// Peek at the next character without consuming it, or `None` at end-of-input.
    fn peek_char(&mut self) -> Option<char>;
    /// Return the current 1-based (line, column) position in the source.
    ///
    /// Returns `(0, 0)` when position tracking is not available
    /// (e.g. when parsing from an arena chain).
    fn position(&self) -> (u32, u32);
}

/// Build a [`ArenaError::ParseError`] carrying the current source position.
fn parse_error(src: &impl CharSource) -> ArenaError {
    let (line, col) = src.position();
    ArenaError::ParseError { line, col }
}

// ── SliceSource ───────────────────────────────────────────────────

/// Character source backed by a `&[u8]` byte slice (ASCII / UTF-8 source text).
pub(crate) struct SliceSource<'a> {
    input: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> SliceSource<'a> {
    pub fn new(input: &'a str) -> Self {
        SliceSource {
            input: input.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    /// Returns `true` if there is remaining non-whitespace input.
    ///
    /// Skips whitespace and comments so that the next `parse_expr` call
    /// will immediately see meaningful input.
    pub fn has_more(&mut self) -> bool {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b'\n' => {
                    self.pos += 1;
                    self.line += 1;
                    self.col = 1;
                }
                b' ' | b'\t' | b'\r' => {
                    self.pos += 1;
                    self.col += 1;
                }
                b';' => {
                    while self.pos < self.input.len() && self.input[self.pos] != b'\n' {
                        self.pos += 1;
                        self.col += 1;
                    }
                }
                _ => return true,
            }
        }
        false
    }
}

impl CharSource for SliceSource<'_> {
    fn read_char(&mut self) -> Option<char> {
        if self.pos < self.input.len() {
            let ch = self.input[self.pos] as char;
            self.pos += 1;
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            Some(ch)
        } else {
            None
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        if self.pos < self.input.len() {
            Some(self.input[self.pos] as char)
        } else {
            None
        }
    }

    fn position(&self) -> (u32, u32) {
        (self.line, self.col)
    }
}

// ── ChainSource ───────────────────────────────────────────────────

/// Character source backed by an arena `CharPair` chain.
pub(crate) struct ChainSource<'a, const N: usize> {
    arena: &'a Arena<Value, N>,
    cursor: ArenaIndex,
}

impl<'a, const N: usize> ChainSource<'a, N> {
    pub fn new(arena: &'a Arena<Value, N>, cursor: ArenaIndex) -> Self {
        ChainSource { arena, cursor }
    }
}

impl<const N: usize> CharSource for ChainSource<'_, N> {
    fn read_char(&mut self) -> Option<char> {
        if self.cursor.is_nil() {
            return None;
        }
        match self.arena.get(self.cursor) {
            Ok(Value::CharPair { ch, cdr }) => {
                self.cursor = cdr;
                Some(ch)
            }
            _ => None,
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        if self.cursor.is_nil() {
            return None;
        }
        match self.arena.get(self.cursor) {
            Ok(Value::CharPair { ch, .. }) => Some(ch),
            _ => None,
        }
    }

    fn position(&self) -> (u32, u32) {
        (0, 0)
    }
}

// ── Unified parser (methods on Lisp) ──────────────────────────────

/// Returns `true` if `c` is a delimiter that terminates atoms and dot
/// separators.
fn is_delimiter(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '(' | ')' | '"' | ';')
}

impl<const N: usize> Lisp<N> {
    /// Parse one s-expression from a character source.
    ///
    /// Returns the parsed value, or `ArenaIndex::NIL` if the source is
    /// exhausted (no more input).
    pub(crate) fn parse_expr(&self, src: &mut impl CharSource) -> ArenaResult<ArenaIndex> {
        self.skip_ws(src);
        let ch = match src.read_char() {
            Some(c) => c,
            None => return Ok(ArenaIndex::NIL),
        };
        match ch {
            '(' => self.parse_list(src),
            '\'' => self.parse_quote(src),
            '"' => self.parse_string(src),
            ')' => Err(parse_error(src)),
            _ => self.parse_atom_from(ch, src),
        }
    }

    /// Skip whitespace and `;`-comments.
    pub(crate) fn skip_ws(&self, src: &mut impl CharSource) {
        loop {
            match src.peek_char() {
                Some(' ' | '\t' | '\n' | '\r') => {
                    let _ = src.read_char();
                }
                Some(';') => {
                    let _ = src.read_char();
                    loop {
                        match src.read_char() {
                            Some('\n') | None => break,
                            _ => {}
                        }
                    }
                }
                _ => break,
            }
        }
    }

    /// Parse a list: `(a b c)` or dotted pair `(a . b)`.
    fn parse_list(&self, src: &mut impl CharSource) -> ArenaResult<ArenaIndex> {
        self.skip_ws(src);
        match src.peek_char() {
            None => return Err(parse_error(src)), // unterminated
            Some(')') => {
                src.read_char();
                return Ok(ArenaIndex::NIL);
            }
            Some('.') => {
                src.read_char(); // consume '.'
                if src.peek_char().is_none_or(is_delimiter) {
                    // Dot separator at start of list — no car element.
                    return Err(parse_error(src));
                }
                // Symbol starting with '.'
                let expr = self.parse_atom_from('.', src)?;
                let rest = self.parse_list(src)?;
                return self.cons(expr, rest);
            }
            _ => {}
        }

        let car = self.parse_expr(src)?;

        // Check for dotted pair after first element.
        self.skip_ws(src);
        if let Some('.') = src.peek_char() {
            src.read_char(); // consume '.'
            if src.peek_char().is_none_or(is_delimiter) {
                // Dotted pair: (car . cdr)
                let cdr = self.parse_expr(src)?;
                self.skip_ws(src);
                match src.read_char() {
                    Some(')') => return self.cons(car, cdr),
                    _ => return Err(parse_error(src)),
                }
            }
            // Symbol starting with '.' in cdr position
            let atom = self.parse_atom_from('.', src)?;
            let rest = self.parse_list(src)?;
            let cdr = self.cons(atom, rest)?;
            return self.cons(car, cdr);
        }

        let cdr = self.parse_list(src)?;
        self.cons(car, cdr)
    }

    /// Parse `'expr` → `(quote expr)`.
    fn parse_quote(&self, src: &mut impl CharSource) -> ArenaResult<ArenaIndex> {
        let expr = self.parse_expr(src)?;
        let quote_sym = self.symbol("quote")?;
        let inner = self.cons(expr, ArenaIndex::NIL)?;
        self.cons(quote_sym, inner)
    }

    /// Parse a string literal (opening `"` already consumed).
    ///
    /// Processes escape sequences: `\n`, `\t`, `\r`, `\\`, `\"`.
    /// Builds a CharPair chain in reverse, then reverses.
    fn parse_string(&self, src: &mut impl CharSource) -> ArenaResult<ArenaIndex> {
        let mut head = ArenaIndex::NIL;
        loop {
            let ch = match src.read_char() {
                Some(c) => c,
                None => return Err(parse_error(src)),
            };
            if ch == '"' {
                break;
            }
            let actual = if ch == '\\' {
                let esc = match src.read_char() {
                    Some(c) => c,
                    None => return Err(parse_error(src)),
                };
                match esc {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '\\' => '\\',
                    '"' => '"',
                    _ => return Err(ArenaError::InvalidArgument),
                }
            } else {
                ch
            };
            head = self.prepend_char(head, actual)?;
        }
        self.reverse_chain(head)
    }

    /// Parse an atom given the first character (already consumed).
    ///
    /// Reads remaining atom characters, builds a CharPair chain,
    /// then classifies (boolean, number, or symbol).
    fn parse_atom_from(&self, first: char, src: &mut impl CharSource) -> ArenaResult<ArenaIndex> {
        let mut head = self.prepend_char(ArenaIndex::NIL, first)?;
        loop {
            match src.peek_char() {
                None => break,
                Some(c) if is_delimiter(c) => break,
                _ => {
                    let c = src.read_char().unwrap();
                    head = self.prepend_char(head, c)?;
                }
            }
        }
        self.classify_atom(self.reverse_chain(head)?)
    }
}
