#![no_std]

//! # Fixed-Size Arena Allocator
//!
//! A minimal no-std, no-alloc arena allocator with fixed capacity.
//!
//! ## Features
//!
//! - **Fixed-size**: All memory pre-allocated at compile time
//! - **No-std, no-alloc**: Works in embedded environments with no heap
//! - **Generic**: Works with any `Copy` type
//! - **Interior mutability**: Safe concurrent access via `RefCell`
//! - **Generational indices**: Detects use-after-free (ABA problem)
//! - **O(1) allocation**: Free-list based allocation and deallocation
//! - **Mark-and-sweep GC**: Trait-based garbage collection via [`Trace`]
//! - **Zero dependencies**: Only uses `core::cell::RefCell`
//!
//! ## Example
//!
//! ```rust
//! use pwn_arena::{Arena, ArenaIndex};
//!
//! #[derive(Clone, Copy, Debug, PartialEq)]
//! enum Node {
//!     Leaf(i32),
//!     Branch(ArenaIndex, ArenaIndex),
//! }
//!
//! let arena: Arena<Node, 1024> = Arena::new(Node::Leaf(0));
//!
//! // Allocate nodes
//! let left = arena.alloc(Node::Leaf(1)).unwrap();
//! let right = arena.alloc(Node::Leaf(2)).unwrap();
//! let root = arena.alloc(Node::Branch(left, right)).unwrap();
//!
//! // Access nodes
//! if let Node::Branch(l, r) = arena.get(root).unwrap() {
//!     println!("Left: {:?}, Right: {:?}", arena.get(l), arena.get(r));
//! }
//!
//! // Free when done
//! arena.free(root).unwrap();
//! ```

use core::cell::RefCell;

// ============================================================================
// Types
// ============================================================================

/// Index into the arena with generational tracking.
///
/// This is a type-safe wrapper that stores both the slot index and the
/// generation at which it was allocated. This prevents the ABA problem
/// where a freed and reallocated slot could be accessed by a stale index.
///
/// # Safety Note
///
/// Indices should only be obtained from arena operations (`alloc`, `iter`).
/// Manually constructing indices with [`ArenaIndex::new`] bypasses the type
/// system's protection and should only be used for serialization/deserialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArenaIndex {
    index: usize,
    generation: u32,
}

impl ArenaIndex {
    /// A sentinel "null" index that is never valid.
    ///
    /// This can be used as a placeholder when an optional index is needed
    /// but `Option<ArenaIndex>` is not desired.
    pub const NULL: ArenaIndex = ArenaIndex {
        index: usize::MAX,
        generation: u32::MAX,
    };

    /// Create a new arena index with the given slot index and generation.
    ///
    /// # Warning
    ///
    /// This is a low-level constructor intended for serialization/deserialization.
    /// For normal use, obtain indices from [`Arena::alloc`] or [`Arena::iter`].
    /// Fabricating indices manually may lead to undefined behavior if the
    /// index/generation pair doesn't correspond to a valid allocation.
    #[inline]
    pub const fn new(index: usize, generation: u32) -> Self {
        ArenaIndex { index, generation }
    }

    /// Get the raw slot index value.
    #[inline]
    pub const fn raw(self) -> usize {
        self.index
    }

    /// Get the generation this index was created with.
    #[inline]
    pub const fn generation(self) -> u32 {
        self.generation
    }

    /// Check if this is the null index.
    #[inline]
    pub const fn is_null(self) -> bool {
        self.index == usize::MAX && self.generation == u32::MAX
    }
}

impl Default for ArenaIndex {
    /// Returns [`ArenaIndex::NULL`].
    fn default() -> Self {
        Self::NULL
    }
}

/// Errors that can occur during arena operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArenaError {
    /// Arena is full, cannot allocate more cells.
    OutOfMemory,

    /// Invalid index (out of bounds, not allocated, or stale generation).
    InvalidIndex,

    /// The index's generation doesn't match the slot's current generation.
    /// This indicates a use-after-free attempt (ABA problem).
    GenerationMismatch,
}

impl ArenaError {
    /// Get a human-readable description of the error.
    pub const fn as_str(&self) -> &'static str {
        match self {
            ArenaError::OutOfMemory => "arena is full",
            ArenaError::InvalidIndex => "invalid index",
            ArenaError::GenerationMismatch => "stale index (generation mismatch)",
        }
    }

    /// Check if this error indicates the arena is full.
    pub const fn is_out_of_memory(&self) -> bool {
        matches!(self, ArenaError::OutOfMemory)
    }

    /// Check if this error indicates an invalid or stale index.
    pub const fn is_invalid_index(&self) -> bool {
        matches!(self, ArenaError::InvalidIndex | ArenaError::GenerationMismatch)
    }
}

/// Result type for arena operations.
pub type ArenaResult<T> = Result<T, ArenaError>;

// ============================================================================
// Slot Implementation (Free-List Support)
// ============================================================================

/// Sentinel value indicating end of free list.
const FREE_LIST_END: usize = usize::MAX;

/// Internal slot representation for free-list based allocation.
///
/// Each slot is either free (storing the next free slot index) or
/// occupied (storing the actual value).
#[derive(Clone, Copy)]
enum Slot<T: Copy> {
    /// Free slot containing index of the next free slot (or FREE_LIST_END).
    Free { next_free: usize },
    /// Occupied slot containing the stored value.
    Occupied { value: T },
}

// ============================================================================
// Arena Implementation
// ============================================================================

/// Fixed-size arena allocator with generational indices and O(1) allocation.
///
/// # Type Parameters
///
/// - `T`: The type of values stored (must be `Copy` for array initialization)
/// - `N`: Maximum number of cells (const generic)
///
/// # Memory Layout
///
/// - `slots`: Array of `Slot<T>` (either free with next pointer, or occupied with value)
/// - `generations`: Array of generation counters for each slot
/// - `free_head`: Head of the free list
/// - `len`: Number of currently allocated slots
/// - `gc_enabled`: Whether garbage collection is enabled
///
/// # Generational Indices
///
/// Each slot has a generation counter that increments when freed. An `ArenaIndex`
/// stores the generation it was created with. If generations don't match during
/// access, `InvalidIndex` is returned, preventing the ABA problem.
///
/// # O(1) Allocation
///
/// Uses a free-list for constant-time allocation and deallocation instead of
/// scanning a bitmap.
///
/// # Garbage Collection
///
/// The arena supports mark-and-sweep garbage collection via the [`Trace`] trait.
/// GC can be enabled or disabled at runtime using [`Arena::set_gc_enabled`].
/// When disabled, [`Arena::collect_garbage`] returns immediately without collecting.
pub struct Arena<T: Copy, const N: usize> {
    slots: RefCell<[Slot<T>; N]>,
    generations: RefCell<[u32; N]>,
    free_head: RefCell<usize>,
    len: RefCell<usize>,
    gc_enabled: RefCell<bool>,
}

