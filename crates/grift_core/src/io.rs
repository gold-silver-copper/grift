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

    /// Return `true` if the port is still open.
    /// Default: returns `true` (assumes ports are open unless overridden).
    fn is_port_open(&self, _port: PortId) -> bool {
        true
    }

    /// Return `true` if the port is a textual port.
    /// Default: returns `true` for any valid input or output port.
    fn is_textual_port(&self, port: PortId) -> bool {
        self.is_input_port(port) || self.is_output_port(port)
    }

    /// Return `true` if the port is a binary port.
    /// Default: returns `false` (all ports are textual by default).
    fn is_binary_port(&self, _port: PortId) -> bool {
        false
    }

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

    /// Convert an output string port into an input string port.
    ///
    /// The accumulated content of the output port becomes the data source
    /// for the new input port. The original output port is closed.
    ///
    /// This avoids the need to copy the accumulated string through a
    /// fixed-size stack buffer when creating an input port from collected
    /// character data.
    ///
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn output_string_to_input_port(&mut self, _output_port: PortId) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    // ----------------------------------------------------------------
    // File port operations (R7RS §6.13.2)
    // ----------------------------------------------------------------

    /// Open a textual input port connected to the named file.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn open_input_file(&mut self, _path: &str) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    /// Open a textual output port connected to the named file.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn open_output_file(&mut self, _path: &str) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    /// Open a binary input port connected to the named file.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn open_binary_input_file(&mut self, _path: &str) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    /// Open a binary output port connected to the named file.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn open_binary_output_file(&mut self, _path: &str) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    // ----------------------------------------------------------------
    // Binary I/O operations (R7RS §6.13.2)
    // ----------------------------------------------------------------

    /// Read a single byte from a binary input port.
    /// Returns [`IoErrorKind::Eof`] at end of file.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn read_u8(&mut self, _port: PortId) -> IoResult<u8> {
        Err(IoErrorKind::Unsupported)
    }

    /// Peek at the next byte without consuming it.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn peek_u8(&mut self, _port: PortId) -> IoResult<u8> {
        Err(IoErrorKind::Unsupported)
    }

    /// Return `true` if a byte is ready on the binary input port.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn u8_ready(&mut self, _port: PortId) -> IoResult<bool> {
        Err(IoErrorKind::Unsupported)
    }

    /// Write a single byte to a binary output port.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn write_u8(&mut self, _port: PortId, _byte: u8) -> IoResult<()> {
        Err(IoErrorKind::Unsupported)
    }

    /// Read up to `k` bytes from a binary input port into `buf`.
    /// Returns the number of bytes actually read.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn read_bytevector(&mut self, _port: PortId, _buf: &mut [u8]) -> IoResult<usize> {
        Err(IoErrorKind::Unsupported)
    }

    /// Write bytes to a binary output port.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn write_bytevector(&mut self, _port: PortId, _bytes: &[u8]) -> IoResult<()> {
        Err(IoErrorKind::Unsupported)
    }

    // ----------------------------------------------------------------
    // Bytevector port operations (R7RS §6.13.2)
    // ----------------------------------------------------------------

    /// Open a binary input port that reads from the given byte slice.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn open_input_bytevector(&mut self, _bytes: &[u8]) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    /// Open a binary output port that accumulates bytes.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn open_output_bytevector(&mut self) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    /// Retrieve the accumulated bytes from an output bytevector port.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn get_output_bytevector(&self, _port: PortId) -> IoResult<&[u8]> {
        Err(IoErrorKind::Unsupported)
    }

    // ----------------------------------------------------------------
    // File system operations (R7RS §6.13)
    // ----------------------------------------------------------------

    /// Check whether a file exists at the given path.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn file_exists(&self, _path: &str) -> IoResult<bool> {
        Err(IoErrorKind::Unsupported)
    }

    /// Delete the file at the given path.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn delete_file(&mut self, _path: &str) -> IoResult<()> {
        Err(IoErrorKind::Unsupported)
    }

    /// Read the entire contents of a file as a string.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn read_file(&mut self, _path: &str) -> IoResult<&str> {
        Err(IoErrorKind::Unsupported)
    }

    // ----------------------------------------------------------------
    // Process / environment operations (R7RS §6.14)
    // ----------------------------------------------------------------

    /// Return the number of command-line arguments.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn command_line_count(&self) -> IoResult<usize> {
        Err(IoErrorKind::Unsupported)
    }

    /// Return the command-line argument at the given index.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn command_line_arg(&self, _index: usize) -> IoResult<&str> {
        Err(IoErrorKind::Unsupported)
    }

    /// Retrieve the value of an environment variable by name.
    /// Returns `Ok(Some(value))` if found, `Ok(None)` if not set.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn get_environment_variable(&mut self, _name: &str) -> IoResult<Option<&str>> {
        Err(IoErrorKind::Unsupported)
    }

    /// Return the number of environment variables.
    /// Must be called before [`environment_variable_at`](Self::environment_variable_at)
    /// to snapshot the current environment.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn environment_variables_count(&mut self) -> IoResult<usize> {
        Err(IoErrorKind::Unsupported)
    }

    /// Return the environment variable name and value at the given index.
    /// [`environment_variables_count`](Self::environment_variables_count) must be
    /// called first to snapshot the environment.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn environment_variable_at(&self, _index: usize) -> IoResult<(&str, &str)> {
        Err(IoErrorKind::Unsupported)
    }

    /// Exit the process with the given status code.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn exit_process(&mut self, _code: i32) -> IoResult<()> {
        Err(IoErrorKind::Unsupported)
    }

    /// Emergency exit the process (no cleanup) with the given status code.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn emergency_exit_process(&mut self, _code: i32) -> IoResult<()> {
        Err(IoErrorKind::Unsupported)
    }

    // ----------------------------------------------------------------
    // Time operations (R7RS §6.13.3)
    // ----------------------------------------------------------------

    /// Return the current time as seconds since the Unix epoch (inexact).
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn current_second(&self) -> IoResult<f64> {
        Err(IoErrorKind::Unsupported)
    }

    /// Return the current jiffy count (monotonic, implementation-defined units).
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn current_jiffy(&self) -> IoResult<i64> {
        Err(IoErrorKind::Unsupported)
    }

    /// Return the number of jiffies per SI second.
    /// Default: returns [`IoErrorKind::Unsupported`].
    fn jiffies_per_second(&self) -> IoResult<i64> {
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
