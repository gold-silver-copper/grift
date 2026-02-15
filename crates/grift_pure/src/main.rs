//! Shared utilities for the Grift Scheme implementation.
//!
//! This crate provides common functions used by both the proc-macro crate
//! (`grift_macros`) and the rest of the workspace, avoiding duplication
//! of logic such as Scheme-name → Rust-name conversion.

/// Convert a Scheme-style name to PascalCase for use as a Rust enum variant.
///
/// # Conversion rules
///
/// | Scheme character | Rust equivalent |
/// |-----------------|-----------------|
/// | `-` or `_`      | capitalize next |
/// | `!`             | stripped        |
/// | `?`             | `P` (predicate) |
/// | `=`             | `Eq`            |
/// | `>`             | `Gt`            |
/// | `<`             | `Lt`            |
/// | `+`             | `Plus`          |
/// | `*`             | `Star`          |
/// | `/`             | `Slash`         |
///
/// # Examples
///
/// ```
/// use grift_util::to_pascal_case;
///
/// assert_eq!(to_pascal_case("map"), "Map");
/// assert_eq!(to_pascal_case("set-car!"), "SetCar");
/// assert_eq!(to_pascal_case("null?"), "NullP");
/// assert_eq!(to_pascal_case("char->integer"), "CharGtInteger");
/// assert_eq!(to_pascal_case("rat+"), "RatPlus");
/// assert_eq!(to_pascal_case("rat*"), "RatStar");
/// assert_eq!(to_pascal_case("rat/"), "RatSlash");
/// ```
pub fn to_pascal_case(name: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for c in name.chars() {
        match c {
            '-' | '_' => {
                capitalize_next = true;
            }
            '!' => {
                // Skip exclamation marks
            }
            '?' => {
                // Replace with 'P' (predicate convention)
                result.push('P');
                capitalize_next = true;
            }
            '=' => {
                // Replace with 'Eq' for equality operators
                result.push_str("Eq");
                capitalize_next = true;
            }
            '>' => {
                // Replace with 'Gt' for greater-than
                result.push_str("Gt");
                capitalize_next = true;
            }
            '<' => {
                // Replace with 'Lt' for less-than
                result.push_str("Lt");
                capitalize_next = true;
            }
            '+' => {
                // Replace with 'Plus' for addition
                result.push_str("Plus");
                capitalize_next = true;
            }
            '*' => {
                // Replace with 'Star' for multiplication
                result.push_str("Star");
                capitalize_next = true;
            }
            '/' => {
                // Replace with 'Slash' for division
                result.push_str("Slash");
                capitalize_next = true;
            }
            _ => {
                if capitalize_next {
                    result.extend(c.to_uppercase());
                    capitalize_next = false;
                } else {
                    result.push(c);
                }
            }
        }
    }

    result
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
}
// ============================================================================
// Parser
// ============================================================================

/// Reverse a cons list in-place by mutating cdr pointers.
///
/// Given a reversed list like `(c b a)` and a tail, produces `(a b c . tail)`
/// by relinking the same cons cells. Zero extra allocation.
///
/// `cur` must be either nil or a proper/improper cons list. The function
/// walks cdr links until it reaches nil, relinking each cell to point to
/// the previous one.
fn reverse_list_in_place<const N: usize>(
    lisp: &Lisp<N>,
    mut cur: ArenaIndex,
    tail: ArenaIndex,
) -> Result<ArenaIndex, ParseError> {
    let mut prev = tail;
    while !cur.is_nil() {
        let next = lisp.cdr(cur)?;
        lisp.set_cdr(cur, prev)?;
        prev = cur;
        cur = next;
    }
    Ok(prev)
}

