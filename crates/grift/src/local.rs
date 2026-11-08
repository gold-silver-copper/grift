//! The interpreter's own values, as the machine and the heap hold them: small, `Copy`, and
//! meaningful only in the [`Grift`](crate::Grift) that made them. The public types in the crate's
//! root carry that interpreter's identity besides, and are checked and converted to these at every
//! entry (see `Grift::import`), so nothing stale or foreign ever gets in.

use crate::evaluate::Primitive;
use crate::heap::{Reference, Seen};
use std::fmt;

/// A value of this interpreter. The variants are the public [`Value`](crate::Value)'s. A whole word
/// for the tag, so writing one writes its first word at once: a copy loads it whole, and a load
/// can't take its data from a one-byte write just before. About 16% off every program on x86-64.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u64)]
pub(crate) enum Value {
    EmptyList,
    Boolean(bool),
    Inert,
    Ignore,
    Integer(i64),
    Float(f64),
    Symbol(Symbol),
    Pair(Reference),
    String(Reference),
    Context(Reference),
    Operative(Reference),
    Applicative(Reference),
    Primitive(Primitive),
    Host(Host),
    Bytes(Reference),
}

// Two words, and aligned to one: every step copies values, so a bigger one costs every program.
// The size can't show the tag's width (a one-byte tag pads to the same 16 bytes):
// `a_values_tag_is_a_whole_word` checks that.
const _: () = assert!(size_of::<Value>() == 16 && align_of::<Value>() == 8);

/// An interned symbol: its index among the names.
#[derive(Clone, Copy, Debug, Eq, Hash)]
#[cfg_attr(not(test), derive(PartialEq))]
// The `eq` below only counts as well: it agrees with the hash.
#[cfg_attr(test, allow(clippy::derived_hash_with_manual_eq))]
pub(crate) struct Symbol(pub u32);

#[cfg(test)]
thread_local! {
    /// References and symbols compared on this thread, for tests that something searches nothing:
    /// a search by value, through a list or a table with a bad hash, comes through their `eq`.
    pub(crate) static COMPARED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl PartialEq for Symbol {
    fn eq(&self, other: &Symbol) -> bool {
        COMPARED.with(|n| n.set(n.get() + 1));
        self.0 == other.0
    }
}

/// A registered host function: its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Host(pub u32);

/// An error as the machine carries it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Error {
    /// Whether the host or the machine began this, not a program: whatever its kind is called,
    /// this alone says a handler may not catch it, and a program can't set it.
    pub limit: bool,
    /// Whether memory really was refused to make this: then what the frames it unwinds held is
    /// worth collecting. A fact about how it began, which no name and no host can claim.
    pub refused: bool,
    pub kind: Symbol,
    pub values: Vec<Value>,
    pub trace: Vec<Value>,
    /// The combinations in `trace`, so adding one searches nothing.
    pub seen: Seen,
}

/// Declares the symbols every [`Grift`](crate::Grift) interns first, in this order, so the core
/// can name them as constants.
macro_rules! known_symbols {
    ($($k:ident $name:literal)*) => {
        #[derive(Clone, Copy)]
        pub(crate) enum K { $($k),* }
        pub(crate) const K_NAMES: &[&str] = &[$($name),*];
    };
}

known_symbols! {
    UnboundSymbol "unbound-symbol" TypeError "type-error" ArityError "arity-error"
    NotACombiner "not-a-combiner" ParameterError "parameter-error" Overflow "overflow"
    DivisionByZero "division-by-zero" OutOfMemory "out-of-memory" ParseError "parse-error"
    StaleValue "stale-value" ForeignValue "foreign-value" Interrupted "interrupted"
    RangeError "range-error" CannotPark "cannot-park" OutputError "output-error"
    EmptyList "empty-list" Boolean "boolean" Inert "inert" Ignore "ignore" Integer "integer"
    Float "float" Symbol "symbol" Pair "pair" String "string" Context "context"
    Operative "operative" Applicative "applicative" Kind "kind" Values "values" Trace "trace"
    UnclosedList "unclosed-list" UnclosedString "unclosed-string"
    UnmatchedParenthesis "unmatched-parenthesis" MisplacedDot "misplaced-dot"
    BadEscape "bad-escape" BadNumber "bad-number" UnknownConstant "unknown-constant"
    Bytes "bytes" BadByte "bad-byte" EncodingError "encoding-error"
}

impl From<K> for Symbol {
    fn from(k: K) -> Symbol {
        Symbol(k as u32)
    }
}

impl Error {
    /// An error with a copy of `values`; if even that copy can't be allocated, `out-of-memory`.
    pub fn new(kind: impl Into<Symbol>, values: &[Value]) -> Error {
        let mut e = Error::oom();
        if e.values.try_reserve_exact(values.len()).is_ok() {
            e.limit = false;
            e.refused = false;
            e.kind = kind.into();
            e.values.extend_from_slice(values);
        }
        e
    }

    /// An error a program describes, from its parts: `seen` follows from `trace`.
    pub fn from_parts(kind: Symbol, values: Vec<Value>, trace: Vec<Value>) -> Result<Error, Error> {
        let mut seen = Seen::default();
        seen.try_reserve(trace.len()).map_err(|_| Error::oom())?;
        seen.extend(trace.iter().filter_map(|v| match v {
            Value::Pair(r) => Some(*r),
            _ => None,
        }));
        Ok(Error {
            limit: false,
            refused: false,
            kind,
            values,
            trace,
            seen,
        })
    }

    /// The machine ran out of memory.
    pub fn oom() -> Error {
        Error {
            limit: true,
            refused: true,
            kind: K::OutOfMemory.into(),
            values: Vec::new(),
            trace: Vec::new(),
            seen: Seen::default(),
        }
    }

    /// The run was stopped.
    pub fn interrupted() -> Error {
        Error {
            limit: true,
            ..Error::new(K::Interrupted, &[])
        }
    }
}

/// The output refused the text.
impl From<fmt::Error> for Error {
    fn from(_: fmt::Error) -> Error {
        Error::new(K::OutputError, &[])
    }
}

/// A copy of `s`, reporting `out-of-memory` instead of aborting.
pub(crate) fn boxed(s: &str) -> Result<Box<str>, Error> {
    let mut b = String::new();
    b.try_reserve_exact(s.len()).map_err(|_| Error::oom())?;
    b.push_str(s);
    Ok(b.into_boxed_str())
}

/// A string to format into, failing when it can't grow instead of aborting.
#[derive(Default)]
pub(crate) struct Growing(pub String);

impl fmt::Write for Growing {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.try_reserve(s.len()).map_err(|_| fmt::Error)?;
        self.0.push_str(s);
        Ok(())
    }
}

/// Pushes onto a vector, reporting `out-of-memory` instead of aborting.
pub(crate) fn push<T>(v: &mut Vec<T>, x: T) -> Result<(), Error> {
    v.try_reserve(1).map_err(|_| Error::oom())?;
    v.push(x);
    Ok(())
}
