# Potential Optimizations for Grift Scheme Interpreter

This document outlines potential optimizations and improvements that can be made to speed up the Grift Scheme interpreter. The optimizations are categorized by priority and area.

---

## Implementation Progress

**Last Updated**: 2026-02-03

### Completed Optimizations

#### Standard Library (stdlib.scm)
- ✅ **map**: Converted to tail-recursive with accumulator pattern
- ✅ **filter**: Converted to tail-recursive with accumulator pattern
- ✅ **append**: Converted to tail-recursive using internal reverse helper
- ✅ **range**: Converted to tail-recursive with countdown iterator
- ✅ **flatten**: Fixed O(n²) complexity by eliminating repeated append calls, now O(n) tail-recursive
- ✅ **filter-map**: Eliminated double function calls using let binding, now tail-recursive
- ✅ **take-right**: Implemented lag-pointer technique for O(n) single traversal
- ✅ **drop-right**: Implemented lag-pointer technique for O(n) single traversal, fixed argument order bug
- ✅ **split-at**: Fixed argument order bug (was passing (lst k) to take/drop which expect (n lst))

#### Macro System (macros.scm)
- ✅ **Named let helpers**: Reduced from 3 helper macros to 1 (`%named-let-helper`)
- ✅ **case macro**: Simplified from 6 patterns to 3 patterns
- ✅ **Dead code removal**: Removed commented-out quasiquote alternative implementation

### Notes on Implementation

1. **Tail recursion importance**: All optimizations prioritize tail-call optimization since this is a `no_std` implementation with limited stack space.

2. **append optimization**: Uses an internal reverse helper to avoid forward references to other stdlib functions.

3. **take-right/drop-right optimization**: Successfully implemented using the lag-pointer technique after resolving issues with the stdlib parsing. The original issue was not with nested lambda/letrec combinations in the evaluator, but rather with inline comments (`;;`) within stdlib function bodies causing parse errors in the `include_stdlib!` macro. The solution was to remove inline comments from the function bodies.

4. **Bugs fixed during optimization**:
   - `drop-right` had wrong argument order: was calling `(take lst n)` instead of `(take n lst)`
   - `split-at` had the same issue with take/drop argument order

5. **member/assoc consolidation**: Skipped to maintain minimal changes as these functions work correctly.

### Known Limitations

1. **syntax-case macro limitations**: The `%cl-arity-check` and `define-values` macros cannot easily be converted to use syntax-case with dynamic length calculation because `list?` and `length` are not available during macro expansion. Adding these builtins requires Rust code changes to `expand.rs`.

2. **Macro expansion builtins**: Currently supported builtins in macro expansion are: `car`, `cdr`, `cons`, `list`, `null?`, `pair?`, `symbol?`, `eq?`, `eqv?`, `+`, `-`, `<`, `>`, `zero?`. Adding `*`, `/`, `modulo`, `list?`, `length` would require Rust changes.

---

## Table of Contents

