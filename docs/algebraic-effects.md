# Algebraic Effects and Handlers for Grift

This document describes the design for adding algebraic effects and handlers
to Grift, a pure lazy Lisp built on a `no_std` arena allocator. The goal is
to give programs a principled way to express side effects (I/O, exceptions,
non-determinism, state, etc.) without sacrificing referential transparency.

## Motivation

With the removal of `set!`, Grift programs are referentially transparent: the
same expression always evaluates to the same value in the same environment.
This is a strong guarantee, but programs still need to interact with the
outside world. Algebraic effects provide a structured mechanism:

* **Effects** describe *what* a program wants to do (e.g., "read a line",
  "write a string", "throw an error").
* **Handlers** describe *how* those effects are interpreted (e.g., "read from
  stdin", "collect output into a list", "convert errors to default values").

Because effects and handlers are first-class values in the language, programs
can be tested, composed, and refactored without changing their core logic.

## Core Concepts

### Effect Declaration

An **effect** is a named operation with a parameter type and a return type.
In Grift, effects are declared with `define-effect`:

```scheme
(define-effect ask  ()   number)   ; no parameter, returns a number
(define-effect tell (val) ())      ; takes a value, returns nothing
(define-effect fail (msg) _)       ; takes a message, never returns normally
```

Each `define-effect` form introduces a global procedure with the same name.
Calling `(ask)` does not execute any I/O directly — it *performs* an effect,
suspending the current computation until a handler decides what value to
resume with.

### Performing Effects

An effect is performed by calling the procedure that `define-effect` created:

```scheme
(define (greet)
  (tell "What is your name?")
  (define name (ask))
  (tell (string-append "Hello, " name "!")))
```

Evaluating `(greet)` without an enclosing handler is an error, just as
evaluating `(car 5)` is a type error.

### Handling Effects

A **handler** intercepts effect operations. It wraps a computation and
provides clauses for each effect it handles, plus a `return` clause for the
final value:

```scheme
(handle (greet)
  (return (v) v)                        ; when greet finishes, return its value
  (ask () (resume "World"))             ; resume the ask with "World"
  (tell (msg) (begin (display msg) (resume '()))))  ; print, then resume
```

Key points:

* `resume` is a one-shot continuation that returns the given value to the
  point where the effect was performed.
* The handler can choose *not* to call `resume`, short-circuiting the
  computation (useful for exceptions / `fail`).
* Handlers compose: an inner handler can re-perform an effect it doesn't
  handle, letting an outer handler catch it.

## Implementation Strategy

### Arena Representation

Effects and handlers live in the arena alongside other values. The following
new `Value` variants are needed:

| Variant | Fields | Purpose |
|---------|--------|---------|
| `Effect` | `name: ArenaIndex` | Declared effect tag |
| `Handler` | `clauses: ArenaIndex, body: ArenaIndex, env: ArenaIndex` | A `handle` form |
| `Continuation` | `frames: ArenaIndex, handler: ArenaIndex` | Captured one-shot continuation |

All fields are `ArenaIndex` values (arena pointers), so the `Copy` trait is
preserved. No heap allocation is required.

### Evaluation Changes

The evaluator's main loop gains two new special forms:

1. **`define-effect`** — Interns the effect name and binds a procedure in the
   global environment that, when called, constructs a `Value::Effect` and
   triggers effect dispatch.

2. **`handle`** — Pushes a handler frame onto a handler stack (an arena-based
   linked list, analogous to the existing GC root stack). Evaluates the body
   expression. On normal completion, invokes the `return` clause. On effect
   performance, searches the handler stack for a matching clause, captures a
   one-shot continuation, and invokes the clause with `resume` bound to that
   continuation.

### Effect Dispatch (Trampolined)

Effect dispatch integrates with the existing TCO trampoline:

```
eval_loop:
    match form {
        ...
        Value::Effect { name, arg } =>
            // walk handler stack for a clause matching `name`
            // capture continuation frames up to the handler
            // set expr = handler clause body, env = handler env + bindings
            // continue eval_loop
        ...
    }
```

Because the trampoline already converts recursive `eval` calls into a loop,
effect dispatch is just another `TailAction::Continue` case: the evaluator
updates `expr` and `env` and re-enters the loop. No additional Rust stack
frames are consumed.

### One-Shot Continuations

Continuations are **one-shot**: calling `resume` more than once is a runtime
error. This is intentional:

* Multi-shot continuations require copying arena-allocated continuation
  frames, which conflicts with the `no_alloc` constraint.
* One-shot continuations are sufficient for exceptions, state, async/await,
  and most practical effect patterns.
* The restriction keeps the implementation simple and predictable.

A continuation is represented as a linked list of evaluation frames
(analogous to a call stack snapshot) stored in the arena. When `resume` is
called, the evaluator restores those frames and continues evaluation.

### Handler Stack

The handler stack is a linked list of handler frames in the arena:

```
handler_stack: ArenaIndex  // points to (handler_frame . rest)
```

Each handler frame is a cons cell containing:

```
(clauses . (saved_env . parent_handler))
```

When `handle` is entered, a new frame is pushed. When `handle` exits
(normally or via an effect), the frame is popped. This mirrors the existing
`gc_roots` linked-list pattern in `Evaluator`.

### Interaction with Laziness

Grift uses call-by-need evaluation. Effects interact with laziness as
follows:

* **Thunks that perform effects** — A thunk `(delay (ask))` captures the
  effect. When forced, the effect is performed at that point. This is
  consistent with Haskell's `unsafePerformIO` semantics but made safe by
  the handler discipline: the effect is only meaningful inside a `handle`.

* **Memoization** — Once a thunk is forced and its effect is handled, the
  result is memoized. Subsequent forces return the memoized value without
  re-performing the effect. This preserves the at-most-once evaluation
  guarantee.

* **Handler scope** — A handler's scope is determined by the dynamic extent
  of the `handle` form, not by the lexical scope of thunks created inside
  it. If a thunk escapes the handler and is forced later, the effect will
  be dispatched to whatever handler is active at the force site.

## Example: Pure State via Effects

State can be modeled purely using effects:

```scheme
(define-effect get ()    number)
(define-effect put (val) ())

(define (stateful-computation)
  (define current (get))
  (put (+ current 1))
  (get))

;; Run with an initial state of 0:
(define (run-state init body)
  (define (loop state thunk)
    (handle (thunk)
      (return (v) (cons v state))
      (get ()     (resume state))
      (put (s)    (loop s (lambda () (resume '()))))))
  (loop init body))

(run-state 0 stateful-computation)  ;; => (1 . 1)
```

The `run-state` handler threads state through the computation without any
mutation. Each `get` resumes with the current state, and each `put` recurses
with the new state.

## Example: Exception Handling

```scheme
(define-effect raise (msg) _)

(define (safe-div a b)
  (if (= b 0)
    (raise "division by zero")
    (/ a b)))

(handle (safe-div 10 0)
  (return (v)    v)
  (raise  (msg)  (cons 'error msg)))
;; => (error . "division by zero")
```

## Example: Non-Determinism

```scheme
(define-effect choose (options) _)

(define (pythagorean-triples limit)
  (define a (choose (range 1 limit)))
  (define b (choose (range a limit)))
  (define c (choose (range b limit)))
  (if (= (+ (* a a) (* b b)) (* c c))
    (list a b c)
    (raise "not a triple")))
```

A handler for `choose` can collect all results, return the first, etc.

## Implementation Phases

### Phase 1: Core Infrastructure
- Add `Effect`, `Handler`, and `Continuation` variants to `Value`
- Add handler stack to `Evaluator`
- Implement `define-effect` special form
- Implement `handle` special form
- Implement `resume` in handler clauses

### Phase 2: Standard Effect Library
- `(scheme effects)` library with `raise`, `guard` (R7RS-compatible exception
  handling built on effects)
- `(scheme state)` library with `get`, `put`, `run-state`
- `(scheme io)` library with `read-char`, `write-char`, `read-line`,
  `display` modeled as effects

### Phase 3: Optimizations
- Effect-free code paths skip handler stack checks (fast path)
- Inline small handlers to eliminate continuation capture
- Tail-resumptive optimization: when a handler clause's last action is
  `(resume val)`, skip continuation capture entirely

## Design Constraints

All implementation choices respect Grift's constraints:

| Constraint | How Effects Comply |
|------------|-------------------|
| `no_std` | All data lives in the arena; no OS/heap dependency |
| `no_alloc` | Continuation frames are arena-allocated cons cells |
| `no unsafe` | All operations use safe Rust; no pointer tricks |
| `Copy` values | `Effect`, `Handler`, `Continuation` are `Copy` (only `ArenaIndex` fields) |
| Referential transparency | Effects are values; handlers give them meaning |
| Tail-call optimization | Effect dispatch integrates with the existing trampoline |

## References

* Plotkin, G. & Pretnar, M. (2009). *Handlers of Algebraic Effects*.
* Kammar, O., Lindley, S., & Oury, N. (2013). *Handlers in Action*.
* Hillerström, D. & Lindley, S. (2016). *Liberating Effects with Rows and Handlers*.
* Leijen, D. (2017). *Type Directed Compilation of Row-Typed Algebraic Effects* (Koka language).
* Brachthäuser, J., Schuster, P., & Ostermann, K. (2020). *Effects as Capabilities*.
