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
//! - **Pluggable storage** — Fixed-size arrays (no-std) or Vec (std feature)
//!
//! ## Quick Start
//!
//! ```rust
//! use grift::{Lisp, Evaluator, Value};
//!
//! // Create a Lisp interpreter with a 10,000-cell arena
//! let lisp: Lisp<10000> = Lisp::new();
//! let mut eval = Evaluator::new(&lisp).unwrap();
//!
//! // Evaluate expressions
//! let result = eval.eval_str("(+ 1 2 3)").unwrap();
//! assert!(matches!(lisp.get(result), Ok(Value::Number(6))));
//! ```
//!
//! ## Vec-backed Storage (std feature)
//!
//! With the `std` feature enabled, you can use `VecStorage` for dynamic
//! capacity at runtime. This is useful when you don't know the arena size
//! at compile time:
//!
//! ```rust,ignore
//! use grift::arena::{GenericArena, VecStorage};
//!
//! // Create storage with runtime-determined capacity
//! let storage = VecStorage::<isize>::with_capacity(50000);
//! let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);
//!
//! let idx = arena.alloc(42).unwrap();
//! assert_eq!(arena.get(idx).unwrap(), 42);
//! ```
//!
//! ## Optional REPL
//!
//! Enable the `std` feature for an interactive REPL:
//!
//! ```toml
//! [dependencies]
//! grift = { version = "0.1", features = ["std"] }
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
//! - [`grift_parser`] — Lexer, parser, and value types
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
        // Generic arena types
        GenericArena, GenericArenaIterator, ArenaStorage, ArrayStorage,
    };
    
    // Vec-backed storage (requires std feature)
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub use grift_arena::VecStorage;
}

pub use arena::{Arena, ArenaIndex, ArenaError, ArenaResult, GcStats, Trace};
pub use arena::{GenericArena, GenericArenaIterator, ArenaStorage, ArrayStorage};
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub use arena::VecStorage;

// ============================================================================
// Parser Re-exports from grift_parser
// ============================================================================

/// Parser, value types, and built-in definitions.
pub mod parser {
    pub use grift_parser::{
        // Core types
        Value, Builtin, StdLib, Lisp,
        // Parsing
        parse, ParseError, ParseErrorKind, SourceLoc,
        // Macros
        define_builtins, define_stdlib,
    };
}

pub use parser::{
    Value, Builtin, StdLib, Lisp,
    parse, ParseError, ParseErrorKind, SourceLoc,
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
