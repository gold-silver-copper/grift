//! Lisp execution context with arena wrapper
//!
//! This module contains the `Lisp<N>` struct and all its methods for managing
//! Lisp values in the arena.
//!
//! Note: The `impl_pack_unpack_refs!` macro has been moved to `src/macros.rs`.

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
/// - Slot 0: `Value::Nil` - empty list ()
/// - Slot 1: `Value::True` - boolean true (#t)
/// - Slot 2: `Value::False` - boolean false (#f)
///
/// These slots are pre-allocated during `Lisp::new()` and returned as
/// constants from `true_val()` and `false_val()`. This optimization
/// avoids allocating new slots for these frequently-used values.
///
/// ## Symbols and Paint
///
/// Symbols carry a `paint` field for hygienic macro expansion. User code symbols
/// get paint 0, while macro-introduced symbols get fresh paint values. This enables
/// Kohlbecker-style hygiene without symbol interning.
///
/// Note: Symbol interning was removed to support painted symbols. Two symbols with
/// the same name but different paint are distinct identifiers.
pub struct Lisp<const N: usize> {
    arena: Arena<Value, N>,
    /// Pre-allocated Nil slot (always slot 0)
    nil_slot: ArenaIndex,
    /// Pre-allocated True slot (always slot 1)
    true_slot: ArenaIndex,
    /// Pre-allocated False slot (always slot 2)
    false_slot: ArenaIndex,
}

/// Number of reserved slots in the arena:
/// - nil (1), true (1), false (1)
pub const RESERVED_SLOTS: usize = 3;

impl<const N: usize> Lisp<N> {
    /// Create a new Lisp context
    ///
    /// Pre-allocates reserved slots for nil, true, and false.
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

        Lisp {
            arena,
            nil_slot,
            true_slot,
            false_slot,
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
    // Symbol Creation (with Paint for Hygiene)
    // ========================================================================

    /// Create a symbol with explicit paint value.
    ///
    /// This is the primary symbol creation method. The paint field enables
    /// hygienic macro expansion:
    /// - paint 0: User code (parsed input)
    /// - paint > 0: Macro-introduced identifiers
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_parser::Lisp;
    /// let lisp = Lisp::<1000>::new();
    ///
    /// // User code symbol (paint 0)
    /// let user_x = lisp.symbol_with_paint("x", 0).unwrap();
    ///
    /// // Macro-introduced symbol (paint 1)
    /// let macro_x = lisp.symbol_with_paint("x", 1).unwrap();
    ///
    /// // Same name, different paint = different identifiers
    /// assert!(!lisp.symbol_eq(user_x, macro_x).unwrap());
    /// assert!(lisp.symbol_name_eq(user_x, macro_x).unwrap());
    /// ```
    pub fn symbol_with_paint(&self, name: &str, paint: usize) -> ArenaResult<ArenaIndex> {
        let name_str = self.string(name)?;
        self.alloc(Value::Symbol { name: name_str, paint })
    }

    /// Create a symbol with paint 0 (user code).
    ///
    /// This is a convenience method for creating symbols from user input.
    /// Equivalent to `symbol_with_paint(name, 0)`.
    #[inline]
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        self.symbol_with_paint(name, 0)
    }

    /// Create a symbol from bytes with explicit paint value.
    ///
    /// Used primarily by the parser for efficiency.
    pub fn symbol_from_bytes_with_paint(&self, bytes: &[u8], paint: usize) -> ArenaResult<ArenaIndex> {
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

        // Create symbol with the specified paint
        self.alloc(Value::Symbol { name: name_str, paint })
    }

    /// Create a symbol from bytes with paint 0 (user code).
    ///
    /// This is a convenience method for the parser.
    /// Equivalent to `symbol_from_bytes_with_paint(bytes, 0)`.
    #[inline]
    pub fn symbol_from_bytes(&self, bytes: &[u8]) -> ArenaResult<ArenaIndex> {
        self.symbol_from_bytes_with_paint(bytes, 0)
    }

