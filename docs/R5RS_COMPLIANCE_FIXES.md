# R5RS Compliance Fixes - Summary

## Overview

This document summarizes the R5RS compliance fixes implemented in grift to address the limitations identified in the peroxide test suite integration.

## Issues Fixed

### 1. Special Forms Can Be Shadowed ✅ FIXED

**Problem**: Special forms like `begin`, `quote`, `if`, `lambda` etc. could not be shadowed by variable bindings, violating R5RS which states that no identifiers should be reserved.

**Example that didn't work**:
```scheme
((lambda (begin) (begin 1 2 3)) (lambda lambda lambda))
; Should return '(1 2 3) but failed because begin was always treated as special form

(let ((quote -)) (eqv? '1 1))
; Should return #f but failed because quote was always treated as special form
```

**Fix**: Modified `crates/grift_eval/src/evaluator/core.rs`:
- Moved variable binding check (`is_var_bound`) to occur BEFORE all special form checks
- Wrapped all special form handling in `if !is_var_bound { ... }`
- Now special forms are only recognized when NOT bound as variables

**Verification**:
```scheme
;; Test 1: begin shadowing
((lambda (begin) (begin 1 2 3)) (lambda lambda lambda))
;; Returns: (1 2 3) ✓

;; Test 2: quote shadowing
(let ((quote -)) (eqv? '1 1))
;; Returns: #f ✓

;; Test 3: lambda as parameter
((lambda lambda lambda) 'x)
;; Returns: (x) ✓
```

### 2. `let` Uses Parallel Binding Semantics ✅ FIXED

**Problem**: `let` was using sequential binding semantics (like `let*`), meaning each binding could see previous bindings. R5RS specifies that `let` should use parallel binding - all values are evaluated in the outer scope before any bindings occur.

**Example that didn't work**:
```scheme
(let ((x 2) (y 3))
  (let ((x 7) (z (+ x y)))  ; z should be (+ 2 3) = 5, not (+ 7 3) = 10
    (* z x)))               ; Should return 35, but returned 70
```

**Fix**: Modified `crates/grift_eval/src/evaluator/macros.scm`:
- Added new `%let-parallel-helper` macro that collects all variables and values before creating bindings
- Changed `let` macro to use `%let-parallel-helper` instead of recursive `%let-binding`
- `let*` continues to use `%let-binding` for sequential binding

**Implementation**:
```scheme
;; New parallel binding helper
(define-syntax %let-parallel-helper
  (syntax-rules ()
    ((%let-parallel-helper () (vars ...) (vals ...) (body ...))
     ((lambda (vars ...) body ...) vals ...))
    ((%let-parallel-helper ((var val) . rest) (vars ...) (vals ...) (body ...))
     (%let-parallel-helper rest (vars ... var) (vals ... val) (body ...)))))

;; Updated let macro
(define-syntax let
  (syntax-rules ()
    ((let () body ...)
     (begin body ...))
    ((let ((var val) . rest) body ...)
     (%let-parallel-helper ((var val) . rest) () () (body ...)))
    ((let loop bindings body ...)
     (%named-let-helper loop bindings () () (body ...)))))
```

**Verification**:
```scheme
;; Test: parallel let binding
(let ((x 2) (y 3))
  (let ((x 7) (z (+ x y)))  ; z = (+ 2 3) = 5
    (* z x)))               ; (* 5 7) = 35 ✓

;; Test: sequential let* binding
(let ((x 2) (y 3))
  (let* ((x 7) (z (+ x y))) ; z = (+ 7 3) = 10
    (* z x)))               ; (* 10 7) = 70 ✓
```

## Test Results

All existing tests continue to pass with the new fixes:

```
Running tests/peroxide_pitfalls_tests.rs
running 4 tests
test test_peroxide_pitfalls_documentation ... ok
test test_peroxide_pitfalls_section_4_no_reserved_identifiers ... ok
test test_peroxide_pitfalls_section_5_false_nil_distinctness ... ok
test test_peroxide_pitfalls_section_8_miscellaneous ... ok

test result: ok. 4 passed; 0 failed

Running tests/peroxide_r5rs_tests.rs
running 6 tests
test test_peroxide_r5rs_documentation ... ok
test test_peroxide_r5rs_if_cond ... ok
test test_peroxide_r5rs_let_letrec ... ok
test test_peroxide_r5rs_basic_lambda ... ok
test test_peroxide_r5rs_and_or ... ok
test test_peroxide_r5rs_list_operations ... ok

test result: ok. 6 passed; 0 failed
```

## Test Runner

Created `scheme-test-runner` binary to execute full .scm test files:

**Location**: `crates/grift/src/bin/scheme-test-runner.rs`

**Usage**:
```bash
cargo run --bin scheme-test-runner --features std -- tests/scheme/r5rs_pitfall.scm
cargo run --bin scheme-test-runner --features std -- tests/scheme/r5rs-tests.scm
```

**Note**: The test runner has issues with some complex test macros that cause stack overflow during macro expansion. The core fixes are verified to work correctly through direct testing.

## Impact

These fixes bring grift significantly closer to R5RS compliance:

1. **No Reserved Identifiers**: Follows R5RS §7.1.1 which states "All other Scheme identifiers have the same status as variable identifiers."

2. **Proper `let` Semantics**: Follows R5RS §4.2.2 which specifies that in `let`, "the <init>s are evaluated in the current environment (in some unspecified order), the <variable>s are bound to fresh locations holding the results, and the <body> is evaluated in the extended environment."

3. **Better Scheme Compatibility**: Code that uses these R5RS features will now work correctly in grift.

## Files Modified

1. `crates/grift_eval/src/evaluator/core.rs` - Special form shadowing fix
2. `crates/grift_eval/src/evaluator/macros.scm` - Parallel let binding fix
3. `crates/grift_eval/tests/peroxide_pitfalls_tests.rs` - Updated tests to expect correct behavior
4. `crates/grift_eval/tests/peroxide_r5rs_tests.rs` - Updated tests to expect correct behavior
5. `crates/grift/src/bin/scheme-test-runner.rs` - New test runner binary
6. `crates/grift/Cargo.toml` - Added test runner binary configuration

## Remaining Work

The third requirement about "Advanced call/cc tests" remains partially unaddressed:
- The core call/cc implementation works for many cases
- Some advanced tests involving complex continuation manipulation cause stack overflow
- This appears to be a deeper architectural issue that would require significant changes to the continuation handling mechanism
- The test runner was created but has issues with macro expansion in some test setups

## Conclusion

Two out of three limitations have been fully addressed, bringing grift significantly closer to R5RS compliance. The fixes enable proper shadowing of special forms and correct parallel binding semantics for `let`, both critical features of standard Scheme.
