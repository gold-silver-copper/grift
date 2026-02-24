//! I/O trait boundary for the Grift Lisp evaluator.
//!
//! This module defines the [`IoProvider`] trait, which serves as the boundary
//! between the pure `no_std` evaluator and platform-specific I/O implementations.
//!
//! ## Stream-Based Model
//!
//! I/O uses a Unix-fd–inspired stream convention:
//!   - Stream 0 = stdin
//!   - Stream 1 = stdout
//!   - Stream 2 = stderr
//!   - Stream 3+ = dynamically opened file handles
//!
//! All IO builtins take a stream number as their first argument.
//!
//! ## Design
//!
//! The evaluator core is fully `no_std` and performs no I/O itself.
//! Instead, an [`IoProvider`] trait abstracts all I/O operations:
//!
//! - **[`NullIoProvider`]** — No-op implementation that silently discards
//!   output on streams 1 and 2. All other operations return `Unsupported`.
//! - **`StdIoProvider`** (behind `std` feature) — Implementation backed
//!   by `std::io`, providing real stdin/stdout/stderr and file operations
//!   with dynamic stream allocation.

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
/// All I/O is stream-oriented. Standard streams are:
///   - `0` = stdin
///   - `1` = stdout
///   - `2` = stderr
///   - `3+` = dynamically opened file handles
///
/// # Implementor Notes
///
/// Only [`write_stream`](Self::write_stream) is required. All other methods
/// have default implementations returning `Err(IoErrorKind::Unsupported)`,
/// so minimal embedded implementations only need the write method.
///
/// # Example
///
/// ```rust,ignore
/// use grift::io::{IoProvider, IoResult};
///
/// struct UartIo;
///
/// impl IoProvider for UartIo {
///     fn write_stream(&mut self, stream: u8, s: &str) -> IoResult<()> {
///         match stream {
///             1 | 2 => { /* write to UART */ Ok(()) }
///             _ => Err(IoErrorKind::Unsupported),
///         }
///     }
/// }
/// ```
pub trait IoProvider {
    /// Write a string slice to the given stream.
    fn write_stream(&mut self, stream: u8, s: &str) -> IoResult<()>;

    /// Read a single character from the given stream.
    fn read_stream_char(&mut self, _stream: u8) -> IoResult<char> {
        Err(IoErrorKind::Unsupported)
    }

    /// Peek at the next character on the given stream without consuming it.
    fn peek_stream_char(&mut self, _stream: u8) -> IoResult<char> {
        Err(IoErrorKind::Unsupported)
    }

    /// Open a file and return a stream number.
    ///
    /// `mode`: 0 = open for reading, 1 = open for writing (create/truncate).
    fn open_file(&mut self, _path: &str, _mode: u8) -> IoResult<u8> {
        Err(IoErrorKind::Unsupported)
    }

    /// Close a dynamic stream (error on 0/1/2).
    fn close_stream(&mut self, _stream: u8) -> IoResult<()> {
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

/// A no-op I/O provider that silently discards output on streams 1 and 2.
///
/// Useful as a default when no I/O back-end is configured,
/// or in `no_std`/embedded contexts where there is no real I/O.
pub struct NullIoProvider;

impl IoProvider for NullIoProvider {
    fn write_stream(&mut self, stream: u8, _s: &str) -> IoResult<()> {
        match stream {
            1 | 2 => Ok(()),
            _ => Err(IoErrorKind::Unsupported),
        }
    }
}

// ============================================================================
// Streaming writer types
// ============================================================================

/// Streams output to a specific stream through a borrowed [`IoProvider`].
///
/// Used by `raw-display` and `raw-write` builtins to walk the value tree
/// and emit output without any intermediate buffer.
pub(crate) struct StreamWriter<'a, IO: IoProvider> {
    pub(crate) io: &'a core::cell::RefCell<IO>,
    pub(crate) stream: u8,
    pub(crate) error: Option<IoErrorKind>,
}

impl<IO: IoProvider> core::fmt::Write for StreamWriter<'_, IO> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.error.is_some() {
            return Err(core::fmt::Error);
        }
        if let Err(e) = self.io.borrow_mut().write_stream(self.stream, s) {
            self.error = Some(e);
            return Err(core::fmt::Error);
        }
        Ok(())
    }
}

// ============================================================================
// StdIoProvider — std-backed implementation with dynamic streams
// ============================================================================

#[cfg(feature = "std")]
mod std_io {
    extern crate std;

