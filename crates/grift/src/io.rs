//! I/O trait boundary for the Grift Lisp evaluator.
//!
//! This module defines the [`IoProvider`] trait, which serves as the boundary
//! between the pure `no_std` evaluator and platform-specific I/O implementations.
//!
//! ## Port Model
//!
//! I/O is modeled in terms of *ports*. A [`PortId`] identifies a port,
//! with well-known constants for standard input, output, and error.
//!
//! ## Design
//!
//! The evaluator core is fully `no_std` and performs no I/O itself.
//! Instead, an [`IoProvider`] trait abstracts all port operations:
//!
//! - **[`NullIoProvider`]** — No-op implementation that silently discards
//!   output. Used in `no_std`/embedded contexts where there is no real I/O.
//! - **`StdIoProvider`** (behind `std` feature) — Implementation backed
//!   by `std::io`, providing stdout/stderr output and dynamic file/string ports.

/// Identifies an I/O port.
///
/// Standard ports ([`STDIN`](PortId::STDIN), [`STDOUT`](PortId::STDOUT),
/// [`STDERR`](PortId::STDERR)) are pre-defined.
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

/// Error kinds for I/O operations in `no_std` environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoErrorKind {
    /// Write operation failed.
    WriteFailed,
    /// Read operation failed.
    ReadFailed,
    /// Operation not supported on this port.
    Unsupported,
    /// End of file reached.
    Eof,
    /// Port is closed.
    PortClosed,
    /// Port not found or invalid.
    InvalidPort,
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
/// * [`PortId::STDIN`], [`PortId::STDOUT`], and [`PortId::STDERR`] should
///   always be accepted as valid ports.
/// * All methods except [`write_str`](Self::write_str) have default
///   implementations returning `Err(IoErrorKind::Unsupported)` or sensible
///   defaults, so minimal embedded implementations only need `write_str`.
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
    // --- String output (required) ---

    /// Write a string slice to the specified output port.
    fn write_str(&mut self, port: PortId, s: &str) -> IoResult<()>;

    // --- Character I/O ---

    /// Read a single character from the specified input port.
    fn read_char(&mut self, _port: PortId) -> IoResult<char> {
        Err(IoErrorKind::Unsupported)
    }

    /// Peek at the next character without consuming it.
    fn peek_char(&mut self, _port: PortId) -> IoResult<char> {
        Err(IoErrorKind::Unsupported)
    }

    /// Write a single character to the specified output port.
    fn write_char(&mut self, port: PortId, c: char) -> IoResult<()> {
        let mut buf = [0u8; 4];
        self.write_str(port, c.encode_utf8(&mut buf))
    }

    /// Return `true` if a character is ready on the input port.
    fn char_ready(&mut self, _port: PortId) -> IoResult<bool> {
        Err(IoErrorKind::Unsupported)
    }

    // --- File ports ---

    /// Open a textual input port connected to the named file.
    fn open_input_file(&mut self, _path: &str) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    /// Open a textual output port connected to the named file.
    fn open_output_file(&mut self, _path: &str) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    // --- String ports ---

    /// Open an input port that reads from the given string.
    fn open_input_string(&mut self, _s: &str) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    /// Open an output port that accumulates characters into a string buffer.
    fn open_output_string(&mut self) -> IoResult<PortId> {
        Err(IoErrorKind::Unsupported)
    }

    /// Retrieve the accumulated string from an output string port.
    fn get_output_string(&self, _port: PortId) -> IoResult<&str> {
        Err(IoErrorKind::Unsupported)
    }

    // --- Port lifecycle ---

    /// Close the specified port.
    fn close_port(&mut self, _port: PortId) -> IoResult<()> {
        Err(IoErrorKind::Unsupported)
    }

    /// Flush the specified output port.
    fn flush(&mut self, _port: PortId) -> IoResult<()> {
        Ok(())
    }

    // --- Port queries ---

    /// Return `true` if the port is an input port.
    fn is_input_port(&self, port: PortId) -> bool {
        port == PortId::STDIN
    }

    /// Return `true` if the port is an output port.
    fn is_output_port(&self, port: PortId) -> bool {
        port == PortId::STDOUT || port == PortId::STDERR
    }

    /// Return `true` if the port is still open.
    fn is_port_open(&self, port: PortId) -> bool {
        port.0 <= 2
    }

    // --- Filesystem ---

    /// Check whether a file exists at the given path.
    fn file_exists(&self, _path: &str) -> IoResult<bool> {
        Err(IoErrorKind::Unsupported)
    }

    /// Delete the file at the given path.
    fn delete_file(&mut self, _path: &str) -> IoResult<()> {
        Err(IoErrorKind::Unsupported)
    }

    /// Read the entire contents of a file as a string.
    fn read_file(&mut self, _path: &str) -> IoResult<&str> {
        Err(IoErrorKind::Unsupported)
    }
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

