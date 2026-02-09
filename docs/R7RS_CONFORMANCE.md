# Grift R7RS-small Conformance Report

**Generated:** 2026-02-09
**Repository:** `skyfskyf/grift`
**Commit:** `1ac29528`
**Version:** 1.4.0

---

## Executive Summary

| Category | Count | Percentage |
|----------|-------|------------|
| **Fully Implemented** ✅ | 153 | 56.9% |
| **Partially Implemented** ⚠️ | 18 | 6.7% |
| **Not Implemented** ❌ | 98 | 36.4% |
| **Total R7RS-small identifiers tracked** | 269 | 100% |

Grift provides solid coverage of core Scheme features: arithmetic, lists, strings, vectors, characters, closures, continuations, hygienic macros (including `syntax-case`), tail calls via trampoline, and a module system. Major gaps include bytevectors, file I/O ports, binary I/O, time functions, complex/rational standard procedures, and several numeric operations.

---

## Chapter 4: Syntax Forms

| Form | Status | Notes |
|------|--------|-------|
| `quote` | ✅ | Native special form |
| `lambda` | ✅ | Native special form |
| `if` | ✅ | Native special form |
| `set!` | ✅ | Native special form |
| `include` | ❌ | Not implemented |
| `include-ci` | ❌ | Not implemented |
| `cond` | ✅ | Macro in prelude.scm |
| `case` | ✅ | Macro in prelude.scm |
| `and` | ✅ | Macro in prelude.scm |
| `or` | ✅ | Macro in prelude.scm |
| `when` | ✅ | Macro in prelude.scm |
| `unless` | ✅ | Macro in prelude.scm |
| `let` | ✅ | Macro in prelude.scm (named `let` supported) |
| `let*` | ✅ | Macro in prelude.scm |
| `letrec` | ✅ | Macro in prelude.scm |
| `letrec*` | ✅ | Macro in prelude.scm |
| `let-values` | ✅ | Macro in prelude.scm |
| `let*-values` | ✅ | Macro in prelude.scm |
| `begin` | ✅ | Native special form |
| `do` | ✅ | Macro in prelude.scm |
| `delay` | ✅ | Macro in prelude.scm |
| `delay-force` | ✅ | Macro in prelude.scm |
| `quasiquote` | ✅ | Native special form |
| `unquote` | ✅ | Handled in quasiquote expansion |
| `unquote-splicing` | ✅ | Handled in quasiquote expansion |
| `define` | ✅ | Native special form |
| `define-values` | ✅ | Macro in prelude.scm |
| `define-syntax` | ✅ | Native special form |
| `define-record-type` | ✅ | Native special form (max 32 fields, R7RS §5.5) |
| `let-syntax` | ✅ | Native special form |
| `letrec-syntax` | ✅ | Native special form |
| `syntax-rules` | ✅ | Macro in prelude.scm |
| `syntax-error` | ❌ | Not implemented |
| `parameterize` | ✅ | Macro in prelude.scm |
| `guard` | ✅ | Macro in prelude.scm |
| `case-lambda` | ✅ | Macro in prelude.scm |
| `cond-expand` | ⚠️ | Recognizes `r7rs`, `grift`, `exact-closed`; no `library` check |

---

## Chapter 6: Standard Procedures

### 6.1 Equivalence Predicates

| Procedure | Status | Location |
|-----------|--------|----------|
| `eqv?` | ✅ | `grift_core/src/value.rs` (Builtin) |
| `eq?` | ✅ | `grift_core/src/value.rs` (Builtin) |
| `equal?` | ✅ | `grift_core/src/value.rs` (Builtin) |

### 6.2 Numbers

#### Type Predicates

