//! Lexer/Tokenizer for Lisp expressions
//!
//! This module contains a standalone lexer that produces tokens on demand
//! without allocating. It can be used independently of the parser for
//! syntax highlighting, incremental parsing, or other token-level processing.
//!
//! The lexer yields [`Token`] values via an iterator interface, each with
//! source location information for error reporting.

// ============================================================================
// Character Classification Lookup Tables
// ============================================================================

/// Lookup table for symbol characters. Index by byte value, true if valid symbol char.
/// Valid symbol chars: a-z, A-Z, 0-9, + - * / < > = ? ! _ & % ^ ~ .
pub(crate) static SYMBOL_CHAR_TABLE: [bool; 256] = {
    let mut table = [false; 256];
    
    // Letters a-z
    table[b'a' as usize] = true; table[b'b' as usize] = true; table[b'c' as usize] = true;
    table[b'd' as usize] = true; table[b'e' as usize] = true; table[b'f' as usize] = true;
    table[b'g' as usize] = true; table[b'h' as usize] = true; table[b'i' as usize] = true;
    table[b'j' as usize] = true; table[b'k' as usize] = true; table[b'l' as usize] = true;
    table[b'm' as usize] = true; table[b'n' as usize] = true; table[b'o' as usize] = true;
    table[b'p' as usize] = true; table[b'q' as usize] = true; table[b'r' as usize] = true;
    table[b's' as usize] = true; table[b't' as usize] = true; table[b'u' as usize] = true;
    table[b'v' as usize] = true; table[b'w' as usize] = true; table[b'x' as usize] = true;
    table[b'y' as usize] = true; table[b'z' as usize] = true;
    
    // Letters A-Z
    table[b'A' as usize] = true; table[b'B' as usize] = true; table[b'C' as usize] = true;
    table[b'D' as usize] = true; table[b'E' as usize] = true; table[b'F' as usize] = true;
    table[b'G' as usize] = true; table[b'H' as usize] = true; table[b'I' as usize] = true;
    table[b'J' as usize] = true; table[b'K' as usize] = true; table[b'L' as usize] = true;
    table[b'M' as usize] = true; table[b'N' as usize] = true; table[b'O' as usize] = true;
    table[b'P' as usize] = true; table[b'Q' as usize] = true; table[b'R' as usize] = true;
    table[b'S' as usize] = true; table[b'T' as usize] = true; table[b'U' as usize] = true;
    table[b'V' as usize] = true; table[b'W' as usize] = true; table[b'X' as usize] = true;
    table[b'Y' as usize] = true; table[b'Z' as usize] = true;
    
    // Digits 0-9
    table[b'0' as usize] = true; table[b'1' as usize] = true; table[b'2' as usize] = true;
    table[b'3' as usize] = true; table[b'4' as usize] = true; table[b'5' as usize] = true;
    table[b'6' as usize] = true; table[b'7' as usize] = true; table[b'8' as usize] = true;
    table[b'9' as usize] = true;
    
    // Special symbol characters: + - * / < > = ? ! _ & % ^ ~ .
    table[b'+' as usize] = true; table[b'-' as usize] = true; table[b'*' as usize] = true;
    table[b'/' as usize] = true; table[b'<' as usize] = true; table[b'>' as usize] = true;
    table[b'=' as usize] = true; table[b'?' as usize] = true; table[b'!' as usize] = true;
    table[b'_' as usize] = true; table[b'&' as usize] = true; table[b'%' as usize] = true;
    table[b'^' as usize] = true; table[b'~' as usize] = true; table[b'.' as usize] = true;
    
    table
};

/// Lookup table for whitespace characters.
pub(crate) static WHITESPACE_TABLE: [bool; 256] = {
    let mut table = [false; 256];
    table[b' ' as usize] = true;   // space
    table[b'\t' as usize] = true;  // tab
    table[b'\n' as usize] = true;  // newline
    table[b'\r' as usize] = true;  // carriage return
    table[0x0B] = true;            // vertical tab
    table[0x0C] = true;            // form feed
    table
};

// ============================================================================
// Token Types
// ============================================================================

