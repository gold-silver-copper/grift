//! Lisp value type.

use grift_arena::{ArenaError, ArenaIndex, Slotted, FREE_LIST_END};

/// Type-safe identifier for built-in functions.
///
/// Wraps a `u8`, supporting up to 256 builtins. Generated automatically
/// by [`define_builtins!`] and matched in [`Evaluator::apply_builtin`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct BuiltinId(pub(crate) u8);

/// A Lisp value stored in the arena. Variants can only inline max two arenaindex sized data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// Free slot in the arena's free list, storing the next free index.
    Free { next_free: usize },
    /// The empty list / nil.
    Nil,
    Boolean(bool),
    /// Integer number.
    Number(isize),
    /// A symbol, pointing to the first `Char` in the name's linked list.
    Symbol(ArenaIndex),
    /// A cons cell (pair) with inline car and cdr.
    Cons {
        car: ArenaIndex,
        cdr: ArenaIndex,
    },
    /// A character with inline cdr pointer forming a linked list for strings.
    /// Strings are simply Char linked lists — no separate String wrapper.
    /// `car`/`cdr` work natively on Char values.
    Char {
        ch: char,
        cdr: ArenaIndex,
    },
    /// Compound operative (vau closure / fexpr).
    /// Created by `(vau params env-param body)`.
    /// params_envparam = (params . env-param), body_env = (body . closed-env)
    Operative {
        params_envparam: ArenaIndex,
        body_env: ArenaIndex,
    },
    /// Applicative wrapper: evaluates arguments, then calls inner combiner.
    /// Created by `(wrap combiner)`. The inner combiner is any callable:
    /// Operative, Builtin, or even another Applicative.
    Applicative(ArenaIndex),
    /// Rust-native primitive operative.
    /// Always an operative — receives unevaluated args + caller env.
    /// Applicative primitives (like +) are (wrap (Builtin id)) at init time.
    Builtin(BuiltinId),
    /// A first-class environment with lexical parent chain.
    /// `bindings`: alist of (symbol . value) pairs in this frame.
    /// `parents`: cons-list of parent environments, or NIL for top-level.
    Environment {
        bindings: ArenaIndex,
        parents: ArenaIndex,
    },
    /// The inert value, written `#inert`.
    /// Returned by combiners whose primary purpose is side-effect (e.g. `$define!`).
    Inert,
    /// The ignore value, written `#ignore`.
    /// Used specifically for parameter matching in formal parameter trees.
    Ignore,
}

/// Generate a `Value` accessor that pattern-matches on a variant and
/// returns its inner data, or `Err(TypeError)` on mismatch.
macro_rules! value_accessor {
    ($(#[$m:meta])* $name:ident -> $out:ty, $pat:pat => $expr:expr) => {
        $(#[$m])*
        #[inline]
        pub fn $name(self) -> Result<$out, ArenaError> {
            match self {
                $pat => Ok($expr),
                _ => Err(ArenaError::TypeError),
            }
        }
    };
}

impl Slotted for Value {
    #[inline]
    fn is_free(&self) -> bool {
        matches!(self, Value::Free { .. })
    }

    #[inline]
    fn next_free(&self) -> usize {
        match self {
            Value::Free { next_free } => *next_free,
            _ => FREE_LIST_END,
        }
    }

    #[inline]
    fn make_free(next: usize) -> Self {
        Value::Free { next_free: next }
    }
}

impl Value {
    /// Returns the type name as a static string (for error messages).
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Free { .. } => "free",
            Value::Nil => "nil",
            Value::Boolean(_) => "boolean",
            Value::Number(_) => "number",
            Value::Symbol(_) => "symbol",
            Value::Cons { .. } => "pair",
            Value::Char { .. } => "char",
            Value::Operative { .. } => "operative",
            Value::Applicative(_) => "applicative",
            Value::Builtin(_) => "builtin",
            Value::Environment { .. } => "environment",
            Value::Inert => "inert",
            Value::Ignore => "ignore",
        }
    }

    /// True for self-evaluating forms (literals, closures, builtins).
    ///
    /// Self-evaluating values exclude symbols and cons cells
    /// (which require lookup / dispatch).
    #[inline]
    pub fn is_self_evaluating(self) -> bool {
        !matches!(self, Value::Symbol(_) | Value::Cons { .. })
    }

    /// True for immutable, encapsulated types whose identity is
    /// determined by value rather than arena slot (used by `eq?`).
    #[inline]
    pub fn is_immutable(self) -> bool {
        matches!(
            self,
            Value::Nil
                | Value::Boolean(_)
                | Value::Number(_)
                | Value::Symbol(_)
                | Value::Char { .. }
                | Value::Inert
                | Value::Ignore
        )
    }

    value_accessor! {
        /// Extract the numeric value, or `Err(TypeError)` if not a number.
        as_number -> isize, Value::Number(n) => n
    }

    value_accessor! {
        /// Extract the car and cdr of a cons cell.
        as_cons -> (ArenaIndex, ArenaIndex), Value::Cons { car, cdr } => (car, cdr)
    }

    value_accessor! {
        /// Extract the symbol's string index.
        as_symbol -> ArenaIndex, Value::Symbol(idx) => idx
    }

    value_accessor! {
        /// Extract the boolean value, or `Err(TypeError)` if not a boolean.
        as_bool -> bool, Value::Boolean(b) => b
    }

    value_accessor! {
        /// Extract the inner combiner of an applicative, or `Err(TypeError)`.
        as_applicative -> ArenaIndex, Value::Applicative(inner) => inner
    }
}

impl core::fmt::Display for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Value::Nil => f.write_str("()"),
            Value::Boolean(true) => f.write_str("#t"),
            Value::Boolean(false) => f.write_str("#f"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Char { ch, .. } => write!(f, "#\\{ch}"),
            Value::Inert => f.write_str("#inert"),
            Value::Ignore => f.write_str("#ignore"),
            _ => write!(f, "<{}>", self.type_name()),
        }
    }
}

macro_rules! impl_from_value {
    ($($ty:ty => $variant:ident),+ $(,)?) => {
        $(impl From<$ty> for Value {
            #[inline]
            fn from(v: $ty) -> Self { Value::$variant(v) }
        })+
    };
}

impl_from_value!(bool => Boolean, isize => Number, BuiltinId => Builtin);

impl From<char> for Value {
    #[inline]
    fn from(v: char) -> Self {
        Value::Char { ch: v, cdr: ArenaIndex::NIL }
    }
}
