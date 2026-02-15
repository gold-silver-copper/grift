//! Lisp value type.

use grift_arena::ArenaIndex;

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
            Value::Lambda { .. } => "procedure",
            Value::Builtin(_) => "procedure",
        }
    }
}