/// Parse an expression starting from a given token
fn parse_token<const N: usize>(
    &mut self,
    token: Token,
    loc: SourceLoc,
    lisp: &Lisp<N>,
) -> Result<ArenaIndex, ParseError> {
    match token {
        Token::LParen => self.parse_list(lisp),
        Token::RParen => Err(ParseError {
            kind: ParseErrorKind::UnmatchedParen,
            loc,
        }),
        Token::Quote => self.parse_quote("quote", lisp),
        Token::Quasiquote => self.parse_quote("quasiquote", lisp),

        Token::Dot => Err(ParseError {
            kind: ParseErrorKind::UnexpectedChar('.'),
            loc,
        }),
        Token::True => lisp.true_val().map_err(Into::into),
        Token::False => lisp.false_val().map_err(Into::into),
        Token::Number(n) => lisp.number(n).map_err(Into::into),

        Token::Char(c) => lisp.char(c).map_err(Into::into),
        Token::Symbol { start, len } => {
            let name = self.lexer.input_slice(start, len);
            lisp.symbol_from_bytes_folded(name, self.lexer.is_fold_case())
                .map_err(Into::into)
        }
        Token::InternedSymbol(idx) => Ok(idx),
        Token::String(idx) => Ok(idx),
    }
}
/// Parse a quoted expression: 'x -> (quote x), etc.
fn parse_quote<const N: usize>(
    &mut self,
    sym_name: &str,
    lisp: &Lisp<N>,
) -> Result<ArenaIndex, ParseError> {
    let expr = self.parse(lisp)?;
    let sym = lisp.symbol(sym_name)?;
    let nil = lisp.nil()?;
    let quoted = lisp.cons(expr, nil)?;
    lisp.cons(sym, quoted).map_err(Into::into)
}

//! # Native Function Interop
//!
//! This module provides traits and macros for calling Rust functions from Lisp code.
//!
//! ## Overview
//!
//! The native function interop system allows you to:
//! - Register Rust functions that can be called from Lisp
//! - Automatically convert Lisp values to Rust types and back
//! - Handle errors gracefully
//!
//! ## Key Traits
//!
//! - [`FromLisp`] - Convert a Lisp value to a Rust type
//! - [`ToLisp`] - Convert a Rust type to a Lisp value
//!
//! ## Usage
//!
//! ```rust
//! use grift_eval::{NativeRegistry, register_native};
//!
//! // Define a native function using the register_native! macro
//! register_native!(add_one, (x: isize) -> isize, { x + 1 });
//!
//! // Register it with an evaluator
//! // eval.register_native("add-one", add_one).unwrap();
//! ```
//!
//! ## Design Notes
//!
//! This module is `no_std` compatible and uses no heap allocation.
//! All conversions work directly with arena-allocated values.

use crate::{ArenaIndex, ArenaResult, ArenaError, Lisp, Value, fsize};

// ============================================================================
// Conversion Traits
// ============================================================================

/// Trait for converting Lisp values to Rust types.
///
/// Implement this trait for any type you want to extract from Lisp arguments.
///
/// # Example
///
/// ```rust
/// use grift_eval::{FromLisp, Lisp, ArenaIndex, ArenaResult, Value};
///
/// // isize is already implemented
/// fn example<const N: usize>(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<isize> {
///     isize::from_lisp(lisp, idx)
/// }
/// ```
pub trait FromLisp<const N: usize>: Sized {
    /// Convert a Lisp value at the given index to this Rust type.
    ///
    /// Returns an error if the conversion fails (e.g., type mismatch).
    fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self>;
}

/// Trait for converting Rust types to Lisp values.
///
/// Implement this trait for any type you want to return to Lisp code.
///
/// # Example
///
/// ```rust
/// use grift_eval::{ToLisp, Lisp, ArenaIndex, ArenaResult};
///
/// // isize is already implemented
/// fn example<const N: usize>(lisp: &Lisp<N>, value: isize) -> ArenaResult<ArenaIndex> {
///     value.to_lisp(lisp)
/// }
/// ```
pub trait ToLisp<const N: usize> {
    /// Convert this Rust value to a Lisp value, allocating in the arena.
    ///
    /// Returns the ArenaIndex of the newly allocated value.
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex>;
}

// ============================================================================
// Implementations for Common Types
// ============================================================================

// Note: Using InvalidIndex for type errors is semantically imprecise,
// but ArenaError doesn't have a TypeError variant and adding one
// would require changes to the core no_std crate.
macro_rules! impl_from_lisp {
    ($ty:ty, $($pattern:pat => $expr:expr),+ $(,)?) => {
        impl<const N: usize> FromLisp<N> for $ty {
            fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self> {
                match lisp.get(idx)? {
                    $($pattern => Ok($expr),)+
                    _ => Err(ArenaError::InvalidIndex),
                }
            }
        }
    };
}

