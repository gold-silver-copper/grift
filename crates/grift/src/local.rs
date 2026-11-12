//! The interpreter's own values, as the machine and the heap hold them: small, `Copy`, and
//! meaningful only in the [`Grift`](crate::Grift) that made them. The public types in the crate's
//! root carry that interpreter's identity besides, and are checked and converted to these at every
//! entry (see `Grift::import`), so nothing stale or foreign ever gets in.

use crate::evaluate::Primitive;
use crate::heap::{Ref, Seen, kind};
use std::fmt;

/// A value of this interpreter: the language's types, as the public [`Value`](crate::Value) shows
/// them, and for an operative also how it is implemented. A whole word for the tag, so writing one
/// writes its first word at once: a copy loads it whole, and a load can't take its data from a
/// one-byte write just before. About 16% off every program on x86-64.
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
    Pair(Ref<kind::Pair>),
    String(Ref<kind::String>),
    Context(Ref<kind::Context>),
    Operative(Ref<kind::Operative>),
    Applicative(Ref<kind::Applicative>),
    Primitive(Primitive),
    Host(Host),
    Bytes(Ref<kind::Bytes>),
}

// Two words, and aligned to one: every step copies values, so a bigger one costs every program.
// The size can't show the tag's width: a one-byte tag pads to the same 16 bytes.
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
#[derive(Debug, PartialEq)]
pub(crate) struct Error {
    pub began: Began,
    pub kind: Symbol,
    pub values: Vec<Value>,
    pub trace: Trace,
}

/// Who began an error, which alone says whether a handler may catch it, whatever its kind is
/// called.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Began {
    /// A program: a handler may catch it.
    Program,
    /// The host or the machine: a limit, which no handler catches and no program can make.
    Limit,
    /// Memory really refused, a limit too: what the frames it unwinds held is worth collecting.
    /// A fact about how it began, which no name and no host can claim. Whoever acts on it makes
    /// it a plain `Limit`, so it is acted on once.
    Refused,
}

/// An error's trace as the machine carries it: the list handlers got or a program gave, newest
/// first, shared and never copied, then the places added since. Nothing copies it: a handler gets
/// the added places put on the list's front ([`Trace::onto`]), raising the list it got takes it
/// back whole ([`Trace::handed`]), and only the host's export reads all of it ([`Trace::parts`]).
/// So a catch and raise at every level costs only the places each level adds. Only the last
/// handler's list is remembered: a handler that catches another error before raising its own
/// raises a list given back, which is walked once.
#[derive(Debug, PartialEq)]
pub(crate) struct Trace {
    shared: Value,
    /// Innermost first.
    added: Vec<Value>,
    /// The combinations in `shared` and `added`, so adding one searches nothing.
    seen: Seen,
}

impl Trace {
    /// No places.
    pub fn new() -> Trace {
        Trace::handed(Value::EmptyList, Seen::default())
    }

    /// A list a handler got, `seen` its combinations, as [`Trace::onto`] gave them.
    pub fn handed(shared: Value, seen: Seen) -> Trace {
        Trace {
            shared,
            added: Vec::new(),
            seen,
        }
    }

    /// The places a program or host gave, innermost first, kept as they are, repeats and all.
    pub fn given(places: Vec<Value>) -> Result<Trace, Error> {
        let mut seen = Seen::default();
        seen.try_reserve(places.len()).map_err(|_| Error::oom())?;
        #[expect(clippy::disallowed_methods, reason = "reserved places.len() above")]
        seen.extend(places.iter().filter_map(|v| match v {
            Value::Pair(r) => Some(r.slot()),
            _ => None,
        }));
        Ok(Trace {
            shared: Value::EmptyList,
            added: places,
            seen,
        })
    }

    /// Adds program combination `r`, unless it is there already. With no memory to spare the
    /// trace stays as it is: the error matters more than its last location.
    #[expect(clippy::disallowed_methods, reason = "reserved 1 in the condition")]
    pub fn add(&mut self, r: Ref<kind::Pair>) {
        if r != Ref::NONE
            && self.seen.try_reserve(1).is_ok()
            && self.added.try_reserve(1).is_ok()
            && self.seen.insert(r.slot())
        {
            self.added.push(Value::Pair(r));
        }
    }