impl<T: Copy, const N: usize> Arena<T, N> {
    /// Create a new arena.
    ///
    /// All slots start as free, linked together in a free list.
    /// Each slot is initialized with a unique generation based on its index,
    /// which provides additional protection against accidentally using an
    /// index meant for a different slot.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::Arena;
    ///
    /// let arena: Arena<i32, 100> = Arena::new(0);
    /// ```
    pub fn new(_default_value: T) -> Self {
        // Initialize all slots as free, linked together
        // Slot 0 -> 1 -> 2 -> ... -> N-1 -> FREE_LIST_END
        let mut slots = [Slot::Free { next_free: FREE_LIST_END }; N];
        for i in 0..N {
            slots[i] = Slot::Free {
                next_free: if i + 1 < N { i + 1 } else { FREE_LIST_END },
            };
        }

        // Initialize each slot with a unique generation based on slot index.
        // This prevents accidental cross-slot index fabrication from succeeding,
        // since different slots will have different initial generations.
        let mut generations = [0u32; N];
        for i in 0..N {
            generations[i] = i as u32;
        }

        Arena {
            slots: RefCell::new(slots),
            generations: RefCell::new(generations),
            free_head: RefCell::new(if N > 0 { 0 } else { FREE_LIST_END }),
            len: RefCell::new(0),
            gc_enabled: RefCell::new(true), // GC enabled by default
        }
    }

    /// Check if garbage collection is enabled.
    ///
    /// When disabled, [`Arena::collect_garbage`] returns immediately without
    /// performing any collection.
    #[inline]
    pub fn is_gc_enabled(&self) -> bool {
        *self.gc_enabled.borrow()
    }

    /// Enable or disable garbage collection.
    ///
    /// When disabled, [`Arena::collect_garbage`] returns immediately with
    /// zero marked/collected stats. This can be useful for:
    /// - Performance-critical sections where you want to defer GC
    /// - Debugging to isolate GC-related issues
    /// - Temporarily pausing GC during batch operations
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::Arena;
    ///
    /// let arena: Arena<i32, 100> = Arena::new(0);
    ///
    /// // Disable GC for a batch operation
    /// arena.set_gc_enabled(false);
    ///
    /// // ... perform many allocations ...
    ///
    /// // Re-enable and collect
    /// arena.set_gc_enabled(true);
    /// ```
    #[inline]
    pub fn set_gc_enabled(&self, enabled: bool) {
        *self.gc_enabled.borrow_mut() = enabled;
    }

