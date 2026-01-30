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
//! - All values are stored in a `pwn_arena` arena
//! - Supports garbage collection via the `Trace` trait
//! - Explicit boolean values (#t, #f) separate from nil/empty list
//! - **Strict evaluation** - All arguments are evaluated before function application (call-by-value)
//!
//! ## Value Representation
//!
//! - `Nil` - The empty list (NOT false!)
//! - `True` - Boolean true (#t)
//! - `False` - Boolean false (#f)
//! - `Number(isize)` - Integer numbers (exact)
//! - `Float(f64)` - Floating-point numbers (inexact)
//! - `Char(char)` - Single character
//! - `Cons { car, cdr }` - Pair/list cell
//! - `Symbol { chars }` - Symbol with contiguous string storage
//! - `Lambda { params, body, env }` - Closure
//! - `Builtin(Builtin)` - Optimized built-in function
//! - `StdLib(StdLib)` - Standard library function (static code, parsed on-demand)
//!
//! ## Reserved Slots
//!
//! The first 4 slots of the arena are reserved:
//! - Slot 0: `Value::Nil` - the empty list singleton
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

pub use pwn_arena::{Arena, ArenaIndex, ArenaError, ArenaResult, Trace, GcStats};

mod value;
mod lisp;
mod parser;

pub use value::{Value, Builtin, StdLib};
// Note: define_builtins and define_stdlib macros are exported at crate root via #[macro_export]

pub use lisp::{Lisp, RESERVED_SLOTS};
pub use parser::{Parser, ParseError, ParseErrorKind, SourceLoc, parse, parse_all};
