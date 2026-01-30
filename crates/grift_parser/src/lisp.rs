//! Lisp execution context with arena wrapper
//!
//! This module contains the `Lisp<N>` struct and all its methods for managing
//! Lisp values in the arena.

use pwn_arena::{Arena, ArenaIndex, ArenaError, ArenaResult, GcStats};
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
/// The first 3 slots of the arena are reserved for singleton values:
/// - Slot 0: `Value::True` - boolean true (#t)
/// - Slot 1: `Value::False` - boolean false (#f)
/// - Slot 2: `Value::Cons` - intern table reference cell (car = intern table root)
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
    /// Pre-allocated True slot (always slot 1)
    true_slot: ArenaIndex,
    /// Pre-allocated False slot (always slot 2)
    false_slot: ArenaIndex,
    /// Intern table reference cell (slots 3-5)
    /// This is a cons cell where car = intern table root (alist)
    /// Using a cons cell avoids needing RefCell for interior mutability
    intern_table_slot: ArenaIndex,
}

/// Number of reserved slots in the arena:
/// - nil (1), true (1), false (1), intern_table_data (2), intern_table_ref (1)
pub const RESERVED_SLOTS: usize = 6;

impl<const N: usize> Lisp<N> {
    /// Create a new Lisp context
    /// 
    /// Pre-allocates reserved slots for nil, true, false, and the intern table.
    /// These slots are never freed and provide O(1) access to common values.
    /// 
    /// # Panics
    /// 
    /// Panics if the arena capacity N < RESERVED_SLOTS.
    pub fn new() -> Self {
        const { assert!(N >= RESERVED_SLOTS, "Lisp arena must have capacity >= RESERVED_SLOTS for reserved slots") };
        
        let arena = Arena::new(Value::Nil);
        
        // Pre-allocate singleton values (slots 0, 1, 2)
        let nil_slot = arena.alloc(Value::Nil)
            .expect("Failed to pre-allocate Nil slot during Lisp initialization");
        let true_slot = arena.alloc(Value::True)
            .expect("Failed to pre-allocate True slot during Lisp initialization");
        let false_slot = arena.alloc(Value::False)
            .expect("Failed to pre-allocate False slot during Lisp initialization");
        
        // Pre-allocate intern table reference cell (slots 3-4 for cons data, slot 5 for cons)
        // Cons cells use contiguous storage: [Ref(car), Ref(cdr)]
        // This is a cons cell where car = intern table root (initially nil)
        let cons_data = arena.alloc_contiguous(2, Value::Nil)
            .expect("Failed to pre-allocate intern table cons data during Lisp initialization");
        arena.set(cons_data, Value::Ref(nil_slot))
            .expect("Failed to set intern table car during Lisp initialization");
        arena.set(ArenaIndex::new(cons_data.raw() + 1), Value::Ref(nil_slot))
            .expect("Failed to set intern table cdr during Lisp initialization");
        let intern_table_slot = arena.alloc(Value::Cons(cons_data))
            .expect("Failed to pre-allocate intern table reference cell during Lisp initialization");
        
        Lisp {
            arena,
            nil_slot,
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
    
    /// Get the pre-allocated True singleton (#t)
    /// 
    /// Returns the reserved slot 1 which always contains `Value::True`.
    #[inline]
    pub fn true_val(&self) -> ArenaResult<ArenaIndex> {
        Ok(self.true_slot)
    }
    
    /// Get the pre-allocated False singleton (#f)
    /// 
    /// Returns the reserved slot 2 which always contains `Value::False`.
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
    
    /// Allocate an unsigned integer (internal use)
    #[inline]
    pub fn usize_val(&self, n: usize) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Usize(n))
    }
    