// ============================================================================
// Streaming writer types
// ============================================================================

/// Streams output through the function-pointer IO path.
///
/// Used by builtins (`display`, `newline`) inside the eval loop,
/// where only a `fn(PortId, &str)` callback is available.
pub(crate) struct IoWriter {
    pub(crate) port: PortId,
    pub(crate) write_fn: fn(PortId, &str),
}

impl core::fmt::Write for IoWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        (self.write_fn)(self.port, s);
        Ok(())
    }
}

/// Streams output through a trait-object [`IoProvider`].
///
/// Used by the public [`display_to_io`](super::Lisp::display_to_io) and
/// [`write_to_io`](super::Lisp::write_to_io) API methods.
pub(crate) struct TraitIoWriter<'a> {
    pub(crate) port: PortId,
    pub(crate) io: &'a mut dyn IoProvider,
    pub(crate) error: Option<IoErrorKind>,
}

impl core::fmt::Write for TraitIoWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.error.is_some() {
            return Err(core::fmt::Error);
        }
        if let Err(e) = self.io.write_str(self.port, s) {
            self.error = Some(e);
            return Err(core::fmt::Error);
        }
        Ok(())
    }
}

// ============================================================================
// StdIoProvider — std-backed implementation with dynamic ports
// ============================================================================

#[cfg(feature = "std")]
mod std_io {
    extern crate std;

    use std::io::{Read, Write, BufRead};
    use std::string::String;
    use std::vec::Vec;

    use super::{IoErrorKind, IoProvider, IoResult, PortId};

    /// First dynamically-allocated port id (after STDIN=0, STDOUT=1, STDERR=2).
    const DYNAMIC_PORT_BASE: usize = 3;

    /// A dynamically-opened port.
    enum DynPort {
        /// Input port backed by a string buffer with a read cursor.
        InputString { data: Vec<char>, cursor: usize, closed: bool },
        /// Output port that accumulates characters.
        OutputString { buf: String, closed: bool },
        /// Textual input port backed by a file.
        InputFile { file: std::fs::File, peeked: Option<char>, closed: bool },
        /// Textual output port backed by a file.
        OutputFile { file: std::io::BufWriter<std::fs::File>, closed: bool },
    }

    impl DynPort {
        fn is_closed(&self) -> bool {
            match self {
                DynPort::InputString { closed, .. }
                | DynPort::OutputString { closed, .. }
                | DynPort::InputFile { closed, .. }
                | DynPort::OutputFile { closed, .. } => *closed,
            }
        }
    }

    /// An [`IoProvider`] implementation backed by Rust's standard I/O.
    ///
    /// Supports the three well-known ports:
    ///
    /// | [`PortId`] | Direction | Backing |
    /// |-----------|-----------|---------|
    /// | `STDIN`   | input     | `std::io::stdin()` |
    /// | `STDOUT`  | output    | `std::io::stdout()` |
    /// | `STDERR`  | output    | `std::io::stderr()` |
    ///
    /// Additionally supports dynamically opened string and file ports
    /// via [`open_input_string`](IoProvider::open_input_string),
    /// [`open_output_string`](IoProvider::open_output_string),
    /// [`open_input_file`](IoProvider::open_input_file), and
    /// [`open_output_file`](IoProvider::open_output_file).
    pub struct StdIoProvider {
        /// One-character peek buffer for stdin.
        peeked: Option<char>,
        /// Dynamically opened ports.
        ports: Vec<Option<DynPort>>,
        /// Temporary buffer for file content (used by read_file).
        file_buf: String,
    }

    impl StdIoProvider {
        /// Create a new [`StdIoProvider`].
        pub fn new() -> Self {
            StdIoProvider {
                peeked: None,
                ports: Vec::new(),
                file_buf: String::new(),
            }
        }

        /// Allocate a fresh [`PortId`] and store the given dynamic port.
        fn alloc_port(&mut self, port: DynPort) -> IoResult<PortId> {
            // Try to reuse a freed slot first
            for (i, slot) in self.ports.iter_mut().enumerate() {
                if slot.is_none() || slot.as_ref().is_some_and(|p| p.is_closed()) {
                    *slot = Some(port);
                    return Ok(PortId(DYNAMIC_PORT_BASE + i));
                }
            }
            // Allocate a new slot
            let id = DYNAMIC_PORT_BASE + self.ports.len();
            self.ports.push(Some(port));
            Ok(PortId(id))
        }