| Procedure | Status | Notes |
|-----------|--------|-------|
| `number?` | ✅ | Builtin; returns `#t` for `Number` and `Float` |
| `complex?` | ⚠️ | Defined in prelude; aliases `number?` (no complex type) |
| `real?` | ⚠️ | Defined in prelude; aliases `number?` (no complex type) |
| `rational?` | ⚠️ | Defined in prelude; aliases `number?` (no rational type) |
| `integer?` | ✅ | Builtin |
| `exact?` | ✅ | Builtin; `#t` for `Number` |
| `inexact?` | ✅ | Builtin; `#t` for `Float` |
| `exact-integer?` | ✅ | Defined in prelude |

#### Exactness Conversion

| Procedure | Status | Notes |
|-----------|--------|-------|
| `exact` | ✅ | Builtin; `Float` → `Number` (truncates) |
| `inexact` | ✅ | Builtin; `Number` → `Float` |
| `exact->inexact` | ✅ | Builtin alias |
| `inexact->exact` | ✅ | Builtin alias |

#### Arithmetic

| Procedure | Status | Notes |
|-----------|--------|-------|
| `+` | ✅ | Builtin; variadic |
| `-` | ✅ | Builtin; variadic |
| `*` | ✅ | Builtin; variadic |
| `/` | ✅ | Builtin; variadic |
| `abs` | ✅ | Defined in prelude |
| `floor/` | ❌ | Not implemented |
| `floor-quotient` | ❌ | Not implemented |
| `floor-remainder` | ❌ | Not implemented |
| `truncate/` | ❌ | Not implemented |
| `truncate-quotient` | ❌ | Not implemented |
| `truncate-remainder` | ❌ | Not implemented |
| `quotient` | ✅ | Builtin |
| `remainder` | ✅ | Builtin |
| `modulo` | ✅ | Builtin |

#### Comparison

| Procedure | Status | Notes |
|-----------|--------|-------|
| `=` | ✅ | Builtin; variadic |
| `<` | ✅ | Builtin; variadic |
| `>` | ✅ | Builtin; variadic |
| `<=` | ✅ | Builtin; variadic |
| `>=` | ✅ | Builtin; variadic |
| `zero?` | ✅ | Prelude |
| `positive?` | ✅ | Prelude |
| `negative?` | ✅ | Prelude |
| `odd?` | ✅ | Prelude |
| `even?` | ✅ | Prelude |
| `max` | ✅ | Prelude |
| `min` | ✅ | Prelude |

#### Division and GCD

| Procedure | Status | Notes |
|-----------|--------|-------|
| `gcd` | ✅ | Prelude |
| `lcm` | ✅ | Prelude |
| `numerator` | ❌ | Not implemented (no rational type) |
| `denominator` | ❌ | Not implemented (no rational type) |

#### Rounding

| Procedure | Status | Notes |
|-----------|--------|-------|
| `floor` | ✅ | Builtin |
| `ceiling` | ✅ | Builtin |
| `truncate` | ✅ | Builtin |
| `round` | ✅ | Builtin |
| `rationalize` | ❌ | Not implemented |

#### Exponentiation

| Procedure | Status | Notes |
|-----------|--------|-------|
| `expt` | ✅ | Builtin |
| `sqrt` | ✅ | Builtin |
| `exact-integer-sqrt` | ❌ | Not implemented |
| `square` | ✅ | Prelude |

#### Transcendental Functions

| Procedure | Status | Notes |
|-----------|--------|-------|
| `exp` | ❌ | Not implemented |
| `log` | ❌ | Not implemented |
| `sin` | ❌ | Not implemented |
| `cos` | ❌ | Not implemented |
| `tan` | ❌ | Not implemented |
| `asin` | ❌ | Not implemented |
| `acos` | ❌ | Not implemented |
| `atan` | ❌ | Not implemented |

#### Complex Number Procedures

| Procedure | Status | Notes |
|-----------|--------|-------|
| `make-rectangular` | ❌ | No native complex type |
| `make-polar` | ❌ | No native complex type |
| `real-part` | ❌ | No native complex type |
| `imag-part` | ❌ | No native complex type |
| `magnitude` | ❌ | No native complex type |
| `angle` | ❌ | No native complex type |

