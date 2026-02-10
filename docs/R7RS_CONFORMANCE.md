# Grift R7RS-small Conformance Report

**Generated:** 2025-02-10  
**Repository:** `skyfskyf/grift`  
**Commit:** `f0cbfe0`  
**Version:** 1.4.0

---

## Executive Summary

| Category | Count | Percentage |
|----------|-------|------------|
| **Total R7RS-small standard procedures (Ch. 6)** | **288** | |
| ✅ Fully Implemented | **279** | **96.9%** |
| ⚠️ Partially Implemented | **4** | **1.4%** |
| ❌ Not Implemented | **5** | **1.7%** |

| Category | Count | Percentage |
|----------|-------|------------|
| **Total R7RS-small syntax forms (Ch. 4)** | **37** | |
| ✅ Fully Implemented | **37** | **100%** |
| ⚠️ Partially Implemented | **0** | **0%** |
| ❌ Not Implemented | **0** | **0%** |

| Category | Count | Percentage |
|----------|-------|------------|
| **Total R7RS-small standard libraries (App. A)** | **16** | |
| ✅ Fully Implemented | **10** | **62.5%** |
| ⚠️ Partially Implemented | **4** | **25.0%** |
| ❌ Not Implemented | **2** | **12.5%** |

---

## Chapter 4: Syntax Forms

### 4.1 Primitive Expression Types

| Form | Status | Notes | Location |
|------|--------|-------|----------|
| `quote` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `lambda` | ✅ | Special form, supports rest args | `grift_eval/src/evaluator/core.rs` |
| `if` | ✅ | Core special form (cannot be shadowed) | `grift_eval/src/evaluator/core.rs` |
| `set!` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `include` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `include-ci` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |

### 4.2 Derived Expression Types

| Form | Status | Notes | Location |
|------|--------|-------|----------|
| `cond` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `case` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `and` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `or` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `when` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `unless` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `let` | ✅ | Macro in prelude; supports named `let` | `grift_core/src/prelude.scm` |
| `let*` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `letrec` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `letrec*` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `let-values` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `let*-values` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `begin` | ✅ | Special form with TCO | `grift_eval/src/evaluator/core.rs` |
| `do` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `delay` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `delay-force` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `parameterize` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `guard` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `quasiquote` | ✅ | Special form with trampolining | `grift_eval/src/evaluator/core.rs` |
| `case-lambda` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |

### 4.3 Macros

| Form | Status | Notes | Location |
|------|--------|-------|----------|
| `let-syntax` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `letrec-syntax` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `syntax-rules` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `syntax-error` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |

### 4.4–4.5 Definitions

| Form | Status | Notes | Location |
|------|--------|-------|----------|
| `define` | ✅ | Special form; supports function shorthand | `grift_eval/src/evaluator/core.rs` |
| `define-values` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `define-syntax` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `define-record-type` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |

### 4.6 Libraries

| Form | Status | Notes | Location |
|------|--------|-------|----------|
| `define-library` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `import` | ✅ | Special form with auto-loading | `grift_eval/src/evaluator/core.rs` |
| `cond-expand` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |

---

## Chapter 6: Standard Procedures

### 6.1 Equivalence Predicates

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `eqv?` | ✅ | Built-in | `grift_core/src/value.rs` · `grift_eval/src/evaluator/builtins.rs` |
| `eq?` | ✅ | Built-in | `grift_core/src/value.rs` · `grift_eval/src/evaluator/builtins.rs` |
| `equal?` | ✅ | Built-in; recursive structural comparison | `grift_core/src/value.rs` · `grift_eval/src/evaluator/builtins.rs` |

### 6.2 Numbers

#### 6.2.1 Numerical Types

Grift uses fixed-size integers (`isize`) and IEEE 754 double-precision floating point (`f64`). Exact integers and inexact reals are supported. Rational and complex number *literals* are not supported by the parser, though complex number operations are available as built-in procedures.

