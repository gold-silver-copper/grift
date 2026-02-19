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

impl<const N: usize> Default for Lisp<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Lisp<N> {
    /// Create a new Lisp interpreter with an empty arena.
    ///
    /// Slots 0–4 are pre-allocated for `Nil`, `#t`, `#f`, `#inert`, and
    /// `#ignore` so that returning these common values is allocation-free.
    pub fn new() -> Self {
        let arena = Arena::new(Value::Nil);
        let nil_idx = arena.alloc(Value::Nil).expect("arena too small for singletons");
        let true_idx = arena.alloc(Value::Boolean(true)).expect("arena too small for singletons");
        let false_idx = arena.alloc(Value::Boolean(false)).expect("arena too small for singletons");
        let inert_idx = arena.alloc(Value::Inert).expect("arena too small for singletons");
        let ignore_idx = arena.alloc(Value::Ignore).expect("arena too small for singletons");
        assert!(nil_idx == ArenaIndex::NIL, "NIL must be slot 0");
        assert!(true_idx == ArenaIndex::TRUE, "TRUE must be slot 1");
        assert!(false_idx == ArenaIndex::FALSE, "FALSE must be slot 2");
        assert!(inert_idx == ArenaIndex::INERT, "INERT must be slot 3");
        assert!(ignore_idx == ArenaIndex::IGNORE, "IGNORE must be slot 4");
        Lisp { arena }
    }

    // — Value constructors —

