//! I/O boundary for the Grift Lisp evaluator.
//!
//! This module defines [`IoState`], a concrete struct of function pointers
//! that serves as the boundary between the pure `no_std` evaluator and
//! platform-specific I/O implementations.
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
//! Instead, an [`IoState`] struct of function pointers abstracts all
//! I/O operations:
//!
//! - **[`IoState::null`]** — No-op configuration that silently discards
//!   output on streams 1 and 2. All other operations return `Unsupported`.
//! - **[`IoState::std_io`]** (behind `std` feature) — Configuration backed
//!   by `std::io`, providing real stdin/stdout/stderr and file operations
//!   with dynamic stream allocation via thread-local state.

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

/// Concrete I/O state using function pointers.
///
/// All I/O is stream-oriented. Standard streams are:
///   - `0` = stdin
///   - `1` = stdout
///   - `2` = stderr
///   - `3+` = dynamically opened file handles
///
/// # Example
///
/// ```rust,ignore
/// use grift::io::IoState;
///
/// fn uart_write(stream: u8, s: &str) -> IoResult<()> {
///     match stream {
///         1 | 2 => { /* write to UART */ Ok(()) }
///         _ => Err(IoErrorKind::Unsupported),
///     }
/// }
///
/// let io = IoState { write_stream: uart_write, ..IoState::null() };
/// ```
pub struct IoState {
    /// Write a string slice to the given stream.
    pub write_stream: fn(u8, &str) -> IoResult<()>,
    /// Read a single character from the given stream.
    pub read_stream_char: fn(u8) -> IoResult<char>,
    /// Peek at the next character on the given stream without consuming it.
    pub peek_stream_char: fn(u8) -> IoResult<char>,
    /// Open a file and return a stream number.
    /// `mode`: 0 = open for reading, 1 = open for writing (create/truncate).
    pub open_file: fn(&str, u8) -> IoResult<u8>,
    /// Close a dynamic stream (error on 0/1/2).
    pub close_stream: fn(u8) -> IoResult<()>,
    /// Check whether a file exists at the given path.
    pub file_exists: fn(&str) -> IoResult<bool>,
    /// Delete the file at the given path.
    pub delete_file: fn(&str) -> IoResult<()>,
}

fn null_write_stream(stream: u8, _s: &str) -> IoResult<()> {
    match stream {
        1 | 2 => Ok(()),
        _ => Err(IoErrorKind::Unsupported),
    }
}

fn unsupported_read_char(_stream: u8) -> IoResult<char> {
    Err(IoErrorKind::Unsupported)
}

fn unsupported_open_file(_path: &str, _mode: u8) -> IoResult<u8> {
    Err(IoErrorKind::Unsupported)
}

fn unsupported_close_stream(_stream: u8) -> IoResult<()> {
    Err(IoErrorKind::Unsupported)
}

fn unsupported_file_exists(_path: &str) -> IoResult<bool> {
    Err(IoErrorKind::Unsupported)
}

fn unsupported_delete_file(_path: &str) -> IoResult<()> {
    Err(IoErrorKind::Unsupported)
}

impl IoState {
    /// Create a no-op I/O state that silently discards output on streams 1 and 2.
    ///
    /// All other operations return `Err(IoErrorKind::Unsupported)`.
    /// Useful as a default when no I/O back-end is configured,
    /// or in `no_std`/embedded contexts where there is no real I/O.
    pub fn null() -> Self {
        IoState {
            write_stream: null_write_stream,
            read_stream_char: unsupported_read_char,
            peek_stream_char: unsupported_read_char,
            open_file: unsupported_open_file,
            close_stream: unsupported_close_stream,
            file_exists: unsupported_file_exists,
            delete_file: unsupported_delete_file,
        }
    }
}

impl Default for IoState {
    fn default() -> Self {
        Self::null()
    }
}

// ============================================================================
// Streaming writer types
// ============================================================================

/// Streams output to a specific stream through a borrowed [`IoState`].
///
/// Used by `raw-display` and `raw-write` builtins to walk the value tree
/// and emit output without any intermediate buffer.
pub(crate) struct StreamWriter<'a> {
    pub(crate) io: &'a core::cell::RefCell<IoState>,
    pub(crate) stream: u8,
    pub(crate) error: Option<IoErrorKind>,
}

impl core::fmt::Write for StreamWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.error.is_some() {
            return Err(core::fmt::Error);
        }
        if let Err(e) = (self.io.borrow().write_stream)(self.stream, s) {
            self.error = Some(e);
            return Err(core::fmt::Error);
        }
        Ok(())
    }
}

// ============================================================================
// std_io — std-backed I/O functions with thread-local dynamic streams
// ============================================================================

#[cfg(feature = "std")]
mod std_io {
    extern crate std;

    use std::io::{BufReader, BufWriter, Read, Write};
    use std::vec::Vec;

    use super::{IoErrorKind, IoResult, IoState};

    /// A dynamically opened file handle.
    enum OpenFile {
        /// File opened for reading.
        Read { reader: BufReader<std::fs::File> },
        /// File opened for writing.
        Write { writer: BufWriter<std::fs::File> },
    }

    /// Thread-local state for std I/O: peek buffers and dynamic file streams.
    struct StdIoState {
        /// Per-stream peek buffers (streams 0–255).
        peek_buf: [Option<char>; 256],
        /// Dynamically opened file handles (index = stream - 3).
        streams: Vec<Option<OpenFile>>,
    }

