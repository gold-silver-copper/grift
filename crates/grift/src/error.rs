//! Error types.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use crate::value::Value;

/// An error raised inside the machine: delivered to the innermost
/// `error-prompt` as `(k kind message irritants)`.
#[derive(Clone, Debug)]
pub(crate) struct Fault {
    pub(crate) kind: Cow<'static, str>,
    pub(crate) message: String,
    pub(crate) irritants: Vec<Value>,
}

impl Fault {
    pub(crate) fn new(kind: impl Into<Cow<'static, str>>, message: impl Into<String>) -> Self {
        Fault {
            kind: kind.into(),
            message: message.into(),
            irritants: Vec::new(),
        }
    }

    /// Add an irritant.
    pub(crate) fn with(mut self, irritant: Value) -> Self {
        self.irritants.push(irritant);
        self
    }
}

/// Result type used inside the interpreter.
pub(crate) type GResult<T> = Result<T, Fault>;

/// An error that reached the host, or an error raised by a native function.
///
/// `kind` is the error kind symbol (e.g. `type-error`), `message` a
/// human-readable description, and `irritants` the written representations
/// of the offending values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// The error kind, e.g. `type-error` or `parse-error`.
    pub kind: String,
    /// Human-readable description.
    pub message: String,
    /// Written representations of the values involved.
    pub irritants: Vec<String>,
}

impl Error {
    /// Create an error with no irritants.
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Error {
            kind: kind.into(),
            message: message.into(),
            irritants: Vec::new(),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)?;
        for irritant in &self.irritants {
            write!(f, " {irritant}")?;
        }
        Ok(())
    }
}

impl core::error::Error for Error {}