impl_from_lisp!(isize, Value::Number(n) => n);
impl_from_lisp!(bool, Value::True => true, Value::False => false);
impl_from_lisp!((), Value::Nil => ());
impl_from_lisp!(char, Value::Char(c) => c);
impl_from_lisp!(fsize, Value::Number(n) => n as fsize, Value::Float(f) => f);

impl<const N: usize> ToLisp<N> for isize {
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        lisp.number(*self)
    }
}

impl<const N: usize> ToLisp<N> for bool {
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        lisp.boolean(*self)
    }
}

impl<const N: usize> ToLisp<N> for () {
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        lisp.nil()
    }
}

impl<const N: usize> ToLisp<N> for char {
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        lisp.char(*self)
    }
}

/// ArenaIndex can be passed through directly (for when you want raw Lisp values)
impl<const N: usize> FromLisp<N> for ArenaIndex {
    fn from_lisp(_lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self> {
        Ok(idx)
    }
}

impl<const N: usize> ToLisp<N> for ArenaIndex {
    fn to_lisp(&self, _lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        Ok(*self)
    }
}

// ============================================================================
// GC Root Tracking
// ============================================================================

/// Trait for types that contain GC roots.
///
/// Implementors should call `tracer` for each [`ArenaIndex`] that represents
/// a live GC root. This ensures the garbage collector does not collect
/// reachable objects.
///
/// By centralizing root enumeration in this trait, adding a new
/// [`ArenaIndex`] field to a type will produce a compile-time reminder
/// (or at least a single, obvious place) to update root tracking, rather
/// than requiring updates in every GC call-site.
pub trait GcRoots {
    /// Call `tracer` once for every [`ArenaIndex`] that is a live GC root.
    fn trace_roots(&self, tracer: &mut dyn FnMut(ArenaIndex));
}

// ============================================================================
// Typed Index Newtypes
// ============================================================================

/// Macro to generate a typed wrapper around [`ArenaIndex`].
///
/// Each generated type provides `index()`, `new()`, and bidirectional `From`
/// conversions, preventing accidentally passing an expression where an
/// environment is expected (or vice versa).
macro_rules! define_index_wrapper {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name(pub(crate) ArenaIndex);

        impl $name {
            /// Get the underlying [`ArenaIndex`].
            pub const fn index(self) -> ArenaIndex {
                self.0
            }

            /// Create from a raw [`ArenaIndex`].
            pub const fn new(idx: ArenaIndex) -> Self {
                $name(idx)
            }
        }

        impl From<ArenaIndex> for $name {
            fn from(idx: ArenaIndex) -> Self {
                $name(idx)
            }
        }

        impl From<$name> for ArenaIndex {
            fn from(r: $name) -> Self {
                r.0
            }
        }
    };
}

define_index_wrapper!(
    /// A typed wrapper around [`ArenaIndex`] representing an environment chain.
    ///
    /// Environments are linked lists of `(name . value)` bindings stored in the
    /// arena. Using a distinct type prevents accidentally passing an expression
    /// where an environment is expected.
    EnvRef
);

define_index_wrapper!(
    /// A typed wrapper around [`ArenaIndex`] representing an expression to evaluate.
    ///
    /// Expressions are S-expressions stored in the arena. Using a distinct type
    /// prevents accidentally passing an environment where an expression is expected.
    ExprRef
);

// ============================================================================
// Continuation Type Enum
// ============================================================================
//
// The `ContType` enum identifies the continuation type stored in each ContFrame.
// Using an enum provides exhaustiveness checking in `step_return()` and makes
// adding new continuation types compiler-checked.
//
// The data field of each ContFrame contains continuation-specific data
// encoded as cons cells in the arena.

/// Trampoline state - what we're currently doing
#[derive(Clone, Copy, Debug)]
pub enum TrampolineState {
    /// Evaluate expression in environment
    Eval { expr: ExprRef, env: EnvRef },
    /// Return a value to the continuation
    Return { val: grift_parser::ArenaIndex },
}

impl GcRoots for TrampolineState {
    fn trace_roots(&self, tracer: &mut dyn FnMut(ArenaIndex)) {
        match self {
            TrampolineState::Eval { expr, env } => {
                tracer(expr.0);
                tracer(env.0);
            }
            TrampolineState::Return { val } => {
                tracer(*val);
            }
        }
    }
}

// ============================================================================
// WriteCursor - Helper for no-alloc string formatting
// ============================================================================

