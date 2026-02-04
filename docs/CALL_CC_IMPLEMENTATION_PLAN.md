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

### Challenges with Current Design

❌ **Continuation stack is mutable array**: Cannot easily snapshot and restore
❌ **Data stack is separate**: Need to capture both cont_stack and data_stack together
❌ **Stack indices are transient**: Continuations reference data by offset, not absolute position
❌ **No continuation values in arena**: Need to add a new `Value::Continuation` variant

### Proposed Arena-Based Design

**Key Insight**: We can move the entire continuation stack into the arena as a linked list, eliminating the separate cont_stack and data_stack arrays entirely.

**Benefits**:
✅ **Natural snapshotting**: Continuations are already arena values that can be captured directly
✅ **Unified storage**: No separate data_stack needed - all data is in the arena
✅ **Persistent references**: ArenaIndex references are stable, not transient stack offsets
✅ **Simpler capture/restore**: Just save/restore a single ArenaIndex to the current continuation

**Design Constraint**: Following the same pattern as `Lambda { params, body_env }` and `Cons { car, cdr }`, continuation values must store at most **two ArenaIndex-sized fields** to maintain the 24-byte Value enum size and ensure cache-friendly memory layout.

## Implementation Progress

### Phase 1 Progress Tracking

| Step | Description | Status |
|------|-------------|--------|
| 1.1 | Add `Value::ContFrame` type to Value enum | ✅ Complete |
| 1.2 | Implement `Trace` for ContFrame (GC support) | ✅ Complete |
| 1.3 | Add helper methods (`cont_frame`, `cont_frame_parts`, `cont_frame_parent`) | ✅ Complete |
| 1.4 | Add unit tests for ContFrame functionality | ✅ Complete |
| 1.5 | Migrate evaluator to use arena-based continuations | 🔲 Partial (hybrid approach) |

### Phase 2 Progress Tracking

| Step | Description | Status |
|------|-------------|--------|
| 2.1 | Add `Value::Continuation` type to Value enum | ✅ Complete |
| 2.2 | Implement `Trace` for Continuation (GC support) | ✅ Complete |
| 2.3 | Add helper methods (`continuation`, `continuation_parts`) | ✅ Complete |
| 2.4 | Add unit tests for Continuation functionality | ✅ Complete |

### Phase 3 Progress Tracking

| Step | Description | Status |
|------|-------------|--------|
| 3.1 | Add `call/cc` and `call-with-current-continuation` special forms | ✅ Complete |
| 3.2 | Implement continuation capture (`capture_continuation`) | ✅ Complete |
| 3.3 | Implement continuation restore (`restore_continuation`) | ✅ Complete |
| 3.4 | Handle continuation invocation in `ApplyForced` | ✅ Complete |
| 3.5 | Add comprehensive tests for call/cc | ✅ Complete |

### Phase 4 Progress Tracking

| Step | Description | Status |
|------|-------------|--------|
| 4.1 | Implement `dynamic-wind` | 🔲 Not started |

**Implementation Notes**:
- Phase 1.5 uses a hybrid approach: the evaluator still uses array-based `cont_stack` and `data_stack`,
  but continuations are serialized to arena-based `ContFrame` chains when captured with `call/cc`
- This hybrid approach provides correct semantics with less invasive changes to the evaluator
- Full migration to arena-based continuations can be done later as an optimization

## Implementation Strategy

### Phase 1: Migrate to Arena-Based Continuation Stack

**Goal**: Move the continuation stack from a separate array into the arena as a linked list structure

**Incremental Approach**: This phase is implemented in steps:
1. First, add the `Value::ContFrame` type to the Value enum with appropriate Trace implementation
2. Add helper methods to Lisp for creating and manipulating ContFrame values
3. Only after the type infrastructure is in place, migrate the evaluator to use arena-based continuations
4. This allows testing the new types without breaking the existing evaluator

#### 1.1 Design Arena-Based Continuation Structure

