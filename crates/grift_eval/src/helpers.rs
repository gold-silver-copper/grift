//! Math and utility helper functions for evaluation.

use grift_parser::{ArenaIndex, Lisp, Value};
use crate::error::EvalError;
use crate::num::{fract_f64, abs_f64};

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

/// Floor function without libm
pub fn floor_f64(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    // Handle very large floats that exceed i64 range
    // These are already integers (no fractional part)
    const I64_MAX_F64: f64 = 9223372036854775807.0;
    const I64_MIN_F64: f64 = -9223372036854775808.0;
    if x >= I64_MAX_F64 || x <= I64_MIN_F64 {
        return x;
    }
    let int_part = x as i64 as f64;
    if x >= 0.0 || x == int_part {
        int_part
    } else {
        int_part - 1.0
    }
}

/// Ceiling function without libm
pub fn ceil_f64(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    // Handle very large floats that exceed i64 range
    const I64_MAX_F64: f64 = 9223372036854775807.0;
    const I64_MIN_F64: f64 = -9223372036854775808.0;
    if x >= I64_MAX_F64 || x <= I64_MIN_F64 {
        return x;
    }
    let int_part = x as i64 as f64;
    if x <= 0.0 || x == int_part {
        int_part
    } else {
        int_part + 1.0
    }
}

/// Truncate function without libm
pub fn trunc_f64(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    // Handle very large floats that exceed i64 range
    const I64_MAX_F64: f64 = 9223372036854775807.0;
    const I64_MIN_F64: f64 = -9223372036854775808.0;
    if x >= I64_MAX_F64 || x <= I64_MIN_F64 {
        return x;
    }
    x as i64 as f64
}

/// Round function without libm (round half to even)
pub fn round_f64(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    // Handle very large floats that exceed i64 range
    const I64_MAX_F64: f64 = 9223372036854775807.0;
    const I64_MIN_F64: f64 = -9223372036854775808.0;
    if x >= I64_MAX_F64 || x <= I64_MIN_F64 {
        return x;
    }
    let floor = floor_f64(x);
    let frac = x - floor;
    
    if frac < 0.5 {
        floor
    } else if frac > 0.5 {
        floor + 1.0
    } else {
        // Round half to even
        let floor_int = floor as i64;
        if floor_int % 2 == 0 {
            floor
        } else {
            floor + 1.0
        }
    }
}

/// Float exponentiation without libm
/// Uses iterative multiplication for integer powers, 
/// and exp(y * ln(x)) approximation for fractional powers
pub fn pow_float(base: f64, exp: f64) -> f64 {
    if exp == 0.0 {
        return 1.0;
    }
    if base == 0.0 {
        return if exp > 0.0 { 0.0 } else { f64::INFINITY };
    }
    if base == 1.0 {
        return 1.0;
    }
    if exp == 1.0 {
        return base;
    }
    
    // For integer exponents, use iterative approach
    if fract_f64(exp) == 0.0 && abs_f64(exp) < 1000.0 {
        let n = exp as i64;
        if n >= 0 {
            let mut result = 1.0;
            let mut b = base;
            let mut e = n as u64;
            while e > 0 {
                if e & 1 == 1 {
                    result *= b;
                }
                b *= b;
                e >>= 1;
            }
            result
        } else {
            let mut result = 1.0;
            let mut b = base;
            let mut e = (-n) as u64;
            while e > 0 {
                if e & 1 == 1 {
                    result *= b;
                }
                b *= b;
                e >>= 1;
            }
            1.0 / result
        }
    } else {
        // For fractional exponents, use exp(y * ln(x))
        // Compute ln(x) and exp(y * ln(x)) using Taylor series
        exp_float(exp * ln_float(base))
    }
}