#### 6.2.6 Numerical Operations

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `number?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `complex?` | ✅ | Prelude; alias for `number?` | `grift_core/src/prelude.scm` |
| `real?` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `rational?` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `integer?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `exact?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `inexact?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `exact-integer?` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `exact` | ✅ | Built-in | `grift_core/src/value.rs` |
| `inexact` | ✅ | Built-in | `grift_core/src/value.rs` |
| `=` | ✅ | Built-in | `grift_core/src/value.rs` |
| `<` | ✅ | Built-in | `grift_core/src/value.rs` |
| `>` | ✅ | Built-in | `grift_core/src/value.rs` |
| `<=` | ✅ | Built-in | `grift_core/src/value.rs` |
| `>=` | ✅ | Built-in | `grift_core/src/value.rs` |
| `zero?` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `positive?` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `negative?` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `odd?` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `even?` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `max` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `min` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `+` | ✅ | Built-in; variadic | `grift_core/src/value.rs` |
| `-` | ✅ | Built-in; variadic, supports negation | `grift_core/src/value.rs` |
| `*` | ✅ | Built-in; variadic | `grift_core/src/value.rs` |
| `/` | ✅ | Built-in; variadic | `grift_core/src/value.rs` |
| `abs` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `floor/` | ✅ | Built-in; returns two values | `grift_core/src/value.rs` |
| `floor-quotient` | ✅ | Built-in | `grift_core/src/value.rs` |
| `floor-remainder` | ✅ | Built-in | `grift_core/src/value.rs` |
| `truncate/` | ✅ | Built-in; returns two values | `grift_core/src/value.rs` |
| `truncate-quotient` | ✅ | Built-in | `grift_core/src/value.rs` |
| `truncate-remainder` | ✅ | Built-in | `grift_core/src/value.rs` |
| `quotient` | ✅ | Built-in (R5RS name) | `grift_core/src/value.rs` |
| `remainder` | ✅ | Built-in (R5RS name) | `grift_core/src/value.rs` |
| `modulo` | ✅ | Built-in (R5RS name) | `grift_core/src/value.rs` |
| `gcd` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `lcm` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `numerator` | ✅ | Built-in | `grift_core/src/value.rs` |
| `denominator` | ✅ | Built-in | `grift_core/src/value.rs` |
| `floor` | ✅ | Built-in | `grift_core/src/value.rs` |
| `ceiling` | ✅ | Built-in | `grift_core/src/value.rs` |
| `truncate` | ✅ | Built-in | `grift_core/src/value.rs` |
| `round` | ✅ | Built-in | `grift_core/src/value.rs` |
| `rationalize` | ✅ | Built-in | `grift_core/src/value.rs` |
| `square` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `exact-integer-sqrt` | ✅ | Built-in | `grift_core/src/value.rs` |
| `expt` | ✅ | Built-in | `grift_core/src/value.rs` |
| `sqrt` | ✅ | Built-in | `grift_core/src/value.rs` |
| `exp` | ✅ | Built-in | `grift_core/src/value.rs` |
| `log` | ✅ | Built-in | `grift_core/src/value.rs` |
| `sin` | ✅ | Built-in | `grift_core/src/value.rs` |
| `cos` | ✅ | Built-in | `grift_core/src/value.rs` |
| `tan` | ✅ | Built-in | `grift_core/src/value.rs` |
| `asin` | ✅ | Built-in | `grift_core/src/value.rs` |
| `acos` | ✅ | Built-in | `grift_core/src/value.rs` |
| `atan` | ✅ | Built-in; supports 1 and 2 arg forms | `grift_core/src/value.rs` |
| `make-rectangular` | ✅ | Built-in | `grift_core/src/value.rs` |
| `make-polar` | ✅ | Built-in | `grift_core/src/value.rs` |
| `real-part` | ✅ | Built-in | `grift_core/src/value.rs` |
| `imag-part` | ✅ | Built-in | `grift_core/src/value.rs` |
| `magnitude` | ✅ | Built-in | `grift_core/src/value.rs` |
| `angle` | ✅ | Built-in | `grift_core/src/value.rs` |
| `number->string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string->number` | ✅ | Built-in | `grift_core/src/value.rs` |
| `finite?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `infinite?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `nan?` | ✅ | Built-in | `grift_core/src/value.rs` |

