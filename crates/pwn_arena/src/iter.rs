//! Iterator support for the arena.

use crate::{Arena, ArenaIndex};
use crate::types::Slot;

/// Iterator over allocated cells in the arena.
pub struct ArenaIterator<'a, T: Copy, const N: usize> {
    pub(crate) arena: &'a Arena<T, N>,
    pub(crate) current: usize,
}

impl<'a, T: Copy, const N: usize> Iterator for ArenaIterator<'a, T, N> {
    type Item = (ArenaIndex, T);

    fn next(&mut self) -> Option<Self::Item> {
        let slots = self.arena.slots.borrow();

        while self.current < N {
            let idx = self.current;
            self.current += 1;

            if let Slot::Occupied { value } = slots[idx] {
                return Some((ArenaIndex::new(idx), value));
            }
        }

        None
    }
}