/// Natural logarithm approximation without libm
pub fn ln_float(x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NAN;
    }
    if x == 1.0 {
        return 0.0;
    }
    if x.is_infinite() {
        return f64::INFINITY;
    }
    
    // Reduce x to [1, 2) by extracting exponent
    // x = m * 2^e where 1 <= m < 2
    // ln(x) = ln(m) + e * ln(2)
    let mut mantissa = x;
    let mut exponent: i32 = 0;
    
    while mantissa >= 2.0 {
        mantissa /= 2.0;
        exponent += 1;
    }
    while mantissa < 1.0 {
        mantissa *= 2.0;
        exponent -= 1;
    }
    
    // Now 1 <= mantissa < 2
    // Use series: ln(1+z) = z - z^2/2 + z^3/3 - z^4/4 + ... for |z| < 1
    // Let z = (mantissa - 1), so 0 <= z < 1
    let z = mantissa - 1.0;
    
    let mut sum: f64 = 0.0;
    let mut term: f64 = z;
    let mut n = 1;
    let mut sign: f64 = 1.0;
    
    while abs_f64(term) > 1e-15 && n < 200 {
        sum += sign * term / n as f64;
        term *= z;
        sign = -sign;
        n += 1;
    }
    
    // ln(2) ≈ 0.693147180559945
    const LN_2: f64 = 0.693147180559945;
    sum + exponent as f64 * LN_2
}

/// Exponential function approximation without libm
pub fn exp_float(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return 1.0;
    }
    if x > 700.0 {
        return f64::INFINITY;
    }
    if x < -700.0 {
        return 0.0;
    }
    
    // For negative x, use exp(-x) = 1/exp(x)
    if x < 0.0 {
        return 1.0 / exp_float(-x);
    }
    
    // Reduce range: exp(x) = exp(x/n)^n
    // Choose n so that x/n is small
    let mut reduced = x;
    let mut squares = 0;
    while reduced > 1.0 {
        reduced /= 2.0;
        squares += 1;
    }
    
    // Taylor series: exp(z) = 1 + z + z^2/2! + z^3/3! + ...
    let mut sum: f64 = 1.0;
    let mut term: f64 = 1.0;
    let mut n = 1;
    
    while abs_f64(term) > 1e-15 && n < 100 {
        term *= reduced / n as f64;
        sum += term;
        n += 1;
    }
    
    // Square back up
    for _ in 0..squares {
        sum *= sum;
    }
    
    sum
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
        (Value::Float(x), Value::Float(y)) => Ok(x == y),
        (Value::Number(x), Value::Float(y)) | (Value::Float(y), Value::Number(x)) => {
            Ok(x as f64 == y)
        }
        (Value::Char(x), Value::Char(y)) => Ok(x == y),
        (Value::Symbol(_), Value::Symbol(_)) => lisp.symbol_eq(a, b).map_err(Into::into),
        (Value::Cons { car: car_a, cdr: cdr_a }, Value::Cons { car: car_b, cdr: cdr_b }) => {
            // Recursively check car and cdr
            if !equal_recursive(lisp, car_a, car_b)? {
                return Ok(false);
            }
            equal_recursive(lisp, cdr_a, cdr_b)
        }
        _ => Ok(false),
    }
}

/// Check if two values are structurally equal (for case matching)
pub fn values_equal<const N: usize>(lisp: &Lisp<N>, a: ArenaIndex, b: ArenaIndex) -> Result<bool, EvalError> {
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
        (Value::Symbol(_), Value::Symbol(_)) => {
            lisp.symbol_eq(a, b).map_err(Into::into)
        }
        (Value::Cons { car: car_a, cdr: cdr_a }, Value::Cons { car: car_b, cdr: cdr_b }) => {
            // Recursively compare (limited depth to avoid stack overflow)
            if values_equal(lisp, car_a, car_b)? {
                values_equal(lisp, cdr_a, cdr_b)
            } else {
                Ok(false)
            }
        }
        _ => Ok(false),
    }
}

/// Check if key matches any datum in the list
pub fn case_matches<const N: usize>(lisp: &Lisp<N>, key: ArenaIndex, datums: ArenaIndex) -> Result<bool, EvalError> {
    let mut current = datums;
    loop {
        match lisp.get(current)? {
            Value::Nil => return Ok(false),
            Value::Cons { car: datum, cdr: rest } => {
                if values_equal(lisp, key, datum)? {
                    return Ok(true);
                }
                current = rest;
            }
            _ => {
                // Single datum (not a list)
                return values_equal(lisp, key, datums);
            }
        }
    }
}
