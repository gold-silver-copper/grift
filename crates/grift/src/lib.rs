//! # Grift
//!
//! A minimal `no_std`, `no_alloc` Scheme implementation built on arena-based
//! garbage collection. Perfect for embedded systems, WebAssembly, or any
//! environment where heap allocation is unavailable or undesirable.
//!
//! ## Features
//!
//! - **`no_std`, `no_alloc` by default** — Runs on bare metal
//! - **Arena-based allocation** — Fixed memory footprint, no heap
//! - **Mark-and-sweep GC** — Controllable from Scheme code
//! - **Proper tail-call optimization** — Via trampolining
//! - **Lexical closures** — First-class functions with captured environments
//! - **R7RS-inspired** — Scheme semantics with only `#f` as false
//!
//! ## Quick Start
//!
//! ```rust
//! use grift::{Lisp, Evaluator, Value};
//!
//! // Create a Lisp interpreter with a 20,000-cell arena
//! let lisp: Lisp<20000> = Lisp::new();
//! let mut eval = Evaluator::new(&lisp).unwrap();
//!
//! // Evaluate expressions
//! let result = eval.eval_str("(+ 1 2 3)").unwrap();
//! assert!(matches!(lisp.get(result), Ok(Value::Number(6))));
//! ```
//!
//! ## Optional REPL
//!
//! Enable the `std` feature for an interactive REPL:
//!
//! ```toml
//! [dependencies]
//! grift = { version = "1.2", features = ["std"] }
//! ```
//!
//! Then run:
//!
//! ```bash
//! cargo install grift --features std
//! grift
//! ```
//!
//! ## Crate Organization
//!
//! This crate re-exports the complete Grift stack:
//!
//! - [`grift_arena`] — Arena allocator with GC
//! - [`grift_core`] — Core types (`Value`, `Builtin`, `StdLib`, `Lisp`)
//! - [`grift_parser`] — Lexer and parser
//! - [`grift_eval`] — Trampolined evaluator
//! - [`grift_repl`] — Interactive REPL (behind `std` feature)

#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// ============================================================================
// Core Re-exports from grift_arena
// ============================================================================

/// Arena allocator and garbage collection primitives.
pub mod arena {
    pub use grift_arena::{
        Arena, ArenaIndex, ArenaError, ArenaResult,
        GcStats, Trace,
    };
}

pub use arena::{Arena, ArenaIndex, ArenaError, ArenaResult, GcStats, Trace};

// ============================================================================
// Core Types Re-exports from grift_core
// ============================================================================

/// Core value types and Lisp context.
pub mod core_types {
    pub use grift_core::{
        Value, Builtin, StdLib, Lisp, RESERVED_SLOTS,
        define_builtins, define_stdlib,
    };
}

// ============================================================================
// Parser Re-exports from grift_parser
// ============================================================================

/// Parser, value types, and built-in definitions.
pub mod parser {
    pub use grift_parser::{
        // Core types (re-exported from grift_core)
        Value, Builtin, StdLib, Lisp,
        // Parsing
        parse, parse_all, Parser, ParseError, ParseErrorKind, SourceLoc,
        // Macros
        define_builtins, define_stdlib,
    };
}

pub use parser::{
    Value, Builtin, StdLib, Lisp,
    parse, parse_all, Parser, ParseError, ParseErrorKind, SourceLoc,
    define_builtins, define_stdlib,
};

// ============================================================================
// Evaluator Re-exports from grift_eval
// ============================================================================

/// Evaluator, error handling, and native function interop.
pub mod eval {
    pub use grift_eval::{
        // Evaluator
        Evaluator, EvalError, EvalResult, ErrorKind, StackFrame,
        // Native FFI
        FromLisp, ToLisp, NativeRegistry, NativeEntry, NativeFn,
        extract_arg, args_empty, count_args, simple_hash, MAX_NATIVE_FUNCTIONS,
    };
}

pub use eval::{
    Evaluator, EvalError, EvalResult, ErrorKind, StackFrame,
    FromLisp, ToLisp, NativeRegistry, NativeEntry, NativeFn,
    extract_arg, args_empty, count_args, simple_hash, MAX_NATIVE_FUNCTIONS,
};

// ============================================================================
// REPL Re-exports (std feature only)
// ============================================================================

/// REPL and formatting utilities (requires `std` feature).
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod repl {
    pub use grift_repl::{
        run_repl, Repl,
        format_value, format_error, value_to_string, eval_to_string,
    };
}

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub use repl::{
    run_repl, Repl,
    format_value, format_error, value_to_string, eval_to_string,
};
