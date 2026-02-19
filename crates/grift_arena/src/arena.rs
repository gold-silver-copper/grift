//! Core arena allocator implementation.
//!
//! This module contains the main [`Arena`] struct and its core operations.

use core::cell::Cell;

use crate::iter::ArenaIterator;
use crate::types::FREE_LIST_END;
use crate::{ArenaCopy, ArenaDelete, ArenaError, ArenaIndex, ArenaResult, ArenaStats, Slotted};

/// Fixed-size arena allocator with O(1) allocation.
///
/// # Type Parameters
///
/// - `T`: The type of values stored (must implement `Slotted` + `Copy`)
/// - `N`: Maximum number of cells (const generic)
///
/// # Memory Layout
///
/// - `slots`: Array of `Cell<T>` where free slots contain `T::make_free(next)` values
/// - `free_head`: Head of the free list
/// - `len`: Number of currently allocated slots
///
/// # O(1) Allocation
///
/// Uses a free-list for constant-time allocation and deallocation instead of
/// scanning a bitmap. Free-list metadata is embedded directly in the value
/// type via the `Slotted` trait.
///
/// # Garbage Collection
///
/// The arena supports mark-and-sweep garbage collection via the [`Trace`] trait.
///
/// # Performance
///
/// Uses `Cell` instead of `RefCell` for interior mutability. This eliminates
/// runtime borrow checking overhead and the risk of borrow panics, while
/// still maintaining safe Rust guarantees.
pub struct Arena<T: Slotted, const N: usize> {
    pub(crate) slots: [Cell<T>; N],
    pub(crate) free_head: Cell<usize>,
    pub(crate) len: Cell<usize>,
}

impl<T: Slotted, const N: usize> Arena<T, N> {
    /// Create a new arena.
    ///
    /// All slots start as free, linked together in a free list.
    /// Free-list metadata is embedded in the values via the `Slotted` trait.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_arena::{Arena, Slotted};
    ///
    /// #[derive(Clone, Copy, Debug, PartialEq)]
    /// enum Val { Free(usize), Num(isize) }
    ///
    /// impl Slotted for Val {
    ///     fn is_free(&self) -> bool { matches!(self, Val::Free(_)) }
    ///     fn next_free(&self) -> usize { match self { Val::Free(n) => *n, _ => unreachable!() } }
    ///     fn make_free(next: usize) -> Self { Val::Free(next) }
    /// }
    ///
    /// let arena: Arena<Val, 100> = Arena::new();
    /// ```
    pub fn new() -> Self {
        // Initialize all slots as free, linked together
        // Slot 0 -> 1 -> 2 -> ... -> N-1 -> FREE_LIST_END
        let slots: [Cell<T>; N] = core::array::from_fn(|i| {
            Cell::new(T::make_free(if i + 1 < N { i + 1 } else { FREE_LIST_END }))
        });

        Arena {
            slots,
            free_head: Cell::new(if N > 0 { 0 } else { FREE_LIST_END }),
            len: Cell::new(0),
        }
    }

