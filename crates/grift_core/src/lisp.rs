//! Lisp execution context with arena wrapper
//!
//! This module contains the `Lisp<N>` struct and all its methods for managing
//! Lisp values in the arena.
//!
//! Note: The `impl_pack_unpack_refs!` macro has been moved to `src/macros.rs`.

use grift_arena::{Arena, ArenaIndex, ArenaError, ArenaResult, GcStats};
use crate::value::{Value, Builtin, StdLib};

// ============================================================================
// Lisp Context - Arena wrapper with helper methods
// ============================================================================

/// A Lisp execution context wrapping an arena
/// 
/// ## Nil Value
/// 
/// The Lisp singleton values (nil, true, false) are pre-allocated in reserved slots
/// at initialization time. This avoids allocation overhead and ensures consistent
/// identity for these commonly-used values.
/// 
/// ## Reserved Slots
/// 
/// The first 4 slots of the arena are reserved for singleton values:
/// - Slot 0: `Value::Nil` - empty list ()
/// - Slot 1: `Value::True` - boolean true (#t)
/// - Slot 2: `Value::False` - boolean false (#f)
/// - Slot 3: `Value::Cons` - intern table reference cell (car = intern table root)
/// 
/// These slots are pre-allocated during `Lisp::new()` and returned as
/// constants from `true_val()` and `false_val()`. This optimization
/// avoids allocating new slots for these frequently-used values.
/// 
/// ## Symbol Interning
/// 
/// All symbols are interned in an association list stored in the arena.
/// The `intern_table_slot` field points to a cons cell whose car is the alist 
/// of `(string_index . symbol_index)` pairs. The cons cell is used as a 
/// "reference cell" to allow updating the intern table without RefCell.
/// When creating a symbol, we first check if it already exists in the table.
/// This ensures that the same symbol name always returns the same index.
pub struct Lisp<const N: usize> {
    arena: Arena<Value, N>,
    /// Pre-allocated Nil slot (always slot 0)
    nil_slot: ArenaIndex,
    /// Pre-allocated Void slot (always slot 1)
    void_slot: ArenaIndex,
    /// Pre-allocated True slot (always slot 2)
    true_slot: ArenaIndex,
    /// Pre-allocated False slot (always slot 3)
    false_slot: ArenaIndex,
    /// Intern table reference cell (slot 4)
    /// This is a cons cell where car = intern table root (alist)
    /// Using a cons cell avoids needing RefCell for interior mutability
    intern_table_slot: ArenaIndex,
}

/// Number of reserved slots in the arena:
/// - nil (1), void (1), true (1), false (1), intern_table_cons (1)
/// Note: With inline cons, we no longer need separate data slots for the intern table
pub const RESERVED_SLOTS: usize = 5;

impl<const N: usize> Lisp<N> {
    /// Create a new Lisp context
    /// 
    /// Pre-allocates reserved slots for nil, void, true, false, and the intern table.
    /// These slots are never freed and provide O(1) access to common values.
    /// 
    /// # Panics
    /// 
    /// Panics if the arena capacity N < RESERVED_SLOTS.
    pub fn new() -> Self {
        const { assert!(N >= RESERVED_SLOTS, "Lisp arena must have capacity >= RESERVED_SLOTS for reserved slots") };
        
        let arena = Arena::new(Value::Nil);
        
        // Pre-allocate singleton values (slots 0, 1, 2, 3)
        let nil_slot = arena.alloc(Value::Nil)
            .expect("Failed to pre-allocate Nil slot during Lisp initialization");
        let void_slot = arena.alloc(Value::Void)
            .expect("Failed to pre-allocate Void slot during Lisp initialization");
        let true_slot = arena.alloc(Value::True)
            .expect("Failed to pre-allocate True slot during Lisp initialization");
        let false_slot = arena.alloc(Value::False)
            .expect("Failed to pre-allocate False slot during Lisp initialization");
        
        // Pre-allocate intern table reference cell (slot 4)
        // With inline cons, we don't need separate data slots
        // This is a cons cell where car = intern table root (initially nil)
        let intern_table_slot = arena.alloc(Value::Cons { car: nil_slot, cdr: nil_slot })
            .expect("Failed to pre-allocate intern table reference cell during Lisp initialization");
        
        Lisp {
            arena,
            nil_slot,
            void_slot,
            true_slot,
            false_slot,
            intern_table_slot,
        }
    }
    
    /// Get reference to the underlying arena
    pub fn arena(&self) -> &Arena<Value, N> {
        &self.arena
    }
    