    /// Temporarily disable GC, run a closure, then restore the previous state.
    ///
    /// This is useful for critical sections where GC should not run.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::Arena;
    ///
    /// let arena: Arena<i32, 100> = Arena::new(0);
    ///
    /// let result = arena.without_gc(|| {
    ///     // GC is disabled in here
    ///     arena.alloc(42).unwrap()
    /// });
    /// // GC is re-enabled here
    /// ```
    pub fn without_gc<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let was_enabled = self.is_gc_enabled();
        self.set_gc_enabled(false);
        let result = f();
        self.set_gc_enabled(was_enabled);
        result
    }

    /// Temporarily enable GC, run a closure, then restore the previous state.
    ///
    /// This is useful when GC is normally disabled but you want to force
    /// a collection in a specific section.
    pub fn with_gc<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let was_enabled = self.is_gc_enabled();
        self.set_gc_enabled(true);
        let result = f();
        self.set_gc_enabled(was_enabled);
        result
    }

    /// Get the maximum capacity of this arena.
    #[inline]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Get the number of currently allocated cells.
    #[inline]
    pub fn len(&self) -> usize {
        *self.len.borrow()
    }

    /// Check if the arena is empty (no allocated cells).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the arena is full (all cells allocated).
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == N
    }

    /// Get the number of free cells.
    #[inline]
    pub fn available(&self) -> usize {
        N - self.len()
    }

    /// Allocate a new cell and return its index.
    ///
    /// Uses O(1) free-list based allocation.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::OutOfMemory` if the arena is full.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::Arena;
    ///
    /// let arena: Arena<i32, 10> = Arena::new(0);
    /// let idx = arena.alloc(42).unwrap();
    /// assert_eq!(arena.get(idx).unwrap(), 42);
    /// ```
    #[must_use]
    pub fn alloc(&self, value: T) -> ArenaResult<ArenaIndex> {
        let mut free_head = self.free_head.borrow_mut();

        // Check if there's a free slot
        if *free_head == FREE_LIST_END {
            return Err(ArenaError::OutOfMemory);
        }

        let idx = *free_head;
        let mut slots = self.slots.borrow_mut();

        // Pop from free list
        let next_free = match slots[idx] {
            Slot::Free { next_free } => next_free,
            Slot::Occupied { .. } => unreachable!("free_head pointed to occupied slot"),
        };

        // Mark as occupied
        slots[idx] = Slot::Occupied { value };
        *free_head = next_free;

        // Get current generation for this slot - reuse slots borrow scope
        drop(slots);
        let generation = self.generations.borrow()[idx];

        // Increment allocated count
        *self.len.borrow_mut() += 1;

        Ok(ArenaIndex::new(idx, generation))
    }

    /// Check that an index is valid (in bounds and generation matches).
    /// Returns the slot index if valid.
    /// Note: Does NOT check if the slot is occupied - caller must verify.
    #[inline]
    fn check_bounds_and_generation(&self, index: ArenaIndex) -> ArenaResult<usize> {
        let idx = index.raw();

        if idx >= N {
            return Err(ArenaError::InvalidIndex);
        }

        // Check generation
        let current_gen = self.generations.borrow()[idx];
        if index.generation() != current_gen {
            return Err(ArenaError::GenerationMismatch);
        }

        Ok(idx)
    }

    /// Validate an index and return the slot index if valid.
    #[inline]
    fn validate_index(&self, index: ArenaIndex) -> ArenaResult<usize> {
        let idx = self.check_bounds_and_generation(index)?;

        // Check if slot is occupied
        match self.slots.borrow()[idx] {
            Slot::Occupied { .. } => Ok(idx),
            Slot::Free { .. } => Err(ArenaError::InvalidIndex),
        }
    }

    /// Get a copy of the value at the given index.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::InvalidIndex` if the index is out of bounds or not allocated.
    /// Returns `ArenaError::GenerationMismatch` if the index is stale (slot was freed and reused).
    #[inline]
    #[must_use]
    pub fn get(&self, index: ArenaIndex) -> ArenaResult<T> {
        let idx = self.check_bounds_and_generation(index)?;

        // Check if slot is occupied and get value in one borrow
        match self.slots.borrow()[idx] {
            Slot::Occupied { value } => Ok(value),
            Slot::Free { .. } => Err(ArenaError::InvalidIndex),
        }
    }

    /// Set the value at the given index.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::InvalidIndex` if the index is out of bounds or not allocated.
    /// Returns `ArenaError::GenerationMismatch` if the index is stale.
    #[inline]
    pub fn set(&self, index: ArenaIndex, value: T) -> ArenaResult<()> {
        let idx = self.validate_index(index)?;

        self.slots.borrow_mut()[idx] = Slot::Occupied { value };
        Ok(())
    }

    /// Modify a value in place using a closure.
    ///
    /// This is more efficient than `get` + `set` as it avoids copying the value twice.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::InvalidIndex` if the index is out of bounds or not allocated.
    /// Returns `ArenaError::GenerationMismatch` if the index is stale.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::Arena;
    ///
    /// let arena: Arena<i32, 10> = Arena::new(0);
    /// let idx = arena.alloc(42).unwrap();
    ///
    /// arena.modify(idx, |v| *v += 10).unwrap();
    /// assert_eq!(arena.get(idx).unwrap(), 52);
    /// ```
    #[inline]
    pub fn modify<F>(&self, index: ArenaIndex, f: F) -> ArenaResult<()>
    where
        F: FnOnce(&mut T),
    {
        let idx = self.validate_index(index)?;

        let mut slots = self.slots.borrow_mut();
        if let Slot::Occupied { ref mut value } = slots[idx] {
            f(value);
            Ok(())
        } else {
            Err(ArenaError::InvalidIndex)
        }
    }

    /// Get a value, returning `None` instead of an error if invalid.
    ///
    /// This is a convenience method for cases where you expect the index
    /// might be invalid and want to handle it with `Option` instead of `Result`.
    #[inline]
    #[must_use]
    pub fn try_get(&self, index: ArenaIndex) -> Option<T> {
        self.get(index).ok()
    }

    /// Swap the values at two indices.
    ///
    /// # Errors
    ///
    /// Returns an error if either index is invalid.
    pub fn swap(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<()> {
        let idx_a = self.validate_index(a)?;
        let idx_b = self.validate_index(b)?;

        if idx_a == idx_b {
            return Ok(()); // Same slot, nothing to do
        }

        let mut slots = self.slots.borrow_mut();

        let (val_a, val_b) = match (&slots[idx_a], &slots[idx_b]) {
            (Slot::Occupied { value: va }, Slot::Occupied { value: vb }) => (*va, *vb),
            _ => return Err(ArenaError::InvalidIndex),
        };

        slots[idx_a] = Slot::Occupied { value: val_b };
        slots[idx_b] = Slot::Occupied { value: val_a };

        Ok(())
    }

    /// Replace the value at an index, returning the old value.
    ///
    /// # Errors
    ///
    /// Returns an error if the index is invalid.
    pub fn replace(&self, index: ArenaIndex, value: T) -> ArenaResult<T> {
        let idx = self.validate_index(index)?;

        let mut slots = self.slots.borrow_mut();
        match slots[idx] {
            Slot::Occupied { value: old } => {
                slots[idx] = Slot::Occupied { value };
                Ok(old)
            }
            Slot::Free { .. } => Err(ArenaError::InvalidIndex),
        }
    }

    /// Free a cell, making it available for reuse.
    ///
    /// This increments the slot's generation, invalidating any existing indices
    /// to this slot.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::InvalidIndex` if the index is out of bounds or not allocated.
    /// Returns `ArenaError::GenerationMismatch` if the index is stale.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::Arena;
    ///
    /// let arena: Arena<i32, 10> = Arena::new(0);
    /// let idx = arena.alloc(42).unwrap();
    /// arena.free(idx).unwrap();
    /// assert_eq!(arena.len(), 0);
    /// ```
    #[inline]
    pub fn free(&self, index: ArenaIndex) -> ArenaResult<()> {
        let idx = self.validate_index(index)?;

        // Increment generation to invalidate any existing indices
        {
            let mut generations = self.generations.borrow_mut();
            generations[idx] = generations[idx].wrapping_add(1);
        }

        // Push onto free list
        let mut free_head = self.free_head.borrow_mut();
        self.slots.borrow_mut()[idx] = Slot::Free { next_free: *free_head };
        *free_head = idx;

        // Decrement allocated count
        *self.len.borrow_mut() -= 1;

        Ok(())
    }

    /// Check if an index is currently valid (allocated with matching generation).
    #[inline]
    pub fn is_allocated(&self, index: ArenaIndex) -> bool {
        self.validate_index(index).is_ok()
    }

    /// Clear all allocations, making the entire arena available.
    ///
    /// This increments all generations to invalidate existing indices.
    ///
    /// # Warning
    ///
    /// This does not call any destructors. Use with caution.
    pub fn clear(&self) {
        // Rebuild free list
        let mut slots = self.slots.borrow_mut();
        for i in 0..N {
            slots[i] = Slot::Free {
                next_free: if i + 1 < N { i + 1 } else { FREE_LIST_END },
            };
        }

        // Increment all generations to invalidate existing indices
        let mut generations = self.generations.borrow_mut();
        for g in generations.iter_mut() {
            *g = g.wrapping_add(1);
        }

        *self.free_head.borrow_mut() = if N > 0 { 0 } else { FREE_LIST_END };
        *self.len.borrow_mut() = 0;
    }

    /// Iterate over all allocated indices and values.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::Arena;
    ///
    /// let arena: Arena<i32, 10> = Arena::new(0);
    /// arena.alloc(1).unwrap();
    /// arena.alloc(2).unwrap();
    /// arena.alloc(3).unwrap();
    ///
    /// for (idx, value) in arena.iter() {
    ///     println!("Index {:?}: {}", idx, value);
    /// }
    /// ```
    pub fn iter(&self) -> ArenaIterator<'_, T, N> {
        ArenaIterator {
            arena: self,
            current: 0,
        }
    }

    /// Get statistics about arena usage.
    pub fn stats(&self) -> ArenaStats {
        let allocated = self.len();
        ArenaStats {
            capacity: N,
            allocated,
            free: N - allocated,
            fragmentation: self.calculate_fragmentation(),
        }
    }

    fn calculate_fragmentation(&self) -> f32 {
        let slots = self.slots.borrow();
        let mut fragments = 0;
        let mut in_free = false;

        for slot in slots.iter() {
            match slot {
                Slot::Free { .. } => {
                    if !in_free {
                        fragments += 1;
                        in_free = true;
                    }
                }
                Slot::Occupied { .. } => {
                    in_free = false;
                }
            }
        }

        if fragments == 0 {
            0.0
        } else {
            fragments as f32 / N as f32
        }
    }

    /// Validate internal consistency of the arena.
    ///
    /// Returns `true` if the arena's internal state is consistent.
    /// This is useful for debugging and testing.
    ///
    /// Checks:
    /// - Free list integrity (no cycles, correct length)
    /// - Slot count matches `len`
    /// - All free slots are in the free list
    pub fn validate(&self) -> bool {
        let slots = self.slots.borrow();
        let free_head = *self.free_head.borrow();
        let len = *self.len.borrow();

        // Count occupied slots
        let occupied_count = slots.iter().filter(|s| matches!(s, Slot::Occupied { .. })).count();
        if occupied_count != len {
            return false;
        }

        // Validate free list
        let mut free_count = 0;
        let mut visited = [false; N];
        let mut current = free_head;

        while current != FREE_LIST_END {
            if current >= N {
                return false; // Invalid index
            }
            if visited[current] {
                return false; // Cycle detected
            }
            visited[current] = true;

            match slots[current] {
                Slot::Free { next_free } => {
                    free_count += 1;
                    current = next_free;
                }
                Slot::Occupied { .. } => {
                    return false; // Free list points to occupied slot
                }
            }
        }

        // Check that free_count + occupied_count == N
        if free_count + occupied_count != N {
            return false;
        }

        // Check all free slots are in the free list
        for (i, slot) in slots.iter().enumerate() {
            if matches!(slot, Slot::Free { .. }) && !visited[i] {
                return false; // Free slot not in free list
            }
        }

        true
    }

    /// Get the generation counter for a slot (for debugging).
    ///
    /// Returns `None` if the index is out of bounds.
    pub fn get_generation(&self, slot_index: usize) -> Option<u32> {
        if slot_index < N {
            Some(self.generations.borrow()[slot_index])
        } else {
            None
        }
    }

    /// Check if a slot index is currently occupied (ignoring generation).
    ///
    /// This is a low-level debugging method. For normal use, prefer [`is_allocated`].
    pub fn is_slot_occupied(&self, slot_index: usize) -> bool {
        if slot_index >= N {
            return false;
        }
        matches!(self.slots.borrow()[slot_index], Slot::Occupied { .. })
    }

    /// Get indices of all allocated slots.
    ///
    /// Returns an array with the first `len()` elements being valid indices.
    /// The remaining elements are [`ArenaIndex::NULL`].
    pub fn allocated_indices(&self) -> [ArenaIndex; N] {
        let mut result = [ArenaIndex::NULL; N];
        let mut count = 0;

        let slots = self.slots.borrow();
        let generations = self.generations.borrow();

        for (idx, slot) in slots.iter().enumerate() {
            if let Slot::Occupied { .. } = slot {
                result[count] = ArenaIndex::new(idx, generations[idx]);
                count += 1;
            }
        }

        result
    }

    /// Apply a function to all allocated values.
    ///
    /// This is useful for bulk updates without the overhead of iteration.
    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(ArenaIndex, &T),
    {
        let slots = self.slots.borrow();
        let generations = self.generations.borrow();

        for (idx, slot) in slots.iter().enumerate() {
            if let Slot::Occupied { value } = slot {
                f(ArenaIndex::new(idx, generations[idx]), value);
            }
        }
    }

    /// Apply a mutating function to all allocated values.
    pub fn for_each_mut<F>(&self, mut f: F)
    where
        F: FnMut(ArenaIndex, &mut T),
    {
        let mut slots = self.slots.borrow_mut();
        let generations = self.generations.borrow();

        for (idx, slot) in slots.iter_mut().enumerate() {
            if let Slot::Occupied { value } = slot {
                f(ArenaIndex::new(idx, generations[idx]), value);
            }
        }
    }

    /// Count values matching a predicate.
    pub fn count_where<F>(&self, predicate: F) -> usize
    where
        F: Fn(&T) -> bool,
    {
        let slots = self.slots.borrow();
        slots
            .iter()
            .filter(|slot| {
                if let Slot::Occupied { value } = slot {
                    predicate(value)
                } else {
                    false
                }
            })
            .count()
    }

    /// Find the first value matching a predicate.
    pub fn find<F>(&self, predicate: F) -> Option<(ArenaIndex, T)>
    where
        F: Fn(&T) -> bool,
    {
        let slots = self.slots.borrow();
        let generations = self.generations.borrow();

        for (idx, slot) in slots.iter().enumerate() {
            if let Slot::Occupied { value } = slot {
                if predicate(value) {
                    return Some((ArenaIndex::new(idx, generations[idx]), *value));
                }
            }
        }

        None
    }

    /// Check if any allocated value matches a predicate.
    pub fn any<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.find(predicate).is_some()
    }

    /// Check if all allocated values match a predicate.
    ///
    /// Returns `true` if the arena is empty.
    pub fn all<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        let slots = self.slots.borrow();

        for slot in slots.iter() {
            if let Slot::Occupied { value } = slot {
                if !predicate(value) {
                    return false;
                }
            }
        }

        true
    }
}

