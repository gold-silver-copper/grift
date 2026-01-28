//! # PWN Arena Embedded
//!
//! Embedded-specific features for the pwn_arena Lisp interpreter.
//!
//! This crate provides native Rust functions for:
//! - Hardware register access (read/write)
//! - Memory peek/poke operations
//! - GPIO control
//! - Bit manipulation utilities
//!
//! ## Design
//!
//! This crate is designed to be modular and work in `no_std` environments.
//! The core Lisp interpreter (`lisp_eval`) remains independent of hardware,
//! while this crate provides the bridge to embedded functionality.
//!
//! ## Usage
//!
//! ```rust
//! use lisp_eval::{Lisp, Evaluator};
//! use pwn_arena_embedded::register_embedded_natives;
//!
//! let lisp: Lisp<10000> = Lisp::new();
//! let mut eval = Evaluator::new(&lisp).unwrap();
//!
//! // Register all embedded native functions
//! register_embedded_natives(&mut eval).unwrap();
//!
//! // Now you can call embedded functions from Lisp:
//! // (peek 0)           ; Read memory at address
//! // (poke 0 42)        ; Write to memory
//! // (gpio-read 0)      ; Read GPIO register 0
//! // (gpio-write 0 1)   ; Write 1 to GPIO register 0
//! ```
//!
//! ## Safety
//!
//! Memory access functions are inherently unsafe on real hardware.
//! This crate provides a mock implementation for testing that uses
//! a simulated memory space rather than real hardware registers.

#![no_std]
#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicUsize, Ordering};
use lisp_eval::{Evaluator, EvalError, define_native_stateful};

// ============================================================================
// Mock Memory/Register Storage using Atomics
// ============================================================================

// Number of 32-bit words in mock memory (256 words = 1KB)
const MOCK_MEMORY_WORDS: usize = 256;

// Number of GPIO registers
const MOCK_GPIO_COUNT: usize = 16;

// Simulated memory for testing (256 x word-size units = 1KB on 32-bit)
// Using AtomicUsize for thread-safe access without unsafe code
static MOCK_MEMORY: [AtomicUsize; MOCK_MEMORY_WORDS] = {
    const INIT: AtomicUsize = AtomicUsize::new(0);
    [INIT; MOCK_MEMORY_WORDS]
};

// Simulated GPIO registers (16 registers, word-size each)
static MOCK_GPIO: [AtomicUsize; MOCK_GPIO_COUNT] = {
    const INIT: AtomicUsize = AtomicUsize::new(0);
    [INIT; MOCK_GPIO_COUNT]
};

// ============================================================================
// Native Functions for Memory Access
// ============================================================================

// Peek: Read a byte from memory at the given address.

// Lisp signature: `(peek address) -> value`

// In the mock implementation, addresses are mapped to a 1KB buffer.
// Negative addresses are treated as unsigned values (wrapped).
define_native_stateful!(
    native_peek,
    static: MOCK_MEMORY,
    (addr: isize) -> isize,
    {
        let byte_addr = (addr as usize) % (MOCK_MEMORY_WORDS * 4);
        let word_idx = byte_addr / 4;
        let byte_offset = byte_addr % 4;
        let word = MOCK_MEMORY[word_idx].load(Ordering::Relaxed);
        ((word >> (byte_offset * 8)) & 0xFF) as isize
    }
);

// Poke: Write a byte to memory at the given address.

// Lisp signature: `(poke address value) -> value`

// In the mock implementation, addresses are mapped to a 1KB buffer.
// Negative addresses are treated as unsigned values (wrapped).
define_native_stateful!(
    native_poke,
    static: MOCK_MEMORY,
    (addr: isize, value: isize) -> isize,
    {
        let byte_addr = (addr as usize) % (MOCK_MEMORY_WORDS * 4);
        let word_idx = byte_addr / 4;
        let byte_offset = byte_addr % 4;
        
        let mask = 0xFFusize << (byte_offset * 8);
        let new_byte = ((value as usize) & 0xFF) << (byte_offset * 8);
        
        let _ = MOCK_MEMORY[word_idx].fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            Some((old & !mask) | new_byte)
        });
        
        value
    }
);

