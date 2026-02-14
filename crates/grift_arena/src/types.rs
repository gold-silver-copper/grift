//! Core types for the arena allocator.
//!
//! This module contains the fundamental types used throughout the arena:
//! - [`ArenaIndex`] - Index into the arena
//! - [`ArenaError`] - Error types for arena operations
//! - [`ArenaResult`] - Result type alias
//! - [`Slot`] - Internal slot representation

// ============================================================================
// ArenaIndex
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

impl ArenaIndex {
    /// The NIL index - points to slot 0 where `Value::Nil` is pre-allocated.
    ///
    /// This constant is useful as:
    /// - A default/placeholder value in arrays
    /// - Direct access to the Lisp nil value without needing a `Lisp` reference
    /// - A sentinel for "empty" or "none" in data structures
    ///
    /// Since slot 0 always contains `Value::Nil`, accessing this index via
    /// `lisp.get(ArenaIndex::NIL)` returns `Value::Nil`.
    pub const NIL: ArenaIndex = ArenaIndex(0);

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
    pub const fn raw(self) -> usize {
        self.0
    }

    /// Check if this is the NIL index (slot 0).
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

// ============================================================================
// ArenaError
// ============================================================================

/// Errors that can occur during arena operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArenaError {
    /// Arena is full, cannot allocate more cells.
    OutOfMemory,

    /// Invalid index (out of bounds or not allocated).
    InvalidIndex,

    /// An error occurred during garbage collection tracing.
    /// This can happen if the mark stack overflows or roots are invalid.
    TraceError,
}

impl ArenaError {
    /// Get a human-readable description of the error.
    pub const fn as_str(&self) -> &'static str {
        match self {
            ArenaError::OutOfMemory => "arena is full",
            ArenaError::InvalidIndex => "invalid index",
            ArenaError::TraceError => "error during GC tracing",
        }
    }

    /// Check if this error indicates the arena is full.
    pub const fn is_out_of_memory(&self) -> bool {
        matches!(self, ArenaError::OutOfMemory)
    }

    /// Check if this error indicates an invalid index.
    pub const fn is_invalid_index(&self) -> bool {
        matches!(self, ArenaError::InvalidIndex)
    }

    /// Check if this error is related to garbage collection.
    pub const fn is_trace_error(&self) -> bool {
        matches!(self, ArenaError::TraceError)
    }
}

/// Result type for arena operations.
pub type ArenaResult<T> = Result<T, ArenaError>;

// ============================================================================
// Slot (Internal)
// ============================================================================

/// Sentinel value indicating end of free list.
pub(crate) const FREE_LIST_END: usize = usize::MAX;

/// Internal slot representation for free-list based allocation.
///
/// Each slot is either free (storing the next free slot index) or
/// occupied (storing the actual value).
#[derive(Clone, Copy)]
pub(crate) enum Slot<T: Copy> {
    /// Free slot containing index of the next free slot (or FREE_LIST_END).
    Free { next_free: usize },
    /// Occupied slot containing the stored value.
    Occupied { value: T },
}