// ============================================================================
// Recursive Deletion Support
// ============================================================================

/// Trait for types that can be recursively deleted from the arena.
///
/// Implement this for types that contain `ArenaIndex` fields pointing
/// to other allocations that should be freed together.
///
/// # Example
///
/// ```rust
/// use pwn_arena::{Arena, ArenaIndex, ArenaDelete, ArenaResult};
///
/// #[derive(Clone, Copy)]
/// enum Tree {
///     Leaf(i32),
///     Branch(ArenaIndex, ArenaIndex),
/// }
///
/// impl ArenaDelete<Tree, 100> for Tree {
///     fn delete_recursive(&self, arena: &Arena<Tree, 100>) -> ArenaResult<()> {
///         match *self {
///             Tree::Leaf(_) => Ok(()),
///             Tree::Branch(left, right) => {
///                 arena.delete_recursive(left)?;
///                 arena.delete_recursive(right)?;
///                 Ok(())
///             }
///         }
///     }
/// }
/// ```
pub trait ArenaDelete<T: Copy, const N: usize> {
    /// Recursively delete this value and any children from the arena.
    fn delete_recursive(&self, arena: &Arena<T, N>) -> ArenaResult<()>;
}

impl<T: Copy, const N: usize> Arena<T, N> {
    /// Delete a value and recursively delete any children.
    ///
    /// This requires `T: ArenaDelete<T, N>`.
    pub fn delete_recursive(&self, index: ArenaIndex) -> ArenaResult<()>
    where
        T: ArenaDelete<T, N>,
    {
        let value = self.get(index)?;
        value.delete_recursive(self)?;
        self.free(index)?;
        Ok(())
    }
}

// ============================================================================
// Copy Support
// ============================================================================

