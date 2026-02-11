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
    let chars = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-*/<>=?!_&%^~.";
    let mut i = 0;
    while i < chars.len() {
        table[chars[i] as usize] = true;
        i += 1;
    }
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
///
/// Note: `Eq` is intentionally not derived because `Token::Float` contains
/// an `fsize` (floating-point) value, and NaN != NaN breaks `Eq` semantics.
#[derive(Clone, Copy, Debug, PartialEq)]
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
    /// Floating-point number literal (value already parsed)
    Float(grift_core::fsize),
    /// Rational number literal (numerator/denominator, already parsed)
    Rational(isize, isize),
    /// Complex number literal (real + imaginary parts, already parsed)
    Complex(grift_core::fsize, grift_core::fsize),
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
#[derive(Clone, Copy, Debug, PartialEq)]
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
            b'-' | b'+' => {
                if self.peek_next().is_some_and(|c| c.is_ascii_digit()) {
                    if c == b'+' { self.advance(); } // consume '+' prefix
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
        
        // Check for decimal point or exponent → floating-point literal
        let has_dot = self.peek() == Some(b'.') 
            && self.peek_next().is_some_and(|c| c.is_ascii_digit() || c == b'e' || c == b'E');
        let has_exp = self.peek() == Some(b'e') || self.peek() == Some(b'E');
        
        if has_dot || has_exp {
            let float_tok = self.lex_float_tail(value, negative)?;
            // After parsing float, check for complex suffix
            if let Token::Float(real) = float_tok {
                return self.try_lex_complex_suffix(real);
            }
            return Ok(float_tok);
        }

        // Check for rational literal: numerator/denominator
        if self.peek() == Some(b'/') && self.peek_next().is_some_and(|c| c.is_ascii_digit()) {
            self.advance(); // consume '/'
            let mut denom: isize = 0;
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                    denom = denom.checked_mul(10)
                        .and_then(|v| v.checked_add((c - b'0') as isize))
                        .ok_or_else(|| self.error(LexErrorKind::NumberOverflow))?;
                } else {
                    break;
                }
            }
            if denom == 0 {
                return Err(self.error(LexErrorKind::NumberOverflow));
            }
            let num = if negative { -value } else { value };
            return Ok(Token::Rational(num, denom));
        }
        
        if negative { value = -value; }

        // Check for complex suffix: +/-imag i or @angle
        self.try_lex_complex_suffix_int(value)
    }
    
    /// Continue lexing a floating-point literal after the integer part.
    /// `int_part` is the integer part parsed so far (always non-negative).
    fn lex_float_tail(&mut self, int_part: isize, negative: bool) -> Result<Token, LexError> {
        // Build the float from the integer part
        let mut result: grift_core::fsize = int_part as grift_core::fsize;
        
        // Parse fractional part
        if self.peek() == Some(b'.') {
            self.advance(); // consume '.'
            let mut frac_scale: grift_core::fsize = 0.1;
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                    result += (c - b'0') as grift_core::fsize * frac_scale;
                    frac_scale *= 0.1;
                } else {
                    break;
                }
            }
        }
        
        // Parse exponent part
        if self.peek() == Some(b'e') || self.peek() == Some(b'E') {
            self.advance(); // consume 'e'/'E'
            let exp_negative = match self.peek() {
                Some(b'+') => { self.advance(); false }
                Some(b'-') => { self.advance(); true }
                _ => false,
            };
            let mut exp: i32 = 0;
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                    exp = exp.saturating_mul(10).saturating_add((c - b'0') as i32);
                } else {
                    break;
                }
            }
            if exp_negative { exp = -exp; }
            // Compute 10^exp using repeated multiplication (no_std)
            result = mul_pow10(result, exp);
        }
        
        if negative { result = -result; }
        Ok(Token::Float(result))
    }

    /// After parsing a float `real`, check for complex suffix: +imag i, -imag i, or @angle
    fn try_lex_complex_suffix(&mut self, real: grift_core::fsize) -> Result<Token, LexError> {
        match self.peek() {
            Some(b'+') | Some(b'-') => {
                let neg = self.peek() == Some(b'-');
                self.advance();
                // Check for bare 'i' (meaning +1i or -1i)
                if self.peek() == Some(b'i') && !self.peek_next().is_some_and(|c| c.is_ascii_alphanumeric()) {
                    self.advance();
                    let imag = if neg { -1.0 } else { 1.0 };
                    return Ok(Token::Complex(real, imag));
                }
                // Parse imaginary part number
                let mut imag_val: grift_core::fsize = 0.0;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        self.advance();
                        imag_val = imag_val * 10.0 + (c - b'0') as grift_core::fsize;
                    } else {
                        break;
                    }
                }
                // Check for decimal part
                if self.peek() == Some(b'.') {
                    self.advance();
                    let mut frac_scale: grift_core::fsize = 0.1;
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() {
                            self.advance();
                            imag_val += (c - b'0') as grift_core::fsize * frac_scale;
                            frac_scale *= 0.1;
                        } else {
                            break;
                        }
                    }
                }
                if neg { imag_val = -imag_val; }
                // Must end with 'i'
                if self.peek() == Some(b'i') {
                    self.advance();
                    Ok(Token::Complex(real, imag_val))
                } else {
                    // Not a complex literal, return just the real part
                    // (the +/- was consumed; this is an error in practice)
                    Err(self.error(LexErrorKind::NumberOverflow))
                }
            }
            Some(b'@') => {
                // Polar form: magnitude@angle
                self.advance();
                let angle_neg = if self.peek() == Some(b'-') {
                    self.advance();
                    true
                } else {
                    if self.peek() == Some(b'+') { self.advance(); }
                    false
                };
                let mut angle: grift_core::fsize = 0.0;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        self.advance();
                        angle = angle * 10.0 + (c - b'0') as grift_core::fsize;
                    } else {
                        break;
                    }
                }
                if self.peek() == Some(b'.') {
                    self.advance();
                    let mut frac_scale: grift_core::fsize = 0.1;
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() {
                            self.advance();
                            angle += (c - b'0') as grift_core::fsize * frac_scale;
                            frac_scale *= 0.1;
                        } else {
                            break;
                        }
                    }
                }
                if angle_neg { angle = -angle; }
                // Convert polar to rectangular
                let r = real;
                let real_part = r * libm_cos(angle);
                let imag_part = r * libm_sin(angle);
                Ok(Token::Complex(real_part, imag_part))
            }
            Some(b'i') if !self.peek_next().is_some_and(|c| c.is_ascii_alphanumeric()) => {
                // Bare number followed by 'i' means pure imaginary
                self.advance();
                Ok(Token::Complex(0.0, real))
            }
            _ => Ok(Token::Float(real)),
        }
    }

    /// After parsing an integer `value`, check for complex suffix
    fn try_lex_complex_suffix_int(&mut self, value: isize) -> Result<Token, LexError> {
        match self.peek() {
            Some(b'+') | Some(b'-') => {
                // Could be complex: integer+imag i
                let saved_pos = self.pos;
                let neg = self.peek() == Some(b'-');
                self.advance();
                // Check for bare 'i'
                if self.peek() == Some(b'i') && !self.peek_next().is_some_and(|c| c.is_ascii_alphanumeric()) {
                    self.advance();
                    let imag = if neg { -1.0 } else { 1.0 };
                    return Ok(Token::Complex(value as grift_core::fsize, imag));
                }
                // Check if followed by digits (for imaginary part)
                if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    let mut imag_val: grift_core::fsize = 0.0;
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() {
                            self.advance();
                            imag_val = imag_val * 10.0 + (c - b'0') as grift_core::fsize;
                        } else {
                            break;
                        }
                    }
                    // Check for decimal part
                    if self.peek() == Some(b'.') {
                        self.advance();
                        let mut frac_scale: grift_core::fsize = 0.1;
                        while let Some(c) = self.peek() {
                            if c.is_ascii_digit() {
                                self.advance();
                                imag_val += (c - b'0') as grift_core::fsize * frac_scale;
                                frac_scale *= 0.1;
                            } else {
                                break;
                            }
                        }
                    }
                    if neg { imag_val = -imag_val; }
                    if self.peek() == Some(b'i') {
                        self.advance();
                        return Ok(Token::Complex(value as grift_core::fsize, imag_val));
                    }
                }
                // Not a complex literal - restore position
                self.pos = saved_pos;
                Ok(Token::Number(value))
            }
            Some(b'@') => {
                // Polar form
                self.try_lex_complex_suffix(value as grift_core::fsize)
            }
            Some(b'i') if !self.peek_next().is_some_and(|c| c.is_ascii_alphanumeric()) => {
                // Pure imaginary: 5i
                self.advance();
                Ok(Token::Complex(0.0, value as grift_core::fsize))
            }
            _ => Ok(Token::Number(value)),
        }
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
        
        // Check for R7RS special float constants: +inf.0, -inf.0, +nan.0, -nan.0
        match &self.symbol_buf[..len] {
            b"+inf.0" => return Ok(Token::Float(grift_core::fsize::INFINITY)),
            b"-inf.0" => return Ok(Token::Float(grift_core::fsize::NEG_INFINITY)),
            b"+nan.0" | b"-nan.0" => return Ok(Token::Float(grift_core::fsize::NAN)),
            _ => {}
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
        let mut exactness: u8 = 0; // 0 = unspecified, 1 = exact (#e), 2 = inexact (#i)
        
        // Parse first prefix character (already peeked)
        match self.peek() {
            Some(b'b') | Some(b'B') => { self.advance(); radix = 2; }
            Some(b'o') | Some(b'O') => { self.advance(); radix = 8; }
            Some(b'd') | Some(b'D') => { self.advance(); radix = 10; }
            Some(b'x') | Some(b'X') => { self.advance(); radix = 16; }
            Some(b'e') | Some(b'E') => { self.advance(); exactness = 1; }
            Some(b'i') | Some(b'I') => { self.advance(); exactness = 2; }
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
                Some(b'e') | Some(b'E') if exactness == 0 && radix != 0 => { self.advance(); exactness = 1; }
                Some(b'i') | Some(b'I') if exactness == 0 && radix != 0 => { self.advance(); exactness = 2; }
                _ => return Err(self.error(LexErrorKind::InvalidHashLiteral)),
            }
        }
        
        // Default radix is 10
        if radix == 0 { radix = 10; }
        
        // Parse optional sign
        let negative = if self.peek() == Some(b'-') {
            self.advance();
            true
        } else if self.peek() == Some(b'+') {
            self.advance();
            false
        } else {
            false
        };
        
        // Check for special R7RS float literals: +inf.0, -inf.0, +nan.0
        if radix == 10
            && let Some(special) = self.try_lex_special_float(negative) {
                return Ok(special);
            }
        
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
        
        // For decimal radix, check for floating-point continuation
        if radix == 10 {
            let has_dot = self.peek() == Some(b'.') 
                && self.peek_next().is_some_and(|c| c.is_ascii_digit() || c == b'e' || c == b'E');
            let has_exp = self.peek() == Some(b'e') || self.peek() == Some(b'E');
            
            if has_dot || has_exp {
                if !has_digits {
                    return Err(self.error(LexErrorKind::InvalidHashLiteral));
                }
                // Force float path; exactness=1 (#e) will convert back to int
                let tok = self.lex_float_tail(value, negative)?;
                if exactness == 1 {
                    // #e forces exact: convert float to integer
                    if let Token::Float(f) = tok {
                        return Ok(Token::Number(f as isize));
                    }
                }
                return Ok(tok);
            }
        }
        
        if !has_digits {
            return Err(self.error(LexErrorKind::InvalidHashLiteral));
        }
        
        if negative { value = -value; }
        
        // #i forces inexact (float) representation
        if exactness == 2 {
            Ok(Token::Float(value as grift_core::fsize))
        } else {
            Ok(Token::Number(value))
        }
    }
    
    /// Try to lex special float constants: inf.0, nan.0
    /// Called when sign has already been parsed. Returns None if not a match (position unchanged).
    fn try_lex_special_float(&mut self, negative: bool) -> Option<Token> {
        let save_pos = self.pos;
        let save_line = self.line;
        let save_col = self.column;
        
        // Peek ahead for "inf.0" or "nan.0"
        let start = self.pos;
        // Read up to 5 chars
        for _ in 0..5 {
            if let Some(c) = self.peek() {
                if c.is_ascii_alphanumeric() || c == b'.' {
                    self.advance();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        
        let name = &self.input[start..self.pos];
        
        if name == b"inf.0" {
            let val = if negative { grift_core::fsize::NEG_INFINITY } else { grift_core::fsize::INFINITY };
            return Some(Token::Float(val));
        }
        if name == b"nan.0" {
            return Some(Token::Float(grift_core::fsize::NAN));
        }
        
        // Not a match - restore position
        self.pos = save_pos;
        self.line = save_line;
        self.column = save_col;
        None
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
                Some(c) if c >= 0x80 => {
                    // Multi-byte UTF-8 character literal
                    let seq_len = if c < 0xE0 { 2 } else if c < 0xF0 { 3 } else { 4 };
                    let s = self.pos;
                    let e = (s + seq_len).min(self.input.len());
                    if let Ok(utf8) = core::str::from_utf8(&self.input[s..e]) {
                        if let Some(ch) = utf8.chars().next() {
                            let byte_len = ch.len_utf8();
                            for _ in 0..byte_len {
                                self.advance();
                            }
                            Ok(Token::Char(ch))
                        } else {
                            self.advance();
                            Ok(Token::Char(c as char))
                        }
                    } else {
                        self.advance();
                        Ok(Token::Char(c as char))
                    }
                }
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
                    if let Some(code) = parse_hex(hex_str)
                        && let Some(c) = char::from_u32(code) {
                            return Ok(Token::Char(c));
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
                    if c < 0x80 {
                        // ASCII byte
                        self.string_buf[len] = c as char;
                        len += 1;
                        self.advance();
                    } else {
                        // Multi-byte UTF-8: determine sequence length from leading byte
                        let seq_len = if c < 0xE0 { 2 } else if c < 0xF0 { 3 } else { 4 };
                        let start = self.pos;
                        let end = (start + seq_len).min(self.input.len());
                        if let Ok(s) = core::str::from_utf8(&self.input[start..end]) {
                            if let Some(ch) = s.chars().next() {
                                self.string_buf[len] = ch;
                                len += 1;
                                let byte_len = ch.len_utf8();
                                for _ in 0..byte_len {
                                    self.advance();
                                }
                            } else {
                                self.advance();
                            }
                        } else {
                            // Invalid UTF-8: store raw byte
                            self.string_buf[len] = c as char;
                            len += 1;
                            self.advance();
                        }
                    }
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

/// Multiply a float by 10^exp without using std math functions.
/// Works in no_std by repeated multiplication/division.
fn mul_pow10(value: grift_core::fsize, exp: i32) -> grift_core::fsize {
    let mut result = value;
    if exp >= 0 {
        for _ in 0..exp {
            result *= 10.0;
        }
    } else {
        for _ in 0..(-exp) {
            result /= 10.0;
        }
    }
    result
}

/// Compute cos using libm (no_std compatible).
fn libm_cos(x: grift_core::fsize) -> grift_core::fsize {
    #[cfg(target_pointer_width = "64")]
    { libm::cos(x) }
    #[cfg(not(target_pointer_width = "64"))]
    { libm::cosf(x) }
}

/// Compute sin using libm (no_std compatible).
fn libm_sin(x: grift_core::fsize) -> grift_core::fsize {
    #[cfg(target_pointer_width = "64")]
    { libm::sin(x) }
    #[cfg(not(target_pointer_width = "64"))]
    { libm::sinf(x) }
}
