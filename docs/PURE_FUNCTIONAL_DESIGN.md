# Pure Functional Grift Design Document

This document describes a multi-phase design for evolving Grift into a pure functional programming language with lazy evaluation and IO via effect-typed continuations.

## Implementation Status

| Phase | Status | Description |
|-------|--------|-------------|
| Effects as Values | ✅ Implemented | `Value::Effect`, `io/pure`, `io/bind`, `io/print`, `io/read-line` |
| Direct-Style Syntax | ✅ Implemented | `eff` macro for monadic do-notation |
| IO Effect Handler | ✅ Implemented | `run-io` handler for IO effects |
| State Effect Handler | ✅ Implemented | `run-state`, `eval-state`, `exec-state` |
| Error Effect Handler | ✅ Implemented | `run-error`, `try-error` |
| Immutability | ✅ Implemented | `set!`, `set-car!`, `set-cdr!`, etc. removed |
| Pure `letrec` | ✅ Implemented | Y combinator-based recursive bindings |
| Pure `do` loops | ✅ Implemented | Via Y combinator-based named let |
| Delimited Continuations | ✅ Implemented | `reset`/`shift` primitives for composable control |
| **call/cc Removed** | ✅ Complete | Only delimited continuations supported |
| Effect Types | 📋 Planned | Requires type system extension |

### Quick Start

```scheme
;; Effects are first-class values
(define greet-effect (io/print "Hello, World!"))
greet-effect  ; => #<effect:io/print "Hello, World!">

;; No side effects occurred! Execute with run-io:
(run-io greet-effect)  ; Actually prints

;; Use eff macro for direct-style composition
(run-io
  (eff
    (io/print "Enter name: ")
    (name <- (io/read-line))
    (io/print (string-append "Hello, " name "!"))))

;; Pure letrec works via Y combinator
(letrec ((fact (lambda (n) (if (= n 0) 1 (* n (fact (- n 1)))))))
  (fact 5))  ; => 120

;; Mutual recursion also works
(letrec ((even? (lambda (n) (if (= n 0) #t (odd? (- n 1)))))
         (odd? (lambda (n) (if (= n 0) #f (even? (- n 1))))))
  (even? 10))  ; => #t

;; State effect handler - pure functional state
(run-state 0
  (eff
    (s <- (state/get))
    (state/put (+ s 1))
    (io/pure s)))
; => (0 . 1) - returned 0, final state is 1

;; Error effect handler - pure error handling
(run-error
  (eff
    (x <- (io/pure 10))
    (if (> x 5) (error/raise 'too-big) (io/pure x))))
; => (error too-big)

(try-error
  (error/raise 'oops)
  (lambda (e) (io/pure 0)))  ; Handler returns 0 on error
; => Returns (io/pure 0)

;; Delimited continuations with reset/shift
(reset (+ 1 (shift k (k 10))))
; => 11  (k captures (+ 1 [hole]), so (k 10) = (+ 1 10) = 11)

;; Continuations are composable - can call k multiple times
(reset (+ 1 (shift k (k (k 10)))))
; => 12  ((k 10) = 11, (k 11) = 12)

;; Capturing nested computations
(reset (* 2 (+ 1 (shift k (k 5)))))
; => 12  (k captures (* 2 (+ 1 [hole])), so (k 5) = (* 2 (+ 1 5)) = 12)
```

## Table of Contents

