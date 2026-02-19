//! Iterator support for the arena.

use crate::{Arena, ArenaIndex, Slotted};

/// Iterator over allocated cells in the arena.
pub struct ArenaIterator<'a, T: Slotted, const N: usize> {
    pub(crate) arena: &'a Arena<T, N>,
    pub(crate) current: usize,
}

impl<'a, T: Slotted, const N: usize> Iterator for ArenaIterator<'a, T, N> {
    type Item = (ArenaIndex, T);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while self.current < N {
            let idx = self.current;
            self.current += 1;

            let value = self.arena.slots[idx].get();
            if !value.is_free() {
                return Some((ArenaIndex::new(idx), value));
            }
        }

        None
    }
}
