//! Standard I/O provider backed by Rust's `std::io`.
//!
//! [`StdIoProvider`] implements [`IoProvider`] for the three standard ports
//! (stdin, stdout, stderr) using the locked handles from `std::io`.

use std::io::{self, BufRead, Read, Write};

use grift_core::{IoErrorKind, IoProvider, IoResult, PortId};

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
/// Additional (file) ports are not yet supported; attempts to use an
/// unknown [`PortId`] return [`IoErrorKind::InvalidPort`].
pub struct StdIoProvider {
    /// One-character peek buffer for stdin.
    /// `None` means nothing has been peeked.
    peeked: Option<char>,
}

impl StdIoProvider {
    /// Create a new [`StdIoProvider`].
    pub fn new() -> Self {
        StdIoProvider { peeked: None }
    }
}

impl Default for StdIoProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read one UTF-8 character from stdin (locked).
fn read_one_char() -> IoResult<char> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    // Read bytes until we have a valid UTF-8 character.
    let mut buf = [0u8; 4];
    let first = {
        let n = handle.read(&mut buf[..1]).map_err(|_| IoErrorKind::ReadFailed)?;
        if n == 0 {
            return Err(IoErrorKind::Eof);
        }
        buf[0]
    };

    // Determine expected UTF-8 byte length from the leading byte.
    let char_len = if first < 0x80 {
        1
    } else if first < 0xE0 {
        2
    } else if first < 0xF0 {
        3
    } else {
        4
    };

    // Read remaining continuation bytes if needed.
    if char_len > 1 {
        let remaining = &mut buf[1..char_len];
        handle
            .read_exact(remaining)
            .map_err(|_| IoErrorKind::ReadFailed)?;
    }

    core::str::from_utf8(&buf[..char_len])
        .ok()
        .and_then(|s| s.chars().next())
        .ok_or(IoErrorKind::ReadFailed)
}

// ---------------------------------------------------------------------------
// IoProvider implementation
// ---------------------------------------------------------------------------

impl IoProvider for StdIoProvider {
    fn read_char(&mut self, port: PortId) -> IoResult<char> {
        if port != PortId::STDIN {
            return Err(IoErrorKind::InvalidPort);
        }
        if let Some(c) = self.peeked.take() {
            return Ok(c);
        }
        read_one_char()
    }

    fn peek_char(&mut self, port: PortId) -> IoResult<char> {
        if port != PortId::STDIN {
            return Err(IoErrorKind::InvalidPort);
        }
        if let Some(c) = self.peeked {
            return Ok(c);
        }
        let c = read_one_char()?;
        self.peeked = Some(c);
        Ok(c)
    }

    fn char_ready(&mut self, port: PortId) -> IoResult<bool> {
        if port != PortId::STDIN {
            return Err(IoErrorKind::InvalidPort);
        }
        if self.peeked.is_some() {
            return Ok(true);
        }
        // Best-effort: check if the stdin buffer has data.
        let stdin = io::stdin();
        let mut handle = stdin.lock();
        Ok(!handle.fill_buf().map_or(true, |b| b.is_empty()))
    }

    fn write_char(&mut self, port: PortId, c: char) -> IoResult<()> {
        let mut buf = [0u8; 4];
        let encoded = c.encode_utf8(&mut buf);
        self.write_str(port, encoded)
    }

    fn write_str(&mut self, port: PortId, s: &str) -> IoResult<()> {
        match port {
            PortId::STDOUT => io::stdout()
                .write_all(s.as_bytes())
                .map_err(|_| IoErrorKind::WriteFailed),
            PortId::STDERR => io::stderr()
                .write_all(s.as_bytes())
                .map_err(|_| IoErrorKind::WriteFailed),
            _ => Err(IoErrorKind::InvalidPort),
        }
    }

    fn flush(&mut self, port: PortId) -> IoResult<()> {
        match port {
            PortId::STDOUT => io::stdout().flush().map_err(|_| IoErrorKind::WriteFailed),
            PortId::STDERR => io::stderr().flush().map_err(|_| IoErrorKind::WriteFailed),
            _ => Err(IoErrorKind::InvalidPort),
        }
    }

    fn close_port(&mut self, _port: PortId) -> IoResult<()> {
        // Standard ports cannot be closed in this implementation.
        Err(IoErrorKind::Unsupported)
    }

    fn is_input_port(&self, port: PortId) -> bool {
        port == PortId::STDIN
    }

    fn is_output_port(&self, port: PortId) -> bool {
        port == PortId::STDOUT || port == PortId::STDERR
    }
}
