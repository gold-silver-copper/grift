//! Generic arena allocator implementation.
//!
//! This module contains the [`GenericArena`] struct which is parameterized by a
//! storage backend implementing [`ArenaStorage`].

use core::cell::Cell;

use crate::storage::ArenaStorage;
use crate::{ArenaIndex, ArenaError, ArenaResult, ArenaStats};
use crate::types::{Slot, FREE_LIST_END};
use crate::traits::{GenericArenaDelete, GenericArenaCopy};

/// Generic arena allocator that works with any storage backend.
///
/// This is a more flexible version of [`Arena`] that can use different
/// storage backends via the [`ArenaStorage`] trait.
///
/// # Type Parameters
///
/// - `T`: The type of values stored (must be `Copy`)
/// - `S`: The storage backend implementing [`ArenaStorage<T>`]
///
/// # Example
///
/// ```rust
/// use grift_arena::{GenericArena, ArrayStorage};
///
/// let arena: GenericArena<isize, ArrayStorage<isize, 100>> = GenericArena::new(0);
/// let idx = arena.alloc(42).unwrap();
/// assert_eq!(arena.get(idx).unwrap(), 42);
/// ```
pub struct GenericArena<T: Copy, S: ArenaStorage<T>> {
    pub(crate) storage: S,
    pub(crate) free_head: Cell<usize>,
    pub(crate) len: Cell<usize>,
    pub(crate) gc_enabled: Cell<bool>,
    pub(crate) _phantom: core::marker::PhantomData<T>,
}

impl<T: Copy, S: ArenaStorage<T>> GenericArena<T, S> {
    /// Create a new arena with the given storage.
    ///
    /// The storage should already be initialized with a free list.
    pub fn with_storage(storage: S) -> Self {
        let capacity = storage.capacity();
        GenericArena {
            storage,
            free_head: Cell::new(if capacity > 0 { 0 } else { FREE_LIST_END }),
            len: Cell::new(0),
            gc_enabled: Cell::new(true),
            _phantom: core::marker::PhantomData,
        }
    }

    /// Create a new arena using default storage initialization.
    ///
    /// All slots start as free, linked together in a free list.
    pub fn new(_default_value: T) -> Self {
        Self::with_storage(S::new_free_list())
    }

    /// Check if garbage collection is enabled.
    #[inline]
    pub fn is_gc_enabled(&self) -> bool {
        self.gc_enabled.get()
    }

    /// Enable or disable garbage collection.
    #[inline]
    pub fn set_gc_enabled(&self, enabled: bool) {
        self.gc_enabled.set(enabled);
    }

    /// Temporarily disable GC, run a closure, then restore the previous state.
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
    pub fn capacity(&self) -> usize {
        self.storage.capacity()
    }

    /// Get the number of currently allocated cells.
    #[inline]
    pub fn len(&self) -> usize {
        self.len.get()
    }

    /// Check if the arena is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the arena is full.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    /// Get the number of free cells.
    #[inline]
    pub fn available(&self) -> usize {
        self.capacity() - self.len()
    }

    /// Allocate a new cell and return its index.
    pub fn alloc(&self, value: T) -> ArenaResult<ArenaIndex> {
        let free_head = self.free_head.get();

        if free_head == FREE_LIST_END {
            return Err(ArenaError::OutOfMemory);
        }

        let idx = free_head;

        // Pop from free list
        let next_free = match self.storage.get_slot(idx) {
            Slot::Free { next_free } => next_free,
            Slot::Occupied { .. } => unreachable!("free_head pointed to occupied slot"),
        };

        // Mark as occupied
        self.storage.set_slot(idx, Slot::Occupied { value });
        self.free_head.set(next_free);
        self.len.set(self.len.get() + 1);

        Ok(ArenaIndex::new(idx))
    }