// Peek32: Read a 32-bit word from memory at the given address.

// Lisp signature: `(peek32 address) -> value`

// Negative addresses are treated as unsigned values (wrapped).
define_native_stateful!(
    native_peek32,
    static: MOCK_MEMORY,
    (addr: isize) -> isize,
    {
        let word_idx = ((addr as usize) / 4) % MOCK_MEMORY_WORDS;
        MOCK_MEMORY[word_idx].load(Ordering::Relaxed) as isize
    }
);

// Poke32: Write a 32-bit word to memory at the given address.

// Lisp signature: `(poke32 address value) -> value`

// Negative addresses are treated as unsigned values (wrapped).
define_native_stateful!(
    native_poke32,
    static: MOCK_MEMORY,
    (addr: isize, value: isize) -> isize,
    {
        let word_idx = ((addr as usize) / 4) % MOCK_MEMORY_WORDS;
        MOCK_MEMORY[word_idx].store(value as usize, Ordering::Relaxed);
        value
    }
);

// ============================================================================
// Native Functions for GPIO
// ============================================================================

// GPIO Read: Read a GPIO register.

// Lisp signature: `(gpio-read register) -> value`

// Register is an index (0-15) into the GPIO register array.
define_native_stateful!(
    native_gpio_read,
    static: MOCK_GPIO,
    (reg: isize) -> isize,
    {
        if reg < 0 || reg >= MOCK_GPIO_COUNT as isize {
            0
        } else {
            MOCK_GPIO[reg as usize].load(Ordering::Relaxed) as isize
        }
    }
);

// GPIO Write: Write to a GPIO register.

// Lisp signature: `(gpio-write register value) -> value`
define_native_stateful!(
    native_gpio_write,
    static: MOCK_GPIO,
    (reg: isize, value: isize) -> isize,
    {
        if reg >= 0 && reg < MOCK_GPIO_COUNT as isize {
            MOCK_GPIO[reg as usize].store(value as usize, Ordering::Relaxed);
        }
        value
    }
);

// GPIO Set Bit: Set a specific bit in a GPIO register.

// Lisp signature: `(gpio-set register bit) -> new-value`
define_native_stateful!(
    native_gpio_set,
    static: MOCK_GPIO,
    (reg: isize, bit: isize) -> isize,
    {
        if reg >= 0 && reg < MOCK_GPIO_COUNT as isize && bit >= 0 && bit < 32 {
            let mask = 1usize << bit;
            let old = MOCK_GPIO[reg as usize].fetch_or(mask, Ordering::Relaxed);
            (old | mask) as isize
        } else {
            0
        }
    }
);

// GPIO Clear Bit: Clear a specific bit in a GPIO register.

// Lisp signature: `(gpio-clear register bit) -> new-value`
define_native_stateful!(
    native_gpio_clear,
    static: MOCK_GPIO,
    (reg: isize, bit: isize) -> isize,
    {
        if reg >= 0 && reg < MOCK_GPIO_COUNT as isize && bit >= 0 && bit < 32 {
            let mask = !(1usize << bit);
            let old = MOCK_GPIO[reg as usize].fetch_and(mask, Ordering::Relaxed);
            (old & mask) as isize
        } else {
            0
        }
    }
);

// GPIO Toggle Bit: Toggle a specific bit in a GPIO register.

// Lisp signature: `(gpio-toggle register bit) -> new-value`
define_native_stateful!(
    native_gpio_toggle,
    static: MOCK_GPIO,
    (reg: isize, bit: isize) -> isize,
    {
        if reg >= 0 && reg < MOCK_GPIO_COUNT as isize && bit >= 0 && bit < 32 {
            let mask = 1usize << bit;
            let old = MOCK_GPIO[reg as usize].fetch_xor(mask, Ordering::Relaxed);
            (old ^ mask) as isize
        } else {
            0
        }
    }
);

// ============================================================================
// Utility Functions
// ============================================================================

use lisp_eval::define_native;

