#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # Grift – A Minimalistic Lisp
//!
//! A `no_std`, `no_alloc` Lisp interpreter built on top of [`grift_arena`],
//! implementing Kernel-style vau calculus (fexprs).
//!
//! ## Features
//!
//! - **No-std, no-alloc**: Works in embedded environments with no heap.
//!   Only `core::` types are used; the crate compiles for bare-metal targets.
//! - **Arena-allocated**: All values live in a fixed-size [`Arena`](grift_arena::Arena)
//!   with const-generic capacity. No `Vec`, `String`, or `Box`.
//! - **Simple API**: Parse and evaluate Lisp expressions in one call via [`Lisp::eval`].
//! - **Tail-call optimization**: Unbounded recursion in tail position without
//!   growing the Rust call stack, implemented via a trampoline loop.
//! - **Mark-and-sweep GC**: Automatic garbage collection triggered on OOM,
//!   with explicit collection available via `(gc-collect)`.
//! - **No unsafe code**: `#![forbid(unsafe_code)]` is enforced crate-wide.
//!
//! ## Architecture
//!
//! The interpreter is split into four internal modules:
//!
//! - [`value`] — The [`Value`] enum (12 variants) representing all Lisp types.
//! - `lisp` — The [`Lisp`] struct: arena wrapper, symbol interning, environments.
//! - `parse` — Recursive-descent S-expression parser.
//! - `eval` — Evaluator with TCO trampoline, builtin dispatch, and GC integration.
//!
//! ## Example
//!
//! ```rust
//! use grift::{Lisp, Value};
//!
//! let lisp: Lisp<20000> = Lisp::new();
//! let three = lisp.eval("(+ 1 2)");
//! assert_eq!(three, Ok(Value::Number(3)));
//! ```

mod value;
mod lisp;
mod parse;
mod eval;
pub mod stdlib;

pub use value::{Value, BuiltinId};
pub use lisp::Lisp;
pub use grift_arena::{ArenaIndex, ArenaError, ArenaResult, ArenaStats, GcStats};
pub use stdlib::{StdLib, StdLibEntry};
