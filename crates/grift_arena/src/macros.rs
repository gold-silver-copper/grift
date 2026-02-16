//! Internal macros for the arena allocator.
//!
//! This module contains macros for:
//! - Generating contiguous get/set operations (`impl_get_contiguous!`, `impl_set_contiguous!`)

// — Macros for generating contiguous get/set operations —

/// Compile-time maximum of a list of `usize` expressions.
macro_rules! const_max {
    ($val:expr) => { $val };
    ($val:expr, $($rest:expr),+) => {{
        let a = $val;
        let b = const_max!($($rest),+);
        if a > b { a } else { b }
    }};
}

/// Internal macro to generate get_contiguousN methods.
/// Reduces ~170 lines of repetitive code to ~10 lines of macro invocations.
macro_rules! impl_get_contiguous {
    // Base case with explicit indices for each variable
    ($fn_name:ident, [$($idx:expr => $var:ident),+ $(,)?]) => {
        pub fn $fn_name(&self, start: ArenaIndex) -> ArenaResult<( $( impl_get_contiguous!(@T $var) ),+ )> {
            let base = start.raw();
            // Bounds check: last index must be < N
            if base + const_max!($($idx),+) >= N {
                return Err(ArenaError::IndexOutOfBounds);
            }
            $(
                let $var = match self.slots[base + $idx].get() {
                    Slot::Occupied { value } => value,
                    Slot::Free { .. } => return Err(ArenaError::IndexNotAllocated),
                };
            )+
            Ok(( $($var),+ ))
        }
    };
    // Helper to generate T for tuple type
    (@T $var:ident) => { T };
}

/// Internal macro to generate set_contiguousN methods.
/// Reduces ~110 lines of repetitive code to ~10 lines of macro invocations.
macro_rules! impl_set_contiguous {
    ($fn_name:ident, [$($idx:expr => $var:ident),+ $(,)?]) => {
        #[allow(clippy::too_many_arguments)]
        pub fn $fn_name(&self, start: ArenaIndex, $($var: T),+) -> ArenaResult<()> {
            let base = start.raw();
            // Bounds check: last index must be < N
            if base + const_max!($($idx),+) >= N {
                return Err(ArenaError::IndexOutOfBounds);
            }
            // Verify all slots are occupied first
            $(
                if !matches!(self.slots[base + $idx].get(), Slot::Occupied { .. }) {
                    return Err(ArenaError::IndexNotAllocated);
                }
            )+
            // Set all values
            $(
                self.slots[base + $idx].set(Slot::Occupied { value: $var });
            )+
            Ok(())
        }
    };
}

// Re-export the internal macros for use in arena.rs
pub(crate) use const_max;
pub(crate) use impl_get_contiguous;
pub(crate) use impl_set_contiguous;