> **Note:** Grift provides user-level `cpx` and `rat-cpx` macros for complex/rational arithmetic as library-level constructs, but these are not R7RS-compliant standard procedures.

#### Conversion

| Procedure | Status | Notes |
|-----------|--------|-------|
| `number->string` | ⚠️ | Builtin; integers and floats only, no radix parameter |
| `string->number` | ⚠️ | Builtin; no optional radix parameter |

### 6.3 Booleans

| Procedure | Status | Notes |
|-----------|--------|-------|
| `not` | ✅ | Prelude |
| `boolean?` | ✅ | Builtin |
| `boolean=?` | ✅ | Prelude |

### 6.4 Pairs and Lists

| Procedure | Status | Notes |
|-----------|--------|-------|
| `pair?` | ✅ | Builtin |
| `cons` | ✅ | Builtin |
| `car` | ✅ | Builtin |
| `cdr` | ✅ | Builtin |
| `set-car!` | ✅ | Builtin |
| `set-cdr!` | ✅ | Builtin |
| `caar` | ✅ | Prelude |
| `cadr` | ✅ | Prelude |
| `cdar` | ✅ | Prelude |
| `cddr` | ✅ | Prelude |
| `null?` | ✅ | Builtin |
| `list?` | ✅ | Prelude |
| `make-list` | ✅ | Prelude |
| `list` | ✅ | Builtin |
| `length` | ✅ | Prelude |
| `append` | ✅ | Macro in prelude (variadic) |
| `reverse` | ✅ | Prelude |
| `list-tail` | ✅ | Prelude |
| `list-ref` | ✅ | Prelude |
| `list-set!` | ✅ | Prelude |
| `list-copy` | ✅ | Prelude |
| `map` | ✅ | Prelude |
| `for-each` | ✅ | Prelude |
| `memq` | ✅ | Prelude |
| `memv` | ✅ | Prelude |
| `member` | ✅ | Prelude |
| `assq` | ✅ | Prelude |
| `assv` | ✅ | Prelude |
| `assoc` | ✅ | Prelude |

### 6.5 Symbols

| Procedure | Status | Notes |
|-----------|--------|-------|
| `symbol?` | ✅ | Builtin |
| `symbol=?` | ✅ | Prelude |
| `symbol->string` | ✅ | Builtin |
| `string->symbol` | ✅ | Builtin |

### 6.6 Characters

| Procedure | Status | Notes |
|-----------|--------|-------|
| `char?` | ✅ | Builtin |
| `char=?` | ✅ | Builtin |
| `char<?` | ✅ | Builtin |
| `char>?` | ✅ | Builtin |
| `char<=?` | ✅ | Builtin |
| `char>=?` | ✅ | Builtin |
| `char-ci=?` | ✅ | Prelude (via `char-foldcase`) |
| `char-ci<?` | ✅ | Prelude |
| `char-ci>?` | ✅ | Prelude |
| `char-ci<=?` | ✅ | Prelude |
| `char-ci>=?` | ✅ | Prelude |
| `char-alphabetic?` | ✅ | Prelude |
| `char-numeric?` | ✅ | Prelude |
| `char-whitespace?` | ✅ | Prelude |
| `char-upper-case?` | ✅ | Prelude |
| `char-lower-case?` | ✅ | Prelude |
| `digit-value` | ✅ | Prelude |
| `char->integer` | ✅ | Builtin |
| `integer->char` | ✅ | Builtin |
| `char-upcase` | ✅ | Builtin |
| `char-downcase` | ✅ | Builtin |
| `char-foldcase` | ✅ | Prelude |

### 6.7 Strings

