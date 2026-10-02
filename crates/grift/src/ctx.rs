//! [`Ctx`]: the heap plus the symbol table, with value helpers.

use alloc::string::String;
use alloc::vec::Vec;

use crate::error::{Fault, GResult};
use crate::heap::{Heap, Object};
use crate::symbol::Symbols;
use crate::value::{ObjId, SymId, Value};

/// The interpreter's store: every heap object and every symbol.
///
/// Native functions receive a `&mut Ctx` to inspect their arguments and
/// build results.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub(crate) heap: Heap,
    pub(crate) syms: Symbols,
    /// Tag for the next encapsulation type.
    pub(crate) next_tag: u32,
    /// Memoized environment lookups; see [`Ctx::lookup`].
    pub(crate) lookup_cache: Vec<(u32, u32, Value)>,
}

impl Default for Ctx {
    fn default() -> Self {
        Ctx {
            heap: Heap::default(),
            syms: Symbols::default(),
            next_tag: 0,
            lookup_cache: alloc::vec![crate::env::EMPTY_CACHE_ENTRY; crate::env::LOOKUP_CACHE_SIZE],
        }
    }
}

impl Ctx {
    /// Allocate a pair.
    pub fn cons(&mut self, car: Value, cdr: Value) -> Value {
        Value::Obj(self.heap.alloc(Object::Pair(car, cdr)))
    }

    /// Build a proper list from `items`.
    pub fn list(&mut self, items: &[Value]) -> Value {
        items
            .iter()
            .rev()
            .fold(Value::Nil, |tail, &item| self.cons(item, tail))
    }

    /// Allocate a string.
    pub fn string(&mut self, s: &str) -> Value {
        Value::Obj(self.heap.alloc(Object::Str(s.into())))
    }

    /// Intern a symbol.
    pub fn symbol(&mut self, name: &str) -> Value {
        Value::Sym(self.syms.intern(name))
    }

    /// The name of a symbol value.
    pub fn symbol_name(&self, v: Value) -> Option<&str> {
        match v {
            Value::Sym(s) => Some(self.syms.name(s)),
            _ => None,
        }
    }

    /// The contents of a string value.
    pub fn as_str(&self, v: Value) -> Option<&str> {
        match v {
            Value::Obj(id) => match self.heap.get(id) {
                Object::Str(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    /// The car and cdr of a pair.
    pub fn pair(&self, v: Value) -> Option<(Value, Value)> {
        match v {
            Value::Obj(id) => match self.heap.get(id) {
                Object::Pair(a, b) => Some((*a, *b)),
                _ => None,
            },
            _ => None,
        }
    }

    /// The elements of a proper list, or `None` if `v` is not one.
    pub fn list_to_vec(&self, v: Value) -> Option<Vec<Value>> {
        let mut out = Vec::new();
        self.list_into(v, &mut out).then_some(out)
    }

    /// Append the elements of the proper list `v` to `out`. Returns false
    /// (leaving `out` partially filled) if `v` is not a proper list.
    pub(crate) fn list_into(&self, mut v: Value, out: &mut Vec<Value>) -> bool {
        loop {
            match v {
                Value::Nil => return true,
                _ => match self.pair(v) {
                    Some((a, d)) => {
                        out.push(a);
                        v = d;
                    }
                    None => return false,
                },
            }
        }
    }

    /// The written representation of a value (strings quoted).
    pub fn write(&self, v: Value) -> String {
        let mut s = String::new();
        self.print(v, true, &mut s);
        s
    }

    /// The displayed representation of a value (strings raw).
    pub fn display(&self, v: Value) -> String {
        let mut s = String::new();
        self.print(v, false, &mut s);
        s
    }

    /// The object behind a value, if it is a heap value.
    #[inline]
    pub(crate) fn obj(&self, v: Value) -> Option<&Object> {
        match v {
            Value::Obj(id) => Some(self.heap.get(id)),
            _ => None,
        }
    }

    pub(crate) fn sym(&mut self, name: &str) -> SymId {
        self.syms.intern(name)
    }

    pub(crate) fn is_pair(&self, v: Value) -> bool {
        matches!(self.obj(v), Some(Object::Pair(..)))
    }

    pub(crate) fn is_env(&self, v: Value) -> bool {
        matches!(self.obj(v), Some(Object::Env(_) | Object::SmallEnv(_)))
    }

    pub(crate) fn is_operative(&self, v: Value) -> bool {
        match v {
            Value::Prim(_) => true,
            Value::Obj(id) => matches!(
                self.heap.get(id),
                Object::Operative(_) | Object::PrimData(..) | Object::Native(_)
            ),
            _ => false,
        }
    }

    pub(crate) fn is_applicative(&self, v: Value) -> bool {
        matches!(self.obj(v), Some(Object::Applicative(_)))
    }

    pub(crate) fn is_combiner(&self, v: Value) -> bool {
        self.is_operative(v) || self.is_applicative(v)
    }

    /// A short description of a value's type, for error messages.
    pub(crate) fn type_name(&self, v: Value) -> &'static str {
        match v {
            Value::Nil => "null",
            Value::Bool(_) => "boolean",
            Value::Inert => "inert",
            Value::Ignore => "ignore",
            Value::Int(_) => "integer",
            Value::Char(_) => "char",
            Value::Sym(_) => "symbol",
            Value::Prim(_) => "operative",
            Value::Obj(id) => match self.heap.get(id) {
                Object::Pair(..) => "pair",
                Object::Str(_) => "string",
                Object::Env(_) | Object::SmallEnv(_) => "environment",
                Object::Operative(_) | Object::PrimData(..) | Object::Native(_) => "operative",
                Object::Applicative(_) => "applicative",
                Object::Encap(..) => "encapsulation",
                Object::Declaration(_) => "declaration",
                Object::Prompt => "prompt",
                Object::SubCont(_) => "sub-continuation",
                Object::Free(_) | Object::Log(_) | Object::Frame(_) => "internal",
            },
        }
    }

    /// A `type-error` fault: `what` expected, `v` found.
    pub(crate) fn type_error(&self, what: &str, v: Value) -> Fault {
        Fault::new(
            "type-error",
            alloc::format!("expected {what}, got {}", self.type_name(v)),
        )
        .with(v)
    }

    pub(crate) fn expect_int(&self, v: Value) -> GResult<i64> {
        v.as_int().ok_or_else(|| self.type_error("an integer", v))
    }

    pub(crate) fn expect_bool(&self, v: Value) -> GResult<bool> {
        v.as_bool().ok_or_else(|| self.type_error("a boolean", v))
    }

    pub(crate) fn expect_char(&self, v: Value) -> GResult<char> {
        v.as_char().ok_or_else(|| self.type_error("a char", v))
    }

    pub(crate) fn expect_env(&self, v: Value) -> GResult<ObjId> {
        match v {
            Value::Obj(id) if matches!(self.heap.get(id), Object::Env(_) | Object::SmallEnv(_)) => {
                Ok(id)
            }
            _ => Err(self.type_error("an environment", v)),
        }
    }

    pub(crate) fn expect_str(&self, v: Value) -> GResult<&str> {
        self.as_str(v).ok_or_else(|| self.type_error("a string", v))
    }

    pub(crate) fn expect_list(&self, v: Value) -> GResult<Vec<Value>> {
        self.list_to_vec(v)
            .ok_or_else(|| self.type_error("a proper list", v))
    }
}
