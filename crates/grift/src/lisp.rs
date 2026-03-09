//! The `Lisp` struct: arena wrapper with symbol interning and convenience methods.
//!
//! [`Lisp`] is the top-level entry point for the interpreter. It owns the
//! fixed-size [`Arena`](crate::arena::Arena), pre-allocates singleton values
//! and environments, manages symbol interning, and exposes the public
//! [`eval`](Lisp::eval) API.
//!
//! ## Slot Layout
//!
//! Slots 0–9 are reserved at construction time for well-known values
//! whose [`ArenaIndex`](crate::arena::ArenaIndex) constants are compile-time
//! values:
//!
//! | Slot | Contents |
//! |------|----------|
//! | 0 | `Nil` |
//! | 1 | `Boolean(true)` |
//! | 2 | `Boolean(false)` |
//! | 3 | `Inert` |
//! | 4 | `Ignore` |
//! | 5 | Ground environment |
//! | 6 | Parents cons cell (ground → global) |
//! | 7 | Global environment |
//! | 8 | GC root stack head |
//! | 9 | Symbol intern list head |

use crate::arena::{Arena, ArenaError, ArenaIndex, ArenaResult, ArenaStats, GcStats, Trace};

use crate::native::{LispOps, NativeFn};
use crate::parse::SliceSource;
use crate::value::Value;

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
    /// Construct a fresh interpreter with the default singleton layout,
    /// builtin set, and prelude bindings.
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Lisp<N> {
    /// Create a new Lisp interpreter.
    ///
    /// Slots 0–9 are pre-allocated for `Nil`, `#t`, `#f`, `#inert`,
    /// `#ignore`, the ground environment, a parents cell, the global
    /// environment, the GC root stack, and the symbol intern list so
    /// that returning these common values is allocation-free.
    ///
    /// # Panics
    ///
    /// Panics if `N < 10` or if the arena is too small to hold the
    /// singleton values, builtin bindings, and standard library entries.
    /// In practice `N` should be at least a few hundred.
    pub fn new() -> Self {
        let arena = Arena::new(Value::Nil);
        let nil_idx = arena
            .alloc(Value::Nil)
            .expect("arena too small for singletons");
        let true_idx = arena
            .alloc(Value::Boolean(true))
            .expect("arena too small for singletons");
        let false_idx = arena
            .alloc(Value::Boolean(false))
            .expect("arena too small for singletons");
        let inert_idx = arena
            .alloc(Value::Inert)
            .expect("arena too small for singletons");
        let ignore_idx = arena
            .alloc(Value::Ignore)
            .expect("arena too small for singletons");
        assert!(nil_idx == ArenaIndex::NIL, "NIL must be slot 0");
        assert!(true_idx == ArenaIndex::TRUE, "TRUE must be slot 1");
        assert!(false_idx == ArenaIndex::FALSE, "FALSE must be slot 2");
        assert!(inert_idx == ArenaIndex::INERT, "INERT must be slot 3");
        assert!(ignore_idx == ArenaIndex::IGNORE, "IGNORE must be slot 4");

        // Pre-allocate ground and global environments at fixed slots.
        let ground_idx = arena
            .alloc(Value::Environment {
                bindings: ArenaIndex::NIL,
                parents: ArenaIndex::NIL,
            })
            .expect("arena too small for environments");
        assert!(
            ground_idx == ArenaIndex::GROUND_ENV,
            "GROUND_ENV must be slot 5"
        );

        let lisp = Lisp {
            arena,
        };

        // Global env is a child of the ground env.
        let parents = lisp
            .cons(ArenaIndex::GROUND_ENV, ArenaIndex::NIL)
            .expect("arena too small for environments");
        let global_idx = lisp
            .arena
            .alloc(Value::Environment {
                bindings: ArenaIndex::NIL,
                parents,
            })
            .expect("arena too small for environments");
        assert!(
            global_idx == ArenaIndex::GLOBAL_ENV,
            "GLOBAL_ENV must be slot 7"
        );

        // Pre-allocate GC root stack at a fixed slot.
        // car = head of the gc roots linked list (initially NIL = empty).
        let gc_roots_idx = lisp
            .arena
            .alloc(Value::Cons {
                car: ArenaIndex::NIL,
                cdr: ArenaIndex::NIL,
            })
            .expect("arena too small for gc_roots");
        assert!(
            gc_roots_idx == ArenaIndex::GC_ROOTS,
            "GC_ROOTS must be slot 8"
        );

        // Pre-allocate symbol intern list at a fixed slot.
        // car = head of the intern alist (initially NIL = empty).
        let intern_idx = lisp
            .arena
            .alloc(Value::Cons {
                car: ArenaIndex::NIL,
                cdr: ArenaIndex::NIL,
            })
            .expect("arena too small for intern_list");
        assert!(
            intern_idx == ArenaIndex::INTERN_LIST,
            "INTERN_LIST must be slot 9"
        );

        // Initialize builtins into the ground environment.
        lisp.init_builtins();

        // Initialize prelude (proc-macro-generated statics + constants).
        lisp.init_prelude();

        lisp
    }

    /// Bind all prelude entries (proc-macro-generated statics + constants)
    /// in the global environment.
    fn init_prelude(&self) {
        use crate::prelude::{PRELUDE_ALL, init_prelude_constants};

        // Bind each prelude function as an Applicative wrapping the Prelude value.
        for &entry in PRELUDE_ALL {
            let prelude_idx = self
                .arena
                .alloc(Value::Prelude(entry))
                .expect("arena too small for prelude");
            let app_idx = self
                .arena
                .alloc(Value::Applicative(prelude_idx))
                .expect("arena too small for prelude");
            let sym = self
                .symbol(entry.name())
                .expect("arena too small for prelude");
            self.env_define(ArenaIndex::GLOBAL_ENV, sym, app_idx)
                .expect("arena too small for prelude");
        }

        // Bind prelude constants (numbers, booleans, strings).
        init_prelude_constants(self);
    }

    // — Native function registration —

    /// Register a native Rust function as a Lisp applicative.
    ///
    /// The function is bound in the global environment under the given `name`.
    /// It receives already-evaluated arguments as a cons-list.
    ///
    /// # Errors
    ///
    /// Returns [`ArenaError::OutOfMemory`] if arena allocation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift::{Lisp, Value, ArenaIndex, ArenaResult, LispOps};
    ///
    /// fn my_double(
    ///     lisp: &dyn LispOps,
    ///     args: ArenaIndex,
    /// ) -> ArenaResult<ArenaIndex> {
    ///     let (a, _) = grift::extract_arg::<isize>(lisp, args)?;
    ///     lisp.number(a * 2)
    /// }
    ///
    /// let lisp: Lisp<20000> = Lisp::new();
    /// lisp.register_native("double", my_double).unwrap();
    /// assert_eq!(lisp.eval("(double 21)"), Ok(Value::Number(42)));
    /// ```
    pub fn register_native(&self, name: &str, f: NativeFn) -> ArenaResult<()> {
        let sym = self.symbol(name)?;
        let native_val = self.arena.alloc(Value::Native(f))?;
        let wrapped = self.wrap(native_val)?;
        self.env_define(ArenaIndex::GLOBAL_ENV, sym, wrapped)
    }

    /// Call a native function pointer.
    pub(crate) fn call_native(
        &self,
        f: NativeFn,
        args: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        f(self, args)
    }

    /// Define a value in the global environment under the given symbol.
    ///
    /// This is the general-purpose equivalent of [`register_native`](Self::register_native)
    /// for arbitrary values. `sym` must be a valid symbol index.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift::{Lisp, Value, LispOps};
    ///
    /// let lisp: Lisp<20000> = Lisp::new();
    /// let sym = lisp.symbol("my-const").unwrap();
    /// let val = lisp.number(42).unwrap();
    /// lisp.define_global(sym, val).unwrap();
    /// assert_eq!(lisp.eval("my-const"), Ok(Value::Number(42)));
    /// ```
    pub fn define_global(&self, sym: ArenaIndex, value: ArenaIndex) -> ArenaResult<()> {
        self.env_define(ArenaIndex::GLOBAL_ENV, sym, value)
    }

    // — Value constructors —

    /// Allocate a number.
    #[inline]
    pub fn number(&self, n: isize) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(n.into())
    }

    /// Return the `ArenaIndex` for a boolean (`#t` or `#f`).
    ///
    /// Booleans are pre-allocated singletons, so this never allocates.
    #[inline]
    pub fn boolean(&self, b: bool) -> ArenaIndex {
        ArenaIndex::from_bool(b)
    }

    /// Return the `ArenaIndex` for nil (the empty list).
    ///
    /// Nil is a pre-allocated singleton, so this never allocates.
    #[inline]
    pub fn nil(&self) -> ArenaIndex {
        ArenaIndex::NIL
    }

    /// Allocate a cons cell.
    #[inline]
    pub fn cons(&self, car: ArenaIndex, cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Cons { car, cdr })
    }

    /// Allocate a character value.
    pub fn char_val(&self, c: char) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Char(c))
    }

    /// Allocate a symbol by name. Interns the symbol: if a symbol with the
    /// same name already exists in the intern list, returns the existing index.
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        if let Some(sym) = self.find_interned_symbol(|sym| Ok(self.symbol_name_eq(sym, name)))? {
            return Ok(sym);
        }

        let char_head = self.alloc_string(name)?;
        self.insert_interned_symbol(char_head)
    }

    /// Allocate a string value from a `&str`.
    ///
    /// Strings are stored as proper `Cons` lists whose elements are `Char`
    /// values. The empty string is represented as `NIL`.
    pub fn alloc_string(&self, s: &str) -> ArenaResult<ArenaIndex> {
        // Pre-check: ensure enough free slots for all chars.
        let char_count = if s.is_ascii() {
            s.len()
        } else {
            s.chars().count()
        };
        let needed = char_count.saturating_mul(2);
        if self.arena.available() < needed {
            return Err(ArenaError::OutOfMemory);
        }

        let mut data = ArenaIndex::NIL;

        // Build the linked list in reverse so the first char is at the head.
        for c in s.chars().rev() {
            let node = self.cons_char(data, c)?;
            data = node;
        }

        Ok(data)
    }

    fn char_list_step(&self, idx: ArenaIndex) -> ArenaResult<Option<(char, ArenaIndex)>> {
        if idx.is_nil() {
            return Ok(None);
        }
        let Value::Cons { car, cdr } = self.arena.get(idx)? else {
            return Err(ArenaError::TypeError);
        };
        let Value::Char(ch) = self.arena.get(car)? else {
            return Err(ArenaError::TypeError);
        };
        Ok(Some((ch, cdr)))
    }

    pub(crate) fn is_char_list(&self, idx: ArenaIndex) -> ArenaResult<bool> {
        let mut cur = idx;
        while !cur.is_nil() {
            let Value::Cons { car, cdr } = self.arena.get(cur)? else {
                return Ok(false);
            };
            if !matches!(self.arena.get(car)?, Value::Char(_)) {
                return Ok(false);
            }
            cur = cdr;
        }
        Ok(true)
    }

    pub(crate) fn is_nonempty_string(&self, idx: ArenaIndex) -> ArenaResult<bool> {
        if idx.is_nil() {
            return Ok(false);
        }
        self.is_char_list(idx)
    }

    /// Compare a char-list starting at `char_head` with a `&str`.
    fn string_eq(&self, char_head: ArenaIndex, s: &str) -> bool {
        let mut cur = char_head;
        for c in s.chars() {
            let Ok(Some((ch, cdr))) = self.char_list_step(cur) else {
                return false;
            };
            if ch != c {
                return false;
            }
            cur = cdr;
        }
        cur.is_nil()
    }

    /// Compare two arena-allocated strings by their char lists.
    pub(crate) fn strings_equal(
        &self,
        data_a: ArenaIndex,
        data_b: ArenaIndex,
    ) -> ArenaResult<bool> {
        if data_a == data_b {
            return Ok(true);
        }
        let mut cur_a = data_a;
        let mut cur_b = data_b;
        loop {
            match (cur_a.is_nil(), cur_b.is_nil()) {
                (true, true) => return Ok(true),
                (true, false) | (false, true) => return Ok(false),
                _ => {}
            }
            let Some((ca, next_a)) = self.char_list_step(cur_a)? else {
                return Err(ArenaError::TypeError);
            };
            let Some((cb, next_b)) = self.char_list_step(cur_b)? else {
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
            .and_then(super::value::Value::as_symbol)
            .is_ok_and(|str_idx| self.string_eq(str_idx, name))
    }

    fn find_interned_symbol(
        &self,
        mut matches: impl FnMut(ArenaIndex) -> ArenaResult<bool>,
    ) -> ArenaResult<Option<ArenaIndex>> {
        let mut cur = self.car(ArenaIndex::INTERN_LIST)?;
        while !cur.is_nil() {
            let sym = self.car(cur)?;
            if matches(sym)? {
                return Ok(Some(sym));
            }
            cur = self.cdr(cur)?;
        }
        Ok(None)
    }

    fn insert_interned_symbol(&self, char_head: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let sym_idx = self.arena.alloc(Value::Symbol(char_head))?;
        let intern_head = self.car(ArenaIndex::INTERN_LIST)?;
        let new_head = self.cons(sym_idx, intern_head)?;

        let Value::Cons { cdr, .. } = self.arena.get(ArenaIndex::INTERN_LIST)? else {
            unreachable!();
        };
        self.arena
            .set(ArenaIndex::INTERN_LIST, Value::Cons { car: new_head, cdr })?;
        Ok(sym_idx)
    }

    /// Prepend a character to the front of a char list.
    pub(crate) fn cons_char(&self, head: ArenaIndex, ch: char) -> ArenaResult<ArenaIndex> {
        let ch = self.char_val(ch)?;
        self.cons(ch, head)
    }

    /// Reverse a singly-linked cons list
    /// in place by swapping `cdr` pointers.
    pub(crate) fn reverse_list(&self, mut head: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let mut prev = ArenaIndex::NIL;
        while !head.is_nil() {
            let (car, cdr) = self.arena.get(head)?.as_cons()?;
            let new_val = Value::Cons { car, cdr: prev };
            self.arena.set(head, new_val)?;
            prev = head;
            head = cdr;
        }
        Ok(prev)
    }

    /// Intern a symbol from an existing char list.
    ///
    /// If a symbol with the same name already exists, returns the existing
    /// symbol index (the unused list is left as garbage for GC). Otherwise
    /// creates a new symbol using the provided char list as its name.
    pub(crate) fn symbol_from_chars(&self, char_head: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if let Some(sym) = self.find_interned_symbol(|sym| {
            let Value::Symbol(existing_chars) = self.arena.get(sym)? else {
                return Ok(false);
            };
            self.strings_equal(existing_chars, char_head)
        })? {
            return Ok(sym);
        }

        self.insert_interned_symbol(char_head)
    }

    /// Parse an integer from a char list.
    ///
    /// Returns `Some(n)` if the char list represents a valid integer literal
    /// (optional leading `-` or `+`, followed by one or more digits).
    /// Returns `None` otherwise.
    pub(crate) fn parse_integer_from_chars(&self, head: ArenaIndex) -> Option<isize> {
        let mut cur = head;
        let mut first = true;
        let mut negative = false;
        let mut result: isize = 0;
        let mut has_digits = false;

        while !cur.is_nil() {
            let (ch, cdr) = self.char_list_step(cur).ok()??;
            if first {
                first = false;
                if ch == '-' {
                    negative = true;
                    cur = cdr;
                    continue;
                } else if ch == '+' {
                    cur = cdr;
                    continue;
                }
            }
            if ch.is_ascii_digit() {
                has_digits = true;
                result = result.checked_mul(10)?;
                result = result.checked_add((ch as u8 - b'0') as isize)?;
            } else {
                return None;
            }
            cur = cdr;
        }

        if !has_digits {
            return None;
        }
        Some(if negative { -result } else { result })
    }

    /// Classify an atom represented as a char list.
    ///
    /// Checks for booleans (#t, #f, etc.), #inert, #ignore, numbers, and
    /// symbols.  Returns the appropriate arena value.
    pub(crate) fn classify_atom(&self, chars: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if self.string_eq(chars, "#t") || self.string_eq(chars, "#true") {
            Ok(ArenaIndex::TRUE)
        } else if self.string_eq(chars, "#f") || self.string_eq(chars, "#false") {
            Ok(ArenaIndex::FALSE)
        } else if self.string_eq(chars, "#inert") {
            Ok(ArenaIndex::INERT)
        } else if self.string_eq(chars, "#ignore") {
            Ok(ArenaIndex::IGNORE)
        } else if let Some(n) = self.parse_integer_from_chars(chars) {
            self.number(n)
        } else {
            self.symbol_from_chars(chars)
        }
    }

    // — Accessors —

    /// Get the value at an arena index.
    #[inline]
    pub fn get(&self, idx: ArenaIndex) -> ArenaResult<Value> {
        self.arena.get(idx)
    }

    /// Get car of a cons cell.
    #[inline(always)]
    pub fn car(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let Value::Cons { car, .. } = self.arena.get(idx)? else {
            return Err(ArenaError::TypeError);
        };
        Ok(car)
    }

    /// Get cdr of a cons cell.
    #[inline(always)]
    pub fn cdr(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let Value::Cons { cdr, .. } = self.arena.get(idx)? else {
            return Err(ArenaError::TypeError);
        };
        Ok(cdr)
    }

    /// Get car of cdr (second element of a list).
    #[inline(always)]
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

    /// Search an alist for a binding whose car equals `name`.
    /// Returns `Some(binding_cons_index)` if found, `None` otherwise.
    fn find_binding(
        &self,
        bindings: ArenaIndex,
        name: ArenaIndex,
    ) -> ArenaResult<Option<ArenaIndex>> {
        let mut cur = bindings;
        while !cur.is_nil() {
            let binding = self.car(cur)?;
            if self.car(binding)? == name {
                return Ok(Some(binding));
            }
            cur = self.cdr(cur)?;
        }
        Ok(None)
    }

    /// Define a binding in an environment (mutates in place via arena.set).
    /// Returns `AlreadyDefined` if a binding for `name` already exists in this frame.
    pub(crate) fn env_define(
        &self,
        env: ArenaIndex,
        name: ArenaIndex,
        val: ArenaIndex,
    ) -> ArenaResult<()> {
        let Value::Environment { bindings, parents } = self.arena.get(env)? else {
            return Err(ArenaError::TypeError);
        };
        if self.find_binding(bindings, name)?.is_some() {
            return Err(ArenaError::AlreadyDefined);
        }
        // Not found — create new binding
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

    /// Set an existing binding in an environment's own frame.
    /// Does NOT walk parents. Returns `UnboundVariable` if not found.
    pub(crate) fn env_set(
        &self,
        env: ArenaIndex,
        name: ArenaIndex,
        val: ArenaIndex,
    ) -> ArenaResult<()> {
        let Value::Environment { bindings, .. } = self.arena.get(env)? else {
            return Err(ArenaError::TypeError);
        };
        match self.find_binding(bindings, name)? {
            Some(binding) => self.arena.set(
                binding,
                Value::Cons {
                    car: name,
                    cdr: val,
                },
            ),
            None => Err(ArenaError::UnboundVariable),
        }
    }

    /// Look up a symbol in an environment.
    ///
    /// Fast path: walks single-parent chains with zero arena allocation.
    /// Falls back to DFS with cycle detection only for multi-parent
    /// environments (created by `make-environment`).
    #[inline]
    pub(crate) fn env_lookup(&self, env: ArenaIndex, name: ArenaIndex) -> ArenaResult<ArenaIndex> {
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

    /// Parse and evaluate Lisp expression(s) in a string.
    ///
    /// Multiple expressions are evaluated in sequence and the result of the
    /// last one is returned.  The global environment persists across calls
    /// so that bindings made by `define!` survive.
    ///
    /// # Errors
    ///
    /// Returns [`ArenaError`] on failure. Common variants include:
    /// - [`ParseError`](ArenaError::ParseError) — malformed S-expression.
    /// - [`UnboundVariable`](ArenaError::UnboundVariable) — undefined symbol.
    /// - [`TypeError`](ArenaError::TypeError) — wrong type for an operation.
    /// - [`OutOfMemory`](ArenaError::OutOfMemory) — arena exhausted even after GC.
    /// - [`ArithmeticOverflow`](ArenaError::ArithmeticOverflow) — integer overflow.
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
        let idx = self.eval_to_index(input)?;
        self.arena.get(idx)
    }

    /// Parse and evaluate Lisp expression(s), returning the arena index.
    ///
    /// Use with [`write_value`](Self::write_value) to properly display the
    /// result, including walking symbol names, string contents, and lists.
    ///
    /// # Errors
    ///
    /// Returns [`ArenaError`] on parse or evaluation failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift::Lisp;
    ///
    /// let lisp: Lisp<20000> = Lisp::new();
    /// let idx = lisp.eval_to_index("(+ 10 20)").unwrap();
    /// let mut buf = String::new();
    /// lisp.write_value(idx, &mut buf).unwrap();
    /// assert_eq!(buf, "30");
    /// ```
    pub fn eval_to_index(&self, input: &str) -> Result<ArenaIndex, ArenaError> {
        let mut src = SliceSource::new(input);

        let mut result_idx = ArenaIndex::INERT;
        while src.has_more() {
            let expr = self.parse_expr(&mut src)?;
            result_idx = self.eval_expr(expr, ArenaIndex::GLOBAL_ENV)?;
        }
        Ok(result_idx)
    }

    // — Arena introspection —

    /// Return arena allocation statistics.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift::Lisp;
    ///
    /// let lisp: Lisp<20000> = Lisp::new();
    /// let stats = lisp.stats();
    /// assert_eq!(stats.capacity, 20000);
    /// assert!(stats.allocated > 0); // singletons + builtins
    /// ```
    pub fn stats(&self) -> ArenaStats {
        self.arena.stats()
    }

    /// Return the number of arena slots consumed by the ground environment
    /// and builtins in a freshly constructed `Lisp` — before any user
    /// expressions are evaluated.
    ///
    /// **Note:** this method triggers a garbage collection cycle to measure
    /// the surviving allocation count. It should not be called in
    /// performance-sensitive code paths.
    ///
    /// Useful for computing test budgets: `baseline + constant` rather
    /// than a magic number.
    pub fn baseline_allocated(&self) -> usize {
        let _ = self.collect_garbage(&[]);
        self.stats().allocated
    }

    /// Run mark-and-sweep garbage collection with the given roots.
    ///
    /// Pass `&[]` to collect all unreachable objects.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift::Lisp;
    ///
    /// let lisp: Lisp<20000> = Lisp::new();
    /// lisp.eval("(define! x 42)").unwrap();
    /// let stats = lisp.collect_garbage(&[]);
    /// assert!(stats.marked > 0);
    /// ```
    pub fn collect_garbage(&self, roots: &[ArenaIndex]) -> GcStats {
        self.collect_with_roots(roots)
    }

    /// Collect garbage, always protecting the macro-generated singleton
    /// root set plus any caller-supplied `extra_roots`.
    pub(crate) fn collect_with_roots(&self, extra_roots: &[ArenaIndex]) -> GcStats {
        self.arena
            .collect_garbage_multi(&[ArenaIndex::ROOTS, extra_roots])
    }

    // — Value formatting —

    /// Write a human-readable representation of the value at `idx`.
    ///
    /// Unlike `Value::Display`, this method has arena access and can walk
    /// char lists to display full symbol names and string contents, and
    /// `Cons` lists to display proper/improper lists.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift::Lisp;
    ///
    /// let lisp: Lisp<20000> = Lisp::new();
    /// let idx = lisp.eval_to_index("(list 1 2 3)").unwrap();
    /// let mut buf = String::new();
    /// lisp.write_value(idx, &mut buf).unwrap();
    /// assert_eq!(buf, "(1 2 3)");
    /// ```
    pub fn write_value(&self, idx: ArenaIndex, w: &mut (impl core::fmt::Write + ?Sized)) -> core::fmt::Result {
        self.fmt_value(idx, w, false)
    }

    /// Human-readable output (like Scheme `display`).
    ///
    /// Same as [`write_value`](Self::write_value) except strings are printed
    /// without surrounding quotes or escape sequences.
    pub fn display_value(
        &self,
        idx: ArenaIndex,
        w: &mut (impl core::fmt::Write + ?Sized),
    ) -> core::fmt::Result {
        self.fmt_value(idx, w, true)
    }

    /// Unified value formatter. When `display` is true, strings are printed
    /// without quotes/escapes (like Scheme `display`); otherwise machine-
    /// readable (like Scheme `write`).
    pub(crate) fn fmt_value(
        &self,
        idx: ArenaIndex,
        w: &mut (impl core::fmt::Write + ?Sized),
        display: bool,
    ) -> core::fmt::Result {
        match self.arena.get(idx) {
            Ok(Value::Nil) => w.write_str("()"),
            Ok(Value::Boolean(true)) => w.write_str("#t"),
            Ok(Value::Boolean(false)) => w.write_str("#f"),
            Ok(Value::Number(n)) => write!(w, "{n}"),
            Ok(Value::Symbol(char_head)) => {
                self.walk_char_list(char_head, w, |ch, w| w.write_char(ch))
            }
            Ok(Value::Char(ch)) => self.fmt_char(ch, w),
            Ok(Value::Cons { .. }) if self.is_nonempty_string(idx).unwrap_or(false) => {
                self.fmt_string(idx, w, display)
            }
            Ok(Value::Cons { car, cdr }) => {
                w.write_char('(')?;
                self.fmt_value(car, w, display)?;
                self.fmt_list_tail(cdr, w, display)?;
                w.write_char(')')
            }
            Ok(Value::Inert) => w.write_str("#inert"),
            Ok(Value::Ignore) => w.write_str("#ignore"),
            Ok(Value::Prelude(s)) => write!(w, "<prelude:{}>", s.name()),
            Ok(val) => write!(w, "<{}>", val.type_name()),
            Err(_) => w.write_str("<error>"),
        }
    }

    fn fmt_char(
        &self,
        ch: char,
        w: &mut (impl core::fmt::Write + ?Sized),
    ) -> core::fmt::Result {
        match ch {
            '\n' => w.write_str("#\\newline"),
            '\t' => w.write_str("#\\tab"),
            '\r' => w.write_str("#\\return"),
            '\\' => w.write_str("#\\\\"),
            '"' => w.write_str("#\\\""),
            ' ' => w.write_str("#\\space"),
            c => write!(w, "#\\{c}"),
        }
    }

    fn fmt_string(
        &self,
        idx: ArenaIndex,
        w: &mut (impl core::fmt::Write + ?Sized),
        display: bool,
    ) -> core::fmt::Result {
        if !display {
            w.write_char('"')?;
        }
        self.walk_char_list(idx, w, |ch, w| {
            if display {
                w.write_char(ch)
            } else {
                Self::fmt_string_char(ch, w)
            }
        })?;
        if !display {
            w.write_char('"')?;
        }
        Ok(())
    }

    fn fmt_string_char(ch: char, w: &mut (impl core::fmt::Write + ?Sized)) -> core::fmt::Result {
        match ch {
            '"' => w.write_str("\\\""),
            '\\' => w.write_str("\\\\"),
            '\n' => w.write_str("\\n"),
            '\t' => w.write_str("\\t"),
            '\r' => w.write_str("\\r"),
            c => w.write_char(c),
        }
    }

    /// Walk a char list, emitting each character via a closure.
    fn walk_char_list<W: core::fmt::Write + ?Sized>(
        &self,
        mut idx: ArenaIndex,
        w: &mut W,
        mut emit: impl FnMut(char, &mut W) -> core::fmt::Result,
    ) -> core::fmt::Result {
        while !idx.is_nil() {
            let Ok(Some((ch, cdr))) = self.char_list_step(idx) else {
                break;
            };
            emit(ch, w)?;
            idx = cdr;
        }
        Ok(())
    }

    /// Write the tail of a list (elements after the first, with separators).
    fn fmt_list_tail(
        &self,
        mut idx: ArenaIndex,
        w: &mut (impl core::fmt::Write + ?Sized),
        display: bool,
    ) -> core::fmt::Result {
        while !idx.is_nil() {
            match self.arena.get(idx) {
                Ok(Value::Cons { car, cdr }) => {
                    w.write_char(' ')?;
                    self.fmt_value(car, w, display)?;
                    idx = cdr;
                }
                _ => {
                    w.write_str(" . ")?;
                    self.fmt_value(idx, w, display)?;
                    break;
                }
            }
        }
        Ok(())
    }
}

// ============================================================================
// LispOps trait implementation — delegates to inherent methods to erase `N`
// ============================================================================

impl<const N: usize> LispOps for Lisp<N> {
    #[inline]
    /// Delegate to [`Lisp::number`].
    fn number(&self, n: isize) -> ArenaResult<ArenaIndex> {
        self.number(n)
    }
    #[inline]
    /// Delegate to [`Lisp::boolean`].
    fn boolean(&self, b: bool) -> ArenaIndex {
        self.boolean(b)
    }
    #[inline]
    /// Delegate to [`Lisp::nil`].
    fn nil(&self) -> ArenaIndex {
        self.nil()
    }
    #[inline]
    /// Delegate to [`Lisp::cons`].
    fn cons(&self, car: ArenaIndex, cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.cons(car, cdr)
    }
    #[inline]
    /// Delegate to [`Lisp::char_val`].
    fn char_val(&self, c: char) -> ArenaResult<ArenaIndex> {
        self.char_val(c)
    }
    #[inline]
    /// Delegate to [`Lisp::alloc_string`].
    fn alloc_string(&self, s: &str) -> ArenaResult<ArenaIndex> {
        self.alloc_string(s)
    }
    #[inline]
    /// Delegate to [`Lisp::symbol`].
    fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        self.symbol(name)
    }
    #[inline]
    /// Delegate to [`Lisp::get`].
    fn get(&self, idx: ArenaIndex) -> ArenaResult<Value> {
        self.get(idx)
    }
    #[inline]
    /// Delegate to [`Lisp::car`].
    fn car(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        Lisp::car(self, idx)
    }
    #[inline]
    /// Delegate to [`Lisp::cdr`].
    fn cdr(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        Lisp::cdr(self, idx)
    }
    #[inline]
    /// Delegate to [`Lisp::cadr`].
    fn cadr(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        Lisp::cadr(self, idx)
    }
    #[inline]
    /// Delegate to [`Lisp::lambda`].
    fn lambda(
        &self,
        params: ArenaIndex,
        body: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        self.lambda(params, body, env)
    }
    #[inline]
    /// Delegate to [`Lisp::wrap`].
    fn wrap(&self, combiner: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.wrap(combiner)
    }
    #[inline]
    /// Delegate to [`Lisp::unwrap_applicative`].
    fn unwrap_applicative(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.unwrap_applicative(idx)
    }
    #[inline]
    /// Delegate to [`Lisp::vau`].
    fn vau(
        &self,
        params: ArenaIndex,
        env_param: ArenaIndex,
        body: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        self.vau(params, env_param, body, env)
    }
    #[inline]
    /// Delegate to [`Lisp::vau_parts`].
    fn vau_parts(
        &self,
        idx: ArenaIndex,
    ) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex)> {
        self.vau_parts(idx)
    }
    #[inline]
    /// Delegate to [`Lisp::eval`].
    fn eval(&self, input: &str) -> Result<Value, ArenaError> {
        self.eval(input)
    }
    #[inline]
    /// Delegate to [`Lisp::eval_to_index`].
    fn eval_to_index(&self, input: &str) -> Result<ArenaIndex, ArenaError> {
        self.eval_to_index(input)
    }
    #[inline]
    /// Delegate to [`Lisp::stats`].
    fn stats(&self) -> ArenaStats {
        self.stats()
    }
    #[inline]
    /// Delegate to [`Lisp::baseline_allocated`].
    fn baseline_allocated(&self) -> usize {
        self.baseline_allocated()
    }
    #[inline]
    /// Delegate to [`Lisp::collect_garbage`].
    fn collect_garbage(&self, roots: &[ArenaIndex]) -> GcStats {
        self.collect_garbage(roots)
    }
    #[inline]
    /// Format a value using write-style semantics.
    fn write_value(&self, idx: ArenaIndex, w: &mut dyn core::fmt::Write) -> core::fmt::Result {
        self.fmt_value(idx, w, false)
    }
    #[inline]
    /// Format a value using display-style semantics.
    fn display_value(&self, idx: ArenaIndex, w: &mut dyn core::fmt::Write) -> core::fmt::Result {
        self.fmt_value(idx, w, true)
    }
    #[inline]
    /// Delegate to [`Lisp::register_native`].
    fn register_native(&self, name: &str, f: NativeFn) -> ArenaResult<()> {
        self.register_native(name, f)
    }
    #[inline]
    /// Delegate to [`Lisp::define_global`].
    fn define_global(&self, sym: ArenaIndex, value: ArenaIndex) -> ArenaResult<()> {
        self.define_global(sym, value)
    }
}

