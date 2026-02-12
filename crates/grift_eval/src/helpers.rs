//! Math and utility helper functions for evaluation.

use grift_parser::{ArenaIndex, Lisp, Value, fsize};
use crate::error::EvalError;

// ============================================================================
// Pure math helper functions
// ============================================================================

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
        return if power.is_multiple_of(2) { 1 } else { -1 };
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

/// Maximum recursion depth for structural equality checks.
///
/// This prevents stack overflow when comparing circular structures
/// created via `set-car!` / `set-cdr!`.  If the depth limit is
/// exceeded the comparison returns `false` rather than diverging.
const EQUAL_MAX_DEPTH: usize = 10_000;

/// Recursive structural equality for equal? predicate
pub fn equal_recursive<const N: usize>(lisp: &Lisp<N>, a: ArenaIndex, b: ArenaIndex) -> Result<bool, EvalError> {
    equal_recursive_depth(lisp, a, b, 0)
}

/// Inner recursion with depth tracking for circular structure detection.
fn equal_recursive_depth<const N: usize>(
    lisp: &Lisp<N>,
    a: ArenaIndex,
    b: ArenaIndex,
    depth: usize,
) -> Result<bool, EvalError> {
    // Guard against circular structures
    if depth > EQUAL_MAX_DEPTH {
        return Ok(false);
    }

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
        (Value::Number(x), Value::Float(y)) => Ok((x as fsize) == y),
        (Value::Float(x), Value::Number(y)) => Ok(x == (y as fsize)),
        (Value::Char(x), Value::Char(y)) => Ok(x == y),
        (Value::Symbol(_), Value::Symbol(_)) => lisp.symbol_eq(a, b).map_err(Into::into),
        (Value::String { .. }, Value::String { .. }) => {
            lisp.string_eq_contiguous(a, b).map_err(Into::into)
        }
        (Value::Cons { .. }, Value::Cons { .. }) => {
            // Recursively check car and cdr
            let (car_a, cdr_a) = lisp.car_cdr(a)?;
            let (car_b, cdr_b) = lisp.car_cdr(b)?;
            if !equal_recursive_depth(lisp, car_a, car_b, depth + 1)? {
                return Ok(false);
            }
            equal_recursive_depth(lisp, cdr_a, cdr_b, depth + 1)
        }
        (Value::Bytevector { len: len_a, .. }, Value::Bytevector { len: len_b, .. }) => {
            if len_a != len_b {
                return Ok(false);
            }
            for i in 0..len_a {
                let byte_a = lisp.bytevector_get(a, i)?;
                let byte_b = lisp.bytevector_get(b, i)?;
                let va = lisp.get(byte_a)?;
                let vb = lisp.get(byte_b)?;
                match (va, vb) {
                    (Value::Number(x), Value::Number(y)) => {
                        if x != y { return Ok(false); }
                    }
                    _ => return Ok(false),
                }
            }
            Ok(true)
        }
        (Value::Array { len: len_a, .. }, Value::Array { len: len_b, .. }) => {
            if len_a != len_b {
                return Ok(false);
            }
            for i in 0..len_a {
                let elem_a = lisp.array_get(a, i)?;
                let elem_b = lisp.array_get(b, i)?;
                if !equal_recursive_depth(lisp, elem_a, elem_b, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Value::Rational { num: n1, denom: d1 }, Value::Rational { num: n2, denom: d2 }) => {
            Ok(n1 * d2 == n2 * d1)
        }
        (Value::Rational { num, denom }, Value::Number(y)) => {
            Ok(num == y * denom)
        }
        (Value::Number(x), Value::Rational { num, denom }) => {
            Ok(x * denom == num)
        }
        (Value::Rational { num, denom }, Value::Float(y)) => {
            Ok((num as fsize / denom as fsize) == y)
        }
        (Value::Float(x), Value::Rational { num, denom }) => {
            Ok(x == (num as fsize / denom as fsize))
        }
        _ => Ok(false),
    }
}

// ============================================================================
// Float math helper functions (no_std compatible)
// ============================================================================

/// Natural logarithm of 2, used in float math helpers.
#[cfg(target_pointer_width = "64")]
const LN_2: fsize = core::f64::consts::LN_2;
#[cfg(target_pointer_width = "32")]
const LN_2: fsize = core::f32::consts::LN_2;

/// Euler's number (e), used in float math helpers.
#[cfg(target_pointer_width = "64")]
const E: fsize = core::f64::consts::E;
#[cfg(target_pointer_width = "32")]
const E: fsize = core::f32::consts::E;

/// Floor function for no_std: largest integer value not greater than x.
pub fn float_floor(x: fsize) -> fsize {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    let i = x as isize;
    let fi = i as fsize;
    if x < fi { fi - 1.0 } else { fi }
}

/// Ceiling function for no_std: smallest integer value not less than x.
pub fn float_ceil(x: fsize) -> fsize {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    let i = x as isize;
    let fi = i as fsize;
    if x > fi { fi + 1.0 } else { fi }
}

/// Truncate function for no_std: integer part of x towards zero.
pub fn float_truncate(x: fsize) -> fsize {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    (x as isize) as fsize
}

/// Round function for no_std: round to nearest, ties to even (banker's rounding).
pub fn float_round(x: fsize) -> fsize {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    let fl = float_floor(x);
    let diff = x - fl;
    if diff < 0.5 {
        fl
    } else if diff > 0.5 {
        fl + 1.0
    } else {
        // Tie: round to even
        let fl_i = fl as isize;
        if fl_i % 2 == 0 { fl } else { fl + 1.0 }
    }
}

/// Square root for no_std: Newton's method approximation.
pub fn float_sqrt(x: fsize) -> fsize {
    if x < 0.0 {
        return fsize::NAN;
    }
    if x == 0.0 || x.is_nan() || x.is_infinite() {
        return x;
    }
    // Newton's method for square root
    let mut guess = x / 2.0;
    if guess == 0.0 { guess = 1.0; }
    for _ in 0..64 {
        let next = (guess + x / guess) / 2.0;
        if next == guess {
            break;
        }
        guess = next;
    }
    guess
}

/// Power function for no_std: compute base^exp for floating-point.
pub fn float_pow(base: fsize, exp: fsize) -> fsize {
    // Handle special cases
    if exp == 0.0 { return 1.0; }
    if base == 0.0 { return 0.0; }
    if base == 1.0 { return 1.0; }
    
    // Integer exponent fast path
    let exp_i = exp as isize;
    if exp == exp_i as fsize && exp_i.unsigned_abs() < 1000 {
        let mut result: fsize = 1.0;
        let mut b = if exp_i < 0 { 1.0 / base } else { base };
        let mut e = if exp_i < 0 { -exp_i as usize } else { exp_i as usize };
        while e > 0 {
            if e % 2 == 1 {
                result *= b;
            }
            e /= 2;
            if e > 0 {
                b *= b;
            }
        }
        return result;
    }
    
    // For non-integer exponents, use exp(exp * ln(base)) via Taylor series
    // This is an approximation suitable for no_std
    float_exp(exp * float_ln(base))
}

/// Natural logarithm for no_std using series expansion.
fn float_ln(x: fsize) -> fsize {
    if x <= 0.0 { return fsize::NAN; }
    if x == 1.0 { return 0.0; }
    
    // Reduce to range [0.5, 2) using: ln(x) = ln(m * 2^e) = ln(m) + e*ln(2)
    let mut val = x;
    let mut exp_count: isize = 0;
    while val > 2.0 {
        val /= 2.0;
        exp_count += 1;
    }
    while val < 0.5 {
        val *= 2.0;
        exp_count -= 1;
    }
    
    // Use series: ln(x) = 2 * sum_{n=0}^inf (1/(2n+1)) * ((x-1)/(x+1))^(2n+1)
    let y = (val - 1.0) / (val + 1.0);
    let y2 = y * y;
    let mut term = y;
    let mut sum = term;
    for n in 1..40 {
        term *= y2;
        sum += term / (2 * n + 1) as fsize;
    }
    sum *= 2.0;
    
    sum + exp_count as fsize * LN_2
}

/// Exponential function for no_std using Taylor series: e^x.
fn float_exp(x: fsize) -> fsize {
    if x == 0.0 { return 1.0; }
    if x > 700.0 { return fsize::INFINITY; }
    if x < -700.0 { return 0.0; }
    
    // Reduce range: e^x = e^(n + r) = e^n * e^r where n = round(x), |r| <= 0.5
    let n = float_round(x) as isize;
    let r = x - n as fsize;
    
    // Compute e^r using Taylor series
    let mut term: fsize = 1.0;
    let mut sum: fsize = 1.0;
    for i in 1..30 {
        term *= r / i as fsize;
        sum += term;
        if term < 1e-15 && term > -1e-15 {
            break;
        }
    }
    
    // Compute e^n by repeated squaring of e
    let mut e_n: fsize = 1.0;
    let mut base = if n < 0 { 1.0 / E } else { E };
    let mut exp = if n < 0 { -n as usize } else { n as usize };
    while exp > 0 {
        if exp % 2 == 1 {
            e_n *= base;
        }
        exp /= 2;
        if exp > 0 {
            base *= base;
        }
    }
    
    e_n * sum
}