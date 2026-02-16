//! Core arena allocator implementation.
//!
//! This module contains the main [`Arena`] struct and its core operations.
//!
//! Note: The `impl_get_contiguous!` and `impl_set_contiguous!` macros have been
//! moved to `src/macros.rs`.

use core::cell::Cell;

use crate::{ArenaIndex, ArenaError, ArenaResult, ArenaStats, ArenaDelete, ArenaCopy};
use crate::types::{Slot, FREE_LIST_END};
use crate::iter::ArenaIterator;
use crate::macros::{impl_get_contiguous, impl_set_contiguous};

/// Fixed-size arena allocator with O(1) allocation.
///
/// # Type Parameters
///
/// - `T`: The type of values stored (must be `Copy` for array initialization)
/// - `N`: Maximum number of cells (const generic)
///
/// # Memory Layout
///
/// - `slots`: Array of `Cell<Slot<T>>` (either free with next pointer, or occupied with value)
/// - `free_head`: Head of the free list
/// - `len`: Number of currently allocated slots
/// - `gc_enabled`: Whether garbage collection is enabled
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
///
/// # Performance
///
/// Uses `Cell` instead of `RefCell` for interior mutability. This eliminates
/// runtime borrow checking overhead and the risk of borrow panics, while
/// still maintaining safe Rust guarantees.
pub struct Arena<T: Copy, const N: usize> {
    pub(crate) slots: [Cell<Slot<T>>; N],
    pub(crate) free_head: Cell<usize>,
    pub(crate) len: Cell<usize>,
    pub(crate) gc_enabled: Cell<bool>,
}

impl<T: Copy, const N: usize> Arena<T, N> {
    /// Create a new arena.
    ///
    /// All slots start as free, linked together in a free list.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_arena::Arena;
    ///
    /// let arena: Arena<isize, 100> = Arena::new(0);
    /// ```
    pub fn new(_default_value: T) -> Self {
        // Initialize all slots as free, linked together
        // Slot 0 -> 1 -> 2 -> ... -> N-1 -> FREE_LIST_END
        let slots: [Cell<Slot<T>>; N] = core::array::from_fn(|i| {
            Cell::new(Slot::Free {
                next_free: if i + 1 < N { i + 1 } else { FREE_LIST_END },
            })
        });

        Arena {
            slots,
            free_head: Cell::new(if N > 0 { 0 } else { FREE_LIST_END }),
            len: Cell::new(0),
            gc_enabled: Cell::new(true), // GC enabled by default
        }
    }

