//! Standard I/O provider backed by Rust's `std::io`.
//!
//! [`StdIoProvider`] implements [`IoProvider`] for the three standard ports
//! (stdin, stdout, stderr) using the locked handles from `std::io`.
//! It also supports dynamically opened string ports via
//! [`open_input_string`](IoProvider::open_input_string) and
//! [`open_output_string`](IoProvider::open_output_string).

use std::io::{self, BufRead, Read, Write};
use std::env;

use grift_core::{IoErrorKind, IoProvider, IoResult, PortId};

/// First dynamically-allocated port id (after STDIN=0, STDOUT=1, STDERR=2).
const DYNAMIC_PORT_BASE: usize = 3;

/// Maximum number of simultaneously open dynamic ports.
const MAX_DYNAMIC_PORTS: usize = 64;

/// A dynamically-opened port.
enum DynPort {
    /// Input port backed by a string buffer with a read cursor.
    InputString { data: Vec<char>, cursor: usize, closed: bool },
    /// Output port that accumulates characters.
    OutputString { buf: String, closed: bool },
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
/// Additionally supports dynamically opened string ports.
pub struct StdIoProvider {
    /// One-character peek buffer for stdin.
    /// `None` means nothing has been peeked.
    peeked: Option<char>,
    /// Dynamically opened ports.
    ports: Vec<Option<DynPort>>,
    /// Cached command-line arguments.
    command_line_args: Vec<String>,
    /// Cached environment variables (name, value).
    env_vars: Vec<(String, String)>,
    /// Temporary buffer for a single env var lookup.
    env_var_buf: Option<String>,
    /// Temporary buffer for file content (used by read_file).
    file_buf: String,
}

impl StdIoProvider {
    /// Create a new [`StdIoProvider`].
    pub fn new() -> Self {
        StdIoProvider {
            peeked: None,
            ports: Vec::new(),
            command_line_args: env::args().collect(),
            env_vars: Vec::new(),
            env_var_buf: None,
            file_buf: String::new(),
        }
    }

