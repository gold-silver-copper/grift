//! Parser types and functions for parsing Lisp expressions
//!
//! This module contains the parser state machine and related error types.

use pwn_arena::{ArenaIndex, ArenaError};
use crate::Lisp;

// ============================================================================
// Parser
// ============================================================================

/// Source location for error reporting
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceLoc {
    pub line: usize,
    pub column: usize,
}

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

/// Parser state
pub struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    line: usize,
    column: usize,
}

impl<'a> Parser<'a> {
    /// Create a new parser
    pub fn new(input: &'a str) -> Self {
        Parser {
            input: input.as_bytes(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }
    
    /// Create from bytes
    pub fn from_bytes(input: &'a [u8]) -> Self {
        Parser { input, pos: 0, line: 1, column: 1 }
    }
    
    /// Get current source location
    fn loc(&self) -> SourceLoc {
        SourceLoc { line: self.line, column: self.column }
    }
    
    /// Create an error at current location
    fn error(&self, kind: ParseErrorKind) -> ParseError {
        ParseError { kind, loc: self.loc() }
    }
    
    /// Peek at the current character
    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }
    
    /// Peek at the next character (lookahead)
    fn peek_next(&self) -> Option<u8> {
        self.input.get(self.pos + 1).copied()
    }
    
    /// Advance and return the current character
    fn advance(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }
    
    /// Skip whitespace and comments
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.advance();
            } else if c == b';' {
                // Comment - skip to end of line
                while let Some(c) = self.advance() {
                    if c == b'\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }
    
    /// Check if a character can be part of a symbol
    fn is_symbol_char(c: u8) -> bool {
        matches!(c, 
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' |
            b'+' | b'-' | b'*' | b'/' | b'<' | b'>' | b'=' |
            b'?' | b'!' | b'_' | b'&' | b'%' | b'^' | b'~' | b'.'
        )
    }
    
    /// Parse a single expression
    pub fn parse<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        self.skip_whitespace();
        
        match self.peek() {
            None => Err(self.error(ParseErrorKind::UnexpectedEof)),
            
            Some(b'(') => self.parse_list(lisp),
            
            Some(b')') => Err(self.error(ParseErrorKind::UnmatchedParen)),
            
            Some(b'"') => self.parse_string(lisp),
            
            Some(b'\'') => {
                // Quote: 'x -> (quote x)
                self.advance();
                let expr = self.parse(lisp)?;
                let quote_sym = lisp.symbol("quote")?;
                let nil = lisp.nil()?;
                let quoted = lisp.cons(expr, nil)?;
                lisp.cons(quote_sym, quoted).map_err(Into::into)
            }
            
            Some(b'#') => self.parse_hash_literal(lisp),
            
            Some(c) if c.is_ascii_digit() => self.parse_number(lisp),
            
            Some(b'-') => {
                // Could be negative number or symbol
                if self.peek_next().map_or(false, |c| c.is_ascii_digit()) {
                    self.parse_number(lisp)
                } else {
                    self.parse_symbol(lisp)
                }
            }
            
            Some(c) if Self::is_symbol_char(c) => self.parse_symbol(lisp),
            
            Some(c) => Err(self.error(ParseErrorKind::UnexpectedChar(c as char))),
        }
    }
    
    /// Parse hash literals (#t, #f, #\char, #(vector), etc.)
    fn parse_hash_literal<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        self.advance(); // consume '#'
        
        match self.peek() {
            Some(b't') | Some(b'T') => {
                self.advance();
                lisp.true_val().map_err(Into::into)
            }
            Some(b'f') | Some(b'F') => {
                self.advance();
                lisp.false_val().map_err(Into::into)
            }
            Some(b'\\') => self.parse_char_literal(lisp),
            Some(b'(') => self.parse_vector_literal(lisp),
            Some(_) => Err(self.error(ParseErrorKind::InvalidHashLiteral)),
            None => Err(self.error(ParseErrorKind::UnexpectedEof)),
        }
    }
    