    /// Allocate a number.
    #[inline]
    pub fn number(&self, n: isize) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(n.into())
    }

    /// Allocate a cons cell.
    #[inline]
    pub fn cons(&self, car: ArenaIndex, cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Cons { car, cdr })
    }

    /// Allocate a character.
    pub fn char_val(&self, c: char) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Char { ch: c, cdr: ArenaIndex::NIL })
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
    ///
    /// Strings are stored as a linked list of `Char` nodes.
    /// Each `Char { ch, cdr }` points to the next character,
    /// with the final character's `cdr` pointing to `NIL`.
    pub(crate) fn alloc_string(&self, s: &str) -> ArenaResult<ArenaIndex> {
        // Pre-check: ensure enough free slots for all chars + the String header.
        // This avoids orphaned Char nodes if allocation fails partway through.
        let char_count = if s.is_ascii() { s.len() } else { s.chars().count() };
        if self.arena.available() < char_count + 1 {
            return Err(ArenaError::OutOfMemory);
        }

        let mut data = ArenaIndex::NIL;

        // Build the linked list in reverse so the first char is at the head.
        for c in s.chars().rev() {
            let node = self.arena.alloc(Value::Char { ch: c, cdr: data })?;
            data = node;
        }

        self.arena.alloc(Value::String { data })
    }

    /// Compare a `Value::String` at the given index with a `&str`.
    fn string_eq(&self, str_idx: ArenaIndex, s: &str) -> bool {
        let Ok(Value::String { data }) = self.arena.get(str_idx) else {
            return false;
        };
        let mut cur = data;
        for c in s.chars() {
            let Ok(Value::Char { ch, cdr }) = self.arena.get(cur) else {
                return false;
            };
            if ch != c {
                return false;
            }
            cur = cdr;
        }
        // The string must be fully consumed (cur should be NIL).
        cur.is_nil()
    }

    /// Compare two arena-allocated `String` values by their character data.
    pub(crate) fn strings_equal(
        &self,
        data_a: ArenaIndex,
        data_b: ArenaIndex,
    ) -> ArenaResult<bool> {
        let mut cur_a = data_a;
        let mut cur_b = data_b;
        loop {
            match (cur_a.is_nil(), cur_b.is_nil()) {
                (true, true) => return Ok(true),
                (true, false) | (false, true) => return Ok(false),
                _ => {}
            }
            let Value::Char { ch: ca, cdr: next_a } = self.arena.get(cur_a)? else {
                return Err(ArenaError::TypeError);
            };
            let Value::Char { ch: cb, cdr: next_b } = self.arena.get(cur_b)? else {
                return Err(ArenaError::TypeError);
            };
            if ca != cb {
                return Ok(false);
            }
            cur_a = next_a;
            cur_b = next_b;
        }
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

    /// Create a new environment with a parents list (cons-list of parent
    /// environments, or NIL for a root environment).
    pub(crate) fn make_env(&self, parents: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Environment {
            bindings: ArenaIndex::NIL,
            parents,
        })
    }

    /// Create a child environment with the given single parent.
    pub(crate) fn make_child_env(&self, parent: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let parents = if parent.is_nil() {
            ArenaIndex::NIL
        } else {
            self.cons(parent, ArenaIndex::NIL)?
        };
        self.make_env(parents)
    }

    /// Define a binding in an environment (mutates in place via arena.set).
    pub(crate) fn env_define(
        &self,
        env: ArenaIndex,
        name: ArenaIndex,
        val: ArenaIndex,
    ) -> ArenaResult<()> {
        let Value::Environment { bindings, parents } = self.arena.get(env)? else {
            return Err(ArenaError::TypeError);
        };
        let pair = self.cons(name, val)?;
        let new_bindings = self.cons(pair, bindings)?;
        self.arena.set(
            env,
            Value::Environment {
                bindings: new_bindings,
                parents,
            },
        )
    }

    /// Look up a symbol in an environment.
    ///
    /// Fast path: walks single-parent chains with zero arena allocation.
    /// Falls back to DFS with cycle detection only for multi-parent
    /// environments (created by `make-environment`).
    #[inline]
    pub(crate) fn env_lookup(
        &self,
        env: ArenaIndex,
        name: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let mut cur = env;
        while !cur.is_nil() {
            let Value::Environment { bindings, parents } = self.arena.get(cur)? else {
                return Err(ArenaError::TypeError);
            };

            // Search local bindings.
            let mut b = bindings;
            while !b.is_nil() {
                let binding = self.car(b)?;
                if self.car(binding)? == name {
                    return self.cdr(binding);
                }
                b = self.cdr(b)?;
            }

            // Single parent → follow directly (no allocation needed).
            if parents.is_nil() {
                break;
            }
            let first_parent = self.car(parents)?;
            let rest = self.cdr(parents)?;
            if rest.is_nil() {
                cur = first_parent;
                continue;
            }

            // Multi-parent: fall back to DFS with cycle detection.
            return self.env_lookup_dfs_parents(parents, name, ArenaIndex::NIL);
        }
        Err(ArenaError::UnboundVariable)
    }

    /// Depth-first lookup with a visited-set (cons-list of env indices
    /// already searched).  Only used for multi-parent environments.
    fn env_lookup_dfs(
        &self,
        env: ArenaIndex,
        name: ArenaIndex,
        visited: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        if env.is_nil() {
            return Err(ArenaError::UnboundVariable);
        }

        // Cycle check: skip if already visited.
        if self.list_contains(visited, env) {
            return Err(ArenaError::UnboundVariable);
        }

        let Value::Environment { bindings, parents } = self.arena.get(env)? else {
            return Err(ArenaError::TypeError);
        };

        // Search local bindings.
        let mut cur = bindings;
        while !cur.is_nil() {
            let binding = self.car(cur)?;
            if self.car(binding)? == name {
                return self.cdr(binding);
            }
            cur = self.cdr(cur)?;
        }

        // Mark this env as visited, then search parents.
        let new_visited = self.cons(env, visited)?;
        self.env_lookup_dfs_parents(parents, name, new_visited)
    }

    /// Search a list of parent environments via DFS.
    fn env_lookup_dfs_parents(
        &self,
        parents: ArenaIndex,
        name: ArenaIndex,
        visited: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let mut parent_cur = parents;
        while !parent_cur.is_nil() {
            let parent_env = self.car(parent_cur)?;
            match self.env_lookup_dfs(parent_env, name, visited) {
                Ok(val) => return Ok(val),
                Err(ArenaError::UnboundVariable) => {}
                Err(e) => return Err(e),
            }
            parent_cur = self.cdr(parent_cur)?;
        }
        Err(ArenaError::UnboundVariable)
    }

    /// Check if a cons-list contains a given index (by ArenaIndex identity).
    pub(crate) fn list_contains(&self, list: ArenaIndex, target: ArenaIndex) -> bool {
        let mut cur = list;
        while !cur.is_nil() {
            if let Ok(head) = self.car(cur)
                && head == target
            {
                return true;
            }
            match self.cdr(cur) {
                Ok(rest) => cur = rest,
                Err(_) => break,
            }
        }
        false
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
        let mut evaluator = match Evaluator::new(self) {
            Ok(e) => e,
            Err(ArenaError::OutOfMemory) => {
                // Collect garbage keeping only singletons and the parsed expression,
                // then retry evaluator creation.
                self.arena.collect_garbage(&[
                    ArenaIndex::TRUE, ArenaIndex::FALSE,
                    ArenaIndex::INERT, ArenaIndex::IGNORE, expr,
                ]);
                Evaluator::new(self)?
            }
            Err(e) => return Err(e),
        };
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
        // Always protect the pre-allocated singletons (NIL, TRUE, FALSE,
        // INERT, IGNORE) so they remain valid after collection.
        self.arena.collect_garbage_multi(&[
            roots,
            &[
                ArenaIndex::NIL, ArenaIndex::TRUE, ArenaIndex::FALSE,
                ArenaIndex::INERT, ArenaIndex::IGNORE,
            ],
        ])
    }
}

impl<const N: usize> Trace<Value, N> for Value {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match *self {
            Value::Cons { car, cdr }
            | Value::Operative { params_envparam: car, body_env: cdr }
            | Value::Environment { bindings: car, parents: cdr } => {
                tracer(car);
                tracer(cdr);
            }
            Value::Char { cdr, .. } if !cdr.is_nil() => tracer(cdr),
            Value::Applicative(inner) => tracer(inner),
            Value::Symbol(s) => tracer(s),
            Value::String { data } if !data.is_nil() => tracer(data),
            _ => {}
        }
    }
}