    use std::io::{BufReader, BufWriter, Read, Write};
    use std::vec::Vec;

    use super::{IoErrorKind, IoProvider, IoResult};

    /// A dynamically opened file handle.
    enum OpenFile {
        /// File opened for reading.
        Read { reader: BufReader<std::fs::File> },
        /// File opened for writing.
        Write { writer: BufWriter<std::fs::File> },
    }

    /// An [`IoProvider`] implementation backed by Rust's standard I/O.
    ///
    /// Provides real stdin/stdout/stderr access and dynamic file stream
    /// management. Streams 0/1/2 are implicit (stdin/stdout/stderr).
    /// Streams 3+ are dynamically allocated file handles stored in a Vec.
    pub struct StdIoProvider {
        /// Per-stream peek buffers (streams 0–255).
        peek_buf: [Option<char>; 256],
        /// Dynamically opened file handles (index = stream - 3).
        streams: Vec<Option<OpenFile>>,
    }

    impl StdIoProvider {
        /// Create a new [`StdIoProvider`].
        pub fn new() -> Self {
            StdIoProvider {
                peek_buf: [None; 256],
                streams: Vec::new(),
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
        fn write_stream(&mut self, stream: u8, s: &str) -> IoResult<()> {
            match stream {
                1 => std::io::stdout()
                    .write_all(s.as_bytes())
                    .map_err(|_| IoErrorKind::WriteFailed),
                2 => std::io::stderr()
                    .write_all(s.as_bytes())
                    .map_err(|_| IoErrorKind::WriteFailed),
                n if n >= 3 => {
                    let idx = (n - 3) as usize;
                    match self.streams.get_mut(idx) {
                        Some(Some(OpenFile::Write { writer })) => {
                            writer.write_all(s.as_bytes())
                                .map_err(|_| IoErrorKind::WriteFailed)
                        }
                        _ => Err(IoErrorKind::Unsupported),
                    }
                }
                _ => Err(IoErrorKind::Unsupported), // stream 0 is not writable
            }
        }

        fn read_stream_char(&mut self, stream: u8) -> IoResult<char> {
            // Check peek buffer first
            if let Some(c) = self.peek_buf[stream as usize].take() {
                return Ok(c);
            }
            match stream {
                0 => {
                    let stdin = std::io::stdin();
                    let mut handle = stdin.lock();
                    read_one_char_from(&mut handle)
                }
                n if n >= 3 => {
                    let idx = (n - 3) as usize;
                    match self.streams.get_mut(idx) {
                        Some(Some(OpenFile::Read { reader })) => {
                            read_one_char_from(reader)
                        }
                        _ => Err(IoErrorKind::Unsupported),
                    }
                }
                _ => Err(IoErrorKind::Unsupported), // 1/2 are not readable
            }
        }

        fn peek_stream_char(&mut self, stream: u8) -> IoResult<char> {
            if let Some(c) = self.peek_buf[stream as usize] {
                return Ok(c);
            }
            let c = self.read_stream_char(stream)?;
            self.peek_buf[stream as usize] = Some(c);
            Ok(c)
        }

        fn open_file(&mut self, path: &str, mode: u8) -> IoResult<u8> {
            let file = match mode {
                0 => {
                    let f = std::fs::File::open(path).map_err(|_| IoErrorKind::ReadFailed)?;
                    OpenFile::Read { reader: BufReader::new(f) }
                }
                1 => {
                    let f = std::fs::File::create(path).map_err(|_| IoErrorKind::WriteFailed)?;
                    OpenFile::Write { writer: BufWriter::new(f) }
                }
                _ => return Err(IoErrorKind::Unsupported),
            };
            // Find first empty slot or push
            for (i, slot) in self.streams.iter_mut().enumerate() {
                if slot.is_none() {
                    *slot = Some(file);
                    return Ok((i + 3) as u8);
                }
            }
            let id = self.streams.len() + 3;
            if id > 255 {
                return Err(IoErrorKind::Unsupported);
            }
            self.streams.push(Some(file));
            Ok(id as u8)
        }

        fn close_stream(&mut self, stream: u8) -> IoResult<()> {
            if stream < 3 {
                return Err(IoErrorKind::Unsupported);
            }
            let idx = (stream - 3) as usize;
            if idx < self.streams.len() && self.streams[idx].is_some() {
                self.streams[idx] = None;
                self.peek_buf[stream as usize] = None;
                Ok(())
            } else {
                Err(IoErrorKind::Unsupported)
            }
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
