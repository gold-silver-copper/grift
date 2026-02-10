# Grift R7RS-small Conformance Report

**Generated:** 2026-02-10
**Repository:** skyfskyf/grift @ `502f2e0`
**Version:** 1.4.0

---

## Executive Summary

| Category | Count | Percentage |
|----------|------:|----------:|
| **Total unique procedures tracked** | **302** | |
| ✅ Fully Implemented | **297** | **98.3%** |
| ⚠️ Partially Implemented | **1** | **0.3%** |
| ❌ Not Implemented | **4** | **1.3%** |

| Category | Count | Percentage |
|----------|------:|----------:|
| **Total R7RS-small syntax forms** | **35** | |
| ✅ Fully Implemented | **35** | **100%** |

| Category | Count | Percentage |
|----------|------:|----------:|
| **Total R7RS-small standard libraries** | **16** | |
| ✅ Fully Implemented | **15** | **93.8%** |
| ⚠️ Partially Implemented | **1** | **6.2%** |

> **Note:** The procedure count of 302 includes all R7RS-small Chapter 6 procedures plus R5RS compatibility names (e.g., both `exact` and `exact->inexact`) and procedures that appear in multiple specification sections (e.g., `map` in both §6.4 and §6.10). The 4 missing items are `(scheme cxr)` 4-level car/cdr combinations.

Grift provides a highly complete R7RS-small implementation with **98%+ procedure coverage**, **100% syntax form coverage**, and all 16 standard libraries. The implementation is notable for its `no_std` design, arena-based allocation, and comprehensive numeric tower including rationals and complex numbers.

---

## Chapter 4: Syntax Forms

| Form | Status | Notes | Location |
|------|--------|-------|----------|
| `quote` | ✅ | Special form in evaluator | `evaluator/core.rs` |
| `lambda` | ✅ | Special form in evaluator | `evaluator/core.rs` |
| `if` | ✅ | Special form in evaluator | `evaluator/core.rs` |
| `set!` | ✅ | Special form in evaluator | `evaluator/core.rs` |
| `include` | ✅ | Special form in evaluator | `evaluator/forms.rs` |
| `include-ci` | ✅ | Special form in evaluator | `evaluator/forms.rs` |
| `cond` | ✅ | Macro in prelude | `prelude.scm` |
| `case` | ✅ | Macro in prelude | `prelude.scm` |
| `and` | ✅ | Macro in prelude | `prelude.scm` |
| `or` | ✅ | Macro in prelude | `prelude.scm` |
| `when` | ✅ | Macro in prelude | `prelude.scm` |
| `unless` | ✅ | Macro in prelude | `prelude.scm` |
| `let` | ✅ | Macro in prelude (incl. named let) | `prelude.scm` |
| `let*` | ✅ | Macro in prelude | `prelude.scm` |
| `letrec` | ✅ | Macro in prelude | `prelude.scm` |
| `letrec*` | ✅ | Macro in prelude | `prelude.scm` |
| `let-values` | ✅ | Macro in prelude | `prelude.scm` |
| `let*-values` | ✅ | Macro in prelude | `prelude.scm` |
| `begin` | ✅ | Special form in evaluator | `evaluator/core.rs` |
| `do` | ✅ | Macro in prelude | `prelude.scm` |
| `delay` | ✅ | Macro in prelude | `prelude.scm` |
| `delay-force` | ✅ | Macro in prelude | `prelude.scm` |
| `quasiquote` | ✅ | Special form in evaluator + prelude | `evaluator/forms.rs`, `prelude.scm` |
| `define` | ✅ | Special form in evaluator | `evaluator/core.rs` |
| `define-values` | ✅ | Macro in prelude | `prelude.scm` |
| `define-syntax` | ✅ | Special form in evaluator | `evaluator/core.rs` |
| `define-record-type` | ✅ | Special form in evaluator | `evaluator/forms.rs` |
| `let-syntax` | ✅ | Special form in evaluator | `evaluator/core.rs` |
| `letrec-syntax` | ✅ | Special form in evaluator | `evaluator/core.rs` |
| `syntax-rules` | ✅ | Macro in prelude (via syntax-case) | `prelude.scm` |
| `syntax-error` | ✅ | Special form in evaluator | `evaluator/core.rs` |
| `parameterize` | ✅ | Macro in prelude (via dynamic-wind) | `prelude.scm` |
| `guard` | ✅ | Macro in prelude (via with-exception-handler) | `prelude.scm` |
| `case-lambda` | ✅ | Macro in prelude | `prelude.scm` |
| `cond-expand` | ✅ | Macro in prelude | `prelude.scm` |

---

## Chapter 6: Standard Procedures

### 6.1 Equivalence Predicates

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `eqv?` | ✅ | Pointer/value identity | `evaluator/builtins.rs` |
| `eq?` | ✅ | Pointer identity | `evaluator/builtins.rs` |
| `equal?` | ✅ | Recursive structural equality; depth-limited (10,000) to handle circular structures | `helpers.rs` |

### 6.2 Numbers

**Type predicates:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `number?` | ✅ | | `evaluator/builtins.rs` |
| `complex?` | ✅ | Defined in prelude | `prelude.scm` |
| `real?` | ✅ | Defined in prelude | `prelude.scm` |
| `rational?` | ✅ | Defined in prelude | `prelude.scm` |
| `integer?` | ✅ | | `evaluator/builtins.rs` |
| `exact?` | ✅ | | `evaluator/builtins.rs` |
| `inexact?` | ✅ | | `evaluator/builtins.rs` |
| `exact-integer?` | ✅ | Defined in prelude | `prelude.scm` |

