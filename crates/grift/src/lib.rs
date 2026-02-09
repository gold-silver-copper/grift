//! # Grift
//!
//! A minimal `no_std`, `no_alloc` R7RS-compliant Scheme implementation built on
//! arena-based garbage collection. Perfect for embedded systems, WebAssembly, or any
//! environment where heap allocation is unavailable or undesirable.
//!
//! ## Features
//!
//! - **`no_std`, `no_alloc` by default** — Runs on bare metal
//! - **Arena-based allocation** — Fixed memory footprint, no heap
//! - **Mark-and-sweep GC** — Controllable from Scheme code
//! - **Proper tail-call optimization** — Via trampolining
//! - **Lexical closures** — First-class functions with captured environments
//! - **R7RS-compliant** — Scheme semantics following the Revised⁷ Report
//! - **Pluggable I/O** — Trait-based I/O boundary (`IoProvider`) keeps the
//!   evaluator `no_std` while allowing real I/O on hosted platforms
//! - **Native FFI** — Register Rust functions callable from Scheme
//! - **Embedded support** — Optional hardware natives for GPIO, memory
//!   peek/poke, and bit manipulation
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
//! ## Optional `std` Feature
//!
//! Enable the `std` feature for an interactive REPL and real I/O:
//!
//! ```toml
//! [dependencies]
//! grift = { version = "1.3", features = ["std"] }
//! ```
//!
//! The `std` feature brings in:
//! - [`grift_repl`] — Interactive REPL with line editing, GC commands, and help
//! - [`grift_std`] — `StdIoProvider`, an [`IoProvider`] implementation using
//!   `std::io` for stdin/stdout/stderr and dynamic string ports
//!
//! Then run:
//!
//! ```bash
//! cargo install grift --features std
//! grift
//! ```
//!
//! ## I/O Architecture
//!
//! The evaluator is fully `no_std` and performs no I/O itself. Instead, an
//! [`IoProvider`] trait (defined in `grift_core`) abstracts all port
//! operations:
//!
//! - **[`NullIoProvider`]** — No-op implementation that silently discards
//!   output and rejects reads. Used in `no_std`/embedded contexts where
//!   there is no real I/O.
//! - **`StdIoProvider`** (behind `std` feature) — Full implementation
//!   backed by `std::io`, providing stdin/stdout/stderr and dynamic string
//!   ports.
//!
//! Pass any `IoProvider` to the evaluator at runtime:
//!
//! ```rust,ignore
//! use grift::{Lisp, Evaluator, NullIoProvider};
//!
//! let lisp: Lisp<20000> = Lisp::new();
//! let mut eval = Evaluator::new(&lisp).unwrap();
//! let mut io = NullIoProvider;
//! eval.set_io_provider(&mut io);
//! ```
//!
//! ## Crate Organization
//!
//! This crate re-exports the complete Grift stack:
//!
//! - [`grift_arena`] — Fixed-size arena allocator with mark-and-sweep GC
//! - [`grift_core`] — Core types (`Value`, `Builtin`, `StdLib`, `Lisp`)
//!   and the `IoProvider` trait boundary
//! - [`grift_parser`] — Lexer and parser
//! - [`grift_eval`] — Fully-trampolined evaluator with native FFI
//! - [`grift_repl`] — Interactive REPL (behind `std` feature)
//! - [`grift_std`] — `StdIoProvider` for hosted I/O (behind `std` feature)
//! - [`grift_macros`] — Procedural macros (`include_stdlib!`)
//! - [`grift_util`] — Shared utilities (Scheme-name ↔ Rust-name conversion)
//! - [`grift_arena_embedded`] — Embedded hardware natives (GPIO, peek/poke,
//!   bit manipulation)

#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

/// Arena allocator and garbage collection primitives.
pub mod arena {
    pub use grift_arena::{
        Arena, ArenaIndex, ArenaError, ArenaResult,
        GcStats, Trace,
    };
}

pub use arena::{Arena, ArenaIndex, ArenaError, ArenaResult, GcStats, Trace};

/// Core value types and Lisp context.
pub mod core_types {
    pub use grift_core::{
        Value, Builtin, StdLib, Lisp, RESERVED_SLOTS,
        DisplayValue,
        define_builtins, define_stdlib,
        IoProvider, NullIoProvider, PortId, IoErrorKind, IoResult, DisplayPort,
    };
}

/// Parser, lexer, value types, and built-in definitions.
pub mod parser {
    pub use grift_parser::{
        Value, Builtin, StdLib, Lisp, DisplayValue,
        Lexer, Token, SpannedToken, LexError, LexErrorKind,
        parse, parse_all, Parser, ParseError, ParseErrorKind, SourceLoc,
        define_builtins, define_stdlib,
    };
}

pub use parser::{
    Value, Builtin, StdLib, Lisp, DisplayValue,
    Lexer, Token, SpannedToken, LexError, LexErrorKind,
    parse, parse_all, Parser, ParseError, ParseErrorKind, SourceLoc,
    define_builtins, define_stdlib,
};

/// Evaluator, error handling, and native function interop.
pub mod eval {
    pub use grift_eval::{
        Evaluator, EvalError, EvalResult, ErrorKind, StackFrame,
        FromLisp, ToLisp, NativeRegistry, NativeEntry, NativeFn,
        extract_arg, args_empty, count_args, MAX_NATIVE_FUNCTIONS,
    };
}

pub use eval::{
    Evaluator, EvalError, EvalResult, ErrorKind, StackFrame,
    FromLisp, ToLisp, NativeRegistry, NativeEntry, NativeFn,
    extract_arg, args_empty, count_args, MAX_NATIVE_FUNCTIONS,
};

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
pub use repl::{
    run_repl, Repl,
    format_value, format_error, value_to_string, eval_to_string,
};

pub use grift_core::{
    IoProvider, NullIoProvider, PortId, IoErrorKind, IoResult, DisplayPort,
};

/// Standard library I/O provider (requires `std` feature).
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod std_io {
    pub use grift_std::StdIoProvider;
}

#[cfg(feature = "std")]
pub use std_io::StdIoProvider;
