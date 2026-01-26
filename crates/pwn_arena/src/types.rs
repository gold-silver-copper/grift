//! Core types for the arena allocator.
//!
//! This module contains the fundamental types used throughout the arena:
//! - [`ArenaIndex`] - Generational index into the arena
//! - [`ArenaError`] - Error types for arena operations
//! - [`ArenaResult`] - Result type alias
//! - [`Slot`] - Internal slot representation

// ============================================================================
// ArenaIndex
// ============================================================================

/// Index into the arena with generational tracking.
///
/// This is a type-safe wrapper that stores both the slot index and the
/// generation at which it was allocated. This prevents the ABA problem
/// where a freed and reallocated slot could be accessed by a stale index.
///
/// # Safety Note
///
/// Indices should only be obtained from arena operations (`alloc`, `iter`).
/// Manually constructing indices with [`ArenaIndex::new`] bypasses the type
/// system's protection and should only be used for serialization/deserialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArenaIndex {
    /// Slot index (limited to 2^32 - 1 = ~4 billion cells).
    index: u32,
    generation: u32,
}

impl ArenaIndex {
    /// A sentinel "null" index that is never valid.
    ///
    /// This can be used as a placeholder when an optional index is needed
    /// but `Option<ArenaIndex>` is not desired.
    pub const NULL: ArenaIndex = ArenaIndex {
        index: u32::MAX,
        generation: u32::MAX,
    };

    /// Create a new arena index with the given slot index and generation.
    ///
    /// # Warning
    ///
    /// This is a low-level constructor intended for serialization/deserialization.
    /// For normal use, obtain indices from [`Arena::alloc`] or [`Arena::iter`].
    /// Fabricating indices manually may lead to undefined behavior if the
    /// index/generation pair doesn't correspond to a valid allocation.
    ///
    /// # Note
    ///
    /// The index is stored as `u32` internally. Values larger than `u32::MAX`
    /// will be truncated. Arena capacity is limited to 2^32 cells.
    #[inline]
    pub const fn new(index: usize, generation: u32) -> Self {
        ArenaIndex { index: index as u32, generation }
    }

    /// Get the raw slot index value.
    #[inline]
    pub const fn raw(self) -> usize {
        self.index as usize
    }

    /// Get the generation this index was created with.
    #[inline]
    pub const fn generation(self) -> u32 {
        self.generation
    }

    /// Check if this is the null index.
    #[inline]
    pub const fn is_null(self) -> bool {
        self.index == u32::MAX && self.generation == u32::MAX
    }
}

impl Default for ArenaIndex {
    /// Returns [`ArenaIndex::NULL`].
    fn default() -> Self {
        Self::NULL
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

    /// Invalid index (out of bounds, not allocated, or stale generation).
    InvalidIndex,

    /// The index's generation doesn't match the slot's current generation.
    /// This indicates a use-after-free attempt (ABA problem).
    GenerationMismatch,

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
            ArenaError::GenerationMismatch => "stale index (generation mismatch)",
            ArenaError::TraceError => "error during GC tracing",
        }
    }

    /// Check if this error indicates the arena is full.
    pub const fn is_out_of_memory(&self) -> bool {
        matches!(self, ArenaError::OutOfMemory)
    }

    /// Check if this error indicates an invalid or stale index.
    pub const fn is_invalid_index(&self) -> bool {
        matches!(self, ArenaError::InvalidIndex | ArenaError::GenerationMismatch)
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
