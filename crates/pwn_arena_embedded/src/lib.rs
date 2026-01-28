//! # PWN Arena Embedded
//!
//! Embedded-specific features for the pwn_arena Lisp interpreter.
//!
//! ## Design Change
//!
//! The embedded functions (peek, poke, GPIO, bit manipulation) are now
//! integrated into the core `lisp_eval` crate as built-in functions.
//! This eliminates the need for runtime registration and provides
//! compile-time binding for all Lisp functions.
//!
//! ## Available Functions
//!
//! The following functions are available as builtins in `lisp_eval`:
//!
//! ### Memory Access
//! - `(peek addr)` - Read byte from memory address
//! - `(poke addr val)` - Write byte to memory address
//! - `(peek32 addr)` - Read 32-bit word from memory
//! - `(poke32 addr val)` - Write 32-bit word to memory
//!
//! ### GPIO Control
//! - `(gpio-read reg)` - Read GPIO register
//! - `(gpio-write reg val)` - Write GPIO register
//! - `(gpio-set reg bit)` - Set bit in GPIO register
//! - `(gpio-clear reg bit)` - Clear bit in GPIO register
//! - `(gpio-toggle reg bit)` - Toggle bit in GPIO register
//!
//! ### Bit Manipulation
//! - `(bit-set? val bit)` - Check if bit is set
//! - `(bit-extract val start width)` - Extract bits from value
//! - `(bit-insert val insert start width)` - Insert bits into value
//!
//! ## Usage
//!
//! ```rust
//! use lisp_eval::{Lisp, Evaluator};
//!
//! let lisp: Lisp<10000> = Lisp::new();
//! let eval = Evaluator::new(&lisp).unwrap();
//!
//! // Embedded functions are available immediately - no registration needed
//! // (peek 0)           ; Read memory at address 0
//! // (poke 0 42)        ; Write 42 to address 0
//! // (gpio-read 0)      ; Read GPIO register 0
//! // (bit-set? 5 0)     ; Check if bit 0 is set in 5 -> #t
//! ```
//!
//! ## Implementation Notes
//!
//! The embedded functions use a mock implementation with simulated memory
//! and GPIO registers for testing purposes. The mock storage is:
//! - 256 words (1KB) of simulated memory
//! - 16 GPIO registers
//!
//! All operations are thread-safe using atomic operations.

#![no_std]
#![forbid(unsafe_code)]

// Re-export lisp_eval for convenience
pub use lisp_eval::{Evaluator, Lisp, Value, Builtin};
