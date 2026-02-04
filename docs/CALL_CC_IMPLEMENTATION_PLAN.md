# call/cc Implementation Plan

## Overview

This document outlines a plan for implementing `call-with-current-continuation` (also known as `call/cc`) in the Grift Scheme interpreter. This is one of the most powerful and complex features in Scheme, allowing programs to capture and manipulate control flow.

## What is call/cc?

`call-with-current-continuation` captures the current continuation as a first-class value and passes it to a procedure. The continuation represents "the rest of the computation" from the point where it was captured.

### Basic Example

```scheme
;; Simple example: escape continuation
(+ 1 (call/cc (lambda (k) (+ 2 (k 3)))))
;; => 4 (not 6, because (k 3) never returns)

;; The continuation k, when called with value 3, 
;; immediately returns 3 as the result of the call/cc,
;; skipping the (+ 2 ...) computation
```

### Key Behaviors

1. **Non-local exit**: When a captured continuation is invoked, it abandons the current computation and returns to the point where the continuation was captured
2. **First-class values**: Continuations can be stored in variables, passed to functions, and called multiple times
3. **Re-entrant**: A continuation can be called multiple times, each time restarting the computation from the capture point

## Current Architecture Analysis

### Trampoline-Based Evaluation

Grift currently uses a trampolined evaluator with an explicit continuation stack:

```rust
pub struct Evaluator<'a, const N: usize> {
    lisp: &'a Lisp<N>,
    global_env: ArenaIndex,
    call_stack: [StackFrame; MAX_STACK_DEPTH],     // For error reporting
    call_stack_depth: usize,
    cont_stack: [Cont; MAX_CONT_DEPTH],            // Current continuation stack
    cont_depth: usize,
    data_stack: [ArenaIndex; MAX_DATA_STACK],      // Data for continuations
    data_stack_top: usize,
    // ...
}
```

The `Cont` enum represents different types of continuations:
- `Done` - computation complete
- `ApplyForced` - after evaluating function
- `IfBranch` - after evaluating condition
- `BuiltinForceArg` - evaluating builtin arguments
- Many others for different control flow scenarios

### Advantages of Current Design

✅ **Already uses continuations internally**: The trampoline is essentially a continuation-passing style (CPS) interpreter
✅ **Explicit stack**: The continuation stack is visible and manipulable
✅ **No Rust recursion**: All evaluation is iterative, making continuation capture feasible
✅ **Bounded stack**: Fixed-size continuation stack prevents unbounded growth

### Challenges

❌ **Continuation stack is mutable array**: Cannot easily snapshot and restore
❌ **Data stack is separate**: Need to capture both cont_stack and data_stack together
❌ **Stack indices are transient**: Continuations reference data by offset, not absolute position
❌ **No continuation values in arena**: Need to add a new `Value::Continuation` variant

## Implementation Strategy

### Phase 1: Add Continuation Value Type

**Goal**: Represent captured continuations as first-class Lisp values

#### 1.1 Extend Value Enum

Add to `crates/grift_parser/src/lib.rs`:

```rust
pub enum Value {
    // ... existing variants ...
    
    /// Captured continuation from call/cc
    /// Contains a snapshot of the continuation stack and data stack
    Continuation {
        cont_snapshot: ArenaIndex,    // Points to ContSnapshot
        data_snapshot: ArenaIndex,    // Points to array of ArenaIndex values
        cont_depth: usize,            // Number of continuations in snapshot
        data_depth: usize,            // Number of data items in snapshot
        capture_env: ArenaIndex,      // Environment at capture point
    },
}
```

#### 1.2 Create Snapshot Storage

Since the arena can only store `Copy` types, we need to store continuation snapshots as arrays:

```rust
// In evaluator
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Capture the current continuation stack as an arena value
    fn capture_continuation(&mut self) -> Result<ArenaIndex, EvalError> {
        // Allocate array for continuation stack snapshot
        // Store each Cont as a tagged enum + associated data
        // Return Continuation value with pointers to snapshots
    }
    
    /// Restore a captured continuation, replacing current stack
    fn restore_continuation(&mut self, cont: ArenaIndex, return_val: ArenaIndex) 
        -> Result<TrampolineState, EvalError> {
        // Validate continuation value
        // Clear current cont_stack and data_stack
        // Copy saved continuations back to cont_stack
        // Copy saved data back to data_stack
        // Return Continue state with return_val as the current value
    }
}
```

### Phase 2: Implement call/cc Special Form

**Goal**: Add `call-with-current-continuation` and `call/cc` as special forms

#### 2.1 Parser Recognition

Add to `crates/grift_parser/src/lib.rs` or evaluator's form recognition:

```rust
// In evaluator's step_eval or form detection
if is_symbol(func, "call-with-current-continuation") ||
   is_symbol(func, "call/cc") {
    // Handle call/cc
}
```

#### 2.2 Evaluator Implementation

Add to `crates/grift_eval/src/evaluator/forms.rs`:

```scheme
;; Semantics:
;; (call/cc proc) 
;; 1. Capture current continuation as k
;; 2. Call (proc k)
;; 3. If proc returns normally, return that value
;; 4. If k is invoked with value v, immediately return v as result of call/cc
```

```rust
// Pseudo-code for call/cc evaluation:
fn eval_call_cc(&mut self, proc_expr: ArenaIndex, env: ArenaIndex) 
    -> Result<TrampolineState, EvalError> {
    
    // Step 1: Capture current continuation
    let cont = self.capture_continuation()?;
    
    // Step 2: Push continuation to evaluate proc_expr
    // After evaluating proc, we'll apply it to the captured continuation
    self.push_cont_callcc_apply(cont, env)?;
    
    // Step 3: Evaluate the procedure expression
    Ok(TrampolineState::Continue(proc_expr, env))
}
```

#### 2.3 Add Continuation Type

```rust
// Add to Cont enum in continuation.rs:
pub enum Cont {
    // ... existing variants ...
    
    /// After evaluating procedure for call/cc, apply it to captured continuation
    /// Stack data: [captured_cont, env] (2 elements)
    CallCcApply(usize),
}
```

#### 2.4 Continuation Application

When a captured continuation is called as a function:

```rust
// In eval_apply or similar:
Value::Continuation { cont_snapshot, data_snapshot, cont_depth, data_depth, .. } => {
    // Get the argument (single argument to continuation)
    let return_val = self.lisp.car(args)?;
    
    // Validate that we got exactly one argument
    if !self.lisp.is_nil(self.lisp.cdr(args)?)? {
        return Err("continuation expects exactly one argument");
    }
    
    // Restore the continuation and return the value
    self.restore_continuation(cont, return_val)
}
```

### Phase 3: Implement dynamic-wind

**Goal**: Ensure before/after thunks are called when entering/exiting dynamic extent

`dynamic-wind` is essential for resource management with continuations:

```scheme
(dynamic-wind
  (lambda () (display "entering"))  ; before thunk
  (lambda () ...)                   ; body thunk  
  (lambda () (display "exiting")))  ; after thunk
```

When a continuation is captured or invoked, `dynamic-wind` ensures:
- After thunks are called when exiting a dynamic extent
- Before thunks are called when re-entering a dynamic extent

#### 3.1 Track Dynamic Wind Chain

```rust
pub struct Evaluator<'a, const N: usize> {
    // ... existing fields ...
    
    /// Stack of active dynamic-wind contexts
    /// Each entry: (before_thunk, after_thunk, parent_chain)
    dynamic_wind_chain: ArenaIndex,
}
```

#### 3.2 Implement dynamic-wind Special Form

