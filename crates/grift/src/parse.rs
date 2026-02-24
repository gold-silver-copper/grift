//! S-expression parser.
//!
//! Defines the [`CharSource`] trait and a single generic parser that works
//! with any character source: byte slices, I/O streams, or CharPair chains.
//!
//! Three `CharSource` implementations cover all parsing needs:
//! - [`SliceSource`] for `&str` / `&[u8]` input (file contents, eval)
//! - [`StreamSource`] for reading via [`IoProvider`] streams (stdin, file handles)
//! - [`ChainSource`] for parsing existing `CharPair` chains (raw-read-string)

use core::cell::RefCell;

use grift_arena::{Arena, ArenaError, ArenaIndex, ArenaResult};

use crate::io::IoProvider;
use crate::lisp::Lisp;
use crate::value::Value;

// ── CharSource trait ──────────────────────────────────────────────

/// Abstraction over character input sources for the parser.
pub(crate) trait CharSource {
    /// Read and consume the next character, or `None` at end-of-input.
    fn read_char(&mut self) -> Option<char>;
    /// Peek at the next character without consuming it, or `None` at end-of-input.
    fn peek_char(&mut self) -> Option<char>;
}

// ── SliceSource ───────────────────────────────────────────────────

/// Character source backed by a `&[u8]` byte slice (ASCII / UTF-8 source text).
pub(crate) struct SliceSource<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> SliceSource<'a> {
    pub fn new(input: &'a str) -> Self {
        SliceSource { input: input.as_bytes(), pos: 0 }
    }

    /// Returns `true` if there is remaining non-whitespace input.
    ///
    /// Skips whitespace and comments so that the next `parse_expr` call
    /// will immediately see meaningful input.
    pub fn has_more(&mut self) -> bool {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                b';' => {
                    while self.pos < self.input.len() && self.input[self.pos] != b'\n' {
                        self.pos += 1;
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
}

// ── StreamSource ──────────────────────────────────────────────────

/// Character source backed by an [`IoProvider`] stream.
///
/// Reads characters from a specific stream number (0 = stdin, 3+ = file handles).
pub(crate) struct StreamSource<'a, IO: IoProvider> {
    io: &'a RefCell<IO>,
    stream: u8,
}

impl<'a, IO: IoProvider> StreamSource<'a, IO> {
    pub fn new(io: &'a RefCell<IO>, stream: u8) -> Self {
        StreamSource { io, stream }
    }
}

impl<IO: IoProvider> CharSource for StreamSource<'_, IO> {
    fn read_char(&mut self) -> Option<char> {
        self.io.borrow_mut().read_stream_char(self.stream).ok()
    }

    fn peek_char(&mut self) -> Option<char> {
        self.io.borrow_mut().peek_stream_char(self.stream).ok()
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
}

// ── Unified parser (methods on Lisp) ──────────────────────────────

/// Returns `true` if `c` is a delimiter that terminates atoms and dot
/// separators.
fn is_delimiter(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '(' | ')' | '"' | ';')
}

impl<const N: usize, IO: IoProvider> Lisp<N, IO> {
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
            ')' => Err(ArenaError::ParseError),
            _ => self.parse_atom_from(ch, src),
        }
    }

    /// Skip whitespace and `;`-comments.
    pub(crate) fn skip_ws(&self, src: &mut impl CharSource) {
        loop {
            match src.peek_char() {
                Some(' ' | '\t' | '\n' | '\r') => { let _ = src.read_char(); }
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
            None => return Err(ArenaError::ParseError), // unterminated
            Some(')') => {
                src.read_char();
                return Ok(ArenaIndex::NIL);
            }
            Some('.') => {
                src.read_char(); // consume '.'
                if src.peek_char().is_none_or(|c| is_delimiter(c)) {
                    // Dot separator at start of list — no car element.
                    return Err(ArenaError::ParseError);
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
            if src.peek_char().is_none_or(|c| is_delimiter(c)) {
                // Dotted pair: (car . cdr)
                let cdr = self.parse_expr(src)?;
                self.skip_ws(src);
                match src.read_char() {
                    Some(')') => return self.cons(car, cdr),
                    _ => return Err(ArenaError::ParseError),
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
            let ch = src.read_char().ok_or(ArenaError::ParseError)?;
            if ch == '"' { break; }
            let actual = if ch == '\\' {
                let esc = src.read_char().ok_or(ArenaError::ParseError)?;
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