    /// Parse vector literal #(obj ...)
    /// 
    /// Note: In no_std environments, vector literals are limited to 256 elements
    /// due to stack allocation constraints. Use `make-vector` or `vector` for
    /// larger vectors.
    fn parse_vector_literal<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        self.advance(); // consume '('
        
        // Parse elements into a stack-allocated array (no_std constraint)
        // Maximum 256 elements for literals; use make-vector for larger vectors
        let mut elements: [ArenaIndex; 256] = [ArenaIndex::NULL; 256];
        let mut count = 0usize;
        
        self.skip_whitespace();
        while let Some(c) = self.peek() {
            if c == b')' {
                break;
            }
            
            if count >= 256 {
                return Err(self.error(ParseErrorKind::VectorLiteralTooLarge));
            }
            
            let elem = self.parse(lisp)?;
            elements[count] = elem;
            count += 1;
            
            self.skip_whitespace();
        }
        
        // Consume closing paren
        match self.advance() {
            Some(b')') => {}
            _ => return Err(self.error(ParseErrorKind::UnmatchedParen)),
        }
        
        // Create the vector
        let placeholder = lisp.number(0)?;
        let vec = lisp.make_array(count, placeholder)?;
        
        // Fill in elements
        for i in 0..count {
            lisp.array_set(vec, i, elements[i])?;
        }
        
