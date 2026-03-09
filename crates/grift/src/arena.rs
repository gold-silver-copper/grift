#![allow(
    clippy::must_use_candidate,
    clippy::doc_markdown,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::single_match_else,
    clippy::module_name_repetitions,
    clippy::inline_always,
    clippy::needless_pass_by_value,
    clippy::wildcard_imports,
    clippy::iter_without_into_iter,
    clippy::iter_filter_is_ok
)]

//! # Fixed-Size Arena Allocator
//!
//! A minimal `no_std`, `no_alloc` arena allocator with fixed capacity,
//! designed for embedded and resource-constrained environments where heap
//! allocation is unavailable or undesirable.
//!
//! ## Features
//!
//! - **Fixed-size**: All memory pre-allocated at compile time via const generics
//! - **No-std, no-alloc**: Works in embedded environments with no heap
//! - **Generic**: Works with any `Copy` type
//! - **Interior mutability**: Safe access via `Cell` (no runtime borrow checking overhead)
//! - **O(1) allocation**: Free-list based allocation and deallocation
//! - **Mark-and-sweep GC**: Trait-based garbage collection via
//!   [`crate::arena::Trace`]
//! - **Zero dependencies**: Only uses `core::cell::Cell`
//!
//! ## Safety Guarantees
//!
//! This crate uses `#![forbid(unsafe_code)]` — there is no `unsafe` anywhere
//! in the implementation. Interior mutability is achieved through `Cell<T>`
//! rather than raw pointers, and all indices are bounds-checked before access.
//!
//! ## Example
//!
//! ```rust
//! use grift::arena::{Arena, ArenaIndex};
//!
//! #[derive(Clone, Copy, Debug, PartialEq)]
//! enum Node {
//!     Leaf(isize),
//!     Branch(ArenaIndex, ArenaIndex),
//! }
//!
//! let arena: Arena<Node, 1024> = Arena::new(Node::Leaf(0));
//!
//! // Allocate nodes
//! let left = arena.alloc(Node::Leaf(1)).unwrap();
//! let right = arena.alloc(Node::Leaf(2)).unwrap();
//! let root = arena.alloc(Node::Branch(left, right)).unwrap();
//!
//! // Access nodes
//! if let Node::Branch(l, r) = arena.get(root).unwrap() {
//!     println!("Left: {:?}, Right: {:?}", arena.get(l), arena.get(r));
//! }
//!
//! // Free when done
//! arena.free(root).unwrap();
//! ```

use core::cell::Cell;

// ============================================================================
// Core Types
// ============================================================================

/// Index into the arena.
///
/// This is a lightweight wrapper around `usize` that directly indexes
/// the arena's internal array.
///
/// # Safety Note
///
/// Indices should only be obtained from arena operations (`alloc`, `iter`).
/// Manually constructing indices bypasses the type system's protection
/// and should only be used for serialization/deserialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArenaIndex(usize);

