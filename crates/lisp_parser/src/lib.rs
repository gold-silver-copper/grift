#![no_std]

//! # Lisp Parser
//!
//! A classic Lisp parser with arena-allocated values.
//!
//! ## Design
//!
//! - Symbols are linked lists of `Char` values (classic Lisp style)
//! - All values are stored in a `pwn_arena` arena
//! - Supports garbage collection via the `Trace` trait
//! - Explicit boolean values (#t, #f) separate from nil/empty list
//! - **Lazy by default** - All evaluation is call-by-need (like Haskell)
//!
//! ## Value Representation
//!
//! - `Nil` - The empty list (NOT false!)
//! - `True` - Boolean true (#t)
//! - `False` - Boolean false (#f)
//! - `Number(i64)` - Integer numbers
//! - `Char(char)` - Single character
//! - `Cons { car, cdr }` - Pair/list cell
//! - `Symbol { chars }` - Symbol (tagged char list)
//! - `Lambda { params, body, env }` - Closure
//! - `Thunk { expr, env, cached }` - Lazy computation (internal, auto-managed)
//! - `Builtin(Builtin)` - Optimized built-in function

pub use pwn_arena::{Arena, ArenaIndex, ArenaError, ArenaResult, Trace, GcStats};

/// Built-in functions (optimization to avoid symbol lookup)
/// 
/// NOTE: This is a PURE, LAZY Lisp (like Haskell)!
/// - No mutation operations
/// - All evaluation is call-by-need (lazy by default)
/// - Values are forced automatically in strict positions
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    // List operations (non-strict - don't force arguments)
    Car,
    Cdr,
    Cons,
    List,
    
    // Predicates (force their argument to check type)
    Atom,
    Eq,
    Null,
    Pairp,
    Numberp,
    Booleanp,
    Procedurep,
    Symbolp,
    
    // Arithmetic (strict - force arguments)
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    
    // Comparison (strict - force arguments)
    Lt,
    Gt,
    Le,
    Ge,
    NumEq,
    
    // Boolean operations
    Not,
    
    // I/O (strict - force arguments for printing)
    Print,
    Newline,
    Display,
    
    // Error handling
    Error,
    
    // Memoization
    Memoize,
    
    // Symbol generation for hygiene
    Gensym,
}

impl Builtin {
    /// Get the symbol name for this builtin
    pub const fn name(&self) -> &'static str {
        match self {
            Builtin::Car => "car",
            Builtin::Cdr => "cdr",
            Builtin::Cons => "cons",
            Builtin::List => "list",
            Builtin::Atom => "atom",
            Builtin::Eq => "eq",
            Builtin::Null => "null?",
            Builtin::Pairp => "pair?",
            Builtin::Numberp => "number?",
            Builtin::Booleanp => "boolean?",
            Builtin::Procedurep => "procedure?",
            Builtin::Symbolp => "symbol?",
            Builtin::Add => "+",
            Builtin::Sub => "-",
            Builtin::Mul => "*",
            Builtin::Div => "/",
            Builtin::Mod => "mod",
            Builtin::Lt => "<",
            Builtin::Gt => ">",
            Builtin::Le => "<=",
            Builtin::Ge => ">=",
            Builtin::NumEq => "=",
            Builtin::Not => "not",
            Builtin::Print => "print",
            Builtin::Newline => "newline",
            Builtin::Display => "display",
            Builtin::Error => "error",
            Builtin::Memoize => "memoize",
            Builtin::Gensym => "gensym",
        }
    }
    
    /// All builtins for initialization
    pub const ALL: &'static [Builtin] = &[
        Builtin::Car, Builtin::Cdr, Builtin::Cons, Builtin::List,
        Builtin::Atom, Builtin::Eq, Builtin::Null, Builtin::Pairp,
        Builtin::Numberp, Builtin::Booleanp, Builtin::Procedurep, Builtin::Symbolp,
        Builtin::Add, Builtin::Sub, Builtin::Mul, Builtin::Div, Builtin::Mod,
        Builtin::Lt, Builtin::Gt, Builtin::Le, Builtin::Ge, Builtin::NumEq,
        Builtin::Not,
        Builtin::Print, Builtin::Newline, Builtin::Display,
        Builtin::Error,
        Builtin::Memoize,
        Builtin::Gensym,
    ];
}

