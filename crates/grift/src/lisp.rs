//! The `Lisp` struct: arena wrapper with symbol interning and convenience methods.

use core::cell::Cell;

use grift_arena::{Arena, ArenaError, ArenaIndex, ArenaResult, ArenaStats, GcStats, Trace};

use crate::io::{IoProvider, PortId};
use crate::parse::Parser;
use crate::value::Value;

/// Function pointer type for I/O write operations.
///
/// Used to bridge between the `&self`-based eval loop and mutable
/// I/O providers. The function takes a port ID and a string slice.
type WriteFn = fn(PortId, &str);

/// Default no-op write function (discards all output).
fn null_write(_port: PortId, _s: &str) {}

/// A minimalistic Lisp interpreter backed by a fixed-size arena.
///
/// The const generic `N` controls the arena capacity (number of slots).
///
/// # I/O Model
///
/// The interpreter uses an [`IoProvider`] trait for all output operations.
/// By default, output is discarded ([`NullIoProvider`] semantics). Call
/// [`set_io`](Self::set_io) with a write function to enable output, or use
/// [`eval_with_io`](Self::eval_with_io) to evaluate with a specific provider.
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
    /// I/O write callback, stored in a Cell for interior mutability.
    pub(crate) write_fn: Cell<WriteFn>,
}

