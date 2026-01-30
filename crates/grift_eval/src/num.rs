//! Numeric types and helpers for mixed arithmetic.

// ============================================================================
// Float helper functions (no_std compatible)
// ============================================================================

/// Get the fractional part of a float (no_std compatible)
#[inline]
pub fn fract_f64(x: f64) -> f64 {
    x - trunc_f64_helper(x)
}

/// Truncate a float to integer (no_std compatible)
#[inline]
pub fn trunc_f64_helper(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        x
    } else {
        x as i64 as f64
    }
}

/// Get the absolute value of a float (no_std compatible)
#[inline]
pub fn abs_f64(x: f64) -> f64 {
    if x < 0.0 { -x } else { x }
}

// ============================================================================
// Numeric Type for Mixed Arithmetic
// ============================================================================

/// Numeric value that can be either an integer or a float.
/// Used internally for arithmetic operations with type promotion.
#[derive(Clone, Copy, Debug)]
pub enum Num {
    Int(isize),
    Float(f64),
}

impl Num {
    /// Convert to f64
    #[inline]
    pub fn to_f64(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(f) => f,
        }
    }
    
    /// Check if this is an integer
    #[inline]
    pub fn is_int(self) -> bool {
        matches!(self, Num::Int(_))
    }
    
    /// Get as isize if it's an integer
    #[inline]
    pub fn as_int(self) -> Option<isize> {
        match self {
            Num::Int(i) => Some(i),
            Num::Float(_) => None,
        }
    }
}