    /// Get the maximum capacity of this arena.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Get the number of currently allocated cells.
    #[inline]
    pub fn len(&self) -> usize {
        self.len.get()
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
    /// use grift_arena::{Arena, Slotted};
    ///
    /// #[derive(Clone, Copy, Debug, PartialEq)]
    /// enum Val { Free(usize), Num(isize) }
    ///
    /// impl Slotted for Val {
    ///     fn is_free(&self) -> bool { matches!(self, Val::Free(_)) }
    ///     fn next_free(&self) -> usize { match self { Val::Free(n) => *n, _ => unreachable!() } }
    ///     fn make_free(next: usize) -> Self { Val::Free(next) }
    /// }
    ///
    /// let arena: Arena<Val, 10> = Arena::new();
    /// let idx = arena.alloc(Val::Num(42)).unwrap();
    /// assert_eq!(arena.get(idx).unwrap(), Val::Num(42));
    /// ```
    #[inline]
    pub fn alloc(&self, value: T) -> ArenaResult<ArenaIndex> {
        let free_head = self.free_head.get();

        // Check if there's a free slot
        if free_head == FREE_LIST_END {
            return Err(ArenaError::OutOfMemory);
        }

        let idx = free_head;

        // Pop from free list
        let slot = self.slots[idx].get();
        debug_assert!(slot.is_free(), "free_head pointed to occupied slot");
        let next_free = slot.next_free();

        // Mark as occupied
        self.slots[idx].set(value);
        self.free_head.set(next_free);

        // Increment allocated count
        self.len.set(self.len.get() + 1);

        Ok(ArenaIndex::new(idx))
    }

    /// Validate an index: in bounds and occupied.
    #[inline]
    fn validate_index(&self, index: ArenaIndex) -> ArenaResult<usize> {
        let idx = index.raw();
        if idx >= N {
            return Err(ArenaError::IndexOutOfBounds);
        }
        if self.slots[idx].get().is_free() {
            Err(ArenaError::IndexNotAllocated)
        } else {
            Ok(idx)
        }
    }

    /// Get a copy of the value at the given index.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::IndexOutOfBounds` if the index is out of bounds,
    /// or `ArenaError::IndexNotAllocated` if the slot is not allocated.
    #[inline]
    pub fn get(&self, index: ArenaIndex) -> ArenaResult<T> {
        let idx = index.raw();
        if idx >= N {
            return Err(ArenaError::IndexOutOfBounds);
        }
        let slot = self.slots[idx].get();
        if slot.is_free() {
            Err(ArenaError::IndexNotAllocated)
        } else {
            Ok(slot)
        }
    }

    /// Set the value at the given index.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::IndexOutOfBounds` or `ArenaError::IndexNotAllocated`
    /// if the index is invalid.
    #[inline]
    pub fn set(&self, index: ArenaIndex, value: T) -> ArenaResult<()> {
        let idx = self.validate_index(index)?;

        self.slots[idx].set(value);
        Ok(())
    }

    /// Modify a value in place using a closure.
    ///
    /// With Cell-based storage, this is implemented as get + modify + set.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::IndexOutOfBounds` or `ArenaError::IndexNotAllocated`
    /// if the index is invalid.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_arena::{Arena, Slotted};
    ///
    /// #[derive(Clone, Copy, Debug, PartialEq)]
    /// enum Val { Free(usize), Num(isize) }
    ///
    /// impl Slotted for Val {
    ///     fn is_free(&self) -> bool { matches!(self, Val::Free(_)) }
    ///     fn next_free(&self) -> usize { match self { Val::Free(n) => *n, _ => unreachable!() } }
    ///     fn make_free(next: usize) -> Self { Val::Free(next) }
    /// }
    ///
    /// let arena: Arena<Val, 10> = Arena::new();
    /// let idx = arena.alloc(Val::Num(42)).unwrap();
    ///
    /// arena.modify(idx, |v| {
    ///     if let Val::Num(n) = v { *n += 10; }
    /// }).unwrap();
    /// assert_eq!(arena.get(idx).unwrap(), Val::Num(52));
    /// ```
    pub fn modify<F>(&self, index: ArenaIndex, f: F) -> ArenaResult<()>
    where
        F: FnOnce(&mut T),
    {
        let idx = self.validate_index(index)?;
        let mut value = self.slots[idx].get();
        f(&mut value);
        self.slots[idx].set(value);
        Ok(())
    }

    /// Get a value, returning `None` instead of an error if invalid.
    ///
    /// This is a convenience method for cases where you expect the index
    /// might be invalid and want to handle it with `Option` instead of `Result`.
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
        if idx_a != idx_b {
            let val_a = self.slots[idx_a].get();
            let val_b = self.slots[idx_b].get();
            self.slots[idx_a].set(val_b);
            self.slots[idx_b].set(val_a);
        }
        Ok(())
    }

    /// Replace the value at an index, returning the old value.
    ///
    /// # Errors
    ///
    /// Returns an error if the index is invalid.
    pub fn replace(&self, index: ArenaIndex, value: T) -> ArenaResult<T> {
        let idx = self.validate_index(index)?;
        let old = self.slots[idx].get();
        self.slots[idx].set(value);
        Ok(old)
    }

