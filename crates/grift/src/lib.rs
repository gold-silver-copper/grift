#![doc = include_str!("../../../README.md")]

mod eval;
mod heap;
mod print;
mod read;
#[cfg(test)]
mod tests;

use heap::{Binds, Heap, Obj};
use std::sync::atomic::{AtomicBool, Ordering};

pub use eval::Prim;
pub use heap::Ref;

/// The library, evaluated in the ground environment by [`Grift::new`].
const LIBRARY: &str = include_str!("library.grift");

/// A grift value. Immediate values are plain data; the rest refer to the heap of the [`Grift`]
/// that made them.
///
/// A value returned to the host stays valid until the next call to [`Grift::eval`] or
/// [`Grift::call`]; the collector may then reclaim it. Every heap reference carries its slot's generation, so using
/// a reclaimed value is the error `stale-value`, never another object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    /// The empty list `()`.
    Nil,
    /// `#t`.
    True,
    /// `#f`.
    False,
    /// `#inert`, the result of forms that have no useful value.
    Inert,
    /// `#ignore`, the formal parameter that binds nothing.
    Ignore,
    /// A 64-bit integer; arithmetic is checked.
    Int(i64),
    /// An interned symbol.
    Sym(Symbol),
    /// A pair.
    Pair(Ref),
    /// An immutable string.
    Str(Ref),
    /// An environment.
    Env(Ref),
    /// A compound operative made by `$vau`.
    Op(Ref),
    /// An applicative: a wrapped combiner.
    App(Ref),
    /// A Rust primitive, as an operative (the library binds most of them wrapped).
    Prim(Prim),
    /// A host function, as an operative ([`Grift::define`] binds it wrapped).
    Host(Host),
}

/// An interned symbol of one [`Grift`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(u32);

/// A registered host function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Host(u32);

/// A host function. It receives the interpreter, the host's state (the one passed to
/// [`Grift::eval`] or [`Grift::call`]) and the evaluated arguments.
pub type HostFn<T> = fn(&mut Grift<T>, &mut T, &[Value]) -> Result<Value, Error>;

/// What went wrong: a symbol naming the error, the values involved, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// The error code, such as `type-error` or a code a program passed to `error`.
    pub code: Symbol,
    /// The values involved, valid as long as a returned [`Value`].
    pub values: Vec<Value>,
    /// Where: the combinations of the program that were being evaluated, innermost first, each
    /// once, ending with the top-level form. The library's own code is skipped, so `($if 1 2 3)`
    /// fails at `($if 1 2 3)`; a call in tail position has finished, so it isn't listed.
    pub at: Vec<Value>,
}

/// Symbols every [`Grift`] interns first, in this order, so the core can name them as constants.
#[derive(Clone, Copy)]
pub(crate) enum K {
    UnboundSymbol,
    TypeError,
    ArityError,
    NotACombiner,
    BadFormals,
    Overflow,
    DivisionByZero,
    OutOfMemory,
    ParseError,
    StaleValue,
    Interrupted,
    RangeError,
    Null,
    Boolean,
    Inert,
    Ignore,
    Integer,
    Symbol,
    Pair,
    String,
    Environment,
    Operative,
    Applicative,
    Code,
    Values,
    At,
}

const K_NAMES: [&str; 26] = [
    "unbound-symbol",
    "type-error",
    "arity-error",
    "not-a-combiner",
    "bad-formals",
    "overflow",
    "division-by-zero",
    "out-of-memory",
    "parse-error",
    "stale-value",
    "interrupted",
    "range-error",
    "null",
    "boolean",
    "inert",
    "ignore",
    "integer",
    "symbol",
    "pair",
    "string",
    "environment",
    "operative",
    "applicative",
    "code",
    "values",
    "at",
];

impl From<K> for Symbol {
    fn from(k: K) -> Symbol {
        Symbol(k as u32)
    }
}

impl Error {
    /// An error with a copy of `values`; if even that copy can't be allocated, `out-of-memory`.
    pub(crate) fn new(code: impl Into<Symbol>, values: &[Value]) -> Error {
        let mut v = Vec::new();
        if v.try_reserve_exact(values.len()).is_err() {
            return Error::oom();
        }
        v.extend_from_slice(values);
        Error {
            code: code.into(),
            values: v,
            at: Vec::new(),
        }
    }

