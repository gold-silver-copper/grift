//! # Scheme Numerical Tower
//!
//! This module implements the R7RS Scheme numerical tower with the following types:
//!
//! ```text
//! number
//!   └── complex
//!         └── real
//!               └── rational
//!                     └── integer
//! ```
//!
//! ## Type Hierarchy
//!
//! Every integer is a rational, every rational is a real, and every real is a complex.
//! This is implemented via the `Number` enum with appropriate coercion.
//!
//! ## Exactness
//!
//! Numbers are either exact or inexact:
//! - **Exact**: Integers and rationals with exact representation
//! - **Inexact**: Floating-point approximations (f64)
//!
//! ## Implementation Notes
//!
//! This is a `no_std` implementation using only fixed-size types:
//! - `isize` for exact integers
//! - `f64` for inexact reals
//! - Rationals stored as `isize/usize` (numerator/denominator)
//! - Complex numbers composed of two reals
//!
//! All types are `Copy` and suitable for arena allocation.

use core::cmp::Ordering;
use core::fmt;

/// A Scheme number supporting the full numerical tower.
///
/// The tower is: integer ⊂ rational ⊂ real ⊂ complex ⊂ number
///
/// In this implementation:
/// - `Integer(isize)` - Exact integer
/// - `Float(f64)` - Inexact real (also used for inexact integers and rationals)
/// - `Rational { num, denom }` - Exact rational (always in lowest terms)
/// - `Complex { real, imag }` - Complex number (real and imaginary as f64)
/// - `ExactComplex { real, imag }` - Complex with exact rational parts
#[derive(Clone, Copy, Debug)]
pub enum Number {
    /// Exact integer (isize)
    Integer(isize),
    
    /// Inexact real (f64) - also represents inexact integers, rationals
    Float(f64),
    
    /// Exact rational (numerator / denominator, always reduced)
    /// Denominator is always positive (stored as usize).
    /// Numerator sign determines the sign of the rational.
    Rational {
        num: isize,
        denom: usize,
    },
    
    /// Complex number with inexact real and imaginary parts
    Complex {
        real: f64,
        imag: f64,
    },
    
    /// Complex number with exact rational real and imaginary parts
    /// real_num/real_denom + (imag_num/imag_denom)i
    ExactComplex {
        real_num: isize,
        real_denom: usize,
        imag_num: isize,
        imag_denom: usize,
    },
}

impl Number {
    // ========================================================================
    // Constructors
    // ========================================================================
    
    /// Create an exact integer
    #[inline]
    pub const fn integer(n: isize) -> Self {
        Number::Integer(n)
    }
    
    /// Create an inexact real (float)
    #[inline]
    pub const fn float(f: f64) -> Self {
        Number::Float(f)
    }
    
    /// Create an exact rational, automatically reducing to lowest terms
    #[inline]
    pub fn rational(num: isize, denom: isize) -> Self {
        if denom == 0 {
            // Division by zero - return NaN
            return Number::Float(f64::NAN);
        }
        
        // Normalize sign: denominator always positive
        let (num, denom) = if denom < 0 {
            (-num, (-denom) as usize)
        } else {
            (num, denom as usize)
        };
        
        // Reduce to lowest terms
        let g = gcd(num.unsigned_abs(), denom);
        let num = num / g as isize;
        let denom = denom / g;
        
        // If denominator is 1, it's an integer
        if denom == 1 {
            Number::Integer(num)
        } else {
            Number::Rational { num, denom }
        }
    }
    
    /// Create a complex number from rectangular coordinates
    #[inline]
    pub const fn complex(real: f64, imag: f64) -> Self {
        // If imaginary part is exactly zero, return real number
        // Note: we use const fn so we can't do f64 comparison with ==
        // We'll handle normalization elsewhere
        Number::Complex { real, imag }
    }
    
    /// Create an exact complex number from rational coordinates
    #[inline]
    pub fn exact_complex(real_num: isize, real_denom: isize, imag_num: isize, imag_denom: isize) -> Self {
        if real_denom == 0 || imag_denom == 0 {
            return Number::Float(f64::NAN);
        }
        
        // Normalize signs
        let (real_num, real_denom) = if real_denom < 0 {
            (-real_num, (-real_denom) as usize)
        } else {
            (real_num, real_denom as usize)
        };
        let (imag_num, imag_denom) = if imag_denom < 0 {
            (-imag_num, (-imag_denom) as usize)
        } else {
            (imag_num, imag_denom as usize)
        };
        
        // Reduce to lowest terms
        let g1 = gcd(real_num.unsigned_abs(), real_denom);
        let g2 = gcd(imag_num.unsigned_abs(), imag_denom);
        let real_num = real_num / g1 as isize;
        let real_denom = real_denom / g1;
        let imag_num = imag_num / g2 as isize;
        let imag_denom = imag_denom / g2;
        
        // If imaginary part is zero, return real number
        if imag_num == 0 {
            if real_denom == 1 {
                Number::Integer(real_num)
            } else {
                Number::Rational { num: real_num, denom: real_denom }
            }
        } else {
            Number::ExactComplex {
                real_num,
                real_denom,
                imag_num,
                imag_denom,
            }
        }
    }
    