**Exactness conversion:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `exact` | ✅ | R7RS name | `evaluator/builtins.rs` |
| `inexact` | ✅ | R7RS name | `evaluator/builtins.rs` |
| `exact->inexact` | ✅ | R5RS compatibility name | `evaluator/builtins.rs` |
| `inexact->exact` | ✅ | R5RS compatibility name | `evaluator/builtins.rs` |

**Arithmetic:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `+` | ✅ | Variadic, supports exact/inexact/complex | `evaluator/builtins.rs` |
| `-` | ✅ | Variadic, supports negation and subtraction | `evaluator/builtins.rs` |
| `*` | ✅ | Variadic | `evaluator/builtins.rs` |
| `/` | ✅ | Variadic, exact division produces rationals | `evaluator/builtins.rs` |
| `abs` | ✅ | Defined in prelude | `prelude.scm` |
| `floor/` | ✅ | Returns two values | `evaluator/builtins.rs` |
| `floor-quotient` | ✅ | | `evaluator/builtins.rs` |
| `floor-remainder` | ✅ | | `evaluator/builtins.rs` |
| `truncate/` | ✅ | Returns two values | `evaluator/builtins.rs` |
| `truncate-quotient` | ✅ | | `evaluator/builtins.rs` |
| `truncate-remainder` | ✅ | | `evaluator/builtins.rs` |

**Comparison:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `=` | ✅ | Chain comparison, supports exact/inexact/complex | `evaluator/builtins.rs` |
| `<` | ✅ | Chain comparison | `evaluator/builtins.rs` |
| `>` | ✅ | Chain comparison | `evaluator/builtins.rs` |
| `<=` | ✅ | Chain comparison | `evaluator/builtins.rs` |
| `>=` | ✅ | Chain comparison | `evaluator/builtins.rs` |

**Min/max, parity, sign:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `max` | ✅ | Defined in prelude | `prelude.scm` |
| `min` | ✅ | Defined in prelude | `prelude.scm` |
| `even?` | ✅ | Defined in prelude | `prelude.scm` |
| `odd?` | ✅ | Defined in prelude | `prelude.scm` |
| `positive?` | ✅ | Defined in prelude | `prelude.scm` |
| `negative?` | ✅ | Defined in prelude | `prelude.scm` |
| `zero?` | ✅ | Defined in prelude | `prelude.scm` |

**Division (R5RS compatibility):**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `quotient` | ✅ | | `evaluator/builtins.rs` |
| `remainder` | ✅ | | `evaluator/builtins.rs` |
| `modulo` | ✅ | | `evaluator/builtins.rs` |
| `gcd` | ✅ | Defined in prelude | `prelude.scm` |
| `lcm` | ✅ | Defined in prelude | `prelude.scm` |
| `numerator` | ✅ | | `evaluator/builtins.rs` |
| `denominator` | ✅ | | `evaluator/builtins.rs` |

**Rounding:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `floor` | ✅ | | `evaluator/builtins.rs` |
| `ceiling` | ✅ | | `evaluator/builtins.rs` |
| `truncate` | ✅ | | `evaluator/builtins.rs` |
| `round` | ✅ | | `evaluator/builtins.rs` |

**Rationalize:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `rationalize` | ✅ | | `evaluator/builtins.rs` |

**Exponentiation:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `expt` | ✅ | | `evaluator/builtins.rs` |
| `sqrt` | ✅ | | `evaluator/builtins.rs` |
| `exact-integer-sqrt` | ✅ | Returns two values (root, remainder) | `evaluator/builtins.rs` |
| `square` | ✅ | Defined in prelude | `prelude.scm` |

**Transcendental:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `exp` | ✅ | | `evaluator/builtins.rs` |
| `log` | ✅ | Supports optional base argument | `evaluator/builtins.rs` |
| `sin` | ✅ | | `evaluator/builtins.rs` |
| `cos` | ✅ | | `evaluator/builtins.rs` |
| `tan` | ✅ | | `evaluator/builtins.rs` |
| `asin` | ✅ | | `evaluator/builtins.rs` |
| `acos` | ✅ | | `evaluator/builtins.rs` |
| `atan` | ✅ | Supports both 1-arg and 2-arg (atan2) forms | `evaluator/builtins.rs` |

**Complex numbers:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `make-rectangular` | ✅ | | `evaluator/builtins.rs` |
| `make-polar` | ✅ | | `evaluator/builtins.rs` |
| `real-part` | ✅ | | `evaluator/builtins.rs` |
| `imag-part` | ✅ | | `evaluator/builtins.rs` |
| `magnitude` | ✅ | | `evaluator/builtins.rs` |
| `angle` | ✅ | | `evaluator/builtins.rs` |

**Conversion:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `number->string` | ✅ | Supports optional radix (2, 8, 10, 16) | `evaluator/builtins.rs` |
| `string->number` | ✅ | Supports optional radix | `evaluator/builtins.rs` |

**Inexact-specific predicates:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `finite?` | ✅ | | `evaluator/builtins.rs` |
| `infinite?` | ✅ | | `evaluator/builtins.rs` |
| `nan?` | ✅ | | `evaluator/builtins.rs` |

### 6.3 Booleans

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `not` | ✅ | Defined in prelude | `prelude.scm` |
| `boolean?` | ✅ | | `evaluator/builtins.rs` |
| `boolean=?` | ✅ | Defined in prelude | `prelude.scm` |

