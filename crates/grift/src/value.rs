//! The [`Value`] type: immediates plus handles into the heap.

/// Interned symbol identifier.
///
/// Two symbols with the same name always have the same id, so symbol
/// comparison is integer comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymId(pub(crate) u32);

/// Handle to an object stored in the heap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjId(pub(crate) u32);

impl ObjId {
    /// Index of the heap slot this handle refers to.
    #[inline]
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

/// Identifier of a Rust primitive.
///
/// A bare `Prim` value is the primitive's *operative* core. Every primitive
/// except `$vau` is bound in the primitive environment wrapped in an
/// applicative.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrimId(pub(crate) u16);

/// A grift value.
///
/// Immediates are stored inline; everything else is an [`ObjId`] handle whose
/// kind is determined by the heap object it points to. `Value` is `Copy` and
/// 16 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    /// The empty list, written `()`.
    Nil,
    /// `#t` or `#f`.
    Bool(bool),
    /// `#inert`, the result of combiners called only for effect.
    Inert,
    /// `#ignore`, the wildcard of parameter trees.
    Ignore,
    /// A checked 64-bit integer.
    Int(i64),
    /// A Unicode scalar value.
    Char(char),
    /// An interned symbol.
    Sym(SymId),
    /// The operative core of a Rust primitive.
    Prim(PrimId),
    /// A heap object (pair, string, environment, combiner, ...).
    Obj(ObjId),
}

impl Value {
    /// True if this value is a symbol.
    #[inline]
    pub fn is_symbol(self) -> bool {
        matches!(self, Value::Sym(_))
    }

    /// The integer payload, if this is an integer.
    #[inline]
    pub fn as_int(self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(n),
            _ => None,
        }
    }

    /// The boolean payload, if this is a boolean.
    #[inline]
    pub fn as_bool(self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    /// The character payload, if this is a character.
    #[inline]
    pub fn as_char(self) -> Option<char> {
        match self {
            Value::Char(c) => Some(c),
            _ => None,
        }
    }

    /// The heap handle, if this is a heap object.
    #[inline]
    pub fn as_obj(self) -> Option<ObjId> {
        match self {
            Value::Obj(id) => Some(id),
            _ => None,
        }
    }
}

const _: () = assert!(core::mem::size_of::<Value>() == 16);