Instead of separate `cont_stack` and `data_stack` arrays, use arena-allocated continuation frames forming a linked list:

```rust
// Internal continuation frame stored in arena
// Represented as a cons-like structure in the Value enum
pub enum Value {
    // ... existing variants ...
    
    /// Continuation frame - forms a linked list in the arena
    /// Similar to Lambda, stores only 2 ArenaIndex fields.
    /// cont_data is a cons cell containing (cont_type_and_data . parent_cont)
    /// where cont_type_and_data encodes both the continuation type and its data
    ContFrame { 
        cont_data: ArenaIndex,   // Points to (cont_type_and_data . parent_cont) cons
        env: ArenaIndex,         // Environment at this continuation point
    },
}
```

The continuation chain structure:
- Each `ContFrame` points to its parent continuation via a cons cell
- The `cont_type_and_data` field encodes the continuation type (Done, ApplyForced, etc.) and any associated data
- For continuations needing more than 2 data values, use nested cons cells
- The chain terminates with a `Done` continuation

#### 1.2 Continuation Type Encoding

Since we need to encode both the continuation type and its data in the arena, use tagged values:

```rust
// Continuation type encoded as Usize in arena
// Each cont type has a unique tag value
const CONT_DONE: usize = 0;
const CONT_APPLY_FORCED: usize = 1;
const CONT_IF_BRANCH: usize = 2;
// ... etc.

// For continuations with data, structure is:
// cont_type_and_data -> (Usize(CONT_TYPE) . data_cons)
// where data_cons is a cons cell containing the continuation's data
//
// Example: IfBranch needs [then_expr, else_expr, env] (3 elements)
// cont_type_and_data -> (Usize(CONT_IF_BRANCH) . (then_expr . (else_expr . env)))
```

#### 1.3 Modify Evaluator Structure

Remove the separate stacks and use a single ArenaIndex for the current continuation:

```rust
pub struct Evaluator<'a, const N: usize> {
    lisp: &'a Lisp<N>,
    global_env: ArenaIndex,
    call_stack: [StackFrame; MAX_STACK_DEPTH],     // For error reporting
    call_stack_depth: usize,
    current_cont: ArenaIndex,                       // Current continuation (arena-based)
    // REMOVED: cont_stack, cont_depth, data_stack, data_stack_top
    // ...
}
```

#### 1.4 Continuation Operations

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Push a new continuation frame onto the arena-based stack
    fn push_cont(&mut self, cont_type: usize, data: ArenaIndex, env: ArenaIndex) 
        -> Result<(), EvalError> {
        // Create parent reference: (cont_type_and_data . old_current_cont)
        let cont_data = self.lisp.cons(
            self.lisp.cons(self.lisp.alloc(Value::Usize(cont_type))?, data)?,
            self.current_cont
        )?;
        
        // Create new ContFrame
        let new_frame = self.lisp.alloc(Value::ContFrame { 
            cont_data, 
            env 
        })?;
        
        self.current_cont = new_frame;
        Ok(())
    }
    
    /// Pop a continuation frame from the arena-based stack
    fn pop_cont(&mut self) -> Result<(usize, ArenaIndex, ArenaIndex), EvalError> {
        let frame = self.lisp.get(self.current_cont)?;
        
        match frame {
            Value::ContFrame { cont_data, env } => {
                // Extract (cont_type_and_data . parent_cont)
                let (type_and_data, parent) = self.lisp.car_cdr(cont_data)?;
                
                // Extract (cont_type . data)
                let (type_val, data) = self.lisp.car_cdr(type_and_data)?;
                
                let cont_type = if let Value::Usize(t) = self.lisp.get(type_val)? {
                    t
                } else {
                    return Err(EvalError::InvalidContinuation);
                };
                
                // Update current_cont to parent
                self.current_cont = parent;
                
                Ok((cont_type, data, env))
            }
            _ => Err(EvalError::InvalidContinuation),
        }
    }
}
```

### Phase 2: Add First-Class Continuation Value Type

**Goal**: Represent captured continuations as first-class Lisp values that can be called

#### 2.1 Extend Value Enum

Add to `crates/grift_parser/src/lib.rs`:

```rust
pub enum Value {
    // ... existing variants ...
    