/// A Lisp value
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// The empty list (NOT false - use False for that)
    Nil,
    
    /// Boolean true (#t)
    True,
    
    /// Boolean false (#f) - the ONLY false value
    False,
    
    /// Integer number
    Number(i64),
    
    /// Single character (for building symbol char-lists)
    Char(char),
    
    /// Cons cell (pair)
    Cons {
        car: ArenaIndex,
        cdr: ArenaIndex,
    },
    
    /// Symbol (contains a linked list of Char values)
    Symbol {
        chars: ArenaIndex,
    },
    
    /// Lambda / closure
    Lambda {
        params: ArenaIndex,  // List of symbols
        body: ArenaIndex,    // Expression
        env: ArenaIndex,     // Captured environment (alist)
    },
    
    /// Thunk - delayed computation for call-by-need
    /// Created by (delay expr), forced by (force thunk)
    Thunk {
        expr: ArenaIndex,    // Unevaluated expression
        env: ArenaIndex,     // Environment for evaluation
        cached: ArenaIndex,  // Cached result (NULL if not yet evaluated)
    },
    
    /// Memoized function - caches results keyed by argument values
    /// Created by (memoize fn), automatically caches return values
    Memo {
        func: ArenaIndex,    // The wrapped function (lambda or builtin)
        cache: ArenaIndex,   // Cache: alist of (args . result) pairs
    },
    
    /// Built-in function (optimized)
    Builtin(Builtin),
}

impl Value {
    /// Check if this value is nil (empty list)
    #[inline]
    pub const fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }
    
    /// Check if this value is false (#f)
    /// This is the ONLY way to be false in this Lisp
    #[inline]
    pub const fn is_false(&self) -> bool {
        matches!(self, Value::False)
    }
    
    /// Check if this value is true (#t)
    #[inline]
    pub const fn is_true(&self) -> bool {
        matches!(self, Value::True)
    }
    
    /// Check if this value is a boolean (#t or #f)
    #[inline]
    pub const fn is_boolean(&self) -> bool {
        matches!(self, Value::True | Value::False)
    }
    
    /// Check if this value is an atom (not a cons cell)
    #[inline]
    pub const fn is_atom(&self) -> bool {
        !matches!(self, Value::Cons { .. })
    }
    
    /// Check if this value is a number
    #[inline]
    pub const fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }
    
    /// Check if this value is a symbol
    #[inline]
    pub const fn is_symbol(&self) -> bool {
        matches!(self, Value::Symbol { .. })
    }
    
    /// Check if this value is a cons cell (pair)
    #[inline]
    pub const fn is_cons(&self) -> bool {
        matches!(self, Value::Cons { .. })
    }
    
    /// Check if this value is a lambda
    #[inline]
    pub const fn is_lambda(&self) -> bool {
        matches!(self, Value::Lambda { .. })
    }
    
    /// Check if this value is a builtin
    #[inline]
    pub const fn is_builtin(&self) -> bool {
        matches!(self, Value::Builtin(_))
    }
    
    /// Check if this value is a procedure (lambda, builtin, or memoized function)
    #[inline]
    pub const fn is_procedure(&self) -> bool {
        matches!(self, Value::Lambda { .. } | Value::Builtin(_) | Value::Memo { .. })
    }
    
    /// Check if this value is a thunk (promise)
    #[inline]
    pub const fn is_thunk(&self) -> bool {
        matches!(self, Value::Thunk { .. })
    }
    
    /// Check if this value is a memoized function
    #[inline]
    pub const fn is_memo(&self) -> bool {
        matches!(self, Value::Memo { .. })
    }
    
    /// Get the number value if this is a number
    #[inline]
    pub const fn as_number(&self) -> Option<i64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
    
    /// Get the char value if this is a char
    #[inline]
    pub const fn as_char(&self) -> Option<char> {
        match self {
            Value::Char(c) => Some(*c),
            _ => None,
        }
    }
    
    /// Get a human-readable type name
    pub const fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::True | Value::False => "boolean",
            Value::Number(_) => "number",
            Value::Char(_) => "char",
            Value::Cons { .. } => "pair",
            Value::Symbol { .. } => "symbol",
            Value::Lambda { .. } => "procedure",
            Value::Thunk { .. } => "promise",
            Value::Memo { .. } => "memoized",
            Value::Builtin(_) => "procedure",
        }
    }
}