### 6.4 Pairs and Lists

**Core pair operations:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `pair?` | ✅ | | `evaluator/builtins.rs` |
| `cons` | ✅ | | `evaluator/builtins.rs` |
| `car` | ✅ | | `evaluator/builtins.rs` |
| `cdr` | ✅ | | `evaluator/builtins.rs` |
| `set-car!` | ✅ | | `evaluator/builtins.rs` |
| `set-cdr!` | ✅ | | `evaluator/builtins.rs` |

**Car/cdr combinations (2-level, in `(scheme base)`):**

| Procedure | Status | Location |
|-----------|--------|----------|
| `caar` | ✅ | `prelude.scm` |
| `cadr` | ✅ | `prelude.scm` |
| `cdar` | ✅ | `prelude.scm` |
| `cddr` | ✅ | `prelude.scm` |

**Car/cdr combinations (3-level):**

| Procedure | Status | Location |
|-----------|--------|----------|
| `caaar` | ✅ | `prelude.scm` |
| `caadr` | ✅ | `prelude.scm` |
| `cadar` | ✅ | `prelude.scm` |
| `caddr` | ✅ | `prelude.scm` |
| `cdaar` | ✅ | `prelude.scm` |
| `cdadr` | ✅ | `prelude.scm` |
| `cddar` | ✅ | `prelude.scm` |
| `cdddr` | ✅ | `prelude.scm` |

**Car/cdr combinations (4-level, `(scheme cxr)`):**

| Procedure | Status | Location |
|-----------|--------|----------|
| `caaaar` | ✅ | `prelude.scm` |
| `caaadr` | ✅ | `prelude.scm` |
| `caadar` | ✅ | `prelude.scm` |
| `caaddr` | ✅ | `prelude.scm` |
| `cadaar` | ✅ | `prelude.scm` |
| `cadadr` | ✅ | `prelude.scm` |
| `caddar` | ✅ | `prelude.scm` |
| `cadddr` | ✅ | `prelude.scm` |
| `cdaaar` | ✅ | `prelude.scm` |
| `cdaadr` | ✅ | `prelude.scm` |
| `cdadar` | ✅ | `prelude.scm` |
| `cdaddr` | ❌ | Missing from prelude and library | — |
| `cddaar` | ❌ | Missing from prelude and library | — |
| `cddadr` | ❌ | Missing from prelude and library | — |
| `cdddar` | ❌ | Missing from prelude and library | — |
| `cddddr` | ✅ | `prelude.scm` |

**Null and list operations:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `null?` | ✅ | | `evaluator/builtins.rs` |
| `list?` | ✅ | Defined in prelude | `prelude.scm` |
| `make-list` | ✅ | Defined in prelude | `prelude.scm` |
| `list` | ✅ | | `evaluator/builtins.rs` |
| `length` | ✅ | Defined in prelude | `prelude.scm` |
| `append` | ✅ | Defined in prelude (variadic) | `prelude.scm` |
| `reverse` | ✅ | Defined in prelude | `prelude.scm` |
| `list-tail` | ✅ | Defined in prelude | `prelude.scm` |
| `list-ref` | ✅ | Defined in prelude | `prelude.scm` |
| `list-set!` | ✅ | Defined in prelude | `prelude.scm` |
| `list-copy` | ✅ | Defined in prelude | `prelude.scm` |

**Mapping:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `map` | ✅ | Supports multi-list mapping | `prelude.scm` |
| `for-each` | ✅ | Supports multi-list iteration | `prelude.scm` |

**Membership:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `memq` | ✅ | Uses eq? | `prelude.scm` |
| `memv` | ✅ | Uses eqv? | `prelude.scm` |
| `member` | ✅ | Uses equal?, supports optional comparator | `prelude.scm` |

**Association lists:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `assq` | ✅ | Uses eq? | `prelude.scm` |
| `assv` | ✅ | Uses eqv? | `prelude.scm` |
| `assoc` | ✅ | Uses equal?, supports optional comparator | `prelude.scm` |

### 6.5 Symbols

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `symbol?` | ✅ | | `evaluator/builtins.rs` |
| `symbol=?` | ✅ | Defined in prelude | `prelude.scm` |
| `symbol->string` | ✅ | | `evaluator/builtins.rs` |
| `string->symbol` | ✅ | | `evaluator/builtins.rs` |

### 6.6 Characters

**Type:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `char?` | ✅ | | `evaluator/builtins.rs` |

**Comparison:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `char=?` | ✅ | | `evaluator/builtins.rs` |
| `char<?` | ✅ | | `evaluator/builtins.rs` |
| `char>?` | ✅ | | `evaluator/builtins.rs` |
| `char<=?` | ✅ | | `evaluator/builtins.rs` |
| `char>=?` | ✅ | | `evaluator/builtins.rs` |

**Case-insensitive comparison:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `char-ci=?` | ✅ | Defined in prelude | `prelude.scm` |
| `char-ci<?` | ✅ | Defined in prelude | `prelude.scm` |
| `char-ci>?` | ✅ | Defined in prelude | `prelude.scm` |
| `char-ci<=?` | ✅ | Defined in prelude | `prelude.scm` |
| `char-ci>=?` | ✅ | Defined in prelude | `prelude.scm` |

**Classification:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `char-alphabetic?` | ✅ | ASCII only | `prelude.scm` |
| `char-numeric?` | ✅ | ASCII only | `prelude.scm` |
| `char-whitespace?` | ✅ | Space, tab, newline, CR, formfeed | `prelude.scm` |
| `char-upper-case?` | ✅ | ASCII only | `prelude.scm` |
| `char-lower-case?` | ✅ | ASCII only | `prelude.scm` |