    /// Captured continuation from call/cc - a first-class callable value
    /// Following the 2-index constraint like Lambda and Cons.
    /// cont_chain points to the continuation stack (ContFrame linked list)
    /// metadata is a cons cell (capture_env . dynamic_wind_chain)
    Continuation {
        cont_chain: ArenaIndex,   // Points to ContFrame linked list (or Nil for empty)
        metadata: ArenaIndex,     // Points to (capture_env . dynamic_wind_chain) cons
    },
}
```

**Design Notes**:
- `cont_chain` is the head of the continuation stack at capture time (an ArenaIndex to a `ContFrame`)
- `metadata` encodes environment and dynamic-wind state as a cons cell
- Only 2 ArenaIndex fields, matching Lambda's design ✓
- When called, this continuation restores the captured `cont_chain` as the current continuation

#### 2.2 Continuation Capture and Restore

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Capture the current continuation as a first-class value
    fn capture_continuation(&mut self) -> Result<ArenaIndex, EvalError> {
        // Create metadata cons: (capture_env . dynamic_wind_chain)
        let metadata = self.lisp.cons(
            self.global_env,          // or current env
            self.dynamic_wind_chain   // for dynamic-wind support (Phase 3)
        )?;
        
        // Create Continuation value pointing to current continuation chain
        self.lisp.alloc(Value::Continuation {
            cont_chain: self.current_cont,
            metadata,
        })
    }
    
    /// Restore a captured continuation, replacing current stack
    fn restore_continuation(&mut self, cont_idx: ArenaIndex, return_val: ArenaIndex) 
        -> Result<TrampolineState, EvalError> {
        let cont_val = self.lisp.get(cont_idx)?;
        
        match cont_val {
            Value::Continuation { cont_chain, metadata } => {
                // Extract capture_env and dynamic_wind_chain from metadata
                let (capture_env, dw_chain) = self.lisp.car_cdr(metadata)?;
                
                // TODO Phase 3: Handle dynamic-wind transitions
                // self.transition_dynamic_wind(self.dynamic_wind_chain, dw_chain)?;
                
                // Replace current continuation with captured one
                self.current_cont = cont_chain;
                
                // Return the value as the result of the call/cc
                Ok(TrampolineState::Continue(return_val, capture_env))
            }
            _ => Err(EvalError::NotAContinuation),
        }
    }
}

### Phase 3: Implement call/cc Special Form

**Goal**: Add `call-with-current-continuation` and `call/cc` as special forms


#### 3.1 Parser Recognition

Add to `crates/grift_parser/src/lib.rs` or evaluator's form recognition:

```rust
// In evaluator's step_eval or form detection
if is_symbol(func, "call-with-current-continuation") ||
   is_symbol(func, "call/cc") {
    // Handle call/cc
}
```

#### 3.2 Evaluator Implementation

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
    // Data for CallCcApply: just the captured continuation
    self.push_cont(CONT_CALLCC_APPLY, cont, env)?;
    
    // Step 3: Evaluate the procedure expression
    Ok(TrampolineState::Continue(proc_expr, env))
}
```

#### 3.3 Add Continuation Type

```rust
// Add to continuation type constants:
const CONT_CALLCC_APPLY: usize = /* next available ID */;

// When handling CONT_CALLCC_APPLY after proc evaluation:
// - data contains the captured continuation
// - current value is the evaluated proc
// - create args list (list captured-cont)
// - apply proc to args
```

#### 3.4 Continuation Application

When a captured continuation is called as a function:

```rust
// In eval_apply or similar:
Value::Continuation { .. } => {
    // Get the argument (single argument to continuation)
    let return_val = self.lisp.car(args)?;
    
    // Validate that we got exactly one argument
    if !self.lisp.is_nil(self.lisp.cdr(args)?)? {
        return Err(EvalError::WrongNumberOfArgs);
    }
    
    // Restore the continuation and return the value
    self.restore_continuation(func, return_val)
}
```

