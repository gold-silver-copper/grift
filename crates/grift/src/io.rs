//! I/O trait boundary for the Grift Lisp evaluator.
//!
//! This module defines the [`IoProvider`] trait, which serves as the boundary
//! between the pure `no_std` evaluator and platform-specific I/O implementations.
//!
//! ## Portless Model
//!
//! I/O is modeled with hardcoded stdin/stdout/stderr channels, bulk file
//! operations, and string serialization. There are no port objects or
//! dynamic port management. Read primitives return `()` (NIL) at
//! end-of-input instead of an EOF sentinel.
//!
//! ## Design
//!
//! The evaluator core is fully `no_std` and performs no I/O itself.
//! Instead, an [`IoProvider`] trait abstracts all I/O operations:
//!
//! - **[`NullIoProvider`]** — No-op implementation that silently discards
//!   output. Used in `no_std`/embedded contexts where there is no real I/O.
//! - **`StdIoProvider`** (behind `std` feature) — Implementation backed
//!   by `std::io`, providing real stdin/stdout/stderr and file operations.

/// Error kinds for I/O operations in `no_std` environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoErrorKind {
    /// Write operation failed.
    WriteFailed,
    /// Read operation failed.
    ReadFailed,
    /// Operation not supported by this provider.
    Unsupported,
    /// End of file / input reached.
    Eof,
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
/// * [`write_stdout`](Self::write_stdout) and [`write_stderr`](Self::write_stderr)
///   are required. All other methods have default implementations returning
///   `Err(IoErrorKind::Unsupported)`, so minimal embedded implementations
///   only need the two write methods.
///
/// # Example
///
/// ```rust,ignore
/// use grift::io::{IoProvider, IoResult};
///
/// struct UartIo;
///
/// impl IoProvider for UartIo {
///     fn write_stdout(&mut self, s: &str) -> IoResult<()> {
///         // Write to UART hardware...
///         Ok(())
///     }
///     fn write_stderr(&mut self, s: &str) -> IoResult<()> {
///         self.write_stdout(s)
///     }
/// }
/// ```
pub trait IoProvider {
    /// Write a string slice to stdout.
    fn write_stdout(&mut self, s: &str) -> IoResult<()>;

    /// Write a string slice to stderr.
    fn write_stderr(&mut self, s: &str) -> IoResult<()>;

    /// Write a single character to stdout (handles UTF-8 encoding internally).
    fn write_char_stdout(&mut self, c: char) -> IoResult<()> {
        let mut buf = [0u8; 4];
        self.write_stdout(c.encode_utf8(&mut buf))
    }

    /// Write a single character to stderr (handles UTF-8 encoding internally).
    fn write_char_stderr(&mut self, c: char) -> IoResult<()> {
        let mut buf = [0u8; 4];
        self.write_stderr(c.encode_utf8(&mut buf))
    }

    /// Read a single character from stdin.
    fn read_stdin_char(&mut self) -> IoResult<char> {
        Err(IoErrorKind::Unsupported)
    }

    /// Peek at the next character on stdin without consuming it.
    fn peek_stdin_char(&mut self) -> IoResult<char> {
        Err(IoErrorKind::Unsupported)
    }

    /// Read the entire contents of a file as a string.
    fn read_file(&mut self, _path: &str) -> IoResult<&str> {
        Err(IoErrorKind::Unsupported)
    }

    /// Write a string to a file (creating or truncating).
    fn write_file(&mut self, _path: &str, _content: &str) -> IoResult<()> {
        Err(IoErrorKind::Unsupported)
    }

    /// Write characters from an iterator to a file (creating or truncating).
    ///
    /// Streams content directly without requiring a contiguous buffer.
    fn write_file_chars(&mut self, _path: &str, _chars: &mut dyn Iterator<Item = char>) -> IoResult<()> {
        Err(IoErrorKind::Unsupported)
    }

    /// Check whether a file exists at the given path.
    fn file_exists(&self, _path: &str) -> IoResult<bool> {
        Err(IoErrorKind::Unsupported)
    }

    /// Delete the file at the given path.
    fn delete_file(&mut self, _path: &str) -> IoResult<()> {
        Err(IoErrorKind::Unsupported)
    }
}

/// A no-op I/O provider that silently discards all output.
///
/// Useful as a default when no I/O back-end is configured,
/// or in `no_std`/embedded contexts where there is no real I/O.
pub struct NullIoProvider;