    std::thread_local! {
        static STD_IO: std::cell::RefCell<StdIoState> = std::cell::RefCell::new(StdIoState {
            peek_buf: [None; 256],
            streams: Vec::new(),
        });
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

    fn std_write_stream(stream: u8, s: &str) -> IoResult<()> {
        match stream {
            1 => std::io::stdout()
                .write_all(s.as_bytes())
                .map_err(|_| IoErrorKind::WriteFailed),
            2 => std::io::stderr()
                .write_all(s.as_bytes())
                .map_err(|_| IoErrorKind::WriteFailed),
            n if n >= 3 => STD_IO.with(|state: &std::cell::RefCell<StdIoState>| {
                let mut state = state.borrow_mut();
                let idx = (n - 3) as usize;
                match state.streams.get_mut(idx) {
                    Some(Some(OpenFile::Write { writer })) => {
                        writer.write_all(s.as_bytes())
                            .map_err(|_| IoErrorKind::WriteFailed)
                    }
                    _ => Err(IoErrorKind::Unsupported),
                }
            }),
            _ => Err(IoErrorKind::Unsupported), // stream 0 is not writable
        }
    }

    fn std_read_stream_char(stream: u8) -> IoResult<char> {
        STD_IO.with(|state: &std::cell::RefCell<StdIoState>| {
            let mut state = state.borrow_mut();
            // Check peek buffer first
            if let Some(c) = state.peek_buf[stream as usize].take() {
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
                    match state.streams.get_mut(idx) {
                        Some(Some(OpenFile::Read { reader })) => {
                            read_one_char_from(reader)
                        }
                        _ => Err(IoErrorKind::Unsupported),
                    }
                }
                _ => Err(IoErrorKind::Unsupported), // 1/2 are not readable
            }
        })
    }

    fn std_peek_stream_char(stream: u8) -> IoResult<char> {
        STD_IO.with(|state: &std::cell::RefCell<StdIoState>| {
            let mut state = state.borrow_mut();
            if let Some(c) = state.peek_buf[stream as usize] {
                return Ok(c);
            }
            // Need to read a char and store it in peek buffer
            let c = match stream {
                0 => {
                    let stdin = std::io::stdin();
                    let mut handle = stdin.lock();
                    read_one_char_from(&mut handle)
                }
                n if n >= 3 => {
                    let idx = (n - 3) as usize;
                    match state.streams.get_mut(idx) {
                        Some(Some(OpenFile::Read { reader })) => {
                            read_one_char_from(reader)
                        }
                        _ => return Err(IoErrorKind::Unsupported),
                    }
                }
                _ => return Err(IoErrorKind::Unsupported),
            }?;
            state.peek_buf[stream as usize] = Some(c);
            Ok(c)
        })
    }

    fn std_open_file(path: &str, mode: u8) -> IoResult<u8> {
        STD_IO.with(|state: &std::cell::RefCell<StdIoState>| {
            let mut state = state.borrow_mut();
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
            for (i, slot) in state.streams.iter_mut().enumerate() {
                if slot.is_none() {
                    *slot = Some(file);
                    return Ok((i + 3) as u8);
                }
            }
            let id = state.streams.len() + 3;
            if id > 255 {
                return Err(IoErrorKind::Unsupported);
            }
            state.streams.push(Some(file));
            Ok(id as u8)
        })
    }

    fn std_close_stream(stream: u8) -> IoResult<()> {
        if stream < 3 {
            return Err(IoErrorKind::Unsupported);
        }
        STD_IO.with(|state: &std::cell::RefCell<StdIoState>| {
            let mut state = state.borrow_mut();
            let idx = (stream - 3) as usize;
            if idx < state.streams.len() && state.streams[idx].is_some() {
                state.streams[idx] = None;
                state.peek_buf[stream as usize] = None;
                Ok(())
            } else {
                Err(IoErrorKind::Unsupported)
            }
        })
    }

    fn std_file_exists(path: &str) -> IoResult<bool> {
        Ok(std::path::Path::new(path).exists())
    }

    fn std_delete_file(path: &str) -> IoResult<()> {
        std::fs::remove_file(path).map_err(|_| IoErrorKind::WriteFailed)
    }

    impl IoState {
        /// Create an [`IoState`] backed by Rust's standard I/O.
        ///
        /// Provides real stdin/stdout/stderr access and dynamic file stream
        /// management via thread-local state. Streams 0/1/2 are implicit
        /// (stdin/stdout/stderr). Streams 3+ are dynamically allocated file
        /// handles.
        ///
        /// Resets any thread-local file stream state from previous calls.
        pub fn std_io() -> Self {
            // Reset thread-local state for a fresh start
            STD_IO.with(|state: &std::cell::RefCell<StdIoState>| {
                let mut state = state.borrow_mut();
                state.peek_buf = [None; 256];
                state.streams.clear();
            });
            IoState {
                write_stream: std_write_stream,
                read_stream_char: std_read_stream_char,
                peek_stream_char: std_peek_stream_char,
                open_file: std_open_file,
                close_stream: std_close_stream,
                file_exists: std_file_exists,
                delete_file: std_delete_file,
            }
        }
    }
}