// ============================================================================
// ArenaWriter — core::fmt::Write that builds a char list in the arena
// ============================================================================

/// A [`core::fmt::Write`] implementation that allocates `Char` values and
/// `Cons` cells directly into the arena, building a string without any
/// intermediate buffer.
pub(crate) struct ArenaWriter<'a, const N: usize> {
    lisp: &'a Lisp<N>,
    head: ArenaIndex,
    error: Option<ArenaError>,
}

impl<'a, const N: usize> ArenaWriter<'a, N> {
    /// Create an empty arena-backed string builder.
    pub(crate) fn new(lisp: &'a Lisp<N>) -> Self {
        ArenaWriter {
            lisp,
            head: ArenaIndex::NIL,
            error: None,
        }
    }

    /// Consume the writer and return the head of the char list
    /// in forward order, or an error if any allocation failed.
    pub(crate) fn finish(self) -> ArenaResult<ArenaIndex> {
        match self.error {
            Some(e) => Err(e),
            None => self.lisp.reverse_list(self.head),
        }
    }
}

impl<const N: usize> core::fmt::Write for ArenaWriter<'_, N> {
    /// Append text by allocating one `Char` and one `Cons` cell per character.
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.error.is_some() {
            return Err(core::fmt::Error);
        }
        for ch in s.chars() {
            match self.lisp.cons_char(self.head, ch) {
                Ok(h) => {
                    self.head = h;
                }
                Err(e) => {
                    self.error = Some(e);
                    return Err(core::fmt::Error);
                }
            }
        }
        Ok(())
    }
}

impl<const N: usize> Trace<Value, N> for Value {
    /// Visit every direct `ArenaIndex` child reachable from this value.
    ///
    /// The tracer intentionally skips inline scalars and singleton variants
    /// that do not own other arena objects.
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match *self {
            Value::Cons { car, cdr }
            | Value::Operative {
                params_envparam: car,
                body_env: cdr,
            }
            | Value::Environment {
                bindings: car,
                parents: cdr,
            } => {
                tracer(car);
                tracer(cdr);
            }
            Value::Applicative(inner) => tracer(inner),
            Value::Symbol(s) => tracer(s),
            _ => {}
        }
    }
}