### Phase 4: Implement dynamic-wind

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

#### 4.1 Track Dynamic Wind Chain

Since we're now arena-based, the dynamic-wind chain is also stored in the arena:

```rust
pub struct Evaluator<'a, const N: usize> {
    // ... existing fields ...
    
    /// Stack of active dynamic-wind contexts (arena-based linked list)
    /// Each entry is a cons: ((before_thunk . after_thunk) . parent_chain)
    /// Stored entirely in the arena
    dynamic_wind_chain: ArenaIndex,  // Points to chain head, or Nil
}
```

#### 4.2 Implement dynamic-wind Special Form

```rust
fn eval_dynamic_wind(&mut self, 
                     before: ArenaIndex,
                     body: ArenaIndex, 
                     after: ArenaIndex,
                     env: ArenaIndex) -> Result<TrampolineState, EvalError> {
    // 1. Push continuation to call after thunk when body completes
    // 2. Push continuation to evaluate body
    // 3. Push continuation to add (before, after) to dynamic_wind_chain
    // 4. Evaluate before thunk
    // 
    // The continuation structure ensures proper ordering:
    // - before thunk runs first
    // - then body thunk runs with updated chain
    // - then after thunk runs
    // - chain is restored
}
```

#### 4.3 Integrate with call/cc

The `dynamic_wind_chain` is automatically captured with each continuation since it's stored in the `Evaluator` struct. When capturing a continuation:

```rust
fn capture_continuation(&mut self) -> Result<ArenaIndex, EvalError> {
    // Create metadata cons: (capture_env . dynamic_wind_chain)
    let metadata = self.lisp.cons(
        self.global_env,
        self.dynamic_wind_chain  // Capture current dynamic-wind state
    )?;
    
    self.lisp.alloc(Value::Continuation {
        cont_chain: self.current_cont,
        metadata,
    })
}
```

When restoring, compare chains and invoke appropriate before/after thunks:

```rust
fn restore_continuation(&mut self, cont_idx: ArenaIndex, return_val: ArenaIndex) 
    -> Result<TrampolineState, EvalError> {
    let (cont_chain, metadata) = /* extract from continuation */;
    let (_, captured_dw_chain) = self.lisp.car_cdr(metadata)?;
    
    // Find common ancestor of current and captured dynamic-wind chains
    // Call after thunks for frames being exited
    // Call before thunks for frames being entered
    self.transition_dynamic_wind(self.dynamic_wind_chain, captured_dw_chain)?;
    
    self.current_cont = cont_chain;
    self.dynamic_wind_chain = captured_dw_chain;
    // ...
}
```

### Phase 5: Testing Strategy

#### 5.1 Basic Tests

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

#### 5.2 Advanced Tests

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

#### 5.3 dynamic-wind Tests

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

### Phase 6: Performance Considerations

#### 6.1 Arena-Based Continuation Benefits

The arena-based design provides several performance advantages:

**Memory Efficiency**:
- No separate cont_stack and data_stack arrays consuming fixed memory
- Continuations only use arena space when actually created
- Natural structure sharing - parent continuations are reused by multiple children
- GC can reclaim unused continuation frames

**Capture Performance**:
- Capturing a continuation is O(1) - just save a single ArenaIndex
- No need to copy entire stack arrays
- Continuation values are lightweight (just 2 ArenaIndex fields)

**Restore Performance**:
- Restoring is O(1) - just update current_cont pointer
- No need to copy data back to stack arrays
- Structure sharing means less memory allocation

#### 6.2 Continuation Frame Size

Each continuation frame uses minimal arena space:

```
ContFrame value: 24 bytes (2 ArenaIndex + discriminant)
+ cons cell for cont_data: 24 bytes
+ cons cell for type_and_data: 24 bytes  
+ cons cells for data (varies by type)
= ~72 bytes minimum per frame + data storage
```

