#![doc = include_str!("../../../README.md")]

mod evaluate;
mod heap;
mod print;
mod read;
#[cfg(test)]
mod tests;

use heap::{Bindings, Heap, Object, Seen};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

pub use evaluate::Primitive;
pub use heap::Reference;

/// The library, evaluated in the built-in context by [`Grift::new`].
const LIBRARY: &str = include_str!("library.grift");

/// A grift value. Immediate values are plain data; the rest refer to the heap of the [`Grift`]
/// that made them.
///
/// A value returned to the host stays valid until the next call to [`Grift::evaluate`] or
/// [`Grift::call`]; the collector may then reclaim it. Every heap reference carries its slot's
/// generation, so using a reclaimed value is the error `stale-value`, never another object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// The empty list `()`.
    EmptyList,
    /// `#true` or `#false`.
    Boolean(bool),
    /// `#inert`, the value of a form with nothing useful to return.
    Inert,
    /// `#ignore`, the parameter that binds nothing.
    Ignore,
    /// A 64-bit integer; arithmetic is checked.
    Integer(i64),
    /// A 64-bit IEEE float.
    Float(f64),
    /// An interned symbol.
    Symbol(Symbol),
    /// A pair.
    Pair(Reference),
    /// An immutable string.
    String(Reference),
    /// A context.
    Context(Reference),
    /// A compound operative made by `vau!`.
    Operative(Reference),
    /// An applicative: a wrapped combiner.
    Applicative(Reference),
    /// A Rust primitive, as an operative (the library binds most of them wrapped).
    Primitive(Primitive),
    /// A host function, as an operative ([`Grift::host_function`] wraps it).
    Host(Host),
}

/// An interned symbol of one [`Grift`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(u32);

/// A registered host function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Host(u32);

/// A host function. It receives the interpreter, the host's state (the one passed to
/// [`Grift::evaluate`] or [`Grift::call`]) and the evaluated arguments.
pub type HostFunction<T> = fn(&mut Grift<T>, &mut T, &[Value]) -> Result<Value, Error>;

/// What went wrong: a symbol naming the kind of error, the values involved, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    /// The kind, such as `type-error` or a kind a program gave `error`.
    pub kind: Symbol,
    /// The values involved, valid as long as a returned [`Value`].
    pub values: Vec<Value>,
    /// Where, read with [`Error::trace`].
    trace: Vec<Value>,
    /// The combinations in `trace`, so adding one searches nothing.
    seen: Seen<Reference>,
}

/// Declares the symbols every [`Grift`] interns first, in this order, so the core can name them as
/// constants.
macro_rules! known_symbols {
    ($($k:ident $name:literal)*) => {
        #[derive(Clone, Copy)]
        pub(crate) enum K { $($k),* }
        const K_NAMES: &[&str] = &[$($name),*];
    };
}

known_symbols! {
    UnboundSymbol "unbound-symbol" TypeError "type-error" ArityError "arity-error"
    NotACombiner "not-a-combiner" ParameterError "parameter-error" Overflow "overflow"
    DivisionByZero "division-by-zero" OutOfMemory "out-of-memory" ParseError "parse-error"
    StaleValue "stale-value" Interrupted "interrupted" RangeError "range-error"
    EmptyList "empty-list" Boolean "boolean" Inert "inert" Ignore "ignore" Integer "integer"
    Float "float" Symbol "symbol" Pair "pair" String "string" Context "context"
    Operative "operative" Applicative "applicative" Kind "kind" Values "values" Trace "trace"
}

impl From<K> for Symbol {
    fn from(k: K) -> Symbol {
        Symbol(k as u32)
    }
}

impl Error {
    /// Where: the combinations of the program that were being evaluated, innermost first, each
    /// once, ending with the top-level form. The library's own code is skipped, so `(if! 1 2 3)`
    /// fails at `(if! 1 2 3)`; a call in tail position has finished, so it isn't listed.
    pub fn trace(&self) -> &[Value] {
        &self.trace
    }

    /// An error with a copy of `values`; if even that copy can't be allocated, `out-of-memory`.
    pub(crate) fn new(kind: impl Into<Symbol>, values: &[Value]) -> Error {
        let mut e = Error::oom();
        if e.values.try_reserve_exact(values.len()).is_ok() {
            e.kind = kind.into();
            e.values.extend_from_slice(values);
        }
        e
    }

