# Grift's Syntax-Case and Macro System

This document provides a comprehensive explanation of grift's implementation of `syntax-case` and macro handling, with a focus on hygiene, trade-offs compared to other Scheme implementations, and the implications for module systems.

## Table of Contents

1. [Overview](#overview)
2. [Grift's Macro Architecture](#grifts-macro-architecture)
3. [Hygiene Implementation](#hygiene-implementation)
4. [Phase Separation (or Lack Thereof)](#phase-separation-or-lack-thereof)
5. [First-Class Syntax Objects](#first-class-syntax-objects)
6. [Trade-offs vs. Racket and Guile](#trade-offs-vs-racket-and-guile)
7. [Macros That Work Differently](#macros-that-work-differently)
8. [Module System Considerations](#module-system-considerations)
9. [Edge Cases and Limitations](#edge-cases-and-limitations)

---

## Overview

Grift implements a hygienic macro system based on the "Macros that Work" algorithm (Clinger & Rees, 1991) with `syntax-case` as the primary pattern-matching facility. Unlike Racket and Guile, grift does not enforce strict phase separation, which has significant implications for what macros are possible and how they behave.

### Key Characteristics

| Feature | Grift | Racket | Guile |
|---------|-------|--------|-------|
| Macro system | syntax-case | syntax-case with phases | syntax-case with phases |
| Phase separation | **No** | **Yes (strict)** | **Yes (strict)** |
| First-class syntax objects | **Yes** | Limited | Limited |
| Runtime syntax creation | **Yes** | Restricted | Restricted |
| Compilation caching | N/A (interpreted) | Yes | Yes |
| `no_std` support | **Yes** | No | No |

---

## Grift's Macro Architecture

### Evaluation-Time Expansion

Grift performs macro expansion during evaluation, not as a separate pre-processing phase. This means:

1. **Macros are evaluated like regular code**: `define-syntax` creates a binding in the macro environment during evaluation
2. **Syntax objects can capture runtime lexical environments**: When `(syntax x)` is evaluated, it can see variables bound at runtime
3. **No compile-time/runtime phase boundary**: There's no distinction between "macro time" and "run time"

```scheme
;; In Grift, this works:
(let ((x 10))
  (let ((stx (syntax x)))
    (let ((x 20))
      (define-syntax use-stx
        (lambda (_) stx))
      (use-stx))))  ; => 10
```

### Components of the Macro System

The macro system consists of several key components in `expand.rs`:

1. **Gensym**: Generates unique symbols (`#:g0`, `#:g1`, etc.) for hygiene
2. **Marks**: Track macro expansion scopes (each expansion gets a fresh mark)
3. **Bindings**: Pattern variable → value mappings from `syntax-case` matching
4. **Renames**: Hygiene rename environment for macro-introduced bindings
5. **Lexical Environment Capture**: Syntax objects store their creation-site environment

---

## Hygiene Implementation

### Mark-Based Hygiene

Grift uses mark-based hygiene as described in "Macros that Work":

1. **Each macro expansion creates a fresh mark**
2. **Marks are applied to all identifiers in the template**
3. **Two identifiers are `bound-identifier=?` if they have the same name AND same marks**

```rust
// From expand.rs: mark_syntax
pub fn mark_syntax(&mut self, stx: ArenaIndex) -> EvalResult {
    let (expr, marks, subst, lex_env) = self.lisp.syntax_parts_with_env(stx)?;
    let new_mark = self.gensym_simple()?;  // Fresh mark
    let new_marks = self.lisp.cons(new_mark, marks)?;
    self.lisp.syntax_with_env(expr, new_marks, subst, lex_env)
}
```

### bound-identifier=? vs free-identifier=?

**`bound-identifier=?`**: Two identifiers are `bound-identifier=?` if they would **bind the same variable** if used in a binding position. This requires:
- Same name
- Same marks (introduced at the same macro expansion level)

**`free-identifier=?`**: Two identifiers are `free-identifier=?` if they **refer to the same binding**. This requires:
- Both resolve to the same binding in their respective lexical environments
- OR both are unbound and have the same name

```scheme
;; Example: bound-identifier=? with different scopes
(define-syntax test-bound-id-eq
  (lambda (stx)
    (syntax-case stx ()
      ((kw)
       (let ((id1 (datum->syntax (syntax kw) 'foo))
             (id2 (datum->syntax (syntax kw) 'foo)))
         (if (bound-identifier=? id1 id2)
             (syntax #t)
             (syntax #f)))))))

(test-bound-id-eq)  ; => #t (same name, same template => same marks)
```

### Hygiene for Binding Forms

When transcribing `lambda` and other binding forms, grift:

1. Identifies which parameters came from user code (pattern variables)
2. Generates fresh names (gensyms) for macro-introduced parameters
3. Updates the rename environment so references in the body use the fresh names

```scheme
;; Macro-introduced 'temp' doesn't capture user's 'temp'
(define-syntax swap!
  (lambda (stx)
    (syntax-case stx ()
      ((_ a b)
       (syntax (let ((temp a))    ; 'temp' is macro-introduced
                 (set! a b)
                 (set! b temp)))))))

(define temp 999)
(define x 1)
(define y 2)
(swap! x y)
x     ; => 2
y     ; => 1
temp  ; => 999 (not captured!)
```

---

## Phase Separation (or Lack Thereof)

### What Phase Separation Means

In Racket and Guile, code exists at different "phases":

- **Phase 0 (runtime)**: Regular program execution
- **Phase 1 (compile-time)**: Macro transformation
- **Phase 2, 3, ...**: Macros that define macros, etc.

Each phase has its own environment. A value at phase 0 cannot directly access a binding at phase 1.

### Grift's Single-Phase Model

Grift does **not** enforce phase separation. There is one unified environment where:

- Macro transformers run during evaluation
- Syntax objects can capture current lexical bindings
- Runtime-created syntax can be used in macro expansion

This is why tests in `syntax_proper_tests.rs` that work in grift would **fail in Racket/Guile**:

```scheme
;; Test 1.2: Syntax Objects Through Procedures
;; This works in Grift but NOT in Racket/Guile

(define (make-syntax-getter val)
  (let ((x val))
    (syntax x)))     ; Creates syntax capturing current 'x' binding

(define stx1 (make-syntax-getter 100))
(define stx2 (make-syntax-getter 200))

(define-syntax test1 (lambda (_) stx1))
(define-syntax test2 (lambda (_) stx2))

(test1)  ; => 100 (in Grift)
(test2)  ; => 200 (in Grift)
```

**Why Racket/Guile reject this:**

1. `(syntax x)` created at runtime (phase 0) doesn't carry a full lexical environment
2. When used at phase 1 (macro expansion), the runtime binding of `x` isn't accessible
3. Phase discipline prevents runtime closures from minting compile-time syntax

---

## First-Class Syntax Objects

### Syntax Objects as Values

In grift, syntax objects are truly first-class values:

```scheme
;; Test 2.2: Syntax Objects in Data Structures
(define (make-stx-list)
  (let ((a 1) (b 2) (c 3))
    (list (syntax a) (syntax b) (syntax c))))

(define stx-list (make-stx-list))

(define-syntax sum-stx-list
  (lambda (_)
    (syntax-case stx-list ()
      ((x y z)
       (syntax (+ x y z))))))

(sum-stx-list)  ; => 6 (1+2+3)
```

**This is controversial because:**

1. Syntax objects are heap-allocated closures over environments
2. They can be stored, reordered, and destructured freely
3. Each syntax object retains its independent creation-site environment

**Racket's alternative approach:**
- Syntax parameters
- Lifts
- `local-expand`
- Controlled phase shifting

---

## Trade-offs vs. Racket and Guile

### What Grift Gains

1. **Simplicity**: No phase system to understand
2. **Flexibility**: Runtime-created syntax works naturally
3. **First-class syntax**: Syntax objects behave like regular values
4. **`no_std` compatibility**: Works in embedded systems without OS support

### What Grift Loses

1. **Compilation caching**: Cannot cache expanded code separately from runtime state
2. **Separate compilation**: Modules can't be compiled independently
3. **Macro serialization**: Syntax objects with captured environments can't be serialized
4. **Cross-module guarantees**: No static checking of macro expansions across modules
5. **Phase errors**: Can't catch errors like "identifier used at wrong phase"

### Semantic Differences

| Aspect | Grift | Racket/Guile |
|--------|-------|--------------|
| `(syntax x)` at runtime | Captures current environment | Returns weakened syntax |
| Storing syntax in lists | Works naturally | Requires special handling |
| Syntax in closures | Preserves full context | Limited context |
| Macro → runtime leakage | Allowed | Prevented |
| Runtime → macro leakage | Allowed | Prevented |

---

## Macros That Work Differently

### Example 1: Runtime Syntax Creation

**Works in Grift:**
```scheme
(define (make-incrementer n)
  (let ((amount n))
    (syntax (+ x amount))))  ; Captures 'amount' from runtime

(define-syntax inc-5
  (lambda (_) (make-incrementer 5)))

(let ((x 10)) (inc-5))  ; => 15
```

**In Racket:** This would fail because `(syntax (+ x amount))` at runtime doesn't capture the local `amount` binding for use at compile time.

### Example 2: Dynamic Syntax Construction

**Works in Grift:**
```scheme
(define captured-stx #f)

(define-syntax capture
  (lambda (stx)
    (syntax-case stx ()
      ((kw val)
       (begin
         (set! captured-stx (syntax val))
         (syntax #f))))))

(let ((x 333)) (capture x))  ; Sets captured-stx

(define-syntax use-captured
  (lambda (_) captured-stx))

(let ((x 444)) (use-captured))
```

**Note:** Test 7.1 is currently **ignored** in grift because pattern variables bind to raw symbols, not syntax objects with full lexical context. This would need call-site environment propagation through `syntax-case`.

### Example 3: Mutual Recursion Between Runtime and Macros

**Works in Grift:**
```scheme
(define (helper x)
  (syntax (+ 1 x)))

(define-syntax with-one-added
  (lambda (stx)
    (syntax-case stx ()
      ((_ e) (helper (syntax e))))))

(with-one-added 5)  ; => 6
```

**In Racket:** The helper function at phase 0 can't directly return syntax for use at phase 1.

---

## Module System Considerations

### Challenges for a Grift Module System

A module/library system for grift would need to address:

1. **No Phase Separation**: Can't rely on phase-based import/export semantics
2. **Runtime Environment Capture**: Syntax objects may reference bindings not visible to importer
3. **Serialization**: Modules can't be saved to disk with captured environments

### Potential Approaches

#### Approach 1: Simple Name-Based Imports

```scheme
(define-library (my-lib)
  (export my-macro helper-proc)
  
  (define (helper-proc x) (* x 2))
  
  (define-syntax my-macro
    (lambda (stx)
      (syntax-case stx ()
        ((_ e) (syntax (helper-proc e)))))))
```

**Semantics:**
- Import copies bindings into the importing module's environment
- Macros expand using importer's view of exported bindings
- No cross-module lexical capture

#### Approach 2: Phased Re-Evaluation

```scheme
(define-library (my-lib)
  (export my-macro)
  
  (begin-for-syntax  ; Evaluated when imported
    (define (helper x) (* x 2)))
  
  (define-syntax my-macro
    (lambda (stx) ...)))
```

**Semantics:**
- `begin-for-syntax` code is re-evaluated in each importer
- Similar to Racket's phase 1 code but without strict separation

#### Approach 3: First-Class Modules (Environments)

```scheme
(define my-module
  (let ()
    (define x 10)
    (define-syntax getter
      (lambda (_) (syntax x)))
    (module-export getter x)))

(module-import my-module getter)
(getter)  ; => 10
```

**Semantics:**
- Modules are first-class environment objects
- Imports establish links to the original environment
- Captures grift's runtime/macro unification naturally

### Recommended Approach for Grift

Given grift's single-phase model and `no_std` constraints, a **simple name-based import system** is most appropriate:

1. **No separate compilation**: Modules are always loaded and evaluated together
2. **Explicit exports**: Only explicitly exported bindings are visible
3. **Macro expansion occurs in importer context**: Avoids captured environment issues
4. **Compatible with arena allocation**: No need for unbounded environment storage

```scheme
;; Proposed syntax
(define-library (grift utils)
  (export fold filter map)
  (import (grift base))
  
  (define (fold f init lst) ...)
  (define (filter pred lst) ...)
  (define (map f lst) ...))

(import (grift utils))
(map (lambda (x) (* x 2)) '(1 2 3))  ; => (2 4 6)
```

---

## Edge Cases and Limitations

### Currently Ignored Tests

**Test 7.1: Syntax Objects with Mutation**

```scheme
(define box #f)

(define-syntax capture
  (lambda (stx)
    (syntax-case stx ()
      ((kw val)
       (begin
         (set! box (syntax val))  ; Capture pattern variable as syntax
         (syntax #f))))))

(let ((x 333)) (capture x))

(define-syntax use-captured
  (lambda (_) box))

(let ((x 444)) (use-captured))  ; Should be 333, not 444
```

**Status**: Currently ignored because pattern variables bind to raw symbols, not syntax objects with call-site lexical context.

**To fix**: Would require passing the macro call-site environment through `syntax-case` pattern matching.

### Known Limitations

1. **letrec-syntax**: Not implemented
2. **syntax-error**: Not implemented
3. **Improper list patterns with rest**: Patterns like `(a b . rest)` combined with `append` may have issues
4. **String operations in macros**: `string-append` and `string->symbol` are not available in the `no_std` environment for compile-time symbol generation

### Verified Working Features

The following features were previously thought to have limitations but have been verified to work correctly:

1. **Nested Ellipsis**: Deeply nested ellipsis patterns like `((a ...) ...)` work correctly
2. **Fenders (Guards)**: Pattern guards in `syntax-case` work correctly and can access pattern bindings
3. **Literal Matching**: Complex literal keyword matching with `free-identifier=?` semantics works correctly
4. **Identifier Comparison**: `bound-identifier=?` and `free-identifier=?` work as expected with `datum->syntax`

### Design Constraints

1. **Fixed Arena Size**: All syntax objects must fit in the pre-allocated arena
2. **No Heap Allocation**: Core macro system is `no_std`, `no_alloc`
3. **Copy Types Only**: Syntax objects must be `Copy` for arena storage

---

## Conclusion

Grift's macro system represents a different point in the design space than Racket or Guile:

- **More flexible**: Runtime and macro time are unified
- **Simpler model**: No phase system to learn
- **Different guarantees**: No phase-based safety checks
- **Suitable for**: Embedded systems, experimentation, teaching

The tests in `syntax_proper_tests.rs` demonstrate behaviors that are **valid syntax-case** but **would fail in Racket/Guile** due to their stricter phase discipline. This is not a bug in grift—it's a deliberate design choice that trades compilation-time safety for runtime flexibility.

For users coming from Racket or Guile:
- Don't rely on phase separation for isolation
- Syntax objects are more powerful (and less restricted)
- Cross-module macro behavior may differ
- The system is well-suited for interactive development and embedded use

---

## References

1. Clinger, W. & Rees, J. (1991). "Macros that Work"
2. Dybvig, R.K., Hieb, R., & Bruggeman, C. "Syntactic Abstraction in Scheme" (psyntax)
3. R6RS Chapter 11 (Syntax-case)
4. R7RS Section 4.3 (Macros)
5. Flatt, M. "Composable and Compilable Macros" (Racket's approach)