**Conversion:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `digit-value` | ✅ | Returns 0–9 or `#f` | `prelude.scm` |
| `char->integer` | ✅ | | `evaluator/builtins.rs` |
| `integer->char` | ✅ | | `evaluator/builtins.rs` |
| `char-upcase` | ✅ | ASCII only | `evaluator/builtins.rs` |
| `char-downcase` | ✅ | ASCII only | `evaluator/builtins.rs` |
| `char-foldcase` | ✅ | Delegates to char-downcase (ASCII) | `prelude.scm` |

### 6.7 Strings

**Construction and access:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `string?` | ✅ | | `evaluator/builtins.rs` |
| `make-string` | ✅ | | `evaluator/builtins.rs` |
| `string` | ✅ | Construct from chars | `evaluator/builtins.rs` |
| `string-length` | ✅ | | `evaluator/builtins.rs` |
| `string-ref` | ✅ | | `evaluator/builtins.rs` |
| `string-set!` | ✅ | | `evaluator/builtins.rs` |

**Comparison:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `string=?` | ✅ | | `evaluator/builtins.rs` |
| `string<?` | ✅ | | `evaluator/builtins.rs` |
| `string>?` | ✅ | | `evaluator/builtins.rs` |
| `string<=?` | ✅ | | `evaluator/builtins.rs` |
| `string>=?` | ✅ | | `evaluator/builtins.rs` |

**Case-insensitive comparison:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `string-ci=?` | ✅ | Defined in prelude | `prelude.scm` |
| `string-ci<?` | ✅ | | `evaluator/builtins.rs` |
| `string-ci>?` | ✅ | | `evaluator/builtins.rs` |
| `string-ci<=?` | ✅ | | `evaluator/builtins.rs` |
| `string-ci>=?` | ✅ | | `evaluator/builtins.rs` |

**Case conversion:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `string-upcase` | ✅ | ASCII only | `prelude.scm` |
| `string-downcase` | ✅ | ASCII only | `prelude.scm` |
| `string-foldcase` | ✅ | ASCII only | `prelude.scm` |

**Substring and manipulation:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `substring` | ✅ | | `evaluator/builtins.rs` |
| `string-append` | ✅ | Variadic | `evaluator/builtins.rs` |
| `string->list` | ✅ | | `evaluator/builtins.rs` |
| `list->string` | ✅ | | `evaluator/builtins.rs` |
| `string-copy` | ✅ | Supports optional start/end | `evaluator/builtins.rs` |
| `string-copy!` | ✅ | | `evaluator/builtins.rs` |
| `string-fill!` | ✅ | | `evaluator/builtins.rs` |

**Iteration:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `string-map` | ✅ | Defined in prelude | `prelude.scm` |
| `string-for-each` | ✅ | Defined in prelude | `prelude.scm` |

### 6.8 Vectors

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `vector?` | ✅ | | `evaluator/builtins.rs` |
| `make-vector` | ✅ | | `evaluator/builtins.rs` |
| `vector` | ✅ | | `evaluator/builtins.rs` |
| `vector-length` | ✅ | | `evaluator/builtins.rs` |
| `vector-ref` | ✅ | | `evaluator/builtins.rs` |
| `vector-set!` | ✅ | | `evaluator/builtins.rs` |
| `vector->list` | ✅ | Supports optional start/end | `evaluator/builtins.rs` |
| `list->vector` | ✅ | | `evaluator/builtins.rs` |
| `vector->string` | ✅ | Supports optional start/end | `evaluator/builtins.rs` |
| `string->vector` | ✅ | Supports optional start/end | `evaluator/builtins.rs` |
| `vector-copy` | ✅ | Supports optional start/end | `evaluator/builtins.rs` |
| `vector-copy!` | ✅ | | `evaluator/builtins.rs` |
| `vector-append` | ✅ | Variadic | `evaluator/builtins.rs` |
| `vector-fill!` | ✅ | | `evaluator/builtins.rs` |
| `vector-map` | ✅ | | `evaluator/builtins.rs` |
| `vector-for-each` | ✅ | | `evaluator/builtins.rs` |

### 6.9 Bytevectors

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `bytevector?` | ✅ | | `evaluator/builtins.rs` |
| `make-bytevector` | ✅ | | `evaluator/builtins.rs` |
| `bytevector` | ✅ | Construct from byte values | `evaluator/builtins.rs` |
| `bytevector-length` | ✅ | | `evaluator/builtins.rs` |
| `bytevector-u8-ref` | ✅ | | `evaluator/builtins.rs` |
| `bytevector-u8-set!` | ✅ | | `evaluator/builtins.rs` |
| `bytevector-copy` | ✅ | Supports optional start/end | `evaluator/builtins.rs` |
| `bytevector-copy!` | ✅ | | `evaluator/builtins.rs` |
| `bytevector-append` | ✅ | Variadic | `evaluator/builtins.rs` |
| `utf8->string` | ✅ | Supports optional start/end, multi-byte | `evaluator/builtins.rs` |
| `string->utf8` | ✅ | Supports optional start/end | `evaluator/builtins.rs` |

