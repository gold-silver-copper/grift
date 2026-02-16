//! Lisp value type.

use grift_arena::{ArenaError, ArenaIndex};

/// Type-safe identifier for built-in functions.
///
/// Wraps a `u8`, supporting up to 256 builtins. Generated automatically
/// by [`define_builtins!`] and matched in [`Evaluator::apply_builtin`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct BuiltinId(pub(crate) u8);

/// A Lisp value stored in the arena.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// The empty list / nil.
    Nil,
    Boolean(bool),
    /// Integer number.
    Number(isize),
    /// A symbol, pointing to a `String` value that holds the name.
    Symbol(ArenaIndex),
    /// A cons cell (pair) with inline car and cdr.
    Cons {
        car: ArenaIndex,
        cdr: ArenaIndex,
    },
    /// A string with inline length and pointer to character data.
    String {
        len: usize,
        data: ArenaIndex,
    },
    /// A character.
    Char(char),
    /// A lambda closure: params list and (body . env) cons cell.
    Lambda {
        params: ArenaIndex,
        body_env: ArenaIndex,
    },
    /// A built-in function identified by index.
    Builtin(BuiltinId),
    /// An unevaluated expression paired with the environment in which
    /// it should be evaluated when forced.
    Thunk {
        expr: ArenaIndex,
        env: ArenaIndex,
    },
    /// A thunk that is currently being forced (cycle detection).
    BlackHole,
    /// A forced thunk pointing to its evaluated result.
    Indirection(ArenaIndex),
}

/// Generate a `Value` accessor that pattern-matches on a variant and
/// returns its inner data, or `Err(InvalidIndex)` on mismatch.
macro_rules! value_accessor {
    ($(#[$m:meta])* $name:ident -> $out:ty, $pat:pat => $expr:expr) => {
        $(#[$m])*
        #[inline]
        pub fn $name(self) -> Result<$out, ArenaError> {
            match self {
                $pat => Ok($expr),
                _ => Err(ArenaError::InvalidIndex),
            }
        }
    };
}

impl Value {
    /// Returns the type name as a static string (for error messages).
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Boolean(_) => "boolean",
            Value::Number(_) => "number",
            Value::Symbol(_) => "symbol",
            Value::Cons { .. } => "pair",
            Value::String { .. } => "string",
            Value::Char(_) => "char",
            Value::Lambda { .. } => "lambda",
            Value::Builtin(_) => "builtin",
            Value::Thunk { .. } => "thunk",
            Value::BlackHole => "black-hole",
            Value::Indirection(_) => "indirection",
        }
    }

    /// True for values already in Weak Head Normal Form (WHNF).
    ///
    /// WHNF values need no further evaluation: they are fully resolved
    /// atoms, pairs, closures, or builtins.
    #[inline]
    pub fn is_whnf(self) -> bool {
        !matches!(
            self,
            Value::Thunk { .. } | Value::BlackHole | Value::Indirection(_)
        )
    }

    /// True for self-evaluating forms (literals, closures, builtins).
    ///
    /// Self-evaluating values are a subset of WHNF that additionally
    /// excludes symbols and cons cells (which require lookup / dispatch).
    #[inline]
    pub fn is_self_evaluating(self) -> bool {
        matches!(
            self,
            Value::Nil
                | Value::Boolean(_)
                | Value::Number(_)
                | Value::Char(_)
                | Value::String { .. }
                | Value::Builtin(_)
                | Value::Lambda { .. }
        )
    }

    value_accessor! {
        /// Extract the numeric value, or `Err(InvalidIndex)` if not a number.
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

    /// Returns `false` only for `Value::False`; all other values are truthy.
    #[inline]
    pub fn is_truthy(self) -> bool {
        !matches!(self, Value::Boolean(false))
    }
}

impl core::fmt::Display for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Value::Nil => f.write_str("()"),
            Value::Boolean(true) => f.write_str("#t"),
            Value::Boolean(false) => f.write_str("#f"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Char(c) => write!(f, "#\\{c}"),
            Value::Symbol(_) => f.write_str("<symbol>"),
            Value::Cons { .. } => f.write_str("<pair>"),
            Value::String { .. } => f.write_str("<string>"),
            Value::Lambda { .. } => f.write_str("<lambda>"),
            Value::Builtin(_) => f.write_str("<builtin>"),
            Value::Thunk { .. } => f.write_str("<thunk>"),
            Value::BlackHole => f.write_str("<black-hole>"),
            Value::Indirection(_) => f.write_str("<indirection>"),
        }
    }
}

impl From<bool> for Value {
    #[inline]
    fn from(b: bool) -> Self {
        Value::Boolean(b)
    }
}

impl From<isize> for Value {
    #[inline]
    fn from(n: isize) -> Self {
        Value::Number(n)
    }
}

impl From<char> for Value {
    #[inline]
    fn from(c: char) -> Self {
        Value::Char(c)
    }
}