/// Implement Trace for GC support
impl<const N: usize> Trace<Value, N> for Value {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match self {
            Value::Nil | Value::True | Value::False | 
            Value::Number(_) | Value::Char(_) | Value::Builtin(_) => {
                // No references
            }
            Value::Cons { car, cdr } => {
                tracer(*car);
                tracer(*cdr);
            }
            Value::Symbol { chars } => {
                tracer(*chars);
            }
            Value::Lambda { params, body, env } => {
                tracer(*params);
                tracer(*body);
                tracer(*env);
            }
            Value::Thunk { expr, env, cached } => {
                tracer(*expr);
                tracer(*env);
                if !cached.is_null() {
                    tracer(*cached);
                }
            }
            Value::Memo { func, cache } => {
                tracer(*func);
                tracer(*cache);
            }
        }
    }
}

// ============================================================================
// Lisp Context - Arena wrapper with helper methods
// ============================================================================

/// A Lisp execution context wrapping an arena
pub struct Lisp<const N: usize> {
    arena: Arena<Value, N>,
}

impl<const N: usize> Lisp<N> {
    /// Create a new Lisp context
    pub fn new() -> Self {
        Lisp {
            arena: Arena::new(Value::Nil),
        }
    }
    
    /// Get reference to the underlying arena
    pub fn arena(&self) -> &Arena<Value, N> {
        &self.arena
    }
    