/// A token produced by the lexer.
///
/// Tokens are lightweight, `Copy`, and do not allocate. Symbol and string
/// content is accessed through the lexer's buffers or the original input
/// after a token is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `'` (quote shorthand)
    Quote,
    /// `` ` `` (quasiquote shorthand)
    Quasiquote,
    /// `,` (unquote shorthand)
    Unquote,
    /// `,@` (unquote-splicing shorthand)
    UnquoteSplice,
    /// `.` (dot for dotted pairs) — only emitted when dot is followed by whitespace or `)`
    Dot,
    /// `#t` or `#T`
    True,
    /// `#f` or `#F`
    False,
    /// `#(` (vector literal open)
    VectorOpen,
    /// `#u8(` (bytevector literal open)
    BytevectorOpen,
    /// `#'` (syntax quote)
    SyntaxQuote,
    /// `#;` (datum comment — parser skips next datum)
    DatumComment,
    /// Integer number literal (value already parsed)
    Number(isize),
    /// Symbol — raw bytes are in `input[start..start+len]` (not yet lowercased).
    /// Use [`Lexer::symbol_bytes`] to get the lowercased bytes.
    Symbol {
        /// Length of the symbol name in bytes
        len: usize,
    },
    /// Character literal (`#\a`, `#\space`, `#\newline`, etc.)
    Char(char),
    /// String literal — processed characters are in the lexer's string buffer.
    /// Use [`Lexer::string_chars`] to access the character data.
    ///
    /// Escape sequences are already processed.
    String {
        /// Number of characters in the string
        len: usize,
    },
}

/// Source location for error reporting
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceLoc {
    pub line: usize,
    pub column: usize,
}

/// A token with its source location
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpannedToken {
    pub token: Token,
    pub loc: SourceLoc,
}

/// Lexer error kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexErrorKind {
    /// Unexpected end of input
    UnexpectedEof,
    /// Unexpected character
    UnexpectedChar(char),
    /// Number too large
    NumberOverflow,
    /// Invalid hash literal
    InvalidHashLiteral,
    /// Invalid character literal
    InvalidCharLiteral,
    /// Invalid string escape sequence
    InvalidEscapeSequence,
    /// Unterminated string literal
    UnterminatedString,
    /// String literal exceeds maximum length
    StringTooLong,
    /// Invalid digit for the given radix (e.g., #b2, #o8)
    InvalidRadixDigit,
}

/// Lexer error with location
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub loc: SourceLoc,
}

// ============================================================================
// Lexer
// ============================================================================

/// Maximum string literal length supported by the lexer
const MAX_STRING_LEN: usize = 1024;

/// Maximum symbol name length supported by the lexer
const MAX_SYMBOL_LEN: usize = 64;

/// A standalone lexer that produces tokens on demand without heap allocation.
///
/// The lexer processes input bytes and yields [`Token`] values one at a time.
/// It handles whitespace/comment skipping, number parsing, string escape
/// sequences, character literals, and symbol case-folding.
///
/// # No-std Compatible
///
/// The lexer uses fixed-size stack buffers for string and symbol content,
/// avoiding any heap allocation. String literals are limited to 1024 characters
/// and symbols to 64 characters.
///
/// # Example
///
/// ```
/// use grift_parser::Lexer;
///
/// let mut lexer = Lexer::new("(+ 1 2)");
/// while let Some(result) = lexer.next_token() {
///     let spanned = result.unwrap();
///     // Process token...
/// }
/// ```
pub struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
    line: usize,
    column: usize,
    /// Whether symbol names are case-folded (lowercased). Controlled by `#!fold-case` / `#!no-fold-case`.
    fold_case: bool,
    /// Internal buffer for lowercased symbol names
    symbol_buf: [u8; MAX_SYMBOL_LEN],
    /// Internal buffer for string literal characters
    string_buf: [char; MAX_STRING_LEN],
}

impl<'a> Lexer<'a> {
    /// Create a new lexer from a string slice
    pub fn new(input: &'a str) -> Self {
        Lexer {
            input: input.as_bytes(),
            pos: 0,
            line: 1,
            column: 1,
            fold_case: true,
            symbol_buf: [0; MAX_SYMBOL_LEN],
            string_buf: ['\0'; MAX_STRING_LEN],
        }
    }
    
    /// Create a new lexer from a byte slice
    pub fn from_bytes(input: &'a [u8]) -> Self {
        Lexer {
            input,
            pos: 0,
            line: 1,
            column: 1,
            fold_case: true,
            symbol_buf: [0; MAX_SYMBOL_LEN],
            string_buf: ['\0'; MAX_STRING_LEN],
        }
    }
    