### 6.10 Control Features

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `apply` | ✅ | Special form in evaluator | `evaluator/core.rs` |
| `procedure?` | ✅ | | `evaluator/builtins.rs` |
| `map` | ✅ | Multi-list support | `prelude.scm` |
| `string-map` | ✅ | | `prelude.scm` |
| `vector-map` | ✅ | | `evaluator/builtins.rs` |
| `for-each` | ✅ | Multi-list support | `prelude.scm` |
| `string-for-each` | ✅ | | `prelude.scm` |
| `vector-for-each` | ✅ | | `evaluator/builtins.rs` |
| `promise?` | ✅ | | `prelude.scm` |
| `make-promise` | ✅ | | `prelude.scm` |
| `force` | ✅ | With memoization | `prelude.scm` |
| `delay` | ✅ | Macro | `prelude.scm` |
| `delay-force` | ✅ | Iterative forcing (R7RS) | `prelude.scm` |
| `make-parameter` | ✅ | Supports optional converter | `prelude.scm` |
| `call-with-current-continuation` | ✅ | O(1) arena-based capture | `evaluator/forms.rs` |
| `call/cc` | ✅ | Alias | `evaluator/forms.rs` |
| `values` | ✅ | Special form | `evaluator/core.rs` |
| `call-with-values` | ✅ | Special form | `evaluator/forms.rs` |
| `dynamic-wind` | ✅ | Full before/after thunk support | `evaluator/forms.rs` |

### 6.11 Exceptions

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `raise` | ✅ | Non-continuable | `evaluator/core.rs` |
| `raise-continuable` | ✅ | Continuable exceptions | `evaluator/core.rs` |
| `error` | ✅ | Creates error object and raises | `evaluator/builtins.rs` |
| `error-object?` | ✅ | | `evaluator/builtins.rs` |
| `error-object-message` | ✅ | | `evaluator/builtins.rs` |
| `error-object-irritants` | ✅ | | `evaluator/builtins.rs` |
| `error-object-type` | ⚠️ | Implemented but non-standard name (Grift extension) | `evaluator/builtins.rs` |
| `read-error?` | ✅ | | `evaluator/builtins.rs` |
| `file-error?` | ✅ | | `evaluator/builtins.rs` |
| `with-exception-handler` | ✅ | Special form | `evaluator/forms.rs` |
| `guard` | ✅ | Macro (via with-exception-handler) | `prelude.scm` |

### 6.12 Environments and Evaluation

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `environment` | ✅ | Builds immutable environment from library specs | `evaluator/core.rs` |
| `eval` | ✅ | 1-arg (global env) and 2-arg forms | `evaluator/core.rs` |
| `scheme-report-environment` | ✅ | Returns R5RS environment | `evaluator/builtins.rs` |
| `null-environment` | ✅ | Returns empty environment with syntax only | `evaluator/builtins.rs` |
| `interaction-environment` | ✅ | Returns mutable global environment | `evaluator/builtins.rs` |

### 6.13 Input and Output

**Port types and predicates:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `port?` | ✅ | | `evaluator/builtins.rs` |
| `input-port?` | ✅ | | `evaluator/builtins.rs` |
| `output-port?` | ✅ | | `evaluator/builtins.rs` |
| `textual-port?` | ✅ | | `evaluator/builtins.rs` |
| `binary-port?` | ✅ | | `evaluator/builtins.rs` |
| `input-port-open?` | ✅ | | `evaluator/builtins.rs` |
| `output-port-open?` | ✅ | | `evaluator/builtins.rs` |

**Standard ports:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `current-input-port` | ✅ | | `evaluator/builtins.rs` |
| `current-output-port` | ✅ | | `evaluator/builtins.rs` |
| `current-error-port` | ✅ | | `evaluator/builtins.rs` |

**Port lifecycle:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `call-with-port` | ✅ | | `evaluator/builtins.rs` |
| `call-with-input-file` | ✅ | | `evaluator/builtins.rs` |
| `call-with-output-file` | ✅ | | `evaluator/builtins.rs` |
| `close-port` | ✅ | | `evaluator/builtins.rs` |
| `close-input-port` | ✅ | | `evaluator/builtins.rs` |
| `close-output-port` | ✅ | | `evaluator/builtins.rs` |

**File ports:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `open-input-file` | ✅ | Requires `std` feature | `evaluator/builtins.rs` |
| `open-binary-input-file` | ✅ | | `evaluator/builtins.rs` |
| `open-output-file` | ✅ | Requires `std` feature | `evaluator/builtins.rs` |
| `open-binary-output-file` | ✅ | | `evaluator/builtins.rs` |
| `with-input-from-file` | ✅ | | `evaluator/builtins.rs` |
| `with-output-to-file` | ✅ | | `evaluator/builtins.rs` |

**String ports:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `open-input-string` | ✅ | | `evaluator/builtins.rs` |
| `open-output-string` | ✅ | | `evaluator/builtins.rs` |
| `get-output-string` | ✅ | | `evaluator/builtins.rs` |

**Bytevector ports:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `open-input-bytevector` | ✅ | | `evaluator/builtins.rs` |
| `open-output-bytevector` | ✅ | | `evaluator/builtins.rs` |
| `get-output-bytevector` | ✅ | | `evaluator/builtins.rs` |

**Reading (textual):**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `read` | ✅ | Full S-expression reader | `evaluator/builtins.rs` |
| `read-char` | ✅ | | `evaluator/builtins.rs` |
| `peek-char` | ✅ | | `evaluator/builtins.rs` |
| `read-line` | ✅ | | `evaluator/builtins.rs` |
| `eof-object?` | ✅ | | `evaluator/builtins.rs` |
| `eof-object` | ✅ | Returns the EOF object | `evaluator/builtins.rs` |
| `char-ready?` | ✅ | | `evaluator/builtins.rs` |
| `read-string` | ✅ | | `evaluator/builtins.rs` |