    /// Check if garbage collection is enabled.
    ///
    /// When disabled, [`Arena::collect_garbage`] returns immediately without
    /// performing any collection.
    pub fn is_gc_enabled(&self) -> bool {
        self.gc_enabled.get()
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
    /// use grift_arena::Arena;
    ///
    /// let arena: Arena<isize, 100> = Arena::new(0);
    ///
    /// // Disable GC for a batch operation
    /// arena.set_gc_enabled(false);
    ///
    /// // ... perform many allocations ...
    ///
    /// // Re-enable and collect
    /// arena.set_gc_enabled(true);
    /// ```
    pub fn set_gc_enabled(&self, enabled: bool) {
        self.gc_enabled.set(enabled);
    }

    /// Temporarily disable GC, run a closure, then restore the previous state.
    ///
    /// This is useful for critical sections where GC should not run.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_arena::Arena;
    ///
    /// let arena: Arena<isize, 100> = Arena::new(0);
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
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Get the number of currently allocated cells.
    pub fn len(&self) -> usize {
        self.len.get()
    }

    /// Check if the arena is empty (no allocated cells).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the arena is full (all cells allocated).
    pub fn is_full(&self) -> bool {
        self.len() == N
    }

    /// Get the number of free cells.
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
    /// use grift_arena::Arena;
    ///
    /// let arena: Arena<isize, 10> = Arena::new(0);
    /// let idx = arena.alloc(42).unwrap();
    /// assert_eq!(arena.get(idx).unwrap(), 42);
    /// ```
    pub fn alloc(&self, value: T) -> ArenaResult<ArenaIndex> {
        let free_head = self.free_head.get();

        // Check if there's a free slot
        if free_head == FREE_LIST_END {
            return Err(ArenaError::OutOfMemory);
        }

        let idx = free_head;

        // Pop from free list
        let next_free = match self.slots[idx].get() {
            Slot::Free { next_free } => next_free,
            Slot::Occupied { .. } => unreachable!("free_head pointed to occupied slot"),
        };

        // Mark as occupied
        self.slots[idx].set(Slot::Occupied { value });
        self.free_head.set(next_free);

        // Increment allocated count
        self.len.set(self.len.get() + 1);

        Ok(ArenaIndex::new(idx))
    }

    /// Check that an index is valid (in bounds).
    /// Returns the slot index if valid.
    /// Note: Does NOT check if the slot is occupied - caller must verify.
    fn check_bounds(&self, index: ArenaIndex) -> ArenaResult<usize> {
        let idx = index.raw();

        if idx >= N {
            return Err(ArenaError::InvalidIndex);
        }

        Ok(idx)
    }

    /// Validate an index and return the slot index if valid.
    fn validate_index(&self, index: ArenaIndex) -> ArenaResult<usize> {
        let idx = self.check_bounds(index)?;

        // Check if slot is occupied
        match self.slots[idx].get() {
            Slot::Occupied { .. } => Ok(idx),
            Slot::Free { .. } => Err(ArenaError::InvalidIndex),
        }
    }

    /// Get a copy of the value at the given index.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::InvalidIndex` if the index is out of bounds or not allocated.
    pub fn get(&self, index: ArenaIndex) -> ArenaResult<T> {
        let idx = self.check_bounds(index)?;

        // Check if slot is occupied and get value
        match self.slots[idx].get() {
            Slot::Occupied { value } => Ok(value),
            Slot::Free { .. } => Err(ArenaError::InvalidIndex),
        }
    }

    /// Set the value at the given index.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::InvalidIndex` if the index is out of bounds or not allocated.
    pub fn set(&self, index: ArenaIndex, value: T) -> ArenaResult<()> {
        let idx = self.validate_index(index)?;

        self.slots[idx].set(Slot::Occupied { value });
        Ok(())
    }

    /// Modify a value in place using a closure.
    ///
    /// With Cell-based storage, this is implemented as get + modify + set.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::InvalidIndex` if the index is out of bounds or not allocated.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_arena::Arena;
    ///
    /// let arena: Arena<isize, 10> = Arena::new(0);
    /// let idx = arena.alloc(42).unwrap();
    ///
    /// arena.modify(idx, |v| *v += 10).unwrap();
    /// assert_eq!(arena.get(idx).unwrap(), 52);
    /// ```
    pub fn modify<F>(&self, index: ArenaIndex, f: F) -> ArenaResult<()>
    where
        F: FnOnce(&mut T),
    {
        let idx = self.validate_index(index)?;

        if let Slot::Occupied { mut value } = self.slots[idx].get() {
            f(&mut value);
            self.slots[idx].set(Slot::Occupied { value });
            Ok(())
        } else {
            Err(ArenaError::InvalidIndex)
        }
    }

    /// Get a value, returning `None` instead of an error if invalid.
    ///
    /// This is a convenience method for cases where you expect the index
    /// might be invalid and want to handle it with `Option` instead of `Result`.
    #[must_use]
    pub fn try_get(&self, index: ArenaIndex) -> Option<T> {
        self.get(index).ok()
    }
    
    // ========================================================================
    // Batch read/write operations for contiguous values
    // Generated by impl_get_contiguous! and impl_set_contiguous! macros
    // ========================================================================
    
    impl_get_contiguous!(get_contiguous2, [0 => a, 1 => b]);
    impl_get_contiguous!(get_contiguous3, [0 => a, 1 => b, 2 => c]);
    impl_get_contiguous!(get_contiguous4, [0 => a, 1 => b, 2 => c, 3 => d]);
    impl_get_contiguous!(get_contiguous5, [0 => a, 1 => b, 2 => c, 3 => d, 4 => e]);
    impl_get_contiguous!(get_contiguous6, [0 => a, 1 => b, 2 => c, 3 => d, 4 => e, 5 => f]);
    impl_get_contiguous!(get_contiguous7, [0 => a, 1 => b, 2 => c, 3 => d, 4 => e, 5 => f, 6 => g]);
    
    impl_set_contiguous!(set_contiguous2, [0 => a, 1 => b]);
    impl_set_contiguous!(set_contiguous3, [0 => a, 1 => b, 2 => c]);
    impl_set_contiguous!(set_contiguous4, [0 => a, 1 => b, 2 => c, 3 => d]);
    impl_set_contiguous!(set_contiguous5, [0 => a, 1 => b, 2 => c, 3 => d, 4 => e]);
    impl_set_contiguous!(set_contiguous6, [0 => a, 1 => b, 2 => c, 3 => d, 4 => e, 5 => f]);
    impl_set_contiguous!(set_contiguous7, [0 => a, 1 => b, 2 => c, 3 => d, 4 => e, 5 => f, 6 => g]);

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

        let (val_a, val_b) = match (self.slots[idx_a].get(), self.slots[idx_b].get()) {
            (Slot::Occupied { value: va }, Slot::Occupied { value: vb }) => (va, vb),
            _ => return Err(ArenaError::InvalidIndex),
        };

        self.slots[idx_a].set(Slot::Occupied { value: val_b });
        self.slots[idx_b].set(Slot::Occupied { value: val_a });

        Ok(())
    }

