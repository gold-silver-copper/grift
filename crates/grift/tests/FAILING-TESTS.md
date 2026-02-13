# Failing R7RS Tests

This document catalogs tests from the Chibi R7RS test suite (`r7rs-tests.scm`) that
are currently commented out because they fail in grift. Tests are grouped by root
cause. Each section explains *why* the tests fail and what would be needed to fix
them.

The tests are marked with `FAILING:` comments in `r7rs-tests.scm` so they can be
easily found and re-enabled as grift's implementation improves.

**Summary:** 993 tests pass, ~176 are commented out.

---

## 1. Complex Number Arithmetic (25 tests)

**Root cause:** Grift's complex number support has precision differences in
transcendental functions (`exp`, `log`, `sin`, `cos`, `atan`, `sqrt` on complex
arguments) and does not correctly handle `+0.0i` vs `0` distinctions.

**Tests affected:**
- `r7rs-53`, `r7rs-54`, `r7rs-55` — `exact-integer-sqrt` on very large integers
  (2^119, 2^120, 2^121) overflows `isize`
- `r7rs-173` — `(real? -2.5+0.0i)` should return `#f` per R7RS but grift
  conflates `+0.0i` with real
- `r7rs-183` — `(integer? 3+0i)` should return `#t`
- `r7rs-194`, `r7rs-198` — `finite?` and `infinite?` on complex with `+inf.0`
  imaginary part
- `r7rs-204` — `(= 1.0 1.0+1.0i)` comparison
- `r7rs-223` — `(zero? 0.0+0.0i)` predicate
- `r7rs-339`–`r7rs-346`, `r7rs-351`–`r7rs-353` — transcendental functions on
  complex arguments (exp, log, sin, cos, atan, sqrt, expt)
- `r7rs-361`, `r7rs-362` — `number->string` for complex numbers
- `r7rs-366` — `atan` with two arguments precision
- Various `test-numeric-syntax` tests for complex literal parsing (`1+2i`,
  `-1-2i`, `+i`, `-i`, `0+1i`, etc.)

**What would fix it:** Implement proper complex number type predicates that
distinguish `x+0.0i` from real `x`. Fix `exact-integer-sqrt` to use arbitrary
precision or fail gracefully on overflow. Improve complex transcendental
precision.

---

## 2. Rational Number Arithmetic (6 tests)

**Root cause:** Grift's rational number implementation does not auto-simplify
results from `/` and has issues with `numerator`/`denominator` and the unary
`(- 3/2)` and `(/ 3)` forms.

**Tests affected:**
- `r7rs-269` — `(- 3/2)` should negate a rational
- `r7rs-271` — `(/ 3 4 5)` should produce `3/20`
- `r7rs-272` — `(/ 3)` should produce `1/3`
- `r7rs-295`, `r7rs-296` — `numerator`/`denominator` of `(/ 6 4)` should
  simplify to `3/2`
- `r7rs-297` — `(denominator (inexact (/ 6 4)))` should give `2.0`

**What would fix it:** Fix rational arithmetic to auto-simplify via GCD, support
unary `/` as reciprocal, and support unary `-` on rationals.

---

## 3. Macro Hygiene & Syntax (7 tests)

**Root cause:** Grift's macro expander does not fully implement R7RS hygienic
macro semantics for several edge cases.

**Tests affected:**
- `r7rs-103` — `let-syntax` macro should capture outer binding of `x`, but
  inner `let` rebinding leaks through
- `r7rs-104` — `letrec-syntax` with `my-or` recursive macro expansion
- `r7rs-109`, `r7rs-110` — Custom ellipsis escape (`(... ...)`) in
  `syntax-rules` templates
- `r7rs-113` — Complex `syntax-rules` pattern with rest arguments and
  multiple match levels
- `r7rs-117`, `r7rs-118` — Underscore (`_`) in `syntax-rules` patterns and
  nested macro definitions
- `r7rs-120`, `r7rs-121` — Macro-generated `define-syntax` forms

**What would fix it:** Implement proper mark-based hygiene that tracks the
lexical environment at macro definition site vs use site. Support custom
ellipsis identifiers. Fix `syntax-rules` template nesting with `...`.

---

## 4. `define-values` (1 test)

**Root cause:** `define-values` with dotted rest parameter
`(define-values (x y . z) (values 1 2 3 4))` is not implemented.

**Tests affected:**
- `r7rs-137` — `define-values` with rest parameter

**What would fix it:** Extend `define-values` to support dotted-pair formals.

---

## 5. Numeric Syntax Parsing (5+ tests)

**Root cause:** Grift's number parser does not support several R7RS numeric
literal forms.

