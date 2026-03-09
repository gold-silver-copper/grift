//! Lisp value type.
//!
//! Defines the [`Value`] enum representing all first-class Lisp types,
//! along with [`BuiltinId`] for identifying primitive operatives.
//!
//! ## Design Constraints
//!
//! Every `Value` variant fits in a single arena slot and may inline at
//! most two `ArenaIndex`-sized fields. Larger structures (parameter
//! trees, environment chains, strings, symbol names) are built as linked
//! lists of `Cons` nodes in the arena.

use crate::arena::{ArenaError, ArenaIndex};

use crate::native::NativeFn;
use crate::prelude::Prelude;

/// Type-safe identifier for built-in operatives and applicatives.
///
/// Wraps a `u8`, supporting up to 256 builtins. Constants are generated
/// automatically by the `define_builtins!` macro and dispatched in
/// `apply_operative_builtin` / `apply_builtin_pure`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct BuiltinId(pub(crate) u8);

/// A Lisp value stored in the arena.
///
/// Each variant is `Copy` and fits in a single arena slot (tag + at most
/// two `ArenaIndex` fields ≈ 24 bytes).
///
/// # Invariants
///
/// - **Pre-allocated singletons**: Slots 0–4 always contain `Nil`, `#t`,
///   `#f`, `#inert`, and `#ignore` respectively. Their [`ArenaIndex`]
///   constants ([`ArenaIndex::NIL`], [`ArenaIndex::TRUE`], etc.) are
///   compile-time values and must never be freed or overwritten.
/// - **Immutable pairs**: `Cons` cells are logically immutable once created;
///   there is no `set-car!` / `set-cdr!`. This simplifies GC and enables
///   structural sharing.
/// - **Interned symbols**: Two symbols with the same name always share the
///   same `ArenaIndex`, so symbol equality is pointer equality.
#[derive(Clone, Copy, Debug)]
pub enum Value {
    /// The empty list / nil.
    Nil,
    /// Boolean value (`#t` or `#f`).
    Boolean(bool),
    /// Integer number (machine-width signed integer).
    Number(isize),
    /// A symbol, pointing to the first `Cons` node of its char-list name.
    Symbol(ArenaIndex),
    /// A cons cell (pair) with inline car and cdr.
    Cons {
        /// Head element of the pair.
        car: ArenaIndex,
        /// Tail element of the pair (next `Cons` node in a proper list, `NIL` at the end, or any value in a dotted pair).
        cdr: ArenaIndex,
    },
    /// A first-class character value.
    Char(char),
    /// Compound operative (vau closure / fexpr).
    /// Created by `(vau params env-param body)`.
    Operative {
        /// Cons cell packing the formal parameter tree and environment parameter.
        params_envparam: ArenaIndex,
        /// Cons cell packing the body expression and the closed-over environment.
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
    Environment {
        /// Alist of `(symbol . value)` pairs in this frame.
        bindings: ArenaIndex,
        /// Cons-list of parent environments, or NIL for top-level.
        parents: ArenaIndex,
    },
    /// The inert value, written `#inert`.
    /// Returned by combiners whose primary purpose is side-effect (e.g. `define!`).
    Inert,
    /// The ignore value, written `#ignore`.
    /// Used specifically for parameter matching in formal parameter trees.
    Ignore,
    /// Prelude function (static memory, parsed on demand).
    Prelude(Prelude),
    /// User-registered native function (Rust function pointer).
    /// Always wrapped in an `Applicative` when registered. The function
    /// pointer is stored directly, with no const generic dependency.
    Native(NativeFn),
}

impl PartialEq for Value {
    /// Compare two values using their stored representation.
    ///
    /// This is intentionally not the same as Lisp `eq?` or `equal?`. It is a
    /// Rust-side structural comparison used by tests and a few runtime helpers.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Symbol(a), Value::Symbol(b)) => a == b,
            (Value::Cons { car: a1, cdr: a2 }, Value::Cons { car: b1, cdr: b2 }) => {
                a1 == b1 && a2 == b2
            }
            (Value::Char(a), Value::Char(b)) => a == b,
            (
                Value::Operative {
                    params_envparam: a1,
                    body_env: a2,
                },
                Value::Operative {
                    params_envparam: b1,
                    body_env: b2,
                },
            ) => a1 == b1 && a2 == b2,
            (Value::Applicative(a), Value::Applicative(b)) => a == b,
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            (
                Value::Environment {
                    bindings: a1,
                    parents: a2,
                },
                Value::Environment {
                    bindings: b1,
                    parents: b2,
                },
            ) => a1 == b1 && a2 == b2,
            (Value::Inert, Value::Inert) => true,
            (Value::Ignore, Value::Ignore) => true,
            (Value::Prelude(a), Value::Prelude(b)) => a == b,
            #[allow(unpredictable_function_pointer_comparisons)]
            (Value::Native(a), Value::Native(b)) => core::ptr::fn_addr_eq(*a, *b),
            _ => false,
        }
    }
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

impl Value {
    /// Returns the type name as a static string (for error messages).
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift::Value;
    ///
    /// assert_eq!(Value::Number(42).type_name(), "number");
    /// assert_eq!(Value::Nil.type_name(), "nil");
    /// assert_eq!(Value::Boolean(true).type_name(), "boolean");
    /// ```
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Boolean(_) => "boolean",
            Value::Number(_) => "number",
            Value::Symbol(_) => "symbol",
            Value::Cons { .. } => "pair",
            Value::Char(_) => "char",
            Value::Operative { .. } => "operative",
            Value::Applicative(_) => "applicative",
            Value::Builtin(_) => "builtin",
            Value::Environment { .. } => "environment",
            Value::Inert => "inert",
            Value::Ignore => "ignore",
            Value::Prelude(_) => "applicative",
            Value::Native(_) => "native",
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
                | Value::Char(_)
                | Value::Inert
                | Value::Ignore
                | Value::Prelude(_)
                | Value::Native(_)
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
    /// Render a compact placeholder-style description of the value.
    ///
    /// This formatter does not have arena access, so compound values such as
    /// strings, symbols, and lists are intentionally summarized rather than
    /// expanded. Use [`crate::Lisp::write_value`] when full Lisp syntax is
    /// required.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Value::Nil => f.write_str("()"),
            Value::Boolean(true) => f.write_str("#t"),
            Value::Boolean(false) => f.write_str("#f"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Char(ch) => write!(f, "#\\{ch}"),
            Value::Inert => f.write_str("#inert"),
            Value::Ignore => f.write_str("#ignore"),
            Value::Prelude(s) => write!(f, "<prelude:{}>", s.name()),
            Value::Native(_) => f.write_str("<native>"),
            _ => write!(f, "<{}>", self.type_name()),
        }
    }
}

/// Generate `From<T> for Value` implementations for simple wrapper variants.
///
/// This keeps the public constructors ergonomic while centralizing the
/// one-field conversion boilerplate in one place.
macro_rules! impl_from_value {
    ($($ty:ty => $variant:ident),+ $(,)?) => {
        $(impl From<$ty> for Value {
            #[inline]
            /// Wrap the Rust value in the corresponding [`Value`] variant.
            fn from(v: $ty) -> Self { Value::$variant(v) }
        })+
    };
}

impl_from_value!(bool => Boolean, isize => Number, BuiltinId => Builtin, Prelude => Prelude);

impl From<char> for Value {
    #[inline]
    /// Convert a Rust `char` into a Lisp character value.
    fn from(v: char) -> Self {
        Value::Char(v)
    }
}