/// Trait for types that can be deep-copied within the arena.
///
/// Implement this for types containing `ArenaIndex` fields that need
/// to recursively copy their children.
///
/// # Example
///
/// ```rust
/// use pwn_arena::{Arena, ArenaIndex, ArenaCopy, ArenaResult};
///
/// #[derive(Clone, Copy)]
/// enum Tree {
///     Leaf(i32),
///     Branch(ArenaIndex, ArenaIndex),
/// }
///
/// impl ArenaCopy<Tree, 100> for Tree {
///     fn copy_deep(&self, arena: &Arena<Tree, 100>) -> ArenaResult<Tree> {
///         match *self {
///             Tree::Leaf(n) => Ok(Tree::Leaf(n)),
///             Tree::Branch(left, right) => {
///                 let new_left = arena.copy_deep(left)?;
///                 let new_right = arena.copy_deep(right)?;
///                 Ok(Tree::Branch(new_left, new_right))
///             }
///         }
///     }
/// }
/// ```
pub trait ArenaCopy<T: Copy, const N: usize> {
    /// Create a deep copy of this value in the arena.
    fn copy_deep(&self, arena: &Arena<T, N>) -> ArenaResult<T>;
}

impl<T: Copy, const N: usize> Arena<T, N> {
    /// Create a deep copy of a value and its children.
    ///
    /// This requires `T: ArenaCopy<T, N>`.
    pub fn copy_deep(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex>
    where
        T: ArenaCopy<T, N>,
    {
        let value = self.get(index)?;
        let copied = value.copy_deep(self)?;
        self.alloc(copied)
    }
}

// ============================================================================
// Garbage Collection Support
// ============================================================================

/// Trait for types that can be traced by the garbage collector.
///
/// Implement this for types that contain `ArenaIndex` fields. The GC will
/// call `trace` to discover all reachable objects starting from the roots.
///
/// # Example
///
/// ```rust
/// use pwn_arena::{Arena, ArenaIndex, Trace};
///
/// #[derive(Clone, Copy)]
/// enum Tree {
///     Leaf(i32),
///     Branch(ArenaIndex, ArenaIndex),
/// }
///
/// impl<const N: usize> Trace<Tree, N> for Tree {
///     fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
///         match *self {
///             Tree::Leaf(_) => {} // No references to trace
///             Tree::Branch(left, right) => {
///                 tracer(left);
///                 tracer(right);
///             }
///         }
///     }
/// }
///
/// let arena: Arena<Tree, 100> = Arena::new(Tree::Leaf(0));
///
/// // Build a tree
/// let leaf1 = arena.alloc(Tree::Leaf(1)).unwrap();
/// let leaf2 = arena.alloc(Tree::Leaf(2)).unwrap();
/// let root = arena.alloc(Tree::Branch(leaf1, leaf2)).unwrap();
///
/// // Also allocate some garbage (unreachable nodes)
/// let garbage1 = arena.alloc(Tree::Leaf(999)).unwrap();
/// let garbage2 = arena.alloc(Tree::Leaf(888)).unwrap();
///
/// assert_eq!(arena.len(), 5);
///
/// // Collect garbage, keeping only objects reachable from `root`
/// let stats = arena.collect_garbage(&[root]);
///
/// assert_eq!(stats.collected, 2); // garbage1 and garbage2 were freed
/// assert_eq!(arena.len(), 3);     // root, leaf1, leaf2 remain
/// ```
pub trait Trace<T: Copy, const N: usize> {
    /// Trace all `ArenaIndex` references contained in this value.
    ///
    /// Call `tracer` once for each `ArenaIndex` field in this value.
    /// The GC uses this to discover the object graph.
    fn trace<F: FnMut(ArenaIndex)>(&self, tracer: F);
}

/// Statistics returned by garbage collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GcStats {
    /// Number of objects that were marked as reachable.
    pub marked: usize,

    /// Number of objects that were collected (freed).
    pub collected: usize,

    /// Number of objects that existed before collection.
    pub total_before: usize,
}

impl GcStats {
    /// Check if any garbage was collected.
    #[inline]
    pub const fn did_collect(&self) -> bool {
        self.collected > 0
    }

    /// Get the number of objects remaining after collection.
    #[inline]
    pub const fn remaining(&self) -> usize {
        self.total_before - self.collected
    }

    /// Get the collection ratio (0.0 to 1.0).
    ///
    /// Returns 0.0 if no objects existed before collection.
    pub fn collection_ratio(&self) -> f32 {
        if self.total_before == 0 {
            0.0
        } else {
            self.collected as f32 / self.total_before as f32
        }
    }

    /// Get the survival ratio (0.0 to 1.0).
    ///
    /// Returns 1.0 if no objects existed before collection.
    pub fn survival_ratio(&self) -> f32 {
        if self.total_before == 0 {
            1.0
        } else {
            self.marked as f32 / self.total_before as f32
        }
    }
}