| Procedure | Status | Notes |
|-----------|--------|-------|
| `string?` | ✅ | Builtin |
| `make-string` | ✅ | Builtin |
| `string` | ✅ | Builtin |
| `string-length` | ✅ | Builtin |
| `string-ref` | ✅ | Builtin |
| `string-set!` | ✅ | Builtin |
| `string=?` | ✅ | Builtin |
| `string<?` | ✅ | Builtin |
| `string>?` | ✅ | Builtin |
| `string<=?` | ✅ | Builtin |
| `string>=?` | ✅ | Builtin |
| `string-ci=?` | ✅ | Prelude |
| `string-ci<?` | ✅ | Builtin |
| `string-ci>?` | ✅ | Builtin |
| `string-ci<=?` | ✅ | Builtin |
| `string-ci>=?` | ✅ | Builtin |
| `string-upcase` | ✅ | Prelude |
| `string-downcase` | ✅ | Prelude |
| `string-foldcase` | ✅ | Prelude |
| `substring` | ✅ | Builtin |
| `string-append` | ✅ | Builtin |
| `string->list` | ✅ | Builtin |
| `list->string` | ✅ | Builtin |
| `string-copy` | ✅ | Builtin |
| `string-copy!` | ✅ | Builtin |
| `string-fill!` | ✅ | Builtin |

### 6.8 Vectors

| Procedure | Status | Notes |
|-----------|--------|-------|
| `vector?` | ✅ | Builtin |
| `make-vector` | ✅ | Builtin |
| `vector` | ✅ | Builtin |
| `vector-length` | ✅ | Builtin |
| `vector-ref` | ✅ | Builtin |
| `vector-set!` | ✅ | Builtin |
| `vector->list` | ✅ | Builtin |
| `list->vector` | ✅ | Builtin |
| `vector->string` | ❌ | Not implemented |
| `string->vector` | ❌ | Not implemented |
| `vector-copy` | ✅ | Builtin |
| `vector-copy!` | ✅ | Builtin |
| `vector-append` | ✅ | Builtin |
| `vector-fill!` | ✅ | Builtin |
| `vector-map` | ✅ | Builtin |
| `vector-for-each` | ✅ | Builtin |

### 6.9 Bytevectors

| Procedure | Status | Notes |
|-----------|--------|-------|
| `bytevector?` | ❌ | Value type exists but no builtin predicate exposed |
| `make-bytevector` | ❌ | Not implemented |
| `bytevector` | ❌ | Not implemented |
| `bytevector-length` | ❌ | Not implemented |
| `bytevector-u8-ref` | ❌ | Not implemented |
| `bytevector-u8-set!` | ❌ | Not implemented |
| `bytevector-copy` | ❌ | Not implemented |
| `bytevector-copy!` | ❌ | Not implemented |
| `bytevector-append` | ❌ | Not implemented |
| `utf8->string` | ❌ | Not implemented |
| `string->utf8` | ❌ | Not implemented |

> **Note:** The `Value::Bytevector` variant exists in the core types but no standard operations are exposed to Scheme.

### 6.10 Control Features

| Procedure | Status | Notes |
|-----------|--------|-------|
| `procedure?` | ✅ | Builtin |
| `apply` | ✅ | Native special form |
| `map` | ✅ | Prelude |
| `string-map` | ✅ | Prelude |
| `vector-map` | ✅ | Builtin |
| `for-each` | ✅ | Prelude |
| `string-for-each` | ✅ | Prelude |
| `vector-for-each` | ✅ | Builtin |
| `call-with-current-continuation` | ✅ | Native special form |
| `call/cc` | ✅ | Native special form (alias) |
| `values` | ✅ | Native special form |
| `call-with-values` | ✅ | Native special form |
| `dynamic-wind` | ✅ | Native special form |
| `make-parameter` | ✅ | Prelude |
| `promise?` | ✅ | Prelude |
| `make-promise` | ✅ | Prelude |
| `force` | ✅ | Macro in prelude |
| `delay` | ✅ | Macro in prelude |
| `delay-force` | ✅ | Macro in prelude |

### 6.11 Exceptions

