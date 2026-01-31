#![no_std]
#![forbid(unsafe_code)]

//! # Lisp Evaluator
//!
//! A Lisp evaluator with **fully trampolined evaluation** - no Rust stack
//! recursion, enabling unlimited recursion depth (bounded only by heap/arena size).
//!
//! ## Key Features
//!
//! - **Full Trampolining**: All evaluation uses continuation-passing style with
//!   an explicit continuation stack. No Rust recursion means no stack overflow.
//! - **Strict Evaluation**: Call-by-value semantics - all arguments are evaluated before function application
//! - **Proper TCO**: Tail calls reuse the same continuation frame
//! - **Lexically Scoped Closures**: First-class functions with captured environments
//! - **Rich Error Handling**: Error messages with stack traces
//! - **Pattern Matching**: `case` for value matching
//! - **Iteration**: `do` loops for imperative-style iteration
//! - **Meta-programming**: `eval` for runtime code evaluation, `quasiquote`/`unquote`
//! - **Mutation**: `set!`, `set-car!`, `set-cdr!` for imperative programming
//!
//! ## Evaluation Strategy
//!
//! This evaluator uses strict (call-by-value) evaluation:
//! - All function arguments are fully evaluated before the function is called
//! - This provides predictable evaluation order and side-effect timing
//! - Tail call optimization is supported for constant-space recursion
//!
//! ## Mutation
//!
//! This Lisp supports mutation operations for imperative programming:
//! - `set!` - Mutate a variable binding
//! - `set-car!` - Mutate the car of a cons cell
//! - `set-cdr!` - Mutate the cdr of a cons cell
//!
//! Note: Mutation breaks referential transparency but enables imperative patterns.
//!
//! ## Truthiness
//!
//! Only `#f` is false. Everything else (including the empty list `'()`) is truthy.
//!
//! ## Special Forms
//!
//! - `quote` - Return expression unevaluated
//! - `if` - Conditional
//! - `cond` - Multi-way conditional
//! - `case` - Pattern matching on values
//! - `lambda` - Create closure
//! - `define` - Define variable/function
//! - `set!` - Mutate variable binding
//! - `let` - Local binding
//! - `let*` - Sequential local binding
//! - `begin` - Sequence of expressions
//! - `and` / `or` - Short-circuit boolean operations
//! - `do` - Iteration with initialization and step expressions
//! - `quasiquote` - Template with `unquote` and `unquote-splicing`
//! - `eval` - Evaluate expression at runtime
//! - `apply` - Apply function to argument list
//! - `values` - Return multiple values as a list

pub use grift_parser::{
    Arena, ArenaIndex, ArenaError, ArenaResult, Trace, GcStats,
    Value, Builtin, StdLib, Lisp, ParseError, ParseErrorKind, SourceLoc, parse,
};

// Native function interop
pub mod native;
pub use native::{
    FromLisp, ToLisp, NativeRegistry, NativeEntry, NativeFn,
    extract_arg, args_empty, count_args, simple_hash, MAX_NATIVE_FUNCTIONS,
};

// Internal modules
mod error;
mod continuation;
mod helpers;
mod evaluator;

// Public re-exports
pub use error::{ErrorKind, ErrorMessage, StackFrame, EvalError, EvalResult};
pub use evaluator::Evaluator;
pub use continuation::{Cont, TrampolineState};

// ============================================================================
// Helper Macros for Code Deduplication
// ============================================================================

/// Macro to extract arguments from a Lisp list using car/cdr.
///
/// This macro extracts multiple arguments from a list, shadowing the `args`
/// variable after each extraction.
///
/// # Example
/// 
/// `extract_args!(self, args, a, b, c)` extracts three arguments from `args`.
#[macro_export]
macro_rules! extract_args {
    ($self:expr, $args:ident, $var:ident) => {
        let $var = $self.lisp.car($args)?;
    };
    ($self:expr, $args:ident, $var:ident, $($rest:ident),+) => {
        let $var = $self.lisp.car($args)?;
        #[allow(unused_variables)]
        let $args = $self.lisp.cdr($args)?;
        $crate::extract_args!($self, $args, $($rest),+)
    };
}

/// Macro for unary predicate builtins.
///
/// Many builtins follow the pattern of extracting one argument and returning
/// a boolean based on some predicate on the value.
///
/// # Example
/// 
/// `builtin_unary_pred!(self, args, |v| v.is_nil())` extracts one argument
/// and returns a boolean result of the predicate.
#[macro_export]
macro_rules! builtin_unary_pred {
    ($self:expr, $args:expr, $check:expr) => {{
        let arg = $self.lisp.car($args)?;
        let val = $self.lisp.get(arg)?;
        $self.lisp.boolean($check(val)).map_err(Into::into)
    }};
}

/// Macro for numeric predicate builtins.
///
/// Extracts one integer argument and returns a boolean based on the predicate.
///
/// # Example
/// 
/// `builtin_numeric_pred!(self, args, call_expr, |n| n == 0)` extracts one integer
/// and returns whether it equals zero.
#[macro_export]
macro_rules! builtin_numeric_pred {
    ($self:expr, $args:expr, $call_expr:expr, $check:expr) => {{
        let n = $self.get_int($self.lisp.car($args)?, $call_expr)?;
        $self.lisp.boolean($check(n)).map_err(Into::into)
    }};
}

/// Macro for integer identity operations (rounding on integers).
///
/// For integers, floor/ceiling/truncate/round are all identity operations.
///
/// # Example
/// 
/// `builtin_int_identity!(self, args, call_expr)` extracts one integer and returns it.
#[macro_export]
macro_rules! builtin_int_identity {
    ($self:expr, $args:expr, $call_expr:expr) => {{
        let n = $self.get_int($self.lisp.car($args)?, $call_expr)?;
        $self.lisp.number(n).map_err(Into::into)
    }};
}

/// Macro for binary integer operations with division-by-zero check.
///
/// Extracts two integer arguments, checks for division by zero, and applies the operation.
///
/// # Example
/// 
/// `builtin_div_op!(self, args, call_expr, |a, b| a / b)` performs integer division.
#[macro_export]
macro_rules! builtin_div_op {
    ($self:expr, $args:expr, $call_expr:expr, $op:expr) => {{
        let a = $self.get_int($self.lisp.car($args)?, $call_expr)?;
        let b = $self.get_int($self.lisp.car($self.lisp.cdr($args)?)?, $call_expr)?;
        if b == 0 {
            return Err($self.make_error($crate::ErrorKind::DivisionByZero, $call_expr));
        }
        $self.lisp.number($op(a, b)).map_err(Into::into)
    }};
}

/// Macro for binary character comparison operations.
///
/// Extracts two character arguments and compares them.
///
/// # Example
/// 
/// `builtin_char_cmp!(self, args, call_expr, |a, b| a == b)` compares two characters.
#[macro_export]
macro_rules! builtin_char_cmp {
    ($self:expr, $args:expr, $call_expr:expr, $cmp:expr) => {{
        let a = $self.get_char($self.lisp.car($args)?, $call_expr)?;
        let b = $self.get_char($self.lisp.car($self.lisp.cdr($args)?)?, $call_expr)?;
        $self.lisp.boolean($cmp(a, b)).map_err(Into::into)
    }};
}
