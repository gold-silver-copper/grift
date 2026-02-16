//! Lisp value type.

use grift_arena::{ArenaIndex, ArenaError};

/// A Lisp value stored in the arena.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// The empty list / nil.
    Nil,
    /// Boolean true.
    True,
    /// Boolean false.
    False,
    /// Integer number.
    Number(isize),
    /// A symbol, pointing to a `String` value that holds the name.
    Symbol(ArenaIndex),
    /// A cons cell (pair) with inline car and cdr.
    Cons { car: ArenaIndex, cdr: ArenaIndex },
    /// A string with inline length and pointer to character data.
    String { len: usize, data: ArenaIndex },
    /// A character.
    Char(char),
    /// A lambda closure: params list and (body . env) cons cell.
    Lambda { params: ArenaIndex, body_env: ArenaIndex },
    /// A built-in function identified by index.
    Builtin(u8),
    /// An unevaluated expression paired with the environment in which
    /// it should be evaluated when forced.
    Thunk { expr: ArenaIndex, env: ArenaIndex },
    /// A thunk that is currently being forced (cycle detection).
    BlackHole,
    /// A forced thunk pointing to its evaluated result.
    Indirection(ArenaIndex),
}

impl Value {
    /// Returns the type name as a static string (for error messages).
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::True | Value::False => "boolean",
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

    /// Extract the numeric value, or return `InvalidIndex` if not a number.
    #[inline]
    pub fn as_number(self) -> Result<isize, ArenaError> {
        match self {
            Value::Number(n) => Ok(n),
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    /// Returns `false` only for `Value::False`; all other values are truthy.
    #[inline]
    pub fn is_truthy(self) -> bool {
        !matches!(self, Value::False)
    }
}