    /// Check that an index is valid (in bounds).
    #[inline]
    fn check_bounds(&self, index: ArenaIndex) -> ArenaResult<usize> {
        let idx = index.raw();
        if idx >= self.capacity() {
            return Err(ArenaError::InvalidIndex);
        }
        Ok(idx)
    }

    /// Validate an index and return the slot index if valid.
    #[inline]
    fn validate_index(&self, index: ArenaIndex) -> ArenaResult<usize> {
        let idx = self.check_bounds(index)?;
        match self.storage.get_slot(idx) {
            Slot::Occupied { .. } => Ok(idx),
            Slot::Free { .. } => Err(ArenaError::InvalidIndex),
        }
    }

    /// Get a copy of the value at the given index.
    #[inline]
    pub fn get(&self, index: ArenaIndex) -> ArenaResult<T> {
        let idx = self.check_bounds(index)?;
        match self.storage.get_slot(idx) {
            Slot::Occupied { value } => Ok(value),
            Slot::Free { .. } => Err(ArenaError::InvalidIndex),
        }
    }

    /// Set the value at the given index.
    #[inline]
    pub fn set(&self, index: ArenaIndex, value: T) -> ArenaResult<()> {
        let idx = self.validate_index(index)?;
        self.storage.set_slot(idx, Slot::Occupied { value });
        Ok(())
    }

    /// Modify a value in place using a closure.
    #[inline]
    pub fn modify<F>(&self, index: ArenaIndex, f: F) -> ArenaResult<()>
    where
        F: FnOnce(&mut T),
    {
        let idx = self.validate_index(index)?;

        if let Slot::Occupied { mut value } = self.storage.get_slot(idx) {
            f(&mut value);
            self.storage.set_slot(idx, Slot::Occupied { value });
            Ok(())
        } else {
            Err(ArenaError::InvalidIndex)
        }
    }

    /// Get a value, returning `None` instead of an error if invalid.
    #[inline]
    #[must_use]
    pub fn try_get(&self, index: ArenaIndex) -> Option<T> {
        self.get(index).ok()
    }

    /// Swap the values at two indices.
    pub fn swap(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<()> {
        let idx_a = self.validate_index(a)?;
        let idx_b = self.validate_index(b)?;

        if idx_a == idx_b {
            return Ok(());
        }

        let (val_a, val_b) = match (self.storage.get_slot(idx_a), self.storage.get_slot(idx_b)) {
            (Slot::Occupied { value: va }, Slot::Occupied { value: vb }) => (va, vb),
            _ => return Err(ArenaError::InvalidIndex),
        };

        self.storage.set_slot(idx_a, Slot::Occupied { value: val_b });
        self.storage.set_slot(idx_b, Slot::Occupied { value: val_a });

        Ok(())
    }

    /// Replace the value at an index, returning the old value.
    pub fn replace(&self, index: ArenaIndex, value: T) -> ArenaResult<T> {
        let idx = self.validate_index(index)?;

        match self.storage.get_slot(idx) {
            Slot::Occupied { value: old } => {
                self.storage.set_slot(idx, Slot::Occupied { value });
                Ok(old)
            }
            Slot::Free { .. } => Err(ArenaError::InvalidIndex),
        }
    }

    /// Free a cell, making it available for reuse.
    #[inline]
    pub fn free(&self, index: ArenaIndex) -> ArenaResult<()> {
        let idx = self.validate_index(index)?;

        let free_head = self.free_head.get();
        self.storage.set_slot(idx, Slot::Free { next_free: free_head });
        self.free_head.set(idx);
        self.len.set(self.len.get() - 1);

        Ok(())
    }

    /// Check if an index is currently valid (allocated).
    #[inline]
    pub fn is_allocated(&self, index: ArenaIndex) -> bool {
        self.validate_index(index).is_ok()
    }

    /// Clear all allocations, making the entire arena available.
    pub fn clear(&self) {
        let capacity = self.capacity();
        for i in 0..capacity {
            self.storage.set_slot(i, Slot::Free {
                next_free: if i + 1 < capacity { i + 1 } else { FREE_LIST_END },
            });
        }
        self.free_head.set(if capacity > 0 { 0 } else { FREE_LIST_END });
        self.len.set(0);
    }

    /// Iterate over all allocated indices and values.
    pub fn iter(&self) -> GenericArenaIterator<'_, T, S> {
        GenericArenaIterator {
            arena: self,
            current: 0,
        }
    }