    /// Create a complex from polar coordinates (r, theta)
    #[inline]
    pub fn from_polar(r: f64, theta: f64) -> Self {
        let real = r * libm::cos(theta);
        let imag = r * libm::sin(theta);
        Number::complex(real, imag).normalize()
    }
    
    /// Normalize the number representation
    /// - Complex with zero imaginary becomes real
    /// - Rational with denominator 1 becomes integer
    pub fn normalize(self) -> Self {
        match self {
            Number::Complex { real, imag } if imag == 0.0 => Number::Float(real),
            Number::Rational { num, denom } if denom == 1 => Number::Integer(num),
            Number::ExactComplex { real_num, real_denom, imag_num, imag_denom } if imag_num == 0 => {
                if real_denom == 1 {
                    Number::Integer(real_num)
                } else {
                    Number::Rational { num: real_num, denom: real_denom }
                }
            }
            other => other,
        }
    }
    
    // ========================================================================
    // Type Predicates
    // ========================================================================
    
    /// Returns true if this is a number (always true for Number)
    #[inline]
    pub const fn is_number(&self) -> bool {
        true
    }
    
    /// Returns true if this is a complex number (all numbers are complex)
    #[inline]
    pub const fn is_complex(&self) -> bool {
        true
    }
    
    /// Returns true if this is a real number (imaginary part is zero)
    #[inline]
    pub fn is_real(&self) -> bool {
        match self {
            Number::Integer(_) | Number::Float(_) | Number::Rational { .. } => true,
            Number::Complex { imag, .. } => *imag == 0.0,
            Number::ExactComplex { imag_num, .. } => *imag_num == 0,
        }
    }
    
    /// Returns true if this is a rational number
    /// (integers and exact rationals, plus finite floats)
    #[inline]
    pub fn is_rational(&self) -> bool {
        match self {
            Number::Integer(_) | Number::Rational { .. } => true,
            Number::Float(f) => f.is_finite(),
            Number::Complex { real, imag } => *imag == 0.0 && real.is_finite(),
            Number::ExactComplex { imag_num, .. } => *imag_num == 0,
        }
    }
    
    /// Returns true if this is an integer
    #[inline]
    pub fn is_integer(&self) -> bool {
        match self {
            Number::Integer(_) => true,
            Number::Float(f) => f.is_finite() && *f == libm::trunc(*f),
            Number::Rational { num: _, denom } => *denom == 1,
            Number::Complex { real, imag } => *imag == 0.0 && real.is_finite() && *real == libm::trunc(*real),
            Number::ExactComplex { imag_num, real_denom, .. } => *imag_num == 0 && *real_denom == 1,
        }
    }
    
    /// Returns true if this is an exact number
    #[inline]
    pub const fn is_exact(&self) -> bool {
        matches!(self, Number::Integer(_) | Number::Rational { .. } | Number::ExactComplex { .. })
    }
    
    /// Returns true if this is an inexact number
    #[inline]
    pub const fn is_inexact(&self) -> bool {
        matches!(self, Number::Float(_) | Number::Complex { .. })
    }
    
    /// Returns true if this is an exact integer
    #[inline]
    pub const fn is_exact_integer(&self) -> bool {
        matches!(self, Number::Integer(_))
    }
    
    /// Returns true if this is finite (not inf or nan)
    #[inline]
    pub fn is_finite(&self) -> bool {
        match self {
            Number::Integer(_) | Number::Rational { .. } | Number::ExactComplex { .. } => true,
            Number::Float(f) => f.is_finite(),
            Number::Complex { real, imag } => real.is_finite() && imag.is_finite(),
        }
    }
    
    /// Returns true if this is infinite
    #[inline]
    pub fn is_infinite(&self) -> bool {
        match self {
            Number::Integer(_) | Number::Rational { .. } | Number::ExactComplex { .. } => false,
            Number::Float(f) => f.is_infinite(),
            Number::Complex { real, imag } => real.is_infinite() || imag.is_infinite(),
        }
    }
    
    /// Returns true if this is NaN
    #[inline]
    pub fn is_nan(&self) -> bool {
        match self {
            Number::Integer(_) | Number::Rational { .. } | Number::ExactComplex { .. } => false,
            Number::Float(f) => f.is_nan(),
            Number::Complex { real, imag } => real.is_nan() || imag.is_nan(),
        }
    }
    
    /// Returns true if this is zero
    #[inline]
    pub fn is_zero(&self) -> bool {
        match self {
            Number::Integer(n) => *n == 0,
            Number::Float(f) => *f == 0.0,
            Number::Rational { num, .. } => *num == 0,
            Number::Complex { real, imag } => *real == 0.0 && *imag == 0.0,
            Number::ExactComplex { real_num, imag_num, .. } => *real_num == 0 && *imag_num == 0,
        }
    }
    
    /// Returns true if this is positive (real numbers only)
    #[inline]
    pub fn is_positive(&self) -> bool {
        match self {
            Number::Integer(n) => *n > 0,
            Number::Float(f) => *f > 0.0,
            Number::Rational { num, .. } => *num > 0,
            _ => false,
        }
    }
    