    /// Get the current source location
    pub fn loc(&self) -> SourceLoc {
        SourceLoc { line: self.line, column: self.column }
    }
    
    /// Get the lowercased symbol bytes from the last `Token::Symbol` produced.
    ///
    /// The `len` field from `Token::Symbol { len }` indicates how many
    /// bytes are valid.
    pub fn symbol_bytes(&self, len: usize) -> &[u8] {
        &self.symbol_buf[..len]
    }
    
    /// Get the string characters from the last `Token::String` produced.
    ///
    /// The `len` field from `Token::String { len }` indicates how many
    /// characters are valid.
    pub fn string_chars(&self, len: usize) -> &[char] {
        &self.string_buf[..len]
    }
    
    /// Get current position in the input
    pub fn position(&self) -> (usize, usize) {
        (self.line, self.column)
    }
    
    /// Check if there's more input (after skipping whitespace/comments)
    pub fn has_more(&mut self) -> bool {
        self.skip_whitespace();
        self.peek().is_some()
    }
    
    /// Produce the next token, or None if input is exhausted.
    ///
    /// Returns `None` when there is no more input (after whitespace).
    /// Returns `Some(Err(...))` for lexer errors.
    /// Returns `Some(Ok(...))` for successfully lexed tokens.
    pub fn next_token(&mut self) -> Option<Result<SpannedToken, LexError>> {
        self.skip_whitespace();
        
        let c = self.peek()?;
        let loc = self.loc();
        
        let result = match c {
            b'(' => { self.advance(); Ok(Token::LParen) }
            b')' => { self.advance(); Ok(Token::RParen) }
            b'\'' => { self.advance(); Ok(Token::Quote) }
            b'`' => { self.advance(); Ok(Token::Quasiquote) }
            b',' => {
                self.advance();
                if self.peek() == Some(b'@') {
                    self.advance();
                    Ok(Token::UnquoteSplice)
                } else {
                    Ok(Token::Unquote)
                }
            }
            b'"' => self.lex_string(),
            b'#' => self.lex_hash(),
            b'0'..=b'9' => self.lex_number(),
            b'-' => {
                if self.peek_next().is_some_and(|c| c.is_ascii_digit()) {
                    self.lex_number()
                } else {
                    self.lex_symbol()
                }
            }
            b'.' => {
                // Could be dot (for dotted pairs) or symbol starting with dot
                let next_pos = self.pos + 1;
                let is_dot = if next_pos < self.input.len() {
                    let next_char = self.input[next_pos];
                    WHITESPACE_TABLE[next_char as usize] || next_char == b')'
                } else {
                    true
                };
                
                if is_dot {
                    self.advance();
                    Ok(Token::Dot)
                } else {
                    self.lex_symbol()
                }
            }
            c if is_symbol_char(c) => self.lex_symbol(),
            c => {
                self.advance();
                Err(LexError { kind: LexErrorKind::UnexpectedChar(c as char), loc })
            }
        };
        
        Some(match result {
            Ok(token) => Ok(SpannedToken { token, loc }),
            Err(e) => Err(e),
        })
    }
    
