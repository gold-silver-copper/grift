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
        let _ = arena.alloc(Value::Nil);
        Lisp { arena }
    }

    // — Value constructors —

    /// Allocate a Nil value (or return the pre-allocated one).
    #[inline]
    pub fn nil(&self) -> ArenaResult<ArenaIndex> {
        Ok(ArenaIndex::NIL)
    }

    /// Allocate a number.
    #[inline]
    pub fn number(&self, n: isize) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(n.into())
    }

    /// Allocate a boolean.
    #[inline]
    pub fn boolean(&self, b: bool) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(b.into())
    }

    /// Allocate an inert value.
    #[inline]
    pub fn inert(&self) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Inert)
    }

    /// Allocate a cons cell.
    #[inline]
    pub fn cons(&self, car: ArenaIndex, cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Cons { car, cdr })
    }

    /// Allocate a character.
    pub fn char_val(&self, c: char) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(c.into())
    }

    /// Allocate a symbol by name. Interns the symbol: if a symbol with the
    /// same name already exists, returns the existing index.
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        if let Some((idx, _)) = self.arena.find(|v| {
            v.as_symbol()
                .is_ok_and(|str_idx| self.string_eq(str_idx, name))
        }) {
            return Ok(idx);
        }

        let str_idx = self.alloc_string(name)?;
        self.arena.alloc(Value::Symbol(str_idx))
    }

    /// Allocate a string value from a `&str`.
    pub(crate) fn alloc_string(&self, s: &str) -> ArenaResult<ArenaIndex> {
        let len = if s.is_ascii() { s.len() } else { s.chars().count() };

        if len == 0 {
            return self.arena.alloc(Value::String {
                len: 0,
                data: ArenaIndex::NIL,
            });
        }

        let data = self.arena.alloc_contiguous(len, Value::Nil)?;
        for (i, c) in s.chars().enumerate() {
            let idx = self.arena.index_at_offset(data, i)?;
            self.arena.set(idx, c.into())?;
        }

        self.arena.alloc(Value::String { len, data })
    }

    /// Compare a `Value::String` at the given index with a `&str`.
    fn string_eq(&self, str_idx: ArenaIndex, s: &str) -> bool {
        let Ok(Value::String { len, data }) = self.arena.get(str_idx) else {
            return false;
        };
        // Fast path: for ASCII strings, byte length == char count.
        let char_count = if s.is_ascii() { s.len() } else { s.chars().count() };
        len == char_count
            && s.chars().enumerate().all(|(i, c)| {
                self.arena
                    .index_at_offset(data, i)
                    .and_then(|idx| self.arena.get(idx))
                    == Ok(Value::Char(c))
            })
    }

    /// Returns true if the symbol at `idx` has the given name.
    pub(crate) fn symbol_name_eq(&self, idx: ArenaIndex, name: &str) -> bool {
        self.arena
            .get(idx)
            .and_then(|v| v.as_symbol())
            .is_ok_and(|str_idx| self.string_eq(str_idx, name))
    }

    // — Accessors —

    /// Get the value at an arena index.
    #[inline]
    pub fn get(&self, idx: ArenaIndex) -> ArenaResult<Value> {
        self.arena.get(idx)
    }

    /// Get car of a cons cell.
    #[inline]
    pub fn car(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.get(idx)?.as_cons().map(|(car, _)| car)
    }

    /// Get cdr of a cons cell.
    #[inline]
    pub fn cdr(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.get(idx)?.as_cons().map(|(_, cdr)| cdr)
    }

    /// Get car of cdr (second element of a list).
    #[inline]
    pub fn cadr(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.car(self.cdr(idx)?)
    }

    /// Allocate a lambda (applicative from an operative that ignores caller env).
    /// This is sugar for: `(wrap (vau params #ignore body))` with closed env.
    /// Produces `Applicative(Operative { ... })` in the arena.
    pub fn lambda(
        &self,
        params: ArenaIndex,
        body: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let operative = self.vau(params, ArenaIndex::NIL, body, env)?;
        self.wrap(operative)
    }

    /// Wrap a combiner in an Applicative.
    pub fn wrap(&self, combiner: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Applicative(combiner))
    }

    /// Unwrap an Applicative to get the inner combiner.
    pub fn unwrap_applicative(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.get(idx)?.as_applicative()
    }

    /// Allocate an operative (fexpr / vau closure).
    pub fn vau(
        &self,
        params: ArenaIndex,
        env_param: ArenaIndex,
        body: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let params_envparam = self.cons(params, env_param)?;
        let body_env = self.cons(body, env)?;
        self.arena.alloc(Value::Operative {
            params_envparam,
            body_env,
        })
    }

    /// Extract operative parts: (params, env_param, body, env).
    pub fn vau_parts(
        &self,
        idx: ArenaIndex,
    ) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex)> {
        let Value::Operative {
            params_envparam,
            body_env,
        } = self.arena.get(idx)?
        else {
            return Err(ArenaError::TypeError);
        };
        let (params, env_param) = self.arena.get(params_envparam)?.as_cons()?;
        let (body, env) = self.arena.get(body_env)?.as_cons()?;
        Ok((params, env_param, body, env))
    }

    // — Environment operations —

    /// Create a root (top-level) environment with no parent.
    pub(crate) fn make_root_env(&self) -> ArenaResult<ArenaIndex> {
        self.make_child_env(ArenaIndex::NIL)
    }

    /// Create a child environment with the given parent.
    pub(crate) fn make_child_env(&self, parent: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Environment {
            bindings: ArenaIndex::NIL,
            parent,
        })
    }

    /// Define a binding in an environment (mutates in place via arena.set).
    pub(crate) fn env_define(
        &self,
        env: ArenaIndex,
        name: ArenaIndex,
        val: ArenaIndex,
    ) -> ArenaResult<()> {
        let Value::Environment { bindings, parent } = self.arena.get(env)? else {
            return Err(ArenaError::TypeError);
        };
        let pair = self.cons(name, val)?;
        let new_bindings = self.cons(pair, bindings)?;
        self.arena.set(
            env,
            Value::Environment {
                bindings: new_bindings,
                parent,
            },
        )
    }

    /// Look up a symbol in an environment, walking the parent chain.
    pub(crate) fn env_lookup(
        &self,
        env: ArenaIndex,
        name: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let mut cur_env = env;
        while !cur_env.is_nil() {
            let Value::Environment { bindings, parent } = self.arena.get(cur_env)? else {
                return Err(ArenaError::TypeError);
            };
            // Search bindings alist in this frame
            let mut cur = bindings;
            while !cur.is_nil() {
                let binding = self.car(cur)?;
                if self.car(binding)? == name {
                    return self.cdr(binding);
                }
                cur = self.cdr(cur)?;
            }
            cur_env = parent;
        }
        Err(ArenaError::UnboundVariable)
    }

    // — Evaluation entry point —

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

    // — Arena introspection —

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

impl<const N: usize> Trace<Value, N> for Value {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match *self {
            Value::Cons { car, cdr }
            | Value::Operative { params_envparam: car, body_env: cdr }
            | Value::Environment { bindings: car, parent: cdr } => {
                tracer(car);
                tracer(cdr);
            }
            Value::Applicative(inner) => tracer(inner),
            Value::Symbol(s) => tracer(s),
            Value::String { data, .. } if !data.is_nil() => tracer(data),
            _ => {}
        }
    }

    fn trace_with_arena<F: FnMut(ArenaIndex)>(&self, arena: &Arena<Value, N>, mut tracer: F) {
        match *self {
            Value::String { len, data } => {
                (0..len).for_each(|i| {
                    if let Ok(idx) = arena.index_at_offset(data, i) {
                        tracer(idx);
                    }
                });
            }
            _ => <Value as Trace<Value, N>>::trace(self, tracer),
        }
    }
}
