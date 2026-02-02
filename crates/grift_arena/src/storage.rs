//! Storage backends for the arena allocator.
//!
//! This module contains the [`ArenaStorage`] trait and implementations for
//! different storage backends:
//!
//! - Fixed-size arrays (`[T; N]`) for no-std/embedded environments
//! - `Vec<T>` for std environments (requires the `std` feature)

use core::cell::Cell;
use crate::types::{Slot, FREE_LIST_END};

/// Trait for arena storage backends.
///
/// This trait abstracts over the underlying storage mechanism used by the arena.
/// It allows the same arena logic to work with both fixed-size arrays (for
/// embedded/no-std environments) and dynamic vectors (for std environments).
///
/// # Note
///
/// This trait is sealed and cannot be implemented outside of this crate.
/// Use the provided storage types: [`ArrayStorage`] or [`VecStorage`].
pub trait ArenaStorage<T: Copy>: Sized + sealed::Sealed {
    /// Create a new storage with all slots initialized as free.
    ///
    /// The slots should form a free list where slot 0 -> 1 -> 2 -> ... -> capacity-1 -> FREE_LIST_END.
    fn new_free_list() -> Self;

    /// Get the capacity of this storage.
    fn capacity(&self) -> usize;

    /// Get a copy of the slot at the given index.
    ///
    /// # Panics
    ///
    /// May panic if `index >= capacity()`.
    #[doc(hidden)]
    fn get_slot(&self, index: usize) -> Slot<T>;

    /// Set the slot at the given index.
    ///
    /// # Panics
    ///
    /// May panic if `index >= capacity()`.
    #[doc(hidden)]
    fn set_slot(&self, index: usize, slot: Slot<T>);
}

mod sealed {
    pub trait Sealed {}
}

// ============================================================================
// Fixed-Size Array Storage
// ============================================================================

/// Fixed-size array storage for no-std/embedded environments.
///
/// This wraps a `[Cell<Slot<T>>; N]` array for O(1) indexed access
/// with interior mutability via `Cell`.
///
/// # Type Parameters
///
/// - `T`: The type of values stored (must be `Copy`)
/// - `N`: Maximum number of slots (const generic)
pub struct ArrayStorage<T: Copy, const N: usize> {
    slots: [Cell<Slot<T>>; N],
}

impl<T: Copy, const N: usize> sealed::Sealed for ArrayStorage<T, N> {}

impl<T: Copy, const N: usize> ArenaStorage<T> for ArrayStorage<T, N> {
    fn new_free_list() -> Self {
        let slots: [Cell<Slot<T>>; N] = core::array::from_fn(|i| {
            Cell::new(Slot::Free {
                next_free: if i + 1 < N { i + 1 } else { FREE_LIST_END },
            })
        });
        ArrayStorage { slots }
    }

    #[inline]
    fn capacity(&self) -> usize {
        N
    }

    #[inline]
    fn get_slot(&self, index: usize) -> Slot<T> {
        self.slots[index].get()
    }

    #[inline]
    fn set_slot(&self, index: usize, slot: Slot<T>) {
        self.slots[index].set(slot);
    }
}

// ============================================================================
// Vec Storage (std feature)
// ============================================================================

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
use std::vec::Vec;

/// Vector-based storage for std environments.
///
/// This uses a `Vec<Cell<Slot<T>>>` for dynamic capacity.
/// The capacity is fixed at creation time and cannot be resized.
///
/// # Feature
///
/// This type is only available with the `std` feature enabled.
#[cfg(feature = "std")]
pub struct VecStorage<T: Copy> {
    slots: Vec<Cell<Slot<T>>>,
}

#[cfg(feature = "std")]
impl<T: Copy> VecStorage<T> {
    /// Create a new VecStorage with the given capacity.
    ///
    /// All slots are initialized as free, forming a free list.
    pub fn with_capacity(capacity: usize) -> Self {
        let mut slots = Vec::with_capacity(capacity);
        for i in 0..capacity {
            slots.push(Cell::new(Slot::Free {
                next_free: if i + 1 < capacity { i + 1 } else { FREE_LIST_END },
            }));
        }
        VecStorage { slots }
    }
}

#[cfg(feature = "std")]
impl<T: Copy> sealed::Sealed for VecStorage<T> {}

#[cfg(feature = "std")]
impl<T: Copy> ArenaStorage<T> for VecStorage<T> {
    fn new_free_list() -> Self {
        // Default to a reasonable capacity when using new_free_list
        // For specific capacities, use VecStorage::with_capacity directly
        Self::with_capacity(1024)
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    fn get_slot(&self, index: usize) -> Slot<T> {
        self.slots[index].get()
    }

    #[inline]
    fn set_slot(&self, index: usize, slot: Slot<T>) {
        self.slots[index].set(slot);
    }
}