/// Helper for no-alloc string formatting
struct WriteCursor<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> WriteCursor<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.buf[..self.pos]).ok()
    }
}

impl core::fmt::Write for WriteCursor<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        if self.pos + bytes.len() <= self.buf.len() {
            self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
            self.pos += bytes.len();
            Ok(())
        } else {
            Err(core::fmt::Error)
        }
    }
}
pub fn gensym(&mut self, base: &str) -> EvalResult {
    use core::fmt::Write;

    // Build symbol name: #:{base}{counter}
    // Using a fixed buffer to avoid heap allocation
    let mut buf = [0u8; 48];
    let mut cursor = WriteCursor::new(&mut buf);

    let _ = write!(cursor, "#:{}{}", base, self.gensym_counter);
    self.gensym_counter += 1;

    let name = cursor.as_str()
        .ok_or_else(|| self.make_error(ErrorKind::Generic, self.lisp.nil().unwrap()))?;

    // Gensym names are unique by construction (monotonic counter),
    // so skip the intern table lookup which would always miss.
    self.lisp.symbol_new_unique(name).map_err(Into::into)
}
/// Generate a simple gensym with default prefix
pub fn gensym_simple(&mut self) -> EvalResult {
    self.gensym("g")
}

/// A Lisp value
///
/// # Memory Optimization
///
/// This enum inlines fixed-size data directly into variants to save arena slots
/// and improve cache locality. Variable-length data (strings, arrays) still uses
/// arena storage but with inline length for O(1) access.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// The empty list (NOT false - use False for that)
    Nil,

    /// Boolean true (#t)
    True,

    /// Boolean false (#f) - the ONLY false value
    False,

    /// Integer number
    Number(isize),

    /// Inexact floating-point number (R7RS numeric tower)
    ///
    /// Uses `fsize` which is `f64` on 64-bit platforms and `f32` on 32-bit,
    /// matching the width of `isize`/`usize`.
    Float(fsize),


    /// Single character (used in strings and symbol storage)
    Char(char),

    /// Cons cell (pair) with inline car/cdr indices
    ///
    /// Both car and cdr are stored inline, saving 2 arena slots per cons.
    ///
    /// # Memory Savings
    ///
    /// Previously: 1 slot for Cons + 2 slots for [Ref(car), Ref(cdr)] = 3 slots
    /// Now: 1 slot for Cons with inline car/cdr = 1 slot (saves 2 slots)
    ///
    /// Use `Lisp::cons()`, `Lisp::car()`, `Lisp::cdr()` to create and access.
    Cons { car: ArenaIndex, cdr: ArenaIndex },

    /// Symbol (contains a contiguous string)
    /// Points to a Value::String which contains the symbol name.
    /// The length is obtained from the String value, providing a single source of truth.
    Symbol(ArenaIndex),

    /// Lambda / closure with inline params and body_env indices
    ///
    /// - `params`: ArenaIndex to list of parameter symbols
    /// - `body_env`: ArenaIndex to a cons cell containing (body . env)
    ///
    /// # Memory Savings
    ///
    /// 1 slot for Lambda with inline params/body_env + 1 cons for body.env = 2 slots (saves 2 slots)
    ///
    /// Use `Lisp::lambda()` to create and `Lisp::lambda_parts()` to extract.
    Lambda { params: ArenaIndex, body_env: ArenaIndex },

    /// Built-in function (optimized)
    Builtin(Builtin),

    /// Standard library function (stored in static memory)
    ///
    /// Unlike Lambda which stores code in the arena, StdLib references static
    /// function definitions. The function body is parsed on each call.
    ///
    /// # Memory Efficiency
    ///
    /// - Function definitions are in static memory (const strings)
    /// - No arena allocation for the function definition itself
    /// - Parsed AST is temporary and GC'd after evaluation
    StdLib(StdLib),



    /// String with inline length and data pointer
    ///
    /// Strings store characters contiguously in the arena. The length is inlined
    /// for O(1) access, saving 1 arena slot per string.
    ///
    /// # Memory Layout
    ///
    /// - `len`: Number of characters (inline)
    /// - `data`: Points directly to first Char value (no length header in arena)
    /// - Empty strings have len=0 and data == NIL
    ///
    /// # Memory Savings
    ///
    /// Previously: 1 slot for String + Number(len) header + chars
    /// Now: 1 slot for String with inline len + chars only (saves 1 slot)
    ///
    /// # Example
    ///
    /// ```lisp
    /// (string-length "hello")   ; => 5 (O(1) - inline!)
    /// (string-ref "hello" 0)    ; => #\h
    /// ```
    String { len: usize, data: ArenaIndex },

    /// Native function with inline id
    ///
    /// Native functions are registered at runtime and identified by their ID.
    /// The actual function pointer is stored in the evaluator's NativeRegistry.
    Native { id: usize },

    /// Raw arena index reference
    ///
    /// Used internally for storing arena indices in contiguous blocks.
    /// This allows other variants (like Cons) to store their references
    /// in the arena rather than inline, enabling memory optimizations.
    ///
    /// # Note
    ///
    /// This is an internal implementation detail and should not be
    /// exposed to Lisp code directly. It's traced by the GC like any
    /// other reference.
    Ref(ArenaIndex),




    /// First-class environment object (R7RS §6.12)
    ///
    /// Created by `environment` or `interaction-environment`.
    /// - `env`: ArenaIndex to the environment bindings chain
    /// - `mutable`: whether new bindings can be added (`true` for
    ///   interaction-environment, `false` for `environment`)
    Environment { env: ArenaIndex, mutable: bool },
}

