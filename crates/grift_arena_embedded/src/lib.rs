//! # Grift Arena Embedded
//!
//! Embedded-specific features for the Grift Scheme interpreter.
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
//! The core Lisp interpreter (`grift_eval`) remains independent of hardware,
//! while this crate provides the bridge to embedded functionality.
//!
//! ## Usage
//!
//! ```rust
//! use grift_eval::{Lisp, Evaluator};
//! use grift_arena_embedded::register_embedded_natives;
//!
//! let lisp: Lisp<20000> = Lisp::new();
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
use grift_eval::{Evaluator, EvalError, register_native, IoProvider, PortId, IoResult, IoErrorKind};

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
register_native!(
    native_peek,
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
register_native!(
    native_poke,
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
register_native!(
    native_peek32,
    (addr: isize) -> isize,
    {
        let word_idx = ((addr as usize) / 4) % MOCK_MEMORY_WORDS;
        MOCK_MEMORY[word_idx].load(Ordering::Relaxed) as isize
    }
);

// Poke32: Write a 32-bit word to memory at the given address.

// Lisp signature: `(poke32 address value) -> value`

// Negative addresses are treated as unsigned values (wrapped).
register_native!(
    native_poke32,
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
register_native!(
    native_gpio_read,
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
register_native!(
    native_gpio_write,
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
register_native!(
    native_gpio_set,
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
register_native!(
    native_gpio_clear,
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
register_native!(
    native_gpio_toggle,
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

// Bit Set: Check if a bit is set in a value.
// Lisp signature: `(bit-set? value bit) -> #t/#f`
register_native!(native_bit_set, (value: isize, bit: isize) -> bool, {
    if bit >= 0 && bit < 64 {
        (value & (1isize << bit)) != 0
    } else {
        false
    }
});

// Bit Extract: Extract bits from a value.
// Lisp signature: `(bit-extract value start width) -> extracted-bits`
register_native!(native_bit_extract, (value: isize, start: isize, width: isize) -> isize, {
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
register_native!(native_bit_insert, (value: isize, insert: isize, start: isize, width: isize) -> isize, {
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
// use grift_eval::{Lisp, Evaluator};
// use grift_arena_embedded::register_embedded_natives;

// let lisp: Lisp<20000> = Lisp::new();
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

// ============================================================================
// Embedded I/O Provider
// ============================================================================

/// Default size of the input and output buffers in [`EmbeddedIoProvider`].
const EMBEDDED_IO_BUF_SIZE: usize = 1024;

/// A `no_std`, fixed-buffer I/O provider for embedded targets.
///
/// `EmbeddedIoProvider` implements [`IoProvider`] using stack-allocated byte
/// buffers of size `BUF` (default 1024 bytes).  It supports:
///
/// * **STDIN** — pre-loaded input via [`load_input`](Self::load_input).
/// * **STDOUT** — buffered output retrieved with [`output_str`](Self::output_str).
///
/// Because everything lives on the stack (or in a `static`), no heap
/// allocation is required, making this suitable for bare-metal or RTOS
/// environments.
///
/// # Example
///
/// ```rust
/// use grift_arena_embedded::EmbeddedIoProvider;
/// use grift_eval::{Lisp, Evaluator, IoProvider, PortId};
///
/// let lisp: Lisp<20000> = Lisp::new();
/// let mut eval = Evaluator::new(&lisp).unwrap();
///
/// // Create an I/O provider with a 256-byte buffer
/// let mut io = EmbeddedIoProvider::<256>::new();
///
/// // Pre-load input that (read-char) will consume
/// io.load_input("hello");
///
/// // Read characters back
/// assert_eq!(io.read_char(PortId::STDIN).unwrap(), 'h');
/// assert_eq!(io.read_char(PortId::STDIN).unwrap(), 'e');
///
/// // Write output
/// io.write_str(PortId::STDOUT, "ok").unwrap();
/// assert_eq!(io.output_str(), "ok");
/// ```
pub struct EmbeddedIoProvider<const BUF: usize = EMBEDDED_IO_BUF_SIZE> {
    /// Input buffer (UTF-8 bytes).
    input_buf: [u8; BUF],
    /// Number of valid bytes in `input_buf`.
    input_len: usize,
    /// Current read position in `input_buf`.
    input_pos: usize,
    /// Output buffer (UTF-8 bytes).
    output_buf: [u8; BUF],
    /// Number of valid bytes in `output_buf`.
    output_len: usize,
    /// Whether STDIN is still open.
    stdin_open: bool,
    /// Whether STDOUT is still open.
    stdout_open: bool,
}

impl<const BUF: usize> EmbeddedIoProvider<BUF> {
    /// Create a new `EmbeddedIoProvider` with empty buffers.
    pub const fn new() -> Self {
        Self {
            input_buf: [0u8; BUF],
            input_len: 0,
            input_pos: 0,
            output_buf: [0u8; BUF],
            output_len: 0,
            stdin_open: true,
            stdout_open: true,
        }
    }

    /// Pre-load input data that will be consumed by [`read_char`](IoProvider::read_char).
    ///
    /// Any unread data is discarded; the read position resets to zero.
    /// If the string exceeds the buffer size it is silently truncated.
    pub fn load_input(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let copy_len = if bytes.len() > BUF { BUF } else { bytes.len() };
        self.input_buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
        self.input_len = copy_len;
        self.input_pos = 0;
    }

    /// Return the output buffer contents as a `&str`.
    ///
    /// The buffer is guaranteed to be valid UTF-8 because only
    /// [`write_char`](IoProvider::write_char) and
    /// [`write_str`](IoProvider::write_str) can append to it.
    pub fn output_str(&self) -> &str {
        // SAFETY: we only ever write valid UTF-8 into output_buf.
        core::str::from_utf8(&self.output_buf[..self.output_len]).unwrap_or("")
    }

    /// Clear the output buffer.
    pub fn clear_output(&mut self) {
        self.output_len = 0;
    }

    /// Clear the input buffer and reset the read position.
    pub fn clear_input(&mut self) {
        self.input_len = 0;
        self.input_pos = 0;
    }
}

impl<const BUF: usize> IoProvider for EmbeddedIoProvider<BUF> {
    fn read_char(&mut self, port: PortId) -> IoResult<char> {
        if port != PortId::STDIN {
            return Err(IoErrorKind::InvalidPort);
        }
        if !self.stdin_open {
            return Err(IoErrorKind::PortClosed);
        }
        if self.input_pos >= self.input_len {
            return Err(IoErrorKind::Eof);
        }
        // Decode one UTF-8 character from the input buffer.
        let remaining = &self.input_buf[self.input_pos..self.input_len];
        match core::str::from_utf8(remaining) {
            Ok(s) => {
                let ch = s.chars().next().unwrap();
                self.input_pos += ch.len_utf8();
                Ok(ch)
            }
            Err(_) => Err(IoErrorKind::ReadFailed),
        }
    }

    fn peek_char(&mut self, port: PortId) -> IoResult<char> {
        if port != PortId::STDIN {
            return Err(IoErrorKind::InvalidPort);
        }
        if !self.stdin_open {
            return Err(IoErrorKind::PortClosed);
        }
        if self.input_pos >= self.input_len {
            return Err(IoErrorKind::Eof);
        }
        let remaining = &self.input_buf[self.input_pos..self.input_len];
        match core::str::from_utf8(remaining) {
            Ok(s) => Ok(s.chars().next().unwrap()),
            Err(_) => Err(IoErrorKind::ReadFailed),
        }
    }

    fn char_ready(&mut self, port: PortId) -> IoResult<bool> {
        if port != PortId::STDIN {
            return Err(IoErrorKind::InvalidPort);
        }
        Ok(self.input_pos < self.input_len)
    }

    fn write_char(&mut self, port: PortId, c: char) -> IoResult<()> {
        if port != PortId::STDOUT && port != PortId::STDERR {
            return Err(IoErrorKind::InvalidPort);
        }
        if !self.stdout_open {
            return Err(IoErrorKind::PortClosed);
        }
        let mut buf = [0u8; 4];
        let encoded = c.encode_utf8(&mut buf);
        let bytes = encoded.as_bytes();
        if self.output_len + bytes.len() > BUF {
            return Err(IoErrorKind::WriteFailed);
        }
        self.output_buf[self.output_len..self.output_len + bytes.len()]
            .copy_from_slice(bytes);
        self.output_len += bytes.len();
        Ok(())
    }

    fn write_str(&mut self, port: PortId, s: &str) -> IoResult<()> {
        if port != PortId::STDOUT && port != PortId::STDERR {
            return Err(IoErrorKind::InvalidPort);
        }
        if !self.stdout_open {
            return Err(IoErrorKind::PortClosed);
        }
        let bytes = s.as_bytes();
        if self.output_len + bytes.len() > BUF {
            return Err(IoErrorKind::WriteFailed);
        }
        self.output_buf[self.output_len..self.output_len + bytes.len()]
            .copy_from_slice(bytes);
        self.output_len += bytes.len();
        Ok(())
    }

    fn flush(&mut self, _port: PortId) -> IoResult<()> {
        // No-op for fixed buffers.
        Ok(())
    }

    fn close_port(&mut self, port: PortId) -> IoResult<()> {
        if port == PortId::STDIN {
            self.stdin_open = false;
        } else if port == PortId::STDOUT || port == PortId::STDERR {
            self.stdout_open = false;
        }
        Ok(())
    }

    fn is_input_port(&self, port: PortId) -> bool {
        port == PortId::STDIN
    }

    fn is_output_port(&self, port: PortId) -> bool {
        port == PortId::STDOUT || port == PortId::STDERR
    }

    fn is_port_open(&self, port: PortId) -> bool {
        if port == PortId::STDIN {
            self.stdin_open
        } else if port == PortId::STDOUT || port == PortId::STDERR {
            self.stdout_open
        } else {
            false
        }
    }
}