1. [Design Overview](#design-overview)
2. [Phase 1: Referential Transparency](#phase-1-referential-transparency)
3. [Phase 2: Effects as Values](#phase-2-effects-as-values)
4. [Phase 3: CPS-Based Control with Delimited Continuations](#phase-3-cps-based-control-with-delimited-continuations)
5. [Phase 4: Effect Handlers as Typed CPS Macros](#phase-4-effect-handlers-as-typed-cps-macros)
6. [Phase 5: Direct-Style Surface Syntax](#phase-5-direct-style-surface-syntax)
7. [Phase 6: IO System Design](#phase-6-io-system-design)
8. [Justification: Preserving Purity](#justification-preserving-purity)

---

## Design Overview

### Core Principles

1. **Referential Transparency**: `(f x)` always yields the same meaning—expressions can be freely substituted without changing program behavior.

2. **Effect Prohibition**: Expressions cannot directly observe time, randomness, input, or perform mutation. Side effects are impossible at the expression level.

3. **Effects as Descriptions**: Effects are first-class values that describe computations rather than execute them. An IO action is data that represents "what to do," not the doing itself.

4. **CPS Foundation**: Control flow is implemented using continuation-passing style (CPS), providing a principled foundation for effects without breaking purity.

5. **Linear, Delimited, Typed Continuations**: Continuations are:
   - **Delimited**: Bounded by explicit prompt/handler boundaries
   - **Linear**: Used exactly once—no arbitrary storage or reuse
   - **Typed**: Effect types track which operations a computation may perform

---

## Phase 1: Referential Transparency

### Goal

Establish a pure core language where expressions have no observable side effects.

### Changes from Current Grift

| Current Feature | Pure Grift Equivalent |
|-----------------|----------------------|
| `set!` | Removed from pure expressions; only in effect handlers |
| `set-car!`, `set-cdr!` | Removed; lists are immutable |
| `display`, `newline` | Become effect descriptions: `(io/print "hello")` |
| `gc`, `gc-enable` | Relegated to runtime control, not user expressions |

### Language Restrictions

```scheme
;; FORBIDDEN in pure expressions:
(set! x 5)              ; Mutation
(display "hello")       ; Direct IO
(current-time)          ; Observing time
(random 100)            ; Non-determinism

;; ALLOWED:
(define x 5)            ; Binding (not mutation)
(let ((x 1)) (+ x 2))   ; Local binding
(lambda (x) (* x x))    ; Pure functions
```

### Immutable Data Structures

All built-in data structures become immutable:

```scheme
;; Lists are immutable
(define xs '(1 2 3))
(cons 0 xs)             ; => (0 1 2 3) - returns NEW list
xs                      ; => (1 2 3) - original unchanged

;; "Modification" returns new values
(define (list-set lst idx val)
  (if (= idx 0)
      (cons val (cdr lst))
      (cons (car lst) (list-set (cdr lst) (- idx 1) val))))
```

### Semantic Guarantee

For any expression `e` that type-checks as pure:
- Evaluating `e` multiple times yields identical results
- `e` can be memoized, reordered, or eliminated without changing behavior
- `(let ((x e)) (f x x))` equals `(f e e)` for any pure `f` and `e`

---

## Phase 2: Effects as Values

### Goal

Represent effects as inert data structures that describe operations without performing them.

### Effect Representation

Effects are tagged values describing operations:

```scheme
;; IO effects as values
(define print-hello (io/print "Hello, World!"))
;; print-hello is a VALUE, not an action
;; Type: (Effect IO Unit)

(define read-name (io/read-line))
;; Type: (Effect IO String)

;; Effects compose into larger effect descriptions
(define greet
  (io/bind read-name
    (lambda (name)
      (io/print (string-append "Hello, " name)))))
;; Type: (Effect IO Unit)
```

### Effect Constructors

```scheme
;; Primitive effect constructors
(io/pure value)           ; Lift pure value into effect context
(io/print string)         ; Describe printing
(io/read-line)            ; Describe reading
(io/bind effect fn)       ; Sequence effects

;; Example: describe a conversation
(define conversation
  (io/bind (io/print "What is your name?")
    (lambda (_)
      (io/bind (io/read-line)
        (lambda (name)
          (io/print (string-append "Nice to meet you, " name "!")))))))
```

### Why Effects as Values Preserves Purity

1. **Creating** an effect value is pure—it's just data construction
2. **Composing** effects is pure—`io/bind` builds a larger description
3. **Executing** effects happens only at the program boundary, outside pure code

```scheme
;; This is a PURE function - it returns an effect description
(define (make-greeter name)
  (io/print (string-append "Hello, " name)))

;; These two expressions are equivalent (referential transparency):
(define greet-alice (make-greeter "Alice"))
(define greet-alice2 (make-greeter "Alice"))
;; greet-alice and greet-alice2 are equal values
```

---

## Phase 3: CPS-Based Control with Delimited Continuations

### Goal

Implement control flow using continuation-passing style with delimited, linear, typed continuations.

### CPS Foundation

Under the hood, all effectful computations are in CPS:

```scheme
;; Surface syntax:
(io/bind (io/read-line)
  (lambda (x) (io/print x)))

;; CPS representation:
(lambda (k)
  (read-line-cps
    (lambda (x)
      (print-cps x k))))
```

### Delimited Continuations

Continuations are bounded by `reset`/`shift` (or `prompt`/`control`) constructs:

```scheme
;; reset establishes a continuation boundary
;; shift captures up to the nearest reset

(reset
  (+ 1 (shift k (k (k 5)))))
;; k = (lambda (v) (+ 1 v))
;; (k (k 5)) = (+ 1 (+ 1 5)) = 7
```

### Continuation Properties

#### 1. Composable (Multiple Use Allowed)

Unlike full `call/cc` continuations, delimited continuations are composable and can be invoked multiple times:

```scheme
;; ALLOWED: using continuation multiple times
(reset
  (+ 1 (shift k (k (k 5)))))
;; k = (lambda (v) (+ 1 v))
;; (k (k 5)) = (k 6) = 7
```

#### 2. Bounded by Reset

Continuations only capture up to the nearest `reset`:

```scheme
(+ 100 (reset (+ 1 (shift k (k 5)))))
;; k only captures (+ 1 [hole]), not the outer (+ 100 ...)
;; Result: (+ 100 (+ 1 5)) = 106
```

#### 3. Not Stored Across Reset Boundaries

Since `set!` is removed, continuations cannot be stored globally:

```scheme
;; This is not possible since set! is removed:
;; (define stored-k #f)
;; (reset (shift k (set! stored-k k)))

;; ALLOWED: immediate use within shift body
(reset
  (shift k 
    (+ 10 (k 5))))  ; k used within shift body
```

#### 4. Why call/cc is Removed

Full `call/cc` allows unbounded continuations that can escape and be stored, breaking:
- Purity (stored continuations create implicit state)
- Local reasoning (effects can jump anywhere)
- Composability (unbounded continuations don't compose well)

Delimited continuations (`reset`/`shift`) are:
- Composable (can be called multiple times)
- Local (bounded by reset)
- Well-typed (can track effects statically)

### Implementation in Arena

Continuations are represented as arena-allocated frames:

```rust
enum ContFrame {
    // Delimited continuation frame
    Prompt {
        handler: ArenaIndex,  // Effect handler
        env: ArenaIndex,      // Lexical environment
        marks: ArenaIndex,    // Hygiene marks
    },
    // Application continuation
    Apply {
        remaining_args: ArenaIndex,
        evaluated_args: ArenaIndex,
        cont: ArenaIndex,
    },
    // ... other frame types
}
```

---

## Phase 4: Effect Handlers as Typed CPS Macros

### Goal

Effect handlers interpret, resume, or transform computations by acting as typed CPS macros.

### Effect Handler Structure

```scheme
;; Handler definition
(define-effect-handler io-handler
  ;; Handle print effect
  ((io/print msg k)
   (runtime-print! msg)      ; Actual side effect (in handler only!)
   (resume k (void)))        ; Continue computation
  
  ;; Handle read-line effect
  ((io/read-line k)
   (let ((input (runtime-read!)))
     (resume k input)))
  
  ;; Handle pure values
  ((pure v) v))
```

### Handlers as CPS Transformers

Effect handlers transform computations by:

1. **Intercepting** effect operations (matching on effect tags)
2. **Performing** actual side effects (isolated within handler)
3. **Resuming** computation via the captured continuation

```scheme
;; Running an effect through a handler
(run-with-handler io-handler
  (io/bind (io/print "Name?")
    (lambda (_)
      (io/bind (io/read-line)
        (lambda (name)
          (io/pure (string-append "Got: " name)))))))
```

### Handler Composition

Handlers can be layered for multiple effect types:

```scheme
;; State effect handler
(define-effect-handler state-handler
  ((state/get k)
   (lambda (s) ((resume k s) s)))
  ((state/put new-s k)
   (lambda (_) ((resume k (void)) new-s)))
  ((pure v)
   (lambda (s) v)))

;; Compose handlers
(run-with-handler io-handler
  (run-with-handler (state-handler 0)
    my-stateful-io-computation))
```

### Type Safety

Effect types ensure handlers match effects:

```scheme
;; Effect type annotation (conceptual)
;; (define greet : (Effect (IO + State Int) String) ...)

;; Handler must cover all effects:
;; io-handler covers IO
;; state-handler covers (State Int)
;; Together they handle (IO + State Int)
```

---

## Phase 5: Direct-Style Surface Syntax

### Goal

Provide ergonomic direct-style syntax that compiles to CPS with static effect tracking.

### The `do` Notation

```scheme
;; Direct-style with implicit effects
(define (greet-user)
  (do
    (io/print "What is your name?")
    (name <- io/read-line)
    (io/print (string-append "Hello, " name "!"))
    (io/pure name)))

;; Equivalent desugaring to explicit bind:
(io/bind (io/print "What is your name?")
  (lambda (_)
    (io/bind (io/read-line)
      (lambda (name)
        (io/bind (io/print (string-append "Hello, " name "!"))
          (lambda (_)
            (io/pure name)))))))
```

### Macro Implementation

The `do` macro transforms direct-style to CPS:

```scheme
(define-syntax do
  (syntax-rules (<-)
    ;; Base case: single expression
    ((do expr)
     expr)
    
    ;; Bind with name
    ((do (name <- expr) rest ...)
     (io/bind expr (lambda (name) (do rest ...))))
    
    ;; Bind without name (discard result)
    ((do expr rest ...)
     (io/bind expr (lambda (_) (do rest ...))))))
```

### Implicit Control Flow

Control flow constructs work within the effect system:

```scheme
;; Conditional effects
(define (maybe-greet should-greet)
  (if should-greet
      (io/print "Hello!")
      (io/pure (void))))

;; Looping with effects
(define (count-down n)
  (do
    (if (= n 0)
        (io/pure 'done)
        (do
          (io/print (number->string n))
          (count-down (- n 1))))))
```

### Static Effect Tracking

The type system tracks effects statically:

```scheme
;; Inferred: (-> Bool (Effect IO Unit))
(define (maybe-print flag msg)
  (if flag
      (io/print msg)
      (io/pure (void))))

;; Pure functions have no effects
;; Inferred: (-> Int Int Int)
(define (add x y) (+ x y))

;; Effect mismatch caught at compile time
;; ERROR: Expected pure, got (Effect IO Unit)
(define result (+ 1 (io/print "oops")))
```

---

## Phase 6: IO System Design

### Goal

Implement a complete IO system using effect-typed continuations.

### IO Effect Primitives

```scheme
;; Console IO
(io/print string)              ; Print to stdout
(io/print-err string)          ; Print to stderr
(io/read-line)                 ; Read line from stdin
(io/read-char)                 ; Read single character

;; Pure lifting
(io/pure value)                ; Lift pure value

;; Sequencing
(io/bind effect fn)            ; Sequence effects
(io/then effect1 effect2)      ; Sequence, discarding first result
```

### Program Entry Point

The `main` function returns an IO effect:

```scheme
;; Program entry point
(define main
  (do
    (io/print "Starting program...")
    (name <- io/read-line)
    (io/print (string-append "Goodbye, " name "!"))
    (io/pure 0)))  ; Exit code

;; Runtime executes the effect description
;; (run-io main) performs actual IO
```

### Error Handling

Effects compose with error handling:

```scheme
;; Error effect type
(define (safe-divide x y)
  (if (= y 0)
      (error/raise 'division-by-zero)
      (pure (/ x y))))

;; Handler for errors
(define-effect-handler error-handler
  ((error/raise e k)
   (error/pure (list 'error e)))  ; Don't resume, return error
  ((error/pure v k)
   (list 'ok v)))
```

### Combining Effect Types

Multiple effects compose cleanly:

```scheme
;; Combined IO + Error + State
(define (interactive-calculator)
  (do
    (io/print "Enter first number: ")
    (x-str <- io/read-line)
    (x <- (parse-int x-str))  ; May raise error
    (io/print "Enter second number: ")
    (y-str <- io/read-line)
    (y <- (parse-int y-str))  ; May raise error
    (result <- (safe-divide x y))  ; May raise error
    (history <- state/get)
    (state/put (cons (list x '/ y '= result) history))
    (io/print (string-append "Result: " (number->string result)))
    (io/pure result)))
```

---

## Justification: Preserving Purity

### How This Design Maintains Referential Transparency

1. **Effect values are inert data**: `(io/print "x")` creates a value, not output. Creating it twice yields equal values.

2. **Side effects isolated in handlers**: Actual mutation/IO occurs only inside effect handlers, which run at the program boundary.

3. **No observable time/state in expressions**: Pure expressions cannot observe the passage of time, system state, or randomness.

4. **Linear continuations prevent time travel**: Without reusing continuations, programs cannot "replay" effects or observe different outcomes.

### How Expressive Control Flow Is Enabled

1. **CPS provides full control**: Any control pattern (exceptions, coroutines, generators) can be expressed in CPS.

2. **Delimited continuations are composable**: Unlike `call/cc`, delimited continuations compose predictably.

3. **Effect handlers abstract patterns**: Common control patterns (try/catch, state, iteration) are library-definable.

### Why Linearity Is Essential

Without linear continuations, purity breaks:

```scheme
;; If continuations were reusable (FORBIDDEN in this design):
(define (dangerous-fn)
  (let ((x (shift k
             (begin
               (do-io!)        ; Side effect!
               (+ (k 1) (k 2))))))  ; k used twice
    x))
;; Each (k n) would re-run code after the shift
;; The second call would observe the effect from the first!
```

Linear use ensures each continuation runs fresh, with no accumulated state.

### Why Delimited Continuations Are Essential

Full `call/cc` breaks modularity:

```scheme
;; With call/cc (problematic):
(define k-escape #f)
(define (library-fn f)
  (f 42))  ; Caller could capture THIS continuation!

;; With delimited continuations (safe):
(define (library-fn f)
  (reset (f 42)))  ; Continuation capture bounded to this call
```

Delimited continuations prevent cross-module continuation capture.

### Why Static Effect Types Matter

Static tracking catches effect errors early:

```scheme
;; Without types: runtime crash when IO handler missing
;; With types: compile-time error "Unhandled effect: IO"

(define (pure-fn x)
  (if (> x 0)
      x
      (io/print "negative!")))  ; TYPE ERROR: effect in pure context
```

---

## Summary

This design evolves Grift into a pure functional language by:

| Aspect | Mechanism |
|--------|-----------|
| Purity | Effects as values, not actions |
| Control | CPS with delimited continuations |
| Safety | Linear, typed continuations |
| Ergonomics | Direct-style syntax via macros |
| IO | Effect handlers interpret descriptions |

The result is a language where:
- Every expression is referentially transparent
- Effects are explicit, composable, and type-safe
- Full control flow expressiveness is available
- Purity and practicality coexist

---

## References

1. Plotkin, G. & Pretnar, M. (2009). "Handlers of Algebraic Effects." *ESOP 2009, LNCS 5502*, pp. 80-94.
2. Kiselyov, O. & Ishii, H. (2015). "Freer Monads, More Extensible Effects." *Haskell Symposium 2015*, pp. 94-105.
3. Leijen, D. (2017). "Type Directed Compilation of Row-Typed Algebraic Effects." *POPL 2017*, pp. 486-499.
4. Dybvig, R.K., Peyton Jones, S. & Sabry, A. (2007). "A Monadic Framework for Delimited Continuations." *Journal of Functional Programming*, 17(6), pp. 687-730.
5. Danvy, O. & Filinski, A. (1990). "Abstracting Control." *LISP and Functional Programming 1990*, pp. 151-160.