    /// The trace as one list, newest first, the added places put on the shared list's front with
    /// `pair`, and its combinations.
    pub fn onto(
        self,
        mut pair: impl FnMut(Value, Value) -> Result<Value, Error>,
    ) -> Result<(Value, Seen), Error> {
        let list = (self.added.iter()).try_fold(self.shared, |l, &x| pair(x, l))?;
        Ok((list, self.seen))
    }

    /// The shared list, newest first, and the places added since, innermost first.
    pub fn parts(&self) -> (Value, &[Value]) {
        (self.shared, &self.added)
    }

    /// The values a collection must keep for it.
    pub fn roots(&self) -> impl Iterator<Item = Value> {
        std::iter::once(self.shared).chain(self.added.iter().copied())
    }

    /// How many objects it counts as outside the heap, for when to collect.
    pub fn weight(&self) -> usize {
        1 + self.added.len()
    }
}

/// Declares the symbols every [`Grift`](crate::Grift) interns first, in this order, so the core
/// can name them as constants; the kinds of error come first, and are the public [`Kind`] too.
macro_rules! known_symbols {
    ($($e:ident $kind:literal)* ; $($k:ident $name:literal)*) => {
        /// A kind of error grift raises, which a host makes or recognises without spelling it.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Kind { $($e),* }
        pub(crate) const KINDS: &[Kind] = &[$(Kind::$e),*];
        impl AsRef<str> for Kind {
            fn as_ref(&self) -> &str { match self { $(Kind::$e => $kind),* } }
        }
        #[derive(Clone, Copy)]
        pub(crate) enum K { $($e,)* $($k),* }
        pub(crate) const K_NAMES: &[&str] = &[$($kind,)* $($name),*];
    };
}

known_symbols! {
    UnboundSymbol "unbound-symbol" TypeError "type-error" ArityError "arity-error"
    NotACombiner "not-a-combiner" ParameterError "parameter-error" Overflow "overflow"
    DivisionByZero "division-by-zero" OutOfMemory "out-of-memory" ParseError "parse-error"
    StaleValue "stale-value" ForeignValue "foreign-value" Interrupted "interrupted"
    RangeError "range-error" CannotPark "cannot-park" OutputError "output-error"
    EncodingError "encoding-error";
    EmptyList "empty-list" Boolean "boolean" Inert "inert" Ignore "ignore" Integer "integer"
    Float "float" Symbol "symbol" Pair "pair" String "string" Context "context"
    Operative "operative" Applicative "applicative" Kind "kind" Values "values" Trace "trace"
    UnclosedList "unclosed-list" UnclosedString "unclosed-string"
    UnmatchedParenthesis "unmatched-parenthesis" MisplacedDot "misplaced-dot"
    BadEscape "bad-escape" BadNumber "bad-number" UnknownConstant "unknown-constant"
    Bytes "bytes" BadByte "bad-byte"
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
            e.began = Began::Program;
            e.kind = kind.into();
            #[expect(
                clippy::disallowed_methods,
                reason = "reserved exactly in the condition"
            )]
            e.values.extend_from_slice(values);
        }
        e
    }

    /// An error a program or host describes, from its parts.
    pub fn from_parts(kind: Symbol, values: Vec<Value>, trace: Trace) -> Error {
        Error {
            began: Began::Program,
            kind,
            values,
            trace,
        }
    }

    /// Whether a handler may not catch it.
    pub fn limit(&self) -> bool {
        self.began != Began::Program
    }

    /// Whether memory was refused to make it and nobody acted on that yet; and nobody will again.
    pub fn take_refusal(&mut self) -> bool {
        let refused = self.began == Began::Refused;
        if refused {
            self.began = Began::Limit;
        }
        refused
    }

    /// The machine ran out of memory.
    pub fn oom() -> Error {
        Error {
            began: Began::Refused,
            kind: K::OutOfMemory.into(),
            values: Vec::new(),
            trace: Trace::new(),
        }
    }

    /// The run was stopped.
    pub fn interrupted() -> Error {
        Error {
            began: Began::Limit,
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
    #[expect(clippy::disallowed_methods, reason = "reserved exactly above")]
    b.push_str(s);
    Ok(b.into_boxed_str())
}

/// Pushes onto a vector, reporting `out-of-memory` instead of aborting.
pub(crate) fn push<T>(v: &mut Vec<T>, x: T) -> Result<(), Error> {
    v.try_reserve(1).map_err(|_| Error::oom())?;
    #[expect(clippy::disallowed_methods, reason = "reserved 1 above")]
    v.push(x);
    Ok(())
}