    /// Allocate a value
    #[inline]
    pub fn alloc(&self, value: Value) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(value)
    }
    
    /// Get a value from the arena by index
    #[inline]
    pub fn get(&self, index: ArenaIndex) -> ArenaResult<Value> {
        self.arena.get(index)
    }
    
    /// Set a value
    #[inline]
    pub fn set(&self, index: ArenaIndex, value: Value) -> ArenaResult<()> {
        self.arena.set(index, value)
    }
    
    /// Get an arena index at a given offset from a base index.
    /// 
    /// This is useful for accessing elements in contiguous storage (strings, arrays).
    #[inline]
    pub fn arena_index_at_offset(&self, base: ArenaIndex, offset: usize) -> ArenaResult<ArenaIndex> {
        self.arena.index_at_offset(base, offset)
    }
    
    /// Get the pre-allocated Nil singleton (empty list)
    /// 
    /// Returns the reserved slot 0 which always contains `Value::Nil`.
    #[inline]
    pub fn nil(&self) -> ArenaResult<ArenaIndex> {
        Ok(self.nil_slot)
    }
    
    /// Get the pre-allocated Void singleton (unspecified value)
    /// 
    /// Returns the reserved slot 1 which always contains `Value::Void`.
    /// Used as return value for side-effect-only forms like `define`, `set!`, `display`.
    #[inline]
    pub fn void_val(&self) -> ArenaResult<ArenaIndex> {
        Ok(self.void_slot)
    }
    
    /// Get the pre-allocated True singleton (#t)
    /// 
    /// Returns the reserved slot 2 which always contains `Value::True`.
    #[inline]
    pub fn true_val(&self) -> ArenaResult<ArenaIndex> {
        Ok(self.true_slot)
    }
    
    /// Get the pre-allocated False singleton (#f)
    /// 
    /// Returns the reserved slot 3 which always contains `Value::False`.
    #[inline]
    pub fn false_val(&self) -> ArenaResult<ArenaIndex> {
        Ok(self.false_slot)
    }
    
    /// Allocate a boolean based on a Rust bool
    #[inline]
    pub fn boolean(&self, b: bool) -> ArenaResult<ArenaIndex> {
        if b { self.true_val() } else { self.false_val() }
    }
    
    /// Allocate a number
    #[inline]
    pub fn number(&self, n: isize) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Number(n))
    }
    
    /// Allocate a character
    #[inline]
    pub fn char(&self, c: char) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Char(c))
    }
    
    /// Allocate a cons cell
    /// 
    /// Creates a cons cell with inline car and cdr indices.
    /// No arena data slots are needed - the indices are stored directly in the Value.
    #[inline]
    pub fn cons(&self, car: ArenaIndex, cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Cons { car, cdr })
    }
    
    /// Get car of a cons cell
    /// 
    /// In Scheme R7RS, car of an empty list is an error.
    /// O(1) access - car is stored inline.
    #[inline]
    pub fn car(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.arena.get(index)? {
            Value::Cons { car, .. } => Ok(car),
            Value::Nil => Err(ArenaError::InvalidIndex),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get cdr of a cons cell
    /// 
    /// In Scheme R7RS, cdr of an empty list is an error.
    /// O(1) access - cdr is stored inline.
    #[inline]
    pub fn cdr(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.arena.get(index)? {
            Value::Cons { cdr, .. } => Ok(cdr),
            Value::Nil => Err(ArenaError::InvalidIndex),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get both car and cdr of a cons cell in one operation
    /// 
    /// More efficient than calling car() and cdr() separately when both are needed.
    /// O(1) access - both are stored inline.
    #[inline]
    pub fn car_cdr(&self, index: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex)> {
        match self.arena.get(index)? {
            Value::Cons { car, cdr } => Ok((car, cdr)),
            Value::Nil => Err(ArenaError::InvalidIndex),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Set car of a cons cell (mutation operation)
    /// Returns the new value on success
    #[inline]
    pub fn set_car(&self, index: ArenaIndex, new_car: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(index)? {
            Value::Cons { cdr, .. } => {
                self.arena.set(index, Value::Cons { car: new_car, cdr })?;
                Ok(new_car)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Set cdr of a cons cell (mutation operation)
    /// Returns the new value on success
    #[inline]
    pub fn set_cdr(&self, index: ArenaIndex, new_cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(index)? {
            Value::Cons { car, .. } => {
                self.arena.set(index, Value::Cons { car, cdr: new_cdr })?;
                Ok(new_cdr)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    // ========================================================================
    // Contiguous Ref Storage (for continuation data)
    // ========================================================================
    // 
    // These methods pack/unpack multiple ArenaIndex values into contiguous
    // arena slots as Ref values. Optimized with batch arena operations.
    // - 3 values via cons: 9 slots (3 cons × 3 slots each)
    // - 3 values via pack_refs3: 3 slots
    
    // Pack/unpack refs operations - generated by impl_pack_unpack_refs! macro
    impl_pack_unpack_refs!(1, pack_refs1, unpack_refs1);
    impl_pack_unpack_refs!(2, pack_refs2, unpack_refs2, set_contiguous2, get_contiguous2, [a, b]);
    impl_pack_unpack_refs!(3, pack_refs3, unpack_refs3, set_contiguous3, get_contiguous3, [a, b, c]);
    impl_pack_unpack_refs!(4, pack_refs4, unpack_refs4, set_contiguous4, get_contiguous4, [a, b, c, d]);
    impl_pack_unpack_refs!(5, pack_refs5, unpack_refs5, set_contiguous5, get_contiguous5, [a, b, c, d, e]);
    impl_pack_unpack_refs!(6, pack_refs6, unpack_refs6, set_contiguous6, get_contiguous6, [a, b, c, d, e, f]);
    impl_pack_unpack_refs!(7, pack_refs7, unpack_refs7, set_contiguous7, get_contiguous7, [a, b, c, d, e, f, g]);
    
    // ========================================================================
    // Symbol Interning
    // ========================================================================
    
    /// Get the intern table root (for GC roots)
    /// 
    /// The intern table is stored in the car of the intern_table_slot cons cell.
    pub fn intern_table(&self) -> ArenaIndex {
        // intern_table_slot always exists and is slot 3
        // Its car contains the actual intern table root
        self.intern_table_slot
    }
    
    /// Get the current intern table root (the actual alist)
    fn get_intern_table_root(&self) -> ArenaResult<ArenaIndex> {
        self.car(self.intern_table_slot)
    }
    
    /// Set the intern table root (update the car of the reference cell)
    fn set_intern_table_root(&self, new_root: ArenaIndex) -> ArenaResult<()> {
        self.set_car(self.intern_table_slot, new_root)?;
        Ok(())
    }
    
    /// Look up a string in the intern table
    /// Returns Some(symbol_index) if found, None otherwise
    fn intern_table_lookup(&self, string_idx: ArenaIndex) -> ArenaResult<Option<ArenaIndex>> {
        let mut current = self.get_intern_table_root()?;
        
        loop {
            match self.get(current)? {
                Value::Nil => return Ok(None),
                Value::Cons { .. } => {
                    // car is (string_index . symbol_index)
                    let car = self.car(current)?;
                    let cdr = self.cdr(current)?;
                    if let Value::Cons { .. } = self.get(car)? {
                        let entry_string = self.car(car)?;
                        let entry_symbol = self.cdr(car)?;
                        if self.string_eq_contiguous(string_idx, entry_string)? {
                            return Ok(Some(entry_symbol));
                        }
                    }
                    current = cdr;
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
        }
    }
    
    /// Look up bytes directly in the intern table without allocating a string first.
    /// 
    /// This is an optimization for `symbol_from_bytes` - on cache hits, we avoid
    /// allocating a string entirely by comparing the bytes directly against
    /// the interned strings.
    /// 
    /// Returns Some(symbol_index) if found, None otherwise
    fn intern_table_lookup_bytes(&self, bytes: &[u8]) -> ArenaResult<Option<ArenaIndex>> {
        let mut current = self.get_intern_table_root()?;
        
        loop {
            match self.get(current)? {
                Value::Nil => return Ok(None),
                Value::Cons { .. } => {
                    // car is (string_index . symbol_index)
                    let car = self.car(current)?;
                    let cdr = self.cdr(current)?;
                    if let Value::Cons { .. } = self.get(car)? {
                        let entry_string = self.car(car)?;
                        let entry_symbol = self.cdr(car)?;
                        // Compare bytes directly without allocating
                        if self.string_matches_bytes(entry_string, bytes)? {
                            return Ok(Some(entry_symbol));
                        }
                    }
                    current = cdr;
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
        }
    }
    
    /// Create or retrieve an interned symbol from a string slice
    /// 
    /// The symbol's `chars` field points to a Value::String with the symbol name.
    /// 
    /// Symbol interning ensures the same symbol name always returns the same index.
    /// 
    /// This method is optimized to avoid allocation on cache hits by comparing
    /// the input string directly against interned symbols before allocating.
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        // Fast path: check intern table by comparing bytes directly
        // This avoids allocation entirely on cache hits
        // Only works for ASCII symbols (which is typical for Lisp)
        if name.is_ascii()
            && let Some(existing_symbol) = self.intern_table_lookup_bytes(name.as_bytes())?
        {
            return Ok(existing_symbol);
        }
        
        // Cache miss - need to create a new symbol
        let name_str = self.string(name)?;
        
        // Double-check for non-ASCII case (we may have skipped the fast path)
        if !name.is_ascii()
            && let Some(existing_symbol) = self.intern_table_lookup(name_str)?
        {
            self.string_free(name_str)?;
            return Ok(existing_symbol);
        }
        
        // Not found - create new symbol
        let symbol = self.alloc(Value::Symbol(name_str))?;
        
        // Add to intern table: (name_str . symbol)
        let binding = self.cons(name_str, symbol)?;
        let current_table = self.get_intern_table_root()?;
        let new_table = self.cons(binding, current_table)?;
        
        // Update intern table root
        self.set_intern_table_root(new_table)?;
        
        Ok(symbol)
    }
    
    /// Create or retrieve an interned symbol from bytes (for parsing)
    /// 
    /// This method is optimized to avoid allocation on cache hits by comparing
    /// the input bytes directly against interned strings before allocating.
    pub fn symbol_from_bytes(&self, bytes: &[u8]) -> ArenaResult<ArenaIndex> {
        // Fast path: check intern table by comparing bytes directly
        // This avoids allocation entirely on cache hits
        if let Some(existing_symbol) = self.intern_table_lookup_bytes(bytes)? {
            return Ok(existing_symbol);
        }
        
        // Cache miss - need to create a new symbol
        let char_count = bytes.len();
        
        // Create a Value::String for the symbol name (with inline length)
        let name_str = if char_count == 0 {
            // Empty string - len=0, data is NIL
            self.alloc(Value::String { len: 0, data: ArenaIndex::NIL })?
        } else {
            // Allocate contiguous block for chars only (no length header)
            let data = self.arena.alloc_contiguous(char_count, Value::Nil)?;
            
            // Set characters in slots (starting at data)
            for (i, &b) in bytes.iter().enumerate() {
                let char_idx = self.arena.index_at_offset(data, i)?;
                self.arena.set(char_idx, Value::Char(b as char))?;
            }
            
            // Create the String value with inline length
            self.alloc(Value::String { len: char_count, data })?
        };
        
        // Create new symbol
        let symbol = self.alloc(Value::Symbol(name_str))?;
        
        // Add to intern table: (name_str . symbol)
        let binding = self.cons(name_str, symbol)?;
        let current_table = self.get_intern_table_root()?;
        let new_table = self.cons(binding, current_table)?;
        
        // Update intern table root
        self.set_intern_table_root(new_table)?;
        
        Ok(symbol)
    }
    
    /// Create or retrieve an interned symbol from an existing string index
    /// 
    /// This method is used by string->symbol to create a symbol from an existing string.
    /// It checks the intern table first to ensure proper symbol interning.
    pub fn symbol_from_string(&self, string_idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        // Verify it's a string
        match self.get(string_idx)? {
            Value::String { len, data } => {
                // Check if a symbol with this string content already exists
                if let Some(existing_symbol) = self.intern_table_lookup(string_idx)? {
                    return Ok(existing_symbol);
                }
                
                // Not found - need to create a new symbol
                // First, copy the string to avoid sharing the mutable string
                let new_str = if len == 0 {
                    self.alloc(Value::String { len: 0, data: ArenaIndex::NIL })?
                } else {
                    // Copy the string content
                    let new_data = self.arena.alloc_contiguous(len, Value::Nil)?;
                    for i in 0..len {
                        let src_idx = self.arena.index_at_offset(data, i)?;
                        let dst_idx = self.arena.index_at_offset(new_data, i)?;
                        let ch = self.get(src_idx)?;
                        self.arena.set(dst_idx, ch)?;
                    }
                    self.alloc(Value::String { len, data: new_data })?
                };
                
                // Create new symbol
                let symbol = self.alloc(Value::Symbol(new_str))?;
                
                // Add to intern table: (new_str . symbol)
                let binding = self.cons(new_str, symbol)?;
                let current_table = self.get_intern_table_root()?;
                let new_table = self.cons(binding, current_table)?;
                
                // Update intern table root
                self.set_intern_table_root(new_table)?;
                
                Ok(symbol)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Create a new unique symbol without checking the intern table.
    /// 
    /// This is an optimization for `gensym` — since generated symbol names are
    /// guaranteed unique (via a monotonic counter), the intern table lookup
    /// will always miss. Skipping the lookup avoids an O(N) scan of the table.
    /// 
    /// The symbol is NOT added to the intern table, which also prevents
    /// bloating the table and slowing down future lookups.
    pub fn symbol_new_unique(&self, name: &str) -> ArenaResult<ArenaIndex> {
        let name_str = self.string(name)?;
        let symbol = self.alloc(Value::Symbol(name_str))?;
        Ok(symbol)
    }
    
    /// Allocate a builtin function
    #[inline]
    pub fn builtin(&self, b: Builtin) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Builtin(b))
    }
    
    /// Allocate a stdlib function
    /// 
    /// StdLib functions are stored in static memory with on-demand parsing.
    /// The function body is parsed on each call.
    #[inline]
    pub fn stdlib(&self, s: StdLib) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::StdLib(s))
    }
    
    /// Allocate a native function reference.
    ///
    /// Native functions are Rust functions registered with the evaluator.
    /// The `id` is the index in the NativeRegistry.
    /// 
    /// The value is stored inline - no arena data slots needed.
    #[inline]
    pub fn native(&self, id: usize) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Native { id })
    }
    
    /// Get the id from a native function
    /// O(1) access - id is stored inline.
    pub fn native_id(&self, native_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.get(native_idx)? {
            Value::Native { id, .. } => Ok(id),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Allocate a lambda
    /// 
    /// Stores lambda with inline params and body_env indices.
    /// body_env is a cons cell containing (body . env).
    pub fn lambda(&self, params: ArenaIndex, body: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        // Create a cons cell for (body . env) 
        let body_env = self.cons(body, env)?;
        self.alloc(Value::Lambda { params, body_env })
    }
    
    /// Extract parts from a lambda: (params, body, env)
    /// 
    /// Lambda has inline params and body_env, where body_env is a cons (body . env).
    #[inline]
    pub fn lambda_parts(&self, index: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex)> {
        let val = self.get(index)?;
        match val {
            Value::Lambda { params, body_env } => {
                // body_env is a cons cell (body . env)
                let (body, env) = self.car_cdr(body_env)?;
                Ok((params, body, env))
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Create a syntax object from an expression
    ///
    /// Syntax objects wrap an expression with lexical context information
    /// for hygienic macro expansion. This is the foundation for `syntax-case`.
    ///
    /// # Arguments
    ///
    /// * `expr` - The expression to wrap (the datum)
    /// * `marks` - List of hygiene marks for tracking macro expansion scopes
    /// * `subst` - Substitution environment for identifier resolution
    ///
    /// # Example
    ///
    /// ```
    /// use grift_core::{Lisp, Value};
    /// 
    /// let lisp: Lisp<1000> = Lisp::new();
    /// 
    /// // Create a syntax object wrapping the symbol 'x' with empty context
    /// let x = lisp.symbol("x").unwrap();
    /// let nil = lisp.nil().unwrap();
    /// let stx = lisp.syntax(x, nil, nil).unwrap();
    /// 
    /// // Verify it's a syntax object
    /// match lisp.get(stx).unwrap() {
    ///     Value::Syntax { .. } => (), // Success
    ///     _ => panic!("Expected Syntax value"),
    /// }
    /// ```
    pub fn syntax(
        &self,
        expr: ArenaIndex,
        marks: ArenaIndex,
        subst: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        // For backward compatibility, use nil as the lexical environment
        let nil = self.nil()?;
        self.syntax_with_env(expr, marks, subst, nil)
    }
    
    /// Create a syntax object with full lexical scope information
    ///
    /// This is the preferred constructor for syntax objects that need to
    /// preserve their creation-site lexical environment for proper scope
    /// resolution.
    ///
    /// # Arguments
    ///
    /// * `expr` - The expression to wrap (the datum)
    /// * `marks` - List of hygiene marks for tracking macro expansion scopes
    /// * `subst` - Substitution environment for identifier resolution
    /// * `lex_env` - The lexical environment at syntax object creation site
    ///
    /// # Memory Layout
    ///
    /// The context is packed as: (marks . (subst . lex_env))
    pub fn syntax_with_env(
        &self,
        expr: ArenaIndex,
        marks: ArenaIndex,
        subst: ArenaIndex,
        lex_env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        // Pack as: (marks . (subst . lex_env))
        let subst_env = self.cons(subst, lex_env)?;
        let context = self.cons(marks, subst_env)?;
        self.arena.alloc(Value::Syntax { expr, context })
    }
    
    /// Extract components from a syntax object
    ///
    /// Returns (expr, marks, subst) unpacked from the internal structure.
    /// Note: This returns only 3 components for backward compatibility.
    /// Use `syntax_parts_with_env` for full 4-component extraction.
    ///
    /// # Returns
    ///
    /// * `expr` - The wrapped datum
    /// * `marks` - List of hygiene marks
    /// * `subst` - Substitution environment
    pub fn syntax_parts(
        &self,
        idx: ArenaIndex,
    ) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex)> {
        match self.get(idx)? {
            Value::Syntax { expr, context } => {
                // Unpack (marks . (subst . lex_env))
                let marks = self.car(context)?;
                let subst_env = self.cdr(context)?;
                let subst = self.car(subst_env)?;
                Ok((expr, marks, subst))
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Extract all components from a syntax object including lexical environment
    ///
    /// Returns (expr, marks, subst, lex_env) for full lexical scope support.
    ///
    /// # Returns
    ///
    /// * `expr` - The wrapped datum
    /// * `marks` - List of hygiene marks
    /// * `subst` - Substitution environment
    /// * `lex_env` - The captured lexical environment
    pub fn syntax_parts_with_env(
        &self,
        idx: ArenaIndex,
    ) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex)> {
        match self.get(idx)? {
            Value::Syntax { expr, context } => {
                // Unpack (marks . (subst . lex_env))
                let marks = self.car(context)?;
                let subst_env = self.cdr(context)?;
                let subst = self.car(subst_env)?;
                let lex_env = self.cdr(subst_env)?;
                Ok((expr, marks, subst, lex_env))
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Unwrap a syntax object to get the raw datum
    ///
    /// For syntax objects, returns the wrapped expression.
    /// For non-syntax values, returns the value unchanged (pass-through).
    ///
    /// This is useful for extracting the underlying S-expression from a
    /// syntax object during macro expansion.
    pub fn syntax_to_datum(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(idx)? {
            Value::Syntax { expr, .. } => Ok(expr),
            _ => Ok(idx), // Non-syntax passes through unchanged
        }
    }
    
    // ========================================================================
    // Continuation Frame Methods (for call/cc support)
    // ========================================================================
    
    /// Create a continuation frame for arena-based continuation stack
    ///
    /// Continuation frames form a linked list in the arena, enabling O(1) capture
    /// for call/cc. Each frame stores the continuation type, associated data,
    /// and a reference to the parent continuation.
    ///
    /// # Arguments
    ///
    /// * `cont_type` - The continuation type encoded as usize (e.g., 0=Done, 1=ApplyForced)
    /// * `data` - ArenaIndex to continuation-specific data (or Nil if none)
    /// * `parent` - ArenaIndex to parent ContFrame (or Nil for Done continuation)
    /// * `env` - ArenaIndex to the environment at this continuation point
    ///
    /// # Internal Structure
    ///
    /// The cont_data field stores: ((Usize(cont_type) . data) . parent)
    /// This allows extracting all information with minimal arena lookups.
    ///
    /// # Example
    ///
    /// ```
    /// use grift_core::Lisp;
    /// 
    /// let lisp: Lisp<1000> = Lisp::new();
    /// 
    /// // Create a Done continuation (type 0, no data, no parent)
    /// let nil = lisp.nil().unwrap();
    /// let done_cont = lisp.cont_frame(0, nil, nil, nil).unwrap();
    /// 
    /// // Create an IfBranch continuation (type 2) with data
    /// let then_expr = lisp.symbol("then").unwrap();
    /// let else_expr = lisp.symbol("else").unwrap();
    /// let data = lisp.cons(then_expr, else_expr).unwrap();
    /// let env = lisp.nil().unwrap();
    /// let if_cont = lisp.cont_frame(2, data, done_cont, env).unwrap();
    /// 
    /// // Verify the continuation parent
    /// let parent = lisp.cont_frame_parent(if_cont).unwrap();
    /// assert_eq!(parent, done_cont);
    /// ```
    pub fn cont_frame(
        &self,
        cont_type: usize,
        data: ArenaIndex,
        parent: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        // Create type_val as Usize
        let type_val = self.arena.alloc(Value::Usize(cont_type))?;
        // Pack type and data: (type . data)
        let type_and_data = self.cons(type_val, data)?;
        // Pack with parent: ((type . data) . parent)
        let cont_data = self.cons(type_and_data, parent)?;
        // Create the ContFrame
        self.arena.alloc(Value::ContFrame { cont_data, env })
    }
    
    /// Extract components from a ContFrame value
    ///
    /// Returns (cont_type, data, parent, env) unpacked from the internal structure.
    ///
    /// # Returns
    ///
    /// * `cont_type` - The continuation type as usize
    /// * `data` - Continuation-specific data (or Nil)
    /// * `parent` - Parent continuation (or Nil for Done)
    /// * `env` - Environment at this continuation point
    ///
    /// # Errors
    ///
    /// Returns ArenaError::InvalidIndex if the index doesn't point to a ContFrame.
    pub fn cont_frame_parts(
        &self,
        idx: ArenaIndex,
    ) -> ArenaResult<(usize, ArenaIndex, ArenaIndex, ArenaIndex)> {
        match self.get(idx)? {
            Value::ContFrame { cont_data, env } => {
                // Unpack ((type . data) . parent)
                let type_and_data = self.car(cont_data)?;
                let parent = self.cdr(cont_data)?;
                // Unpack (type . data)
                let type_val = self.car(type_and_data)?;
                let data = self.cdr(type_and_data)?;
                // Get the usize value
                match self.get(type_val)? {
                    Value::Usize(cont_type) => Ok((cont_type, data, parent, env)),
                    _ => Err(ArenaError::InvalidIndex),
                }
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get the parent continuation from a ContFrame
    ///
    /// This is a convenience method for walking up the continuation chain.
    ///
    /// # Returns
    ///
    /// The parent continuation ArenaIndex (or Nil if this is the Done continuation).
    ///
    /// # Special Cases
    ///
    /// - If `idx` points to a ContFrame, returns the parent continuation
    /// - If `idx` is Nil, returns Nil (end of chain sentinel - allows safe iteration)
    /// - Otherwise returns ArenaError::InvalidIndex
    ///
    /// The Nil case enables writing simple iteration loops that naturally terminate:
    /// ```
    /// use grift_core::{Lisp, Value};
    /// 
    /// let lisp: Lisp<1000> = Lisp::new();
    /// 
    /// // Create a chain: cont3 -> cont2 -> cont1 -> nil
    /// let nil = lisp.nil().unwrap();
    /// let cont1 = lisp.cont_frame(1, nil, nil, nil).unwrap();
    /// let cont2 = lisp.cont_frame(2, nil, cont1, nil).unwrap();
    /// let cont3 = lisp.cont_frame(3, nil, cont2, nil).unwrap();
    /// 
    /// // Iterate through the chain
    /// let mut current = cont3;
    /// let mut count = 0;
    /// while !matches!(lisp.get(current).unwrap(), Value::Nil) {
    ///     current = lisp.cont_frame_parent(current).unwrap();
    ///     count += 1;
    /// }
    /// assert_eq!(count, 3); // Visited cont3, cont2, cont1
    /// ```
    pub fn cont_frame_parent(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(idx)? {
            Value::ContFrame { cont_data, .. } => {
                self.cdr(cont_data)
            }
            Value::Nil => Ok(idx), // Nil represents end of chain - return self for safe iteration
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Create a captured continuation value for call/cc
    ///
    /// A continuation represents "the rest of the computation" and can be
    /// called as a procedure to jump back to the point where it was captured.
    ///
    /// # Arguments
    ///
    /// * `cont_chain` - ArenaIndex to ContFrame linked list (captured continuation stack)
    /// * `capture_env` - ArenaIndex to the environment at capture point
    /// * `dynamic_wind_chain` - ArenaIndex to dynamic-wind chain (or Nil if none)
    ///
    /// # Example
    ///
    /// ```
    /// use grift_core::{Lisp, Value};
    /// 
    /// let lisp: Lisp<1000> = Lisp::new();
    /// 
    /// // Create a simple continuation
    /// let nil = lisp.nil().unwrap();
    /// let cont_chain = lisp.cont_frame(0, nil, nil, nil).unwrap();
    /// let env = lisp.nil().unwrap();
    /// let cont = lisp.continuation(cont_chain, env, nil).unwrap();
    /// 
    /// // Verify it's a continuation value
    /// match lisp.get(cont).unwrap() {
    ///     Value::Continuation { .. } => (), // Success
    ///     _ => panic!("Expected Continuation value"),
    /// }
    /// ```
    pub fn continuation(
        &self,
        cont_chain: ArenaIndex,
        capture_env: ArenaIndex,
        dynamic_wind_chain: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        // Create metadata cons: (capture_env . dynamic_wind_chain)
        let metadata = self.cons(capture_env, dynamic_wind_chain)?;
        // Create the Continuation value
        self.arena.alloc(Value::Continuation { cont_chain, metadata })
    }
    
    /// Extract components from a Continuation value
    ///
    /// Returns (cont_chain, capture_env, dynamic_wind_chain) unpacked from the internal structure.
    ///
    /// # Returns
    ///
    /// * `cont_chain` - Captured continuation stack (ContFrame linked list)
    /// * `capture_env` - Environment at capture point
    /// * `dynamic_wind_chain` - Dynamic-wind chain for proper before/after thunk handling
    ///
    /// # Errors
    ///
    /// Returns ArenaError::InvalidIndex if the index doesn't point to a Continuation.
    pub fn continuation_parts(
        &self,
        idx: ArenaIndex,
    ) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex)> {
        match self.get(idx)? {
            Value::Continuation { cont_chain, metadata } => {
                // Unpack (capture_env . dynamic_wind_chain)
                let capture_env = self.car(metadata)?;
                let dynamic_wind_chain = self.cdr(metadata)?;
                Ok((cont_chain, capture_env, dynamic_wind_chain))
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Build a list from an iterator of indices
    pub fn list<I: IntoIterator<Item = ArenaIndex>>(&self, items: I) -> ArenaResult<ArenaIndex>
    where
        I::IntoIter: DoubleEndedIterator,
    {
        let mut result = self.nil()?;
        for item in items.into_iter().rev() {
            result = self.cons(item, result)?;
        }
        Ok(result)
    }
    
    /// Get the length of a list
    pub fn list_len(&self, mut list: ArenaIndex) -> ArenaResult<usize> {
        let mut len = 0;
        loop {
            match self.get(list)? {
                Value::Nil => return Ok(len),
                Value::Cons { .. } => {
                    len += 1;
                    list = self.cdr(list)?;
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
        }
    }
    
    /// Check if two symbols are equal.
    /// 
    /// Symbols are compared by their underlying string content.
    #[inline]
    pub fn symbol_eq(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        // Fast path: same index means same symbol (common for interned symbols)
        if a == b {
            return Ok(true);
        }
        
        let val_a = self.get(a)?;
        let val_b = self.get(b)?;
        
        match (val_a, val_b) {
            (Value::Symbol(chars_a), Value::Symbol(chars_b)) => {
                // Second fast path: same underlying string index
                if chars_a == chars_b {
                    return Ok(true);
                }
                self.string_eq_contiguous(chars_a, chars_b)
            }
            _ => Ok(false),
        }
    }
    
    /// Check if two values are eqv? (Scheme eqv? predicate)
    /// 
    /// Returns true if values are identical or have the same primitive value.
    #[inline]
    pub fn eqv(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        // Fast path: same index
        if a == b {
            return Ok(true);
        }
        
        let val_a = self.get(a)?;
        let val_b = self.get(b)?;
        
        match (val_a, val_b) {
            (Value::Nil, Value::Nil) => Ok(true),
            (Value::True, Value::True) => Ok(true),
            (Value::False, Value::False) => Ok(true),
            (Value::Number(n1), Value::Number(n2)) => Ok(n1 == n2),
            (Value::Char(c1), Value::Char(c2)) => Ok(c1 == c2),
            (Value::Symbol(_), Value::Symbol(_)) => self.symbol_eq(a, b),
            _ => Ok(false),
        }
    }
    
    /// Check if a symbol matches a string.
    #[inline]
    pub fn symbol_matches(&self, sym: ArenaIndex, name: &str) -> ArenaResult<bool> {
        let val = self.get(sym)?;
        
        match val {
            Value::Symbol(chars) => {
                // Fast path for ASCII names (all Scheme keywords are ASCII):
                // Uses O(1) length check via bytes.len() and avoids per-char
                // string_char_at overhead.
                if name.is_ascii() {
                    self.string_matches_bytes(chars, name.as_bytes())
                } else {
                    self.string_matches(chars, name)
                }
            }
            _ => Ok(false),
        }
    }
    
    /// Extract symbol name to a fixed buffer.
    pub fn symbol_to_bytes(&self, sym: ArenaIndex, buf: &mut [u8]) -> ArenaResult<usize> {
        let val = self.get(sym)?;
        
        match val {
            Value::Symbol(chars) => self.string_to_bytes(chars, buf),
            _ => Ok(0),
        }
    }
    
    /// Get the length of a symbol's name.
    pub fn symbol_len(&self, sym: ArenaIndex) -> ArenaResult<usize> {
        match self.get(sym)? {
            Value::Symbol(chars) => self.string_len(chars),
            _ => Ok(0),
        }
    }
    
    /// Get a character at a specific index within a symbol's name
    /// Returns None if the index is out of bounds or if the value is not a symbol
    pub fn symbol_char_at(&self, sym: ArenaIndex, index: usize) -> ArenaResult<Option<char>> {
        match self.get(sym)? {
            Value::Symbol(chars) => {
                let len = self.string_len(chars)?;
                if index >= len {
                    Ok(None)
                } else {
                    Ok(Some(self.string_char_at(chars, index)?))
                }
            }
            _ => Ok(None),
        }
    }
    
    /// Get the first character of a symbol's name efficiently.
    /// 
    /// Returns `Some(char)` if the symbol has at least one character,
    /// or `None` if the value is not a symbol, the symbol name is empty,
    /// or the underlying string data is invalid.
    /// 
    /// This is optimized over `symbol_char_at` by directly accessing the
    /// first character slot (3 arena gets vs 4), without a redundant
    /// length bounds check. Used for fast dispatch in keyword matching.
    #[inline]
    pub fn symbol_first_char(&self, sym: ArenaIndex) -> ArenaResult<Option<char>> {
        match self.get(sym)? {
            Value::Symbol(chars) => {
                match self.arena.get(chars)? {
                    Value::String { len, data } => {
                        // data.is_nil() is a safety check for inconsistent state
                        if len == 0 || data.is_nil() {
                            Ok(None)
                        } else {
                            match self.arena.get(ArenaIndex::new(data.raw()))? {
                                Value::Char(c) => Ok(Some(c)),
                                _ => Ok(None),
                            }
                        }
                    }
                    _ => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }
    
    /// Run garbage collection with intern table as an additional root
    /// 
    /// The intern table reference cell (slot 3) is always included as a GC root
    /// to prevent interned symbols from being collected. The intern table is
    /// stored as a cons cell whose car points to the alist of interned symbols.
    /// 
    /// # Panics
    /// 
    /// Panics if the number of roots exceeds the internal limit (512 roots).
    /// This limit is chosen to balance stack usage in no_std environments
    /// with typical program needs. Most Lisp programs use far fewer roots.
    pub fn gc(&self, roots: &[ArenaIndex]) -> GcStats {
        // Create a new roots array with reserved slots and intern table included
        // Using const-sized array to avoid alloc in no_std
        // 512 roots should be sufficient for most programs while keeping
        // stack usage reasonable (~8KB on 64-bit systems)
        const MAX_ROOTS: usize = 512;
        
        // Panic if too many roots - this indicates a programming error
        // Account for 5 reserved roots (nil, void, true, false, intern_table)
        assert!(roots.len() < MAX_ROOTS - 5, 
            "Too many GC roots: {} (max {})", roots.len(), MAX_ROOTS - 5 - 1);
        
        let mut all_roots = [ArenaIndex::NIL; MAX_ROOTS];
        let mut root_count = 0;
        
        // Add reserved slots as roots to prevent them from being collected
        all_roots[root_count] = self.nil_slot;
        root_count += 1;
        all_roots[root_count] = self.void_slot;
        root_count += 1;
        all_roots[root_count] = self.true_slot;
        root_count += 1;
        all_roots[root_count] = self.false_slot;
        root_count += 1;
        
        // Add intern table reference cell as root
        // This is a cons cell whose car is the intern table alist
        // Tracing from this cell will reach all interned symbols
        all_roots[root_count] = self.intern_table_slot;
        root_count += 1;
        
        // Copy provided roots
        for &root in roots {
            all_roots[root_count] = root;
            root_count += 1;
        }
        
        self.arena.collect_garbage(&all_roots[..root_count])
    }
    
    
    /// Get arena stats
    pub fn stats(&self) -> grift_arena::ArenaStats {
        self.arena.stats()
    }
    
    // ========================================================================
    // Contiguous String Storage
    // ========================================================================
    // 
    // Strings are stored as Value::String { len, data } where:
    // - len: Number of characters (inline in the Value)
    // - data: Points directly to first Char value (no length header)
    // 
    // This provides:
    // - O(1) length lookup (inline, no arena access needed!)
    // - Cache-friendly sequential access
    // - Reduced arena usage (no length header slot needed)
    // ========================================================================
    
    /// Allocate a string as contiguous Char values.
    /// 
    /// Returns a Value::String with inline length and data pointing directly
    /// to the first character (no length header in arena).
    /// Empty strings have len=0 and data == NIL.
    /// 
    /// # Memory Usage
    /// 
    /// Allocates s.chars().count() slots (characters only, no header),
    /// plus 1 slot for the String value itself.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::OutOfMemory` if:
    /// - No contiguous block is available
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use grift_core::Lisp;
    /// let lisp = Lisp::<1000>::new();
    /// let hello = lisp.string("hello").unwrap();
    /// 
    /// assert_eq!(lisp.string_len(hello).unwrap(), 5);
    /// assert_eq!(lisp.string_char_at(hello, 0).unwrap(), 'h');
    /// ```
    pub fn string(&self, s: &str) -> ArenaResult<ArenaIndex> {
        // For ASCII strings, s.len() == char count in O(1).
        let char_count = if s.is_ascii() { s.len() } else { s.chars().count() };
        
        if char_count == 0 {
            // Empty string - len=0, data is NIL
            return self.alloc(Value::String { len: 0, data: ArenaIndex::NIL });
        }
        
        // Allocate contiguous block for chars only (no length header)
        let data = self.arena.alloc_contiguous(char_count, Value::Nil)?;
        
        // Set characters in slots (starting at data)
        for (i, c) in s.chars().enumerate() {
            let char_idx = self.arena.index_at_offset(data, i)?;
            self.arena.set(char_idx, Value::Char(c))?;
        }
        
        // Create the String value with inline length
        self.alloc(Value::String { len: char_count, data })
    }
    
    /// Allocate a string from a slice of chars.
    /// 
    /// Returns an ArenaIndex pointing to a Value::String.
    pub fn string_from_chars(&self, chars: &[char]) -> ArenaResult<ArenaIndex> {
        let char_count = chars.len();
        
        if char_count == 0 {
            // Empty string - len=0, data is NIL
            return self.alloc(Value::String { len: 0, data: ArenaIndex::NIL });
        }
        
        // Allocate contiguous block for chars only (no length header)
        let data = self.arena.alloc_contiguous(char_count, Value::Nil)?;
        
        // Set characters in slots (starting at data)
        for (i, &c) in chars.iter().enumerate() {
            let char_idx = self.arena.index_at_offset(data, i)?;
            self.arena.set(char_idx, Value::Char(c))?;
        }
        
        // Create the String value with inline length
        self.alloc(Value::String { len: char_count, data })
    }
    
    /// Get the length of a string.
    /// 
    /// Returns O(1) since length is stored inline in the Value.
    /// No arena access needed!
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if the index doesn't point to
    /// a valid string.
    pub fn string_len(&self, str_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.arena.get(str_idx)? {
            Value::String { len, .. } => Ok(len),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get a character at the given index within a string.
    /// 
    /// Character indices are 0-based. Returns O(1) access.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if:
    /// - The string index is invalid
    /// - The character index is out of bounds
    /// - The slot doesn't contain a Char value
    pub fn string_char_at(&self, str_idx: ArenaIndex, char_index: usize) -> ArenaResult<char> {
        match self.arena.get(str_idx)? {
            Value::String { len, data } => {
                if char_index >= len {
                    return Err(ArenaError::InvalidIndex);
                }
                if data.is_nil() {
                    return Err(ArenaError::InvalidIndex);
                }
                // Characters start at data (no header)
                let char_slot = self.arena.index_at_offset(data, char_index)?;
                match self.arena.get(char_slot)? {
                    Value::Char(c) => Ok(c),
                    _ => Err(ArenaError::InvalidIndex),
                }
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Compare two strings for equality.
    /// 
    /// # Errors
    /// 
    /// Returns an error if either string index is invalid.
    pub fn string_eq_contiguous(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        // Fast path: same index
        if a == b {
            return Ok(true);
        }
        
        // Get both string headers once to avoid re-fetching per character
        let (len_a, data_a) = match self.arena.get(a)? {
            Value::String { len, data } => (len, data),
            _ => return Err(ArenaError::InvalidIndex),
        };
        let (len_b, data_b) = match self.arena.get(b)? {
            Value::String { len, data } => (len, data),
            _ => return Err(ArenaError::InvalidIndex),
        };
        
        if len_a != len_b {
            return Ok(false);
        }
        
        if len_a == 0 {
            return Ok(true);
        }
        
        // Compare characters directly using raw index arithmetic
        let base_a = data_a.raw();
        let base_b = data_b.raw();
        
        for i in 0..len_a {
            let slot_a = self.arena.get(ArenaIndex::new(base_a + i))?;
            let slot_b = self.arena.get(ArenaIndex::new(base_b + i))?;
            match (slot_a, slot_b) {
                (Value::Char(ca), Value::Char(cb)) => {
                    if ca != cb {
                        return Ok(false);
                    }
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
        }
        
        Ok(true)
    }
    
    /// Check if a string matches a Rust string slice.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the string index is invalid.
    pub fn string_matches(&self, str_idx: ArenaIndex, s: &str) -> ArenaResult<bool> {
        let len = self.string_len(str_idx)?;
        // For ASCII strings, s.len() == char count in O(1).
        // Only fall back to s.chars().count() for non-ASCII.
        let s_len = if s.is_ascii() { s.len() } else { s.chars().count() };
        
        if len != s_len {
            return Ok(false);
        }
        
        for (i, expected) in s.chars().enumerate() {
            let actual = self.string_char_at(str_idx, i)?;
            if actual != expected {
                return Ok(false);
            }
        }
        
        Ok(true)
    }
    
    /// Check if a string matches a byte slice directly.
    /// 
    /// This is an optimization for symbol interning - allows comparing
    /// an interned string against input bytes without allocating a new string.
    /// Assumes the bytes are ASCII (valid for Lisp symbols).
    /// 
    /// # Errors
    /// 
    /// Returns an error if the string index is invalid.
    #[inline]
    pub fn string_matches_bytes(&self, str_idx: ArenaIndex, bytes: &[u8]) -> ArenaResult<bool> {
        match self.arena.get(str_idx)? {
            Value::String { len, data } => {
                // Quick length check (O(1) with inline len)
                if len != bytes.len() {
                    return Ok(false);
                }
                
                // Handle empty string case
                if len == 0 {
                    return Ok(bytes.is_empty());
                }
                
                // Compare each character (characters start at data, no header)
                let base_idx = data.raw();
                for (i, &byte) in bytes.iter().enumerate() {
                    let char_slot = ArenaIndex::new(base_idx + i);
                    match self.arena.get(char_slot)? {
                        Value::Char(c) => {
                            if c as u8 != byte {
                                return Ok(false);
                            }
                        }
                        _ => return Err(ArenaError::InvalidIndex),
                    }
                }
                
                Ok(true)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Copy a string's contents to a byte buffer.
    /// 
    /// Returns the number of bytes written. Only ASCII characters (0-127)
    /// are copied; non-ASCII characters are skipped.
    /// 
    /// # Warning
    /// 
    /// This method is designed for ASCII strings. For strings containing
    /// non-ASCII Unicode characters, some characters will be skipped and
    /// the byte count may not match the character count.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the string index is invalid.
    pub fn string_to_bytes(&self, str_idx: ArenaIndex, buf: &mut [u8]) -> ArenaResult<usize> {
        let len = self.string_len(str_idx)?;
        let mut buf_idx = 0;
        
        for i in 0..len {
            if buf_idx >= buf.len() {
                break;
            }
            let c = self.string_char_at(str_idx, i)?;
            // Only copy ASCII characters (0-127)
            if c.is_ascii() {
                buf[buf_idx] = c as u8;
                buf_idx += 1;
            }
            // Non-ASCII characters are skipped
        }
        
        Ok(buf_idx)
    }
    
    /// Free a string and all its character slots.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the string index is invalid.
    pub fn string_free(&self, str_idx: ArenaIndex) -> ArenaResult<()> {
        match self.arena.get(str_idx)? {
            Value::String { len, data } => {
                // Free the data slots (characters only, no header)
                if len > 0 && !data.is_nil() {
                    self.arena.free_contiguous(data, len)?;
                }
                // Free the String value itself
                self.arena.free(str_idx)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    // ========================================================================
    // Contiguous Array Storage
    // ========================================================================
    // 
    // Arrays are stored as Value::Array { len, data } where:
    // - len: Number of elements (inline in the Value)
    // - data: Points directly to first element (no length header)
    // 
    // This provides:
    // - O(1) indexed access and mutation
    // - O(1) length lookup (inline, no arena access needed!)
    // - Cache-friendly sequential access
    // - Reduced arena usage (no length header slot needed)
    // ========================================================================
    
    /// Create an array with the given length, initialized with a default value.
    /// 
    /// The array stores `len` values contiguously in the arena with inline length.
    /// 
    /// # Memory Usage
    /// 
    /// Allocates len slots (elements only, no header), plus 1 slot for the Array value itself.
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use grift_core::Lisp;
    /// let lisp = Lisp::<1000>::new();
    /// let arr = lisp.make_array(3, lisp.nil().unwrap()).unwrap();
    /// 
    /// assert_eq!(lisp.array_len(arr).unwrap(), 3);
    /// ```
    pub fn make_array(&self, len: usize, default: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if len == 0 {
            // Empty array - len=0, data is NIL
            return self.alloc(Value::Array { len: 0, data: ArenaIndex::NIL });
        }
        
        // Allocate contiguous block for elements only (no length header)
        let default_val = self.get(default)?;
        let data = self.arena.alloc_contiguous(len, default_val)?;
        
        // Elements are already initialized with default_val at data through data+len-1
        // (alloc_contiguous initializes all slots with the provided value)
        
        // Create the Array value with inline length
        self.alloc(Value::Array { len, data })
    }
    
    /// Get the length of an array.
    /// 
    /// Returns O(1) since length is stored inline in the Value.
    /// No arena access needed!
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if the index doesn't point to an array.
    pub fn array_len(&self, arr_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.arena.get(arr_idx)? {
            Value::Array { len, .. } => Ok(len),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get the element at the given index within an array.
    /// 
    /// Returns O(1) access via direct index calculation.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if:
    /// - The array index is invalid
    /// - The element index is out of bounds
    pub fn array_get(&self, arr_idx: ArenaIndex, index: usize) -> ArenaResult<ArenaIndex> {
        match self.arena.get(arr_idx)? {
            Value::Array { len, data } => {
                if index >= len {
                    return Err(ArenaError::InvalidIndex);
                }
                if data.is_nil() {
                    return Err(ArenaError::InvalidIndex);
                }
                // Elements start at data (no header)
                self.arena.index_at_offset(data, index)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Set the element at the given index within an array.
    /// 
    /// Returns O(1) mutation via direct index calculation.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if:
    /// - The array index is invalid
    /// - The element index is out of bounds
    pub fn array_set(&self, arr_idx: ArenaIndex, index: usize, value: ArenaIndex) -> ArenaResult<()> {
        match self.arena.get(arr_idx)? {
            Value::Array { len, data } => {
                if index >= len {
                    return Err(ArenaError::InvalidIndex);
                }
                if data.is_nil() {
                    return Err(ArenaError::InvalidIndex);
                }
                // Elements start at data (no header)
                let elem_slot = self.arena.index_at_offset(data, index)?;
                let val = self.get(value)?;
                self.arena.set(elem_slot, val)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Free an array and all its element slots.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the array index is invalid.
    pub fn array_free(&self, arr_idx: ArenaIndex) -> ArenaResult<()> {
        match self.arena.get(arr_idx)? {
            Value::Array { len, data } => {
                // Free the data slots (elements only, no header)
                if len > 0 && !data.is_nil() {
                    self.arena.free_contiguous(data, len)?;
                }
                // Free the Array value itself
                self.arena.free(arr_idx)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Create a [`DisplayValue`](crate::DisplayValue) wrapper for formatting.
    ///
    /// The returned wrapper implements `core::fmt::Display`, enabling
    /// standard formatting via `write!` and `format!` without needing
    /// a separate `format_value` function.
    ///
    /// # Example
    ///
    /// ```
    /// use grift_core::Lisp;
    ///
    /// let lisp = Lisp::<1000>::new();
    /// let val = lisp.number(42).unwrap();
    /// let dv = lisp.display(val);
    /// // write!(f, "{}", dv) or format!("{}", dv)
    /// ```
    #[inline]
    pub fn display(&self, value: ArenaIndex) -> crate::display::DisplayValue<'_, N> {
        crate::display::DisplayValue::new(value, self)
    }
}

impl<const N: usize> Default for Lisp<N> {
    fn default() -> Self {
        Self::new()
    }
}