impl<T: Copy, const N: usize> Arena<T, N> {
    /// Perform mark-and-sweep garbage collection.
    ///
    /// Starting from the given `roots`, marks all reachable objects by
    /// following `ArenaIndex` references (via the `Trace` trait), then
    /// frees all unmarked (unreachable) objects.
    ///
    /// # Algorithm
    ///
    /// 1. **Mark phase**: Starting from roots, recursively mark all reachable
    ///    objects. Handles cycles correctly by checking if already marked.
    /// 2. **Sweep phase**: Iterate through all slots and free any that are
    ///    allocated but not marked.
    ///
    /// # Returns
    ///
    /// Returns `GcStats` with information about what was collected.
    ///
    /// # Complexity
    ///
    /// - Time: O(reachable + N) where N is arena capacity
    /// - Space: O(N) for the mark bitmap
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::{Arena, ArenaIndex, Trace};
    ///
    /// #[derive(Clone, Copy)]
    /// struct Node {
    ///     value: i32,
    ///     next: Option<ArenaIndex>,
    /// }
    ///
    /// impl<const N: usize> Trace<Node, N> for Node {
    ///     fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
    ///         if let Some(next) = self.next {
    ///             tracer(next);
    ///         }
    ///     }
    /// }
    ///
    /// let arena: Arena<Node, 10> = Arena::new(Node { value: 0, next: None });
    ///
    /// // Create a linked list: root -> n1 -> n2
    /// let n2 = arena.alloc(Node { value: 3, next: None }).unwrap();
    /// let n1 = arena.alloc(Node { value: 2, next: Some(n2) }).unwrap();
    /// let root = arena.alloc(Node { value: 1, next: Some(n1) }).unwrap();
    ///
    /// // Create some garbage
    /// let _garbage = arena.alloc(Node { value: -1, next: None }).unwrap();
    ///
    /// // Collect with root as the only GC root
    /// let stats = arena.collect_garbage(&[root]);
    ///
    /// assert_eq!(stats.collected, 1);
    /// assert_eq!(arena.len(), 3);
    /// ```
    pub fn collect_garbage(&self, roots: &[ArenaIndex]) -> GcStats
    where
        T: Trace<T, N>,
    {
        let total_before = self.len();

        // If GC is disabled, return immediately without collecting
        if !self.is_gc_enabled() {
            return GcStats {
                marked: 0,
                collected: 0,
                total_before,
            };
        }

        // Mark phase: track which slots are reachable
        // Using fixed-size arrays instead of Vec for no-alloc compatibility
        let mut marked = [false; N];

        // Fixed-size mark stack (worst case: all N slots could be on stack)
        let mut mark_stack = [0usize; N];
        let mut stack_len = 0usize;

        // Initialize stack with valid roots
        for &root in roots {
            if self.is_allocated(root) {
                let idx = root.raw();
                if idx < N && !marked[idx] {
                    marked[idx] = true;
                    if stack_len < N {
                        mark_stack[stack_len] = idx;
                        stack_len += 1;
                    }
                }
            }
        }

        // Process mark stack (depth-first traversal)
        while stack_len > 0 {
            stack_len -= 1;
            let current_idx = mark_stack[stack_len];

            let slots = self.slots.borrow();

            if let Slot::Occupied { value } = slots[current_idx] {
                drop(slots);

                // Collect ALL children by processing in batches of 16
                // This ensures we never silently drop children
                let mut batch = [0usize; 16];
                let mut batch_count = 0usize;
                let mut overflow_detected = false;

                value.trace(|child_index| {
                    let idx = child_index.raw();
                    if idx < N && !marked[idx] {
                        if batch_count < 16 {
                            batch[batch_count] = idx;
                            batch_count += 1;
                        } else {
                            overflow_detected = true;
                        }
                    }
                });

                // Process the batch
                for i in 0..batch_count {
                    let idx = batch[i];
                    if !marked[idx] && self.is_allocated(ArenaIndex::new(idx, self.generations.borrow()[idx])) {
                        marked[idx] = true;
                        if stack_len < N {
                            mark_stack[stack_len] = idx;
                            stack_len += 1;
                        }
                    }
                }

                // If there were more than 16 children, re-trace to get the rest
                // This is rare but ensures correctness
                if overflow_detected {
                    let slots = self.slots.borrow();
                    if let Slot::Occupied { value } = slots[current_idx] {
                        drop(slots);

                        value.trace(|child_index| {
                            let idx = child_index.raw();
                            if idx < N && !marked[idx] {
                                if self.is_allocated(ArenaIndex::new(idx, self.generations.borrow()[idx])) {
                                    marked[idx] = true;
                                    if stack_len < N {
                                        mark_stack[stack_len] = idx;
                                        stack_len += 1;
                                    }
                                }
                            }
                        });
                    }
                }
            }
        }

        let marked_count = marked.iter().filter(|&&m| m).count();

        // Sweep phase: free all unmarked but allocated slots
        // Collect indices to free into fixed-size array
        let mut to_free = [0usize; N];
        let mut to_free_len = 0usize;

        {
            let slots = self.slots.borrow();

            for idx in 0..N {
                if let Slot::Occupied { .. } = slots[idx] {
                    if !marked[idx] {
                        // This slot is allocated but not reachable - garbage!
                        to_free[to_free_len] = idx;
                        to_free_len += 1;
                    }
                }
            }
        }

        // Free the garbage
        let mut collected = 0;
        for i in 0..to_free_len {
            let idx = to_free[i];
            let generation = self.generations.borrow()[idx];
            if self.free(ArenaIndex::new(idx, generation)).is_ok() {
                collected += 1;
            }
        }

        GcStats {
            marked: marked_count,
            collected,
            total_before,
        }
    }

    /// Force garbage collection even if GC is disabled.
    ///
    /// This ignores the `gc_enabled` flag and always performs collection.
    /// Useful when you need to collect garbage regardless of the current
    /// GC state.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::{Arena, ArenaIndex, Trace};
    ///
    /// #[derive(Clone, Copy)]
    /// struct Leaf(i32);
    ///
    /// impl<const N: usize> Trace<Leaf, N> for Leaf {
    ///     fn trace<F: FnMut(ArenaIndex)>(&self, _tracer: F) {}
    /// }
    ///
    /// let arena: Arena<Leaf, 10> = Arena::new(Leaf(0));
    /// arena.set_gc_enabled(false);
    ///
    /// arena.alloc(Leaf(1)).unwrap();
    /// arena.alloc(Leaf(2)).unwrap(); // garbage
    ///
    /// let root = arena.alloc(Leaf(3)).unwrap();
    ///
    /// // This will NOT collect (GC disabled)
    /// let stats = arena.collect_garbage(&[root]);
    /// assert_eq!(stats.collected, 0);
    ///
    /// // This WILL collect (forced)
    /// let stats = arena.collect_garbage_forced(&[root]);
    /// assert_eq!(stats.collected, 2);
    /// ```
    pub fn collect_garbage_forced(&self, roots: &[ArenaIndex]) -> GcStats
    where
        T: Trace<T, N>,
    {
        let was_enabled = self.is_gc_enabled();
        self.set_gc_enabled(true);
        let result = self.collect_garbage(roots);
        self.set_gc_enabled(was_enabled);
        result
    }

    /// Perform garbage collection with multiple root sets.
    ///
    /// This iterates through all provided root sets and marks objects
    /// reachable from any of them.
    ///
    /// Respects the `gc_enabled` flag - use [`Arena::collect_garbage_multi_forced`]
    /// to ignore the flag.
    ///
    /// # Note
    ///
    /// For no-alloc compatibility, this method iterates through root sets
    /// sequentially rather than flattening them. This has the same effect
    /// but uses constant stack space.
    pub fn collect_garbage_multi(&self, root_sets: &[&[ArenaIndex]]) -> GcStats
    where
        T: Trace<T, N>,
    {
        let total_before = self.len();

        // If GC is disabled, return immediately without collecting
        if !self.is_gc_enabled() {
            return GcStats {
                marked: 0,
                collected: 0,
                total_before,
            };
        }

        // Mark phase with multiple root sets
        let mut marked = [false; N];
        let mut mark_stack = [0usize; N];
        let mut stack_len = 0usize;

        // Initialize stack with valid roots from all root sets
        for root_set in root_sets {
            for &root in *root_set {
                if self.is_allocated(root) {
                    let idx = root.raw();
                    if idx < N && !marked[idx] {
                        marked[idx] = true;
                        if stack_len < N {
                            mark_stack[stack_len] = idx;
                            stack_len += 1;
                        }
                    }
                }
            }
        }

        // Process mark stack (same logic as collect_garbage with overflow handling)
        while stack_len > 0 {
            stack_len -= 1;
            let current_idx = mark_stack[stack_len];

            let slots = self.slots.borrow();

            if let Slot::Occupied { value } = slots[current_idx] {
                drop(slots);

                let mut batch = [0usize; 16];
                let mut batch_count = 0usize;
                let mut overflow_detected = false;

                value.trace(|child_index| {
                    let idx = child_index.raw();
                    if idx < N && !marked[idx] {
                        if batch_count < 16 {
                            batch[batch_count] = idx;
                            batch_count += 1;
                        } else {
                            overflow_detected = true;
                        }
                    }
                });

                for i in 0..batch_count {
                    let idx = batch[i];
                    if !marked[idx] && self.is_allocated(ArenaIndex::new(idx, self.generations.borrow()[idx])) {
                        marked[idx] = true;
                        if stack_len < N {
                            mark_stack[stack_len] = idx;
                            stack_len += 1;
                        }
                    }
                }

                // Handle overflow case
                if overflow_detected {
                    let slots = self.slots.borrow();
                    if let Slot::Occupied { value } = slots[current_idx] {
                        drop(slots);

                        value.trace(|child_index| {
                            let idx = child_index.raw();
                            if idx < N && !marked[idx] {
                                if self.is_allocated(ArenaIndex::new(idx, self.generations.borrow()[idx])) {
                                    marked[idx] = true;
                                    if stack_len < N {
                                        mark_stack[stack_len] = idx;
                                        stack_len += 1;
                                    }
                                }
                            }
                        });
                    }
                }
            }
        }

        let marked_count = marked.iter().filter(|&&m| m).count();

        // Sweep phase
        let mut to_free = [0usize; N];
        let mut to_free_len = 0usize;

        {
            let slots = self.slots.borrow();
            for idx in 0..N {
                if let Slot::Occupied { .. } = slots[idx] {
                    if !marked[idx] {
                        to_free[to_free_len] = idx;
                        to_free_len += 1;
                    }
                }
            }
        }

        let mut collected = 0;
        for i in 0..to_free_len {
            let idx = to_free[i];
            let generation = self.generations.borrow()[idx];
            if self.free(ArenaIndex::new(idx, generation)).is_ok() {
                collected += 1;
            }
        }

        GcStats {
            marked: marked_count,
            collected,
            total_before,
        }
    }

