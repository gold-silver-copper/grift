#![no_std]
#![forbid(unsafe_code)]

//! # Lisp Parser
//!
//! A classic Lisp parser with arena-allocated values.
//!
//! ## Design
//!
//! - Symbols use contiguous string storage for memory efficiency
//! - Symbol interning ensures the same symbol name returns the same index
//! - All values are stored in a `grift_arena` arena
//! - Supports garbage collection via the `Trace` trait
//! - Explicit boolean values (#t, #f) separate from nil/empty list
//! - **Strict evaluation** - All arguments are evaluated before function application (call-by-value)
//!
//! ## Core Types
//!
//! Core types (`Value`, `Builtin`, `StdLib`, `Lisp`) are defined in the
//! [`grift_core`] crate and re-exported here for backward compatibility.
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
//! - `Native { id, name_hash }` - Native Rust function reference
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

// Re-export everything from grift_core for backward compatibility
pub use grift_core::{
    Arena, ArenaIndex, ArenaError, ArenaResult, Trace, GcStats,
    Value, Builtin, StdLib,
    Lisp, RESERVED_SLOTS,
    DisplayValue,
    define_builtins, define_stdlib,
};

pub mod lexer;
mod parser;

pub use lexer::{Lexer, Token, SpannedToken, LexError, LexErrorKind};
pub use parser::{Parser, ParseError, ParseErrorKind, SourceLoc, parse, parse_all};
