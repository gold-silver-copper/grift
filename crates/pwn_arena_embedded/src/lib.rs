//! # PWN Arena Embedded
//!
//! Embedded-specific features for the pwn_arena Lisp interpreter.
//!
//! This crate provides re-exports for embedded/hardware operations that are now
//! built into the core Lisp interpreter as builtins.
//!
//! ## Available Builtins
//!
//! The following builtins are now part of the core Lisp:
//!
//! ## Memory Access
//! - `(peek addr)` - Read byte from memory
//! - `(poke addr val)` - Write byte to memory
//! - `(peek32 addr)` - Read 32-bit word
//! - `(poke32 addr val)` - Write 32-bit word
//!
//! ## GPIO
//! - `(gpio-read reg)` - Read GPIO register
//! - `(gpio-write reg val)` - Write GPIO register
//! - `(gpio-set reg bit)` - Set bit in GPIO register
//! - `(gpio-clear reg bit)` - Clear bit in GPIO register
//! - `(gpio-toggle reg bit)` - Toggle bit in GPIO register
//!
//! ## Bit Manipulation
//! - `(bit-set? val bit)` - Check if bit is set
//! - `(bit-extract val start width)` - Extract bits
//! - `(bit-insert val insert start width)` - Insert bits
//!
//! ## Usage
//!
//! ```rust
//! use lisp_eval::{Lisp, Evaluator};
//!
//! let lisp: Lisp<10000> = Lisp::new();
//! let eval = Evaluator::new(&lisp).unwrap();
//!
//! // Embedded functions are automatically available as builtins:
//! // (peek 0)           ; Read memory at address
//! // (poke 0 42)        ; Write to memory
//! // (gpio-read 0)      ; Read GPIO register 0
//! // (gpio-write 0 1)   ; Write 1 to GPIO register 0
//! ```
//!
//! ## Safety
//!
//! Memory access functions use a mock implementation that uses a simulated
//! memory space rather than real hardware registers. For real hardware access,
//! you would need to modify the builtin implementations in lisp_eval.

#![no_std]
#![forbid(unsafe_code)]

// Re-export the mock hardware and reset function from lisp_eval
pub use lisp_eval::{
    MOCK_MEMORY, MOCK_GPIO, MOCK_MEMORY_WORDS, MOCK_GPIO_COUNT,
    reset_mock_hardware,
};