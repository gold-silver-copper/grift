//! Trait definitions for arena operations.
//!
//! This module contains the traits used for arena functionality:
//! - [`ArenaDelete`] - Recursive deletion support
//! - [`ArenaCopy`] - Deep copy support
//! - [`Trace`] - Garbage collection tracing

use crate::{Arena, ArenaIndex, ArenaResult};

// ============================================================================
// Recursive Deletion Support
// ============================================================================

/// Trait for types that can be recursively deleted from the arena.
///
/// Implement this for types that contain `ArenaIndex` fields pointing
/// to other allocations that should be freed together.
///
/// # Example
///
/// ```rust
/// use pwn_arena::{Arena, ArenaIndex, ArenaDelete, ArenaResult};
///
/// #[derive(Clone, Copy)]
/// enum Tree {
///     Leaf(isize),
///     Branch(ArenaIndex, ArenaIndex),
/// }
///
/// impl ArenaDelete<Tree, 100> for Tree {
///     fn delete_recursive(&self, arena: &Arena<Tree, 100>) -> ArenaResult<()> {
///         match *self {
///             Tree::Leaf(_) => Ok(()),
///             Tree::Branch(left, right) => {
///                 arena.delete_recursive(left)?;
///                 arena.delete_recursive(right)?;
///                 Ok(())
///             }
///         }
///     }
/// }
/// ```
pub trait ArenaDelete<T: Copy, const N: usize> {
    /// Recursively delete this value and any children from the arena.
    fn delete_recursive(&self, arena: &Arena<T, N>) -> ArenaResult<()>;
}

// ============================================================================
// Copy Support
// ============================================================================

/// Trait for types that can be deep-copied within the arena.
///
/// Implement this for types containing `ArenaIndex` fields that need
/// to recursively copy their children.
///
/// # Example
///
/// ```rust
/// use pwn_arena::{Arena, ArenaIndex, ArenaCopy, ArenaResult};
///
/// #[derive(Clone, Copy)]
/// enum Tree {
///     Leaf(isize),
///     Branch(ArenaIndex, ArenaIndex),
/// }
///
/// impl ArenaCopy<Tree, 100> for Tree {
///     fn copy_deep(&self, arena: &Arena<Tree, 100>) -> ArenaResult<Tree> {
///         match *self {
///             Tree::Leaf(n) => Ok(Tree::Leaf(n)),
///             Tree::Branch(left, right) => {
///                 let new_left = arena.copy_deep(left)?;
///                 let new_right = arena.copy_deep(right)?;
///                 Ok(Tree::Branch(new_left, new_right))
///             }
///         }
///     }
/// }
/// ```
pub trait ArenaCopy<T: Copy, const N: usize> {
    /// Create a deep copy of this value in the arena.
    fn copy_deep(&self, arena: &Arena<T, N>) -> ArenaResult<T>;
}

// ============================================================================
// Garbage Collection Support
// ============================================================================

/// Trait for types that can be traced by the garbage collector.
///
/// Implement this for types that contain `ArenaIndex` fields. The GC will
/// call `trace` to discover all reachable objects starting from the roots.
///
/// # Example
///
/// ```rust
/// use pwn_arena::{Arena, ArenaIndex, Trace};
///
/// #[derive(Clone, Copy)]
/// enum Tree {
///     Leaf(isize),
///     Branch(ArenaIndex, ArenaIndex),
/// }
///
/// impl<const N: usize> Trace<Tree, N> for Tree {
///     fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
///         match *self {
///             Tree::Leaf(_) => {} // No references to trace
///             Tree::Branch(left, right) => {
///                 tracer(left);
///                 tracer(right);
///             }
///         }
///     }
/// }
///
/// let arena: Arena<Tree, 100> = Arena::new(Tree::Leaf(0));
///
/// // Build a tree
/// let leaf1 = arena.alloc(Tree::Leaf(1)).unwrap();
/// let leaf2 = arena.alloc(Tree::Leaf(2)).unwrap();
/// let root = arena.alloc(Tree::Branch(leaf1, leaf2)).unwrap();
///
/// // Also allocate some garbage (unreachable nodes)
/// let garbage1 = arena.alloc(Tree::Leaf(999)).unwrap();
/// let garbage2 = arena.alloc(Tree::Leaf(888)).unwrap();
///
/// assert_eq!(arena.len(), 5);
///
/// // Collect garbage, keeping only objects reachable from `root`
/// let stats = arena.collect_garbage(&[root]);
///
/// assert_eq!(stats.collected, 2); // garbage1 and garbage2 were freed
/// assert_eq!(arena.len(), 3);     // root, leaf1, leaf2 remain
/// ```
pub trait Trace<T: Copy, const N: usize> {
    /// Trace all `ArenaIndex` references contained in this value.
    ///
    /// Call `tracer` once for each `ArenaIndex` field in this value.
    /// The GC uses this to discover the object graph.
    fn trace<F: FnMut(ArenaIndex)>(&self, tracer: F);
    
    /// Trace with arena access for types that store metadata in the arena.
    ///
    /// Some types (like arrays/strings that store their length in the arena)
    /// need to read from the arena during tracing to determine how many
    /// elements to trace. Override this method for such types.
    ///
    /// The default implementation just calls `trace()`.
    #[inline]
    fn trace_with_arena<F: FnMut(ArenaIndex)>(&self, _arena: &Arena<T, N>, tracer: F) {
        self.trace(tracer)
    }
}