| Procedure | Status | Notes |
|-----------|--------|-------|
| `with-exception-handler` | ✅ | Native special form |
| `raise` | ✅ | Native special form |
| `raise-continuable` | ✅ | Native special form |
| `error` | ✅ | Builtin |
| `error-object?` | ✅ | Builtin |
| `error-object-message` | ✅ | Builtin |
| `error-object-irritants` | ✅ | Builtin |
| `error-object-type` | ✅ | Builtin (non-standard extension) |
| `read-error?` | ❌ | Not implemented |
| `file-error?` | ❌ | Not implemented |
| `guard` | ✅ | Macro in prelude |

### 6.12 Environments and Evaluation

| Procedure | Status | Notes |
|-----------|--------|-------|
| `environment` | ✅ | Native special form |
| `eval` | ✅ | Native special form |
| `scheme-report-environment` | ❌ | Not implemented |
| `null-environment` | ❌ | Not implemented |
| `interaction-environment` | ✅ | Builtin |

### 6.13 Input and Output

#### Ports

| Procedure | Status | Notes |
|-----------|--------|-------|
| `input-port?` | ✅ | Builtin |
| `output-port?` | ✅ | Builtin |
| `textual-port?` | ✅ | Builtin |
| `binary-port?` | ✅ | Builtin |
| `port?` | ✅ | Builtin |
| `input-port-open?` | ✅ | Builtin |
| `output-port-open?` | ✅ | Builtin |
| `current-input-port` | ✅ | Builtin |
| `current-output-port` | ✅ | Builtin |
| `current-error-port` | ✅ | Builtin |
| `close-port` | ✅ | Builtin |
| `close-input-port` | ✅ | Builtin |
| `close-output-port` | ✅ | Builtin |
| `call-with-port` | ❌ | Not implemented |
| `call-with-input-file` | ❌ | Not implemented |
| `call-with-output-file` | ❌ | Not implemented |

#### File Ports

| Procedure | Status | Notes |
|-----------|--------|-------|
| `open-input-file` | ❌ | Not implemented |
| `open-binary-input-file` | ❌ | Not implemented |
| `open-output-file` | ❌ | Not implemented |
| `open-binary-output-file` | ❌ | Not implemented |
| `with-input-from-file` | ❌ | Not implemented |
| `with-output-to-file` | ❌ | Not implemented |

#### String/Bytevector Ports

| Procedure | Status | Notes |
|-----------|--------|-------|
| `open-input-string` | ✅ | Builtin |
| `open-output-string` | ✅ | Builtin |
| `get-output-string` | ✅ | Builtin |
| `open-input-bytevector` | ❌ | Not implemented |
| `open-output-bytevector` | ❌ | Not implemented |
| `get-output-bytevector` | ❌ | Not implemented |

#### Reading

| Procedure | Status | Notes |
|-----------|--------|-------|
| `read` | ✅ | Builtin |
| `read-char` | ✅ | Builtin |
| `peek-char` | ✅ | Builtin |
| `read-line` | ✅ | Builtin |
| `eof-object?` | ✅ | Builtin |
| `eof-object` | ✅ | Builtin |
| `char-ready?` | ✅ | Builtin |
| `read-string` | ✅ | Builtin |
| `read-u8` | ❌ | Not implemented |
| `peek-u8` | ❌ | Not implemented |
| `u8-ready?` | ❌ | Not implemented |
| `read-bytevector` | ❌ | Not implemented |
| `read-bytevector!` | ❌ | Not implemented |

#### Writing

| Procedure | Status | Notes |
|-----------|--------|-------|
| `write` | ✅ | Builtin |
| `write-shared` | ✅ | Builtin |
| `write-simple` | ✅ | Builtin |
| `display` | ✅ | Builtin |
| `newline` | ✅ | Builtin |
| `write-char` | ✅ | Builtin |
| `write-string` | ❌ | Not implemented |
| `write-u8` | ❌ | Not implemented |
| `write-bytevector` | ❌ | Not implemented |
| `flush-output-port` | ❌ | Not implemented |

### 6.14 System Interface