/// Allocate a character
pub fn char(&self, c: char) -> ArenaResult<ArenaIndex> {
    self.alloc(Value::Char(c))
}

/// Allocate a cons cell
///
/// Creates a cons cell with inline car and cdr indices.
/// No arena data slots are needed - the indices are stored directly in the Value.
pub fn cons(&self, car: ArenaIndex, cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
    self.alloc(Value::Cons { car, cdr })
}

/// Get car of a cons cell
///
/// In Scheme R7RS, car of an empty list is an error.
/// O(1) access - car is stored inline.
pub fn car(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
    match self.arena.get(index)? {
        Value::Cons { car, .. } => Ok(car),
        Value::Nil => Err(ArenaError::InvalidIndex),
        _ => Err(ArenaError::InvalidIndex),
    }
}

/// Get cdr of a cons cell
///
/// In Scheme R7RS, cdr of an empty list is an error.
/// O(1) access - cdr is stored inline.
pub fn cdr(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
    match self.arena.get(index)? {
        Value::Cons { cdr, .. } => Ok(cdr),
        Value::Nil => Err(ArenaError::InvalidIndex),
        _ => Err(ArenaError::InvalidIndex),
    }
}

/// Get both car and cdr of a cons cell in one operation
///
/// More efficient than calling car() and cdr() separately when both are needed.
/// O(1) access - both are stored inline.
pub fn car_cdr(&self, index: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex)> {
    match self.arena.get(index)? {
        Value::Cons { car, cdr } => Ok((car, cdr)),
        Value::Nil => Err(ArenaError::InvalidIndex),
        _ => Err(ArenaError::InvalidIndex),
    }
}

/// Set car of a cons cell (mutation operation)
/// Returns the new value on success
pub fn set_car(&self, index: ArenaIndex, new_car: ArenaIndex) -> ArenaResult<ArenaIndex> {
    match self.get(index)? {
        Value::Cons { cdr, .. } => {
            self.arena.set(index, Value::Cons { car: new_car, cdr })?;
            Ok(new_car)
        }
        _ => Err(ArenaError::InvalidIndex),
    }
}

/// Set cdr of a cons cell (mutation operation)
/// Returns the new value on success
pub fn set_cdr(&self, index: ArenaIndex, new_cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
    match self.get(index)? {
        Value::Cons { car, .. } => {
            self.arena.set(index, Value::Cons { car, cdr: new_cdr })?;
            Ok(new_cdr)
        }
        _ => Err(ArenaError::InvalidIndex),
    }
}

/// Free a string and all its character slots.
///
/// # Errors
///
/// Returns an error if the string index is invalid.
pub fn string_free(&self, str_idx: ArenaIndex) -> ArenaResult<()> {
    match self.arena.get(str_idx)? {
        Value::String { len, data } => {
            // Free the data slots (characters only, no header)
            if len > 0 && !data.is_nil() {
                self.arena.free_contiguous(data, len)?;
            }
            // Free the String value itself
            self.arena.free(str_idx)
        }
        _ => Err(ArenaError::InvalidIndex),
    }
}
