//! The `Lisp` struct: arena wrapper with symbol interning and convenience methods.

use grift_arena::{Arena, ArenaIndex, ArenaError, ArenaResult, ArenaStats, GcStats, Trace};

use crate::value::Value;
use crate::parse::Parser;
use crate::eval::Evaluator;

/// A minimalistic Lisp interpreter backed by a fixed-size arena.
///
/// The const generic `N` controls the arena capacity (number of slots).
///
/// # Example
///
/// ```rust
/// use grift::{Lisp, Value};
///
/// let lisp: Lisp<20000> = Lisp::new();
/// let result = lisp.eval("(+ 1 2)");
/// assert_eq!(result, Ok(Value::Number(3)));
/// ```
pub struct Lisp<const N: usize> {
    pub(crate) arena: Arena<Value, N>,
}

impl<const N: usize> Lisp<N> {
    /// Create a new Lisp interpreter with an empty arena.
    ///
    /// Slot 0 is pre-allocated as `Value::Nil`.
    pub fn new() -> Self {
        let arena = Arena::new(Value::Nil);
        // Pre-allocate slot 0 as Nil
        let _ = arena.alloc(Value::Nil);
        Lisp { arena }
    }

    // ========================================================================
    // Value constructors
    // ========================================================================

    /// Allocate a Nil value (or return the pre-allocated one).
    pub fn nil(&self) -> ArenaResult<ArenaIndex> {
        Ok(ArenaIndex::NIL)
    }

    /// Allocate a number.
    pub fn number(&self, n: isize) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Number(n))
    }

    /// Allocate a boolean.
    pub fn boolean(&self, b: bool) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(if b { Value::True } else { Value::False })
    }

    /// Allocate a cons cell.
    pub fn cons(&self, car: ArenaIndex, cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Cons { car, cdr })
    }

    /// Allocate a character.
    pub fn char_val(&self, c: char) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Char(c))
    }

    /// Allocate a symbol by name. Interns the symbol: if a symbol with the
    /// same name already exists, returns the existing index.
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        // Search for an existing symbol with the same name
        if let Some((idx, _)) = self.arena.find(|v| {
            if let Value::Symbol(str_idx) = v {
                self.string_eq(*str_idx, name)
            } else {
                false
            }
        }) {
            return Ok(idx);
        }

        // Allocate a new string for the symbol name
        let str_idx = self.alloc_string(name)?;
        self.arena.alloc(Value::Symbol(str_idx))
    }

    /// Allocate a string value from a `&str`.
    pub(crate) fn alloc_string(&self, s: &str) -> ArenaResult<ArenaIndex> {
        let len = s.chars().count();

        if len == 0 {
            return self.arena.alloc(Value::String {
                len: 0,
                data: ArenaIndex::NIL,
            });
        }

        // Allocate contiguous slots for characters
        let data = self.arena.alloc_contiguous(len, Value::Nil)?;
        let mut i = 0;
        for c in s.chars() {
            let idx = self.arena.index_at_offset(data, i)?;
            self.arena.set(idx, Value::Char(c))?;
            i += 1;
        }

        self.arena.alloc(Value::String { len, data })
    }

    /// Compare a `Value::String` at the given index with a `&str`.
    fn string_eq(&self, str_idx: ArenaIndex, s: &str) -> bool {
        let Ok(Value::String { len, data }) = self.arena.get(str_idx) else {
            return false;
        };
        if len != s.chars().count() {
            return false;
        }
        for (i, c) in s.chars().enumerate() {
            let Ok(idx) = self.arena.index_at_offset(data, i) else {
                return false;
            };
            let Ok(Value::Char(ch)) = self.arena.get(idx) else {
                return false;
            };
            if ch != c {
                return false;
            }
        }
        true
    }

    /// Get symbol name as a function that compares against a given string.
    /// Returns true if the symbol at `idx` has the given name.
    pub(crate) fn symbol_name_eq(&self, idx: ArenaIndex, name: &str) -> bool {
        match self.arena.get(idx) {
            Ok(Value::Symbol(str_idx)) => self.string_eq(str_idx, name),
            _ => false,
        }
    }

    // ========================================================================
    // Accessors
    // ========================================================================

    /// Get the value at an arena index.
    pub fn get(&self, idx: ArenaIndex) -> ArenaResult<Value> {
        self.arena.get(idx)
    }

    /// Get car of a cons cell.
    pub fn car(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.arena.get(idx)? {
            Value::Cons { car, .. } => Ok(car),
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    /// Get cdr of a cons cell.
    pub fn cdr(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.arena.get(idx)? {
            Value::Cons { cdr, .. } => Ok(cdr),
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    /// Allocate a lambda.
    pub fn lambda(
        &self,
        params: ArenaIndex,
        body: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let body_env = self.cons(body, env)?;
        self.arena.alloc(Value::Lambda {
            params,
            body_env,
        })
    }

    /// Extract lambda parts: (params, body, env).
    pub fn lambda_parts(
        &self,
        idx: ArenaIndex,
    ) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex)> {
        match self.arena.get(idx)? {
            Value::Lambda { params, body_env } => {
                let body = self.car(body_env)?;
                let env = self.cdr(body_env)?;
                Ok((params, body, env))
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    // ========================================================================
    // Evaluation entry point
    // ========================================================================

    /// Parse and evaluate a Lisp expression string.
    ///
    /// Returns the resulting `Value`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift::{Lisp, Value};
    ///
    /// let lisp: Lisp<20000> = Lisp::new();
    /// assert_eq!(lisp.eval("(+ 1 2)"), Ok(Value::Number(3)));
    /// ```
    pub fn eval(&self, input: &str) -> Result<Value, ArenaError> {
        let mut parser = Parser::new(input);
        let expr = parser.parse(self)?;
        let mut evaluator = Evaluator::new(self);
        let result_idx = evaluator.eval(expr, evaluator.global_env)?;
        self.arena.get(result_idx)
    }

    // ========================================================================
    // Arena introspection
    // ========================================================================

    /// Return arena allocation statistics.
    pub fn stats(&self) -> ArenaStats {
        self.arena.stats()
    }

    /// Run mark-and-sweep garbage collection with the given roots.
    ///
    /// Pass `&[]` to collect all unreachable objects.
    pub fn collect_garbage(&self, roots: &[ArenaIndex]) -> GcStats {
        self.arena.collect_garbage(roots)
    }
}

// Implement Trace for GC support
impl<const N: usize> Trace<Value, N> for Value {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match *self {
            Value::Cons { car, cdr } => {
                tracer(car);
                tracer(cdr);
            }
            Value::Symbol(s) => tracer(s),
            Value::Lambda { params, body_env } => {
                tracer(params);
                tracer(body_env);
            }
            Value::String { data, .. } => {
                if !data.is_nil() {
                    tracer(data);
                }
            }
            _ => {}
        }
    }
}