    pub(crate) fn oom() -> Error {
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

/// An interpreter: a heap, the built-in context holding the primitives and the library, and the
/// top-level context, a child of the built-in one, where programs define things. `T` is the
/// host's state, which the host lends to each [`Grift::evaluate`] and [`Grift::call`] and which
/// they pass on to host functions.
pub struct Grift<T = ()> {
    heap: Heap,
    names: Vec<Box<str>>,
    /// Symbol ids sorted by name, for interning.
    sorted: Vec<u32>,
    hosts: Vec<(Symbol, HostFunction<T>)>,
    built_in: Reference,
    top: Reference,
    machine: evaluate::Machine,
    interrupt: Option<&'static AtomicBool>,
}

impl<T> Grift<T> {
    /// Builds the built-in context, evaluates the library in it and makes an empty top level.
    pub fn new() -> Result<Grift<T>, Error> {
        let mut heap = Heap::default();
        let built_in = heap.alloc(Object::Context(None, Bindings::default()))?;
        let top = heap.alloc(Object::Context(Some(built_in), Bindings::default()))?;
        let mut g = Grift {
            heap,
            names: Vec::new(),
            sorted: Vec::new(),
            hosts: Vec::new(),
            built_in,
            top,
            machine: evaluate::Machine::default(),
            interrupt: None,
        };
        for name in K_NAMES {
            g.symbol(name)?;
        }
        for &p in Primitive::ALL {
            let s = g.symbol(p.name())?;
            let v = match p {
                Primitive::Vau => Value::Primitive(p),
                _ => Value::Applicative(g.heap.alloc(Object::Applicative(Value::Primitive(p)))?),
            };
            g.bind_symbol(built_in, s, v)?;
        }
        let forms = g.read(LIBRARY)?;
        g.evaluate_forms(None, forms, built_in)?;
        // Everything in the heap now is the library's; errors skip its code.
        g.heap.seal_library();
        Ok(g)
    }

    /// Reads every form in `src`, then evaluates them in order in the top-level context.
    /// Returns the last form's value (`#inert` for no forms). Nothing is evaluated if `src`
    /// doesn't read. Host functions get `state`.
    pub fn evaluate(&mut self, state: &mut T, src: &str) -> Result<Value, Error> {
        let forms = self.read(src)?;
        self.evaluate_forms(Some(state), forms, self.top)
    }

    /// An applicative calling `f`, bound nowhere: the host binds it with [`Grift::define`], or
    /// hands it to grift code that keeps it private.
    pub fn host_function(&mut self, name: &str, f: HostFunction<T>) -> Result<Value, Error> {
        let s = self.symbol(name)?;
        let id = u32::try_from(self.hosts.len()).map_err(|_| Error::oom())?;
        push(&mut self.hosts, (s, f))?;
        let applicative = self
            .heap
            .alloc(Object::Applicative(Value::Host(Host(id))))?;
        Ok(Value::Applicative(applicative))
    }

    /// A new empty context whose parent is the built-in one: a private top level. Grift code
    /// evaluated there (with `call`ing `evaluate`) sees the primitives and the library, never
    /// what a program defines.
    pub fn context(&mut self) -> Result<Value, Error> {
        let c = Object::Context(Some(self.built_in), Bindings::default());
        Ok(Value::Context(self.heap.alloc(c)?))
    }

    /// Binds `name` to `v` in the top-level context.
    pub fn define(&mut self, name: &str, v: Value) -> Result<(), Error> {
        let s = self.symbol(name)?;
        self.bind_symbol(self.top, s, v)
    }

    /// Interns `name`.
    pub fn symbol(&mut self, name: &str) -> Result<Symbol, Error> {
        let names = &self.names;
        let at = self.sorted.binary_search_by(|&i| {
            names
                .get(i as usize)
                .map(|n| (**n).cmp(name))
                .unwrap_or(std::cmp::Ordering::Less)
        });
        match at {
            Ok(i) => Ok(Symbol(self.sorted.get(i).copied().unwrap_or_default())),
            Err(i) => {
                let id = u32::try_from(self.names.len()).map_err(|_| Error::oom())?;
                push(&mut self.names, boxed(name)?)?;
                self.sorted.try_reserve(1).map_err(|_| Error::oom())?;
                self.sorted.insert(i, id);
                Ok(Symbol(id))
            }
        }
    }

    /// After this, evaluation stops with the error `interrupted` at the next step once `flag`
    /// is set, and clears it. The host sets it from another thread or a signal handler.
    pub fn set_interrupt_flag(&mut self, flag: &'static AtomicBool) {
        self.interrupt = Some(flag);
    }

    /// Whether the interrupt flag was set; clears it.
    pub(crate) fn interrupted(&self) -> bool {
        // A load first: the flag is read at every step and set almost never.
        self.interrupt
            .is_some_and(|f| f.load(Ordering::Relaxed) && f.swap(false, Ordering::Relaxed))
    }

    /// The name of symbol `s`.
    pub(crate) fn name(&self, s: Symbol) -> &str {
        self.names.get(s.0 as usize).map_or("", |n| n)
    }

    /// An error of kind `kind` with a copy of `values`, for a host function to return.
    pub fn error(&mut self, kind: &str, values: &[Value]) -> Error {
        match self.symbol(kind) {
            Ok(kind) => Error::new(kind, values),
            Err(e) => e,
        }
    }

    /// A new string holding a copy of `s`.
    pub fn string(&mut self, s: &str) -> Result<Value, Error> {
        let s = boxed(s)?;
        Ok(Value::String(self.heap.alloc(Object::String(s))?))
    }

    /// The new pair `(a . b)`.
    pub fn pair(&mut self, a: Value, b: Value) -> Result<Value, Error> {
        Ok(Value::Pair(self.heap.alloc(Object::Pair(a, b))?))
    }

    /// The first and the rest of a pair; `None` for anything else.
    pub fn as_pair(&self, v: Value) -> Option<(Value, Value)> {
        match v {
            Value::Pair(r) => self.pair_at(r).ok(),
            _ => None,
        }
    }

    /// The contents of a string value; `None` for anything else.
    pub fn as_str(&self, v: Value) -> Option<&str> {
        match v {
            Value::String(r) => match self.heap.get(r) {
                Ok(Object::String(s)) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }
}
