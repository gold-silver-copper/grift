//! I/O trait boundary for the Grift Scheme evaluator.
//!
//! This module defines the [`IoProvider`] trait, which serves as the boundary
//! between the pure `no_std` evaluator and platform-specific I/O implementations.
//!
//! The `grift_std` crate provides a standard implementation using Rust's `std::io`.
//!
//! ## Port Model
//!
//! R7RS defines I/O in terms of *ports*. A [`PortId`] identifies a port,
//! with well-known constants for standard input, output, and error.

use grift_arena::ArenaIndex;

// ============================================================================
// Port Identifier
// ============================================================================

/// Identifies an I/O port.
///
/// Standard ports ([`STDIN`](PortId::STDIN), [`STDOUT`](PortId::STDOUT),
/// [`STDERR`](PortId::STDERR)) are pre-defined.  Implementations may allocate
/// additional ports for file or string I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortId(pub usize);

impl PortId {
    /// Standard input port.
    pub const STDIN: PortId = PortId(0);
    /// Standard output port.
    pub const STDOUT: PortId = PortId(1);
    /// Standard error port.
    pub const STDERR: PortId = PortId(2);
}

// ============================================================================
// I/O Error
// ============================================================================

/// Error kinds for I/O operations in `no_std` environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoErrorKind {
    /// End of file reached.
    Eof,
    /// Port not found or invalid.
    InvalidPort,
    /// Write operation failed.
    WriteFailed,
    /// Read operation failed.
    ReadFailed,
    /// Port is closed.
    PortClosed,
    /// Operation not supported on this port.
    Unsupported,
}

/// Result type for I/O operations.
pub type IoResult<T> = Result<T, IoErrorKind>;

// ============================================================================
// IoProvider Trait
// ============================================================================

/// Trait for providing I/O operations to the evaluator.
///
/// This trait defines the boundary between the pure `no_std` evaluator
/// and platform-specific I/O implementations.  The `grift_std` crate
/// provides a standard implementation backed by Rust's `std::io`.
///
/// # Implementor Notes
///
/// * [`PortId::STDIN`], [`PortId::STDOUT`], and [`PortId::STDERR`] should
///   always be accepted as valid ports.
/// * Implementations may support additional dynamically-opened ports
///   (e.g. file ports) by returning fresh [`PortId`] values.
pub trait IoProvider {
    /// Read a single character from the specified input port.
    fn read_char(&mut self, port: PortId) -> IoResult<char>;

    /// Peek at the next character without consuming it.
    fn peek_char(&mut self, port: PortId) -> IoResult<char>;

    /// Return `true` if a character is ready on the input port.
    fn char_ready(&mut self, port: PortId) -> IoResult<bool>;

    /// Write a single character to the specified output port.
    fn write_char(&mut self, port: PortId, c: char) -> IoResult<()>;

    /// Write a string slice to the specified output port.
    fn write_str(&mut self, port: PortId, s: &str) -> IoResult<()>;

    /// Flush the specified output port.
    fn flush(&mut self, port: PortId) -> IoResult<()>;

    /// Close the specified port.
    fn close_port(&mut self, port: PortId) -> IoResult<()>;

    /// Return `true` if the port is an input port.
    fn is_input_port(&self, port: PortId) -> bool;

    /// Return `true` if the port is an output port.
    fn is_output_port(&self, port: PortId) -> bool;

    /// Open an input port that reads from the given string.
    ///
    /// Returns a fresh [`PortId`] for the new port.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn open_input_string(&mut self, _s: &str) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    /// Open an output port that accumulates characters into a string buffer.
    ///
    /// Returns a fresh [`PortId`] for the new port.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn open_output_string(&mut self) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    /// Retrieve the accumulated string from an output string port.
    ///
    /// The port must have been created by [`open_output_string`](Self::open_output_string).
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn get_output_string(&self, _port: PortId) -> IoResult<&str> {
        Err(IoErrorKind::Unsupported)
    }
}

/// A no-op I/O provider that silently discards all output and returns
/// [`IoErrorKind::Unsupported`] for reads.
///
/// Useful as a default when no I/O back-end is configured.
pub struct NullIoProvider;

impl IoProvider for NullIoProvider {
    fn read_char(&mut self, _port: PortId) -> IoResult<char> {
        Err(IoErrorKind::Unsupported)
    }

    fn peek_char(&mut self, _port: PortId) -> IoResult<char> {
        Err(IoErrorKind::Unsupported)
    }

    fn char_ready(&mut self, _port: PortId) -> IoResult<bool> {
        Err(IoErrorKind::Unsupported)
    }

    fn write_char(&mut self, _port: PortId, _c: char) -> IoResult<()> {
        Ok(())
    }

    fn write_str(&mut self, _port: PortId, _s: &str) -> IoResult<()> {
        Ok(())
    }

    fn flush(&mut self, _port: PortId) -> IoResult<()> {
        Ok(())
    }

    fn close_port(&mut self, _port: PortId) -> IoResult<()> {
        Ok(())
    }

    fn is_input_port(&self, _port: PortId) -> bool {
        false
    }

    fn is_output_port(&self, _port: PortId) -> bool {
        false
    }
}

// ============================================================================
// Display helper (for use with IoProvider)
// ============================================================================

/// Write a [`Value`](crate::Value) through an [`IoProvider`] port.
///
/// This is a convenience for the evaluator's `display` / `write` builtins
/// to emit a value through the I/O abstraction without requiring `alloc`.
/// The `idx` is an arena index that the caller can format into the port.
///
/// Implementations of higher-level display logic live in the evaluator
/// (or in `grift_std`), but this marker type can carry the index through
/// the trait boundary.
#[derive(Debug, Clone, Copy)]
pub struct DisplayPort {
    /// The arena index of the value to display.
    pub value: ArenaIndex,
    /// The target port.
    pub port: PortId,
}
