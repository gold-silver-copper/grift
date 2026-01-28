//! Core arena allocator implementation.
//!
//! This module contains the main [`Arena`] struct and its core operations.

use core::cell::RefCell;

use crate::{ArenaIndex, ArenaError, ArenaResult, ArenaStats, ArenaDelete, ArenaCopy};
use crate::types::{Slot, FREE_LIST_END};
use crate::iter::ArenaIterator;

/// Fixed-size arena allocator with O(1) allocation.
///
/// # Type Parameters
///
/// - `T`: The type of values stored (must be `Copy` for array initialization)
/// - `N`: Maximum number of cells (const generic)
///
/// # Memory Layout
///
/// - `slots`: Array of `Slot<T>` (either free with next pointer, or occupied with value)
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
pub struct Arena<T: Copy, const N: usize> {
    pub(crate) slots: RefCell<[Slot<T>; N]>,
    pub(crate) free_head: RefCell<usize>,
    pub(crate) len: RefCell<usize>,
    pub(crate) gc_enabled: RefCell<bool>,
}

impl<T: Copy, const N: usize> Arena<T, N> {
    /// Create a new arena.
    ///
    /// All slots start as free, linked together in a free list.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::Arena;
    ///
    /// let arena: Arena<isize, 100> = Arena::new(0);
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

        Arena {
            slots: RefCell::new(slots),
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
    /// let arena: Arena<isize, 10> = Arena::new(0);
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

        // Increment allocated count
        drop(slots);
        *self.len.borrow_mut() += 1;

        Ok(ArenaIndex::new(idx))
    }

    /// Check that an index is valid (in bounds).
    /// Returns the slot index if valid.
    /// Note: Does NOT check if the slot is occupied - caller must verify.
    #[inline]
    fn check_bounds(&self, index: ArenaIndex) -> ArenaResult<usize> {
        let idx = index.raw();

        if idx >= N {
            return Err(ArenaError::InvalidIndex);
        }

        Ok(idx)
    }

    /// Validate an index and return the slot index if valid.
    #[inline]
    fn validate_index(&self, index: ArenaIndex) -> ArenaResult<usize> {
        let idx = self.check_bounds(index)?;

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
    #[inline]
    #[must_use]
    pub fn get(&self, index: ArenaIndex) -> ArenaResult<T> {
        let idx = self.check_bounds(index)?;

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
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::Arena;
    ///
    /// let arena: Arena<isize, 10> = Arena::new(0);
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
    /// # Errors
    ///
    /// Returns `ArenaError::InvalidIndex` if the index is out of bounds or not allocated.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pwn_arena::Arena;
    ///
    /// let arena: Arena<isize, 10> = Arena::new(0);
    /// let idx = arena.alloc(42).unwrap();
    /// arena.free(idx).unwrap();
    /// assert_eq!(arena.len(), 0);
    /// ```
    #[inline]
    pub fn free(&self, index: ArenaIndex) -> ArenaResult<()> {
        let idx = self.validate_index(index)?;

        // Push onto free list
        let mut free_head = self.free_head.borrow_mut();
        self.slots.borrow_mut()[idx] = Slot::Free { next_free: *free_head };
        *free_head = idx;

        // Decrement allocated count
        *self.len.borrow_mut() -= 1;

        Ok(())
    }

    /// Check if an index is currently valid (allocated).
    #[inline]
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
        let mut slots = self.slots.borrow_mut();
        for i in 0..N {
            slots[i] = Slot::Free {
                next_free: if i + 1 < N { i + 1 } else { FREE_LIST_END },
            };
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

    /// Check if a slot index is currently occupied.
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

        for (idx, slot) in slots.iter().enumerate() {
            if let Slot::Occupied { .. } = slot {
                result[count] = ArenaIndex::new(idx);
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

        for (idx, slot) in slots.iter().enumerate() {
            if let Slot::Occupied { value } = slot {
                f(ArenaIndex::new(idx), value);
            }
        }
    }

    /// Apply a mutating function to all allocated values.
    pub fn for_each_mut<F>(&self, mut f: F)
    where
        F: FnMut(ArenaIndex, &mut T),
    {
        let mut slots = self.slots.borrow_mut();

        for (idx, slot) in slots.iter_mut().enumerate() {
            if let Slot::Occupied { value } = slot {
                f(ArenaIndex::new(idx), value);
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

        for (idx, slot) in slots.iter().enumerate() {
            if let Slot::Occupied { value } = slot {
                if predicate(value) {
                    return Some((ArenaIndex::new(idx), *value));
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

        let slots = self.slots.borrow();
        let mut consecutive = 0;
        let mut start = 0;

        for i in 0..N {
            match slots[i] {
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

    /// Remove a slot from the free list.
    /// 
    /// This is a helper for contiguous allocation that removes a specific
    /// slot index from the free list by updating the linked list.
    fn remove_from_free_list(&self, slot_idx: usize) {
        let mut free_head = self.free_head.borrow_mut();
        let mut slots = self.slots.borrow_mut();

        // If the slot to remove is the head of the free list
        if *free_head == slot_idx {
            if let Slot::Free { next_free } = slots[slot_idx] {
                *free_head = next_free;
            }
            return;
        }

        // Otherwise, search for the slot in the free list
        let mut current = *free_head;
        while current != FREE_LIST_END {
            if let Slot::Free { next_free } = slots[current] {
                if next_free == slot_idx {
                    // Found it! Update the previous slot to skip this one
                    if let Slot::Free { next_free: removed_next } = slots[slot_idx] {
                        slots[current] = Slot::Free { next_free: removed_next };
                    }
                    return;
                }
                current = next_free;
            } else {
                break;
            }
        }
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
    /// use pwn_arena::Arena;
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

        // Remove all slots from free list and mark as occupied
        for i in 0..count {
            let idx = start_idx + i;
            self.remove_from_free_list(idx);
            self.slots.borrow_mut()[idx] = Slot::Occupied { value: default };
        }

        // Update length
        *self.len.borrow_mut() += count;

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
    /// use pwn_arena::Arena;
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
        let new_idx = start.raw() + offset;
        if new_idx >= N {
            return Err(ArenaError::InvalidIndex);
        }
        
        let index = ArenaIndex::new(new_idx);
        
        // Verify the slot is actually occupied
        match self.slots.borrow()[new_idx] {
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
    /// use pwn_arena::Arena;
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

        // Validate all slots are allocated and in bounds
        for i in 0..count {
            let idx = start_idx + i;
            if idx >= N {
                return Err(ArenaError::InvalidIndex);
            }
            
            // Check that slot is occupied
            match self.slots.borrow()[idx] {
                Slot::Occupied { .. } => {}
                Slot::Free { .. } => return Err(ArenaError::InvalidIndex),
            }
        }

        // Free all slots and add to free list
        let mut free_head = self.free_head.borrow_mut();
        let mut slots = self.slots.borrow_mut();

        // Link freed slots together, then link to free_head
        // Process in reverse so the slots end up in order in the free list
        for i in (0..count).rev() {
            let idx = start_idx + i;
            slots[idx] = Slot::Free {
                next_free: if i == count - 1 {
                    *free_head
                } else {
                    start_idx + i + 1
                },
            };
        }

        *free_head = start_idx;
        *self.len.borrow_mut() -= count;

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