    /// Get statistics about arena usage.
    pub fn stats(&self) -> ArenaStats {
        let allocated = self.len();
        let capacity = self.capacity();
        ArenaStats {
            capacity,
            allocated,
            free: capacity - allocated,
            fragmentation: self.calculate_fragmentation(),
        }
    }

    fn calculate_fragmentation(&self) -> f32 {
        let capacity = self.capacity();
        let mut fragments = 0;
        let mut in_free = false;

        for i in 0..capacity {
            match self.storage.get_slot(i) {
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
            fragments as f32 / capacity as f32
        }
    }

    /// Check if a slot index is currently occupied.
    pub fn is_slot_occupied(&self, slot_index: usize) -> bool {
        if slot_index >= self.capacity() {
            return false;
        }
        matches!(self.storage.get_slot(slot_index), Slot::Occupied { .. })
    }

    /// Apply a function to all allocated values.
    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(ArenaIndex, &T),
    {
        for idx in 0..self.capacity() {
            if let Slot::Occupied { value } = self.storage.get_slot(idx) {
                f(ArenaIndex::new(idx), &value);
            }
        }
    }

    /// Apply a mutating function to all allocated values.
    pub fn for_each_mut<F>(&self, mut f: F)
    where
        F: FnMut(ArenaIndex, &mut T),
    {
        for idx in 0..self.capacity() {
            if let Slot::Occupied { mut value } = self.storage.get_slot(idx) {
                f(ArenaIndex::new(idx), &mut value);
                self.storage.set_slot(idx, Slot::Occupied { value });
            }
        }
    }

    /// Count values matching a predicate.
    pub fn count_where<F>(&self, predicate: F) -> usize
    where
        F: Fn(&T) -> bool,
    {
        let mut count = 0;
        for i in 0..self.capacity() {
            if let Slot::Occupied { value } = self.storage.get_slot(i)
                && predicate(&value)
            {
                count += 1;
            }
        }
        count
    }

    /// Find the first value matching a predicate.
    pub fn find<F>(&self, predicate: F) -> Option<(ArenaIndex, T)>
    where
        F: Fn(&T) -> bool,
    {
        for idx in 0..self.capacity() {
            if let Slot::Occupied { value } = self.storage.get_slot(idx)
                && predicate(&value)
            {
                return Some((ArenaIndex::new(idx), value));
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
    pub fn all<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        for i in 0..self.capacity() {
            if let Slot::Occupied { value } = self.storage.get_slot(i)
                && !predicate(&value)
            {
                return false;
            }
        }
        true
    }

    // ========================================================================
    // Contiguous Allocation
    // ========================================================================

    /// Find a contiguous block of `count` free slots.
    fn find_contiguous_free_slots(&self, count: usize) -> Option<usize> {
        let capacity = self.capacity();
        if count == 0 || count > capacity {
            return None;
        }

        let mut consecutive = 0;
        let mut start = 0;

        for i in 0..capacity {
            match self.storage.get_slot(i) {
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
    fn remove_from_free_list(&self, slot_idx: usize) {
        let free_head = self.free_head.get();

        if free_head == slot_idx {
            if let Slot::Free { next_free } = self.storage.get_slot(slot_idx) {
                self.free_head.set(next_free);
            }
            return;
        }

        let mut current = free_head;
        while current != FREE_LIST_END {
            if let Slot::Free { next_free } = self.storage.get_slot(current) {
                if next_free == slot_idx {
                    if let Slot::Free { next_free: removed_next } = self.storage.get_slot(slot_idx) {
                        self.storage.set_slot(current, Slot::Free { next_free: removed_next });
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
    pub fn alloc_contiguous(&self, count: usize, default: T) -> ArenaResult<ArenaIndex> {
        if count == 0 {
            return Err(ArenaError::InvalidIndex);
        }

        let start_idx = self.find_contiguous_free_slots(count)
            .ok_or(ArenaError::OutOfMemory)?;

        for i in 0..count {
            let idx = start_idx + i;
            self.remove_from_free_list(idx);
            self.storage.set_slot(idx, Slot::Occupied { value: default });
        }

        self.len.set(self.len.get() + count);

        Ok(ArenaIndex::new(start_idx))
    }

    /// Get an ArenaIndex at a given offset from a starting index.
    pub fn index_at_offset(&self, start: ArenaIndex, offset: usize) -> ArenaResult<ArenaIndex> {
        let new_idx = start.raw() + offset;
        if new_idx >= self.capacity() {
            return Err(ArenaError::InvalidIndex);
        }

        let index = ArenaIndex::new(new_idx);

        match self.storage.get_slot(new_idx) {
            Slot::Occupied { .. } => Ok(index),
            Slot::Free { .. } => Err(ArenaError::InvalidIndex),
        }
    }

    /// Free a contiguous block starting at `start` with `count` slots.
    pub fn free_contiguous(&self, start: ArenaIndex, count: usize) -> ArenaResult<()> {
        if count == 0 {
            return Ok(());
        }

        let start_idx = start.raw();
        let capacity = self.capacity();

        // Validate all slots are allocated and in bounds
        for i in 0..count {
            let idx = start_idx + i;
            if idx >= capacity {
                return Err(ArenaError::InvalidIndex);
            }

            match self.storage.get_slot(idx) {
                Slot::Occupied { .. } => {}
                Slot::Free { .. } => return Err(ArenaError::InvalidIndex),
            }
        }

        let free_head = self.free_head.get();

        for i in (0..count).rev() {
            let idx = start_idx + i;
            self.storage.set_slot(idx, Slot::Free {
                next_free: if i == count - 1 {
                    free_head
                } else {
                    start_idx + i + 1
                },
            });
        }

        self.free_head.set(start_idx);
        self.len.set(self.len.get() - count);

        Ok(())
    }
}

// ============================================================================
// Trait Implementations
// ============================================================================

impl<T: Copy, S: ArenaStorage<T>> GenericArena<T, S> {
    /// Delete a value and recursively delete any children.
    pub fn delete_recursive(&self, index: ArenaIndex) -> ArenaResult<()>
    where
        T: GenericArenaDelete<T, S>,
    {
        let value = self.get(index)?;
        value.delete_recursive(self)?;
        self.free(index)?;
        Ok(())
    }

    /// Create a deep copy of a value and its children.
    pub fn copy_deep(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex>
    where
        T: GenericArenaCopy<T, S>,
    {
        let value = self.get(index)?;
        let copied = value.copy_deep(self)?;
        self.alloc(copied)
    }
}

// ============================================================================
// Iterator for GenericArena
// ============================================================================

/// Iterator over allocated cells in a generic arena.
pub struct GenericArenaIterator<'a, T: Copy, S: ArenaStorage<T>> {
    arena: &'a GenericArena<T, S>,
    current: usize,
}

impl<'a, T: Copy, S: ArenaStorage<T>> Iterator for GenericArenaIterator<'a, T, S> {
    type Item = (ArenaIndex, T);

    fn next(&mut self) -> Option<Self::Item> {
        while self.current < self.arena.capacity() {
            let idx = self.current;
            self.current += 1;

            if let Slot::Occupied { value } = self.arena.storage.get_slot(idx) {
                return Some((ArenaIndex::new(idx), value));
            }
        }
        None
    }
}