        /// Get a reference to a dynamic port, or `None` if not found.
        fn get_dyn(&self, port: PortId) -> Option<&DynPort> {
            if port.0 < DYNAMIC_PORT_BASE { return None; }
            let idx = port.0 - DYNAMIC_PORT_BASE;
            self.ports.get(idx).and_then(|s| s.as_ref())
        }

        /// Get a mutable reference to a dynamic port, or `None` if not found.
        fn get_dyn_mut(&mut self, port: PortId) -> Option<&mut DynPort> {
            if port.0 < DYNAMIC_PORT_BASE { return None; }
            let idx = port.0 - DYNAMIC_PORT_BASE;
            self.ports.get_mut(idx).and_then(|s| s.as_mut())
        }
    }

    impl Default for StdIoProvider {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Read one UTF-8 character from a `Read` source.
    fn read_one_char_from<R: Read>(reader: &mut R) -> IoResult<char> {
        let mut buf = [0u8; 4];
        let first = {
            let n = reader.read(&mut buf[..1]).map_err(|_| IoErrorKind::ReadFailed)?;
            if n == 0 {
                return Err(IoErrorKind::Eof);
            }
            buf[0]
        };

        let char_len = if first < 0x80 {
            1
        } else if (0xC2..=0xDF).contains(&first) {
            2
        } else if (0xE0..=0xEF).contains(&first) {
            3
        } else if (0xF0..=0xF4).contains(&first) {
            4
        } else {
            return Err(IoErrorKind::ReadFailed);
        };

        if char_len > 1 {
            let remaining = &mut buf[1..char_len];
            reader.read_exact(remaining).map_err(|_| IoErrorKind::ReadFailed)?;
        }

        core::str::from_utf8(&buf[..char_len])
            .ok()
            .and_then(|s| s.chars().next())
            .ok_or(IoErrorKind::ReadFailed)
    }

    /// Read one UTF-8 character from stdin (locked).
    fn read_stdin_char() -> IoResult<char> {
        let stdin = std::io::stdin();
        let mut handle = stdin.lock();
        read_one_char_from(&mut handle)
    }

    impl IoProvider for StdIoProvider {
        fn write_str(&mut self, port: PortId, s: &str) -> IoResult<()> {
            match port {
                PortId::STDOUT => std::io::stdout()
                    .write_all(s.as_bytes())
                    .map_err(|_| IoErrorKind::WriteFailed),
                PortId::STDERR => std::io::stderr()
                    .write_all(s.as_bytes())
                    .map_err(|_| IoErrorKind::WriteFailed),
                _ => {
                    match self.get_dyn_mut(port) {
                        Some(DynPort::OutputString { buf, closed }) => {
                            if *closed { return Err(IoErrorKind::PortClosed); }
                            buf.push_str(s);
                            Ok(())
                        }
                        Some(DynPort::OutputFile { file, closed }) => {
                            if *closed { return Err(IoErrorKind::PortClosed); }
                            file.write_all(s.as_bytes()).map_err(|_| IoErrorKind::WriteFailed)
                        }
                        _ => Err(IoErrorKind::InvalidPort),
                    }
                }
            }
        }

        fn read_char(&mut self, port: PortId) -> IoResult<char> {
            if port == PortId::STDIN {
                if let Some(c) = self.peeked.take() {
                    return Ok(c);
                }
                return read_stdin_char();
            }
            match self.get_dyn_mut(port) {
                Some(DynPort::InputString { data, cursor, closed }) => {
                    if *closed { return Err(IoErrorKind::PortClosed); }
                    if *cursor >= data.len() { return Err(IoErrorKind::Eof); }
                    let c = data[*cursor];
                    *cursor += 1;
                    Ok(c)
                }
                Some(DynPort::InputFile { file, peeked, closed }) => {
                    if *closed { return Err(IoErrorKind::PortClosed); }
                    if let Some(c) = peeked.take() {
                        return Ok(c);
                    }
                    read_one_char_from(file)
                }
                _ => Err(IoErrorKind::InvalidPort),
            }
        }

        fn peek_char(&mut self, port: PortId) -> IoResult<char> {
            if port == PortId::STDIN {
                if let Some(c) = self.peeked {
                    return Ok(c);
                }
                let c = read_stdin_char()?;
                self.peeked = Some(c);
                return Ok(c);
            }
            match self.get_dyn_mut(port) {
                Some(DynPort::InputString { data, cursor, closed }) => {
                    if *closed { return Err(IoErrorKind::PortClosed); }
                    if *cursor >= data.len() { return Err(IoErrorKind::Eof); }
                    Ok(data[*cursor])
                }
                Some(DynPort::InputFile { file, peeked, closed }) => {
                    if *closed { return Err(IoErrorKind::PortClosed); }
                    if let Some(c) = *peeked {
                        return Ok(c);
                    }
                    let c = read_one_char_from(file)?;
                    *peeked = Some(c);
                    Ok(c)
                }
                _ => Err(IoErrorKind::InvalidPort),
            }
        }