**Tests affected:**
- `#e` (exactness) prefix — `#e-.0`, `#e1/2`, `#e0/10` not parsed
- `#i` (inexactness) prefix — `#i3/2`, `#i+nan.0` not parsed
- Combined prefixes — `#e#x10`, `#i#x1/10`, `#x#i1/10` not parsed
- Exponent markers — `1s2`, `1f2`, `1d2`, `1l2` (short, float, double, long)
  not recognized
- Decimal prefix — `#d.1` not parsed
- `.1` and `-.1` — `number->string` writes `0.1` instead of matching `.1`

**What would fix it:** Extend the lexer to recognize `#e`/`#i` exactness
prefixes, the `s`/`f`/`d`/`l` exponent markers, and combined
radix+exactness prefixes.

---

## 6. Floating-Point Precision (11 tests)

**Root cause:** Grift's `number->string` does not produce shortest-representation
output for subnormal floating-point numbers and extreme values.

**Tests affected:**
- All `test-precision` tests — subnormals like `4.940656458412465e-324` are
  written as `"0.000000000000000000000000000000"` instead of scientific notation
- `1.7976931348623157e+308` written without `+inf.0` alternative
- Round-trip `string->number→number->string` produces different representations

**What would fix it:** Use a shortest-representation algorithm like Ryu or
Dragon4 for `number->string`.

---

## 7. Comparison Operators (2 tests)

**Root cause:** Multi-argument comparison and exact/inexact equality.

**Tests affected:**
- `r7rs-210` — `(<= 1 2 1)` should return `#f` but grift may not chain
  comparisons correctly
- `r7rs-218` — `(= 9007199254740992.0 9007199254740993)` should return `#f`
  (IEEE 754 distinguishability)

**What would fix it:** Fix multi-argument comparison chaining. Ensure exact vs
inexact comparison respects IEEE 754 precision limits.

---

## 8. Quasiquote Vectors (1 test)

**Root cause:** Grift does not evaluate `unquote` and `unquote-splicing` inside
quasiquoted vector literals.

**Tests affected:**
- `r7rs-84` — `` `#(10 5 ,(square 2) ,@(map square '(4 3)) 8) `` should
  produce `#(10 5 4 16 9 8)` but returns the unevaluated template

**What would fix it:** Extend the quasiquote expander to handle `#(...)` vector
templates by converting to `(list->vector (list ...))` during expansion.

---

## 9. Exception Handling (5 tests)

**Root cause:** `guard` with `=>` clauses and nested exception handler
interactions.

**Tests affected:**
- `r7rs-815`, `r7rs-816` — `with-exception-handler` + `guard` interaction
  where the handler sets a flag
- `r7rs-819`, `r7rs-820` — `guard` with multiple clauses and `=>` arrow
  syntax
- `r7rs-827` — `guard` with `(assq 'a condition) => cdr` clause
- `r7rs-828`, `r7rs-829` — `cond-expand` alternative clause selection

**What would fix it:** Fix `guard` to properly implement `=>` arrow clauses
per R7RS §4.2.7. Fix nested handler/guard interactions.

---

## 10. I/O and Write Syntax (2 tests)

**Root cause:** `write` does not produce the shortest or most precise output
for certain values, and `read` syntax for datum comments in dotted pairs.

**Tests affected:**
- `r7rs-890` — `write` of shared/circular structures
- `r7rs-893` — `write` of shared list structures
- Read syntax tests for `#;` datum comments in dotted pairs

**What would fix it:** Implement `write-shared` and `write-simple` per R7RS
§6.13.3. Fix the reader to handle datum comments adjacent to `.` in dotted
pairs.

---

## 11. Miscellaneous (20+ tests)

**Tests affected:**
- `r7rs-319` — `(exp 3)` precision: expected `20.0855369231877`, got
  `20.085536923187668`
- `r7rs-330`, `r7rs-332`, `r7rs-334` — `string-upcase`/`string-downcase`
  edge cases with locale-specific characters
- `r7rs-465`, `r7rs-467`, `r7rs-469`, `r7rs-471`, `r7rs-472` — string
  copy/fill operations
- `r7rs-639` — `vector-map` with multiple vector arguments
- `r7rs-662`, `r7rs-663` — `string->utf8` with start/end indices
- `r7rs-695`, `r7rs-696` — `utf8->string` with start/end indices
- `r7rs-716`, `r7rs-718`, `r7rs-719` — multi-argument `map` edge cases
- `r7rs-750` — `call-with-values` interaction
- `r7rs-770` — `dynamic-wind` edge case
- `r7rs-785`, `r7rs-788`, `r7rs-793` — `guard` re-raise semantics
- `r7rs-901` — `read` of `#true`/`#false` literals
- `r7rs-907`–`r7rs-932` — `write`/`display` formatting for special values,
  characters, strings with escapes, and nested quasiquotes