### 6.3 Booleans

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `not` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `boolean?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `boolean=?` | ✅ | Prelude | `grift_core/src/prelude.scm` |

### 6.4 Pairs and Lists

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `pair?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `cons` | ✅ | Built-in | `grift_core/src/value.rs` |
| `car` | ✅ | Built-in | `grift_core/src/value.rs` |
| `cdr` | ✅ | Built-in | `grift_core/src/value.rs` |
| `set-car!` | ✅ | Built-in | `grift_core/src/value.rs` |
| `set-cdr!` | ✅ | Built-in | `grift_core/src/value.rs` |
| `caar` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `cadr` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `cdar` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `cddr` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `caaar` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `caadr` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `cadar` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `caddr` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `cdaar` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `cdadr` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `cddar` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `cdddr` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `cadddr` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `cddddr` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `null?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `list?` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `make-list` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `list` | ✅ | Built-in | `grift_core/src/value.rs` |
| `length` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `append` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `reverse` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `list-tail` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `list-ref` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `list-set!` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `list-copy` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `map` | ⚠️ | Single-list only; R7RS requires multi-list | `grift_core/src/prelude.scm` |
| `for-each` | ⚠️ | Single-list only; R7RS requires multi-list | `grift_core/src/prelude.scm` |
| `memq` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `memv` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `member` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `assq` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `assv` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `assoc` | ✅ | Prelude | `grift_core/src/prelude.scm` |

### 6.5 Symbols

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `symbol?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `symbol=?` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `symbol->string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string->symbol` | ✅ | Built-in | `grift_core/src/value.rs` |

### 6.6 Characters

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `char?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `char=?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `char<?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `char>?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `char<=?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `char>=?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `char-ci=?` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `char-ci<?` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `char-ci>?` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `char-ci<=?` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `char-ci>=?` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `char-alphabetic?` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `char-numeric?` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `char-whitespace?` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `char-upper-case?` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `char-lower-case?` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `digit-value` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `char->integer` | ✅ | Built-in | `grift_core/src/value.rs` |
| `integer->char` | ✅ | Built-in | `grift_core/src/value.rs` |
| `char-upcase` | ✅ | Built-in | `grift_core/src/value.rs` |
| `char-downcase` | ✅ | Built-in | `grift_core/src/value.rs` |
| `char-foldcase` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |

### 6.7 Strings

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `string?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `make-string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-length` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-ref` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-set!` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string=?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string<?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string>?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string<=?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string>=?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-ci=?` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `string-ci<?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-ci>?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-ci<=?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-ci>=?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-upcase` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `string-downcase` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `string-foldcase` | ✅ | (scheme char) library | `grift_core/src/lib/scheme/char.scm` |
| `substring` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-append` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string->list` | ✅ | Built-in | `grift_core/src/value.rs` |
| `list->string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-copy` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-copy!` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string-fill!` | ✅ | Built-in | `grift_core/src/value.rs` |