| Procedure | Status | Notes |
|-----------|--------|-------|
| `load` | ✅ | Builtin |
| `file-exists?` | ✅ | Builtin |
| `delete-file` | ✅ | Builtin |
| `command-line` | ✅ | Builtin |
| `exit` | ✅ | Builtin |
| `emergency-exit` | ✅ | Builtin |
| `get-environment-variable` | ✅ | Builtin |
| `get-environment-variables` | ✅ | Builtin |
| `current-second` | ❌ | Not implemented |
| `current-jiffy` | ❌ | Not implemented |
| `jiffies-per-second` | ❌ | Not implemented |
| `features` | ❌ | Not implemented as a procedure |

---

## Appendix A: Standard Libraries

### `(scheme base)`

**Completeness:** ~80% of exported identifiers implemented

The `(scheme base)` library is the most complete, exporting the majority of core procedures, syntax forms, and predicates listed in sections above.

**Notable gaps in (scheme base):**
- `include`, `include-ci`, `syntax-error`
- `floor/`, `floor-quotient`, `floor-remainder`, `truncate/`, `truncate-quotient`, `truncate-remainder`
- `exact-integer-sqrt`
- `string->vector`, `vector->string`
- `bytevector?`, `make-bytevector`, `bytevector`, `bytevector-length`, `bytevector-u8-ref`, `bytevector-u8-set!`, `bytevector-copy`, `bytevector-copy!`, `bytevector-append`
- `utf8->string`, `string->utf8`
- `read-error?`, `file-error?`
- `call-with-port`
- `write-string`, `flush-output-port`
- `features`

### `(scheme case-lambda)`

| Export | Status |
|--------|--------|
| `case-lambda` | ✅ |

**Completeness:** 100%

### `(scheme char)`

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

**Completeness:** 100%

### `(scheme complex)`

❌ **Not defined as a library.** No native complex number type exists.

### `(scheme cxr)`

| Export | Status |
|--------|--------|
| `caar` | ✅ |
| `cadr` | ✅ |
| `cdar` | ✅ |
| `cddr` | ✅ |
| `caaar` | ✅ |
| `caadr` | ✅ |
| `cadar` | ✅ |
| `caddr` | ✅ |
| `cdaar` | ✅ |
| `cdadr` | ✅ |
| `cddar` | ✅ |
| `cdddr` | ✅ |
| `cadddr` | ✅ |
| `cddddr` | ✅ |

**Completeness:** ⚠️ Partial — 14 of 24 standard cxr combinations exported. Missing the full set of 4-level compositions (`caaaar` through `cddddr`).

### `(scheme eval)`

| Export | Status |
|--------|--------|
| `eval` | ✅ |
| `environment` | ✅ |

**Completeness:** 100%

### `(scheme file)`

| Export | Status |
|--------|--------|
| `file-exists?` | ✅ |
| `delete-file` | ✅ |
| `open-input-file` | ❌ |
| `open-binary-input-file` | ❌ |
| `open-output-file` | ❌ |
| `open-binary-output-file` | ❌ |
| `call-with-input-file` | ❌ |
| `call-with-output-file` | ❌ |
| `with-input-from-file` | ❌ |
| `with-output-to-file` | ❌ |

**Completeness:** ~20%

### `(scheme inexact)`

| Export | Status |
|--------|--------|
| `finite?` | ✅ |
| `infinite?` | ✅ |
| `nan?` | ✅ |
| `sqrt` | ✅ |
| `exp` | ❌ |
| `log` | ❌ |
| `sin` | ❌ |
| `cos` | ❌ |
| `tan` | ❌ |
| `asin` | ❌ |
| `acos` | ❌ |
| `atan` | ❌ |

**Completeness:** ~33%

### `(scheme lazy)`

| Export | Status |
|--------|--------|
| `delay` | ✅ |
| `force` | ✅ |
| `delay-force` | ✅ |
| `make-promise` | ✅ |
| `promise?` | ✅ |