        Ok(vec)
    }
    
    /// Parse character literal (#\a, #\space, #\newline, etc.)
    fn parse_char_literal<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        self.advance(); // consume '\'
        
        // First check for named characters
        // We need to look ahead to see if there's a multi-char name
        let start = self.pos;
        
        // Read characters that could form a name
        while let Some(c) = self.peek() {
            if Self::is_symbol_char(c) {
                self.advance();
            } else {
                break;
            }
        }
        
        let name_len = self.pos - start;
        
        if name_len == 0 {
            // #\ followed by non-symbol character like space: #\ 
            return match self.peek() {
                Some(c) => {
                    self.advance();
                    lisp.char(c as char).map_err(Into::into)
                }
                None => Err(self.error(ParseErrorKind::UnexpectedEof)),
            };
        }
        
        // Get the name as a slice
        let name = &self.input[start..self.pos];
        
        // If it's a single character, return it directly
        if name_len == 1 {
            return lisp.char(name[0] as char).map_err(Into::into);
        }
        
        // Check for named characters (R7RS Section 6.6)
        match name {
            b"alarm" => lisp.char('\x07').map_err(Into::into),
            b"backspace" => lisp.char('\x08').map_err(Into::into),
            b"delete" => lisp.char('\x7F').map_err(Into::into),
            b"escape" => lisp.char('\x1B').map_err(Into::into),
            b"newline" => lisp.char('\n').map_err(Into::into),
            b"null" => lisp.char('\0').map_err(Into::into),
            b"return" => lisp.char('\r').map_err(Into::into),
            b"space" => lisp.char(' ').map_err(Into::into),
            b"tab" => lisp.char('\t').map_err(Into::into),
            _ => {
                // Check for hex character #\xNN...
                if name.len() >= 2 && (name[0] == b'x' || name[0] == b'X') {
                    let hex_str = &name[1..];
                    if let Some(code) = Self::parse_hex(hex_str) {
                        if let Some(c) = char::from_u32(code) {
                            return lisp.char(c).map_err(Into::into);
                        }
                    }
                }
                Err(self.error(ParseErrorKind::InvalidCharLiteral))
            }
        }
    }
    
    /// Parse hex digits into a u32 value
    fn parse_hex(bytes: &[u8]) -> Option<u32> {
        if bytes.is_empty() {
            return None;
        }
        let mut result: u32 = 0;
        for &b in bytes {
            let digit = match b {
                b'0'..=b'9' => (b - b'0') as u32,
                b'a'..=b'f' => (b - b'a' + 10) as u32,
                b'A'..=b'F' => (b - b'A' + 10) as u32,
                _ => return None,
            };
            result = result.checked_mul(16)?.checked_add(digit)?;
        }
        Some(result)
    }
    
    /// Parse a string literal ("..." with escape sequences)
    fn parse_string<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        self.advance(); // consume opening '"'
        
        // Collect characters into a fixed-size buffer
        const MAX_STRING_LEN: usize = 1024;
        let mut chars: [char; MAX_STRING_LEN] = ['\0'; MAX_STRING_LEN];
        let mut len = 0;
        
        loop {
            match self.peek() {
                None => return Err(self.error(ParseErrorKind::UnterminatedString)),
                Some(b'"') => {
                    self.advance(); // consume closing '"'
                    break;
                }
                Some(b'\\') => {
                    // Escape sequence
                    self.advance(); // consume '\'
                    let c = match self.peek() {
                        None => return Err(self.error(ParseErrorKind::UnterminatedString)),
                        Some(b'a') => { self.advance(); '\x07' } // alarm
                        Some(b'b') => { self.advance(); '\x08' } // backspace
                        Some(b't') => { self.advance(); '\t' }   // tab
                        Some(b'n') => { self.advance(); '\n' }   // newline
                        Some(b'r') => { self.advance(); '\r' }   // return
                        Some(b'"') => { self.advance(); '"' }    // double quote
                        Some(b'\\') => { self.advance(); '\\' }  // backslash
                        Some(b'|') => { self.advance(); '|' }    // vertical line
                        Some(b'x') => {
                            // Hex escape: \xNN;
                            self.advance(); // consume 'x'
                            let hex_start = self.pos;
                            while let Some(c) = self.peek() {
                                if c == b';' {
                                    break;
                                }
                                if c.is_ascii_hexdigit() {
                                    self.advance();
                                } else {
                                    return Err(self.error(ParseErrorKind::InvalidEscapeSequence));
                                }
                            }
                            let hex_bytes = &self.input[hex_start..self.pos];
                            if self.peek() != Some(b';') {
                                return Err(self.error(ParseErrorKind::InvalidEscapeSequence));
                            }
                            self.advance(); // consume ';'
                            match Self::parse_hex(hex_bytes) {
                                Some(code) => {
                                    match char::from_u32(code) {
                                        Some(ch) => ch,
                                        None => return Err(self.error(ParseErrorKind::InvalidEscapeSequence)),
                                    }
                                }
                                None => return Err(self.error(ParseErrorKind::InvalidEscapeSequence)),
                            }
                        }
                        Some(b'\n') | Some(b'\r') => {
                            // Line continuation: skip the line ending and any intraline whitespace on next line
                            // Per R7RS, skip only the first line ending, then intraline whitespace
                            self.advance(); // consume the \n or \r
                            // Handle \r\n as a single line ending
                            if self.peek() == Some(b'\n') {
                                self.advance();
                            }
                            // Skip intraline whitespace on next line (spaces and tabs only, not newlines)
                            while let Some(c) = self.peek() {
                                if c == b' ' || c == b'\t' {
                                    self.advance();
                                } else {
                                    break;
                                }
                            }
                            continue; // Don't add any character
                        }
                        Some(c) if c == b' ' || c == b'\t' => {
                            // \<intraline whitespace>*<line ending> - skip whitespace until line ending
                            while let Some(c) = self.peek() {
                                if c == b' ' || c == b'\t' {
                                    self.advance();
                                } else if c == b'\n' || c == b'\r' {
                                    self.advance();
                                    // Handle \r\n as a single line ending
                                    if c == b'\r' && self.peek() == Some(b'\n') {
                                        self.advance();
                                    }
                                    // Skip trailing whitespace on next line (intraline only)
                                    while let Some(c) = self.peek() {
                                        if c == b' ' || c == b'\t' {
                                            self.advance();
                                        } else {
                                            break;
                                        }
                                    }
                                    break;
                                } else {
                                    return Err(self.error(ParseErrorKind::InvalidEscapeSequence));
                                }
                            }
                            continue; // Don't add any character
                        }
                        Some(_) => return Err(self.error(ParseErrorKind::InvalidEscapeSequence)),
                    };
                    if len >= MAX_STRING_LEN {
                        return Err(self.error(ParseErrorKind::OutOfMemory));
                    }
                    chars[len] = c;
                    len += 1;
                }
                Some(c) => {
                    if len >= MAX_STRING_LEN {
                        return Err(self.error(ParseErrorKind::OutOfMemory));
                    }
                    chars[len] = c as char;
                    len += 1;
                    self.advance();
                }
            }
        }
        
        // Allocate the string in the arena
        lisp.string_from_chars(&chars[..len]).map_err(Into::into)
    }
    
    /// Parse a list (including nil)
    fn parse_list<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        self.advance(); // consume '('
        self.skip_whitespace();
        
        if self.peek() == Some(b')') {
            self.advance();
            return lisp.nil().map_err(Into::into);
        }
        
        // Parse elements and build list
        const MAX_LIST_DEPTH: usize = 128;
        let mut elements: [ArenaIndex; MAX_LIST_DEPTH] = [ArenaIndex::NULL; MAX_LIST_DEPTH];
        let mut count = 0;
        
        loop {
            self.skip_whitespace();
            
            match self.peek() {
                None => return Err(self.error(ParseErrorKind::UnexpectedEof)),
                Some(b')') => {
                    self.advance();
                    break;
                }
                Some(b'.') => {
                    // Dotted pair: (a . b)
                    self.advance();
                    self.skip_whitespace();
                    
                    if count == 0 {
                        return Err(self.error(ParseErrorKind::UnexpectedChar('.')));
                    }
                    
                    let cdr = self.parse(lisp)?;
                    self.skip_whitespace();
                    
                    if self.advance() != Some(b')') {
                        return Err(self.error(ParseErrorKind::UnmatchedParen));
                    }
                    
                    // Build the dotted list
                    let mut result = cdr;
                    for i in (0..count).rev() {
                        result = lisp.cons(elements[i], result)?;
                    }
                    return Ok(result);
                }
                Some(_) => {
                    if count >= MAX_LIST_DEPTH {
                        return Err(self.error(ParseErrorKind::OutOfMemory));
                    }
                    elements[count] = self.parse(lisp)?;
                    count += 1;
                }
            }
        }
        
        // Build proper list
        let mut result = lisp.nil()?;
        for i in (0..count).rev() {
            result = lisp.cons(elements[i], result)?;
        }
        Ok(result)
    }
    
    /// Parse an integer number
    fn parse_number<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        let negative = if self.peek() == Some(b'-') {
            self.advance();
            true
        } else {
            false
        };
        
        // Parse integer
        let mut int_value: isize = 0;
        
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
                int_value = int_value.checked_mul(10)
                    .and_then(|v| v.checked_add((c - b'0') as isize))
                    .ok_or_else(|| self.error(ParseErrorKind::NumberOverflow))?;
            } else {
                break;
            }
        }
        
        if negative {
            int_value = -int_value;
        }
        lisp.number(int_value).map_err(Into::into)
    }
    
    /// Parse a symbol
    fn parse_symbol<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        const MAX_SYMBOL_LEN: usize = 64;
        let mut buffer: [u8; MAX_SYMBOL_LEN] = [0; MAX_SYMBOL_LEN];
        let mut len = 0;
        
        while let Some(c) = self.peek() {
            if Self::is_symbol_char(c) && len < MAX_SYMBOL_LEN {
                buffer[len] = c.to_ascii_lowercase();
                len += 1;
                self.advance();
            } else {
                break;
            }
        }
        
        let name = &buffer[..len];
        
        // Note: In Scheme, nil is just a regular symbol.
        // The empty list is written as () or '() only.
        // No special handling for 'nil' - it's parsed as a regular symbol.
        
        lisp.symbol_from_bytes(name).map_err(Into::into)
    }
    
    /// Check if there's more input (after whitespace)
    pub fn has_more(&mut self) -> bool {
        self.skip_whitespace();
        self.peek().is_some()
    }
    
    /// Get current position for error reporting
    pub fn position(&self) -> (usize, usize) {
        (self.line, self.column)
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
    let mut results = [ArenaIndex::NULL; 64];
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
