//! I/O trait boundary for the Grift Lisp evaluator.
//!
//! This module defines the [`IoProvider`] trait, which serves as the boundary
//! between the pure `no_std` evaluator and platform-specific I/O implementations.
//!
//! ## Port Model
//!
//! I/O is modeled in terms of *ports*. A [`PortId`] identifies a port,
//! with well-known constants for standard output and standard error.
//!
//! ## Design
//!
//! The evaluator core is fully `no_std` and performs no I/O itself.
//! Instead, an [`IoProvider`] trait abstracts all port operations:
//!
//! - **[`NullIoProvider`]** — No-op implementation that silently discards
//!   output. Used in `no_std`/embedded contexts where there is no real I/O.
//! - **`StdIoProvider`** (behind `std` feature) — Implementation backed
//!   by `std::io`, providing stdout/stderr output.

/// Identifies an I/O port.
///
/// Standard ports ([`STDOUT`](PortId::STDOUT), [`STDERR`](PortId::STDERR))
/// are pre-defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortId(pub usize);

impl PortId {
    /// Standard output port.
    pub const STDOUT: PortId = PortId(0);
    /// Standard error port.
    pub const STDERR: PortId = PortId(1);
}

/// Error kinds for I/O operations in `no_std` environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoErrorKind {
    /// Write operation failed.
    WriteFailed,
    /// Operation not supported on this port.
    Unsupported,
}

/// Result type for I/O operations.
pub type IoResult<T> = Result<T, IoErrorKind>;

/// Trait for providing I/O operations to the evaluator.
///
/// This trait defines the boundary between the pure `no_std` evaluator
/// and platform-specific I/O implementations.
///
/// # Implementor Notes
///
/// * [`PortId::STDOUT`] and [`PortId::STDERR`] should always be
///   accepted as valid ports.
/// * Embedded implementations can write to UART, SPI, or other
///   hardware peripherals by implementing this trait.
///
/// # Example
///
/// ```rust,ignore
/// use grift::io::{IoProvider, PortId, IoResult};
///
/// struct UartIo;
///
/// impl IoProvider for UartIo {
///     fn write_str(&mut self, _port: PortId, s: &str) -> IoResult<()> {
///         // Write to UART hardware...
///         Ok(())
///     }
/// }
/// ```
pub trait IoProvider {
    /// Write a string slice to the specified output port.
    fn write_str(&mut self, port: PortId, s: &str) -> IoResult<()>;
}

/// A no-op I/O provider that silently discards all output.
///
/// Useful as a default when no I/O back-end is configured,
/// or in `no_std`/embedded contexts where there is no real I/O.
pub struct NullIoProvider;

impl IoProvider for NullIoProvider {
    fn write_str(&mut self, _port: PortId, _s: &str) -> IoResult<()> {
        Ok(())
    }
}

/// Standard I/O provider backed by Rust's `std::io`.
///
/// Writes to stdout and stderr using the standard library.
///
/// Only available when the `std` feature is enabled.
#[cfg(feature = "std")]
pub struct StdIoProvider;

#[cfg(feature = "std")]
impl StdIoProvider {
    /// Create a new [`StdIoProvider`].
    pub fn new() -> Self {
        StdIoProvider
    }
}

#[cfg(feature = "std")]
impl Default for StdIoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "std")]
impl IoProvider for StdIoProvider {
    fn write_str(&mut self, port: PortId, s: &str) -> IoResult<()> {
        extern crate std;
        use std::io::Write;
        match port {
            PortId::STDOUT => std::io::stdout()
                .write_all(s.as_bytes())
                .map_err(|_| IoErrorKind::WriteFailed),
            PortId::STDERR => std::io::stderr()
                .write_all(s.as_bytes())
                .map_err(|_| IoErrorKind::WriteFailed),
            _ => Err(IoErrorKind::Unsupported),
        }
    }
}