### 6.8 Vectors

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `vector?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `make-vector` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector-length` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector-ref` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector-set!` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector->list` | ✅ | Built-in | `grift_core/src/value.rs` |
| `list->vector` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector->string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string->vector` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector-copy` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector-copy!` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector-append` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector-fill!` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector-map` | ✅ | Built-in | `grift_core/src/value.rs` |
| `vector-for-each` | ✅ | Built-in | `grift_core/src/value.rs` |

### 6.9 Bytevectors

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `bytevector?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `make-bytevector` | ✅ | Built-in | `grift_core/src/value.rs` |
| `bytevector` | ❌ | Variadic constructor not implemented | — |
| `bytevector-length` | ✅ | Built-in | `grift_core/src/value.rs` |
| `bytevector-u8-ref` | ✅ | Built-in | `grift_core/src/value.rs` |
| `bytevector-u8-set!` | ✅ | Built-in | `grift_core/src/value.rs` |
| `bytevector-copy` | ✅ | Built-in | `grift_core/src/value.rs` |
| `bytevector-copy!` | ❌ | Destructive copy not implemented | — |
| `bytevector-append` | ✅ | Built-in | `grift_core/src/value.rs` |
| `utf8->string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `string->utf8` | ✅ | Built-in | `grift_core/src/value.rs` |

### 6.10 Control Features

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `procedure?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `apply` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `map` | ⚠️ | Single-list only | `grift_core/src/prelude.scm` |
| `string-map` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `vector-map` | ✅ | Built-in | `grift_core/src/value.rs` |
| `for-each` | ⚠️ | Single-list only | `grift_core/src/prelude.scm` |
| `string-for-each` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `vector-for-each` | ✅ | Built-in | `grift_core/src/value.rs` |
| `call-with-current-continuation` | ✅ | Special form; escape continuations | `grift_eval/src/evaluator/core.rs` |
| `call/cc` | ✅ | Alias for above | `grift_eval/src/evaluator/core.rs` |
| `values` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `call-with-values` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `dynamic-wind` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `make-promise` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `promise?` | ✅ | Prelude | `grift_core/src/prelude.scm` |
| `force` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |
| `make-parameter` | ✅ | Prelude | `grift_core/src/prelude.scm` |

### 6.11 Exceptions

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `with-exception-handler` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `raise` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `raise-continuable` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `error` | ✅ | Built-in | `grift_core/src/value.rs` |
| `error-object?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `error-object-message` | ✅ | Built-in | `grift_core/src/value.rs` |
| `error-object-irritants` | ✅ | Built-in | `grift_core/src/value.rs` |
| `error-object-type` | ✅ | Built-in (Grift extension) | `grift_core/src/value.rs` |
| `read-error?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `file-error?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `guard` | ✅ | Macro in prelude | `grift_core/src/prelude.scm` |

### 6.12 Environments and Evaluation

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `environment` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `eval` | ✅ | Special form | `grift_eval/src/evaluator/core.rs` |
| `scheme-report-environment` | ❌ | Not implemented | — |
| `null-environment` | ❌ | Not implemented | — |
| `interaction-environment` | ✅ | Built-in | `grift_core/src/value.rs` |

### 6.13 Input and Output

#### Ports

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `call-with-port` | ❌ | Not implemented | — |
| `call-with-input-file` | ✅ | Built-in | `grift_core/src/value.rs` |
| `call-with-output-file` | ✅ | Built-in | `grift_core/src/value.rs` |
| `input-port?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `output-port?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `textual-port?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `binary-port?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `port?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `input-port-open?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `output-port-open?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `current-input-port` | ✅ | Built-in | `grift_core/src/value.rs` |
| `current-output-port` | ✅ | Built-in | `grift_core/src/value.rs` |
| `current-error-port` | ✅ | Built-in | `grift_core/src/value.rs` |
| `close-port` | ✅ | Built-in | `grift_core/src/value.rs` |
| `close-input-port` | ✅ | Built-in | `grift_core/src/value.rs` |
| `close-output-port` | ✅ | Built-in | `grift_core/src/value.rs` |

#### File Ports

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `open-input-file` | ✅ | Built-in | `grift_core/src/value.rs` |
| `open-binary-input-file` | ✅ | Built-in | `grift_core/src/value.rs` |
| `open-output-file` | ✅ | Built-in | `grift_core/src/value.rs` |
| `open-binary-output-file` | ✅ | Built-in | `grift_core/src/value.rs` |
| `with-input-from-file` | ✅ | Built-in | `grift_core/src/value.rs` |
| `with-output-to-file` | ✅ | Built-in | `grift_core/src/value.rs` |