**Reading (binary):**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `read-u8` | ✅ | | `evaluator/builtins.rs` |
| `peek-u8` | ✅ | | `evaluator/builtins.rs` |
| `u8-ready?` | ✅ | | `evaluator/builtins.rs` |
| `read-bytevector` | ✅ | Stack-buffered (2048 bytes max) | `evaluator/builtins.rs` |
| `read-bytevector!` | ✅ | Supports start/end indices | `evaluator/builtins.rs` |

**Writing:**

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `write` | ✅ | With datum labels for cycles | `evaluator/builtins.rs` |
| `write-shared` | ✅ | Explicit shared structure output | `evaluator/builtins.rs` |
| `write-simple` | ✅ | No datum labels | `evaluator/builtins.rs` |
| `display` | ✅ | Human-readable output | `evaluator/builtins.rs` |
| `newline` | ✅ | | `evaluator/builtins.rs` |
| `write-char` | ✅ | | `evaluator/builtins.rs` |
| `write-string` | ✅ | | `evaluator/builtins.rs` |
| `write-u8` | ✅ | | `evaluator/builtins.rs` |
| `write-bytevector` | ✅ | | `evaluator/builtins.rs` |
| `flush-output-port` | ✅ | | `evaluator/builtins.rs` |

### 6.14 System Interface

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `load` | ✅ | Requires `std` feature | `evaluator/builtins.rs` |
| `file-exists?` | ✅ | Requires `std` feature | `evaluator/builtins.rs` |
| `delete-file` | ✅ | Requires `std` feature | `evaluator/builtins.rs` |
| `command-line` | ✅ | | `evaluator/builtins.rs` |
| `exit` | ✅ | | `evaluator/builtins.rs` |
| `emergency-exit` | ✅ | | `evaluator/builtins.rs` |
| `get-environment-variable` | ✅ | | `evaluator/builtins.rs` |
| `get-environment-variables` | ✅ | | `evaluator/builtins.rs` |
| `current-second` | ✅ | TAI seconds since epoch | `evaluator/builtins.rs` |
| `current-jiffy` | ✅ | High-resolution timer | `evaluator/builtins.rs` |
| `jiffies-per-second` | ✅ | | `evaluator/builtins.rs` |
| `features` | ✅ | Returns `(r7rs grift exact-closed)` | `prelude.scm` |

---

## Appendix A: Standard Libraries

### `(scheme base)` — Core Language

**Completeness: ~99%** (exports 220+ identifiers)

All required `(scheme base)` identifiers are exported and implemented. This includes:
- All arithmetic, comparison, and type-checking procedures
- All list, vector, string, bytevector, and character operations
- All control flow: `apply`, `call/cc`, `values`, `dynamic-wind`
- All exception handling: `with-exception-handler`, `raise`, `guard`
- All I/O: ports, reading, writing
- All syntax forms: `let`, `cond`, `case`, `do`, `define-record-type`, etc.
- Promises: `delay`, `force`, `delay-force`, `make-promise`, `promise?`
- Parameters: `make-parameter`, `parameterize`
- Feature detection: `features`, `cond-expand`

### `(scheme case-lambda)`

**Completeness: 100%** (1/1)

| Export | Status |
|--------|--------|
| `case-lambda` | ✅ |

### `(scheme char)`

**Completeness: 100%** (18/18)

| Export | Status |
|--------|--------|
| `char-alphabetic?` | ✅ |
| `char-numeric?` | ✅ |
| `char-whitespace?` | ✅ |
| `char-upper-case?` | ✅ |
| `char-lower-case?` | ✅ |
| `char-ci=?` | ✅ |
| `char-ci<?` | ✅ |
| `char-ci>?` | ✅ |
| `char-ci<=?` | ✅ |
| `char-ci>=?` | ✅ |
| `char-upcase` | ✅ |
| `char-downcase` | ✅ |
| `char-foldcase` | ✅ |
| `digit-value` | ✅ |
| `string-ci=?` | ✅ |
| `string-upcase` | ✅ |
| `string-downcase` | ✅ |
| `string-foldcase` | ✅ |

> **Note:** Character classification and case operations are ASCII-only; full Unicode support is not yet implemented.

### `(scheme complex)`

**Completeness: 100%** (6/6)

| Export | Status |
|--------|--------|
| `make-rectangular` | ✅ |
| `make-polar` | ✅ |
| `real-part` | ✅ |
| `imag-part` | ✅ |
| `magnitude` | ✅ |
| `angle` | ✅ |

### `(scheme cxr)`

**Completeness: 83%** (20/24)

All 2-level and 3-level combinations are implemented. Four 4-level combinations are missing:

| Missing Export | Status |
|----------------|--------|
| `cdaddr` | ❌ |
| `cddaar` | ❌ |
| `cddadr` | ❌ |
| `cdddar` | ❌ |

### `(scheme eval)`

**Completeness: 100%** (2/2)

| Export | Status |
|--------|--------|
| `eval` | ✅ |
| `environment` | ✅ |

### `(scheme file)`

**Completeness: 100%** (10/10)

| Export | Status |
|--------|--------|
| `file-exists?` | ✅ |
| `delete-file` | ✅ |
| `call-with-input-file` | ✅ |
| `call-with-output-file` | ✅ |
| `with-input-from-file` | ✅ |
| `with-output-to-file` | ✅ |
| `open-input-file` | ✅ |
| `open-output-file` | ✅ |
| `open-binary-input-file` | ✅ |
| `open-binary-output-file` | ✅ |