/// Single source of truth for every well-known arena slot.
///
/// Generates [`ArenaIndex`] constants, the singleton root set
/// ([`ArenaIndex::ROOTS`]), and [`ArenaIndex::FIRST_FREE`] marking
/// the first user-allocatable slot.
macro_rules! define_singletons {
    (
        $(
            $(#[$meta:meta])*
            $name:ident = $idx:expr,
        )*
        ; FIRST_FREE = $first_free:expr
    ) => {
        impl ArenaIndex {
            $(
                $(#[$meta])*
                pub const $name: ArenaIndex = ArenaIndex($idx);
            )*

            /// The first slot available for user allocation.
            ///
            /// Singletons occupy slots `0..FIRST_FREE`.  A future compaction
            /// pass must never move slots below this boundary because their
            /// [`ArenaIndex`] values are compile-time constants.
            pub const FIRST_FREE: ArenaIndex = ArenaIndex($first_free);

            /// All singleton slots that must survive every GC cycle.
            pub const ROOTS: &'static [ArenaIndex] = &[
                $(ArenaIndex($idx),)*
            ];
        }
    };
}

define_singletons! {
    /// The NIL index - points to slot 0 where `Value::Nil` is pre-allocated.
    ///
    /// This constant is useful as:
    /// - A default/placeholder value in arrays
    /// - Direct access to the Lisp nil value without needing a `Lisp` reference
    /// - A sentinel for "empty" or "none" in data structures
    ///
    /// Since slot 0 always contains `Value::Nil`, accessing this index via
    /// `lisp.get(ArenaIndex::NIL)` returns `Value::Nil`.
    NIL = 0,

    /// The TRUE index - points to slot 1 where `Value::Boolean(true)` is pre-allocated.
    TRUE = 1,

    /// The FALSE index - points to slot 2 where `Value::Boolean(false)` is pre-allocated.
    FALSE = 2,

    /// The INERT index - points to slot 3 where `Value::Inert` is pre-allocated.
    INERT = 3,

    /// The IGNORE index - points to slot 4 where `Value::Ignore` is pre-allocated.
    IGNORE = 4,

    /// The GROUND_ENV index - points to slot 5 where the ground (builtin)
    /// environment is pre-allocated.
    GROUND_ENV = 5,

    /// The GLOBAL_ENV index - points to slot 7 where the global/standard
    /// environment (child of ground) is pre-allocated.  Slot 6 holds the
    /// parents cons cell linking ground to global.
    GLOBAL_ENV = 7,

    /// The GC_ROOTS index - points to slot 8 where the GC root stack head
    /// is stored.  This is a cons cell whose `car` holds the current head
    /// of the GC roots linked list and whose `cdr` is always NIL.
    GC_ROOTS = 8,

    /// The INTERN_LIST index - points to slot 9 where the symbol intern
    /// alist head is stored.  This is a cons cell whose `car` holds the
    /// current head of the intern list and whose `cdr` is always NIL.
    INTERN_LIST = 9,
    ;
    FIRST_FREE = 10
}

impl ArenaIndex {
    /// Return `ArenaIndex::TRUE` if `b` is true, `ArenaIndex::FALSE` otherwise.
    #[inline]
    pub const fn from_bool(b: bool) -> ArenaIndex {
        if b { Self::TRUE } else { Self::FALSE }
    }

    /// Create a new arena index with the given slot index.
    ///
    /// # Warning
    ///
    /// This is a low-level constructor intended for serialization/deserialization.
    /// For normal use, obtain indices from [`Arena::alloc`] or [`Arena::iter`].
    /// Fabricating indices manually may lead to undefined behavior if the
    /// index doesn't correspond to a valid allocation.
    pub const fn new(index: usize) -> Self {
        ArenaIndex(index)
    }

    /// Get the raw slot index value.
    #[inline]
    pub const fn raw(self) -> usize {
        self.0
    }

    /// Check if this is the NIL index (slot 0).
    #[inline]
    pub const fn is_nil(self) -> bool {
        self.0 == 0
    }
}

impl Default for ArenaIndex {
    /// Returns [`ArenaIndex::NIL`] (slot 0).
    fn default() -> Self {
        Self::NIL
    }
}

impl core::fmt::Display for ArenaIndex {
    /// Format the index as `@<slot>` for concise debugging output.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "@{}", self.0)
    }
}

// — ArenaError —