    /// Returns true if this is negative (real numbers only)
    #[inline]
    pub fn is_negative(&self) -> bool {
        match self {
            Number::Integer(n) => *n < 0,
            Number::Float(f) => *f < 0.0,
            Number::Rational { num, .. } => *num < 0,
            _ => false,
        }
    }
    
    /// Returns true if this is odd (integers only)
    #[inline]
    pub fn is_odd(&self) -> bool {
        match self {
            Number::Integer(n) => *n % 2 != 0,
            Number::Float(f) if f.is_finite() && *f == libm::trunc(*f) => (*f as isize) % 2 != 0,
            _ => false,
        }
    }
    
    /// Returns true if this is even (integers only)
    #[inline]
    pub fn is_even(&self) -> bool {
        match self {
            Number::Integer(n) => *n % 2 == 0,
            Number::Float(f) if f.is_finite() && *f == libm::trunc(*f) => (*f as isize) % 2 == 0,
            _ => false,
        }
    }
    
    // ========================================================================
    // Component Access
    // ========================================================================
    
    /// Get the real part
    #[inline]
    pub fn real_part(&self) -> Number {
        match self {
            Number::Integer(n) => Number::Integer(*n),
            Number::Float(f) => Number::Float(*f),
            Number::Rational { num, denom } => Number::Rational { num: *num, denom: *denom },
            Number::Complex { real, .. } => Number::Float(*real),
            Number::ExactComplex { real_num, real_denom, .. } => {
                if *real_denom == 1 {
                    Number::Integer(*real_num)
                } else {
                    Number::Rational { num: *real_num, denom: *real_denom }
                }
            }
        }
    }
    
    /// Get the imaginary part
    #[inline]
    pub fn imag_part(&self) -> Number {
        match self {
            Number::Integer(_) | Number::Float(_) | Number::Rational { .. } => Number::Integer(0),
            Number::Complex { imag, .. } => Number::Float(*imag),
            Number::ExactComplex { imag_num, imag_denom, .. } => {
                if *imag_num == 0 {
                    Number::Integer(0)
                } else if *imag_denom == 1 {
                    Number::Integer(*imag_num)
                } else {
                    Number::Rational { num: *imag_num, denom: *imag_denom }
                }
            }
        }
    }
    
    /// Get the magnitude (absolute value for real, modulus for complex)
    #[inline]
    pub fn magnitude(&self) -> Number {
        match self {
            Number::Integer(n) => Number::Integer(n.abs()),
            Number::Float(f) => Number::Float(libm::fabs(*f)),
            Number::Rational { num, denom } => Number::Rational { num: num.abs(), denom: *denom },
            Number::Complex { real, imag } => Number::Float(libm::sqrt(*real * *real + *imag * *imag)),
            Number::ExactComplex { real_num, real_denom, imag_num, imag_denom } => {
                // For exact complex, we need to convert to float for sqrt
                let r = *real_num as f64 / *real_denom as f64;
                let i = *imag_num as f64 / *imag_denom as f64;
                Number::Float(libm::sqrt(r * r + i * i))
            }
        }
    }
    
    /// Get the angle (argument) of a complex number in radians
    #[inline]
    pub fn angle(&self) -> Number {
        match self {
            Number::Integer(n) => {
                if *n >= 0 { Number::Float(0.0) } else { Number::Float(core::f64::consts::PI) }
            }
            Number::Float(f) => {
                if *f >= 0.0 { Number::Float(0.0) } else { Number::Float(core::f64::consts::PI) }
            }
            Number::Rational { num, .. } => {
                if *num >= 0 { Number::Float(0.0) } else { Number::Float(core::f64::consts::PI) }
            }
            Number::Complex { real, imag } => Number::Float(libm::atan2(*imag, *real)),
            Number::ExactComplex { real_num, real_denom, imag_num, imag_denom } => {
                let r = *real_num as f64 / *real_denom as f64;
                let i = *imag_num as f64 / *imag_denom as f64;
                Number::Float(libm::atan2(i, r))
            }
        }
    }
    
    /// Get the numerator (for rationals)
    #[inline]
    pub fn numerator(&self) -> Number {
        match self {
            Number::Integer(n) => Number::Integer(*n),
            Number::Rational { num, .. } => Number::Integer(*num),
            Number::Float(f) => {
                // Convert to rational representation
                if f.is_finite() {
                    let (num, _denom) = float_to_rational(*f);
                    Number::Float(num as f64)
                } else {
                    Number::Float(*f)
                }
            }
            _ => Number::Float(f64::NAN),
        }
    }
    
    /// Get the denominator (for rationals)
    #[inline]
    pub fn denominator(&self) -> Number {
        match self {
            Number::Integer(_) => Number::Integer(1),
            Number::Rational { denom, .. } => Number::Integer(*denom as isize),
            Number::Float(f) => {
                if f.is_finite() {
                    let (_num, denom) = float_to_rational(*f);
                    Number::Float(denom as f64)
                } else {
                    Number::Float(1.0)
                }
            }
            _ => Number::Float(f64::NAN),
        }
    }
    
    // ========================================================================
    // Conversions
    // ========================================================================
    