    /// Allocate a character
    #[inline]
    pub fn char(&self, c: char) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Char(c))
    }
    
    /// Allocate a cons cell
    /// 
    /// Creates a contiguous block of 2 slots: [Ref(car), Ref(cdr)]
    #[inline]
    pub fn cons(&self, car: ArenaIndex, cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
        // Allocate 2 slots for [Ref(car), Ref(cdr)]
        let data = self.arena.alloc_contiguous(2, Value::Nil)?;
        // Single borrow to set both
        self.arena.set_contiguous2(data, Value::Ref(car), Value::Ref(cdr))?;
        self.alloc(Value::Cons(data))
    }
    
    /// Get car of a cons cell
    /// 
    /// In Scheme R7RS, car of an empty list is an error.
    #[inline]
    pub fn car(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
        // Fast path: direct arena access without NIL check (cons is never NIL)
        match self.arena.get(index)? {
            Value::Cons(data) => {
                // Direct read - we know layout is [Ref(car), Ref(cdr)]
                self.arena.get(data)?.as_ref().ok_or(ArenaError::InvalidIndex)
            }
            Value::Nil => Err(ArenaError::InvalidIndex),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get cdr of a cons cell
    /// 
    /// In Scheme R7RS, cdr of an empty list is an error.
    #[inline]
    pub fn cdr(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
        // Fast path: direct arena access without NIL check (cons is never NIL)
        match self.arena.get(index)? {
            Value::Cons(data) => {
                // Direct read - we know layout is [Ref(car), Ref(cdr)]
                self.arena.get(ArenaIndex::new(data.raw() + 1))?.as_ref().ok_or(ArenaError::InvalidIndex)
            }
            Value::Nil => Err(ArenaError::InvalidIndex),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get both car and cdr of a cons cell in one operation
    /// 
    /// More efficient than calling car() and cdr() separately when both are needed.
    #[inline]
    pub fn car_cdr(&self, index: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex)> {
        match self.arena.get(index)? {
            Value::Cons(data) => {
                // Single borrow to read both - data layout is [Ref(car), Ref(cdr)]
                let (vcar, vcdr) = self.arena.get_contiguous2(data)?;
                match (vcar.as_ref(), vcdr.as_ref()) {
                    (Some(car), Some(cdr)) => Ok((car, cdr)),
                    _ => Err(ArenaError::InvalidIndex),
                }
            }
            Value::Nil => Err(ArenaError::InvalidIndex),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get car from a known Cons data pointer (internal use)
    /// 
    /// Caller must ensure `data` is a valid cons data pointer.
    #[inline]
    pub fn car_from_data(&self, data: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.get(data)?.as_ref().ok_or(ArenaError::InvalidIndex)
    }
    
    /// Get cdr from a known Cons data pointer (internal use)
    /// 
    /// Caller must ensure `data` is a valid cons data pointer.
    #[inline]
    pub fn cdr_from_data(&self, data: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.get(ArenaIndex::new(data.raw() + 1))?.as_ref().ok_or(ArenaError::InvalidIndex)
    }
    
    /// Get both car and cdr from a known Cons data pointer (internal use)
    /// 
    /// Caller must ensure `data` is a valid cons data pointer.
    /// Uses single RefCell borrow for efficiency.
    #[inline]
    pub fn car_cdr_from_data(&self, data: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex)> {
        let (vcar, vcdr) = self.arena.get_contiguous2(data)?;
        match (vcar.as_ref(), vcdr.as_ref()) {
            (Some(car), Some(cdr)) => Ok((car, cdr)),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Set car of a cons cell (mutation operation)
    /// Returns the new value on success
    #[inline]
    pub fn set_car(&self, index: ArenaIndex, new_car: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(index)? {
            Value::Cons(data) => {
                self.arena.set(data, Value::Ref(new_car))?;
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
            Value::Cons(data) => {
                self.arena.set(ArenaIndex::new(data.raw() + 1), Value::Ref(new_cdr))?;
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
    
    /// Pack 1 ArenaIndex into contiguous storage
    #[inline]
    pub fn pack_refs1(&self, a: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(Value::Ref(a))
    }
    
    /// Unpack 1 ArenaIndex from contiguous storage
    #[inline]
    pub fn unpack_refs1(&self, data: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.arena.get(data)?.as_ref().ok_or(ArenaError::InvalidIndex)
    }
    
    /// Pack 2 ArenaIndex values into contiguous storage
    #[inline]
    pub fn pack_refs2(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let data = self.arena.alloc_contiguous(2, Value::Nil)?;
        // Single borrow for both writes
        self.arena.set_contiguous2(data, Value::Ref(a), Value::Ref(b))?;
        Ok(data)
    }
    
    /// Unpack 2 ArenaIndex values from contiguous storage
    #[inline]
    pub fn unpack_refs2(&self, data: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex)> {
        // Single borrow for both reads
        let (va, vb) = self.arena.get_contiguous2(data)?;
        match (va.as_ref(), vb.as_ref()) {
            (Some(a), Some(b)) => Ok((a, b)),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Pack 3 ArenaIndex values into contiguous storage
    #[inline]
    pub fn pack_refs3(&self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let data = self.arena.alloc_contiguous(3, Value::Nil)?;
        self.arena.set_contiguous3(data, Value::Ref(a), Value::Ref(b), Value::Ref(c))?;
        Ok(data)
    }
    
    /// Unpack 3 ArenaIndex values from contiguous storage
    #[inline]
    pub fn unpack_refs3(&self, data: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex)> {
        let (va, vb, vc) = self.arena.get_contiguous3(data)?;
        match (va.as_ref(), vb.as_ref(), vc.as_ref()) {
            (Some(a), Some(b), Some(c)) => Ok((a, b, c)),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Pack 4 ArenaIndex values into contiguous storage
    #[inline]
    pub fn pack_refs4(&self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let data = self.arena.alloc_contiguous(4, Value::Nil)?;
        self.arena.set_contiguous4(data, Value::Ref(a), Value::Ref(b), Value::Ref(c), Value::Ref(d))?;
        Ok(data)
    }
    
    /// Unpack 4 ArenaIndex values from contiguous storage
    #[inline]
    pub fn unpack_refs4(&self, data: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex)> {
        let (va, vb, vc, vd) = self.arena.get_contiguous4(data)?;
        match (va.as_ref(), vb.as_ref(), vc.as_ref(), vd.as_ref()) {
            (Some(a), Some(b), Some(c), Some(d)) => Ok((a, b, c, d)),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Pack 5 ArenaIndex values into contiguous storage
    #[inline]
    pub fn pack_refs5(&self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex, e: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let data = self.arena.alloc_contiguous(5, Value::Nil)?;
        self.arena.set_contiguous5(data, Value::Ref(a), Value::Ref(b), Value::Ref(c), Value::Ref(d), Value::Ref(e))?;
        Ok(data)
    }
    
    /// Unpack 5 ArenaIndex values from contiguous storage
    #[inline]
    pub fn unpack_refs5(&self, data: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex)> {
        let (va, vb, vc, vd, ve) = self.arena.get_contiguous5(data)?;
        match (va.as_ref(), vb.as_ref(), vc.as_ref(), vd.as_ref(), ve.as_ref()) {
            (Some(a), Some(b), Some(c), Some(d), Some(e)) => Ok((a, b, c, d, e)),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Pack 6 ArenaIndex values into contiguous storage
    #[inline]
    pub fn pack_refs6(&self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex, e: ArenaIndex, f: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let data = self.arena.alloc_contiguous(6, Value::Nil)?;
        self.arena.set_contiguous6(data, Value::Ref(a), Value::Ref(b), Value::Ref(c), Value::Ref(d), Value::Ref(e), Value::Ref(f))?;
        Ok(data)
    }
    
    /// Unpack 6 ArenaIndex values from contiguous storage
    #[inline]
    pub fn unpack_refs6(&self, data: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex)> {
        let (va, vb, vc, vd, ve, vf) = self.arena.get_contiguous6(data)?;
        match (va.as_ref(), vb.as_ref(), vc.as_ref(), vd.as_ref(), ve.as_ref(), vf.as_ref()) {
            (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f)) => Ok((a, b, c, d, e, f)),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Pack 7 ArenaIndex values into contiguous storage
    #[inline]
    pub fn pack_refs7(&self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex, e: ArenaIndex, f: ArenaIndex, g: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let data = self.arena.alloc_contiguous(7, Value::Nil)?;
        self.arena.set_contiguous7(data, Value::Ref(a), Value::Ref(b), Value::Ref(c), Value::Ref(d), Value::Ref(e), Value::Ref(f), Value::Ref(g))?;
        Ok(data)
    }
    
    /// Unpack 7 ArenaIndex values from contiguous storage
    #[inline]
    pub fn unpack_refs7(&self, data: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex)> {
        let (va, vb, vc, vd, ve, vf, vg) = self.arena.get_contiguous7(data)?;
        match (va.as_ref(), vb.as_ref(), vc.as_ref(), vd.as_ref(), ve.as_ref(), vf.as_ref(), vg.as_ref()) {
            (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f), Some(g)) => Ok((a, b, c, d, e, f, g)),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
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
                Value::Cons(_) => {
                    // car is (string_index . symbol_index)
                    let car = self.car(current)?;
                    let cdr = self.cdr(current)?;
                    if let Value::Cons(_) = self.get(car)? {
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
    
    /// Create or retrieve an interned symbol from a string slice
    /// 
    /// The symbol's `chars` field points to a Value::String with the symbol name.
    /// 
    /// Symbol interning ensures the same symbol name always returns the same index.
    /// 
    /// This provides ~44% memory savings compared to linked list representation.
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        // Create string for the symbol name
        let name_str = self.string(name)?;
        
        // Check intern table
        if let Some(existing_symbol) = self.intern_table_lookup(name_str)? {
            // Free the string we just created since we're using the interned one
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
    pub fn symbol_from_bytes(&self, bytes: &[u8]) -> ArenaResult<ArenaIndex> {
        let char_count = bytes.len();
        
        // Create a Value::String for the symbol name
        let name_str = if char_count == 0 {
            // Empty string - data is NULL
            self.alloc(Value::String(ArenaIndex::NULL))?
        } else {
            // Allocate contiguous block: 1 slot for length + char_count slots for chars
            let data = self.arena.alloc_contiguous(1 + char_count, Value::Nil)?;
            
            // Store length at data[0]
            self.arena.set(data, Value::Number(char_count as isize))?;
            
            // Set characters in slots (starting at data+1)
            for (i, &b) in bytes.iter().enumerate() {
                let char_idx = self.arena.index_at_offset(data, 1 + i)?;
                self.arena.set(char_idx, Value::Char(b as char))?;
            }
            
            // Create the String value
            self.alloc(Value::String(data))?
        };
        
        // Check intern table
        if let Some(existing_symbol) = self.intern_table_lookup(name_str)? {
            // Free the string we just created since we're using the interned one
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
    /// The `id` is the index in the NativeRegistry, and `name_hash` is
    /// a simple hash for verification.
    /// 
    /// Creates a contiguous block of 2 slots: [Usize(id), Usize(name_hash)]
    #[inline]
    pub fn native(&self, id: usize, name_hash: usize) -> ArenaResult<ArenaIndex> {
        let data = self.arena.alloc_contiguous(2, Value::Nil)?;
        self.arena.set(data, Value::Usize(id))?;
        self.arena.set(ArenaIndex::new(data.raw() + 1), Value::Usize(name_hash))?;
        self.alloc(Value::Native(data))
    }
    
    /// Get the id from a native function
    pub fn native_id(&self, native_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.get(native_idx)? {
            Value::Native(data) => {
                match self.arena.get(data)? {
                    Value::Usize(id) => Ok(id),
                    _ => Err(ArenaError::InvalidIndex),
                }
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get the name_hash from a native function
    pub fn native_name_hash(&self, native_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.get(native_idx)? {
            Value::Native(data) => {
                match self.arena.get(ArenaIndex::new(data.raw() + 1))? {
                    Value::Usize(name_hash) => Ok(name_hash),
                    _ => Err(ArenaError::InvalidIndex),
                }
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Allocate a lambda
    /// 
    /// Stores lambda data as a linked structure in the arena: `(params . (body . env))`
    pub fn lambda(&self, params: ArenaIndex, body: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        // Store [params, body, env] contiguously (3 slots vs 6 for cons-list)
        let data = self.pack_refs3(params, body, env)?;
        self.alloc(Value::Lambda(data))
    }
    
    /// Extract parts from a lambda: (params, body, env)
    /// 
    /// Lambda data is stored as `[params, body, env]` in contiguous Ref slots.
    #[inline]
    pub fn lambda_parts(&self, index: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex)> {
        let val = self.get(index)?;
        match val {
            Value::Lambda(data) => {
                // data = [Ref(params), Ref(body), Ref(env)]
                self.unpack_refs3(data)
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
                Value::Cons(_) => {
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
        // Fast path: same index means same symbol
        if a == b {
            return Ok(true);
        }
        
        let val_a = self.get(a)?;
        let val_b = self.get(b)?;
        
        match (val_a, val_b) {
            (Value::Symbol(chars_a), Value::Symbol(chars_b)) => {
                self.string_eq_contiguous(chars_a, chars_b)
            }
            _ => Ok(false),
        }
    }
    
    /// Check if a symbol matches a string.
    #[inline]
    pub fn symbol_matches(&self, sym: ArenaIndex, name: &str) -> ArenaResult<bool> {
        let val = self.get(sym)?;
        
        match val {
            Value::Symbol(chars) => self.string_matches(chars, name),
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
        // Account for 4 reserved roots (nil, true, false, intern_table)
        assert!(roots.len() < MAX_ROOTS - 4, 
            "Too many GC roots: {} (max {})", roots.len(), MAX_ROOTS - 4 - 1);
        
        let mut all_roots = [ArenaIndex::NULL; MAX_ROOTS];
        let mut root_count = 0;
        
        // Add reserved slots as roots to prevent them from being collected
        all_roots[root_count] = self.nil_slot;
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
    
    /// Allocate with GC on failure
    pub fn alloc_or_gc(&self, value: Value, roots: &[ArenaIndex]) -> ArenaResult<ArenaIndex> {
        self.arena.alloc_or_gc(value, roots)
    }
    
    /// Get arena stats
    pub fn stats(&self) -> pwn_arena::ArenaStats {
        self.arena.stats()
    }
    
    // ========================================================================
    // Contiguous String Storage
    // ========================================================================
    // 
    // Strings are stored as Value::String(data) pointing to a contiguous
    // block in the arena with the following layout:
    // [Number(len), Char(c1), Char(c2), ..., Char(cn)]
    // 
    // This provides:
    // - O(1) length lookup (stored in the arena at data[0])
    // - Cache-friendly sequential access
    // - Consistent design with arrays
    // - Reduced Value enum size (no inline len field)
    // ========================================================================
    
    /// Allocate a string as contiguous Char values.
    /// 
    /// Returns a Value::String with data pointing to a contiguous block where:
    /// - data[0] contains Value::Number(len)
    /// - data[1..len+1] contains the characters
    /// Empty strings have data == NULL.
    /// 
    /// # Memory Usage
    /// 
    /// Allocates 1 + s.chars().count() slots (length header + characters),
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
    /// use grift_parser::Lisp;
    /// let lisp = Lisp::<1000>::new();
    /// let hello = lisp.string("hello").unwrap();
    /// 
    /// assert_eq!(lisp.string_len(hello).unwrap(), 5);
    /// assert_eq!(lisp.string_char_at(hello, 0).unwrap(), 'h');
    /// ```
    pub fn string(&self, s: &str) -> ArenaResult<ArenaIndex> {
        let char_count = s.chars().count();
        
        if char_count == 0 {
            // Empty string - data is NULL
            return self.alloc(Value::String(ArenaIndex::NULL));
        }
        
        // Allocate contiguous block: 1 slot for length + char_count slots for chars
        let data = self.arena.alloc_contiguous(1 + char_count, Value::Nil)?;
        
        // Store length at data[0]
        self.arena.set(data, Value::Number(char_count as isize))?;
        
        // Set characters in slots (starting at data+1)
        for (i, c) in s.chars().enumerate() {
            let char_idx = self.arena.index_at_offset(data, 1 + i)?;
            self.arena.set(char_idx, Value::Char(c))?;
        }
        
        // Create the String value pointing to the data
        self.alloc(Value::String(data))
    }
    
    /// Allocate a string from a slice of chars.
    /// 
    /// Returns an ArenaIndex pointing to a Value::String.
    pub fn string_from_chars(&self, chars: &[char]) -> ArenaResult<ArenaIndex> {
        let char_count = chars.len();
        
        if char_count == 0 {
            // Empty string - data is NULL
            return self.alloc(Value::String(ArenaIndex::NULL));
        }
        
        // Allocate contiguous block: 1 slot for length + char_count slots for chars
        let data = self.arena.alloc_contiguous(1 + char_count, Value::Nil)?;
        
        // Store length at data[0]
        self.arena.set(data, Value::Number(char_count as isize))?;
        
        // Set characters in slots (starting at data+1)
        for (i, &c) in chars.iter().enumerate() {
            let char_idx = self.arena.index_at_offset(data, 1 + i)?;
            self.arena.set(char_idx, Value::Char(c))?;
        }
        
        // Create the String value pointing to the data
        self.alloc(Value::String(data))
    }
    
    /// Get the length of a string.
    /// 
    /// Returns O(1) since length is stored in the arena at data[0].
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if the index doesn't point to
    /// a valid string.
    pub fn string_len(&self, str_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.arena.get(str_idx)? {
            Value::String(data) => {
                if data.is_null() {
                    Ok(0)
                } else {
                    match self.arena.get(data)? {
                        Value::Number(len) => Ok(len as usize),
                        _ => Err(ArenaError::InvalidIndex),
                    }
                }
            }
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
            Value::String(data) => {
                if data.is_null() {
                    return Err(ArenaError::InvalidIndex);
                }
                let len = match self.arena.get(data)? {
                    Value::Number(len) => len as usize,
                    _ => return Err(ArenaError::InvalidIndex),
                };
                if char_index >= len {
                    return Err(ArenaError::InvalidIndex);
                }
                // Characters start at data+1
                let char_slot = self.arena.index_at_offset(data, 1 + char_index)?;
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
        
        let len_a = self.string_len(a)?;
        let len_b = self.string_len(b)?;
        
        if len_a != len_b {
            return Ok(false);
        }
        
        for i in 0..len_a {
            let char_a = self.string_char_at(a, i)?;
            let char_b = self.string_char_at(b, i)?;
            if char_a != char_b {
                return Ok(false);
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
        let s_len = s.chars().count();
        
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
            Value::String(data) => {
                // Free the data slots (length header + characters)
                if !data.is_null() {
                    let len = match self.arena.get(data)? {
                        Value::Number(len) => len as usize,
                        _ => return Err(ArenaError::InvalidIndex),
                    };
                    // Free 1 + len slots (length header + characters)
                    self.arena.free_contiguous(data, 1 + len)?;
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
    // Arrays are stored as Value::Array(data) pointing to a contiguous
    // block in the arena with the following layout:
    // [Number(len), elem1, elem2, ..., elemn]
    // 
    // This provides:
    // - O(1) indexed access and mutation
    // - O(1) length lookup (stored in the arena at data[0])
    // - Cache-friendly sequential access
    // - Reduced Value enum size (no inline len field)
    // ========================================================================
    
    /// Create an array with the given length, initialized with a default value.
    /// 
    /// The array stores `len` values contiguously in the arena with a length header.
    /// 
    /// # Memory Usage
    /// 
    /// Allocates 1 + len slots (length header + elements), plus 1 slot for the Array value itself.
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use grift_parser::Lisp;
    /// let lisp = Lisp::<1000>::new();
    /// let arr = lisp.make_array(3, lisp.nil().unwrap()).unwrap();
    /// 
    /// assert_eq!(lisp.array_len(arr).unwrap(), 3);
    /// ```
    pub fn make_array(&self, len: usize, default: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if len == 0 {
            // Empty array - data is NULL
            return self.alloc(Value::Array(ArenaIndex::NULL));
        }
        
        // Allocate contiguous block: 1 slot for length + len slots for elements
        let default_val = self.get(default)?;
        let data = self.arena.alloc_contiguous(1 + len, default_val)?;
        
        // Store length at data[0]
        self.arena.set(data, Value::Number(len as isize))?;
        
        // Elements are already initialized with default_val at data+1 through data+len
        // (alloc_contiguous initializes all slots with the provided value)
        
        // Create the Array value pointing to the data
        self.alloc(Value::Array(data))
    }
    
    /// Get the length of an array.
    /// 
    /// Returns O(1) since length is stored in the arena at data[0].
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if the index doesn't point to an array.
    pub fn array_len(&self, arr_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.arena.get(arr_idx)? {
            Value::Array(data) => {
                if data.is_null() {
                    Ok(0)
                } else {
                    match self.arena.get(data)? {
                        Value::Number(len) => Ok(len as usize),
                        _ => Err(ArenaError::InvalidIndex),
                    }
                }
            }
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
            Value::Array(data) => {
                if data.is_null() {
                    return Err(ArenaError::InvalidIndex);
                }
                let len = match self.arena.get(data)? {
                    Value::Number(len) => len as usize,
                    _ => return Err(ArenaError::InvalidIndex),
                };
                if index >= len {
                    return Err(ArenaError::InvalidIndex);
                }
                // Elements start at data+1
                self.arena.index_at_offset(data, 1 + index)
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
            Value::Array(data) => {
                if data.is_null() {
                    return Err(ArenaError::InvalidIndex);
                }
                let len = match self.arena.get(data)? {
                    Value::Number(len) => len as usize,
                    _ => return Err(ArenaError::InvalidIndex),
                };
                if index >= len {
                    return Err(ArenaError::InvalidIndex);
                }
                // Elements start at data+1
                let elem_slot = self.arena.index_at_offset(data, 1 + index)?;
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
            Value::Array(data) => {
                // Free the data slots (length header + elements)
                if !data.is_null() {
                    let len = match self.arena.get(data)? {
                        Value::Number(len) => len as usize,
                        _ => return Err(ArenaError::InvalidIndex),
                    };
                    // Free 1 + len slots (length header + elements)
                    self.arena.free_contiguous(data, 1 + len)?;
                }
                // Free the Array value itself
                self.arena.free(arr_idx)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
}

impl<const N: usize> Default for Lisp<N> {
    fn default() -> Self {
        Self::new()
    }
}