```rust
fn eval_dynamic_wind(&mut self, 
                     before: ArenaIndex,
                     body: ArenaIndex, 
                     after: ArenaIndex,
                     env: ArenaIndex) -> Result<TrampolineState, EvalError> {
    // 1. Call before thunk
    // 2. Add (before, after) to dynamic_wind_chain
    // 3. Call body thunk
    // 4. Remove from dynamic_wind_chain
    // 5. Call after thunk
    // 6. Return body result
}
```

#### 3.3 Integrate with call/cc

When capturing continuation, also capture `dynamic_wind_chain`.
When restoring continuation, invoke necessary before/after thunks to transition between chains.

### Phase 4: Testing Strategy

#### 4.1 Basic Tests

```scheme
;; Test 1: Simple escape
(test-equal 3
  (+ 1 (call/cc (lambda (k) (k 2)))))

;; Test 2: Continuation doesn't escape
(test-equal 4
  (+ 1 (call/cc (lambda (k) 3))))

;; Test 3: Continuation stored and called later
(define saved-cont #f)
(test-equal 5
  (+ 1 (call/cc (lambda (k) 
                  (set! saved-cont k)
                  4))))
(test-equal 10
  (saved-cont 9))
```

#### 4.2 Advanced Tests

```scheme
;; Test 4: Re-entrant continuation
(define counter 0)
(define k-saved #f)

(test-equal 0
  (call/cc (lambda (k)
             (set! k-saved k)
             (set! counter (+ counter 1))
             (if (< counter 5)
                 (k counter)
                 counter))))
;; Calls k-saved multiple times, counter increments each time

;; Test 5: Nested call/cc
(test-equal 'outer
  (call/cc (lambda (outer)
             (call/cc (lambda (inner)
                        (outer 'outer)))
             'inner)))
```

#### 4.3 dynamic-wind Tests

```scheme
;; Test 6: Before/after thunks called
(define log '())
(dynamic-wind
  (lambda () (set! log (cons 'before log)))
  (lambda () (set! log (cons 'body log)))
  (lambda () (set! log (cons 'after log))))
(test-equal '(after body before) log)

;; Test 7: dynamic-wind with continuation escape
(define log '())
(define k-out #f)
(call/cc (lambda (k) (set! k-out k)))
(dynamic-wind
  (lambda () (set! log (cons 'before log)))
  (lambda () (k-out 'escaped))
  (lambda () (set! log (cons 'after log))))
;; after thunk should be called when escaping
```

### Phase 5: Performance Considerations

#### 5.1 Continuation Snapshot Size

Each continuation snapshot copies the entire cont_stack and data_stack. For deep continuations, this could be expensive.

**Optimization**: Use copy-on-write or incremental snapshots
- Only copy frames that have changed since last snapshot
- Use arena-allocated linked list of frames instead of array

#### 5.2 Continuation Invocation

Restoring a continuation requires clearing current stacks and copying saved stacks.

**Optimization**: Mark current continuation as "dead" and switch to saved continuation in place

#### 5.3 Memory Usage

Captured continuations can be large and long-lived, potentially causing arena exhaustion.

**Consideration**: 
- Ensure GC can trace through continuation snapshots
- Consider continuation-specific memory limits
- Document memory implications in user guide

### Phase 6: Documentation

#### 6.1 User Documentation

- Add examples to REPL help
- Document memory implications
- Provide common use cases (exceptions, backtracking, generators)

#### 6.2 Architecture Documentation

- Update LISP_ARCHITECTURE.md with continuation implementation details
- Document continuation snapshot format
- Explain dynamic-wind chain management

#### 6.3 Conformance Documentation

- Update SCHEME_R7RS_CONFORMANCE.md
- Mark call/cc and dynamic-wind as implemented
- Note any deviations from R7RS spec

## Implementation Phases Summary

| Phase | Description | Estimated Complexity | Priority |
|-------|-------------|---------------------|----------|
| 1 | Add Continuation value type | Medium | High |
| 2 | Implement call/cc special form | High | High |
| 3 | Implement dynamic-wind | High | Medium |
| 4 | Comprehensive testing | Medium | High |
| 5 | Performance optimization | Medium | Low |
| 6 | Documentation | Low | Medium |