impl IoProvider for NullIoProvider {
    fn write_stdout(&mut self, _s: &str) -> IoResult<()> {
        Ok(())
    }
    fn write_stderr(&mut self, _s: &str) -> IoResult<()> {
        Ok(())
    }
}

// ============================================================================
// Streaming writer types
// ============================================================================

/// Streams output to stdout through a borrowed [`IoProvider`].
///
/// Used by `raw-display` and `raw-write` builtins to walk the value tree
/// and emit output without any intermediate buffer.
pub(crate) struct StdoutWriter<'a, IO: IoProvider> {
    pub(crate) io: &'a core::cell::RefCell<IO>,
    pub(crate) error: Option<IoErrorKind>,
}

impl<IO: IoProvider> core::fmt::Write for StdoutWriter<'_, IO> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.error.is_some() {
            return Err(core::fmt::Error);
        }
        if let Err(e) = self.io.borrow_mut().write_stdout(s) {
            self.error = Some(e);
            return Err(core::fmt::Error);
        }
        Ok(())
    }
}

// ============================================================================
// StdIoProvider — std-backed implementation (no ports)
// ============================================================================

#[cfg(feature = "std")]
mod std_io {
    extern crate std;

    use std::io::{Read, Write};
    use std::string::String;

    use super::{IoErrorKind, IoProvider, IoResult};

    /// An [`IoProvider`] implementation backed by Rust's standard I/O.
    ///
    /// Provides real stdin/stdout/stderr access and filesystem operations.
    /// No port management — just hardcoded standard streams and bulk
    /// file I/O.
    pub struct StdIoProvider {
        /// One-character peek buffer for stdin.
        peeked: Option<char>,
        /// Temporary buffer for file content (used by `read_file`).
        file_buf: String,
    }

    impl StdIoProvider {
        /// Create a new [`StdIoProvider`].
        pub fn new() -> Self {
            StdIoProvider {
                peeked: None,
                file_buf: String::new(),
            }
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

    impl IoProvider for StdIoProvider {
        fn write_stdout(&mut self, s: &str) -> IoResult<()> {
            std::io::stdout()
                .write_all(s.as_bytes())
                .map_err(|_| IoErrorKind::WriteFailed)
        }

        fn write_stderr(&mut self, s: &str) -> IoResult<()> {
            std::io::stderr()
                .write_all(s.as_bytes())
                .map_err(|_| IoErrorKind::WriteFailed)
        }

        fn read_stdin_char(&mut self) -> IoResult<char> {
            if let Some(c) = self.peeked.take() {
                return Ok(c);
            }
            let stdin = std::io::stdin();
            let mut handle = stdin.lock();
            read_one_char_from(&mut handle)
        }

        fn peek_stdin_char(&mut self) -> IoResult<char> {
            if let Some(c) = self.peeked {
                return Ok(c);
            }
            let stdin = std::io::stdin();
            let mut handle = stdin.lock();
            let c = read_one_char_from(&mut handle)?;
            self.peeked = Some(c);
            Ok(c)
        }

        fn read_file(&mut self, path: &str) -> IoResult<&str> {
            self.file_buf = std::fs::read_to_string(path).map_err(|_| IoErrorKind::ReadFailed)?;
            Ok(&self.file_buf)
        }

        fn write_file(&mut self, path: &str, content: &str) -> IoResult<()> {
            std::fs::write(path, content).map_err(|_| IoErrorKind::WriteFailed)
        }

        fn write_file_chars(&mut self, path: &str, chars: &mut dyn Iterator<Item = char>) -> IoResult<()> {
            let mut file = std::fs::File::create(path).map_err(|_| IoErrorKind::WriteFailed)?;
            let mut buf = [0u8; 4];
            for c in chars {
                let s = c.encode_utf8(&mut buf);
                file.write_all(s.as_bytes()).map_err(|_| IoErrorKind::WriteFailed)?;
            }
            Ok(())
        }

        fn file_exists(&self, path: &str) -> IoResult<bool> {
            Ok(std::path::Path::new(path).exists())
        }

        fn delete_file(&mut self, path: &str) -> IoResult<()> {
            std::fs::remove_file(path).map_err(|_| IoErrorKind::WriteFailed)
        }
    }
}

#[cfg(feature = "std")]
pub use std_io::StdIoProvider;