    /// Force garbage collection with multiple root sets, ignoring the `gc_enabled` flag.
    pub fn collect_garbage_multi_forced(&self, root_sets: &[&[ArenaIndex]]) -> GcStats
    where
        T: Trace<T, N>,
    {
        let was_enabled = self.is_gc_enabled();
        self.set_gc_enabled(true);
        let result = self.collect_garbage_multi(root_sets);
        self.set_gc_enabled(was_enabled);
        result
    }

    /// Allocate a value, running GC first if the arena is full.
    ///
    /// If allocation fails due to `OutOfMemory`, this method runs garbage
    /// collection with the provided roots and retries the allocation.
    ///
    /// # Errors
    ///
    /// Returns `OutOfMemory` if allocation still fails after GC.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::{Arena, ArenaIndex, Trace};
    ///
    /// #[derive(Clone, Copy)]
    /// struct Node(i32);
    ///
    /// impl<const N: usize> Trace<Node, N> for Node {
    ///     fn trace<F: FnMut(ArenaIndex)>(&self, _: F) {}
    /// }
    ///
    /// let arena: Arena<Node, 3> = Arena::new(Node(0));
    ///
    /// let root = arena.alloc(Node(1)).unwrap();
    /// arena.alloc(Node(2)).unwrap(); // garbage
    /// arena.alloc(Node(3)).unwrap(); // garbage
    ///
    /// // Arena is full, but alloc_or_gc will collect garbage first
    /// let new_idx = arena.alloc_or_gc(Node(4), &[root]).unwrap();
    /// assert_eq!(arena.len(), 2); // root + new_idx
    /// ```
    pub fn alloc_or_gc(&self, value: T, roots: &[ArenaIndex]) -> ArenaResult<ArenaIndex>
    where
        T: Trace<T, N>,
    {
        match self.alloc(value) {
            Ok(idx) => Ok(idx),
            Err(ArenaError::OutOfMemory) => {
                // Run GC and retry
                self.collect_garbage_forced(roots);
                self.alloc(value)
            }
            Err(e) => Err(e),
        }
    }
}

// ============================================================================
// Iterator
// ============================================================================

/// Iterator over allocated cells in the arena.
pub struct ArenaIterator<'a, T: Copy, const N: usize> {
    arena: &'a Arena<T, N>,
    current: usize,
}

impl<'a, T: Copy, const N: usize> Iterator for ArenaIterator<'a, T, N> {
    type Item = (ArenaIndex, T);

    fn next(&mut self) -> Option<Self::Item> {
        let slots = self.arena.slots.borrow();
        let generations = self.arena.generations.borrow();

        while self.current < N {
            let idx = self.current;
            self.current += 1;

            if let Slot::Occupied { value } = slots[idx] {
                let generation = generations[idx];
                return Some((ArenaIndex::new(idx, generation), value));
            }
        }

        None
    }
}

// ============================================================================
// Statistics
// ============================================================================

/// Statistics about arena usage.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ArenaStats {
    /// Total capacity of the arena.
    pub capacity: usize,

    /// Number of currently allocated cells.
    pub allocated: usize,

    /// Number of free cells.
    pub free: usize,

    /// Fragmentation ratio (0.0 = not fragmented, 1.0 = highly fragmented).
    pub fragmentation: f32,
}

impl ArenaStats {
    /// Get usage as a percentage (0-100).
    pub fn usage_percent(&self) -> f32 {
        if self.capacity == 0 {
            0.0
        } else {
            (self.allocated as f32 / self.capacity as f32) * 100.0
        }
    }

    /// Get free space as a percentage (0-100).
    pub fn free_percent(&self) -> f32 {
        100.0 - self.usage_percent()
    }