    /// Convert to exact representation
    #[inline]
    pub fn to_exact(&self) -> Number {
        match self {
            Number::Integer(_) | Number::Rational { .. } | Number::ExactComplex { .. } => *self,
            Number::Float(f) => {
                if f.is_finite() {
                    let (num, denom) = float_to_rational(*f);
                    Number::rational(num, denom as isize)
                } else {
                    // Can't convert inf/nan to exact
                    *self
                }
            }
            Number::Complex { real, imag } => {
                if real.is_finite() && imag.is_finite() {
                    let (rn, rd) = float_to_rational(*real);
                    let (in_, id) = float_to_rational(*imag);
                    Number::exact_complex(rn, rd as isize, in_, id as isize)
                } else {
                    *self
                }
            }
        }
    }
    
    /// Convert to inexact representation
    #[inline]
    pub fn to_inexact(&self) -> Number {
        match self {
            Number::Integer(n) => Number::Float(*n as f64),
            Number::Float(_) | Number::Complex { .. } => *self,
            Number::Rational { num, denom } => Number::Float(*num as f64 / *denom as f64),
            Number::ExactComplex { real_num, real_denom, imag_num, imag_denom } => {
                Number::Complex {
                    real: *real_num as f64 / *real_denom as f64,
                    imag: *imag_num as f64 / *imag_denom as f64,
                }
            }
        }
    }
    
    /// Convert to f64 (for real numbers)
    #[inline]
    pub fn to_f64(&self) -> f64 {
        match self {
            Number::Integer(n) => *n as f64,
            Number::Float(f) => *f,
            Number::Rational { num, denom } => *num as f64 / *denom as f64,
            Number::Complex { real, imag } if *imag == 0.0 => *real,
            Number::ExactComplex { real_num, real_denom, imag_num, .. } if *imag_num == 0 => {
                *real_num as f64 / *real_denom as f64
            }
            _ => f64::NAN,
        }
    }
    
    /// Try to convert to isize (for exact integers)
    #[inline]
    pub fn to_isize(&self) -> Option<isize> {
        match self {
            Number::Integer(n) => Some(*n),
            Number::Float(f) if f.is_finite() && *f == libm::trunc(*f) && *f >= isize::MIN as f64 && *f <= isize::MAX as f64 => {
                Some(*f as isize)
            }
            Number::Rational { num, denom } if *denom == 1 => Some(*num),
            _ => None,
        }
    }
    
    // ========================================================================
    // Rounding Operations
    // ========================================================================
    
    /// Floor: largest integer not larger than x
    #[inline]
    pub fn floor(&self) -> Number {
        match self {
            Number::Integer(_) => *self,
            Number::Float(f) => Number::Float(libm::floor(*f)),
            Number::Rational { num, denom } => {
                let q = *num / (*denom as isize);
                let r = *num % (*denom as isize);
                if r < 0 { Number::Integer(q - 1) } else { Number::Integer(q) }
            }
            _ => Number::Float(f64::NAN),
        }
    }
    
    /// Ceiling: smallest integer not smaller than x  
    #[inline]
    pub fn ceiling(&self) -> Number {
        match self {
            Number::Integer(_) => *self,
            Number::Float(f) => Number::Float(libm::ceil(*f)),
            Number::Rational { num, denom } => {
                let q = *num / (*denom as isize);
                let r = *num % (*denom as isize);
                if r > 0 { Number::Integer(q + 1) } else { Number::Integer(q) }
            }
            _ => Number::Float(f64::NAN),
        }
    }
    
    /// Truncate: integer closest to x whose absolute value is not larger
    #[inline]
    pub fn truncate(&self) -> Number {
        match self {
            Number::Integer(_) => *self,
            Number::Float(f) => Number::Float(libm::trunc(*f)),
            Number::Rational { num, denom } => Number::Integer(*num / (*denom as isize)),
            _ => Number::Float(f64::NAN),
        }
    }
    
    /// Round: closest integer, rounding to even on ties
    #[inline]
    pub fn round(&self) -> Number {
        match self {
            Number::Integer(_) => *self,
            Number::Float(f) => Number::Float(libm::round(*f)),
            Number::Rational { num, denom } => {
                // Add 0.5 and floor, with ties to even
                let q = *num / (*denom as isize);
                let r = (*num % (*denom as isize)).abs() as usize;
                let half = *denom / 2;
                if r > half || (r == half && *denom % 2 == 0 && q % 2 != 0) {
                    if *num >= 0 { Number::Integer(q + 1) } else { Number::Integer(q - 1) }
                } else {
                    Number::Integer(q)
                }
            }
            _ => Number::Float(f64::NAN),
        }
    }
    
    // ========================================================================
    // Arithmetic Operations
    // ========================================================================
    
    /// Negate the number
    #[inline]
    pub fn neg(&self) -> Number {
        match self {
            Number::Integer(n) => Number::Integer(-*n),
            Number::Float(f) => Number::Float(-*f),
            Number::Rational { num, denom } => Number::Rational { num: -*num, denom: *denom },
            Number::Complex { real, imag } => Number::Complex { real: -*real, imag: -*imag },
            Number::ExactComplex { real_num, real_denom, imag_num, imag_denom } => {
                Number::ExactComplex { 
                    real_num: -*real_num, 
                    real_denom: *real_denom,
                    imag_num: -*imag_num,
                    imag_denom: *imag_denom,
                }
            }
        }
    }
    