// Bit Set: Check if a bit is set in a value.
// Lisp signature: `(bit-set? value bit) -> #t/#f`
define_native!(native_bit_set, (value: isize, bit: isize) -> bool, {
    if bit >= 0 && bit < 64 {
        (value & (1isize << bit)) != 0
    } else {
        false
    }
});

// Bit Extract: Extract bits from a value.
// Lisp signature: `(bit-extract value start width) -> extracted-bits`
define_native!(native_bit_extract, (value: isize, start: isize, width: isize) -> isize, {
    // Validate that start and width are in valid ranges and don't cause overflow
    if start >= 0 && start < 64 && width > 0 && width <= 64 && (start + width) <= 64 {
        let mask = if width >= 64 { !0isize } else { (1isize << width) - 1 };
        (value >> start) & mask
    } else {
        0isize
    }
});

// Bit Insert: Insert bits into a value.
// Lisp signature: `(bit-insert value insert start width) -> new-value`
define_native!(native_bit_insert, (value: isize, insert: isize, start: isize, width: isize) -> isize, {
    // Validate that start and width are in valid ranges and don't cause overflow
    if start >= 0 && start < 64 && width > 0 && width <= 64 && (start + width) <= 64 {
        let mask = if width >= 64 { !0isize } else { (1isize << width) - 1 };
        let cleared = value & !(mask << start);
        cleared | ((insert & mask) << start)
    } else {
        value
    }
});

// ============================================================================
// Registration
// ============================================================================

// Register all embedded native functions with an evaluator.

// This function registers the following native functions:

// ## Memory Access
// - `(peek addr)` - Read byte from memory
// - `(poke addr val)` - Write byte to memory
// - `(peek32 addr)` - Read 32-bit word
// - `(poke32 addr val)` - Write 32-bit word

// ## GPIO
// - `(gpio-read reg)` - Read GPIO register
// - `(gpio-write reg val)` - Write GPIO register
// - `(gpio-set reg bit)` - Set bit in GPIO register
// - `(gpio-clear reg bit)` - Clear bit in GPIO register
// - `(gpio-toggle reg bit)` - Toggle bit in GPIO register

// ## Bit Manipulation
// - `(bit-set? val bit)` - Check if bit is set
// - `(bit-extract val start width)` - Extract bits
// - `(bit-insert val insert start width)` - Insert bits

// # Example

// ```rust
// use lisp_eval::{Lisp, Evaluator};
// use pwn_arena_embedded::register_embedded_natives;

// let lisp: Lisp<10000> = Lisp::new();
// let mut eval = Evaluator::new(&lisp).unwrap();
// register_embedded_natives(&mut eval).unwrap();

// // Now embedded functions are available in Lisp
// let result = eval.eval_str("(poke 0 42)").unwrap();
// assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));

// let result = eval.eval_str("(peek 0)").unwrap();
// assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
// ```
pub fn register_embedded_natives<const N: usize>(eval: &mut Evaluator<N>) -> Result<(), EvalError> {
    // Memory access
    eval.register_native("peek", native_peek)?;
    eval.register_native("poke", native_poke)?;
    eval.register_native("peek32", native_peek32)?;
    eval.register_native("poke32", native_poke32)?;
    
    // GPIO
    eval.register_native("gpio-read", native_gpio_read)?;
    eval.register_native("gpio-write", native_gpio_write)?;
    eval.register_native("gpio-set", native_gpio_set)?;
    eval.register_native("gpio-clear", native_gpio_clear)?;
    eval.register_native("gpio-toggle", native_gpio_toggle)?;
    
    // Bit manipulation
    eval.register_native("bit-set?", native_bit_set)?;
    eval.register_native("bit-extract", native_bit_extract)?;
    eval.register_native("bit-insert", native_bit_insert)?;
    
    Ok(())
}

// Reset mock memory and GPIO registers to zero.

// This is useful for testing to ensure a clean state.
// Uses SeqCst ordering to ensure all resets are visible across threads.
pub fn reset_mock_hardware() {
    for mem in MOCK_MEMORY.iter() {
        mem.store(0, Ordering::SeqCst);
    }
    for gpio in MOCK_GPIO.iter() {
        gpio.store(0, Ordering::SeqCst);
    }
}