    /// Check if the arena is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.allocated == 0
    }

    /// Check if the arena is full.
    #[inline]
    pub const fn is_full(&self) -> bool {
        self.free == 0
    }

    /// Check if fragmentation is above a threshold.
    pub fn is_fragmented(&self, threshold: f32) -> bool {
        self.fragmentation > threshold
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_allocation() {
        let arena: Arena<i32, 10> = Arena::new(0);

        let idx1 = arena.alloc(42).unwrap();
        let idx2 = arena.alloc(43).unwrap();

        assert_eq!(arena.get(idx1).unwrap(), 42);
        assert_eq!(arena.get(idx2).unwrap(), 43);
        assert_eq!(arena.len(), 2);
    }

    #[test]
    fn test_free_and_reuse() {
        let arena: Arena<i32, 10> = Arena::new(0);

        let idx1 = arena.alloc(42).unwrap();
        assert_eq!(arena.len(), 1);

        arena.free(idx1).unwrap();
        assert_eq!(arena.len(), 0);

        let idx2 = arena.alloc(43).unwrap();
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.get(idx2).unwrap(), 43);
    }

    #[test]
    fn test_out_of_memory() {
        let arena: Arena<i32, 3> = Arena::new(0);

        assert!(arena.alloc(1).is_ok());
        assert!(arena.alloc(2).is_ok());
        assert!(arena.alloc(3).is_ok());
        assert_eq!(arena.alloc(4), Err(ArenaError::OutOfMemory));
    }

    #[test]
    fn test_invalid_index() {
        let arena: Arena<i32, 10> = Arena::new(0);

        let idx = arena.alloc(42).unwrap();
        arena.free(idx).unwrap();

        // After freeing, the generation increments, so we get GenerationMismatch
        assert_eq!(arena.get(idx), Err(ArenaError::GenerationMismatch));
    }

    #[test]
    fn test_generational_indices_aba_protection() {
        let arena: Arena<i32, 10> = Arena::new(0);

        // Allocate and free a slot
        let old_idx = arena.alloc(42).unwrap();
        arena.free(old_idx).unwrap();

        // Allocate a new value in the same slot
        let new_idx = arena.alloc(999).unwrap();

        // The old index should now be invalid (ABA problem prevented)
        assert_eq!(arena.get(old_idx), Err(ArenaError::GenerationMismatch));
        assert_eq!(arena.free(old_idx), Err(ArenaError::GenerationMismatch));

        // The new index should work fine
        assert_eq!(arena.get(new_idx).unwrap(), 999);

        // Verify they point to the same raw slot but different generations
        assert_eq!(old_idx.raw(), new_idx.raw());
        assert_ne!(old_idx.generation(), new_idx.generation());
    }

    #[test]
    fn test_free_list_o1_allocation() {
        let arena: Arena<i32, 5> = Arena::new(0);

        // Allocate all slots
        let idx0 = arena.alloc(0).unwrap();
        let idx1 = arena.alloc(1).unwrap();
        let idx2 = arena.alloc(2).unwrap();
        let idx3 = arena.alloc(3).unwrap();
        let idx4 = arena.alloc(4).unwrap();

        assert!(arena.is_full());

        // Free some slots in non-sequential order
        arena.free(idx2).unwrap();
        arena.free(idx0).unwrap();
        arena.free(idx4).unwrap();

        assert_eq!(arena.len(), 2);
        assert_eq!(arena.available(), 3);

        // Allocate again - should reuse freed slots (LIFO order from free list)
        let new1 = arena.alloc(100).unwrap();
        let new2 = arena.alloc(200).unwrap();
        let new3 = arena.alloc(300).unwrap();

        assert_eq!(arena.len(), 5);

        // Verify the new values are accessible
        assert_eq!(arena.get(new1).unwrap(), 100);
        assert_eq!(arena.get(new2).unwrap(), 200);
        assert_eq!(arena.get(new3).unwrap(), 300);

        // Original unfreed indices should still work
        assert_eq!(arena.get(idx1).unwrap(), 1);
        assert_eq!(arena.get(idx3).unwrap(), 3);
    }

    #[test]
    fn test_clear_invalidates_all_indices() {
        let arena: Arena<i32, 10> = Arena::new(0);

        let idx1 = arena.alloc(1).unwrap();
        let idx2 = arena.alloc(2).unwrap();
        let idx3 = arena.alloc(3).unwrap();

        arena.clear();

        // All old indices should be invalid
        assert_eq!(arena.get(idx1), Err(ArenaError::GenerationMismatch));
        assert_eq!(arena.get(idx2), Err(ArenaError::GenerationMismatch));
        assert_eq!(arena.get(idx3), Err(ArenaError::GenerationMismatch));

        // New allocations should work
        let new_idx = arena.alloc(42).unwrap();
        assert_eq!(arena.get(new_idx).unwrap(), 42);
    }

    #[test]
    fn test_stats() {
        let arena: Arena<i32, 10> = Arena::new(0);

        arena.alloc(1).unwrap();
        arena.alloc(2).unwrap();

        let stats = arena.stats();
        assert_eq!(stats.capacity, 10);
        assert_eq!(stats.allocated, 2);
        assert_eq!(stats.free, 8);
        assert_eq!(stats.usage_percent(), 20.0);
    }

    #[test]
    fn test_clear() {
        let arena: Arena<i32, 10> = Arena::new(0);

        arena.alloc(1).unwrap();
        arena.alloc(2).unwrap();
        arena.alloc(3).unwrap();

        assert_eq!(arena.len(), 3);

        arena.clear();

        assert_eq!(arena.len(), 0);
        assert!(arena.is_empty());
    }

    // Example of recursive deletion
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Tree {
        Leaf(i32),
        Branch(ArenaIndex, ArenaIndex),
    }

    impl ArenaDelete<Tree, 100> for Tree {
        fn delete_recursive(&self, arena: &Arena<Tree, 100>) -> ArenaResult<()> {
            match *self {
                Tree::Leaf(_) => Ok(()),
                Tree::Branch(left, right) => {
                    arena.delete_recursive(left)?;
                    arena.delete_recursive(right)?;
                    Ok(())
                }
            }
        }
    }

    impl ArenaCopy<Tree, 100> for Tree {
        fn copy_deep(&self, arena: &Arena<Tree, 100>) -> ArenaResult<Tree> {
            match *self {
                Tree::Leaf(n) => Ok(Tree::Leaf(n)),
                Tree::Branch(left, right) => {
                    let new_left = arena.copy_deep(left)?;
                    let new_right = arena.copy_deep(right)?;
                    Ok(Tree::Branch(new_left, new_right))
                }
            }
        }
    }

    #[test]
    fn test_recursive_delete() {
        let arena: Arena<Tree, 100> = Arena::new(Tree::Leaf(0));

        let left = arena.alloc(Tree::Leaf(1)).unwrap();
        let right = arena.alloc(Tree::Leaf(2)).unwrap();
        let root = arena.alloc(Tree::Branch(left, right)).unwrap();

        assert_eq!(arena.len(), 3);

        arena.delete_recursive(root).unwrap();

        assert_eq!(arena.len(), 0);
    }

    #[test]
    fn test_deep_copy() {
        let arena: Arena<Tree, 100> = Arena::new(Tree::Leaf(0));

        let left = arena.alloc(Tree::Leaf(1)).unwrap();
        let right = arena.alloc(Tree::Leaf(2)).unwrap();
        let root = arena.alloc(Tree::Branch(left, right)).unwrap();

        let copied_root = arena.copy_deep(root).unwrap();

        // Should have 6 nodes total (3 original + 3 copied)
        assert_eq!(arena.len(), 6);

        // Verify structure is copied
        if let Tree::Branch(cl, cr) = arena.get(copied_root).unwrap() {
            assert_eq!(arena.get(cl).unwrap(), Tree::Leaf(1));
            assert_eq!(arena.get(cr).unwrap(), Tree::Leaf(2));
        } else {
            panic!("Expected branch");
        }
    }

    #[test]
    fn test_set() {
        let arena: Arena<i32, 10> = Arena::new(0);

        let idx = arena.alloc(42).unwrap();
        assert_eq!(arena.get(idx).unwrap(), 42);

        arena.set(idx, 100).unwrap();
        assert_eq!(arena.get(idx).unwrap(), 100);
    }
}
