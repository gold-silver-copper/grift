//! The interpreter's own values, as the machine and the heap hold them: small, `Copy`, and
//! meaningful only in the [`Grift`](crate::Grift) that made them. The public types in the crate's
//! root carry that interpreter's identity besides, and are checked and converted to these at every
//! entry (see `Grift::import`), so nothing stale or foreign ever gets in.

use crate::evaluate::Primitive;
use crate::heap::{Reference, Seen};
use std::fmt;

/// A value of this interpreter. The variants are the public [`Value`](crate::Value)'s.
#[derive(Clone, Copy, Debug, PartialEq)]
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

/// An interned symbol: its index among the names. Indices at or above [`KEPT`] name kept values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Symbol(pub u32);

/// A registered host function: its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Host(pub u32);

/// The first symbol index no text can name: the names of kept values start here.
pub(crate) const KEPT: u32 = 1 << 31;

/// An error as the machine carries it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Error {
    pub kind: Symbol,
    pub values: Vec<Value>,
    pub trace: Vec<Value>,
    /// The combinations in `trace`, so adding one searches nothing.
    pub seen: Seen<Reference>,
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
    RangeError "range-error" CannotPark "cannot-park" Abandoned "abandoned"
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
            e.kind = kind.into();
            e.values.extend_from_slice(values);
        }
        e
    }

    pub fn oom() -> Error {
        Error {
            kind: K::OutOfMemory.into(),
            values: Vec::new(),
            trace: Vec::new(),
            seen: Seen::default(),
        }
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
