# R7RS Exception System Implementation

## Reference Specification

The authoritative R7RS specification is located at:
- **`scheme-spec-r7rs/spec.html`** - Section 6.11 (Exceptions) and Section 4.2.7 (Exception handling syntax)

## Goal

Implement R7RS-compliant exception handling where exceptions are first-class Scheme values, handlers are dynamically scoped procedures, and error objects are proper inspectable Scheme objects.

## Design Constraints

### Value Enum Constraint

**IMPORTANT**: The `Value` enum should never inline more than two `ArenaIndex` fields per variant. This constraint exists for:
- Memory efficiency (keeping enum size bounded)
- Consistent slot sizing in the arena
- Cache-friendly access patterns

Current examples that follow this rule:
- `Cons { car: ArenaIndex, cdr: ArenaIndex }` - 2 indices ✓
- `Lambda { params: ArenaIndex, body_env: ArenaIndex }` - 2 indices ✓
- `Array { len: usize, data: ArenaIndex }` - 1 index + 1 usize ✓

For error objects, use indirection via cons cells if you need more than 2 references.

### no_std / no_alloc Constraints

- Fixed-size handler stack (suggest 16 max depth)
- All objects arena-allocated
- No heap allocations

## Required Procedures (Section 6.11)

### Core Exception Procedures

| Procedure | Signature | Description |
|-----------|-----------|-------------|
| `with-exception-handler` | `(with-exception-handler handler thunk)` | Install handler for thunk's dynamic extent |
| `raise` | `(raise obj)` | Raise non-continuable exception |
| `raise-continuable` | `(raise-continuable obj)` | Raise continuable exception |
| `error` | `(error message obj ...)` | Create error object and raise it |

### Error Object Inspection

| Procedure | Signature | Description |
|-----------|-----------|-------------|
| `error-object?` | `(error-object? obj)` | Returns `#t` if obj is an error object |
| `error-object-message` | `(error-object-message error-obj)` | Extract message string |
| `error-object-irritants` | `(error-object-irritants error-obj)` | Extract irritants list |
| `read-error?` | `(read-error? obj)` | Returns `#t` if obj is a read error |
| `file-error?` | `(file-error? obj)` | Returns `#t` if obj is a file error |

### Exception Handling Syntax (Section 4.2.7)

```scheme
(guard (⟨variable⟩ ⟨cond clause₁⟩ ⟨cond clause₂⟩ …)
  ⟨body⟩)
```

The body is evaluated with an exception handler that binds the raised object to `⟨variable⟩` and evaluates the clauses like `cond`. If no clause matches and there's no `else`, re-raises with `raise-continuable`.

## Implementation Plan

### Phase 1: Error Objects as First-Class Values

1. Add `Value::ErrorObject` variant (respecting 2-index max constraint):
   ```rust
   /// Error object with message and irritants
   /// Uses indirection: data points to cons (message . irritants)
   ErrorObject { kind: ErrorObjectKind, data: ArenaIndex }
   ```

2. Add `ErrorObjectKind` enum:
   ```rust
   #[derive(Clone, Copy, Debug, PartialEq)]
   pub enum ErrorObjectKind {
       User,      // Created by (error ...)
       Read,      // Read/parse errors
       File,      // File I/O errors
       Internal,  // Implementation errors
   }
   ```

3. Add helper methods to `Lisp`:
   ```rust
   fn make_error_object(&self, kind: ErrorObjectKind, message: ArenaIndex, irritants: ArenaIndex) -> ArenaResult<ArenaIndex>
   fn error_object_message(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex>
   fn error_object_irritants(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex>
   ```

4. Add builtins: `error-object?`, `error-object-message`, `error-object-irritants`, `read-error?`, `file-error?`

### Phase 2: Exception Handler Infrastructure

1. Add handler stack to `Evaluator`:
   ```rust
   const MAX_EXCEPTION_HANDLERS: usize = 16;
   
   struct Evaluator<'a, N> {
       // ... existing fields ...
       exception_handlers: [ArenaIndex; MAX_EXCEPTION_HANDLERS],
       handler_depth: usize,
   }
   ```

2. Implement `with-exception-handler`:
   - Push handler onto stack
   - Evaluate thunk
   - Pop handler (even on error)
   - Return thunk's result

3. Implement `raise`:
   - If no handler: convert to Rust `EvalError` 
   - Pop current handler before calling (R7RS requirement)
   - Apply handler to raised object
   - If handler returns: raise secondary exception (non-continuable)

4. Update `error` builtin:
   - Create `ErrorObject` with message + irritants
   - Call `raise` on it

### Phase 3: Continuable Exceptions

1. Implement `raise-continuable`:
   - Like `raise`, but handler can return
   - Returned value becomes result of `raise-continuable` call
   - Handler is restored after return

2. This requires careful integration with the trampoline/continuation system

### Phase 4: Guard Syntax

1. Add `guard` as special form in evaluator

2. Semantics (from R7RS):
   - Evaluate body with exception handler installed
   - Handler binds raised object to variable
   - Evaluate cond clauses with guard's continuation/environment
   - If no clause matches: `raise-continuable` the object

3. Reference implementation from spec (Appendix B):
   ```scheme
   (define-syntax guard
     (syntax-rules ()
       ((guard (var clause ...) e1 e2 ...)
        ((call/cc
          (lambda (guard-k)
            (with-exception-handler
             (lambda (condition)
               ((call/cc
                 (lambda (handler-k)
                   (guard-k
                    (lambda ()
                      (let ((var condition))
                        (guard-aux
                         (handler-k
                          (lambda ()
                            (raise-continuable condition)))
                         clause ...))))))))
             (lambda () e1 e2 ...))))))))
   ```

## Testing

### Basic Error Creation
```scheme
(error-object? (guard (e (else e)) (error "test" 1 2 3)))
;; => #t

(error-object-message (guard (e (else e)) (error "oops")))
;; => "oops"

(error-object-irritants (guard (e (else e)) (error "bad" 'a 'b)))
;; => (a b)
```

### Exception Handling
```scheme
(with-exception-handler
  (lambda (x) (display "caught: ") (display x) (newline) 42)
  (lambda () (+ 1 (raise 'an-error))))
;; prints: caught: an-error
;; Note: For non-continuable, handler must not return normally

(guard (condition
         ((assq 'a condition) => cdr)
         ((assq 'b condition)))
  (raise (list (cons 'a 42))))
;; => 42
```

### Continuable Exceptions
```scheme
(with-exception-handler
  (lambda (con)
    (cond
      ((string? con) (display con))
      (else (display "a]warning has been issued")))
    42)
  (lambda ()
    (+ (raise-continuable "should be a number") 23)))
;; prints: should be a number
;; => 65
```

## Migration Notes

- Existing code using `(error "message")` should continue to work
- Internal errors (type errors, etc.) should create appropriate error objects
- The Rust `EvalError` type remains for internal error propagation but should wrap/unwrap error objects at boundaries