#### String Ports

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `open-input-string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `open-output-string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `get-output-string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `open-input-bytevector` | ✅ | Built-in | `grift_core/src/value.rs` |
| `open-output-bytevector` | ✅ | Built-in | `grift_core/src/value.rs` |
| `get-output-bytevector` | ✅ | Built-in | `grift_core/src/value.rs` |

#### Reading

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `read` | ✅ | Built-in | `grift_core/src/value.rs` |
| `read-char` | ✅ | Built-in | `grift_core/src/value.rs` |
| `peek-char` | ✅ | Built-in | `grift_core/src/value.rs` |
| `read-line` | ✅ | Built-in | `grift_core/src/value.rs` |
| `eof-object?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `eof-object` | ✅ | Built-in | `grift_core/src/value.rs` |
| `char-ready?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `read-string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `read-u8` | ✅ | Built-in | `grift_core/src/value.rs` |
| `peek-u8` | ✅ | Built-in | `grift_core/src/value.rs` |
| `u8-ready?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `read-bytevector` | ✅ | Built-in | `grift_core/src/value.rs` |
| `read-bytevector!` | ✅ | Built-in | `grift_core/src/value.rs` |

#### Writing

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `write` | ✅ | Built-in | `grift_core/src/value.rs` |
| `write-shared` | ✅ | Built-in | `grift_core/src/value.rs` |
| `write-simple` | ✅ | Built-in | `grift_core/src/value.rs` |
| `display` | ✅ | Built-in | `grift_core/src/value.rs` |
| `newline` | ✅ | Built-in | `grift_core/src/value.rs` |
| `write-char` | ✅ | Built-in | `grift_core/src/value.rs` |
| `write-string` | ✅ | Built-in | `grift_core/src/value.rs` |
| `write-u8` | ✅ | Built-in | `grift_core/src/value.rs` |
| `write-bytevector` | ✅ | Built-in | `grift_core/src/value.rs` |
| `flush-output-port` | ✅ | Built-in | `grift_core/src/value.rs` |

### 6.14 System Interface

| Procedure | Status | Notes | Location |
|-----------|--------|-------|----------|
| `load` | ✅ | Built-in | `grift_core/src/value.rs` |
| `file-exists?` | ✅ | Built-in | `grift_core/src/value.rs` |
| `delete-file` | ✅ | Built-in | `grift_core/src/value.rs` |
| `command-line` | ✅ | Built-in | `grift_core/src/value.rs` |
| `exit` | ✅ | Built-in | `grift_core/src/value.rs` |
| `emergency-exit` | ✅ | Built-in | `grift_core/src/value.rs` |
| `get-environment-variable` | ✅ | Built-in | `grift_core/src/value.rs` |
| `get-environment-variables` | ✅ | Built-in | `grift_core/src/value.rs` |
| `current-second` | ✅ | Built-in | `grift_core/src/value.rs` |
| `current-jiffy` | ✅ | Built-in | `grift_core/src/value.rs` |
| `jiffies-per-second` | ✅ | Built-in | `grift_core/src/value.rs` |
| `features` | ✅ | Prelude | `grift_core/src/prelude.scm` |

---

## Appendix A: Standard Libraries

### `(scheme base)`

**Status:** ⚠️ Partially complete  
**Completeness:** ~96% — Most R7RS base procedures are exported.

**Missing from exports (available as builtins or special forms but not re-exported):**
- `apply`, `call-with-current-continuation`, `call/cc`
- `values`, `call-with-values`, `dynamic-wind`
- `with-exception-handler`, `raise`, `raise-continuable`
- `define-record-type`, `include`, `include-ci`
- `write-string`, `flush-output-port`
- `bytevector?`, `make-bytevector`, `bytevector-length`, `bytevector-u8-ref`, `bytevector-u8-set!`, `bytevector-copy`, `bytevector-append`
- `bytevector` (not implemented at all)
- `bytevector-copy!` (not implemented at all)
- `call-with-port` (not implemented)
- `open-input-file`, `open-output-file`
- `read`, `write`, `write-shared`, `write-simple`
- `read-u8`, `peek-u8`, `u8-ready?`, `write-u8`, `write-bytevector`
- `open-input-bytevector`, `open-output-bytevector`, `get-output-bytevector`
- `features`

> **Note:** Many of these are available globally as built-in procedures or special forms. They function correctly at the top level but are not re-exported through the `(scheme base)` library's `export` declaration.

### `(scheme case-lambda)`

**Status:** ✅ Complete  
**Exports:** `case-lambda`

### `(scheme char)`

**Status:** ✅ Complete  
**Exports:** `char-alphabetic?`, `char-numeric?`, `char-whitespace?`, `char-upper-case?`, `char-lower-case?`, `char-ci=?`, `char-ci<?`, `char-ci>?`, `char-ci<=?`, `char-ci>=?`, `char-upcase`, `char-downcase`, `char-foldcase`, `digit-value`, `string-ci=?`, `string-upcase`, `string-downcase`, `string-foldcase`

### `(scheme complex)`

**Status:** ❌ Not implemented  
Library file does not exist. Complex number operations are available as builtins but not packaged as a library.

**Required exports:** `make-rectangular`, `make-polar`, `real-part`, `imag-part`, `magnitude`, `angle`

### `(scheme cxr)`

**Status:** ⚠️ Partially complete  
**Exports:** 14 of 24 required cxr procedures.

**Implemented:** `caar`, `cadr`, `cdar`, `cddr`, `caaar`, `caadr`, `cadar`, `caddr`, `cdaar`, `cdadr`, `cddar`, `cdddr`, `cadddr`, `cddddr`

**Missing:** `caaaar`, `caaadr`, `caadar`, `caaddr`, `cadaar`, `cadadr`, `caddar`, `cdaaar`, `cdaadr`, `cdadar`

### `(scheme eval)`

**Status:** ✅ Complete  
**Exports:** `eval`, `environment`

### `(scheme file)`

**Status:** ⚠️ Partially complete  
**Exports:** `file-exists?`, `delete-file`

**Missing:** `call-with-input-file`, `call-with-output-file`, `with-input-from-file`, `with-output-to-file`, `open-input-file`, `open-output-file`, `open-binary-input-file`, `open-binary-output-file`

> **Note:** All missing procedures are available as builtins but not re-exported from this library.

### `(scheme inexact)`

**Status:** ✅ Complete  
**Exports:** `finite?`, `infinite?`, `nan?`, `sqrt`, `exp`, `log`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `exact->inexact`, `inexact->exact`

### `(scheme lazy)`

**Status:** ✅ Complete  
**Exports:** `delay`, `force`, `delay-force`, `make-promise`, `promise?`

### `(scheme load)`

**Status:** ✅ Complete  
**Exports:** `load`

### `(scheme process-context)`

**Status:** ✅ Complete  
**Exports:** `command-line`, `exit`, `emergency-exit`, `get-environment-variable`, `get-environment-variables`

### `(scheme read)`

**Status:** ✅ Complete  
**Exports:** `read`

### `(scheme repl)`

**Status:** ✅ Complete  
**Exports:** `interaction-environment`

### `(scheme time)`

**Status:** ❌ Empty placeholder  
Library file exists but exports nothing. Time procedures (`current-second`, `current-jiffy`, `jiffies-per-second`) are available as builtins.

### `(scheme write)`

**Status:** ✅ Complete  
**Exports:** `write`, `display`, `write-shared`, `write-simple`

### `(scheme r5rs)`

**Status:** ✅ Complete  
R5RS compatibility library. Exports a comprehensive set of R5RS procedures including `exact->inexact`, `inexact->exact`, interaction-environment, file I/O, and cxr procedures.

---

## Number Syntax

| Feature | Status | Examples | Notes |
|---------|--------|----------|-------|
| Integer literals | ✅ | `42`, `-7`, `0` | `isize` representation |
| Floating-point literals | ✅ | `3.14`, `-0.5` | IEEE 754 `f64` |
| Binary prefix `#b` | ✅ | `#b1010` → `10` | |
| Octal prefix `#o` | ✅ | `#o17` → `15` | |
| Decimal prefix `#d` | ✅ | `#d42` → `42` | |
| Hexadecimal prefix `#x` | ✅ | `#xff` → `255` | |
| Exactness `#e` | ✅ | `#e1.5` → `1` | Converts to integer |
| Exactness `#i` | ✅ | `#i3` → `3.0` | Converts to float |
| Combined prefixes | ✅ | `#e#x1f`, `#b#i101` | Both orderings supported |
| Scientific notation | ✅ | `1e10`, `3.14e-5`, `2E+3` | |
| `+inf.0` | ✅ | `+inf.0` | Positive infinity |
| `-inf.0` | ✅ | `-inf.0` | Negative infinity |
| `+nan.0` | ✅ | `+nan.0` | Not-a-number |
| `-nan.0` | ✅ | `-nan.0` | Not-a-number |
| Rational literals | ❌ | `3/4`, `-1/2` | Not supported by parser |
| Complex literals | ❌ | `1+2i`, `3@4` | Not supported by parser |