### `(scheme inexact)`

**Completeness: 100%** (14/14)

| Export | Status |
|--------|--------|
| `finite?` | ✅ |
| `infinite?` | ✅ |
| `nan?` | ✅ |
| `sqrt` | ✅ |
| `exp` | ✅ |
| `log` | ✅ |
| `sin` | ✅ |
| `cos` | ✅ |
| `tan` | ✅ |
| `asin` | ✅ |
| `acos` | ✅ |
| `atan` | ✅ |
| `exact->inexact` | ✅ |
| `inexact->exact` | ✅ |

### `(scheme lazy)`

**Completeness: 100%** (5/5)

| Export | Status |
|--------|--------|
| `delay` | ✅ |
| `force` | ✅ |
| `delay-force` | ✅ |
| `make-promise` | ✅ |
| `promise?` | ✅ |

### `(scheme load)`

**Completeness: 100%** (1/1)

| Export | Status |
|--------|--------|
| `load` | ✅ |

### `(scheme process-context)`

**Completeness: 100%** (5/5)

| Export | Status |
|--------|--------|
| `command-line` | ✅ |
| `exit` | ✅ |
| `emergency-exit` | ✅ |
| `get-environment-variable` | ✅ |
| `get-environment-variables` | ✅ |

### `(scheme read)`

**Completeness: 100%** (1/1)

| Export | Status |
|--------|--------|
| `read` | ✅ |

### `(scheme repl)`

**Completeness: 100%** (1/1)

| Export | Status |
|--------|--------|
| `interaction-environment` | ✅ |

### `(scheme time)`

**Completeness: 100%** (3/3)

| Export | Status |
|--------|--------|
| `current-second` | ✅ |
| `current-jiffy` | ✅ |
| `jiffies-per-second` | ✅ |

### `(scheme write)`

**Completeness: 100%** (7/7)

| Export | Status |
|--------|--------|
| `write` | ✅ |
| `display` | ✅ |
| `write-shared` | ✅ |
| `write-simple` | ✅ |
| `write-char` | ✅ |
| `write-string` | ✅ |
| `newline` | ✅ |

### `(scheme r5rs)`

**Completeness: 100%**

All R5RS compatibility identifiers are exported and functional, including the full set of arithmetic, list, string, character, vector, and I/O operations from R5RS.

---

## Number Syntax Support

| Feature | Status | Examples | Location |
|---------|--------|----------|----------|
| Integer literals | ✅ | `42`, `-7` | `lexer.rs` |
| Floating point | ✅ | `3.14`, `-0.5` | `lexer.rs` |
| Scientific notation | ✅ | `1e10`, `1.5e-3` | `lexer.rs` |
| Rational literals | ✅ | `3/4`, `-1/2` | `lexer.rs` |
| Complex (rectangular) | ✅ | `1+2i`, `3-4i` | `lexer.rs` |
| Complex (polar) | ✅ | `5@1.57` | `lexer.rs` |
| Binary prefix `#b` | ✅ | `#b1010` → 10 | `lexer.rs` |
| Octal prefix `#o` | ✅ | `#o17` → 15 | `lexer.rs` |
| Decimal prefix `#d` | ✅ | `#d42` → 42 | `lexer.rs` |
| Hexadecimal prefix `#x` | ✅ | `#xff` → 255 | `lexer.rs` |
| Exact prefix `#e` | ✅ | `#e1.5` → 3/2 | `lexer.rs` |
| Inexact prefix `#i` | ✅ | `#i3` → 3.0 | `lexer.rs` |
| `+inf.0` | ✅ | Positive infinity | `lexer.rs` |
| `-inf.0` | ✅ | Negative infinity | `lexer.rs` |
| `+nan.0` | ✅ | Not-a-number | `lexer.rs` |
| `-nan.0` | ✅ | Not-a-number | `lexer.rs` |

---

## Implementation Priorities

### High Priority — Missing Core Procedures

These 4 missing `(scheme cxr)` procedures are trivial to add:

| Procedure | Definition |
|-----------|-----------|
| `cdaddr` | `(define (cdaddr lst) (cdr (car (cdr (cdr lst)))))` |
| `cddaar` | `(define (cddaar lst) (cdr (cdr (car (car lst)))))` |
| `cddadr` | `(define (cddadr lst) (cdr (cdr (car (cdr lst)))))` |
| `cdddar` | `(define (cdddar lst) (cdr (cdr (cdr (car lst)))))` |

### Medium Priority — Partial Implementations

| Feature | Current State | Work Needed |
|---------|---------------|-------------|
| Unicode character support | ASCII-only for classification/case | Full Unicode tables for `char-alphabetic?`, `char-upcase`, etc. |
| `equal?` cycle detection | Depth-limited (10,000) | True tortoise-and-hare cycle detection per R7RS |
| `error-object-type` | Non-standard extension | Rename or align with R7RS conventions |
| Tail context in `guard` | Basic implementation | Verify tail-call behavior matches R7RS §4.2.7 |
| Stack-buffered `read-bytevector` | 2048-byte limit | Configurable or arena-based allocation |

### Low Priority — Optional/Nice-to-Have

| Feature | Notes |
|---------|-------|
| Full Unicode support | Would affect `(scheme char)` library operations |
| Datum labels in `read` | `#n=` / `#n#` for reading circular structures |
| `syntax-rules` `_` literal | Verify underscore handling in pattern matching |
| Multiple return values optimization | Currently uses list representation |

