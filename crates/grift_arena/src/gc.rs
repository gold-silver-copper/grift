//! Garbage collection implementation.
//!
//! This module contains the mark-and-sweep garbage collection logic
//! for the arena allocator.

use crate::{Arena, ArenaIndex, GcStats};
use crate::types::Slot;
use crate::traits::Trace;

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
        let mut collected = 0;

        // Single-pass sweep: iterate once and free immediately
        for (idx, &is_marked) in marked.iter().enumerate().take(N) {
            let should_free = matches!(self.slots[idx].get(), Slot::Occupied { .. }) && !is_marked;

            if should_free && self.free(ArenaIndex::new(idx)).is_ok() {
                collected += 1;
            }
        }

        collected
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
        self.initialize_roots(roots, &mut marked, &mut mark_stack, &mut stack_len);

        // Process mark stack (depth-first traversal)
        self.process_mark_stack(&mut marked, &mut mark_stack, &mut stack_len);

        let marked_count = marked.iter().filter(|&&m| m).count();

        // Sweep phase: free all unmarked but allocated slots
        let collected = self.sweep_unmarked(&marked);

        GcStats {
            marked: marked_count,
            collected,
            total_before,
        }
    }

    /// Perform garbage collection unconditionally, even if GC is disabled.
    ///
    /// This ignores the `gc_enabled` flag and always performs collection.
    /// Useful when you need to collect garbage regardless of the current
    /// GC state.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_arena::{Arena, ArenaIndex, Trace};
    ///
    /// #[derive(Clone, Copy)]
    /// struct Leaf(isize);
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
    /// // This WILL collect (unconditionally)
    /// let stats = arena.collect_garbage_unconditional(&[root]);
    /// assert_eq!(stats.collected, 2);
    /// ```
    pub fn collect_garbage_unconditional(&self, roots: &[ArenaIndex]) -> GcStats
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
            self.initialize_roots(root_set, &mut marked, &mut mark_stack, &mut stack_len);
        }

        // Process mark stack (depth-first traversal)
        self.process_mark_stack(&mut marked, &mut mark_stack, &mut stack_len);

        let marked_count = marked.iter().filter(|&&m| m).count();

        // Sweep phase: free all unmarked but allocated slots
        let collected = self.sweep_unmarked(&marked);

        GcStats {
            marked: marked_count,
            collected,
            total_before,
        }
    }

    /// Perform garbage collection unconditionally with multiple root sets, ignoring the `gc_enabled` flag.
    pub fn collect_garbage_multi_unconditional(&self, root_sets: &[&[ArenaIndex]]) -> GcStats
    where
        T: Trace<T, N>,
    {
        let was_enabled = self.is_gc_enabled();
        self.set_gc_enabled(true);
        let result = self.collect_garbage_multi(root_sets);
        self.set_gc_enabled(was_enabled);
        result
    }
}
