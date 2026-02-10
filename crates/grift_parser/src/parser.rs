//! Parser for Lisp expressions
//!
//! This module contains the parser that builds arena-allocated AST nodes
//! from a token stream produced by the [`Lexer`](crate::lexer::Lexer).

use grift_arena::{ArenaIndex, ArenaError};
use crate::{Lisp, Value};
use crate::lexer::{Lexer, Token, LexError, LexErrorKind};

// Re-export SourceLoc from lexer for backward compatibility
pub use crate::lexer::SourceLoc;

// ============================================================================
// Parser Error Types
// ============================================================================

/// Parser error with location
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub loc: SourceLoc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// Unexpected end of input
    UnexpectedEof,
    /// Unexpected character
    UnexpectedChar(char),
    /// Unmatched parenthesis
    UnmatchedParen,
    /// Number too large
    NumberOverflow,
    /// Arena is full
    OutOfMemory,
    /// Invalid hash literal
    InvalidHashLiteral,
    /// Invalid character literal
    InvalidCharLiteral,
    /// Invalid string escape sequence
    InvalidEscapeSequence,
    /// Unterminated string literal
    UnterminatedString,
}

impl ParseError {
    pub fn new(kind: ParseErrorKind, line: usize, column: usize) -> Self {
        ParseError { kind, loc: SourceLoc { line, column } }
    }
}

impl From<ArenaError> for ParseError {
    fn from(e: ArenaError) -> Self {
        ParseError {
            kind: match e {
                ArenaError::OutOfMemory => ParseErrorKind::OutOfMemory,
                _ => ParseErrorKind::OutOfMemory,
            },
            loc: SourceLoc::default(),
        }
    }
}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError {
            kind: match e.kind {
                LexErrorKind::UnexpectedEof => ParseErrorKind::UnexpectedEof,
                LexErrorKind::UnexpectedChar(c) => ParseErrorKind::UnexpectedChar(c),
                LexErrorKind::NumberOverflow => ParseErrorKind::NumberOverflow,
                LexErrorKind::InvalidHashLiteral => ParseErrorKind::InvalidHashLiteral,
                LexErrorKind::InvalidCharLiteral => ParseErrorKind::InvalidCharLiteral,
                LexErrorKind::InvalidEscapeSequence => ParseErrorKind::InvalidEscapeSequence,
                LexErrorKind::UnterminatedString => ParseErrorKind::UnterminatedString,
                LexErrorKind::StringTooLong => ParseErrorKind::OutOfMemory,
                LexErrorKind::InvalidRadixDigit => ParseErrorKind::InvalidHashLiteral,
            },
            loc: e.loc,
        }
    }
}

// ============================================================================
// Parser
// ============================================================================

/// Parser state — wraps a [`Lexer`] and builds arena-allocated AST nodes.
///
/// The parser consumes tokens from the lexer and constructs S-expression
/// trees in the arena. It handles list building, dotted pairs, quote
/// desugaring, and vector literals.
pub struct Parser<'a> {
    lexer: Lexer<'a>,
}

impl<'a> Parser<'a> {
    /// Create a new parser
    pub fn new(input: &'a str) -> Self {
        Parser { lexer: Lexer::new(input) }
    }
    
    /// Create from bytes
    pub fn from_bytes(input: &'a [u8]) -> Self {
        Parser { lexer: Lexer::from_bytes(input) }
    }
    