    /// Get the name (String index) from a symbol.
    ///
    /// Returns the ArenaIndex pointing to the symbol's name string.
    #[inline]
    pub fn symbol_name(&self, sym: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(sym)? {
            Value::Symbol { name, .. } => Ok(name),
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    /// Get the paint value from a symbol.
    #[inline]
    pub fn symbol_paint(&self, sym: ArenaIndex) -> ArenaResult<usize> {
        match self.get(sym)? {
            Value::Symbol { paint, .. } => Ok(paint),
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    /// Get both name and paint from a symbol.
    #[inline]
    pub fn symbol_parts(&self, sym: ArenaIndex) -> ArenaResult<(ArenaIndex, usize)> {
        match self.get(sym)? {
            Value::Symbol { name, paint } => Ok((name, paint)),
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    /// Create a copy of a symbol with a different paint value.
    ///
    /// The name string is shared (same ArenaIndex), only paint changes.
    /// Used during macro expansion to repaint identifiers.
    #[inline]
    pub fn repaint_symbol(&self, sym: ArenaIndex, new_paint: usize) -> ArenaResult<ArenaIndex> {
        let name = self.symbol_name(sym)?;
        self.alloc(Value::Symbol { name, paint: new_paint })
    }

    /// Check if two symbols have the same name (ignoring paint).
    ///
    /// This is used for pattern matching in macros where we care about
    /// the identifier name but not its hygiene context.
    pub fn symbol_name_eq(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        let name_a = self.symbol_name(a)?;
        let name_b = self.symbol_name(b)?;
        self.string_eq_contiguous(name_a, name_b)
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
    /// Both values are stored inline - no arena data slots needed.
    #[inline]
    pub fn native(&self, id: usize, name_hash: usize) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Native { id, name_hash })
    }
    
    /// Get the id from a native function
    /// O(1) access - id is stored inline.
    pub fn native_id(&self, native_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.get(native_idx)? {
            Value::Native { id, .. } => Ok(id),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get the name_hash from a native function
    /// O(1) access - name_hash is stored inline.
    pub fn native_name_hash(&self, native_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.get(native_idx)? {
            Value::Native { name_hash, .. } => Ok(name_hash),
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
    
    /// Check if two symbols are fully equal (same name AND same paint).
    ///
    /// For hygienic macros, two symbols with the same name but different paint
    /// are considered different identifiers. Use `symbol_name_eq` to compare
    /// only by name.
    #[inline]
    pub fn symbol_eq(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        // Fast path: same index means same symbol
        if a == b {
            return Ok(true);
        }

        let val_a = self.get(a)?;
        let val_b = self.get(b)?;

        match (val_a, val_b) {
            (Value::Symbol { name: name_a, paint: paint_a },
             Value::Symbol { name: name_b, paint: paint_b }) => {
                // Must have same paint AND same name
                if paint_a != paint_b {
                    return Ok(false);
                }
                self.string_eq_contiguous(name_a, name_b)
            }
            _ => Ok(false),
        }
    }
    
    /// Check if a symbol's name matches a string (ignores paint).
    ///
    /// This is used for special form detection in the evaluator.
    #[inline]
    pub fn symbol_matches(&self, sym: ArenaIndex, target: &str) -> ArenaResult<bool> {
        match self.get(sym)? {
            Value::Symbol { name, .. } => self.string_matches(name, target),
            _ => Ok(false),
        }
    }
    
    /// Extract symbol name to a fixed buffer.
    pub fn symbol_to_bytes(&self, sym: ArenaIndex, buf: &mut [u8]) -> ArenaResult<usize> {
        match self.get(sym)? {
            Value::Symbol { name, .. } => self.string_to_bytes(name, buf),
            _ => Ok(0),
        }
    }

    /// Get the length of a symbol's name.
    pub fn symbol_len(&self, sym: ArenaIndex) -> ArenaResult<usize> {
        match self.get(sym)? {
            Value::Symbol { name, .. } => self.string_len(name),
            _ => Ok(0),
        }
    }

    /// Get a character at a specific index within a symbol's name.
    /// Returns None if the index is out of bounds or if the value is not a symbol.
    pub fn symbol_char_at(&self, sym: ArenaIndex, index: usize) -> ArenaResult<Option<char>> {
        match self.get(sym)? {
            Value::Symbol { name, .. } => {
                let len = self.string_len(name)?;
                if index >= len {
                    Ok(None)
                } else {
                    Ok(Some(self.string_char_at(name, index)?))
                }
            }
            _ => Ok(None),
        }
    }
    
    /// Run garbage collection.
    ///
    /// The reserved slots (nil, true, false) are always included as GC roots.
    ///
    /// # Panics
    ///
    /// Panics if the number of roots exceeds the internal limit (512 roots).
    /// This limit is chosen to balance stack usage in no_std environments
    /// with typical program needs. Most Lisp programs use far fewer roots.
    pub fn gc(&self, roots: &[ArenaIndex]) -> GcStats {
        // Create a new roots array with reserved slots included
        // Using const-sized array to avoid alloc in no_std
        // 512 roots should be sufficient for most programs while keeping
        // stack usage reasonable (~8KB on 64-bit systems)
        const MAX_ROOTS: usize = 512;

        // Panic if too many roots - this indicates a programming error
        // Account for 3 reserved roots (nil, true, false)
        assert!(roots.len() < MAX_ROOTS - 3,
            "Too many GC roots: {} (max {})", roots.len(), MAX_ROOTS - 3 - 1);

        let mut all_roots = [ArenaIndex::NIL; MAX_ROOTS];
        let mut root_count = 0;

        // Add reserved slots as roots to prevent them from being collected
        all_roots[root_count] = self.nil_slot;
        root_count += 1;
        all_roots[root_count] = self.true_slot;
        root_count += 1;
        all_roots[root_count] = self.false_slot;
        root_count += 1;

        // Copy provided roots
        for &root in roots {
            all_roots[root_count] = root;
            root_count += 1;
        }

        self.arena.collect_garbage(&all_roots[..root_count])
    }
    
    
    /// Get arena stats
    pub fn stats(&self) -> pwn_arena::ArenaStats {
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
    /// use grift_parser::Lisp;
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
}

impl<const N: usize> Default for Lisp<N> {
    fn default() -> Self {
        Self::new()
    }
}
