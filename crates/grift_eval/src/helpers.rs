//! Math and utility helper functions for evaluation.

use grift_parser::{ArenaIndex, Lisp, Value};
use crate::error::EvalError;

// ============================================================================
// Pure math helper functions
// ============================================================================

/// Helper for computing GCD using Euclidean algorithm
pub fn gcd_helper(mut a: isize, mut b: isize) -> isize {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.abs()
}

/// Helper for integer exponentiation (base^power) with overflow checking
pub fn int_pow(base: isize, power: usize) -> isize {
    if power == 0 {
        return 1;
    }
    if base == 0 {
        return 0;
    }
    if base == 1 {
        return 1;
    }
    if base == -1 {
        return if power % 2 == 0 { 1 } else { -1 };
    }
    
    // Use exponentiation by squaring with saturating operations
    let mut result: isize = 1;
    let mut base = base;
    let mut exp = power;
    
    while exp > 0 {
        if exp % 2 == 1 {
            result = result.saturating_mul(base);
        }
        exp /= 2;
        if exp > 0 {
            base = base.saturating_mul(base);
        }
    }
    result
}

/// Recursive structural equality for equal? predicate
pub fn equal_recursive<const N: usize>(lisp: &Lisp<N>, a: ArenaIndex, b: ArenaIndex) -> Result<bool, EvalError> {
    // Check if they're the same index first
    if a == b {
        return Ok(true);
    }
    
    let val_a = lisp.get(a)?;
    let val_b = lisp.get(b)?;
    
    match (val_a, val_b) {
        (Value::Nil, Value::Nil) => Ok(true),
        (Value::True, Value::True) => Ok(true),
        (Value::False, Value::False) => Ok(true),
        (Value::Number(x), Value::Number(y)) => Ok(x == y),
        (Value::Char(x), Value::Char(y)) => Ok(x == y),
        (Value::Symbol(_), Value::Symbol(_)) => lisp.symbol_eq(a, b).map_err(Into::into),
        (Value::String { .. }, Value::String { .. }) => {
            lisp.string_eq_contiguous(a, b).map_err(Into::into)
        }
        (Value::Cons { .. }, Value::Cons { .. }) => {
            // Recursively check car and cdr
            let (car_a, cdr_a) = lisp.car_cdr(a)?;
            let (car_b, cdr_b) = lisp.car_cdr(b)?;
            if !equal_recursive(lisp, car_a, car_b)? {
                return Ok(false);
            }
            equal_recursive(lisp, cdr_a, cdr_b)
        }
        _ => Ok(false),
    }
}


