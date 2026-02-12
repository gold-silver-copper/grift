//! Internal macros for the arena allocator.
//!
//! This module contains macros for:
//! - Generating contiguous get/set operations (`impl_get_contiguous!`, `impl_set_contiguous!`)

// ============================================================================
// Macros for generating contiguous get/set operations
// ============================================================================

/// Internal macro to generate get_contiguousN methods.
/// Reduces ~170 lines of repetitive code to ~10 lines of macro invocations.
macro_rules! impl_get_contiguous {
    // Base case with explicit indices for each variable
    ($fn_name:ident, [$($idx:expr => $var:ident),+ $(,)?]) => {
        pub fn $fn_name(&self, start: ArenaIndex) -> ArenaResult<( $( impl_get_contiguous!(@T $var) ),+ )> {
            let base = start.raw();
            // Bounds check: last index must be < N
            let max_offset = impl_get_contiguous!(@max $($idx),+);
            if base + max_offset >= N {
                return Err(ArenaError::InvalidIndex);
            }
            $(
                let $var = match self.slots[base + $idx].get() {
                    Slot::Occupied { value } => value,
                    Slot::Free { .. } => return Err(ArenaError::InvalidIndex),
                };
            )+
            Ok(( $($var),+ ))
        }
    };
    // Helper to generate T for tuple type
    (@T $var:ident) => { T };
    // Helper to get max of indices
    (@max $first:expr $(, $rest:expr)*) => {
        impl_get_contiguous!(@max_impl $first $(, $rest)*)
    };
    (@max_impl $val:expr) => { $val };
    (@max_impl $val:expr, $($rest:expr),+) => {{
        let a = $val;
        let b = impl_get_contiguous!(@max_impl $($rest),+);
        if a > b { a } else { b }
    }};
}

/// Internal macro to generate set_contiguousN methods.
/// Reduces ~110 lines of repetitive code to ~10 lines of macro invocations.
macro_rules! impl_set_contiguous {
    ($fn_name:ident, [$($idx:expr => $var:ident),+ $(,)?]) => {
        #[allow(clippy::too_many_arguments)]
        pub fn $fn_name(&self, start: ArenaIndex, $($var: T),+) -> ArenaResult<()> {
            let base = start.raw();
            // Bounds check: last index must be < N
            let max_offset = impl_set_contiguous!(@max $($idx),+);
            if base + max_offset >= N {
                return Err(ArenaError::InvalidIndex);
            }
            // Verify all slots are occupied first
            $(
                if !matches!(self.slots[base + $idx].get(), Slot::Occupied { .. }) {
                    return Err(ArenaError::InvalidIndex);
                }
            )+
            // Set all values
            $(
                self.slots[base + $idx].set(Slot::Occupied { value: $var });
            )+
            Ok(())
        }
    };
    // Helper to get max of indices (same as get)
    (@max $first:expr $(, $rest:expr)*) => {
        impl_set_contiguous!(@max_impl $first $(, $rest)*)
    };
    (@max_impl $val:expr) => { $val };
    (@max_impl $val:expr, $($rest:expr),+) => {{
        let a = $val;
        let b = impl_set_contiguous!(@max_impl $($rest),+);
        if a > b { a } else { b }
    }};
}

// Re-export the internal macros for use in arena.rs
pub(crate) use impl_get_contiguous;
pub(crate) use impl_set_contiguous;
