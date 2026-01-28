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

use core::sync::atomic::{AtomicU32, Ordering};
use lisp_eval::{
    Lisp, Evaluator, EvalError, ArenaIndex, ArenaResult,
    ToLisp, extract_arg,
};

// ============================================================================
// Mock Memory/Register Storage using Atomics
// ============================================================================

/// Number of 32-bit words in mock memory (256 words = 1KB)
const MOCK_MEMORY_WORDS: usize = 256;

/// Number of GPIO registers
const MOCK_GPIO_COUNT: usize = 16;

/// Simulated memory for testing (256 x 32-bit words = 1KB)
/// Using AtomicU32 for thread-safe access without unsafe code
static MOCK_MEMORY: [AtomicU32; MOCK_MEMORY_WORDS] = {
    // Use a const block to initialize the array
    const INIT: AtomicU32 = AtomicU32::new(0);
    [INIT; MOCK_MEMORY_WORDS]
};

/// Simulated GPIO registers (16 registers, 32 bits each)
static MOCK_GPIO: [AtomicU32; MOCK_GPIO_COUNT] = {
    const INIT: AtomicU32 = AtomicU32::new(0);
    [INIT; MOCK_GPIO_COUNT]
};

// ============================================================================
// Native Functions for Memory Access
// ============================================================================

/// Peek: Read a byte from memory at the given address.
///
/// Lisp signature: `(peek address) -> value`
///
/// In the mock implementation, addresses are mapped to a 1KB buffer.
/// Negative addresses are treated as unsigned values (wrapped).
pub fn native_peek<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (addr, _rest): (i64, _) = extract_arg(lisp, args)?;
    
    // Map address to word index and byte offset
    // Use wrapping conversion for negative addresses (they wrap to positive)
    let byte_addr = (addr as u64 as usize) % (MOCK_MEMORY_WORDS * 4);
    let word_idx = byte_addr / 4;
    let byte_offset = byte_addr % 4;
    
    let word = MOCK_MEMORY[word_idx].load(Ordering::Relaxed);
    let value = ((word >> (byte_offset * 8)) & 0xFF) as i64;
    
    value.to_lisp(lisp)
}

/// Poke: Write a byte to memory at the given address.
///
/// Lisp signature: `(poke address value) -> value`
///
/// In the mock implementation, addresses are mapped to a 1KB buffer.
/// Negative addresses are treated as unsigned values (wrapped).
pub fn native_poke<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (addr, rest): (i64, _) = extract_arg(lisp, args)?;
    let (value, _rest): (i64, _) = extract_arg(lisp, rest)?;
    
    // Map address to word index and byte offset
    // Use wrapping conversion for negative addresses
    let byte_addr = (addr as u64 as usize) % (MOCK_MEMORY_WORDS * 4);
    let word_idx = byte_addr / 4;
    let byte_offset = byte_addr % 4;
    
    // Read-modify-write the word
    let mask = 0xFFu32 << (byte_offset * 8);
    let new_byte = ((value as u32) & 0xFF) << (byte_offset * 8);
    
    // Use fetch_update for atomic RMW
    let _ = MOCK_MEMORY[word_idx].fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some((old & !mask) | new_byte)
    });
    
    value.to_lisp(lisp)
}

/// Peek32: Read a 32-bit word from memory at the given address.
///
/// Lisp signature: `(peek32 address) -> value`
///
/// Negative addresses are treated as unsigned values (wrapped).
pub fn native_peek32<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (addr, _rest): (i64, _) = extract_arg(lisp, args)?;
    
    // Word-aligned access with wrapping conversion for negative addresses
    let word_idx = ((addr as u64 as usize) / 4) % MOCK_MEMORY_WORDS;
    let value = MOCK_MEMORY[word_idx].load(Ordering::Relaxed) as i64;
    
    value.to_lisp(lisp)
}