    /// Replace the value at an index, returning the old value.
    ///
    /// # Errors
    ///
    /// Returns an error if the index is invalid.
    pub fn replace(&self, index: ArenaIndex, value: T) -> ArenaResult<T> {
        let idx = self.validate_index(index)?;

        match self.slots[idx].get() {
            Slot::Occupied { value: old } => {
                self.slots[idx].set(Slot::Occupied { value });
                Ok(old)
            }
            Slot::Free { .. } => Err(ArenaError::InvalidIndex),
        }
    }

    /// Free a cell, making it available for reuse.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::InvalidIndex` if the index is out of bounds or not allocated.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_arena::Arena;
    ///
    /// let arena: Arena<isize, 10> = Arena::new(0);
    /// let idx = arena.alloc(42).unwrap();
    /// arena.free(idx).unwrap();
    /// assert_eq!(arena.len(), 0);
    /// ```
    pub fn free(&self, index: ArenaIndex) -> ArenaResult<()> {
        let idx = self.validate_index(index)?;

        // Push onto free list
        let free_head = self.free_head.get();
        self.slots[idx].set(Slot::Free { next_free: free_head });
        self.free_head.set(idx);

        // Decrement allocated count
        self.len.set(self.len.get() - 1);

        Ok(())
    }

    /// Check if an index is currently valid (allocated).
    pub fn is_allocated(&self, index: ArenaIndex) -> bool {
        self.validate_index(index).is_ok()
    }

    /// Clear all allocations, making the entire arena available.
    ///
    /// # Warning
    ///
    /// This does not call any destructors. Use with caution.
    pub fn clear(&self) {
        // Rebuild free list
        for i in 0..N {
            self.slots[i].set(Slot::Free {
                next_free: if i + 1 < N { i + 1 } else { FREE_LIST_END },
            });
        }

        self.free_head.set(if N > 0 { 0 } else { FREE_LIST_END });
        self.len.set(0);
    }

    /// Iterate over all allocated indices and values.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_arena::Arena;
    ///
    /// let arena: Arena<isize, 10> = Arena::new(0);
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
        let fragments = (0..N).fold((0u32, false), |(count, was_free), i| {
            let is_free = matches!(self.slots[i].get(), Slot::Free { .. });
            (count + u32::from(is_free && !was_free), is_free)
        }).0;

        if fragments == 0 { 0.0 } else { fragments as f32 / N as f32 }
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
        let free_head = self.free_head.get();
        let len = self.len.get();

        // Count occupied slots
        let occupied_count = (0..N)
            .filter(|&i| matches!(self.slots[i].get(), Slot::Occupied { .. }))
            .count();
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

            match self.slots[current].get() {
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
        for (i, &was_visited) in visited.iter().enumerate().take(N) {
            if matches!(self.slots[i].get(), Slot::Free { .. }) && !was_visited {
                return false; // Free slot not in free list
            }
        }

        true
    }

    /// Check if a slot index is currently occupied.
    ///
    /// This is a low-level debugging method. For normal use, prefer [`is_allocated`].
    pub fn is_slot_occupied(&self, slot_index: usize) -> bool {
        slot_index < N && matches!(self.slots[slot_index].get(), Slot::Occupied { .. })
    }