    /// Allocate a value
    #[inline]
    pub fn alloc(&self, value: Value) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(value)
    }
    
    /// Get a value
    #[inline]
    pub fn get(&self, index: ArenaIndex) -> ArenaResult<Value> {
        self.arena.get(index)
    }
    
    /// Set a value
    #[inline]
    pub fn set(&self, index: ArenaIndex, value: Value) -> ArenaResult<()> {
        self.arena.set(index, value)
    }
    
    /// Allocate Nil (empty list)
    #[inline]
    pub fn nil(&self) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Nil)
    }
    
    /// Allocate True (#t)
    #[inline]
    pub fn true_val(&self) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::True)
    }
    
    /// Allocate False (#f)
    #[inline]
    pub fn false_val(&self) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::False)
    }
    
    /// Allocate a boolean based on a Rust bool
    #[inline]
    pub fn boolean(&self, b: bool) -> ArenaResult<ArenaIndex> {
        if b { self.true_val() } else { self.false_val() }
    }
    
    /// Allocate a number
    #[inline]
    pub fn number(&self, n: i64) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Number(n))
    }
    
    /// Allocate a character
    #[inline]
    pub fn char(&self, c: char) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Char(c))
    }
    
    /// Allocate a cons cell
    #[inline]
    pub fn cons(&self, car: ArenaIndex, cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Cons { car, cdr })
    }
    
    /// Get car of a cons cell
    #[inline]
    pub fn car(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(index)? {
            Value::Cons { car, .. } => Ok(car),
            Value::Nil => self.nil(), // car of nil is nil in classic Lisp
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get cdr of a cons cell
    #[inline]
    pub fn cdr(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(index)? {
            Value::Cons { cdr, .. } => Ok(cdr),
            Value::Nil => self.nil(), // cdr of nil is nil in classic Lisp
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    // NOTE: set_car and set_cdr removed - this is a PURE Lisp!
    // Mutation breaks referential transparency and call-by-need semantics.
    
    /// Create a symbol from a string slice (builds char list)
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        let mut chars = self.nil()?;
        
        // Build in reverse order
        for c in name.chars().rev() {
            let char_val = self.char(c)?;
            chars = self.cons(char_val, chars)?;
        }
        
        self.alloc(Value::Symbol { chars })
    }
    
    /// Create a symbol from bytes (for parsing)
    pub fn symbol_from_bytes(&self, bytes: &[u8]) -> ArenaResult<ArenaIndex> {
        let mut chars = self.nil()?;
        
        // Build in reverse order
        for &b in bytes.iter().rev() {
            let char_val = self.char(b as char)?;
            chars = self.cons(char_val, chars)?;
        }
        
        self.alloc(Value::Symbol { chars })
    }
    
    /// Allocate a builtin function
    #[inline]
    pub fn builtin(&self, b: Builtin) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Builtin(b))
    }
    
    /// Allocate a lambda
    pub fn lambda(&self, params: ArenaIndex, body: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Lambda { params, body, env })
    }
    
    /// Allocate a thunk (delayed computation)
    pub fn thunk(&self, expr: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Thunk { expr, env, cached: ArenaIndex::NULL })
    }
    
    /// Create a memoized function (wraps a function with a cache)
    pub fn memo(&self, func: ArenaIndex, cache: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Memo { func, cache })
    }
    
    /// Build a list from an iterator of indices
    pub fn list<I: IntoIterator<Item = ArenaIndex>>(&self, items: I) -> ArenaResult<ArenaIndex>
    where
        I::IntoIter: DoubleEndedIterator,
    {
        let mut result = self.nil()?;
        for item in items.into_iter().rev() {
            result = self.cons(item, result)?;
        }
        Ok(result)
    }
    
    /// Get the length of a list
    pub fn list_len(&self, mut list: ArenaIndex) -> ArenaResult<usize> {
        let mut len = 0;
        loop {
            match self.get(list)? {
                Value::Nil => return Ok(len),
                Value::Cons { cdr, .. } => {
                    len += 1;
                    list = cdr;
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
        }
    }
    
    /// Check if two symbols are equal (compare char lists)
    #[inline]
    pub fn symbol_eq(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        // Fast path: same index means same symbol
        if a == b {
            return Ok(true);
        }
        
        let val_a = self.get(a)?;
        let val_b = self.get(b)?;
        
        match (val_a, val_b) {
            (Value::Symbol { chars: chars_a }, Value::Symbol { chars: chars_b }) => {
                self.char_list_eq(chars_a, chars_b)
            }
            _ => Ok(false),
        }
    }
    
    /// Compare two char lists for equality
    fn char_list_eq(&self, mut a: ArenaIndex, mut b: ArenaIndex) -> ArenaResult<bool> {
        // Fast path: same index means same char list
        if a == b {
            return Ok(true);
        }
        
        loop {
            let val_a = self.get(a)?;
            let val_b = self.get(b)?;
            
            match (val_a, val_b) {
                (Value::Nil, Value::Nil) => return Ok(true),
                (Value::Cons { car: car_a, cdr: cdr_a }, Value::Cons { car: car_b, cdr: cdr_b }) => {
                    let char_a = self.get(car_a)?;
                    let char_b = self.get(car_b)?;
                    
                    match (char_a, char_b) {
                        (Value::Char(c1), Value::Char(c2)) if c1 == c2 => {
                            a = cdr_a;
                            b = cdr_b;
                        }
                        _ => return Ok(false),
                    }
                }
                _ => return Ok(false),
            }
        }
    }
    
    /// Check if a symbol matches a string
    #[inline]
    pub fn symbol_matches(&self, sym: ArenaIndex, name: &str) -> ArenaResult<bool> {
        let val = self.get(sym)?;
        
        match val {
            Value::Symbol { chars } => {
                let mut list = chars;
                let mut chars_iter = name.chars();
                
                loop {
                    let list_val = self.get(list)?;
                    let next_char = chars_iter.next();
                    
                    match (list_val, next_char) {
                        (Value::Nil, None) => return Ok(true),
                        (Value::Cons { car, cdr }, Some(expected)) => {
                            if let Value::Char(c) = self.get(car)? {
                                if c != expected {
                                    return Ok(false);
                                }
                                list = cdr;
                            } else {
                                return Ok(false);
                            }
                        }
                        _ => return Ok(false),
                    }
                }
            }
            _ => Ok(false),
        }
    }
    
    /// Extract symbol name to a fixed buffer
    pub fn symbol_to_bytes(&self, sym: ArenaIndex, buf: &mut [u8]) -> ArenaResult<usize> {
        let val = self.get(sym)?;
        
        match val {
            Value::Symbol { chars } => {
                let mut list = chars;
                let mut len = 0;
                
                loop {
                    if len >= buf.len() {
                        break;
                    }
                    match self.get(list)? {
                        Value::Nil => break,
                        Value::Cons { car, cdr } => {
                            if let Value::Char(c) = self.get(car)? {
                                buf[len] = c as u8;
                                len += 1;
                            }
                            list = cdr;
                        }
                        _ => break,
                    }
                }
                Ok(len)
            }
            _ => Ok(0),
        }
    }
    
    /// Run garbage collection
    pub fn gc(&self, roots: &[ArenaIndex]) -> GcStats {
        self.arena.collect_garbage(roots)
    }
    
    /// Allocate with GC on failure
    pub fn alloc_or_gc(&self, value: Value, roots: &[ArenaIndex]) -> ArenaResult<ArenaIndex> {
        self.arena.alloc_or_gc(value, roots)
    }
    
    /// Get arena stats
    pub fn stats(&self) -> pwn_arena::ArenaStats {
        self.arena.stats()
    }
}

impl<const N: usize> Default for Lisp<N> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Parser
// ============================================================================

/// Source location for error reporting
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceLoc {
    pub line: u32,
    pub column: u32,
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
}

impl ParseError {
    pub fn new(kind: ParseErrorKind, line: u32, column: u32) -> Self {
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
    line: u32,
    column: u32,
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
            b'?' | b'!' | b'_' | b'&' | b'%' | b'^' | b'~'
        )
    }
    
    /// Parse a single expression
    pub fn parse<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        self.skip_whitespace();
        
        match self.peek() {
            None => Err(self.error(ParseErrorKind::UnexpectedEof)),
            
            Some(b'(') => self.parse_list(lisp),
            
            Some(b')') => Err(self.error(ParseErrorKind::UnmatchedParen)),
            
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
    
    /// Parse hash literals (#t, #f, etc.)
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
            Some(_) => Err(self.error(ParseErrorKind::InvalidHashLiteral)),
            None => Err(self.error(ParseErrorKind::UnexpectedEof)),
        }
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
    
    /// Parse a number
    fn parse_number<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        let mut value: i64 = 0;
        let negative = if self.peek() == Some(b'-') {
            self.advance();
            true
        } else {
            false
        };
        
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
                value = value.checked_mul(10)
                    .and_then(|v| v.checked_add((c - b'0') as i64))
                    .ok_or_else(|| self.error(ParseErrorKind::NumberOverflow))?;
            } else {
                break;
            }
        }
        
        if negative {
            value = -value;
        }
        
        lisp.number(value).map_err(Into::into)
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
        
        // Check for special symbols
        if name == b"nil" {
            return lisp.nil().map_err(Into::into);
        }
        // Note: #t and #f are now the canonical booleans
        // but we can still allow 'true' and 'false' as symbols that 
        // the evaluator can bind to #t and #f
        
        lisp.symbol_from_bytes(name).map_err(Into::into)
    }
    
    /// Check if there's more input (after whitespace)
    pub fn has_more(&mut self) -> bool {
        self.skip_whitespace();
        self.peek().is_some()
    }
    
    /// Get current position for error reporting
    pub fn position(&self) -> (u32, u32) {
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_number() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "42").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::Number(42));
        
        let idx = parse(&lisp, "-123").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::Number(-123));
    }
    
    #[test]
    fn test_parse_symbol() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "hello").unwrap();
        assert!(lisp.symbol_matches(idx, "hello").unwrap());
    }
    
    #[test]
    fn test_parse_nil() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "nil").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::Nil);
        
        let idx = parse(&lisp, "()").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::Nil);
    }
    
    #[test]
    fn test_parse_booleans() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "#t").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::True);
        
        let idx = parse(&lisp, "#f").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::False);
        
        let idx = parse(&lisp, "#T").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::True);
        
        let idx = parse(&lisp, "#F").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::False);
    }
    
    #[test]
    fn test_parse_list() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "(1 2 3)").unwrap();
        
        // Check it's a cons
        let val = lisp.get(idx).unwrap();
        assert!(val.is_cons());
        
        // Check first element
        let car = lisp.car(idx).unwrap();
        assert_eq!(lisp.get(car).unwrap(), Value::Number(1));
    }
    
    #[test]
    fn test_parse_nested() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "(+ 1 (- 3 2))").unwrap();
        assert!(lisp.get(idx).unwrap().is_cons());
    }
    
    #[test]
    fn test_parse_quote() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "'x").unwrap();
        
        // Should be (quote x)
        let car = lisp.car(idx).unwrap();
        assert!(lisp.symbol_matches(car, "quote").unwrap());
    }
    
    #[test]
    fn test_symbol_equality() {
        let lisp: Lisp<100> = Lisp::new();
        
        let a = lisp.symbol("hello").unwrap();
        let b = lisp.symbol("hello").unwrap();
        let c = lisp.symbol("world").unwrap();
        
        assert!(lisp.symbol_eq(a, b).unwrap());
        assert!(!lisp.symbol_eq(a, c).unwrap());
    }
    
    // NOTE: test_set_car_cdr removed - this is a PURE Lisp!
    
    #[test]
    fn test_gc() {
        let lisp: Lisp<100> = Lisp::new();
        
        let root = parse(&lisp, "(1 2 3)").unwrap();
        
        // Allocate garbage
        for i in 0..20 {
            lisp.number(i * 1000).unwrap();
        }
        
        let stats = lisp.gc(&[root]);
        assert!(stats.collected > 0);
    }
    
    #[test]
    fn test_thunk_creation() {
        let lisp: Lisp<100> = Lisp::new();
        
        let expr = lisp.number(42).unwrap();
        let env = lisp.nil().unwrap();
        let thunk = lisp.thunk(expr, env).unwrap();
        
        assert!(lisp.get(thunk).unwrap().is_thunk());
    }
    
    #[test]
    fn test_thunk_structure() {
        let lisp: Lisp<100> = Lisp::new();
        
        let expr = lisp.number(42).unwrap();
        let env = lisp.nil().unwrap();
        let thunk = lisp.thunk(expr, env).unwrap();
        
        // Check thunk structure
        match lisp.get(thunk).unwrap() {
            Value::Thunk { expr: e, env: en, cached: c } => {
                assert_eq!(lisp.get(e).unwrap(), Value::Number(42));
                assert_eq!(lisp.get(en).unwrap(), Value::Nil);
                assert!(c.is_null()); // Not yet cached
            }
            _ => panic!("Expected Thunk"),
        }
    }
    
    #[test]
    fn test_thunk_with_complex_expr() {
        let lisp: Lisp<200> = Lisp::new();
        
        // Create a complex expression as the delayed expr
        let one = lisp.number(1).unwrap();
        let two = lisp.number(2).unwrap();
        let pair = lisp.cons(one, two).unwrap();
        let env = lisp.nil().unwrap();
        
        let thunk = lisp.thunk(pair, env).unwrap();
        
        assert!(lisp.get(thunk).unwrap().is_thunk());
        
        // Verify the expr is our pair
        match lisp.get(thunk).unwrap() {
            Value::Thunk { expr, .. } => {
                match lisp.get(expr).unwrap() {
                    Value::Cons { car, cdr } => {
                        assert_eq!(lisp.get(car).unwrap(), Value::Number(1));
                        assert_eq!(lisp.get(cdr).unwrap(), Value::Number(2));
                    }
                    _ => panic!("Expected Cons in thunk"),
                }
            }
            _ => panic!("Expected Thunk"),
        }
    }
    
    #[test]
    fn test_thunk_with_environment() {
        let lisp: Lisp<300> = Lisp::new();
        
        // Create a non-empty environment
        let name = lisp.symbol("x").unwrap();
        let value = lisp.number(100).unwrap();
        let binding = lisp.cons(name, value).unwrap();
        let env = lisp.cons(binding, lisp.nil().unwrap()).unwrap();
        
        let expr = lisp.number(42).unwrap();
        let thunk = lisp.thunk(expr, env).unwrap();
        
        // Verify environment is captured
        match lisp.get(thunk).unwrap() {
            Value::Thunk { env: e, .. } => {
                // First binding
                let first = lisp.car(e).unwrap();
                let bound_name = lisp.car(first).unwrap();
                let bound_val = lisp.cdr(first).unwrap();
                
                assert!(lisp.symbol_matches(bound_name, "x").unwrap());
                assert_eq!(lisp.get(bound_val).unwrap(), Value::Number(100));
            }
            _ => panic!("Expected Thunk"),
        }
    }
    
    #[test]
    fn test_thunk_gc_trace() {
        // Test that GC properly traces through thunks
        let lisp: Lisp<200> = Lisp::new();
        
        // Create data that will be referenced by thunk
        let inner_data = lisp.cons(lisp.number(1).unwrap(), lisp.number(2).unwrap()).unwrap();
        let env_val = lisp.number(999).unwrap();
        let env_name = lisp.symbol("y").unwrap();
        let binding = lisp.cons(env_name, env_val).unwrap();
        let env = lisp.cons(binding, lisp.nil().unwrap()).unwrap();
        
        let thunk = lisp.thunk(inner_data, env).unwrap();
        
        // Create garbage
        for i in 0..50 {
            lisp.number(i * 1000).unwrap();
        }
        
        // Run GC with thunk as root
        let stats = lisp.gc(&[thunk]);
        assert!(stats.collected > 0);
        
        // Thunk and its referenced data should still be accessible
        assert!(lisp.get(thunk).unwrap().is_thunk());
        match lisp.get(thunk).unwrap() {
            Value::Thunk { expr, env, .. } => {
                // inner_data should still be valid
                assert!(lisp.get(expr).unwrap().is_cons());
                // environment should still be valid
                assert!(lisp.get(env).unwrap().is_cons());
            }
            _ => panic!("Expected Thunk"),
        }
    }
    
    #[test]
    fn test_thunk_value_predicates() {
        let lisp: Lisp<100> = Lisp::new();
        
        let expr = lisp.number(42).unwrap();
        let env = lisp.nil().unwrap();
        let thunk = lisp.thunk(expr, env).unwrap();
        let thunk_val = lisp.get(thunk).unwrap();
        
        // Test all predicates on thunk
        assert!(thunk_val.is_thunk());
        assert!(!thunk_val.is_nil());
        assert!(!thunk_val.is_number());
        assert!(!thunk_val.is_symbol());
        assert!(!thunk_val.is_cons());
        assert!(!thunk_val.is_lambda());
        assert!(!thunk_val.is_builtin());
        assert!(!thunk_val.is_true());
        assert!(!thunk_val.is_false());
        assert!(!thunk_val.is_boolean());
        assert!(thunk_val.is_atom()); // Thunks are atoms
        assert!(!thunk_val.is_procedure()); // Thunks are not procedures
        
        // Type name
        assert_eq!(thunk_val.type_name(), "promise");
    }
    
    #[test]
    fn test_multiple_thunks() {
        let lisp: Lisp<500> = Lisp::new();
        
        // Create multiple thunks
        let env = lisp.nil().unwrap();
        let t1 = lisp.thunk(lisp.number(1).unwrap(), env).unwrap();
        let t2 = lisp.thunk(lisp.number(2).unwrap(), env).unwrap();
        let t3 = lisp.thunk(lisp.number(3).unwrap(), env).unwrap();
        
        // Put them in a list
        let list = lisp.cons(t3, lisp.nil().unwrap()).unwrap();
        let list = lisp.cons(t2, list).unwrap();
        let list = lisp.cons(t1, list).unwrap();
        
        // All should be thunks
        let first = lisp.car(list).unwrap();
        assert!(lisp.get(first).unwrap().is_thunk());
        
        let second = lisp.car(lisp.cdr(list).unwrap()).unwrap();
        assert!(lisp.get(second).unwrap().is_thunk());
        
        let third = lisp.car(lisp.cdr(lisp.cdr(list).unwrap()).unwrap()).unwrap();
        assert!(lisp.get(third).unwrap().is_thunk());
    }
}