---

## Tail Call Optimization

Grift implements full proper tail calls via a **trampolining architecture**:

- All evaluation uses continuation-passing style — no Rust stack recursion
- Arena-based continuation frames enable O(1) capture for `call/cc`
- Proper tail position recognized in: `if` branches, `begin` last expression, `lambda` body, `cond`/`case` branches, named `let`, `do`
- Verified through tests with 150–200 levels of deep recursion without stack overflow

---

## Testing Coverage

### Dedicated R7RS Test Files

| Test File | Coverage Area |
|-----------|---------------|
| `r7rs_numeric_tests.rs` | §6.2 number predicates, division, transcendental, complex |
| `r7rs_new_procedures_tests.rs` | Time procedures, error predicates, vector-string conversion, `include` |
| `bytevector_tests.rs` | §6.9 bytevector operations |
| `io_tests.rs` | §6.13 file I/O, string ports |
| `system_tests.rs` | §6.14 file-exists?, delete-file |
| `library_tests.rs` | define-library, import, module system |
| `continuation_delay_tests.rs` | call/cc, delay/force |
| `environment_tests.rs` | eval, environment, scoping |

### R5RS Compliance Suites

| Test File | Source |
|-----------|--------|
| `r5rs_chibi_tests.rs` | Adapted from chibi-scheme |
| `r5rs_pitfalls_tests.rs` | Adapted from chicken-scheme |
| `peroxide_r5rs_tests.rs` | Peroxide test suite |
| `peroxide_pitfalls_tests.rs` | Peroxide pitfalls |