    /// Add two numbers
    #[inline]
    pub fn add(&self, other: &Number) -> Number {
        match (self, other) {
            // Integer + Integer
            (Number::Integer(a), Number::Integer(b)) => {
                a.checked_add(*b).map_or(Number::Float(*a as f64 + *b as f64), Number::Integer)
            }
            
            // Float + any real => Float
            (Number::Float(a), Number::Float(b)) => Number::Float(*a + *b),
            (Number::Float(a), Number::Integer(b)) => Number::Float(*a + *b as f64),
            (Number::Integer(a), Number::Float(b)) => Number::Float(*a as f64 + *b),
            (Number::Float(a), Number::Rational { num, denom }) => {
                Number::Float(*a + *num as f64 / *denom as f64)
            }
            (Number::Rational { num, denom }, Number::Float(b)) => {
                Number::Float(*num as f64 / *denom as f64 + *b)
            }
            
            // Rational operations
            (Number::Integer(a), Number::Rational { num, denom }) => {
                Number::rational(*a * (*denom as isize) + *num, *denom as isize)
            }
            (Number::Rational { num, denom }, Number::Integer(b)) => {
                Number::rational(*num + *b * (*denom as isize), *denom as isize)
            }
            (Number::Rational { num: n1, denom: d1 }, Number::Rational { num: n2, denom: d2 }) => {
                // a/b + c/d = (a*d + c*b) / (b*d)
                Number::rational(*n1 * (*d2 as isize) + *n2 * (*d1 as isize), (*d1 * *d2) as isize)
            }
            
            // Complex operations
            (Number::Complex { real: r1, imag: i1 }, Number::Complex { real: r2, imag: i2 }) => {
                Number::complex(*r1 + *r2, *i1 + *i2).normalize()
            }
            (Number::Complex { real, imag }, other) => {
                Number::complex(*real + other.to_f64(), *imag).normalize()
            }
            (other, Number::Complex { real, imag }) => {
                Number::complex(other.to_f64() + *real, *imag).normalize()
            }
            
            // ExactComplex - convert to inexact Complex for simplicity
            (Number::ExactComplex { .. }, _) | (_, Number::ExactComplex { .. }) => {
                let c1 = self.to_inexact();
                let c2 = other.to_inexact();
                c1.add(&c2)
            }
        }
    }
    
    /// Subtract two numbers
    #[inline]
    pub fn sub(&self, other: &Number) -> Number {
        self.add(&other.neg())
    }
    
    /// Multiply two numbers
    #[inline]
    pub fn mul(&self, other: &Number) -> Number {
        match (self, other) {
            // Integer * Integer
            (Number::Integer(a), Number::Integer(b)) => {
                a.checked_mul(*b).map_or(Number::Float(*a as f64 * *b as f64), Number::Integer)
            }
            
            // Float * any real => Float
            (Number::Float(a), Number::Float(b)) => Number::Float(*a * *b),
            (Number::Float(a), Number::Integer(b)) => Number::Float(*a * *b as f64),
            (Number::Integer(a), Number::Float(b)) => Number::Float(*a as f64 * *b),
            (Number::Float(a), Number::Rational { num, denom }) => {
                Number::Float(*a * *num as f64 / *denom as f64)
            }
            (Number::Rational { num, denom }, Number::Float(b)) => {
                Number::Float(*num as f64 / *denom as f64 * *b)
            }
            
            // Rational operations
            (Number::Integer(a), Number::Rational { num, denom }) => {
                Number::rational(*a * *num, *denom as isize)
            }
            (Number::Rational { num, denom }, Number::Integer(b)) => {
                Number::rational(*num * *b, *denom as isize)
            }
            (Number::Rational { num: n1, denom: d1 }, Number::Rational { num: n2, denom: d2 }) => {
                Number::rational(*n1 * *n2, (*d1 * *d2) as isize)
            }
            
            // Complex operations: (a+bi)(c+di) = (ac-bd) + (ad+bc)i
            (Number::Complex { real: a, imag: b }, Number::Complex { real: c, imag: d }) => {
                Number::complex(*a * *c - *b * *d, *a * *d + *b * *c).normalize()
            }
            (Number::Complex { real, imag }, other) => {
                let f = other.to_f64();
                Number::complex(*real * f, *imag * f).normalize()
            }
            (other, Number::Complex { real, imag }) => {
                let f = other.to_f64();
                Number::complex(f * *real, f * *imag).normalize()
            }
            
            // ExactComplex
            (Number::ExactComplex { .. }, _) | (_, Number::ExactComplex { .. }) => {
                let c1 = self.to_inexact();
                let c2 = other.to_inexact();
                c1.mul(&c2)
            }
        }
    }
    