/// Errors that can occur during arena and Lisp operations.
///
/// Each variant captures a specific failure mode, enabling precise
/// diagnostics without heap-allocated error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArenaError {
    /// Arena is full, cannot allocate more cells.
    OutOfMemory,

    /// Index exceeds the arena's capacity (>= N).
    IndexOutOfBounds,

    /// Index refers to a slot that has been freed or was never allocated.
    IndexNotAllocated,

    /// An argument to an arena operation was invalid (e.g., zero-length contiguous allocation).
    InvalidArgument,

    /// An error occurred during garbage collection tracing.
    /// This can happen if the mark stack overflows or roots are invalid.
    TraceError,

    /// Cycle detected during structure traversal (e.g., graph traversal
    /// or recursive data structure operations).
    Cyclic,

    /// A value had the wrong type for the requested operation
    /// (e.g., expected a Number but found a Cons).
    TypeError,

    /// A parse error occurred while reading an S-expression.
    ///
    /// Carries the 1-based line and column where the error was detected.
    ParseError {
        /// 1-based line number in the source text.
        line: u32,
        /// 1-based column number in the source text.
        col: u32,
    },

    /// Checked arithmetic overflowed (e.g., addition, negation).
    ArithmeticOverflow,

    /// Division or modulo by zero.
    DivisionByZero,

    /// A variable was not found in the searched environment chain.
    UnboundVariable,

    /// Attempted to apply a value that is not callable.
    NotCallable,

    /// Attempted to mutate an immutable environment (e.g., the ground environment).
    ImmutableEnvironment,

    /// Attempted to define a variable that already has a binding in the current frame.
    AlreadyDefined,
}

impl ArenaError {
    /// Get a human-readable description of the error.
    pub const fn as_str(&self) -> &'static str {
        match self {
            ArenaError::OutOfMemory => "Arena is full",
            ArenaError::IndexOutOfBounds => "Index out of bounds",
            ArenaError::IndexNotAllocated => "Index not allocated",
            ArenaError::InvalidArgument => "Invalid argument",
            ArenaError::TraceError => "Error during GC tracing",
            ArenaError::Cyclic => "Cycle detected in evaluation",
            ArenaError::TypeError => "Type error",
            ArenaError::ParseError { .. } => "Parse error",
            ArenaError::ArithmeticOverflow => "Arithmetic overflow",
            ArenaError::DivisionByZero => "Division by zero",
            ArenaError::UnboundVariable => "Unbound variable",
            ArenaError::NotCallable => "Not callable",
            ArenaError::ImmutableEnvironment => "Attempt to mutate immutable environment",
            ArenaError::AlreadyDefined => "Variable already defined",
        }
    }

    /// Check if this error indicates the arena is full.
    pub const fn is_out_of_memory(&self) -> bool {
        matches!(self, ArenaError::OutOfMemory)
    }

    /// Check if this error indicates an invalid index (out of bounds or not allocated).
    pub const fn is_invalid_index(&self) -> bool {
        matches!(
            self,
            ArenaError::IndexOutOfBounds | ArenaError::IndexNotAllocated
        )
    }

    /// Check if this error is related to garbage collection.
    pub const fn is_trace_error(&self) -> bool {
        matches!(self, ArenaError::TraceError)
    }
}

impl core::fmt::Display for ArenaError {
    /// Format the error using its short description, preserving line/column
    /// details for parse failures.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ArenaError::ParseError { line, col } => {
                write!(f, "Parse error at line {line}, column {col}")
            }
            other => f.write_str(other.as_str()),
        }
    }
}

/// Result type for arena operations.
pub type ArenaResult<T> = Result<T, ArenaError>;

// — Slot (Internal) —

/// Sentinel value indicating end of free list.
const FREE_LIST_END: usize = usize::MAX;

/// Internal slot representation for free-list based allocation.
///
/// Each slot is either free (storing the next free slot index) or
/// occupied (storing the actual value).
#[derive(Clone, Copy)]
enum Slot<T: Copy> {
    /// Free slot containing index of the next free slot (or FREE_LIST_END).
    Free { next_free: usize },
    /// Occupied slot containing the stored value.
    Occupied { value: T },
}

// ============================================================================
// Statistics
// ============================================================================

/// Statistics returned by garbage collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GcStats {
    /// Number of objects that were marked as reachable.
    pub marked: usize,

    /// Number of objects that were collected (freed).
    pub collected: usize,

    /// Number of objects that existed before collection.
    pub total_before: usize,
}

