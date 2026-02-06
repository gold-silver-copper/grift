# Fixing Syntax Object Scope Tracking

This document describes the analysis and fix for tests 6.2 and 7.1 in `syntax_proper_tests.rs`, which test advanced lexical scoping of syntax objects in hygienic macros.

## Background: R6RS Hygienic Macros

R6RS specifies that macro systems must maintain **hygiene**: macro-introduced bindings should not accidentally capture user bindings, and user bindings should not accidentally shadow macro-defined bindings. This is achieved through:

1. **Syntax Objects**: Identifiers wrapped with lexical context information
2. **Marks**: Track which macro expansion introduced an identifier
3. **Lexical Environment Capture**: Syntax objects remember where they were created

The key insight is that `(syntax x)` inside a macro should create a syntax object that "knows" which `x` binding it refers to based on the lexical context at the point where `(syntax x)` is evaluated.

## Test 6.2: `free-identifier=?` with Different Lexical Contexts (FIXED)

### Test Description

```scheme
(define-syntax test-free-id-eq
  (lambda (stx)
    (syntax-case stx ()
      ((kw x)
       (let ((user-x (syntax x)))      ; user-x refers to pattern-bound x
         (let ((x 999))                 ; SHADOWS pattern x with a new local x
           (let ((macro-x (syntax x)))  ; macro-x should refer to LOCAL x (999)
             (if (free-identifier=? user-x macro-x)
                 (syntax #t)
                 (syntax #f)))))))))

(let ((x 111)) (test-free-id-eq x))  ; Should return #f
```

### Expected Behavior

- `user-x` is created by `(syntax x)` when `x` refers to the **pattern variable** from `syntax-case`
- After `(let ((x 999)) ...)`, the identifier `x` refers to the **local binding** with value 999
- `macro-x` is created by `(syntax x)` when `x` refers to this **local binding**
- `free-identifier=?` should return `#f` because they refer to different bindings

### Root Cause Analysis

In `transcribe_symbol_with_env()` (expand.rs), the code checked for pattern variables FIRST:

```rust
// 1. Check if it's a pattern variable
if let Some(val) = self.bindings_lookup(bindings, sym)? {
    return Ok(val);
}
```

This always substituted a pattern variable, even when a local binding shadows it.

### The Fix

Modified `transcribe_symbol_with_env()` to check local bindings BEFORE pattern variables. When both a local binding and a pattern variable exist for the same name, we compare them to determine if the local binding shadows the pattern variable. If they're different, the local binding wins.

The key insight: local bindings from `let`, `lambda`, etc. within the macro transformer should shadow pattern variables when creating syntax objects via `(syntax ...)`.

## Test 7.1: Syntax Objects with Mutation (NOT FIXED - DEFERRED)

### Test Description

```scheme
(define box #f)

(define-syntax capture
  (lambda (stx)
    (syntax-case stx ()
      ((kw val)
       (begin
         (set! box (syntax val))  ; Capture syntax object of pattern variable
         (syntax #f))))))

(let ((x 333)) (capture x))  ; box now holds syntax object for x

(define-syntax use-captured
  (lambda (_) box))  ; Return the captured syntax object

(let ((x 444)) (use-captured))  ; Should evaluate to 333, not 444
```

### Expected Behavior

- When `(capture x)` is called inside `(let ((x 333)) ...)`, the pattern variable `val` is bound to `x`
- `(syntax val)` should create a syntax object that remembers `x` refers to the binding with value 333
- Later, `(use-captured)` returns this syntax object
- When evaluated in `(let ((x 444)) ...)`, it should still refer to the original binding (333)

### Root Cause Analysis

Pattern variables are bound to **raw symbols** from the macro input, not syntax objects with lexical context. When `syntax-case` matches `(capture x)`, the pattern variable `val` gets bound to the plain symbol `x`, not a syntax object wrapping `x` with the call-site environment.

### Why This Is Deferred

Fixing test 7.1 requires **passing the macro call-site environment through to syntax-case pattern matching**. This is non-trivial because:

1. **Wrapping the entire macro input breaks keyword matching**: Macros like `let`, `if`, etc. pattern-match against literal keywords. If we wrap `let` in a syntax object, the pattern `((let bindings ...) ...)` fails to match.

2. **Selective wrapping is complex**: We would need to wrap only the "data" parts of the macro input (identifiers that could be bound), not the "structure" parts (keywords, literal symbols in patterns).

3. **Historical context**: The comment in `step_eval_list` notes that wrapping macro inputs previously caused issues with `set!` and `define` when the macro produces code referencing the same identifiers.

### Possible Future Solutions

1. **Two-layer input representation**: Pass macro inputs as a structure that has both:
   - The raw expression (for pattern matching structure)
   - Lexical context metadata (for identifier resolution)

2. **Post-matching wrapping**: After pattern matching, wrap symbol values in the bindings with the call-site environment.

3. **Lazy context capture**: Record that a pattern variable came from macro input, and add context when `(syntax val)` is called.

Each solution has trade-offs in complexity, performance, and compatibility with existing code.

## Testing

After implementing the fix for test 6.2:

```bash
# Test 6.2 now passes
cargo test --package grift_eval --test syntax_proper_tests test_6_2_free_identifier_eq

# Full test suite passes
cargo test --workspace
```

Test 7.1 remains ignored with a clear explanation of what would be needed to fix it.

## References

- R6RS Chapter 11 (Syntax-case)
- "Macros that Work" (Clinger & Rees, 1991)
- psyntax implementation (Dybvig, Hieb, Bruggeman)
- grift docs/SCHEME_R7RS_CONFORMANCE.md