### Syntax Test Files

| Test File | Coverage |
|-----------|----------|
| `syntax_proper_tests.rs` | `syntax-case`, `syntax-rules`, hygiene |
| `syntax_extended_tests.rs` | Extended syntax forms |
| `syntax_edge_case_tests.rs` | Edge cases in macro expansion |
| `syntactic_extension_html_tests.rs` | HTML spec examples |

---

## Grift Extensions (Non-R7RS)

The following features are Grift-specific extensions beyond R7RS-small:

| Feature | Description | Location |
|---------|-------------|----------|
| `gc`, `gc-enable`, `gc-disable`, `gc-enabled?` | Manual GC control | Built-in |
| `arena-stats` | Arena memory statistics | Built-in |
| `error-object-type` | Error type field access | Built-in |
| `syntax-case` | R6RS-style procedural macros | Special form |
| `syntax->datum`, `datum->syntax` | Syntax object manipulation | Built-in |
| `identifier?`, `bound-identifier=?`, `free-identifier=?` | Identifier inspection | Built-in |
| `generate-temporaries` | Hygienic macro temporaries | Built-in |
| `filter`, `fold`, `fold-left`, `fold-right` | List utilities (SRFI-1 subset) | Prelude |
| `sign` | Numeric sign function | Prelude |
| `define-syntax-rule` | Single-clause macro shorthand | Prelude |