    /// Get a reference to the underlying lexer
    pub fn lexer(&self) -> &Lexer<'a> {
        &self.lexer
    }
    
    /// Get a mutable reference to the underlying lexer
    pub fn lexer_mut(&mut self) -> &mut Lexer<'a> {
        &mut self.lexer
    }
    
    /// Parse a single expression
    pub fn parse<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        let loc = self.lexer.loc();
        let spanned = self.lexer.next_token()
            .ok_or(ParseError { kind: ParseErrorKind::UnexpectedEof, loc })??;
        let token = spanned.token;
        let loc = spanned.loc;
        
        self.parse_token(token, loc, lisp)
    }
    
    /// Parse an expression starting from a given token
    fn parse_token<const N: usize>(&mut self, token: Token, loc: SourceLoc, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        match token {
            Token::LParen => self.parse_list(lisp),
            Token::RParen => Err(ParseError { kind: ParseErrorKind::UnmatchedParen, loc }),
            Token::Quote => self.parse_quote("quote", lisp),
            Token::Quasiquote => self.parse_quote("quasiquote", lisp),
            Token::Unquote => self.parse_quote("unquote", lisp),
            Token::UnquoteSplice => self.parse_quote("unquote-splicing", lisp),
            Token::SyntaxQuote => self.parse_quote("syntax", lisp),
            Token::Dot => Err(ParseError { kind: ParseErrorKind::UnexpectedChar('.'), loc }),
            Token::True => lisp.true_val().map_err(Into::into),
            Token::False => lisp.false_val().map_err(Into::into),
            Token::Number(n) => lisp.number(n).map_err(Into::into),
            Token::Float(f) => lisp.float(f).map_err(Into::into),
            Token::Char(c) => lisp.char(c).map_err(Into::into),
            Token::Symbol { len } => {
                let name = self.lexer.symbol_bytes(len);
                lisp.symbol_from_bytes(name).map_err(Into::into)
            }
            Token::String { len } => {
                let chars = self.lexer.string_chars(len);
                lisp.string_from_chars_interned(chars).map_err(Into::into)
            }
            Token::VectorOpen => self.parse_vector_literal(lisp),
            Token::BytevectorOpen => self.parse_bytevector_literal(lisp),
            Token::DatumComment => {
                // Skip the next datum, then parse the one after it
                self.parse(lisp)?; // parsed and discarded
                self.parse(lisp)
            }
        }
    }
    
    /// Parse a quoted expression: 'x -> (quote x), etc.
    fn parse_quote<const N: usize>(&mut self, sym_name: &str, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        let expr = self.parse(lisp)?;
        let sym = lisp.symbol(sym_name)?;
        let nil = lisp.nil()?;
        let quoted = lisp.cons(expr, nil)?;
        lisp.cons(sym, quoted).map_err(Into::into)
    }
    
    /// Parse a list (after consuming the opening paren via the lexer)
    fn parse_list<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        let nil = lisp.nil()?;
        let mut reversed = nil;
        let mut count = 0usize;

        loop {
            let loc = self.lexer.loc();
            let spanned = self.lexer.next_token()
                .ok_or(ParseError { kind: ParseErrorKind::UnexpectedEof, loc })??;
            let token = spanned.token;
            let loc = spanned.loc;

            match token {
                Token::RParen => break,
                Token::Dot => {
                    // Dotted pair: (a . b)
                    if count == 0 {
                        return Err(ParseError { kind: ParseErrorKind::UnexpectedChar('.'), loc });
                    }

                    let cdr = self.parse(lisp)?;

                    // Expect closing paren
                    let close_loc = self.lexer.loc();
                    let closing = self.lexer.next_token()
                        .ok_or(ParseError { kind: ParseErrorKind::UnexpectedEof, loc: close_loc })??;
                    if !matches!(closing.token, Token::RParen) {
                        return Err(ParseError { kind: ParseErrorKind::UnmatchedParen, loc: closing.loc });
                    }

                    // Reverse the accumulated list in-place with cdr as the tail
                    let mut prev = cdr;
                    let mut current = reversed;
                    while current != nil {
                        let next = lisp.cdr(current)?;
                        lisp.set_cdr(current, prev)?;
                        prev = current;
                        current = next;
                    }
                    return Ok(prev);
                }
                other => {
                    let elem = self.parse_token(other, loc, lisp)?;
                    reversed = lisp.cons(elem, reversed)?;
                    count += 1;
                }
            }
        }

        // Build proper list by reversing in-place
        if count == 0 {
            return Ok(nil);
        }
        let mut prev = nil;
        let mut current = reversed;
        while current != nil {
            let next = lisp.cdr(current)?;
            lisp.set_cdr(current, prev)?;
            prev = current;
            current = next;
        }
        Ok(prev)
    }
    
    /// Parse vector literal #(obj ...)
    fn parse_vector_literal<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        // The VectorOpen token has consumed only the '#' character;
        // the '(' must be consumed separately
        let open_loc = self.lexer.loc();
        let opening = self.lexer.next_token()
            .ok_or(ParseError { kind: ParseErrorKind::UnexpectedEof, loc: open_loc })??;
        if !matches!(opening.token, Token::LParen) {
            return Err(ParseError { kind: ParseErrorKind::InvalidHashLiteral, loc: opening.loc });
        }

        let nil = lisp.nil()?;
        let mut reversed = nil;
        let mut count = 0usize;

        loop {
            let elem_loc = self.lexer.loc();
            let spanned = self.lexer.next_token()
                .ok_or(ParseError { kind: ParseErrorKind::UnexpectedEof, loc: elem_loc })??;
            let token = spanned.token;
            let loc = spanned.loc;

            match token {
                Token::RParen => break,
                other => {
                    let elem = self.parse_token(other, loc, lisp)?;
                    reversed = lisp.cons(elem, reversed)?;
                    count += 1;
                }
            }
        }

        // Create the vector
        let placeholder = lisp.number(0)?;
        let vec = lisp.make_array(count, placeholder)?;

        // Fill from the end since the list is reversed
        let mut current = reversed;
        let mut i = count;
        while current != nil {
            i -= 1;
            let (car, next) = lisp.car_cdr(current)?;
            lisp.array_set(vec, i, car)?;
            current = next;
        }

        Ok(vec)
    }
    
    /// Parse bytevector literal #u8(byte ...)
    ///
    /// Each element must be an exact integer in the range 0–255.
    fn parse_bytevector_literal<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        // BytevectorOpen token consumed '#u8'; the '(' must be consumed separately
        let open_loc = self.lexer.loc();
        let opening = self.lexer.next_token()
            .ok_or(ParseError { kind: ParseErrorKind::UnexpectedEof, loc: open_loc })??;
        if !matches!(opening.token, Token::LParen) {
            return Err(ParseError { kind: ParseErrorKind::InvalidHashLiteral, loc: opening.loc });
        }

        let nil = lisp.nil()?;
        let mut reversed = nil;
        let mut count = 0usize;

        loop {
            let elem_loc = self.lexer.loc();
            let spanned = self.lexer.next_token()
                .ok_or(ParseError { kind: ParseErrorKind::UnexpectedEof, loc: elem_loc })??;
            let token = spanned.token;
            let loc = spanned.loc;

            match token {
                Token::RParen => break,
                Token::Number(n) => {
                    if !(0..=255).contains(&n) {
                        return Err(ParseError { kind: ParseErrorKind::InvalidHashLiteral, loc });
                    }
                    let num = lisp.number(n)?;
                    reversed = lisp.cons(num, reversed)?;
                    count += 1;
                }
                _ => {
                    return Err(ParseError { kind: ParseErrorKind::InvalidHashLiteral, loc });
                }
            }
        }

        // Create the bytevector and fill from the end
        let bv = lisp.make_bytevector(count, 0)?;
        let mut current = reversed;
        let mut i = count;
        while current != nil {
            i -= 1;
            let (car, next) = lisp.car_cdr(current)?;
            if let Value::Number(n) = lisp.get(car)? {
                lisp.bytevector_set(bv, i, n as u8)?;
            }
            current = next;
        }

        Ok(bv)
    }
    
    /// Check if there's more input (after whitespace)
    pub fn has_more(&mut self) -> bool {
        self.lexer.has_more()
    }
    
    /// Get current position for error reporting
    pub fn position(&self) -> (usize, usize) {
        self.lexer.position()
    }
}

/// Parse a string into a Lisp expression
pub fn parse<const N: usize>(lisp: &Lisp<N>, input: &str) -> Result<ArenaIndex, ParseError> {
    let mut parser = Parser::new(input);
    parser.parse(lisp)
}

/// Parse multiple expressions
pub fn parse_all<const N: usize>(lisp: &Lisp<N>, input: &str) -> Result<ArenaIndex, ParseError> {
    let mut parser = Parser::new(input);
    let mut results = [ArenaIndex::NIL; 64];
    let mut count = 0;

    while parser.has_more() && count < 64 {
        results[count] = parser.parse(lisp)?;
        count += 1;
    }

    // Build list of results
    let mut result = lisp.nil()?;
    for i in (0..count).rev() {
        result = lisp.cons(results[i], result)?;
    }
    Ok(result)
}