    /// Divide two numbers
    #[inline]
    pub fn div(&self, other: &Number) -> Number {
        // Check for division by zero
        if other.is_zero() {
            return Number::Float(f64::NAN);
        }
        
        match (self, other) {
            // Integer / Integer -> may produce rational
            (Number::Integer(a), Number::Integer(b)) => {
                Number::rational(*a, *b)
            }
            
            // Float / any real => Float
            (Number::Float(a), Number::Float(b)) => Number::Float(*a / *b),
            (Number::Float(a), Number::Integer(b)) => Number::Float(*a / *b as f64),
            (Number::Integer(a), Number::Float(b)) => Number::Float(*a as f64 / *b),
            (Number::Float(a), Number::Rational { num, denom }) => {
                Number::Float(*a * (*denom as f64) / *num as f64)
            }
            (Number::Rational { num, denom }, Number::Float(b)) => {
                Number::Float(*num as f64 / (*denom as f64 * *b))
            }
            
            // Rational operations: (a/b) / (c/d) = (a*d) / (b*c)
            (Number::Integer(a), Number::Rational { num, denom }) => {
                Number::rational(*a * (*denom as isize), *num)
            }
            (Number::Rational { num, denom }, Number::Integer(b)) => {
                Number::rational(*num, (*denom as isize) * *b)
            }
            (Number::Rational { num: n1, denom: d1 }, Number::Rational { num: n2, denom: d2 }) => {
                Number::rational(*n1 * (*d2 as isize), (*d1 as isize) * *n2)
            }
            
            // Complex division: (a+bi)/(c+di) = ((ac+bd) + (bc-ad)i) / (c²+d²)
            (Number::Complex { real: a, imag: b }, Number::Complex { real: c, imag: d }) => {
                let denom = *c * *c + *d * *d;
                Number::complex((*a * *c + *b * *d) / denom, (*b * *c - *a * *d) / denom).normalize()
            }
            (Number::Complex { real, imag }, other) => {
                let f = other.to_f64();
                Number::complex(*real / f, *imag / f).normalize()
            }
            (other, Number::Complex { real: c, imag: d }) => {
                let a = other.to_f64();
                let denom = *c * *c + *d * *d;
                Number::complex((a * *c) / denom, (-a * *d) / denom).normalize()
            }
            
            // ExactComplex
            (Number::ExactComplex { .. }, _) | (_, Number::ExactComplex { .. }) => {
                let c1 = self.to_inexact();
                let c2 = other.to_inexact();
                c1.div(&c2)
            }
        }
    }
    
    /// Absolute value
    #[inline]
    pub fn abs(&self) -> Number {
        match self {
            Number::Integer(n) => Number::Integer(n.abs()),
            Number::Float(f) => Number::Float(libm::fabs(*f)),
            Number::Rational { num, denom } => Number::Rational { num: num.abs(), denom: *denom },
            Number::Complex { .. } | Number::ExactComplex { .. } => self.magnitude(),
        }
    }
    
    /// Modulo operation (floor remainder)
    #[inline]
    pub fn modulo(&self, other: &Number) -> Number {
        if other.is_zero() {
            return Number::Float(f64::NAN);
        }
        match (self, other) {
            (Number::Integer(a), Number::Integer(b)) => {
                // Floor modulo
                let r = *a % *b;
                if (r < 0 && *b > 0) || (r > 0 && *b < 0) {
                    Number::Integer(r + *b)
                } else {
                    Number::Integer(r)
                }
            }
            _ => {
                let a = self.to_f64();
                let b = other.to_f64();
                Number::Float(a - b * libm::floor(a / b))
            }
        }
    }
    
    /// Remainder (truncate remainder)
    #[inline]
    pub fn remainder(&self, other: &Number) -> Number {
        if other.is_zero() {
            return Number::Float(f64::NAN);
        }
        match (self, other) {
            (Number::Integer(a), Number::Integer(b)) => Number::Integer(*a % *b),
            _ => {
                let a = self.to_f64();
                let b = other.to_f64();
                Number::Float(a - b * libm::trunc(a / b))
            }
        }
    }
    
    /// Quotient (truncate quotient)
    #[inline]
    pub fn quotient(&self, other: &Number) -> Number {
        if other.is_zero() {
            return Number::Float(f64::NAN);
        }
        match (self, other) {
            (Number::Integer(a), Number::Integer(b)) => Number::Integer(*a / *b),
            _ => {
                let a = self.to_f64();
                let b = other.to_f64();
                Number::Float(libm::trunc(a / b))
            }
        }
    }
    
    /// Exponentiation
    #[inline]
    pub fn expt(&self, other: &Number) -> Number {
        match (self, other) {
            (Number::Integer(base), Number::Integer(exp)) if *exp >= 0 => {
                // Try exact integer power
                if let Some(result) = pow_checked(*base, *exp as usize) {
                    return Number::Integer(result);
                }
                // Fall back to float
                Number::Float(libm::pow(*base as f64, *exp as f64))
            }
            _ => {
                // General case: use floating point
                let base = self.to_f64();
                let exp = other.to_f64();
                Number::Float(libm::pow(base, exp))
            }
        }
    }
    
    /// Square
    #[inline]
    pub fn square(&self) -> Number {
        self.mul(self)
    }
    