/// Poke32: Write a 32-bit word to memory at the given address.
///
/// Lisp signature: `(poke32 address value) -> value`
///
/// Negative addresses are treated as unsigned values (wrapped).
pub fn native_poke32<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (addr, rest): (i64, _) = extract_arg(lisp, args)?;
    let (value, _rest): (i64, _) = extract_arg(lisp, rest)?;
    
    // Word-aligned access with wrapping conversion for negative addresses
    let word_idx = ((addr as u64 as usize) / 4) % MOCK_MEMORY_WORDS;
    MOCK_MEMORY[word_idx].store(value as u32, Ordering::Relaxed);
    
    value.to_lisp(lisp)
}

// ============================================================================
// Native Functions for GPIO
// ============================================================================

/// GPIO Read: Read a GPIO register.
///
/// Lisp signature: `(gpio-read register) -> value`
///
/// Register is an index (0-15) into the GPIO register array.
pub fn native_gpio_read<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (reg, _rest): (i64, _) = extract_arg(lisp, args)?;
    
    if reg < 0 || reg >= MOCK_GPIO_COUNT as i64 {
        return 0i64.to_lisp(lisp); // Out of range returns 0
    }
    
    let value = MOCK_GPIO[reg as usize].load(Ordering::Relaxed) as i64;
    value.to_lisp(lisp)
}

/// GPIO Write: Write to a GPIO register.
///
/// Lisp signature: `(gpio-write register value) -> value`
pub fn native_gpio_write<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (reg, rest): (i64, _) = extract_arg(lisp, args)?;
    let (value, _rest): (i64, _) = extract_arg(lisp, rest)?;
    
    if reg >= 0 && reg < MOCK_GPIO_COUNT as i64 {
        MOCK_GPIO[reg as usize].store(value as u32, Ordering::Relaxed);
    }
    
    value.to_lisp(lisp)
}

/// GPIO Set Bit: Set a specific bit in a GPIO register.
///
/// Lisp signature: `(gpio-set register bit) -> new-value`
pub fn native_gpio_set<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (reg, rest): (i64, _) = extract_arg(lisp, args)?;
    let (bit, _rest): (i64, _) = extract_arg(lisp, rest)?;
    
    if reg >= 0 && reg < MOCK_GPIO_COUNT as i64 && bit >= 0 && bit < 32 {
        let mask = 1u32 << bit;
        let old = MOCK_GPIO[reg as usize].fetch_or(mask, Ordering::Relaxed);
        return ((old | mask) as i64).to_lisp(lisp);
    }
    
    0i64.to_lisp(lisp)
}

/// GPIO Clear Bit: Clear a specific bit in a GPIO register.
///
/// Lisp signature: `(gpio-clear register bit) -> new-value`
pub fn native_gpio_clear<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (reg, rest): (i64, _) = extract_arg(lisp, args)?;
    let (bit, _rest): (i64, _) = extract_arg(lisp, rest)?;
    
    if reg >= 0 && reg < MOCK_GPIO_COUNT as i64 && bit >= 0 && bit < 32 {
        let mask = !(1u32 << bit);
        let old = MOCK_GPIO[reg as usize].fetch_and(mask, Ordering::Relaxed);
        return ((old & mask) as i64).to_lisp(lisp);
    }
    
    0i64.to_lisp(lisp)
}

/// GPIO Toggle Bit: Toggle a specific bit in a GPIO register.
///
/// Lisp signature: `(gpio-toggle register bit) -> new-value`
pub fn native_gpio_toggle<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (reg, rest): (i64, _) = extract_arg(lisp, args)?;
    let (bit, _rest): (i64, _) = extract_arg(lisp, rest)?;
    
    if reg >= 0 && reg < MOCK_GPIO_COUNT as i64 && bit >= 0 && bit < 32 {
        let mask = 1u32 << bit;
        let old = MOCK_GPIO[reg as usize].fetch_xor(mask, Ordering::Relaxed);
        return ((old ^ mask) as i64).to_lisp(lisp);
    }
    
    0i64.to_lisp(lisp)
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Bit Set: Check if a bit is set in a value.
///
/// Lisp signature: `(bit-set? value bit) -> #t/#f`
pub fn native_bit_set<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (value, rest): (i64, _) = extract_arg(lisp, args)?;
    let (bit, _rest): (i64, _) = extract_arg(lisp, rest)?;
    
    if bit >= 0 && bit < 64 {
        let is_set = (value & (1i64 << bit)) != 0;
        return is_set.to_lisp(lisp);
    }
    
    false.to_lisp(lisp)
}