    /// Free a cell, making it available for reuse.
    ///
    /// # Errors
    ///
    /// Returns `ArenaError::IndexOutOfBounds` or `ArenaError::IndexNotAllocated`
    /// if the index is invalid.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_arena::{Arena, Slotted};
    ///
    /// #[derive(Clone, Copy, Debug, PartialEq)]
    /// enum Val { Free(usize), Num(isize) }
    ///
    /// impl Slotted for Val {
    ///     fn is_free(&self) -> bool { matches!(self, Val::Free(_)) }
    ///     fn next_free(&self) -> usize { match self { Val::Free(n) => *n, _ => unreachable!() } }
    ///     fn make_free(next: usize) -> Self { Val::Free(next) }
    /// }
    ///
    /// let arena: Arena<Val, 10> = Arena::new();
    /// let idx = arena.alloc(Val::Num(42)).unwrap();
    /// arena.free(idx).unwrap();
    /// assert_eq!(arena.len(), 0);
    /// ```
    #[inline]
    pub fn free(&self, index: ArenaIndex) -> ArenaResult<()> {
        let idx = self.validate_index(index)?;

        // Push onto free list
        let free_head = self.free_head.get();
        self.slots[idx].set(T::make_free(free_head));
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
            self.slots[i].set(T::make_free(if i + 1 < N { i + 1 } else { FREE_LIST_END }));
        }

        self.free_head.set(if N > 0 { 0 } else { FREE_LIST_END });
        self.len.set(0);
    }

    /// Iterate over all allocated indices and values.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_arena::{Arena, ArenaIndex, Slotted};
    ///
    /// #[derive(Clone, Copy, Debug)]
    /// enum Val { Free(usize), V(isize) }
    /// impl Slotted for Val {
    ///     fn is_free(&self) -> bool { matches!(self, Val::Free(_)) }
    ///     fn next_free(&self) -> usize { match self { Val::Free(n) => *n, _ => unreachable!() } }
    ///     fn make_free(next: usize) -> Self { Val::Free(next) }
    /// }
    ///
    /// let arena: Arena<Val, 10> = Arena::new();
    /// arena.alloc(Val::V(1)).unwrap();
    /// arena.alloc(Val::V(2)).unwrap();
    /// arena.alloc(Val::V(3)).unwrap();
    ///
    /// for (idx, value) in arena.iter() {
    ///     println!("Index {:?}: {:?}", idx, value);
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
        // Count free-space fragments: contiguous runs of free slots.
        let (fragments, _) = (0..N).fold((0u32, false), |(count, was_free), i| {
            let is_free = self.slots[i].get().is_free();
            (count + u32::from(is_free && !was_free), is_free)
        });

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
        let free_head = self.free_head.get();
        let len = self.len.get();

        // Count occupied slots
        let occupied_count = (0..N)
            .filter(|&i| !self.slots[i].get().is_free())
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

            let slot = self.slots[current].get();
            if slot.is_free() {
                free_count += 1;
                current = slot.next_free();
            } else {
                return false; // Free list points to occupied slot
            }
        }

        // Check that free_count + occupied_count == N
        if free_count + occupied_count != N {
            return false;
        }

        // Check all free slots are in the free list
        for (i, &was_visited) in visited.iter().enumerate().take(N) {
            if self.slots[i].get().is_free() && !was_visited {
                return false; // Free slot not in free list
            }
        }

        true
    }

    /// Check if a slot index is currently occupied.
    ///
    /// This is a low-level debugging method. For normal use, prefer [`is_allocated`].
    pub fn is_slot_occupied(&self, slot_index: usize) -> bool {
        slot_index < N && !self.slots[slot_index].get().is_free()
    }

    /// Get indices of all allocated slots.
    ///
    /// Returns an array with the first `len()` elements being valid indices.
    /// The remaining elements are [`ArenaIndex::NIL`].
    pub fn allocated_indices(&self) -> [ArenaIndex; N] {
        let mut result = [ArenaIndex::NIL; N];
        let mut count = 0;

        for idx in 0..N {
            if !self.slots[idx].get().is_free() {
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
            let value = self.slots[idx].get();
            if !value.is_free() {
                let mut value = value;
                f(ArenaIndex::new(idx), &mut value);
                self.slots[idx].set(value);
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
}

// — Trait Implementations for Arena —

impl<T: Slotted, const N: usize> Arena<T, N> {
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
