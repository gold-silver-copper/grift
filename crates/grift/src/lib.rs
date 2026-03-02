#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(
    clippy::must_use_candidate,
    clippy::doc_markdown,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::single_match_else,
    clippy::module_name_repetitions,
    clippy::inline_always,
    clippy::needless_pass_by_value,
    clippy::wildcard_imports,
    clippy::unused_self,
    clippy::manual_let_else,
    clippy::needless_continue,
    clippy::unnecessary_wraps,
    clippy::doc_link_with_quotes,
    clippy::cast_possible_wrap
)]

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

mod eval;
mod lisp;
/// Native function registration support.
pub mod native;
mod parse;
/// Prelude types and generated entries.
pub mod prelude;
mod value;

pub use grift_arena::{ArenaError, ArenaIndex, ArenaResult, ArenaStats, GcStats};
pub use lisp::Lisp;
pub use native::{FromLisp, LispOps, NativeFn, ToLisp, extract_arg};
pub use prelude::{Prelude, PreludeEntry};
pub use value::{BuiltinId, Value};