        fn char_ready(&mut self, port: PortId) -> IoResult<bool> {
            if port == PortId::STDIN {
                if self.peeked.is_some() {
                    return Ok(true);
                }
                let stdin = std::io::stdin();
                let mut handle = stdin.lock();
                return Ok(!handle.fill_buf().map_or(true, |b| b.is_empty()));
            }
            match self.get_dyn(port) {
                Some(DynPort::InputString { data, cursor, closed }) => {
                    Ok(!closed && *cursor < data.len())
                }
                Some(DynPort::InputFile { closed, .. }) => Ok(!closed),
                _ => Err(IoErrorKind::InvalidPort),
            }
        }

        fn open_input_file(&mut self, path: &str) -> IoResult<PortId> {
            let file = std::fs::File::open(path).map_err(|_| IoErrorKind::ReadFailed)?;
            self.alloc_port(DynPort::InputFile { file, peeked: None, closed: false })
        }

        fn open_output_file(&mut self, path: &str) -> IoResult<PortId> {
            let file = std::fs::File::create(path).map_err(|_| IoErrorKind::WriteFailed)?;
            self.alloc_port(DynPort::OutputFile { file: std::io::BufWriter::new(file), closed: false })
        }

        fn open_input_string(&mut self, s: &str) -> IoResult<PortId> {
            let data: Vec<char> = s.chars().collect();
            self.alloc_port(DynPort::InputString { data, cursor: 0, closed: false })
        }

        fn open_output_string(&mut self) -> IoResult<PortId> {
            self.alloc_port(DynPort::OutputString { buf: String::new(), closed: false })
        }

        fn get_output_string(&self, port: PortId) -> IoResult<&str> {
            match self.get_dyn(port) {
                Some(DynPort::OutputString { buf, .. }) => Ok(buf.as_str()),
                _ => Err(IoErrorKind::InvalidPort),
            }
        }

        fn close_port(&mut self, port: PortId) -> IoResult<()> {
            if port.0 < DYNAMIC_PORT_BASE {
                return Err(IoErrorKind::Unsupported);
            }
            match self.get_dyn_mut(port) {
                Some(DynPort::OutputFile { file, closed }) => {
                    let _ = file.flush();
                    *closed = true;
                    Ok(())
                }
                Some(DynPort::InputString { closed, .. })
                | Some(DynPort::OutputString { closed, .. })
                | Some(DynPort::InputFile { closed, .. }) => {
                    *closed = true;
                    Ok(())
                }
                None => Err(IoErrorKind::InvalidPort),
            }
        }

        fn flush(&mut self, port: PortId) -> IoResult<()> {
            match port {
                PortId::STDOUT => std::io::stdout().flush().map_err(|_| IoErrorKind::WriteFailed),
                PortId::STDERR => std::io::stderr().flush().map_err(|_| IoErrorKind::WriteFailed),
                _ => {
                    match self.get_dyn_mut(port) {
                        Some(DynPort::OutputFile { file, closed }) => {
                            if *closed { return Err(IoErrorKind::PortClosed); }
                            file.flush().map_err(|_| IoErrorKind::WriteFailed)
                        }
                        _ => Ok(()),
                    }
                }
            }
        }

        fn is_input_port(&self, port: PortId) -> bool {
            if port == PortId::STDIN { return true; }
            matches!(self.get_dyn(port),
                Some(DynPort::InputString { .. }) | Some(DynPort::InputFile { .. })
            )
        }

        fn is_output_port(&self, port: PortId) -> bool {
            if port == PortId::STDOUT || port == PortId::STDERR { return true; }
            matches!(self.get_dyn(port),
                Some(DynPort::OutputString { .. }) | Some(DynPort::OutputFile { .. })
            )
        }

        fn is_port_open(&self, port: PortId) -> bool {
            if port.0 < DYNAMIC_PORT_BASE { return true; }
            match self.get_dyn(port) {
                Some(p) => !p.is_closed(),
                None => false,
            }
        }

        fn file_exists(&self, path: &str) -> IoResult<bool> {
            Ok(std::path::Path::new(path).exists())
        }

        fn delete_file(&mut self, path: &str) -> IoResult<()> {
            std::fs::remove_file(path).map_err(|_| IoErrorKind::WriteFailed)
        }

        fn read_file(&mut self, path: &str) -> IoResult<&str> {
            self.file_buf = std::fs::read_to_string(path).map_err(|_| IoErrorKind::ReadFailed)?;
            Ok(&self.file_buf)
        }
    }
}

#[cfg(feature = "std")]
pub use std_io::StdIoProvider;