Compare to old design:
- Separate arrays: (1024 * size_of(Cont)) + (8192 * 8) = significant fixed overhead
- Arena design: Only allocates what's needed, freed by GC when unreachable

#### 6.3 Potential Optimizations

**Optimization 1: Continuation Frame Pooling**
- Keep a free-list of unused ContFrame structures
- Reuse frames instead of always allocating new ones
- Reduces GC pressure for programs with many continuations

**Optimization 2: Compact Representation**
- For common continuation types (ApplyForced, IfBranch), use specialized variants
- Inline more data directly in the ContFrame variant
- Trade enum size for fewer indirections

**Optimization 3: Continuation Marking**
- Add a "generation" counter to detect unreachable continuations early
- Allow early reclamation during GC without full trace

### Phase 7: Documentation

#### 7.1 User Documentation

- Add examples to REPL help
- Document memory implications
- Provide common use cases (exceptions, backtracking, generators)

#### 7.2 Architecture Documentation

- Update LISP_ARCHITECTURE.md with arena-based continuation implementation
- Document ContFrame structure and continuation chain format
- Explain dynamic-wind chain management
- Document the 2-index design constraint and how it applies to continuations

#### 7.3 Conformance Documentation

- Update SCHEME_R7RS_CONFORMANCE.md
- Mark call/cc and dynamic-wind as implemented
- Note any deviations from R7RS spec

## Implementation Phases Summary

| Phase | Description | Estimated Complexity | Priority |
|-------|-------------|---------------------|----------|
| 1 | Migrate to arena-based continuation stack | High | High |
| 2 | Add Continuation value type (2-index constraint) | Medium | High |
| 3 | Implement call/cc special form | High | High |
| 4 | Implement dynamic-wind | High | Medium |
| 5 | Comprehensive testing | Medium | High |
| 6 | Performance optimization | Medium | Low |
| 7 | Documentation | Low | Medium |

## Alternative Approaches Considered

### 1. Separate Array-Based Stacks (Original Design)

Copy entire cont_stack and data_stack arrays when capturing a continuation.

**Pros**: Simple implementation, matches current evaluator structure
**Cons**: 
- Expensive O(n) capture operation
- High memory usage (duplicate entire stacks)
- Violates 2-index constraint (needs 4+ fields)
- Fixed memory overhead even when call/cc not used

### 2. Arena-Based Linked List (Chosen Design)

Store continuation frames as linked list in the arena.

**Pros**: 
- O(1) capture (just save a pointer)
- Natural structure sharing between continuations
- Respects 2-index constraint
- Only uses memory when needed
- GC can reclaim unused frames
**Cons**: 
- More complex implementation
- Requires refactoring evaluator
- Slightly more indirection per continuation access

### 3. Hybrid: Stack for Current, Arena for Captured

Keep array-based stacks for current continuation, copy to arena only when capturing.

**Pros**: Fast normal evaluation, reasonable capture
**Cons**: 
- Most complex implementation
- Still violates 2-index constraint
- Two different continuation representations to maintain

### 4. Delimited Continuations

Implement `shift`/`reset` or `prompt`/`control` instead of full call/cc.

**Pros**: More composable, easier to reason about
**Cons**: Not R7RS standard, different semantics from call/cc

## Risks and Mitigations

### Risk 1: Arena Memory Exhaustion

Deep or numerous continuation captures could exhaust arena memory.

**Mitigation**: 
- Arena-based design naturally benefits from GC - unused continuations are reclaimed
- Continuation frames share structure, reducing duplication
- Monitor arena pressure and trigger GC when threshold reached
- Provide clear error messages when limits exceeded
- Document memory implications in user guide

### Risk 2: Semantic Complexity

call/cc interacts with all other language features in subtle ways.

**Mitigation**:
- Comprehensive test suite covering edge cases
- Study R7RS specification carefully
- Test against reference implementations (Racket, Chez Scheme)
- Start with simple cases and incrementally add complexity