    pub(crate) fn oom() -> Error {
        Error {
            code: K::OutOfMemory.into(),
            values: Vec::new(),
            at: Vec::new(),
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

/// Pushes onto a vector, reporting `out-of-memory` instead of aborting.
pub(crate) fn push<T>(v: &mut Vec<T>, x: T) -> Result<(), Error> {
    v.try_reserve(1).map_err(|_| Error::oom())?;
    v.push(x);
    Ok(())
}

/// An interpreter: a heap, the ground environment holding the primitives and the library, and
/// the top-level environment, a child of the ground, where programs define things. `T` is the
/// host's state, which the host lends to each [`Grift::eval`] and [`Grift::call`] and which
/// they pass on to host functions.
pub struct Grift<T = ()> {
    heap: Heap,
    names: Vec<Box<str>>,
    /// Symbol ids sorted by name, for interning.
    sorted: Vec<u32>,
    hosts: Vec<(Symbol, HostFn<T>)>,
    ground: Ref,
    top: Ref,
    machine: eval::Machine,
    interrupt: Option<&'static AtomicBool>,
}

impl<T> Grift<T> {
    /// Builds the ground environment, evaluates the library in it and makes an empty top level.
    pub fn new() -> Result<Grift<T>, Error> {
        let mut heap = Heap::default();
        let ground = heap.alloc(Obj::Env(None, Binds::default()))?;
        let top = heap.alloc(Obj::Env(Some(ground), Binds::default()))?;
        let mut g = Grift {
            heap,
            names: Vec::new(),
            sorted: Vec::new(),
            hosts: Vec::new(),
            ground,
            top,
            machine: eval::Machine::default(),
            interrupt: None,
        };
        for name in K_NAMES {
            g.symbol(name)?;
        }
        for &p in Prim::ALL {
            let s = g.symbol(p.name())?;
            let v = match p {
                Prim::Vau => Value::Prim(p),
                _ => Value::App(g.heap.alloc(Obj::App(Value::Prim(p)))?),
            };
            g.bind_symbol(ground, s, v)?;
        }
        let forms = g.read(LIBRARY)?;
        g.eval_forms(None, forms, ground)?;
        // Everything in the heap now is the library's; errors skip its code.
        g.heap.seal_library();
        Ok(g)
    }

    /// Reads every form in `src`, then evaluates them in order in the top-level environment.
    /// Returns the last form's value (`#inert` for no forms). Nothing is evaluated if `src`
    /// doesn't read. Host functions get `state`.
    pub fn eval(&mut self, state: &mut T, src: &str) -> Result<Value, Error> {
        let forms = self.read(src)?;
        self.eval_forms(Some(state), forms, self.top)
    }

    /// Binds `name` in the top-level environment to an applicative calling `f`.
    pub fn define(&mut self, name: &str, f: HostFn<T>) -> Result<(), Error> {
        let s = self.symbol(name)?;
        let id = u32::try_from(self.hosts.len()).map_err(|_| Error::oom())?;
        push(&mut self.hosts, (s, f))?;
        let app = self.heap.alloc(Obj::App(Value::Host(Host(id))))?;
        self.bind_symbol(self.top, s, Value::App(app))
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
            Ok(i) => self
                .sorted
                .get(i)
                .map(|&id| Symbol(id))
                .ok_or_else(Error::oom),
            Err(i) => {
                let id = u32::try_from(self.names.len()).map_err(|_| Error::oom())?;
                let mut s = String::new();
                s.try_reserve_exact(name.len()).map_err(|_| Error::oom())?;
                s.push_str(name);
                push(&mut self.names, s.into_boxed_str())?;
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

    /// An error with code `code` and a copy of `values`, for a host function to return.
    pub fn error(&mut self, code: &str, values: &[Value]) -> Error {
        match self.symbol(code) {
            Ok(code) => Error::new(code, values),
            Err(e) => e,
        }
    }

    /// A new string holding a copy of `s`.
    pub fn make_string(&mut self, s: &str) -> Result<Value, Error> {
        let s = boxed(s)?;
        Ok(Value::Str(self.heap.alloc(Obj::Str(s))?))
    }

    /// The new pair `(a . d)`.
    pub fn cons(&mut self, a: Value, d: Value) -> Result<Value, Error> {
        Ok(Value::Pair(self.heap.alloc(Obj::Pair(a, d))?))
    }

    /// The car and cdr of a pair; `None` for anything else.
    pub fn uncons(&self, v: Value) -> Option<(Value, Value)> {
        match v {
            Value::Pair(r) => self.pair(r).ok(),
            _ => None,
        }
    }

    /// The contents of a string value; `None` for anything else.
    pub fn string(&self, v: Value) -> Option<&str> {
        match v {
            Value::Str(r) => match self.heap.get(r) {
                Ok(Obj::Str(s)) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }
}
