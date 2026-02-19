//! The `Lisp` struct: arena wrapper with symbol interning and convenience methods.

use grift_arena::{Arena, ArenaIndex, ArenaError, ArenaResult, ArenaStats, GcStats, Trace};

use crate::value::Value;
use crate::parse::Parser;

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
    /// Slots 0–8 are pre-allocated for `Nil`, `#t`, `#f`, `#inert`,
    /// `#ignore`, the ground environment, a parents cell, the global
    /// environment, and the GC root stack so that returning these common
    /// values is allocation-free.
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

        // Pre-allocate ground and global environments at fixed slots.
        let ground_idx = arena.alloc(Value::Environment {
            bindings: ArenaIndex::NIL,
            parents: ArenaIndex::NIL,
        }).expect("arena too small for environments");
        assert!(ground_idx == ArenaIndex::GROUND_ENV, "GROUND_ENV must be slot 5");

        let lisp = Lisp { arena };

        // Global env is a child of the ground env.
        let parents = lisp.cons(ArenaIndex::GROUND_ENV, ArenaIndex::NIL)
            .expect("arena too small for environments");
        let global_idx = lisp.arena.alloc(Value::Environment {
            bindings: ArenaIndex::NIL,
            parents,
        }).expect("arena too small for environments");
        assert!(global_idx == ArenaIndex::GLOBAL_ENV, "GLOBAL_ENV must be slot 7");

        // Pre-allocate GC root stack at a fixed slot.
        // car = head of the gc roots linked list (initially NIL = empty).
        let gc_roots_idx = lisp.arena.alloc(Value::Cons {
            car: ArenaIndex::NIL,
            cdr: ArenaIndex::NIL,
        }).expect("arena too small for gc_roots");
        assert!(gc_roots_idx == ArenaIndex::GC_ROOTS, "GC_ROOTS must be slot 8");

        // Initialize builtins into the ground environment.
        lisp.init_builtins();

        lisp
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

    /// Allocate a character (one-element string).
    pub fn char_val(&self, c: char) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::CharPair { ch: c, cdr: ArenaIndex::NIL })
    }

    /// Allocate a symbol by name. Interns the symbol: if a symbol with the
    /// same name already exists, returns the existing index.
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        if let Some((idx, _)) = self.arena.find(|v| {
            v.as_symbol()
                .is_ok_and(|char_head| self.string_eq(char_head, name))
        }) {
            return Ok(idx);
        }

        let char_head = self.alloc_string(name)?;
        self.arena.alloc(Value::Symbol(char_head))
    }

    /// Allocate a string value from a `&str`.
    ///
    /// Strings are stored as a linked list of `CharPair` nodes.
    /// Each `CharPair { ch, cdr }` points to the next character,
    /// with the final character's `cdr` pointing to `NIL`.
    /// An empty string is represented as `NIL`.
    pub(crate) fn alloc_string(&self, s: &str) -> ArenaResult<ArenaIndex> {
        // Pre-check: ensure enough free slots for all chars.
        let char_count = if s.is_ascii() { s.len() } else { s.chars().count() };
        if self.arena.available() < char_count {
            return Err(ArenaError::OutOfMemory);
        }

        let mut data = ArenaIndex::NIL;

        // Build the linked list in reverse so the first char is at the head.
        for c in s.chars().rev() {
            let node = self.arena.alloc(Value::CharPair { ch: c, cdr: data })?;
            data = node;
        }

        Ok(data)
    }

    /// Compare a `CharPair` linked list starting at `char_head` with a `&str`.
    fn string_eq(&self, char_head: ArenaIndex, s: &str) -> bool {
        let mut cur = char_head;
        for c in s.chars() {
            let Ok(Value::CharPair { ch, cdr }) = self.arena.get(cur) else {
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

    /// Compare two arena-allocated strings by their `CharPair` chains.
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
            let Value::CharPair { ch: ca, cdr: next_a } = self.arena.get(cur_a)? else {
                return Err(ArenaError::TypeError);
            };
            let Value::CharPair { ch: cb, cdr: next_b } = self.arena.get(cur_b)? else {
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

    /// Get car of a cons cell or CharPair (user-facing).
    /// For CharPair, allocates a fresh one-element string.
    #[inline]
    pub fn car(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.arena.get(idx)? {
            Value::Cons { car, .. } => Ok(car),
            Value::CharPair { ch, .. } => {
                self.arena.alloc(Value::CharPair { ch, cdr: ArenaIndex::NIL })
            }
            _ => Err(ArenaError::TypeError),
        }
    }

    /// Get cdr of a cons cell or CharPair (user-facing).
    #[inline]
    pub fn cdr(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.arena.get(idx)? {
            Value::Cons { cdr, .. } | Value::CharPair { cdr, .. } => Ok(cdr),
            _ => Err(ArenaError::TypeError),
        }
    }

    /// Get car of cdr (second element of a list, user-facing).
    #[inline]
    pub fn cadr(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.car(self.cdr(idx)?)
    }

    /// Get car of a Cons cell only (internal hot-path accessor).
    #[inline(always)]
    pub(crate) fn car_cons(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let Value::Cons { car, .. } = self.arena.get(idx)? else {
            return Err(ArenaError::TypeError);
        };
        Ok(car)
    }

    /// Get cdr of a Cons cell only (internal hot-path accessor).
    #[inline(always)]
    pub(crate) fn cdr_cons(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let Value::Cons { cdr, .. } = self.arena.get(idx)? else {
            return Err(ArenaError::TypeError);
        };
        Ok(cdr)
    }

    /// Get car of cdr of Cons cells only (internal hot-path accessor).
    #[inline(always)]
    pub(crate) fn cadr_cons(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.car_cons(self.cdr_cons(idx)?)
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
                let binding = self.car_cons(b)?;
                if self.car_cons(binding)? == name {
                    return self.cdr_cons(binding);
                }
                b = self.cdr_cons(b)?;
            }

            // Single parent → follow directly (no allocation needed).
            if parents.is_nil() {
                break;
            }
            let first_parent = self.car_cons(parents)?;
            let rest = self.cdr_cons(parents)?;
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
            let binding = self.car_cons(cur)?;
            if self.car_cons(binding)? == name {
                return self.cdr_cons(binding);
            }
            cur = self.cdr_cons(cur)?;
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
            let parent_env = self.car_cons(parent_cur)?;
            match self.env_lookup_dfs(parent_env, name, visited) {
                Ok(val) => return Ok(val),
                Err(ArenaError::UnboundVariable) => {}
                Err(e) => return Err(e),
            }
            parent_cur = self.cdr_cons(parent_cur)?;
        }
        Err(ArenaError::UnboundVariable)
    }

    /// Check if a cons-list contains a given index (by ArenaIndex identity).
    pub(crate) fn list_contains(&self, list: ArenaIndex, target: ArenaIndex) -> bool {
        let mut cur = list;
        while !cur.is_nil() {
            if let Ok(head) = self.car_cons(cur)
                && head == target
            {
                return true;
            }
            match self.cdr_cons(cur) {
                Ok(rest) => cur = rest,
                Err(_) => break,
            }
        }
        false
    }

    // — Evaluation entry point —

    /// Parse and evaluate Lisp expression(s) in a string.
    ///
    /// Multiple expressions are evaluated in sequence and the result of the
    /// last one is returned.  The global environment persists across calls
    /// so that bindings made by `define!` survive.
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

        let mut result_idx = ArenaIndex::INERT;
        while parser.has_more() {
            let expr = parser.parse(self)?;
            result_idx = self.eval_expr(expr, ArenaIndex::GLOBAL_ENV)?;
        }
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
        // INERT, IGNORE), the ground/global environments, and the GC root
        // stack so they remain valid after collection.
        self.arena.collect_garbage_multi(&[
            roots,
            &[
                ArenaIndex::NIL, ArenaIndex::TRUE, ArenaIndex::FALSE,
                ArenaIndex::INERT, ArenaIndex::IGNORE,
                ArenaIndex::GROUND_ENV, ArenaIndex::GLOBAL_ENV,
                ArenaIndex::GC_ROOTS,
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
            Value::CharPair { cdr, .. } if !cdr.is_nil() => tracer(cdr),
            Value::Applicative(inner) => tracer(inner),
            Value::Symbol(s) => tracer(s),
            _ => {}
        }
    }
}