    /// Get indices of all allocated slots.
    ///
    /// Returns an array with the first `len()` elements being valid indices.
    /// The remaining elements are [`ArenaIndex::NIL`].
    pub fn allocated_indices(&self) -> [ArenaIndex; N] {
        let mut result = [ArenaIndex::NIL; N];
        let mut count = 0;

        for idx in 0..N {
            if let Slot::Occupied { .. } = self.slots[idx].get() {
                result[count] = ArenaIndex::new(idx);
                count += 1;
            }
        }

        result
    }

    /// Apply a function to all allocated values.
    ///
    /// This is useful for bulk reads without the overhead of iteration.
    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(ArenaIndex, &T),
    {
        self.iter().for_each(|(idx, val)| f(idx, &val));
    }

    /// Apply a mutating function to all allocated values.
    pub fn for_each_mut<F>(&self, mut f: F)
    where
        F: FnMut(ArenaIndex, &mut T),
    {
        for idx in 0..N {
            if let Slot::Occupied { mut value } = self.slots[idx].get() {
                f(ArenaIndex::new(idx), &mut value);
                self.slots[idx].set(Slot::Occupied { value });
            }
        }
    }

    /// Count values matching a predicate.
    pub fn count_where<F>(&self, predicate: F) -> usize
    where
        F: Fn(&T) -> bool,
    {
        self.iter().filter(|(_, v)| predicate(v)).count()
    }

    /// Find the first value matching a predicate.
    pub fn find<F>(&self, predicate: F) -> Option<(ArenaIndex, T)>
    where
        F: Fn(&T) -> bool,
    {
        self.iter().find(|(_, v)| predicate(v))
    }