    /// Allocate a fresh [`PortId`] and store the given dynamic port.
    fn alloc_port(&mut self, port: DynPort) -> IoResult<PortId> {
        // Try to reuse a freed slot
        for (i, slot) in self.ports.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(port);
                return Ok(PortId(DYNAMIC_PORT_BASE + i));
            }
        }
        // Allocate a new slot
        if self.ports.len() >= MAX_DYNAMIC_PORTS {
            return Err(IoErrorKind::Unsupported);
        }
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
    } else if (0xC2..0xE0).contains(&first) {
        2
    } else if (0xE0..0xF0).contains(&first) {
        3
    } else if (0xF0..0xF5).contains(&first) {
        4
    } else {
        // Invalid leading byte (0x80-0xBF are continuation bytes,
        // 0xC0-0xC1 are overlong, 0xF5-0xFF are invalid).
        return Err(IoErrorKind::ReadFailed);
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
        if port == PortId::STDIN {
            if let Some(c) = self.peeked.take() {
                return Ok(c);
            }
            return read_one_char();
        }
        // Dynamic port
        match self.get_dyn_mut(port) {
            Some(DynPort::InputString { data, cursor, closed }) => {
                if *closed { return Err(IoErrorKind::PortClosed); }
                if *cursor >= data.len() { return Err(IoErrorKind::Eof); }
                let c = data[*cursor];
                *cursor += 1;
                Ok(c)
            }
            _ => Err(IoErrorKind::InvalidPort),
        }
    }

    fn peek_char(&mut self, port: PortId) -> IoResult<char> {
        if port == PortId::STDIN {
            if let Some(c) = self.peeked {
                return Ok(c);
            }
            let c = read_one_char()?;
            self.peeked = Some(c);
            return Ok(c);
        }
        // Dynamic port
        match self.get_dyn(port) {
            Some(DynPort::InputString { data, cursor, closed }) => {
                if *closed { return Err(IoErrorKind::PortClosed); }
                if *cursor >= data.len() { return Err(IoErrorKind::Eof); }
                Ok(data[*cursor])
            }
            _ => Err(IoErrorKind::InvalidPort),
        }
    }

    fn char_ready(&mut self, port: PortId) -> IoResult<bool> {
        if port == PortId::STDIN {
            if self.peeked.is_some() {
                return Ok(true);
            }
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            return Ok(!handle.fill_buf().map_or(true, |b| b.is_empty()));
        }
        // Dynamic port
        match self.get_dyn(port) {
            Some(DynPort::InputString { data, cursor, closed }) => {
                Ok(!closed && *cursor < data.len())
            }
            _ => Err(IoErrorKind::InvalidPort),
        }
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
            _ => {
                // Dynamic port
                match self.get_dyn_mut(port) {
                    Some(DynPort::OutputString { buf, closed }) => {
                        if *closed { return Err(IoErrorKind::PortClosed); }
                        buf.push_str(s);
                        Ok(())
                    }
                    _ => Err(IoErrorKind::InvalidPort),
                }
            }
        }
    }

    fn flush(&mut self, port: PortId) -> IoResult<()> {
        match port {
            PortId::STDOUT => io::stdout().flush().map_err(|_| IoErrorKind::WriteFailed),
            PortId::STDERR => io::stderr().flush().map_err(|_| IoErrorKind::WriteFailed),
            _ => Ok(()), // String ports don't need flushing
        }
    }

    fn close_port(&mut self, port: PortId) -> IoResult<()> {
        if port.0 < DYNAMIC_PORT_BASE {
            return Err(IoErrorKind::Unsupported); // Can't close standard ports
        }
        let idx = port.0 - DYNAMIC_PORT_BASE;
        match self.ports.get_mut(idx) {
            Some(Some(DynPort::InputString { closed, .. })) => { *closed = true; Ok(()) }
            Some(Some(DynPort::OutputString { closed, .. })) => { *closed = true; Ok(()) }
            _ => Err(IoErrorKind::InvalidPort),
        }
    }

    fn is_input_port(&self, port: PortId) -> bool {
        if port == PortId::STDIN { return true; }
        matches!(self.get_dyn(port), Some(DynPort::InputString { .. }))
    }

    fn is_output_port(&self, port: PortId) -> bool {
        if port == PortId::STDOUT || port == PortId::STDERR { return true; }
        matches!(self.get_dyn(port), Some(DynPort::OutputString { .. }))
    }

    fn is_port_open(&self, port: PortId) -> bool {
        // Standard ports are always open
        if port.0 < DYNAMIC_PORT_BASE { return true; }
        match self.get_dyn(port) {
            Some(DynPort::InputString { closed, .. }) => !closed,
            Some(DynPort::OutputString { closed, .. }) => !closed,
            None => false,
        }
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

    fn file_exists(&self, path: &str) -> IoResult<bool> {
        Ok(std::path::Path::new(path).exists())
    }

    fn delete_file(&mut self, path: &str) -> IoResult<()> {
        std::fs::remove_file(path).map_err(|_| IoErrorKind::WriteFailed)
    }

    fn read_file(&mut self, path: &str) -> IoResult<&str> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                self.file_buf = content;
                Ok(self.file_buf.as_str())
            }
            Err(_) => Err(IoErrorKind::ReadFailed),
        }
    }

    fn command_line_count(&self) -> IoResult<usize> {
        Ok(self.command_line_args.len())
    }

    fn command_line_arg(&self, index: usize) -> IoResult<&str> {
        self.command_line_args.get(index)
            .map(|s| s.as_str())
            .ok_or(IoErrorKind::ReadFailed)
    }

    fn get_environment_variable(&mut self, name: &str) -> IoResult<Option<&str>> {
        match env::var(name) {
            Ok(val) => {
                self.env_var_buf = Some(val);
                Ok(self.env_var_buf.as_deref())
            }
            Err(_) => Ok(None),
        }
    }

    fn environment_variables_count(&mut self) -> IoResult<usize> {
        self.env_vars = env::vars().collect();
        Ok(self.env_vars.len())
    }

    fn environment_variable_at(&self, index: usize) -> IoResult<(&str, &str)> {
        self.env_vars.get(index)
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .ok_or(IoErrorKind::ReadFailed)
    }

    fn exit_process(&mut self, code: i32) -> IoResult<()> {
        std::process::exit(code);
    }

    fn emergency_exit_process(&mut self, code: i32) -> IoResult<()> {
        // Use abort() to bypass cleanup (destructors, atexit handlers)
        // per R7RS semantics for emergency-exit
        let _ = code; // abort doesn't support exit codes
        std::process::abort();
    }
}