impl<const N: usize> Default for Lisp<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Lisp<N> {
    /// Create a new Lisp interpreter with an empty arena.
    ///
    /// Slots 0–9 are pre-allocated for `Nil`, `#t`, `#f`, `#inert`,
    /// `#ignore`, the ground environment, a parents cell, the global
    /// environment, the GC root stack, and the symbol intern list so
    /// that returning these common values is allocation-free.
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
            write_fn: Cell::new(null_write),
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
        self.arena.alloc(Value::CharPair {
            ch: c,
            cdr: ArenaIndex::NIL,
        })
    }

    /// Allocate a symbol by name. Interns the symbol: if a symbol with the
    /// same name already exists in the intern list, returns the existing index.
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        // Walk the intern alist
        let intern_head = self.car(ArenaIndex::INTERN_LIST)?;
        let mut cur = intern_head;
        while !cur.is_nil() {
            let sym = self.car(cur)?;
            if self.symbol_name_eq(sym, name) {
                return Ok(sym);
            }
            cur = self.cdr(cur)?;
        }

        // Not found — allocate new symbol and prepend to intern list
        let char_head = self.alloc_string(name)?;
        let sym_idx = self.arena.alloc(Value::Symbol(char_head))?;
        let new_head = self.cons(sym_idx, intern_head)?;

        // Update the intern list head in place
        let Value::Cons { cdr, .. } = self.arena.get(ArenaIndex::INTERN_LIST)? else {
            unreachable!();
        };
        self.arena
            .set(ArenaIndex::INTERN_LIST, Value::Cons { car: new_head, cdr })?;

        Ok(sym_idx)
    }

    /// Allocate a string value from a `&str`.
    ///
    /// Strings are stored as a linked list of `CharPair` nodes.
    /// Each `CharPair { ch, cdr }` points to the next character,
    /// with the final character's `cdr` pointing to `NIL`.
    /// An empty string is represented as `NIL`.
    pub(crate) fn alloc_string(&self, s: &str) -> ArenaResult<ArenaIndex> {
        // Pre-check: ensure enough free slots for all chars.
        let char_count = if s.is_ascii() {
            s.len()
        } else {
            s.chars().count()
        };
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
            let Value::CharPair {
                ch: ca,
                cdr: next_a,
            } = self.arena.get(cur_a)?
            else {
                return Err(ArenaError::TypeError);
            };
            let Value::CharPair {
                ch: cb,
                cdr: next_b,
            } = self.arena.get(cur_b)?
            else {
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
    pub fn car_char(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.arena.get(idx)? {
            Value::Cons { car, .. } => Ok(car),
            Value::CharPair { ch, .. } => self.arena.alloc(Value::CharPair {
                ch,
                cdr: ArenaIndex::NIL,
            }),
            _ => Err(ArenaError::TypeError),
        }
    }

    /// Get cdr of a cons cell or CharPair (user-facing).
    #[inline]
    pub fn cdr_char(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.arena.get(idx)? {
            Value::Cons { cdr, .. } | Value::CharPair { cdr, .. } => Ok(cdr),
            _ => Err(ArenaError::TypeError),
        }
    }

    /// Get car of cdr (second element of a list, user-facing).
    #[inline]
    pub fn cadr_char(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.car_char(self.cdr_char(idx)?)
    }

    /// Get car of a Cons cell only (internal hot-path accessor).
    #[inline(always)]
    pub(crate) fn car(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let Value::Cons { car, .. } = self.arena.get(idx)? else {
            return Err(ArenaError::TypeError);
        };
        Ok(car)
    }

    /// Get cdr of a Cons cell only (internal hot-path accessor).
    #[inline(always)]
    pub(crate) fn cdr(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let Value::Cons { cdr, .. } = self.arena.get(idx)? else {
            return Err(ArenaError::TypeError);
        };
        Ok(cdr)
    }

    /// Get car of cdr of Cons cells only (internal hot-path accessor).
    #[inline(always)]
    pub(crate) fn cadr(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
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
    fn find_binding(&self, bindings: ArenaIndex, name: ArenaIndex) -> ArenaResult<Option<ArenaIndex>> {
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
    /// If a binding for `name` already exists in this frame, overwrite it.
    pub(crate) fn env_define(
        &self,
        env: ArenaIndex,
        name: ArenaIndex,
        val: ArenaIndex,
    ) -> ArenaResult<()> {
        let Value::Environment { bindings, parents } = self.arena.get(env)? else {
            return Err(ArenaError::TypeError);
        };
        if let Some(binding) = self.find_binding(bindings, name)? {
            return self.arena.set(binding, Value::Cons { car: name, cdr: val });
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
            Some(binding) => self.arena.set(binding, Value::Cons { car: name, cdr: val }),
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
    pub fn eval_to_index(&self, input: &str) -> Result<ArenaIndex, ArenaError> {
        let mut parser = Parser::new(input);

        let mut result_idx = ArenaIndex::INERT;
        while parser.has_more() {
            let expr = parser.parse(self)?;
            result_idx = self.eval_expr(expr, ArenaIndex::GLOBAL_ENV)?;
        }
        Ok(result_idx)
    }

    // — I/O integration —

    /// Set the I/O write function used by `display` and `newline` builtins.
    ///
    /// The function pointer is stored internally and called whenever the
    /// Lisp program invokes `(display ...)` or `(newline)`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use grift::{Lisp, io::PortId};
    ///
    /// let lisp: Lisp<20000> = Lisp::new();
    /// lisp.set_io(|port, s| {
    ///     // Write to your I/O device here
    /// });
    /// ```
    pub fn set_io(&self, write: fn(PortId, &str)) {
        self.write_fn.set(write);
    }

    /// Write a value's display representation through an [`IoProvider`].
    ///
    /// This is the trait-based equivalent of the `display` builtin,
    /// suitable for use from Rust code with any I/O backend.
    pub fn display_to_io(
        &self,
        idx: ArenaIndex,
        port: PortId,
        io: &mut dyn IoProvider,
    ) -> crate::io::IoResult<()> {
        let mut buf = [0u8; 256];
        let mut writer = StackWriter::new(&mut buf);
        let _ = self.display_value(idx, &mut writer);
        io.write_str(port, writer.as_str())
    }

    /// Write a value's write representation through an [`IoProvider`].
    ///
    /// This is the trait-based equivalent of `write_value`,
    /// suitable for use from Rust code with any I/O backend.
    pub fn write_to_io(
        &self,
        idx: ArenaIndex,
        port: PortId,
        io: &mut dyn IoProvider,
    ) -> crate::io::IoResult<()> {
        let mut buf = [0u8; 256];
        let mut writer = StackWriter::new(&mut buf);
        let _ = self.write_value(idx, &mut writer);
        io.write_str(port, writer.as_str())
    }

    /// Emit I/O output through the currently configured write function.
    ///
    /// Called by the `display` and `newline` builtins.
    pub(crate) fn io_write(&self, port: PortId, s: &str) {
        (self.write_fn.get())(port, s);
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
        self.collect_with_roots(roots)
    }

    /// Collect garbage, always protecting the macro-generated singleton
    /// root set plus any caller-supplied `extra_roots`.
    pub(crate) fn collect_with_roots(&self, extra_roots: &[ArenaIndex]) -> GcStats {
        self.arena.collect_garbage_multi(&[
            ArenaIndex::ROOTS,
            extra_roots,
        ])
    }

    // — Value formatting —

    /// Write a human-readable representation of the value at `idx`.
    ///
    /// Unlike `Value::Display`, this method has arena access and can walk
    /// `CharPair` chains to display full symbol names and string contents,
    /// and `Cons` chains to display proper/improper lists.
    pub fn write_value(&self, idx: ArenaIndex, w: &mut impl core::fmt::Write) -> core::fmt::Result {
        match self.arena.get(idx) {
            Ok(Value::Nil) => w.write_str("()"),
            Ok(Value::Boolean(true)) => w.write_str("#t"),
            Ok(Value::Boolean(false)) => w.write_str("#f"),
            Ok(Value::Number(n)) => write!(w, "{n}"),
            Ok(Value::Symbol(char_head)) => self.walk_chars(char_head, w, |ch, w| w.write_char(ch)),
            Ok(Value::CharPair { .. }) => {
                w.write_char('"')?;
                self.walk_chars(idx, w, |ch, w| match ch {
                    '"' => w.write_str("\\\""),
                    '\\' => w.write_str("\\\\"),
                    '\n' => w.write_str("\\n"),
                    '\t' => w.write_str("\\t"),
                    '\r' => w.write_str("\\r"),
                    c => w.write_char(c),
                })?;
                w.write_char('"')
            }
            Ok(Value::Cons { car, cdr }) => {
                w.write_char('(')?;
                self.write_value(car, w)?;
                self.write_list_tail(cdr, w)?;
                w.write_char(')')
            }
            Ok(Value::Inert) => w.write_str("#inert"),
            Ok(Value::Ignore) => w.write_str("#ignore"),
            Ok(val) => write!(w, "<{}>", val.type_name()),
            Err(_) => w.write_str("<error>"),
        }
    }

    /// Human-readable output (like Scheme `display`).
    ///
    /// Same as [`write_value`](Self::write_value) except strings are printed
    /// without surrounding quotes or escape sequences.
    pub fn display_value(&self, idx: ArenaIndex, w: &mut impl core::fmt::Write) -> core::fmt::Result {
        match self.arena.get(idx) {
            Ok(Value::CharPair { .. }) => {
                self.walk_chars(idx, w, |ch, w| w.write_char(ch))
            }
            Ok(Value::Cons { car, cdr }) => {
                w.write_char('(')?;
                self.display_value(car, w)?;
                self.display_list_tail(cdr, w)?;
                w.write_char(')')
            }
            _ => self.write_value(idx, w),
        }
    }

    /// Display-mode list tail (no quoting on strings).
    fn display_list_tail(
        &self,
        mut idx: ArenaIndex,
        w: &mut impl core::fmt::Write,
    ) -> core::fmt::Result {
        while !idx.is_nil() {
            match self.arena.get(idx) {
                Ok(Value::Cons { car, cdr }) => {
                    w.write_char(' ')?;
                    self.display_value(car, w)?;
                    idx = cdr;
                }
                _ => {
                    w.write_str(" . ")?;
                    self.display_value(idx, w)?;
                    break;
                }
            }
        }
        Ok(())
    }

    /// Walk a CharPair chain, emitting each character via a closure.
    fn walk_chars<W: core::fmt::Write>(
        &self,
        mut idx: ArenaIndex,
        w: &mut W,
        mut emit: impl FnMut(char, &mut W) -> core::fmt::Result,
    ) -> core::fmt::Result {
        while !idx.is_nil() {
            match self.arena.get(idx) {
                Ok(Value::CharPair { ch, cdr }) => {
                    emit(ch, w)?;
                    idx = cdr;
                }
                _ => break,
            }
        }
        Ok(())
    }

    /// Write the tail of a list (elements after the first, with separators).
    fn write_list_tail(
        &self,
        mut idx: ArenaIndex,
        w: &mut impl core::fmt::Write,
    ) -> core::fmt::Result {
        while !idx.is_nil() {
            match self.arena.get(idx) {
                Ok(Value::Cons { car, cdr }) => {
                    w.write_char(' ')?;
                    self.write_value(car, w)?;
                    idx = cdr;
                }
                _ => {
                    w.write_str(" . ")?;
                    self.write_value(idx, w)?;
                    break;
                }
            }
        }
        Ok(())
    }
}

/// A stack-allocated writer for formatting values without heap allocation.
///
/// Implements `core::fmt::Write` by writing UTF-8 bytes into a fixed-size
/// buffer. Used by [`Lisp::display_to_io`] and [`Lisp::write_to_io`] to
/// format values before sending them through an [`IoProvider`].
pub(crate) struct StackWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> StackWriter<'a> {
    pub(crate) fn new(buf: &'a mut [u8]) -> Self {
        StackWriter { buf, pos: 0 }
    }

    pub(crate) fn as_str(&self) -> &str {
        // core::fmt::Write::write_str only accepts valid UTF-8 by contract,
        // so this should always succeed. The fallback is defensive since we
        // forbid(unsafe_code) and cannot use from_utf8_unchecked.
        core::str::from_utf8(&self.buf[..self.pos]).unwrap_or("")
    }
}

impl core::fmt::Write for StackWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.buf.len() - self.pos;
        let to_copy = bytes.len().min(remaining);
        self.buf[self.pos..self.pos + to_copy].copy_from_slice(&bytes[..to_copy]);
        self.pos += to_copy;
        Ok(())
    }
}

impl<const N: usize> Trace<Value, N> for Value {
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
            Value::CharPair { cdr, .. } if !cdr.is_nil() => tracer(cdr),
            Value::Applicative(inner) => tracer(inner),
            Value::Symbol(s) => tracer(s),
            _ => {}
        }
    }
}
