//! Error handling types for evaluation.
//!
//! # Design Goals
//!
//! This module is optimized for `no_std` environments with minimal stack usage.
//! The `EvalError` struct is kept small (~88 bytes) to avoid bloating every
//! `Result<ArenaIndex, EvalError>` on the stack.
//!
//! ## Key Optimizations
//!
//! 1. **No inline backtrace**: The evaluator maintains its own call stack. Errors
//!    store only the stack depth at error time; display code can access the
//!    evaluator's stack directly.
//!
//! 2. **Static message strings**: Messages are `&'static str` instead of inline
//!    buffers. Most error messages are compile-time constants anyway.
//!
//! 3. **Packed detail fields**: Type and argument count info use compact
//!    representations with u16/u8 instead of usize.

use grift_parser::{ArenaIndex, ArenaError, ParseError};

/// Maximum call stack depth for traces
pub const MAX_STACK_DEPTH: usize = 64;

/// Error kind enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ErrorKind {
    /// Arena is full
    OutOfMemory = 0,
    /// Unbound variable
    UnboundVariable = 1,
    /// Not a function
    NotAFunction = 2,
    /// Wrong number of arguments
    WrongArgCount = 3,
    /// Type error
    TypeError = 4,
    /// Division by zero
    DivisionByZero = 5,
    /// Parse error
    Parse = 6,
    /// User-raised error
    UserError = 7,
    /// Stack overflow (recursion too deep)
    StackOverflow = 8,
    /// Cannot mutate non-pair
    NotAPair = 9,
    /// Generic error
    Generic = 10,
    /// Syntax/macro expansion error
    SyntaxError = 11,
}

impl ErrorKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            ErrorKind::OutOfMemory => "out of memory",
            ErrorKind::UnboundVariable => "unbound variable",
            ErrorKind::NotAFunction => "not a function",
            ErrorKind::WrongArgCount => "wrong number of arguments",
            ErrorKind::TypeError => "type error",
            ErrorKind::DivisionByZero => "division by zero",
            ErrorKind::Parse => "parse error",
            ErrorKind::UserError => "error",
            ErrorKind::StackOverflow => "stack overflow",
            ErrorKind::NotAPair => "not a pair",
            ErrorKind::Generic => "error",
            ErrorKind::SyntaxError => "syntax error",
        }
    }
}

/// A stack frame for error reporting
#[derive(Clone, Copy, Debug)]
pub struct StackFrame {
    /// The expression being evaluated (for display)
    pub expr: ArenaIndex,
    /// Function being called (if applicable)
    pub func: ArenaIndex,
}

impl Default for StackFrame {
    fn default() -> Self {
        StackFrame {
            expr: ArenaIndex::NIL,
            func: ArenaIndex::NIL,
        }
    }
}

/// Compact argument count info (expected, got) packed into u32.
/// Uses u16 for each count, supporting up to 65535 arguments.
#[derive(Debug, Clone, Copy, Default)]
pub struct ArgCountInfo {
    /// Expected argument count (u16)
    pub expected: u16,
    /// Got argument count (u16)
    pub got: u16,
}

impl ArgCountInfo {
    pub const fn new(expected: usize, got: usize) -> Self {
        ArgCountInfo {
            expected: expected as u16,
            got: got as u16,
        }
    }
}

/// Evaluation error with context - optimized for minimal size.
///
/// # Size Optimization
///
/// This struct is kept small (~88 bytes on 64-bit) to minimize stack usage:
/// - No inline backtrace array (saves ~256 bytes)
/// - Static string messages instead of inline buffers (saves ~56 bytes)
/// - Compact arg count representation (saves ~24 bytes)
#[derive(Debug)]
pub struct EvalError {
    /// What kind of error (1 byte + padding)
    pub kind: ErrorKind,
    /// The expression that caused the error
    pub expr: ArenaIndex,
    /// Human-readable message context (static string, 16 bytes)
    pub message: &'static str,
    /// Expected and got types for type errors (2 × 16 bytes)
    pub expected: Option<&'static str>,
    pub got: Option<&'static str>,
    /// Argument count info for WrongArgCount errors (4 bytes)
    pub arg_info: Option<ArgCountInfo>,
    /// Original parse error (if applicable)
    pub parse_error: Option<ParseError>,
}

impl EvalError {
    pub const fn new(kind: ErrorKind) -> Self {
        EvalError {
            kind,
            expr: ArenaIndex::NIL,
            message: "",
            expected: None,
            got: None,
            arg_info: None,
            parse_error: None,
        }
    }
    
    pub const fn with_expr(mut self, expr: ArenaIndex) -> Self {
        self.expr = expr;
        self
    }
    
    pub const fn with_message(mut self, msg: &'static str) -> Self {
        self.message = msg;
        self
    }
    
    pub const fn with_types(mut self, expected: &'static str, got: &'static str) -> Self {
        self.expected = Some(expected);
        self.got = Some(got);
        self
    }
    
    pub const fn with_args(mut self, expected: usize, got: usize) -> Self {
        self.arg_info = Some(ArgCountInfo::new(expected, got));
        self
    }
    
    /// Create an EvalError from a ParseError with expression context
    pub fn from_parse_error(e: ParseError, expr: ArenaIndex) -> Self {
        let mut err = EvalError::new(ErrorKind::Parse);
        err.parse_error = Some(e);
        err.expr = expr;
        err
    }
}

impl From<ArenaError> for EvalError {
    fn from(e: ArenaError) -> Self {
        match e {
            ArenaError::OutOfMemory => EvalError::new(ErrorKind::OutOfMemory),
            ArenaError::InvalidIndex => EvalError::new(ErrorKind::Generic)
                .with_message("arena invalid index - possible use of freed/invalid reference"),
            ArenaError::TraceError => EvalError::new(ErrorKind::Generic)
                .with_message("arena trace error during GC"),
        }
    }
}

impl From<ParseError> for EvalError {
    fn from(e: ParseError) -> Self {
        let mut err = EvalError::new(ErrorKind::Parse);
        err.parse_error = Some(e);
        err
    }
}

/// Result type for evaluation
pub type EvalResult = Result<ArenaIndex, EvalError>;
