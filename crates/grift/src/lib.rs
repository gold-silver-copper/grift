#![no_std]
#![forbid(unsafe_code)]

//! # Grift – A Minimalistic Lisp
//!
//! A simple Lisp built on top of [`grift_arena`], the arena allocator.
//!
//! ## Features
//!
//! - **No-std, no-alloc**: Works in embedded environments with no heap
//! - **Arena-allocated**: All values live in a fixed-size arena
//! - **Simple API**: Parse and evaluate Lisp expressions in one call
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
pub mod io;

pub use value::{Value, BuiltinId};
pub use lisp::Lisp;
pub use grift_arena::{ArenaIndex, ArenaError, ArenaResult, ArenaStats, GcStats};
pub use io::{IoProvider, NullIoProvider, PortId, IoErrorKind, IoResult};

#[cfg(feature = "std")]
pub use io::StdIoProvider;