---

## Implementation Priorities

### High Priority (Core R7RS Compliance)

1. **`(scheme base)` library exports** — Many implemented procedures are not re-exported from `(scheme base)`. Adding them to the export list would significantly improve library-based conformance. Affected: `apply`, `call/cc`, `values`, `call-with-values`, `dynamic-wind`, `with-exception-handler`, `raise`, `raise-continuable`, `define-record-type`, `include`, `include-ci`, bytevector operations, file port operations, binary I/O, `write-string`, `flush-output-port`, `features`.
2. **Multi-list `map` and `for-each`** — R7RS requires `(map f list1 list2 ...)` to operate on multiple lists simultaneously. Current implementation only accepts a single list.
3. **`bytevector` variadic constructor** — `(bytevector 1 2 3)` is not implemented; only `make-bytevector` exists.
4. **`bytevector-copy!`** — Destructive bytevector copy is missing.
5. **`call-with-port`** — Generic port procedure not implemented.

### Medium Priority (Library Completeness)

6. **`(scheme time)` library** — Empty placeholder. Builtins exist (`current-second`, `current-jiffy`, `jiffies-per-second`) but need to be re-exported.
7. **`(scheme complex)` library** — Library file missing. All procedures are available as builtins but need packaging.
8. **`(scheme cxr)` library** — Only 14 of 24 required cxr procedures are exported. Missing the 4-deep compositions (`caaaar`, `caaadr`, etc.).
9. **`(scheme file)` library exports** — File I/O builtins exist but are not re-exported.
10. **`scheme-report-environment` and `null-environment`** — R5RS compatibility procedures not implemented.

### Low Priority (Edge Cases)

11. **Rational number literal syntax** — Parser does not support `3/4` notation.
12. **Complex number literal syntax** — Parser does not support `1+2i` or polar `3@4` notation.
13. **`(scheme write)` library** — Missing `write-char`, `write-string`, `newline` exports (available as builtins).

---

## Appendix: Architecture Notes

### Value Representation

- **Arena-allocated**: All values reside in a custom arena allocator (`grift_arena`)
- **Copy semantics**: All value types implement `Copy` for arena compatibility
- **No heap allocation**: Core crates (`grift_arena`, `grift_parser`, `grift_eval`) are `#![no_std]` with no `alloc` dependency
- **Number types**: `isize` for exact integers, `f64` for inexact reals

### Evaluation Model

- **Trampolining evaluator**: No recursive Rust calls for Scheme evaluation
- **Continuation-passing style**: Proper tail calls guaranteed
- **Arena-based environments**: O(1) continuation capture for `call/cc`
- **Prelude loading**: Standard procedures defined in Scheme (`prelude.scm`) and loaded at startup
- **Library auto-loading**: `(import (scheme ...))` triggers on-demand compilation of embedded `.scm` library sources

### Feature Flags

Libraries are gated behind Cargo feature flags (default: `all-libraries`):
- `scheme-base`, `scheme-char`, `scheme-cxr`, `scheme-eval`, `scheme-file`
- `scheme-inexact`, `scheme-lazy`, `scheme-load`, `scheme-process-context`
- `scheme-read`, `scheme-repl`, `scheme-r5rs`, `scheme-time`, `scheme-write`
- `scheme-case-lambda`