    /// Check if any allocated value matches a predicate.
    pub fn any<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter().any(|(_, v)| predicate(&v))
    }

    /// Check if all allocated values match a predicate.
    ///
    /// Returns `true` if the arena is empty.
    pub fn all<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter().all(|(_, v)| predicate(&v))
    }

    // ========================================================================
    // Contiguous Allocation
    // ========================================================================

    /// Find a contiguous block of `count` free slots.
    /// 
    /// Returns the starting index if found, `None` otherwise.
    /// 
    /// # Algorithm
    /// 
    /// Linear scan through the slots array looking for consecutive free slots.
    /// Uses first-fit strategy for simplicity.
    fn find_contiguous_free_slots(&self, count: usize) -> Option<usize> {
        if count == 0 || count > N {
            return None;
        }

        let mut consecutive = 0;
        let mut start = 0;

        for i in 0..N {
            match self.slots[i].get() {
                Slot::Free { .. } => {
                    if consecutive == 0 {
                        start = i;
                    }
                    consecutive += 1;
                    if consecutive >= count {
                        return Some(start);
                    }
                }
                Slot::Occupied { .. } => {
                    consecutive = 0;
                }
            }
        }

        None
    }

    /// Allocate a contiguous block of `count` slots.
    /// 
    /// Returns the starting `ArenaIndex` if successful. The allocated slots
    /// are consecutive in memory, starting from the returned index.
    /// 
    /// # Use Case
    /// 
    /// This is primarily used for string storage where characters need to be
    /// stored in contiguous memory for efficient access. The first slot
    /// typically stores length metadata, followed by character data.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if `count` is 0.
    /// Returns `ArenaError::OutOfMemory` if no contiguous block is available.
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use grift_arena::Arena;
    /// 
    /// let arena: Arena<isize, 100> = Arena::new(0);
    /// 
    /// // Allocate 5 contiguous slots
    /// let start = arena.alloc_contiguous(5, 0).unwrap();
    /// 
    /// // All slots are consecutive
    /// for i in 0..5 {
    ///     let idx = arena.index_at_offset(start, i).unwrap();
    ///     arena.set(idx, i as isize).unwrap();
    /// }
    /// ```
    pub fn alloc_contiguous(&self, count: usize, default: T) -> ArenaResult<ArenaIndex> {
        if count == 0 {
            return Err(ArenaError::InvalidIndex);
        }

        let start_idx = self.find_contiguous_free_slots(count)
            .ok_or(ArenaError::OutOfMemory)?;
        let end_idx = start_idx + count;

        // Safety: find_contiguous_free_slots guarantees start_idx + count <= N
        debug_assert!(end_idx <= N);

        // Remove all slots in [start_idx, end_idx) from the free list in one pass.
        // This is O(free_list_length) instead of O(count × free_list_length).

        // Skip any head nodes that fall in the range
        let mut head = self.free_head.get();
        while head != FREE_LIST_END && head >= start_idx && head < end_idx {
            if let Slot::Free { next_free } = self.slots[head].get() {
                head = next_free;
            } else {
                break;
            }
        }
        self.free_head.set(head);

        // Walk the rest of the free list, splicing out nodes in the range
        let mut current = head;
        while current != FREE_LIST_END {
            if let Slot::Free { next_free } = self.slots[current].get() {
                if next_free != FREE_LIST_END && next_free >= start_idx && next_free < end_idx {
                    // Skip over all consecutive nodes in the range
                    let mut skip = next_free;
                    while skip != FREE_LIST_END && skip >= start_idx && skip < end_idx {
                        if let Slot::Free { next_free: inner_next } = self.slots[skip].get() {
                            skip = inner_next;
                        } else {
                            break;
                        }
                    }
                    self.slots[current].set(Slot::Free { next_free: skip });
                    current = skip;
                } else {
                    current = next_free;
                }
            } else {
                break;
            }
        }

        // Mark all slots in the range as occupied
        for i in start_idx..end_idx {
            self.slots[i].set(Slot::Occupied { value: default });
        }

        // Update length
        self.len.set(self.len.get() + count);

        Ok(ArenaIndex::new(start_idx))
    }

    /// Get an ArenaIndex at a given offset from a starting index.
    /// 
    /// This is used to access slots within a contiguous allocation.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if the offset goes out of bounds
    /// or the slot is not allocated.
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use grift_arena::Arena;
    /// 
    /// let arena: Arena<isize, 100> = Arena::new(0);
    /// let start = arena.alloc_contiguous(3, 0).unwrap();
    /// 
    /// // Access slot at offset 1
    /// let idx1 = arena.index_at_offset(start, 1).unwrap();
    /// arena.set(idx1, 42).unwrap();
    /// assert_eq!(arena.get(idx1).unwrap(), 42);
    /// ```
    pub fn index_at_offset(&self, start: ArenaIndex, offset: usize) -> ArenaResult<ArenaIndex> {
        let index = start.offset(offset).ok_or(ArenaError::InvalidIndex)?;
        let idx = index.raw();
        if idx >= N {
            return Err(ArenaError::InvalidIndex);
        }
        
        // Verify the slot is actually occupied
        match self.slots[idx].get() {
            Slot::Occupied { .. } => Ok(index),
            Slot::Free { .. } => Err(ArenaError::InvalidIndex),
        }
    }

    /// Free a contiguous block starting at `start` with `count` slots.
    /// 
    /// All slots must be allocated.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if any slot is out of bounds or
    /// not allocated.
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use grift_arena::Arena;
    /// 
    /// let arena: Arena<isize, 100> = Arena::new(0);
    /// let start = arena.alloc_contiguous(5, 0).unwrap();
    /// 
    /// assert_eq!(arena.len(), 5);
    /// 
    /// arena.free_contiguous(start, 5).unwrap();
    /// 
    /// assert_eq!(arena.len(), 0);
    /// ```
    pub fn free_contiguous(&self, start: ArenaIndex, count: usize) -> ArenaResult<()> {
        if count == 0 {
            return Ok(());
        }

        let start_idx = start.raw();
        let end_idx = start_idx.checked_add(count).ok_or(ArenaError::InvalidIndex)?;
        if end_idx > N {
            return Err(ArenaError::InvalidIndex);
        }

        // Validate all slots are occupied
        if (start_idx..end_idx).any(|i| !matches!(self.slots[i].get(), Slot::Occupied { .. })) {
            return Err(ArenaError::InvalidIndex);
        }

        // Free all slots: link them together then chain to the existing free list
        let free_head = self.free_head.get();
        for i in (0..count).rev() {
            let idx = start_idx + i;
            let next = if i == count - 1 { free_head } else { start_idx + i + 1 };
            self.slots[idx].set(Slot::Free { next_free: next });
        }

        self.free_head.set(start_idx);
        self.len.set(self.len.get() - count);

        Ok(())
    }
}

// ============================================================================
// Trait Implementations for Arena
// ============================================================================

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