## Alternative Approaches Considered

### 1. Copy-on-Write Continuation Stacks

Instead of copying the entire stack, use a linked list where each node shares structure with parent.

**Pros**: More efficient for deep stacks
**Cons**: More complex implementation, harder to debug

### 2. Delimited Continuations

Implement `shift`/`reset` or `prompt`/`control` instead of full call/cc.

**Pros**: More composable, easier to reason about
**Cons**: Not R7RS standard, different semantics

### 3. Stack Copying to Rust Heap

Copy continuation stacks to `Vec` when capturing (only in std mode).

**Pros**: Simpler implementation
**Cons**: Breaks no_std compatibility, two implementations needed

## Risks and Mitigations

### Risk 1: Arena Memory Exhaustion

Large continuation snapshots could exhaust arena memory quickly.

**Mitigation**: 
- Implement continuation-specific GC pressure monitoring
- Add user-configurable limits on continuation depth
- Provide clear error messages when limits exceeded

### Risk 2: Semantic Complexity

call/cc interacts with all other language features in subtle ways.

**Mitigation**:
- Comprehensive test suite covering edge cases
- Study R7RS specification carefully
- Test against reference implementations (Racket, Chez Scheme)

### Risk 3: Performance Degradation

Continuation capture/restore could slow down normal evaluation.

**Mitigation**:
- Benchmark before/after implementation
- Optimize hot paths
- Consider lazy copying strategies

## Success Criteria

- [ ] `call-with-current-continuation` and `call/cc` work as per R7RS spec
- [ ] `dynamic-wind` properly manages before/after thunks
- [ ] All standard test cases pass
- [ ] Re-entrant continuations work correctly
- [ ] Memory usage is reasonable (no more than 2x current usage for typical programs)
- [ ] Performance degradation < 10% for programs not using call/cc
- [ ] Comprehensive documentation added

## References

- [R7RS Small Specification (Section 6.10)](https://small.r7rs.org/attachment/r7rs.pdf) - Official specification for call/cc and dynamic-wind
- [SICP Section 5.5](https://mitpress.mit.edu/sites/default/files/sicp/full-text/book/book-Z-H-34.html) - Compilation and implementation of continuations
- [Three Implementation Models for Scheme](http://www.cs.indiana.edu/~dyb/pubs/3imp.pdf) - Academic paper on continuation implementation
- [Representing Control in the Presence of First-Class Continuations](https://citeseerx.ist.psu.edu/document?repid=rep1&type=pdf&doi=10.1.1.46.3653) - Continuation representation strategies

## Open Questions

1. **Should we support one-shot continuations?** Some implementations distinguish between continuations that can be called once vs. multiple times for performance.

2. **How should continuations interact with native functions?** If a continuation is captured across a native function boundary, what happens?

3. **Should we provide continuation-barrier forms?** Forms that prevent continuation capture from crossing certain boundaries (useful for FFI).

4. **Memory model for captured environments?** Should we deep-copy environments or share structure?

5. **Stack depth limits?** Should there be separate limits for call_stack, cont_stack, and continuation snapshot depth?

## Conclusion

Implementing call/cc is a significant undertaking that will require careful design and extensive testing. The current trampolined architecture provides a solid foundation, but substantial work is needed to:

1. Add continuation values to the arena
2. Implement snapshot/restore logic for continuation stacks
3. Integrate with the evaluator's control flow
4. Implement dynamic-wind for proper cleanup
5. Test exhaustively for correctness

The implementation should be done incrementally, with each phase thoroughly tested before moving to the next. Performance should be monitored throughout to ensure that programs not using call/cc are not significantly impacted.

Once implemented, call/cc will enable powerful programming patterns in Grift including:
- Exception handling (before native exception support)
- Backtracking search
- Coroutines and generators  
- Web continuation servers
- Non-deterministic programming

This feature will move Grift significantly closer to full R7RS compliance.
