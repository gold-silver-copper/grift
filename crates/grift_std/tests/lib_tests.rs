//! Tests for the grift_std crate.

use grift_core::{IoProvider, IoErrorKind, PortId};
use grift_std::StdIoProvider;

#[test]
fn std_io_provider_port_classification() {
    let io = StdIoProvider::new();
    assert!(io.is_input_port(PortId::STDIN));
    assert!(!io.is_output_port(PortId::STDIN));
    assert!(io.is_output_port(PortId::STDOUT));
    assert!(!io.is_input_port(PortId::STDOUT));
    assert!(io.is_output_port(PortId::STDERR));
    assert!(!io.is_input_port(PortId::STDERR));
}

#[test]
fn std_io_provider_invalid_port_read() {
    let mut io = StdIoProvider::new();
    assert_eq!(io.read_char(PortId::STDOUT), Err(IoErrorKind::InvalidPort));
    assert_eq!(io.peek_char(PortId::STDERR), Err(IoErrorKind::InvalidPort));
}

#[test]
fn std_io_provider_invalid_port_write() {
    let mut io = StdIoProvider::new();
    assert_eq!(io.write_str(PortId::STDIN, "hello"), Err(IoErrorKind::InvalidPort));
}

#[test]
fn std_io_provider_write_stdout() {
    let mut io = StdIoProvider::new();
    // Writing to stdout should succeed (output goes to test harness).
    assert!(io.write_str(PortId::STDOUT, "test output").is_ok());
    assert!(io.write_char(PortId::STDOUT, '\n').is_ok());
    assert!(io.flush(PortId::STDOUT).is_ok());
}

#[test]
fn std_io_provider_write_stderr() {
    let mut io = StdIoProvider::new();
    assert!(io.write_str(PortId::STDERR, "test error").is_ok());
    assert!(io.flush(PortId::STDERR).is_ok());
}

#[test]
fn std_io_provider_close_unsupported() {
    let mut io = StdIoProvider::new();
    assert_eq!(io.close_port(PortId::STDIN), Err(IoErrorKind::Unsupported));
    assert_eq!(io.close_port(PortId::STDOUT), Err(IoErrorKind::Unsupported));
}

#[test]
fn null_io_provider_basics() {
    use grift_core::NullIoProvider;

    let mut io = NullIoProvider;
    // Reads are unsupported
    assert_eq!(io.read_char(PortId::STDIN), Err(IoErrorKind::Unsupported));
    assert_eq!(io.peek_char(PortId::STDIN), Err(IoErrorKind::Unsupported));
    assert_eq!(io.char_ready(PortId::STDIN), Err(IoErrorKind::Unsupported));
    // Writes silently succeed
    assert!(io.write_str(PortId::STDOUT, "hello").is_ok());
    assert!(io.write_char(PortId::STDOUT, 'x').is_ok());
    assert!(io.flush(PortId::STDOUT).is_ok());
    assert!(io.close_port(PortId::STDOUT).is_ok());
    // Port classification
    assert!(!io.is_input_port(PortId::STDIN));
    assert!(!io.is_output_port(PortId::STDOUT));
}
