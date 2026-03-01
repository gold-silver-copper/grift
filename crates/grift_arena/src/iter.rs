//! Iterator support for the arena.
//!
//! Provides [`ArenaIterator`], which yields `(ArenaIndex, T)` pairs
//! for every currently occupied slot. Slots that are free are silently
//! skipped, so the iterator produces exactly `arena.len()` items.

use crate::{Arena, ArenaIndex};
use crate::types::Slot;

/// Iterator over allocated cells in the arena.
///
/// Created by [`Arena::iter`]. Visits slots in index order (0..N),
/// skipping free slots. The iterator borrows the arena immutably, so
/// allocations and frees must not occur while iterating.
pub struct ArenaIterator<'a, T: Copy, const N: usize> {
    pub(crate) arena: &'a Arena<T, N>,
    pub(crate) current: usize,
}

impl<T: Copy, const N: usize> Iterator for ArenaIterator<'_, T, N> {
    type Item = (ArenaIndex, T);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while self.current < N {
            let idx = self.current;
            self.current += 1;

            if let Slot::Occupied { value } = self.arena.slots[idx].get() {
                return Some((ArenaIndex::new(idx), value));
            }
        }

        None
    }
}