1. [Macro System Optimizations](#macro-system-optimizations)
2. [Standard Library Optimizations](#standard-library-optimizations)
3. [Evaluator Optimizations](#evaluator-optimizations)
4. [General Recommendations](#general-recommendations)

---

## Macro System Optimizations

Located in `crates/grift_eval/src/evaluator/macros.scm`

### High Priority

#### 1. ✅ IMPLEMENTED - Simplify Named Let Helper Chain

**Status**: ✅ Completed - Reduced from 3 helpers to 1

**Original Issue**: The named let implementation uses three helper macros (`%named-let-build` → `%named-let-expand` → `%named-let-extract-and-call`) creating unnecessary expansion overhead.

**Implementation**: Collapsed into a single helper macro `%named-let-helper` as recommended.

**Benefit**: Reduces macro expansion steps from 3 to 1.

#### 2. Convert `%cl-arity-check` to syntax-case

**Current Issue**: Hard-coded patterns for arities 0-8 are repetitive and don't scale.

```scheme
;; Current: Manual enumeration
((%cl-arity-check n ()) (= n 0))
((%cl-arity-check n (a)) (= n 1))
((%cl-arity-check n (a b)) (= n 2))
;; ... etc
```

**Recommendation**: Use `syntax-case` for dynamic arity checking:

```scheme
(define-syntax %cl-arity-check
  (lambda (stx)
    (syntax-case stx ()
      ((_ n formals)
       (if (symbol? (syntax->datum #'formals))
           (syntax #t)  ;; variadic - matches any arity
           (with-syntax ((count (length (syntax->datum #'formals))))
             (syntax (= n count))))))))
```

**Benefit**: Handles any arity dynamically, eliminates manual enumeration.

### Medium Priority

#### 3. ✅ IMPLEMENTED - Simplify `case` Macro Patterns

**Status**: ✅ Completed - Reduced from 6 to 3 patterns

**Original Issue**: 6 patterns for handling with/without else × single/multiple clauses.

**Implementation**: Reduced to 3 patterns as recommended:
1. No clauses case
2. Else clause
3. Regular clause with recursion

**Benefit**: Simpler to maintain, same functionality.

#### 4. Optimize `define-values` with syntax-case

**Current Issue**: Explicit patterns for arities 0-4 create code duplication.

**Recommendation**: Use syntax-case with a helper to generate definitions dynamically for any arity.

### Low Priority

#### 5. ✅ IMPLEMENTED - Remove Commented Dead Code

**Status**: ✅ Completed

**Original Issue**: Lines 279-283 contain commented-out quasiquote implementation.

**Action**: Removed to clean up the codebase.

#### 6. Optimize `or` Macro Temporary Bindings

**Current**: Creates temporary binding on each recursive step:
```scheme
((or test rest ...)
 (let ((temp test))
   (if temp temp (or rest ...))))
```

**Alternative** (when return value isn't needed):
```scheme
((or test rest ...)
 (if test #t (or rest ...)))
```

---

## Standard Library Optimizations

Located in `crates/grift_parser/src/stdlib.scm`

### High Priority

#### 1. ✅ IMPLEMENTED - Make Core Functions Tail-Recursive

**Status**: ✅ Completed for map, filter, append, range, flatten

The following functions have been converted to use accumulator patterns for tail-call optimization:

##### ✅ `map` - IMPLEMENTED
Converted to tail-recursive with accumulator pattern and reverse at the end.

##### ✅ `filter` - IMPLEMENTED
Converted to tail-recursive with accumulator pattern and reverse at the end.

##### ✅ `append` - IMPLEMENTED
Converted to tail-recursive using internal reverse helper to avoid forward references.
Note: Uses a self-contained implementation rather than `fold-right` to avoid dependency ordering issues.

##### ✅ `range` - IMPLEMENTED
Converted to tail-recursive using countdown iterator pattern:
```scheme
(define (range start end)
  (define (range-iter n acc)
    (if (< n start)
        acc
        (range-iter (- n 1) (cons n acc))))
  (range-iter (- end 1) '()))
```
```

##### `range` (currently non-tail-recursive)
```scheme
;; Current:
(define (range start end)
  (if (>= start end) '()
      (cons start (range (+ start 1) end))))

;; Optimized:
(define (range start end)
  (define (range-iter n acc)
    (if (< n start)
        acc
        (range-iter (- n 1) (cons n acc))))
  (range-iter (- end 1) '()))
```

#### 2. ✅ IMPLEMENTED - Fix O(n²) in `flatten`

**Status**: ✅ Completed

**Original Issue**: Uses repeated `append` calls leading to O(n²) complexity.

**Implementation**: Converted to O(n) tail-recursive using accumulator pattern as recommended.

### Medium Priority

#### 3. ✅ IMPLEMENTED - Avoid Double Calls in `filter-map`

**Status**: ✅ Completed

**Original Issue**: Calls `(f (car lst))` twice.

**Implementation**: Converted to tail-recursive with let binding to store result:
```scheme
(define (filter-map f lst)
  (define (filter-map-iter lst acc)
    (if (null? lst)
        (reverse acc)
        (let ((result (f (car lst))))
          (if result
              (filter-map-iter (cdr lst) (cons result acc))
              (filter-map-iter (cdr lst) acc)))))
  (filter-map-iter lst '()))
```

**Note**: The previously mentioned "let-binding issue in recursion" was resolved - the issue was with variable scoping, not let itself.

#### 4. ✅ IMPLEMENTED - Optimize `take-right` and `drop-right`

**Status**: ✅ Completed with lag-pointer technique

**Original Issue**: Both call `length` unnecessarily, requiring O(n) + O(n) = O(2n) traversals.

**Implementation**: Both now use the lag-pointer technique for O(n) single traversal:

```scheme
;;; (take-right lst k) - Return the last k elements of lst
;;; Uses lag-pointer technique: O(n) single traversal
(define (take-right lst k)
  (define (advance p count)
    (if (= count 0)
        p
        (if (null? p)
            '()
            (advance (cdr p) (- count 1)))))
  (define (walk lead lag)
    (if (null? lead)
        lag
        (walk (cdr lead) (cdr lag))))
  (let ((lead (advance lst k)))
    (if (null? lead)
        lst
        (walk lead lst))))

;;; (drop-right lst k) - Return all but the last k elements
;;; Uses lag-pointer technique: O(n) single traversal, tail-recursive
(define (drop-right lst k)
  (define (advance p count)
    (if (= count 0)
        p
        (if (null? p)
            '()
            (advance (cdr p) (- count 1)))))
  (define (walk lead lag acc)
    (if (null? lead)
        (reverse acc)
        (walk (cdr lead) (cdr lag) (cons (car lag) acc))))
  (let ((lead (advance lst k)))
    (if (null? lead)
        '()
        (walk lead lst '()))))
```

**Note**: The original issue was not with nested lambda/letrec in the evaluator, but with the `include_stdlib!` macro parser having trouble with inline comments (`;; comment`) within function bodies. Removing inline comments resolved the issue.

**Bug Fix**: Also fixed `split-at` which had the wrong argument order for `take` and `drop` (was passing `(lst k)` but they expect `(k lst)`).

#### 5. ⏸️ NOT IMPLEMENTED - Consolidate Member/Assoc Functions

**Status**: ⏸️ Skipped to maintain minimal changes

**Issue**: `member`, `memq`, `memv`, `assoc`, `assq`, `assv` share nearly identical logic.

**Recommendation**: Could create internal helpers, but current implementations work correctly and consolidation provides minimal performance benefit.



---

## Evaluator Optimizations

### 1. ⏸️ NOT IMPLEMENTED - Support More Builtins in Macro Expansion

**Status**: ⏸️ Deferred - requires Rust code changes

**Current Limitation**: The following builtins are supported in macro expansion: `+`, `-`, `<`, `>`, `zero?`, `eq?`, `eqv?`, `null?`, `pair?`, `symbol?`, `car`, `cdr`, `cons`, `list`.

**Recommendation**: Add support for `*`, `/`, `modulo`, `list?`, `length` to enable more computation at compile time. This would allow `%cl-arity-check` and `define-values` to be converted to dynamic syntax-case implementations.

**Location**: `crates/grift_eval/src/evaluator/expand.rs`, function `apply_builtin_for_expansion`.

**Note**: This optimization requires Rust code changes and was deemed out of scope for this initial optimization pass focusing on Scheme code.

---

## General Recommendations

### Short-Term (Easy Wins) - ✅ ALL COMPLETED

1. ✅ Convert `map`, `filter`, `append`, `range` to tail-recursive versions
2. ✅ Fix `flatten` O(n²) complexity
3. ✅ Remove dead code/comments from macros.scm
4. ✅ Simplify `case` macro patterns
5. ✅ Simplify named let helper chain
6. ✅ Fix `filter-map` double calls
7. ✅ Optimize `take-right` and `drop-right` with lag-pointer technique
8. ✅ Fix `split-at` argument order bug

### Medium-Term - Remaining Work

1. ⏸️ Convert `%cl-arity-check` and `define-values` to syntax-case (requires `list?` and `length` in macro expansion)
2. ⏸️ Add more builtins to macro expansion mini-evaluator (requires Rust changes)
3. ⏸️ Consolidate similar helper functions in stdlib.scm (minimal benefit)

### Key Principles

- **Tail-recursion is critical** since this is a no_std implementation with limited stack space
- **All core list operations** now use tail-call optimization
- **Minimal changes** - focused on high-impact, low-risk optimizations
- **Tested and verified** - all changes validated with comprehensive test suite

---

## Appendix: The Nested Lambda/Letrec "Issue"

During optimization, there was a note that the lag-pointer technique for `take-right` was "reverted due to complexity with nested lambda/letrec combinations." Investigation revealed that this was **not** an evaluator issue but rather a **stdlib parsing issue**.

### Root Cause

The `include_stdlib!` procedural macro in `crates/grift_macros/src/lib.rs` parses the stdlib.scm file and extracts function definitions. When function bodies contained inline comments (e.g., `;; comment`), the parser would include these in the function body string, which then caused parse errors when the Scheme parser tried to parse them.

### Solution

Simply remove inline comments from within function bodies in `stdlib.scm`. The documentation comments (`;;;`) at the start of each function are properly handled and should be kept.

### Verification

After removing inline comments, the lag-pointer implementation works correctly:

```scheme
(take-right '(1 2 3 4 5) 3)  ; => (3 4 5)
(drop-right '(1 2 3 4 5) 2)  ; => (1 2 3)
```

Both `letrec` with nested `lambda` and internal `define` forms work correctly in the evaluator.