---

## Test Coverage Map

### Well-Tested Areas ✅

| Area | Test File(s) | Coverage |
|------|-------------|----------|
| Rational numbers | `r7rs_compliance_tests.rs`, `r7rs_numeric_tests.rs` | Comprehensive |
| Complex numbers | `r7rs_compliance_tests.rs`, `r7rs_numeric_tests.rs` | Comprehensive |
| Division procedures | `r7rs_numeric_tests.rs` | `floor/`, `truncate/`, quotient, remainder |
| Transcendental functions | `r7rs_numeric_tests.rs` | exp, log, sin, cos, tan, asin, acos, atan |
| Number conversion | `r7rs_numeric_tests.rs` | Radix support for `number->string`, `string->number` |
| Bytevectors | `bytevector_tests.rs`, `r7rs_compliance_tests.rs` | All operations |
| UTF-8 conversion | `bytevector_tests.rs` | Multi-byte, range support |
| Continuations | `continuation_delay_tests.rs` | call/cc, dynamic-wind, unwind-protect |
| Lazy evaluation | `continuation_delay_tests.rs` | delay, force, streams |
| File I/O | `io_tests.rs` | File/string/bytevector ports, binary I/O |
| Libraries | `library_tests.rs` | All 16 libraries, import modifiers |
| Environments | `environment_tests.rs` | eval, environment, interaction-environment |
| System interface | `system_tests.rs` | file-exists?, command-line, load |
| Time procedures | `r7rs_new_procedures_tests.rs` | current-second, current-jiffy |
| Error predicates | `r7rs_new_procedures_tests.rs` | read-error?, file-error? |
| Vector-string | `r7rs_new_procedures_tests.rs` | vector->string, string->vector |
| R5RS compatibility | `r5rs_chibi_tests.rs`, `peroxide_r5rs_tests.rs` | Broad coverage |
| Syntax/macros | `syntax_proper_tests.rs`, `syntax_extended_tests.rs` | Extensive |

### Areas Needing More Tests ⚠️

| Area | Notes |
|------|-------|
| `parameterize` | Tested via guard but needs dedicated tests |
| `define-record-type` | Needs field mutator tests |
| `string-map`, `string-for-each` | Defined in prelude, limited test coverage |
| `case-lambda` | Needs arity-dispatch edge case tests |
| `cond-expand` | Feature-based expansion testing |
| 4-level c\*r procedures | Missing procedures not tested |
| `write-shared` vs `write` | Shared structure output differences |
| Binary port operations | `read-bytevector!` range edge cases |

---

## Detailed Evidence

### Implementation Architecture

Grift implements R7RS-small across multiple layers:

1. **Native builtins** (`crates/grift_eval/src/evaluator/builtins.rs`): 176+ procedures implemented in Rust for performance. These include all arithmetic, I/O, string, vector, bytevector, and port operations.

2. **Special forms** (`crates/grift_eval/src/evaluator/core.rs`, `forms.rs`): 28 forms handled directly by the evaluator including `if`, `lambda`, `define`, `set!`, `quote`, `begin`, `eval`, `apply`, `call/cc`, `dynamic-wind`, `values`, `with-exception-handler`, `raise`, `define-record-type`, `define-library`, `import`, `include`.

3. **Prelude** (`crates/grift_core/src/prelude.scm`): 159+ procedures and macros written in Scheme. Includes all derived expressions (`let`, `cond`, `case`, `do`, `guard`, `parameterize`), list operations (`map`, `filter`, `fold`), higher-order utilities, and c\*r combinations.

4. **Libraries** (`crates/grift_core/src/libraries.rs` + `lib/scheme/*.scm`): All 16 R7RS standard libraries with feature-gated compilation for embedded/no_std use.

5. **Parser/Lexer** (`crates/grift_parser/src/lexer.rs`): Full R7RS number syntax including radix prefixes, exactness prefixes, rationals, complex numbers (rectangular and polar), scientific notation, and special float constants.

### Known Limitations

| Limitation | Impact | Workaround |
|-----------|--------|------------|
| `no_std` design | No heap allocation in core crates | Arena allocator provides equivalent functionality |
| ASCII-only character operations | Unicode characters not classified correctly | Sufficient for most Western text processing |
| Arena-based strings | Fixed-size string buffers | Configurable arena size at initialization |
| `read-bytevector` stack buffer | 2048-byte limit per read | Multiple reads for larger data |
| `equal?` depth limit | 10,000 levels max | Sufficient for practical use; prevents stack overflow |

### Non-Standard Extensions

Grift provides several extensions beyond R7RS-small:

| Extension | Description |
|-----------|-------------|
| `gc`, `gc-enable`, `gc-disable`, `gc-enabled?` | Manual garbage collection control |
| `arena-stats` | Arena allocator statistics |
| `error-object-type` | Error type classification |
| `syntax-case` | R6RS-style macro system (superset of `syntax-rules`) |
| `identifier?`, `bound-identifier=?`, `free-identifier=?` | Syntax object operations |
| `datum->syntax`, `syntax->datum` | Syntax object conversion |
| `generate-temporaries` | Hygienic macro helper |
| `filter`, `fold`, `fold-left`, `fold-right` | SRFI-1 list operations |
| `compose`, `identity`, `constantly`, `flip`, `curry` | Higher-order utilities |
| `range`, `zip`, `take`, `drop`, `nth` | Additional list operations |