    /// Square root
    #[inline]
    pub fn sqrt(&self) -> Number {
        match self {
            Number::Integer(n) if *n >= 0 => {
                // Check for perfect square
                let root = libm::sqrt(*n as f64) as isize;
                if root * root == *n {
                    Number::Integer(root)
                } else {
                    Number::Float(libm::sqrt(*n as f64))
                }
            }
            Number::Integer(n) => {
                // Negative: complex result
                Number::complex(0.0, libm::sqrt((-*n) as f64))
            }
            Number::Float(f) if *f >= 0.0 => Number::Float(libm::sqrt(*f)),
            Number::Float(f) => Number::complex(0.0, libm::sqrt(-*f)),
            Number::Rational { num, denom } => {
                let f = *num as f64 / *denom as f64;
                if f >= 0.0 {
                    Number::Float(libm::sqrt(f))
                } else {
                    Number::complex(0.0, libm::sqrt(-f))
                }
            }
            Number::Complex { real, imag } => {
                // sqrt(a+bi) = sqrt((r+a)/2) + i*sign(b)*sqrt((r-a)/2)
                // where r = |a+bi|
                let r = libm::sqrt(*real * *real + *imag * *imag);
                let real_part = libm::sqrt((r + *real) / 2.0);
                let imag_part = if *imag >= 0.0 {
                    libm::sqrt((r - *real) / 2.0)
                } else {
                    -libm::sqrt((r - *real) / 2.0)
                };
                Number::complex(real_part, imag_part).normalize()
            }
            Number::ExactComplex { .. } => self.to_inexact().sqrt(),
        }
    }
}

// ============================================================================
// Comparison
// ============================================================================

impl Number {
    /// Compare two real numbers
    /// Returns None for complex numbers or NaN
    #[inline]
    pub fn partial_cmp_real(&self, other: &Number) -> Option<Ordering> {
        if !self.is_real() || !other.is_real() {
            return None;
        }
        let a = self.to_f64();
        let b = other.to_f64();
        a.partial_cmp(&b)
    }
    
    /// Check equality (numeric equality, not structural)
    #[inline]
    pub fn numeric_eq(&self, other: &Number) -> bool {
        match (self, other) {
            (Number::Integer(a), Number::Integer(b)) => a == b,
            (Number::Float(a), Number::Float(b)) => a == b,
            (Number::Rational { num: n1, denom: d1 }, Number::Rational { num: n2, denom: d2 }) => {
                n1 == n2 && d1 == d2
            }
            (Number::Complex { real: r1, imag: i1 }, Number::Complex { real: r2, imag: i2 }) => {
                r1 == r2 && i1 == i2
            }
            // Mixed types: convert to common representation
            _ => {
                if self.is_exact() && other.is_exact() {
                    // Compare as rationals
                    let (n1, d1) = self.to_rational_parts();
                    let (n2, d2) = other.to_rational_parts();
                    n1 * (d2 as i128) == n2 * (d1 as i128)
                } else {
                    // Compare as floats
                    self.to_f64() == other.to_f64()
                }
            }
        }
    }
    
    /// Get rational parts (numerator, denominator) for exact comparison
    fn to_rational_parts(&self) -> (i128, u128) {
        match self {
            Number::Integer(n) => (*n as i128, 1),
            Number::Rational { num, denom } => (*num as i128, *denom as u128),
            _ => (0, 1), // Not applicable for inexact
        }
    }
}

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        self.numeric_eq(other)
    }
}

// ============================================================================
// Display
// ============================================================================

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Number::Integer(n) => write!(f, "{}", n),
            Number::Float(v) => {
                if v.is_nan() {
                    write!(f, "+nan.0")
                } else if v.is_infinite() {
                    if *v > 0.0 { write!(f, "+inf.0") } else { write!(f, "-inf.0") }
                } else if *v == libm::trunc(*v) && v.abs() < 1e15 {
                    // Integer-valued float: add .0
                    write!(f, "{}.0", *v as i64)
                } else {
                    write!(f, "{}", v)
                }
            }
            Number::Rational { num, denom } => write!(f, "{}/{}", num, denom),
            Number::Complex { real, imag } => {
                if *imag >= 0.0 {
                    write!(f, "{}+{}i", real, imag)
                } else {
                    write!(f, "{}{}i", real, imag)
                }
            }
            Number::ExactComplex { real_num, real_denom, imag_num, imag_denom } => {
                let real_str = if *real_denom == 1 {
                    format_isize(*real_num)
                } else {
                    format_rational(*real_num, *real_denom)
                };
                let imag_str = if *imag_denom == 1 {
                    format_isize(*imag_num)
                } else {
                    format_rational(*imag_num, *imag_denom)
                };
                if *imag_num >= 0 {
                    write!(f, "{}+{}i", real_str, imag_str)
                } else {
                    write!(f, "{}{}i", real_str, imag_str)
                }
            }
        }
    }
}

// Helper to format isize without allocation
fn format_isize(n: isize) -> FormattedNumber {
    FormattedNumber::Integer(n)
}

fn format_rational(num: isize, denom: usize) -> FormattedNumber {
    FormattedNumber::Rational(num, denom)
}

