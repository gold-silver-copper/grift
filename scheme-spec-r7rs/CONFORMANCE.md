# R7RS Scheme Conformance

This document tracks the implementation progress of R7RS Scheme features in this Lisp interpreter.

## Numbers (Section 6.2)

### Numerical Types

| Type | Status | Notes |
|------|--------|-------|
| Integer | ✅ Implemented | Exact integers using `isize` |
| Rational | ✅ Implemented | Exact rationals as `num/denom` pairs, automatically reduced |
| Real | ✅ Implemented | Inexact floats using `f64` |
| Complex | ✅ Implemented | Both rectangular (`a+bi`) and polar (`r@θ`) forms |

### Number Syntax

| Syntax | Status | Example |
|--------|--------|---------|
| Decimal integers | ✅ | `42`, `-17`, `+5` |
| Binary prefix | ✅ | `#b1010` → 10 |
| Octal prefix | ✅ | `#o755` → 493 |
| Hexadecimal prefix | ✅ | `#xFF` → 255 |
| Decimal prefix | ✅ | `#d42` → 42 |
| Floating point | ✅ | `3.14`, `.5`, `5.`, `1.5e10` |
| Rationals | ✅ | `3/4`, `-1/3` |
| Complex rectangular | ✅ | `3+4i`, `1-2i`, `+3i`, `-i` |
| Complex polar | ✅ | `1@0.5` |
| Exactness prefixes | ✅ | `#e3.14`, `#i42` |
| Special floats | ✅ | `+inf.0`, `-inf.0`, `+nan.0`, `-nan.0` |

### Numerical Type Predicates

| Predicate | Status | Notes |
|-----------|--------|-------|
| `number?` | ✅ | |
| `complex?` | ✅ | All numbers are complex |
| `real?` | ✅ | True if imaginary part is zero |
| `rational?` | ✅ | Includes integers and finite floats |
| `integer?` | ✅ | Includes integer-valued floats |
| `exact?` | ✅ | |
| `inexact?` | ✅ | |
| `exact-integer?` | ✅ | |
| `finite?` | ✅ | |
| `infinite?` | ✅ | |
| `nan?` | ✅ | |

### Numerical Predicates

| Predicate | Status | Notes |
|-----------|--------|-------|
| `zero?` | ✅ | |
| `positive?` | ✅ | |
| `negative?` | ✅ | |
| `odd?` | ✅ | |
| `even?` | ✅ | |

### Arithmetic Operations

| Operation | Status | Notes |
|-----------|--------|-------|
| `+` | ✅ | Mixed-type arithmetic |
| `-` | ✅ | Unary negation and subtraction |
| `*` | ✅ | Mixed-type arithmetic |
| `/` | ✅ | Produces rationals for exact integers |
| `abs` | ✅ | |
| `quotient` | ✅ | Integer quotient |
| `remainder` | ✅ | |
| `modulo` | ✅ | |
| `gcd` | ✅ | |
| `lcm` | ✅ | |
| `expt` | ✅ | Integer and float exponentiation |
| `square` | ✅ | |
| `sqrt` | ✅ | Returns complex for negative inputs |
| `max` | ✅ | |
| `min` | ✅ | |

### Comparison Operations

| Operation | Status | Notes |
|-----------|--------|-------|
| `=` | ✅ | Numeric equality |
| `<` | ✅ | |
| `>` | ✅ | |
| `<=` | ✅ | |
| `>=` | ✅ | |

### Rounding Operations

| Operation | Status | Notes |
|-----------|--------|-------|
| `floor` | ✅ | |
| `ceiling` | ✅ | |
| `truncate` | ✅ | |
| `round` | ✅ | Banker's rounding |

### Rational Operations

| Operation | Status | Notes |
|-----------|--------|-------|
| `numerator` | ✅ | |
| `denominator` | ✅ | |

### Complex Operations

| Operation | Status | Notes |
|-----------|--------|-------|
| `make-rectangular` | ✅ | Create from real and imaginary parts |
| `make-polar` | ✅ | Create from magnitude and angle |
| `real-part` | ✅ | |
| `imag-part` | ✅ | |
| `magnitude` | ✅ | |
| `angle` | ✅ | |

### Exactness Conversion

| Operation | Status | Notes |
|-----------|--------|-------|
| `exact` | ✅ | Convert to exact representation |
| `inexact` | ✅ | Convert to inexact representation |

## Implementation Notes

### Memory Layout

The numerical tower is implemented using a `Number` enum with the following variants:

```rust
pub enum Number {
    Integer(isize),           // Exact integer
    Float(f64),               // Inexact real
    Rational { num, denom },  // Exact rational
    Complex { real, imag },   // Inexact complex
    ExactComplex { ... },     // Exact complex (rare)
}
```

### Exactness Rules

1. **Integers** are always exact
2. **Rationals** are always exact and automatically reduced to lowest terms
3. **Floats** are always inexact
4. **Operations** preserve exactness when possible:
   - Exact + Exact → Exact
   - Exact + Inexact → Inexact
   - Integer ÷ Integer → Exact Rational

### Limitations

- **No arbitrary precision**: Integers are limited to `isize` range
- **Float precision**: Uses `f64` (IEEE 754 double precision)
- **Rational overflow**: Very large numerator/denominator may overflow
- **Memory usage**: The `Number` enum is 40 bytes (due to `ExactComplex` variant)

## Future Work

- [ ] Trigonometric functions (`sin`, `cos`, `tan`, etc.)
- [ ] Logarithmic functions (`log`, `exp`)
- [ ] Arbitrary precision integers (bignum)
- [ ] `rationalize` procedure
- [ ] `floor/`, `floor-quotient`, `floor-remainder`
- [ ] `truncate/`, `truncate-quotient`, `truncate-remainder`
