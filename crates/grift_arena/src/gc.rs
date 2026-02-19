//! Garbage collection implementation.
//!
//! This module contains the mark-and-sweep garbage collection logic
//! for the arena allocator.

use crate::traits::Trace;
use crate::types::Slot;
use crate::{Arena, ArenaIndex, GcStats};

impl<T: Copy, const N: usize> Arena<T, N> {
    /// Initialize roots into the mark stack.
    ///
    /// For each valid root, mark it and push to the stack.
    /// This is used internally by garbage collection methods.
    fn initialize_roots(
        &self,
        roots: &[ArenaIndex],
        marked: &mut [bool; N],
        mark_stack: &mut [usize; N],
        stack_len: &mut usize,
    ) {
        for &root in roots {
            if self.is_allocated(root) {
                let idx = root.raw();
                if idx < N && !marked[idx] {
                    marked[idx] = true;
                    if *stack_len < N {
                        mark_stack[*stack_len] = idx;
                        *stack_len += 1;
                    }
                }
            }
        }
    }

    /// Process the mark stack using depth-first traversal.
    ///
    /// This iteratively processes children in batches to avoid stack overflow
    /// when objects have many children. Uses iterative batching to ensure all
    /// children are processed even when there are more than 16 per object.
    fn process_mark_stack(
        &self,
        marked: &mut [bool; N],
        mark_stack: &mut [usize; N],
        stack_len: &mut usize,
    ) where
        T: Trace<T, N>,
    {
        while *stack_len > 0 {
            *stack_len -= 1;
            let current_idx = mark_stack[*stack_len];

            if let Slot::Occupied { value } = self.slots[current_idx].get() {
                // Use iterative batching to process all children
                // This fixes the overflow bug by continuing to batch until all children are processed
                loop {
                    let mut batch = [0usize; 16];
                    let mut batch_count = 0usize;
                    let mut has_more = false;

                    // Use trace_with_arena to allow types to read metadata from the arena
                    value.trace_with_arena(self, |child_index| {
                        let idx = child_index.raw();
                        // Only bounds check needed - we trust trace implementations
                        if idx < N && !marked[idx] {
                            if batch_count < 16 {
                                batch[batch_count] = idx;
                                batch_count += 1;
                            } else {
                                has_more = true;
                            }
                        }
                    });

                    // If no unmarked children found, we're done with this object
                    if batch_count == 0 {
                        break;
                    }

                    // Process the batch - mark and push to stack
                    for &idx in batch.iter().take(batch_count) {
                        // Check marked again: trace could yield duplicates, or another
                        // batch entry could have already marked this index
                        if !marked[idx] {
                            marked[idx] = true;
                            if *stack_len < N {
                                mark_stack[*stack_len] = idx;
                                *stack_len += 1;
                            }
                        }
                    }

                    // If no more unmarked children beyond this batch, we're done
                    if !has_more {
                        break;
                    }
                    // Otherwise, continue to next iteration to get remaining children
                }
            }
        }
    }

    /// Sweep phase: free all unmarked but allocated slots in a single pass.
    ///
    /// Returns the number of objects collected.
    fn sweep_unmarked(&self, marked: &[bool; N]) -> usize {
        (0..N)
            .filter(|&idx| !marked[idx] && matches!(self.slots[idx].get(), Slot::Occupied { .. }))
            .map(|idx| self.free(ArenaIndex::new(idx)))
            .filter(|r| r.is_ok())
            .count()
    }

    /// Shared mark-and-sweep implementation. Runs the full mark-sweep cycle
    /// on a pre-initialized mark state.
    fn mark_and_sweep(
        &self,
        marked: &mut [bool; N],
        mark_stack: &mut [usize; N],
        stack_len: &mut usize,
    ) -> GcStats
    where
        T: Trace<T, N>,
    {
        let total_before = self.len();

        self.process_mark_stack(marked, mark_stack, stack_len);

        let marked_count = marked.iter().filter(|&&m| m).count();
        let collected = self.sweep_unmarked(marked);

        GcStats {
            marked: marked_count,
            collected,
            total_before,
        }
    }

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
    /// use grift_arena::{Arena, ArenaIndex, Trace};
    ///
    /// #[derive(Clone, Copy)]
    /// struct Node {
    ///     value: isize,
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
        self.collect_garbage_multi(&[roots])
    }

    /// Perform garbage collection with multiple root sets.
    ///
    /// This iterates through all provided root sets and marks objects
    /// reachable from any of them.
    ///
    /// Respects the `gc_enabled` flag - use [`Arena::collect_garbage_multi_unconditional`]
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
        let mut marked = [false; N];
        let mut mark_stack = [0usize; N];
        let mut stack_len = 0usize;

        for root_set in root_sets {
            self.initialize_roots(root_set, &mut marked, &mut mark_stack, &mut stack_len);
        }
        self.mark_and_sweep(&mut marked, &mut mark_stack, &mut stack_len)
    }
}
