//! Statistics types for arena usage monitoring.

// ============================================================================
// GcStats
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

// ============================================================================
// ArenaStats
// ============================================================================

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
