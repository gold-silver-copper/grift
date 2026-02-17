//! Lisp value type.

use grift_arena::{ArenaError, ArenaIndex};

/// Type-safe identifier for built-in functions.
///
/// Wraps a `u8`, supporting up to 256 builtins. Generated automatically
/// by [`define_builtins!`] and matched in [`Evaluator::apply_builtin`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct BuiltinId(pub(crate) u8);

impl core::ops::Deref for BuiltinId {
    type Target = u8;
    #[inline]
    fn deref(&self) -> &u8 {
        &self.0
    }
}

/// A Lisp value stored in the arena. Variants can only inline max two arenaindex sized data.
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
    /// `parent`: ArenaIndex to parent Environment, or NIL for top-level.
    Environment {
        bindings: ArenaIndex,
        parent: ArenaIndex,
    },
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
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Boolean(_) => "boolean",
            Value::Number(_) => "number",
            Value::Symbol(_) => "symbol",
            Value::Cons { .. } => "pair",
            Value::String { .. } => "string",
            Value::Char(_) => "char",
            Value::Operative { .. } => "operative",
            Value::Applicative(_) => "applicative",
            Value::Builtin(_) => "builtin",
            Value::Environment { .. } => "environment",
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

    /// Returns true only for bool true
    #[inline]
    pub fn is_truthy(self) -> bool {
        matches!(self, Value::Boolean(true))
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
            _ => write!(f, "<{}>", self.type_name()),
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