### Risk 3: Evaluator Refactoring Risk

Moving from array-based to arena-based continuations requires significant refactoring.

**Mitigation**:
- Implement incrementally with tests at each step
- Keep old implementation in comments during transition
- Extensive testing after each change
- Benchmark performance before/after to detect regressions
- Consider feature flag to toggle between implementations during development

## Success Criteria

- [ ] Arena-based continuation stack replaces array-based cont_stack and data_stack
- [ ] Continuation value type respects 2-index constraint (matches Lambda/Cons design)
- [ ] `call-with-current-continuation` and `call/cc` work as per R7RS spec
- [ ] `dynamic-wind` properly manages before/after thunks
- [ ] All standard test cases pass
- [ ] Re-entrant continuations work correctly
- [ ] GC correctly traces and reclaims continuation frames
- [ ] Memory usage is reasonable (arena-based design should reduce fixed overhead)
- [ ] Performance for programs not using call/cc is unchanged or improved
- [ ] Comprehensive documentation added

## References

- [R7RS Small Specification (Section 6.10)](https://small.r7rs.org/attachment/r7rs.pdf) - Official specification for call/cc and dynamic-wind
- [SICP Section 5.5](https://mitpress.mit.edu/sites/default/files/sicp/full-text/book/book-Z-H-34.html) - Compilation and implementation of continuations
- [Three Implementation Models for Scheme](http://www.cs.indiana.edu/~dyb/pubs/3imp.pdf) - Academic paper on continuation implementation
- [Representing Control in the Presence of First-Class Continuations](https://citeseerx.ist.psu.edu/document?repid=rep1&type=pdf&doi=10.1.1.46.3653) - Continuation representation strategies

## Open Questions

1. **Should we support one-shot continuations?** Some implementations distinguish between continuations that can be called once vs. multiple times for performance. With arena-based design, multi-shot is natural.

2. **How should continuations interact with native functions?** If a continuation is captured across a native function boundary, what happens? May need continuation barriers.

3. **Should we provide continuation-barrier forms?** Forms that prevent continuation capture from crossing certain boundaries (useful for FFI and native functions).

4. **Continuation frame compaction?** Should we compress continuation frames periodically to reduce arena usage? The linked-list structure makes this feasible.

5. **Continuation depth limits?** Should there be a maximum continuation chain depth separate from arena capacity?

6. **Garbage collection strategy?** Should we use a separate GC pass for continuation frames, or integrate with the main arena GC?

## Conclusion

Implementing call/cc requires a fundamental redesign of the continuation system to:

1. **Move continuation stack from arrays to arena** - Store continuation frames as a linked list in the arena instead of separate cont_stack and data_stack arrays
2. **Respect the 2-index constraint** - Design Continuation values with only 2 ArenaIndex fields, matching Lambda and Cons
3. **Enable natural snapshotting** - Capturing a continuation becomes O(1) by saving a single ArenaIndex
4. **Improve memory efficiency** - Arena-based design eliminates fixed overhead and enables GC reclamation
5. **Implement dynamic-wind** - Properly handle resource cleanup with before/after thunks

The arena-based design provides several key advantages over the original array-based approach:

- **O(1) capture** instead of O(n) stack copying
- **Natural structure sharing** between parent and child continuations  
- **GC-friendly** - unused continuations are automatically reclaimed
- **No fixed overhead** - only uses arena space when continuations are created
- **Respects design constraints** - maintains the 2-index limit per Value variant

The implementation should be done incrementally, with each phase thoroughly tested before moving to the next. The transition from array-based to arena-based continuations is the most complex phase but provides the foundation for efficient call/cc support.

Once implemented, call/cc will enable powerful programming patterns in Grift including:
- Exception handling (before native exception support)
- Backtracking search
- Coroutines and generators  
- Web continuation servers
- Non-deterministic programming

This feature will move Grift significantly closer to full R7RS compliance while maintaining the no_std, no_alloc constraints that make Grift suitable for embedded systems.