/// Bit Extract: Extract bits from a value.
///
/// Lisp signature: `(bit-extract value start width) -> extracted-bits`
pub fn native_bit_extract<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (value, rest): (i64, _) = extract_arg(lisp, args)?;
    let (start, rest): (i64, _) = extract_arg(lisp, rest)?;
    let (width, _rest): (i64, _) = extract_arg(lisp, rest)?;
    
    // Validate that start and width are in valid ranges and don't cause overflow
    if start >= 0 && start < 64 && width > 0 && width <= 64 && (start + width) <= 64 {
        let mask = if width >= 64 { !0i64 } else { (1i64 << width) - 1 };
        let extracted = (value >> start) & mask;
        return extracted.to_lisp(lisp);
    }
    
    0i64.to_lisp(lisp)
}

/// Bit Insert: Insert bits into a value.
///
/// Lisp signature: `(bit-insert value insert start width) -> new-value`
pub fn native_bit_insert<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (value, rest): (i64, _) = extract_arg(lisp, args)?;
    let (insert, rest): (i64, _) = extract_arg(lisp, rest)?;
    let (start, rest): (i64, _) = extract_arg(lisp, rest)?;
    let (width, _rest): (i64, _) = extract_arg(lisp, rest)?;
    
    // Validate that start and width are in valid ranges and don't cause overflow
    if start >= 0 && start < 64 && width > 0 && width <= 64 && (start + width) <= 64 {
        let mask = if width >= 64 { !0i64 } else { (1i64 << width) - 1 };
        let cleared = value & !(mask << start);
        let inserted = cleared | ((insert & mask) << start);
        return inserted.to_lisp(lisp);
    }
    
    value.to_lisp(lisp)
}

// ============================================================================
// Registration
// ============================================================================

/// Register all embedded native functions with an evaluator.
///
/// This function registers the following native functions:
///
/// ## Memory Access
/// - `(peek addr)` - Read byte from memory
/// - `(poke addr val)` - Write byte to memory
/// - `(peek32 addr)` - Read 32-bit word
/// - `(poke32 addr val)` - Write 32-bit word
///
/// ## GPIO
/// - `(gpio-read reg)` - Read GPIO register
/// - `(gpio-write reg val)` - Write GPIO register
/// - `(gpio-set reg bit)` - Set bit in GPIO register
/// - `(gpio-clear reg bit)` - Clear bit in GPIO register
/// - `(gpio-toggle reg bit)` - Toggle bit in GPIO register
///
/// ## Bit Manipulation
/// - `(bit-set? val bit)` - Check if bit is set
/// - `(bit-extract val start width)` - Extract bits
/// - `(bit-insert val insert start width)` - Insert bits
///
/// # Example
///
/// ```rust
/// use lisp_eval::{Lisp, Evaluator};
/// use pwn_arena_embedded::register_embedded_natives;
///
/// let lisp: Lisp<10000> = Lisp::new();
/// let mut eval = Evaluator::new(&lisp).unwrap();
/// register_embedded_natives(&mut eval).unwrap();
///
/// // Now embedded functions are available in Lisp
/// let result = eval.eval_str("(poke 0 42)").unwrap();
/// assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
///
/// let result = eval.eval_str("(peek 0)").unwrap();
/// assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
/// ```
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

/// Reset mock memory and GPIO registers to zero.
///
/// This is useful for testing to ensure a clean state.
pub fn reset_mock_hardware() {
    for mem in MOCK_MEMORY.iter() {
        mem.store(0, Ordering::Relaxed);
    }
    for gpio in MOCK_GPIO.iter() {
        gpio.store(0, Ordering::Relaxed);
    }
}