    // ========================================================================
    // Internal helpers
    // ========================================================================
    
    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }
    
    fn peek_next(&self) -> Option<u8> {
        self.input.get(self.pos + 1).copied()
    }
    
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
    
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if WHITESPACE_TABLE[c as usize] {
                self.advance();
            } else if c == b';' {
                while let Some(c) = self.advance() {
                    if c == b'\n' {
                        break;
                    }
                }
            } else if c == b'#' && self.peek_next() == Some(b'|') {
                // Block comment #| ... |# (R7RS, nestable)
                self.advance(); // consume '#'
                self.advance(); // consume '|'
                let mut nesting_depth = 1u32;
                while nesting_depth > 0 {
                    match self.advance() {
                        None => break,
                        Some(b'#') if self.peek() == Some(b'|') => {
                            self.advance();
                            nesting_depth += 1;
                        }
                        Some(b'|') if self.peek() == Some(b'#') => {
                            self.advance();
                            nesting_depth -= 1;
                        }
                        _ => {}
                    }
                }
            } else if c == b'#' && self.peek_next() == Some(b'!') {
                // #!fold-case or #!no-fold-case directive (R7RS §2.1)
                if self.try_skip_fold_case_directive() {
                    // Directive consumed, continue skipping whitespace
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
    
    /// Try to consume a `#!fold-case` or `#!no-fold-case` directive.
    /// Returns true if a directive was consumed, false otherwise (position unchanged).
    fn try_skip_fold_case_directive(&mut self) -> bool {
        let save_pos = self.pos;
        let save_line = self.line;
        let save_col = self.column;
        
        self.advance(); // consume '#'
        self.advance(); // consume '!'
        
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b'-' || c.is_ascii_alphabetic() {
                self.advance();
            } else {
                break;
            }
        }
        let directive = &self.input[start..self.pos];
        
        if directive == b"fold-case" {
            self.fold_case = true;
            return true;
        }
        if directive == b"no-fold-case" {
            self.fold_case = false;
            return true;
        }
        
        // Not a recognized directive — restore position
        self.pos = save_pos;
        self.line = save_line;
        self.column = save_col;
        false
    }
    
    fn error(&self, kind: LexErrorKind) -> LexError {
        LexError { kind, loc: self.loc() }
    }
    
    // ========================================================================
    // Token-specific lexing
    // ========================================================================
    
    fn lex_number(&mut self) -> Result<Token, LexError> {
        let negative = if self.peek() == Some(b'-') {
            self.advance();
            true
        } else {
            false
        };
        
        let mut value: isize = 0;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
                value = value.checked_mul(10)
                    .and_then(|v| v.checked_add((c - b'0') as isize))
                    .ok_or_else(|| self.error(LexErrorKind::NumberOverflow))?;
            } else {
                break;
            }
        }
        
        if negative { value = -value; }
        Ok(Token::Number(value))
    }
    
    fn lex_symbol(&mut self) -> Result<Token, LexError> {
        let mut len = 0;
        
        while let Some(c) = self.peek() {
            if is_symbol_char(c) && len < MAX_SYMBOL_LEN {
                self.symbol_buf[len] = if self.fold_case { c.to_ascii_lowercase() } else { c };
                len += 1;
                self.advance();
            } else {
                break;
            }
        }
        
        Ok(Token::Symbol { len })
    }
    
    fn lex_hash(&mut self) -> Result<Token, LexError> {
        self.advance(); // consume '#'
        
        match self.peek() {
            Some(b't') | Some(b'T') => { self.advance(); Ok(Token::True) }
            Some(b'f') | Some(b'F') => { self.advance(); Ok(Token::False) }
            Some(b'\\') => self.lex_char_literal(),
            Some(b'(') => { Ok(Token::VectorOpen) } // Don't consume '(' - parser handles it
            Some(b'\'') => { self.advance(); Ok(Token::SyntaxQuote) }
            Some(b';') => { self.advance(); Ok(Token::DatumComment) }
            Some(b'b') | Some(b'B') | Some(b'o') | Some(b'O') | Some(b'd') | Some(b'D') | Some(b'x') | Some(b'X')
            | Some(b'e') | Some(b'E') | Some(b'i') | Some(b'I') => {
                self.lex_prefixed_number()
            }
            Some(b'u') => {
                // #u8( bytevector literal
                let save_pos = self.pos;
                let save_line = self.line;
                let save_col = self.column;
                self.advance(); // consume 'u'
                if self.peek() == Some(b'8') {
                    self.advance(); // consume '8'
                    if self.peek() == Some(b'(') {
                        Ok(Token::BytevectorOpen)
                    } else {
                        self.pos = save_pos;
                        self.line = save_line;
                        self.column = save_col;
                        Err(self.error(LexErrorKind::InvalidHashLiteral))
                    }
                } else {
                    self.pos = save_pos;
                    self.line = save_line;
                    self.column = save_col;
                    Err(self.error(LexErrorKind::InvalidHashLiteral))
                }
            }
            Some(_) => Err(self.error(LexErrorKind::InvalidHashLiteral)),
            None => Err(self.error(LexErrorKind::UnexpectedEof)),
        }
    }
    
    /// Parse a number with radix and/or exactness prefix.
    /// Called after '#' has been consumed and peek() is one of b/o/d/x/e/i.
    fn lex_prefixed_number(&mut self) -> Result<Token, LexError> {
        let mut radix: u8 = 0; // 0 = not specified
        let mut has_exactness = false;
        
        // Parse first prefix character (already peeked)
        match self.peek() {
            Some(b'b') | Some(b'B') => { self.advance(); radix = 2; }
            Some(b'o') | Some(b'O') => { self.advance(); radix = 8; }
            Some(b'd') | Some(b'D') => { self.advance(); radix = 10; }
            Some(b'x') | Some(b'X') => { self.advance(); radix = 16; }
            Some(b'e') | Some(b'E') => { self.advance(); has_exactness = true; }
            Some(b'i') | Some(b'I') => { self.advance(); has_exactness = true; }
            _ => return Err(self.error(LexErrorKind::InvalidHashLiteral)),
        }
        
        // Check for second prefix (#e#x, #x#e, etc.)
        if self.peek() == Some(b'#') {
            self.advance(); // consume '#'
            match self.peek() {
                Some(b'b') | Some(b'B') if radix == 0 => { self.advance(); radix = 2; }
                Some(b'o') | Some(b'O') if radix == 0 => { self.advance(); radix = 8; }
                Some(b'd') | Some(b'D') if radix == 0 => { self.advance(); radix = 10; }
                Some(b'x') | Some(b'X') if radix == 0 => { self.advance(); radix = 16; }
                Some(b'e') | Some(b'E') if !has_exactness && radix != 0 => { self.advance(); }
                Some(b'i') | Some(b'I') if !has_exactness && radix != 0 => { self.advance(); }
                _ => return Err(self.error(LexErrorKind::InvalidHashLiteral)),
            }
        }
        
        // Default radix is 10
        if radix == 0 { radix = 10; }
        
        // Parse optional negative sign
        let negative = if self.peek() == Some(b'-') {
            self.advance();
            true
        } else {
            false
        };
        
        // Parse digits in the given radix
        let mut value: isize = 0;
        let mut has_digits = false;
        while let Some(c) = self.peek() {
            let digit = match c {
                b'0'..=b'9' => (c - b'0') as isize,
                b'a'..=b'f' if radix == 16 => (c - b'a' + 10) as isize,
                b'A'..=b'F' if radix == 16 => (c - b'A' + 10) as isize,
                _ => break,
            };
            if digit >= radix as isize {
                return Err(self.error(LexErrorKind::InvalidRadixDigit));
            }
            self.advance();
            has_digits = true;
            value = value.checked_mul(radix as isize)
                .and_then(|v| v.checked_add(digit))
                .ok_or_else(|| self.error(LexErrorKind::NumberOverflow))?;
        }
        
        if !has_digits {
            return Err(self.error(LexErrorKind::InvalidHashLiteral));
        }
        
        if negative { value = -value; }
        
        // Note: #e and #i are no-ops for integers (our only number type)
        Ok(Token::Number(value))
    }
    
    fn lex_char_literal(&mut self) -> Result<Token, LexError> {
        self.advance(); // consume '\'
        
        let start = self.pos;
        
        // Read characters that could form a name
        while let Some(c) = self.peek() {
            if is_symbol_char(c) {
                self.advance();
            } else {
                break;
            }
        }
        
        let name_len = self.pos - start;
        
        if name_len == 0 {
            // #\ followed by non-symbol character
            return match self.peek() {
                Some(c) => {
                    self.advance();
                    Ok(Token::Char(c as char))
                }
                None => Err(self.error(LexErrorKind::UnexpectedEof)),
            };
        }
        
        let name = &self.input[start..self.pos];
        
        // Single character
        if name_len == 1 {
            return Ok(Token::Char(name[0] as char));
        }
        
        // Named characters (R7RS Section 6.6)
        match name {
            b"alarm" => Ok(Token::Char('\x07')),
            b"backspace" => Ok(Token::Char('\x08')),
            b"delete" => Ok(Token::Char('\x7F')),
            b"escape" => Ok(Token::Char('\x1B')),
            b"newline" => Ok(Token::Char('\n')),
            b"null" => Ok(Token::Char('\0')),
            b"return" => Ok(Token::Char('\r')),
            b"space" => Ok(Token::Char(' ')),
            b"tab" => Ok(Token::Char('\t')),
            _ => {
                // Hex character #\xNN...
                if name.len() >= 2 && (name[0] == b'x' || name[0] == b'X') {
                    let hex_str = &name[1..];
                    if let Some(code) = parse_hex(hex_str) {
                        if let Some(c) = char::from_u32(code) {
                            return Ok(Token::Char(c));
                        }
                    }
                }
                Err(self.error(LexErrorKind::InvalidCharLiteral))
            }
        }
    }
    
    fn lex_string(&mut self) -> Result<Token, LexError> {
        self.advance(); // consume opening '"'
        let mut len = 0;
        
        loop {
            match self.peek() {
                None => return Err(self.error(LexErrorKind::UnterminatedString)),
                Some(b'"') => {
                    self.advance();
                    break;
                }
                Some(b'\\') => {
                    self.advance(); // consume '\'
                    let c = match self.peek() {
                        None => return Err(self.error(LexErrorKind::UnterminatedString)),
                        Some(b'a') => { self.advance(); '\x07' }
                        Some(b'b') => { self.advance(); '\x08' }
                        Some(b't') => { self.advance(); '\t' }
                        Some(b'n') => { self.advance(); '\n' }
                        Some(b'r') => { self.advance(); '\r' }
                        Some(b'"') => { self.advance(); '"' }
                        Some(b'\\') => { self.advance(); '\\' }
                        Some(b'|') => { self.advance(); '|' }
                        Some(b'x') => {
                            self.advance(); // consume 'x'
                            let hex_start = self.pos;
                            while let Some(c) = self.peek() {
                                if c == b';' { break; }
                                if c.is_ascii_hexdigit() {
                                    self.advance();
                                } else {
                                    return Err(self.error(LexErrorKind::InvalidEscapeSequence));
                                }
                            }
                            let hex_bytes = &self.input[hex_start..self.pos];
                            if self.peek() != Some(b';') {
                                return Err(self.error(LexErrorKind::InvalidEscapeSequence));
                            }
                            self.advance(); // consume ';'
                            match parse_hex(hex_bytes) {
                                Some(code) => match char::from_u32(code) {
                                    Some(ch) => ch,
                                    None => return Err(self.error(LexErrorKind::InvalidEscapeSequence)),
                                },
                                None => return Err(self.error(LexErrorKind::InvalidEscapeSequence)),
                            }
                        }
                        Some(b'\n') | Some(b'\r') => {
                            self.advance();
                            if self.peek() == Some(b'\n') { self.advance(); }
                            while let Some(c) = self.peek() {
                                if c == b' ' || c == b'\t' { self.advance(); } else { break; }
                            }
                            continue;
                        }
                        Some(c) if c == b' ' || c == b'\t' => {
                            while let Some(c) = self.peek() {
                                if c == b' ' || c == b'\t' {
                                    self.advance();
                                } else if c == b'\n' || c == b'\r' {
                                    self.advance();
                                    if c == b'\r' && self.peek() == Some(b'\n') { self.advance(); }
                                    while let Some(c) = self.peek() {
                                        if c == b' ' || c == b'\t' { self.advance(); } else { break; }
                                    }
                                    break;
                                } else {
                                    return Err(self.error(LexErrorKind::InvalidEscapeSequence));
                                }
                            }
                            continue;
                        }
                        Some(_) => return Err(self.error(LexErrorKind::InvalidEscapeSequence)),
                    };
                    if len >= MAX_STRING_LEN {
                        return Err(self.error(LexErrorKind::StringTooLong));
                    }
                    self.string_buf[len] = c;
                    len += 1;
                }
                Some(c) => {
                    if len >= MAX_STRING_LEN {
                        return Err(self.error(LexErrorKind::StringTooLong));
                    }
                    self.string_buf[len] = c as char;
                    len += 1;
                    self.advance();
                }
            }
        }
        
        Ok(Token::String { len })
    }
}

// ============================================================================
// Free Functions
// ============================================================================

/// Check if a byte is a valid symbol character.
/// Uses a 256-byte lookup table for O(1) classification.
#[inline]
pub(crate) fn is_symbol_char(c: u8) -> bool {
    SYMBOL_CHAR_TABLE[c as usize]
}

/// Parse hex digits into a u32 value
pub(crate) fn parse_hex(bytes: &[u8]) -> Option<u32> {
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
