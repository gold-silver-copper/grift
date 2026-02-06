//! Parser for Lisp expressions
//!
//! This module contains the parser that builds arena-allocated AST nodes
//! from a token stream produced by the [`Lexer`](crate::lexer::Lexer).

use grift_arena::{ArenaIndex, ArenaError};
use crate::Lisp;
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
    /// Vector literal exceeds maximum size (256 elements in no_std)
    VectorLiteralTooLarge,
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
            Token::Char(c) => lisp.char(c).map_err(Into::into),
            Token::Symbol { len } => {
                let name = self.lexer.symbol_bytes(len);
                lisp.symbol_from_bytes(name).map_err(Into::into)
            }
            Token::String { len } => {
                let chars = self.lexer.string_chars(len);
                lisp.string_from_chars(chars).map_err(Into::into)
            }
            Token::VectorOpen => self.parse_vector_literal(lisp),
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
        const MAX_LIST_DEPTH: usize = 128;
        let mut elements: [ArenaIndex; MAX_LIST_DEPTH] = [ArenaIndex::NIL; MAX_LIST_DEPTH];
        let mut count = 0;
        
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
                    
                    // Build the dotted list
                    let mut result = cdr;
                    for i in (0..count).rev() {
                        result = lisp.cons(elements[i], result)?;
                    }
                    return Ok(result);
                }
                other => {
                    if count >= MAX_LIST_DEPTH {
                        return Err(ParseError { kind: ParseErrorKind::OutOfMemory, loc });
                    }
                    elements[count] = self.parse_token(other, loc, lisp)?;
                    count += 1;
                }
            }
        }
        
        // Build proper list
        if count == 0 {
            return lisp.nil().map_err(Into::into);
        }
        let mut result = lisp.nil()?;
        for i in (0..count).rev() {
            result = lisp.cons(elements[i], result)?;
        }
        Ok(result)
    }
    
    /// Parse vector literal #(obj ...)
    /// 
    /// Note: In no_std environments, vector literals are limited to 256 elements
    /// due to stack allocation constraints. Use `make-vector` or `vector` for
    /// larger vectors.
    fn parse_vector_literal<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        // The VectorOpen token has already consumed '#' but NOT '('
        // We need to consume the '(' next
        let open_loc = self.lexer.loc();
        let opening = self.lexer.next_token()
            .ok_or(ParseError { kind: ParseErrorKind::UnexpectedEof, loc: open_loc })??;
        if !matches!(opening.token, Token::LParen) {
            return Err(ParseError { kind: ParseErrorKind::InvalidHashLiteral, loc: opening.loc });
        }
        
        let mut elements: [ArenaIndex; 256] = [ArenaIndex::NIL; 256];
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
                    if count >= 256 {
                        return Err(ParseError { kind: ParseErrorKind::VectorLiteralTooLarge, loc });
                    }
                    elements[count] = self.parse_token(other, loc, lisp)?;
                    count += 1;
                }
            }
        }
        
        // Create the vector
        let placeholder = lisp.number(0)?;
        let vec = lisp.make_array(count, placeholder)?;
        
        // Fill in elements
        for (i, &elem) in elements.iter().enumerate().take(count) {
            lisp.array_set(vec, i, elem)?;
        }
        
        Ok(vec)
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