**Completeness:** 100%

### `(scheme load)`

| Export | Status |
|--------|--------|
| `load` | ✅ |

**Completeness:** 100%

### `(scheme process-context)`

| Export | Status |
|--------|--------|
| `command-line` | ✅ |
| `exit` | ✅ |
| `emergency-exit` | ✅ |
| `get-environment-variable` | ✅ |
| `get-environment-variables` | ✅ |

**Completeness:** 100%

### `(scheme read)`

| Export | Status |
|--------|--------|
| `read` | ✅ |

**Completeness:** 100%

### `(scheme repl)`

| Export | Status |
|--------|--------|
| `interaction-environment` | ✅ |

**Completeness:** 100%

### `(scheme time)`

| Export | Status |
|--------|--------|
| `current-second` | ❌ |
| `current-jiffy` | ❌ |
| `jiffies-per-second` | ❌ |

**Completeness:** 0% (placeholder library)

### `(scheme write)`

| Export | Status |
|--------|--------|
| `write` | ✅ |
| `display` | ✅ |
| `write-shared` | ✅ |
| `write-simple` | ✅ |

**Completeness:** 100%

### `(scheme r5rs)`

❌ **Not defined as a library.**

---

## Number Syntax

| Feature | Status | Examples |
|---------|--------|----------|
| Decimal integers | ✅ | `42`, `-7`, `0` |
| Floating point | ✅ | `3.14`, `-0.5` |
| Scientific notation | ✅ | `1e10`, `2.5E-3` |
| Binary prefix `#b` | ✅ | `#b1010` → 10 |
| Octal prefix `#o` | ✅ | `#o77` → 63 |
| Decimal prefix `#d` | ✅ | `#d42` → 42 |
| Hex prefix `#x` | ✅ | `#x1f` → 31 |
| Exactness `#e` | ✅ | `#e42` → exact 42 |
| Exactness `#i` | ✅ | `#i42` → inexact 42.0 |
| Combined prefixes | ✅ | `#e#x1f`, `#b#i1010` |
| Special `+inf.0` | ✅ | Positive infinity |
| Special `-inf.0` | ✅ | Negative infinity |
| Special `+nan.0` | ✅ | Not a number |
| Rational literals `3/4` | ❌ | Use `(rat 3/4)` macro instead |
| Complex literals `1+2i` | ❌ | Use `(cpx 1 + 2 i)` macro instead |
| Polar form `3@4` | ❌ | Not supported |

---

## Testing Coverage

| Test File | Features Tested |
|-----------|----------------|
| `lib_tests.rs` | Core: arithmetic, lists, closures, macros, GC, rationals, tail calls (~150 tests) |
| `library_tests.rs` | Module system: `define-library`, `import`, library resolution |
| `native_tests.rs` | FFI: `FromLisp`/`ToLisp` conversions |
| `r5rs_pitfalls_tests.rs` | Edge cases: `call/cc`, hygiene, hyper-static scope |
| `r5rs_chibi_tests.rs` | R5RS compliance (Chibi-Scheme test suite subset) |
| `peroxide_r5rs_tests.rs` | R5RS compliance (Peroxide test suite) |
| `environment_tests.rs` | `eval`, `environment`, scoping |
| `syntax_extended_tests.rs` | `syntax-case`, hygienic macros |
| `system_tests.rs` | System-level features |
| `continuation_delay_tests.rs` | `call/cc`, `delay`, `force`, continuations |
| `issue_tests.rs` | Regression tests for bug fixes |

**Key areas lacking test coverage:**
- Bytevector operations
- File I/O ports
- Binary I/O
- Time functions
- Complex/rational standard procedures
- `define-record-type` field mutators
- `string->number` / `number->string` edge cases

---

## Implementation Priorities

### High Priority (Core R7RS-small compliance)