enum FormattedNumber {
    Integer(isize),
    Rational(isize, usize),
}

impl fmt::Display for FormattedNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormattedNumber::Integer(n) => write!(f, "{}", n),
            FormattedNumber::Rational(num, denom) => write!(f, "{}/{}", num, denom),
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Greatest Common Divisor (Euclidean algorithm)
#[inline]
pub fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Least Common Multiple
#[inline]
pub fn lcm(a: usize, b: usize) -> usize {
    if a == 0 || b == 0 {
        0
    } else {
        (a / gcd(a, b)) * b
    }
}

/// Convert float to rational approximation
/// Uses continued fraction algorithm
fn float_to_rational(f: f64) -> (isize, usize) {
    if !f.is_finite() {
        return (0, 1);
    }
    
    let sign = if f < 0.0 { -1isize } else { 1 };
    let f = libm::fabs(f);
    
    // Simple approach: multiply by power of 10 until we have an integer
    // then reduce
    let mut denom = 1usize;
    let mut value = f;
    
    // Limit precision to avoid overflow
    const MAX_DENOM: usize = 1_000_000;
    
    while value != libm::trunc(value) && denom < MAX_DENOM {
        value *= 10.0;
        denom *= 10;
    }
    
    let num = (value as isize) * sign;
    let g = gcd(num.unsigned_abs(), denom);
    (num / g as isize, denom / g)
}

/// Checked integer power
fn pow_checked(base: isize, exp: usize) -> Option<isize> {
    if exp == 0 {
        return Some(1);
    }
    let mut result: isize = 1;
    let mut base = base;
    let mut exp = exp;
    
    while exp > 0 {
        if exp % 2 == 1 {
            result = result.checked_mul(base)?;
        }
        exp /= 2;
        if exp > 0 {
            base = base.checked_mul(base)?;
        }
    }
    Some(result)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_integer_basics() {
        let n = Number::integer(42);
        assert!(n.is_integer());
        assert!(n.is_rational());
        assert!(n.is_real());
        assert!(n.is_complex());
        assert!(n.is_exact());
        assert!(!n.is_inexact());
    }
    
    #[test]
    fn test_float_basics() {
        let n = Number::float(3.14);
        assert!(!n.is_integer());
        assert!(n.is_rational());
        assert!(n.is_real());
        assert!(n.is_complex());
        assert!(!n.is_exact());
        assert!(n.is_inexact());
    }
    
    #[test]
    fn test_rational_reduction() {
        let n = Number::rational(6, 4);
        match n {
            Number::Rational { num, denom } => {
                assert_eq!(num, 3);
                assert_eq!(denom, 2);
            }
            _ => panic!("Expected Rational"),
        }
    }
    
    #[test]
    fn test_rational_to_integer() {
        let n = Number::rational(6, 3);
        match n {
            Number::Integer(i) => assert_eq!(i, 2),
            _ => panic!("Expected Integer"),
        }
    }
    
    #[test]
    fn test_complex_basics() {
        let n = Number::complex(3.0, 4.0);
        assert!(!n.is_real());
        assert!(n.is_complex());
        assert!(!n.is_exact());
    }
    
    #[test]
    fn test_complex_magnitude() {
        let n = Number::complex(3.0, 4.0);
        let mag = n.magnitude();
        assert_eq!(mag.to_f64(), 5.0);
    }
    
    #[test]
    fn test_addition() {
        let a = Number::integer(1);
        let b = Number::integer(2);
        assert_eq!(a.add(&b), Number::integer(3));
        
        let c = Number::rational(1, 2);
        let d = Number::rational(1, 3);
        // 1/2 + 1/3 = 5/6
        let sum = c.add(&d);
        match sum {
            Number::Rational { num, denom } => {
                assert_eq!(num, 5);
                assert_eq!(denom, 6);
            }
            _ => panic!("Expected Rational"),
        }
    }
    
    #[test]
    fn test_division() {
        let a = Number::integer(6);
        let b = Number::integer(4);
        let result = a.div(&b);
        // 6/4 = 3/2
        match result {
            Number::Rational { num, denom } => {
                assert_eq!(num, 3);
                assert_eq!(denom, 2);
            }
            _ => panic!("Expected Rational, got {:?}", result),
        }
    }
    
    #[test]
    fn test_gcd() {
        assert_eq!(gcd(12, 8), 4);
        assert_eq!(gcd(17, 13), 1);
        assert_eq!(gcd(0, 5), 5);
        assert_eq!(gcd(5, 0), 5);
    }
    
    #[test]
    fn test_predicates() {
        assert!(Number::integer(0).is_zero());
        assert!(Number::integer(5).is_positive());
        assert!(Number::integer(-5).is_negative());
        assert!(Number::integer(5).is_odd());
        assert!(Number::integer(4).is_even());
    }
    
    #[test]
    fn test_special_floats() {
        let inf = Number::float(f64::INFINITY);
        let nan = Number::float(f64::NAN);
        
        assert!(inf.is_infinite());
        assert!(!inf.is_finite());
        assert!(nan.is_nan());
        assert!(!nan.is_finite());
    }
}
