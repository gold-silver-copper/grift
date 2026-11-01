#![doc = include_str!("../../../README.md")]
#![no_std]

extern crate alloc;

mod eval;
mod heap;
mod print;
mod read;
#[cfg(test)]
mod tests;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use heap::{Heap, Obj};

pub use eval::Prim;
pub use heap::Ref;

/// The library, evaluated in the ground environment by [`Grift::new`].
const LIBRARY: &str = include_str!("library.grift");

/// A grift value. Immediate values are plain data; the rest refer to the heap of the [`Grift`]
/// that made them.
///
/// A value returned to the host stays valid until the next call to [`Grift::eval`]; the
/// collector may then reclaim it. Every heap reference carries its slot's generation, so using
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Symbol(u32);

/// A registered host function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Host(u32);

/// A host function. It receives the evaluated arguments.
pub type HostFn = fn(&mut Grift, &[Value]) -> Result<Value, Error>;

/// What went wrong: a symbol naming the error, and the values involved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// The error code, such as `type-error` or a code a program passed to `error`.
    pub code: Symbol,
    /// The values involved, valid as long as a returned [`Value`].
    pub values: Vec<Value>,
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
}

const K_NAMES: [&str; 21] = [
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
];

impl From<K> for Symbol {
    fn from(k: K) -> Symbol {
        Symbol(k as u32)
    }
}

impl Error {
    /// An error with a copy of `values`; if even that copy can't be allocated, `out-of-memory`.
    pub(crate) fn new(code: K, values: &[Value]) -> Error {
        let mut v = Vec::new();
        if v.try_reserve_exact(values.len()).is_err() {
            return Error::oom();
        }
        v.extend_from_slice(values);
        Error {
            code: code.into(),
            values: v,
        }
    }

    pub(crate) fn oom() -> Error {
        Error {
            code: K::OutOfMemory.into(),
            values: Vec::new(),
        }
    }
}

/// Pushes onto a vector, reporting `out-of-memory` instead of aborting.
pub(crate) fn push<T>(v: &mut Vec<T>, x: T) -> Result<(), Error> {
    v.try_reserve(1).map_err(|_| Error::oom())?;
    v.push(x);
    Ok(())
}

/// An interpreter: a heap, the ground environment holding the primitives and the library, and
/// the top-level environment, a child of the ground, where programs define things.
pub struct Grift {
    heap: Heap,
    names: Vec<Box<str>>,
    /// Symbol ids sorted by name, for interning.
    sorted: Vec<u32>,
    hosts: Vec<(Symbol, HostFn)>,
    ground: Ref,
    top: Ref,
    machine: eval::Machine,
}

impl Grift {
    /// Builds the ground environment, evaluates the library in it and makes an empty top level.
    pub fn new() -> Result<Grift, Error> {
        let mut heap = Heap::default();
        let ground = heap.alloc(Obj::Env(None, Vec::new()))?;
        let top = heap.alloc(Obj::Env(Some(ground), Vec::new()))?;
        let mut g = Grift {
            heap,
            names: Vec::new(),
            sorted: Vec::new(),
            hosts: Vec::new(),
            ground,
            top,
            machine: eval::Machine::default(),
        };
        for name in K_NAMES {
            g.symbol(name)?;
        }
        for p in Prim::ALL {
            let s = g.symbol(p.name())?;
            let v = match p {
                Prim::Vau => Value::Prim(p),
                _ => Value::App(g.heap.alloc(Obj::App(Value::Prim(p)))?),
            };
            g.bind_symbol(ground, s, v)?;
        }
        g.eval_in(LIBRARY, ground)?;
        Ok(g)
    }

    /// Reads every form in `src`, then evaluates them in order in the top-level environment.
    /// Returns the last form's value (`#inert` for no forms). Nothing is evaluated if `src`
    /// doesn't read.
    pub fn eval(&mut self, src: &str) -> Result<Value, Error> {
        self.eval_in(src, self.top)
    }

    /// Binds `name` in the top-level environment to an applicative calling `f`.
    pub fn define(&mut self, name: &str, f: HostFn) -> Result<(), Error> {
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
                .unwrap_or(core::cmp::Ordering::Less)
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