impl GcStats {
    /// Check if any garbage was collected.
    pub const fn did_collect(&self) -> bool {
        self.collected > 0
    }

    /// Get the number of objects remaining after collection.
    pub const fn remaining(&self) -> usize {
        self.total_before - self.collected
    }

    /// Get the collection ratio (0.0 to 1.0).
    ///
    /// Returns 0.0 if no objects existed before collection.
    pub fn collection_ratio(&self) -> f32 {
        if self.total_before == 0 {
            0.0
        } else {
            self.collected as f32 / self.total_before as f32
        }
    }

    /// Get the survival ratio (0.0 to 1.0).
    ///
    /// Returns 1.0 if no objects existed before collection.
    pub fn survival_ratio(&self) -> f32 {
        if self.total_before == 0 {
            1.0
        } else {
            self.marked as f32 / self.total_before as f32
        }
    }
}

/// Statistics about arena usage.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ArenaStats {
    /// Total capacity of the arena.
    pub capacity: usize,

    /// Number of currently allocated cells.
    pub allocated: usize,

    /// Number of free cells.
    pub free: usize,

    /// Fragmentation ratio (0.0 = not fragmented, 1.0 = highly fragmented).
    pub fragmentation: f32,
}

impl ArenaStats {
    /// Get usage as a percentage (0-100).
    pub fn usage_percent(&self) -> f32 {
        if self.capacity == 0 {
            0.0
        } else {
            (self.allocated as f32 / self.capacity as f32) * 100.0
        }
    }

    /// Get free space as a percentage (0-100).
    pub fn free_percent(&self) -> f32 {
        100.0 - self.usage_percent()
    }

    /// Check if the arena is empty.
    pub const fn is_empty(&self) -> bool {
        self.allocated == 0
    }

    /// Check if the arena is full.
    pub const fn is_full(&self) -> bool {
        self.free == 0
    }

    /// Check if fragmentation is above a threshold.
    pub fn is_fragmented(&self, threshold: f32) -> bool {
        self.fragmentation > threshold
    }
}

// ============================================================================
// Traits
// ============================================================================

/// Trait for types that can be recursively deleted from the arena.
///
/// Implement this for types that contain `ArenaIndex` fields pointing
/// to other allocations that should be freed together.
///
/// # Example
///
/// ```rust
/// use grift::arena::{Arena, ArenaIndex, ArenaDelete, ArenaResult};
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

/// Trait for types that can be deep-copied within the arena.
///
/// Implement this for types containing `ArenaIndex` fields that need
/// to recursively copy their children.
///
/// # Example
///
/// ```rust
/// use grift::arena::{Arena, ArenaIndex, ArenaCopy, ArenaResult};
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

/// Trait for types that can be traced by the garbage collector.
///
/// Implement this for types that contain `ArenaIndex` fields. The GC will
/// call `trace` to discover all reachable objects starting from the roots.
///
/// # Example
///
/// ```rust
/// use grift::arena::{Arena, ArenaIndex, Trace};
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
    fn trace_with_arena<F: FnMut(ArenaIndex)>(&self, _arena: &Arena<T, N>, tracer: F) {
        self.trace(tracer);
    }
}

// ============================================================================
// Iterator
// ============================================================================

/// Iterator over allocated cells in the arena.
///
/// Created by [`Arena::iter`]. Visits slots in index order (0..N),
/// skipping free slots. The iterator borrows the arena immutably, so
/// allocations and frees must not occur while iterating.
pub struct ArenaIterator<'a, T: Copy, const N: usize> {
    arena: &'a Arena<T, N>,
    current: usize,
}

impl<T: Copy, const N: usize> Iterator for ArenaIterator<'_, T, N> {
    type Item = (ArenaIndex, T);

    #[inline]
    /// Return the next occupied slot in ascending index order.
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