1. **Bytevector operations** — The `Value::Bytevector` type exists but no operations are exposed. Implementing `bytevector?`, `make-bytevector`, `bytevector-length`, `bytevector-u8-ref`, `bytevector-u8-set!`, `bytevector-copy`, `bytevector-append`, `utf8->string`, `string->utf8` would complete §6.9.
2. **File I/O ports** — `open-input-file`, `open-output-file`, `call-with-input-file`, `call-with-output-file`, `with-input-from-file`, `with-output-to-file` are essential for practical programs.
3. **Division procedures** — `floor-quotient`, `floor-remainder`, `truncate-quotient`, `truncate-remainder` (R7RS §6.2.6).
4. **`write-string`** and **`flush-output-port`** — Common output operations.
5. **`read-error?`** and **`file-error?`** — Error classification predicates.

### Medium Priority (Extended functionality)

6. **Transcendental functions** — `exp`, `log`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan` (requires `libm` or similar in `no_std`).
7. **Time functions** — `current-second`, `current-jiffy`, `jiffies-per-second` (requires platform time access).
8. **`number->string`** with radix parameter and **`string->number`** with radix parameter.
9. **`exact-integer-sqrt`** — Integer square root returning two values.
10. **`vector->string`** and **`string->vector`** — Conversion between vectors of characters and strings.
11. **`include`** and **`include-ci`** — Source file inclusion.
12. **`syntax-error`** — Compile-time error reporting in macros.
13. **`features`** procedure — Runtime feature query.
14. **`call-with-port`** — Port lifecycle management.

### Low Priority (Optional / rare usage)

15. **Complex number procedures** — `make-rectangular`, `make-polar`, `real-part`, `imag-part`, `magnitude`, `angle` (requires native complex type or significant refactoring).
16. **`(scheme complex)`** library.
17. **`(scheme r5rs)`** compatibility library.
18. **`scheme-report-environment`** / **`null-environment`** — Legacy R5RS environment constructors.
19. **`rationalize`**, **`numerator`**, **`denominator`** — Require native rational type.
20. **Binary I/O** — `read-u8`, `peek-u8`, `write-u8`, `read-bytevector`, `write-bytevector`, `open-input-bytevector`, `open-output-bytevector`, `get-output-bytevector`.
21. **Full cxr library** — Complete all 24 four-level compositions (`caaaar` through `cddddr`).

---

## Non-Standard Extensions

Grift includes several features beyond R7RS-small:

| Feature | Description |
|---------|-------------|
| `syntax-case` | R6RS-style `syntax-case` macro system |
| `error-object-type` | Additional error object accessor |
| `gc`, `gc-enable`, `gc-disable`, `gc-enabled?` | Garbage collector control |
| `arena-stats` | Arena allocator statistics |
| `identifier?`, `bound-identifier=?`, `free-identifier=?` | Syntax-case support procedures |
| `datum->syntax`, `syntax->datum`, `generate-temporaries` | Syntax object manipulation |
| `rat`, `cpx`, `rat-cpx` macros | User-level rational/complex arithmetic |
| `filter`, `fold`, `fold-left`, `fold-right` | SRFI-1 style list operations |
| `compose`, `identity`, `constantly`, `flip`, `curry` | Functional programming utilities |

---

## Architectural Notes

- **Arena-based allocation:** All values live in a custom arena (`grift_arena`). No heap allocation in core crates (`no_std`, `no_alloc`).
- **Trampoline-based evaluation:** Tail calls handled via `TrampolineState` with 48 continuation types, enabling proper tail recursion without stack overflow.
- **Hygienic macros:** Full `syntax-case` support with `syntax-rules` layered on top as a macro.
- **Module system:** R7RS `define-library` / `import` with support for `only`, `except`, `rename`, `prefix` import modifiers.
- **Record types:** Native `define-record-type` (R7RS §5.5) with constructor, predicate, accessors, and mutators (max 32 fields).
- **Numeric tower:** Two native types — `isize` (exact integers) and `f64` (inexact floats). Rationals and complex numbers available only through library-level macros.
