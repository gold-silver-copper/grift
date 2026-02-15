#![no_std]
#![forbid(unsafe_code)]
#![allow(non_camel_case_types)]

//! # Grift Core Types
//!
//! Core types and Lisp context for the Grift R7RS-compliant Scheme implementation.
//!
//! This crate contains the fundamental types shared between the parser and evaluator:
//!
//! - [`Value`] — The core value enum representing all Lisp types
//! - [`Builtin`] — Enum of built-in functions
//! - [`StdLib`] — Standard library function wrapper (static string data)
//! - [`Lisp`] — The Lisp execution context wrapping an arena
//!
//! ## Design
//!
//! - Symbols use contiguous string storage for memory efficiency
//! - Symbol interning ensures the same symbol name returns the same index
//! - All values are stored in a `grift_arena` arena
//! - Supports garbage collection via the `Trace` trait
//! - Explicit boolean values (#t, #f) separate from nil/empty list
//! - **Strict evaluation** - All arguments are evaluated before function application (call-by-value)
//! - **Pluggable I/O** - The [`io`] module defines the [`IoProvider`] trait so the
//!   evaluator stays `no_std` while real I/O can be supplied on hosted platforms
//!   (see `grift_std::StdIoProvider`). A [`NullIoProvider`] is provided for
//!   environments without I/O.
//!
//! ## Value Representation
//!
//! - `Nil` - The empty list (NOT false!)
//! - `True` - Boolean true (#t)
//! - `False` - Boolean false (#f)
//! - `Number(isize)` - Integer numbers
//! - `Char(char)` - Single character
//! - `Cons { car, cdr }` - Pair/list cell with inline indices
//! - `Symbol(ArenaIndex)` - Symbol pointing to interned string
//! - `Lambda { params, body_env }` - Closure with inline indices
//! - `Builtin(Builtin)` - Optimized built-in function
//! - `StdLib(StdLib)` - Standard library function (static code, parsed on-demand)
//! - `Array { len, data }` - Vector with inline length
//! - `String { len, data }` - String with inline length
//! - `Native { id }` - Native Rust function reference
//!
//! ## Reserved Slots
//!
//! The Lisp singleton values (nil, true, false) are pre-allocated in reserved
//! slots at initialization time.
//!
//! The first 4 slots of the arena are reserved:
//! - Slot 0: `Value::Nil` - empty list singleton
//! - Slot 1: `Value::True` - boolean true singleton
//! - Slot 2: `Value::False` - boolean false singleton
//! - Slot 3: `Value::Cons` - intern table reference cell
//!
//! ## Pitfalls and Gotchas
//!
//! ### Truthiness
//! - **Only `#f` is false!** Everything else is truthy, including:
//!   - `nil` / `'()` (the empty list)
//!   - `0` (the number zero)
//!   - Empty strings
//!
//! ### Garbage Collection
//! - The intern table is always a GC root - interned symbols are never collected
//! - Reserved slots (nil, true, false) are implicitly preserved
//! - Run `gc()` with appropriate roots to reclaim memory
//!
//! ### StdLib Functions
//! - Body is parsed on each call (minor overhead, but keeps code out of arena)
//! - Recursive stdlib functions work via the global environment
//! - Errors in static source strings are only caught at runtime

pub use grift_arena::{Arena, ArenaError, ArenaIndex, ArenaResult, GcStats, Trace};

/// Number of bits per limb (depends on pointer width).
///
/// BigNum values are stored as arrays of `usize` limbs in base 2^LIMB_BITS.
pub const LIMB_BITS: u32 = core::mem::size_of::<usize>() as u32 * 8;

/// Maximum number of limbs supported (128 limbs = 4096 bits on 32-bit, 8192 bits on 64-bit).
pub const MAX_LIMBS: usize = 128;

/// Platform-dependent floating-point type, matching the width of `isize`/`usize`.
///
/// On 64-bit platforms this is `f64`; on 32-bit platforms it is `f32`.
/// This provides a consistent numeric tower where the float type has the same
/// bit-width as the native integer types.
#[cfg(target_pointer_width = "64")]
pub type fsize = f64;

/// Platform-dependent floating-point type (32-bit variant).
#[cfg(target_pointer_width = "32")]
pub type fsize = f32;

/// Platform-dependent floating-point type (32-bit variant).
#[cfg(target_pointer_width = "16")]
pub type fsize = f16;

// Macros module (must be declared before other modules that use the macros)
#[macro_use]
mod macros;

mod cont_type;
mod display;
pub mod io;
mod lisp;
mod value;

pub use cont_type::ContType;
pub use value::{Builtin, StdLib, StdLibEntry, Value};
// Note: define_builtins macro is exported at crate root via #[macro_export]

pub use display::DisplayValue;
pub use io::{
    DisplayPort, FileOpenMode, IoErrorKind, IoProvider, IoResult, NullIoProvider, PortId,
};
pub use lisp::{Lisp, RESERVED_SLOTS};