// ============================================================================
// Arena
// ============================================================================

/// Fixed-size arena allocator with O(1) allocation.
///
/// # Type Parameters
///
/// - `T`: The type of values stored (must be `Copy` for array initialization)
/// - `N`: Maximum number of cells (const generic)
///
/// # Memory Layout
///
/// - `slots`: Array of `Cell<Slot<T>>` (either free with next pointer, or occupied with value)
/// - `free_head`: Head of the free list
/// - `len`: Number of currently allocated slots
///
/// # O(1) Allocation
///
/// Uses a free-list for constant-time allocation and deallocation instead of
/// scanning a bitmap.
///
/// # Garbage Collection
///
/// The arena supports mark-and-sweep garbage collection via the
/// [`crate::arena::Trace`] trait.
///
/// # Performance
///
/// Uses `Cell` instead of `RefCell` for interior mutability. This eliminates
/// runtime borrow checking overhead and the risk of borrow panics, while
/// still maintaining safe Rust guarantees.
pub struct Arena<T: Copy, const N: usize> {
    slots: [Cell<Slot<T>>; N],
    free_head: Cell<usize>,
    len: Cell<usize>,
}

impl<T: Copy, const N: usize> Arena<T, N> {
    /// Create a new arena.
    ///
    /// All slots start as free, linked together in a free list.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift::arena::Arena;
    ///
    /// let arena: Arena<isize, 100> = Arena::new(0);
    /// ```
    pub fn new(_default_value: T) -> Self {
        // Initialize all slots as free, linked together
        // Slot 0 -> 1 -> 2 -> ... -> N-1 -> FREE_LIST_END
        let slots: [Cell<Slot<T>>; N] = core::array::from_fn(|i| {
            Cell::new(Slot::Free {
                next_free: if i + 1 < N { i + 1 } else { FREE_LIST_END },
            })
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
    /// use grift::arena::Arena;
    ///
    /// let arena: Arena<isize, 10> = Arena::new(0);
    /// let idx = arena.alloc(42).unwrap();
    /// assert_eq!(arena.get(idx).unwrap(), 42);
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
        let next_free = match self.slots[idx].get() {
            Slot::Free { next_free } => next_free,
            Slot::Occupied { .. } => unreachable!("free_head pointed to occupied slot"),
        };

        // Mark as occupied
        self.slots[idx].set(Slot::Occupied { value });
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
        match self.slots[idx].get() {
            Slot::Occupied { .. } => Ok(idx),
            Slot::Free { .. } => Err(ArenaError::IndexNotAllocated),
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
        match self.slots[idx].get() {
            Slot::Occupied { value } => Ok(value),
            Slot::Free { .. } => Err(ArenaError::IndexNotAllocated),
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

        self.slots[idx].set(Slot::Occupied { value });
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
    /// use grift::arena::Arena;
    ///
    /// let arena: Arena<isize, 10> = Arena::new(0);
    /// let idx = arena.alloc(42).unwrap();
    ///
    /// arena.modify(idx, |v| *v += 10).unwrap();
    /// assert_eq!(arena.get(idx).unwrap(), 52);
    /// ```
    pub fn modify<F>(&self, index: ArenaIndex, f: F) -> ArenaResult<()>
    where
        F: FnOnce(&mut T),
    {
        let idx = self.validate_index(index)?;
        let Slot::Occupied { mut value } = self.slots[idx].get() else {
            unreachable!("validate_index guarantees slot is Occupied")
        };
        f(&mut value);
        self.slots[idx].set(Slot::Occupied { value });
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
            // Both validated as Occupied
            let val_a = self.get(a)?;
            let val_b = self.get(b)?;
            self.slots[idx_a].set(Slot::Occupied { value: val_b });
            self.slots[idx_b].set(Slot::Occupied { value: val_a });
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
        let Slot::Occupied { value: old } = self.slots[idx].get() else {
            unreachable!("validate_index guarantees slot is Occupied")
        };
        self.slots[idx].set(Slot::Occupied { value });
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
    /// use grift::arena::Arena;
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
        let free_head = self.free_head.get();
        self.slots[idx].set(Slot::Free {
            next_free: free_head,
        });
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
            self.slots[i].set(Slot::Free {
                next_free: if i + 1 < N { i + 1 } else { FREE_LIST_END },
            });
        }

        self.free_head.set(if N > 0 { 0 } else { FREE_LIST_END });
        self.len.set(0);
    }

    /// Iterate over all allocated indices and values.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift::arena::Arena;
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

    /// Estimate fragmentation as the number of free-space runs divided by the
    /// total arena capacity.
    fn calculate_fragmentation(&self) -> f32 {
        // Count free-space fragments: contiguous runs of free slots.
        let (fragments, _) = (0..N).fold((0u32, false), |(count, was_free), i| {
            let is_free = matches!(self.slots[i].get(), Slot::Free { .. });
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
            .filter(|&i| matches!(self.slots[i].get(), Slot::Occupied { .. }))
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

            match self.slots[current].get() {
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
        for (i, &was_visited) in visited.iter().enumerate().take(N) {
            if matches!(self.slots[i].get(), Slot::Free { .. }) && !was_visited {
                return false; // Free slot not in free list
            }
        }

        true
    }

    /// Check if a slot index is currently occupied.
    ///
    /// This is a low-level debugging method. For normal use, prefer [`is_allocated`](Arena::is_allocated).
    pub fn is_slot_occupied(&self, slot_index: usize) -> bool {
        slot_index < N && matches!(self.slots[slot_index].get(), Slot::Occupied { .. })
    }

    /// Get indices of all allocated slots.
    ///
    /// Returns an array with the first `len()` elements being valid indices.
    /// The remaining elements are [`ArenaIndex::NIL`].
    pub fn allocated_indices(&self) -> [ArenaIndex; N] {
        let mut result = [ArenaIndex::NIL; N];
        let mut count = 0;

        for idx in 0..N {
            if let Slot::Occupied { .. } = self.slots[idx].get() {
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
            if let Slot::Occupied { mut value } = self.slots[idx].get() {
                f(ArenaIndex::new(idx), &mut value);
                self.slots[idx].set(Slot::Occupied { value });
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

// ============================================================================
// Garbage Collection
// ============================================================================

impl<T: Copy, const N: usize> Arena<T, N> {
    /// Initialize roots into the mark stack.
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
                loop {
                    let mut batch = [0usize; 16];
                    let mut batch_count = 0usize;
                    let mut has_more = false;

                    value.trace_with_arena(self, |child_index| {
                        let idx = child_index.raw();
                        if idx < N && !marked[idx] {
                            if batch_count < 16 {
                                batch[batch_count] = idx;
                                batch_count += 1;
                            } else {
                                has_more = true;
                            }
                        }
                    });

                    if batch_count == 0 {
                        break;
                    }

                    for &idx in batch.iter().take(batch_count) {
                        if !marked[idx] {
                            marked[idx] = true;
                            if *stack_len < N {
                                mark_stack[*stack_len] = idx;
                                *stack_len += 1;
                            }
                        }
                    }

                    if !has_more {
                        break;
                    }
                }
            }
        }
    }

    /// Sweep phase: free all unmarked but allocated slots in a single pass.
    fn sweep_unmarked(&self, marked: &[bool; N]) -> usize {
        (0..N)
            .filter(|&idx| !marked[idx] && matches!(self.slots[idx].get(), Slot::Occupied { .. }))
            .map(|idx| self.free(ArenaIndex::new(idx)))
            .filter(core::result::Result::is_ok)
            .count()
    }

    /// Shared mark-and-sweep implementation.
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
    /// use grift::arena::{Arena, ArenaIndex, Trace};
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
