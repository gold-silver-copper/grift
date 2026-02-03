# Potential Optimizations for Grift Scheme Interpreter

This document outlines potential optimizations and improvements that can be made to speed up the Grift Scheme interpreter. The optimizations are categorized by priority and area.

## Table of Contents

1. [Macro System Optimizations](#macro-system-optimizations)
2. [Standard Library Optimizations](#standard-library-optimizations)
3. [Evaluator Optimizations](#evaluator-optimizations)
4. [General Recommendations](#general-recommendations)

---

## Macro System Optimizations

Located in `crates/grift_eval/src/evaluator/macros.scm`

### High Priority

#### 1. Simplify Named Let Helper Chain

**Current Issue**: The named let implementation uses three helper macros (`%named-let-build` → `%named-let-expand` → `%named-let-extract-and-call`) creating unnecessary expansion overhead.

**Recommendation**: Collapse into a single helper macro:

```scheme
(define-syntax %named-let-helper
  (syntax-rules ()
    ((%named-let-helper loop () (vars ...) (vals ...) (body ...))
     ((lambda (vars ...)
        (letrec ((loop (lambda (vars ...) . body)))
          (loop vars ...)))
      vals ...))
    ((%named-let-helper loop ((var val) . rest) (vars ...) (vals ...) (body ...))
     (%named-let-helper loop rest (vars ... var) (vals ... val) (body ...)))))
```

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

#### 3. Simplify `case` Macro Patterns

**Current Issue**: 6 patterns for handling with/without else × single/multiple clauses.

**Recommendation**: Reduce to 3 patterns:

```scheme
(define-syntax case
  (syntax-rules (else)
    ((case key) (if #f #f))
    ((case key (else result ...)) (begin result ...))
    ((case key ((datum ...) result ...) . rest)
     (if (memv key '(datum ...))
         (begin result ...)
         (case key . rest)))))
```

**Benefit**: Simpler to maintain, same functionality.

#### 4. Optimize `define-values` with syntax-case

**Current Issue**: Explicit patterns for arities 0-4 create code duplication.

**Recommendation**: Use syntax-case with a helper to generate definitions dynamically for any arity.

### Low Priority

#### 5. Remove Commented Dead Code

**Issue**: Lines 279-283 contain commented-out quasiquote implementation.

**Action**: Remove or move to documentation.

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

#### 1. Make Core Functions Tail-Recursive

The following functions should be converted to use accumulator patterns for tail-call optimization:

##### `map` (currently non-tail-recursive)
```scheme
;; Current (non-tail-recursive):
(define (map f lst)
  (if (null? lst) '()
      (cons (f (car lst)) (map f (cdr lst)))))

;; Optimized (tail-recursive):
(define (map f lst)
  (define (map-iter lst acc)
    (if (null? lst)
        (reverse acc)
        (map-iter (cdr lst) (cons (f (car lst)) acc))))
  (map-iter lst '()))
```

##### `filter` (currently non-tail-recursive)
```scheme
;; Current:
(define (filter pred lst)
  (if (null? lst) '()
      (if (pred (car lst))
          (cons (car lst) (filter pred (cdr lst)))
          (filter pred (cdr lst)))))

;; Optimized:
(define (filter pred lst)
  (define (filter-iter lst acc)
    (if (null? lst)
        (reverse acc)
        (if (pred (car lst))
            (filter-iter (cdr lst) (cons (car lst) acc))
            (filter-iter (cdr lst) acc))))
  (filter-iter lst '()))
```

##### `append` (currently non-tail-recursive)
```scheme
;; Current:
(define (append a b)
  (if (null? a) b
      (cons (car a) (append (cdr a) b))))

;; Optimized:
(define (append a b)
  (define (append-iter a acc)
    (if (null? a)
        acc
        (append-iter (cdr a) (cons (car a) acc))))
  (append-iter (reverse a) b))
;; Alternative using fold:
(define (append a b)
  (fold-right cons b a))
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

#### 2. Fix O(n²) in `flatten`

**Current Issue**: Uses repeated `append` calls leading to O(n²) complexity.

```scheme
;; Current:
(define (flatten lst)
  (cond
    ((null? lst) '())
    ((not (pair? lst)) (list lst))
    (else (append (flatten (car lst)) (flatten (cdr lst))))))

;; Optimized (O(n)):
(define (flatten lst)
  (define (flatten-iter lst acc)
    (cond
      ((null? lst) acc)
      ((not (pair? lst)) (cons lst acc))
      (else (flatten-iter (car lst) (flatten-iter (cdr lst) acc)))))
  (flatten-iter lst '()))
```

### Medium Priority

#### 3. Avoid Double Calls in `filter-map`

**Current Issue**: Calls `(f (car lst))` twice.

```scheme
;; Current:
(define (filter-map f lst)
  (if (null? lst) '()
      (if (f (car lst))
          (cons (f (car lst)) (filter-map f (cdr lst)))
          (filter-map f (cdr lst)))))
```

**Fix**: Use `let` to store result (but note: the file comment mentions a "let-binding issue in recursion" - investigate first).

#### 4. Optimize `take-right` and `drop-right`

**Current Issue**: Both call `length` unnecessarily.

```scheme
;; Current:
(define (take-right lst k)
  (drop lst (- (length lst) k)))

;; More efficient approach:
;; Use a "lag pointer" technique - no length call needed
(define (take-right lst k)
  (define (helper fast slow)
    (if (null? fast)
        slow
        (helper (cdr fast) (cdr slow))))
  (define (skip-k lst k)
    (if (= k 0) lst (skip-k (cdr lst) (- k 1))))
  (helper (skip-k lst k) lst))
```

#### 5. Consolidate Member/Assoc Functions

**Issue**: `member`, `memq`, `memv`, `assoc`, `assq`, `assv` share nearly identical logic.

**Recommendation**: Create internal helpers:
```scheme
(define (%member-by pred obj lst)
  (if (null? lst) #f
      (if (pred obj (car lst)) lst
          (%member-by pred obj (cdr lst)))))

(define (memq obj lst) (%member-by eq? obj lst))
(define (memv obj lst) (%member-by eqv? obj lst))
(define (member-equal obj lst) (%member-by equal? obj lst))
```

### Low Priority

#### 6. Optimize Character Case-Insensitive Comparisons

**Issue**: Each `char-ci` comparison calls `char-foldcase` twice.

```scheme
;; Current:
(define (char-ci=? c1 c2)
  (char=? (char-foldcase c1) (char-foldcase c2)))

;; Could cache foldcase results when comparing multiple characters
```

---

## Evaluator Optimizations

### 1. Support More Builtins in Macro Expansion

**Current Limitation**: Only `+`, `-`, `<`, `>`, `zero?`, `eq?`, `eqv?`, `null?`, `pair?`, `symbol?` are supported in macro expansion.

**Recommendation**: Add support for `*`, `/`, `modulo`, `cons`, `car`, `cdr`, `list` to enable more computation at compile time.

**Location**: `crates/grift_eval/src/evaluator/expand.rs`, function `apply_builtin_for_expansion`.



---

## General Recommendations

### Short-Term (Easy Wins)

1. Convert `map`, `filter`, `append`, `range` to tail-recursive versions
2. Fix `flatten` O(n²) complexity
3. Remove dead code/comments from macros.scm
4. Simplify `case` macro patterns

### Medium-Term

1. Convert `%cl-arity-check` and `define-values` to syntax-case
2. Add more builtins to macro expansion mini-evaluator
3. Consolidate similar helper functions in stdlib.scm


- Tail-recursion is critical since this is a no_std implementation

