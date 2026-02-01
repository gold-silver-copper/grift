# Continuation-Based Multiple Values Implementation

## Reference Specification

The authoritative R7RS specification is located at:
- **`scheme-spec-r7rs/spec.html`** - Section 6.10 (Multiple return values)

## Goal

Implement R7RS-compliant multiple values where:
- Multiple values are a **control-flow concept**, not a data type
- Values flow through continuations, not as storable objects
- Only MV-aware forms (`call-with-values`, `let-values`) can consume multiple values
- Single-value contexts work transparently with 1 value

## Design Approach: Continuation-Based

Multiple values exist **only in the return path** - they never become a `Value` variant that can be stored in data structures. This matches the R7RS semantic model.

### Key Insight

```scheme
(cons (values 1 2) 'x)  ; ERROR or takes first value - MV can't be stored
(call-with-values 
  (lambda () (values 1 2))
  (lambda (a b) (+ a b)))  ; => 3, MV consumed properly
```

## Design Constraints

### Value Enum Constraint

**IMPORTANT**: Do NOT add a `Value::MultipleValues` variant. Multiple values are handled purely in the trampoline/continuation system.

### no_std / no_alloc Constraints

- Use existing arena-allocated lists to hold the values
- No new fixed-size arrays needed
- The common single-value case should have zero overhead

## Required Changes

### 1. TrampolineState (continuation.rs)

```rust
/// Trampoline state - what we're currently doing
#[derive(Clone, Copy, Debug)]
pub enum TrampolineState {
    /// Evaluate expression in environment
    Eval { expr: ArenaIndex, env: ArenaIndex },
    
    /// Return a single value to the continuation (common case, optimized)
    Return { val: ArenaIndex },
    
    /// Return multiple values to the continuation
    /// vals is a proper list of the values: (v1 v2 v3 ...)
    /// Use this for (values ...) with 0 or 2+ values
    ReturnMultiple { vals: ArenaIndex },
}
```

### 2. Continuation MV-Awareness

Add method to `Cont` to indicate which continuations accept multiple values:

```rust
impl Cont {
    /// Does this continuation accept multiple values?
    /// If false and ReturnMultiple is received, extract first value or error.
    pub fn accepts_multiple_values(&self) -> bool {
        matches!(self,
            Cont::Done |  // Top-level can accept MV (returns list to Rust)
            Cont::CallWithValuesConsumer(_) |
            Cont::LetValuesBindings(_) |
            Cont::LetStarValuesBindings(_)
        )
    }
}
```

### 3. Trampoline Loop Update (evaluator.rs)

The main trampoline loop needs to handle `ReturnMultiple`:

```rust
fn run_trampoline(&mut self) -> EvalResult {
    loop {
        match self.state {
            TrampolineState::Eval { expr, env } => {
                self.state = self.eval_dispatch(expr, env)?;
            }
            TrampolineState::Return { val } => {
                match self.pop_cont()? {
                    None => return Ok(val),
                    Some(cont) => {
                        self.state = self.apply_cont(cont, val)?;
                    }
                }
            }
            TrampolineState::ReturnMultiple { vals } => {
                match self.pop_cont()? {
                    None => {
                        // Top-level: return the values as a list
                        return Ok(vals);
                    }
                    Some(cont) => {
                        if cont.accepts_multiple_values() {
                            self.state = self.apply_cont_multiple(cont, vals)?;
                        } else {
                            // Single-value context: extract first value
                            let val = self.extract_single_value(vals)?;
                            self.state = self.apply_cont(cont, val)?;
                        }
                    }
                }
            }
        }
    }
}
```

### 4. Helper Methods

```rust
/// Extract a single value from a values list.
/// - Empty list: return #void or error (configurable)
/// - Single element: return that element  
/// - Multiple elements: return first (lenient) or error (strict)
fn extract_single_value(&self, vals: ArenaIndex) -> EvalResult {
    if self.lisp.get(vals)?.is_nil() {
        // Zero values - return void/unspecified
        self.lisp.nil()  // or a dedicated #void value
    } else {
        // One or more values - take first
        self.lisp.car(vals)
    }
}

/// Apply a MV-aware continuation to multiple values
fn apply_cont_multiple(&mut self, cont: Cont, vals: ArenaIndex) -> Result<TrampolineState, EvalError> {
    match cont {
        Cont::Done => unreachable!(), // Handled in trampoline
        Cont::CallWithValuesConsumer(data_start) => {
            self.apply_call_with_values_consumer(data_start, vals)
        }
        Cont::LetValuesBindings(data_start) => {
            self.apply_let_values_bindings(data_start, vals)
        }
        // etc.
        _ => unreachable!("Non-MV continuation in apply_cont_multiple"),
    }
}
```

## Required Procedures (Section 6.10)

### Core Multiple Value Procedures

| Procedure | Signature | Description |
|-----------|-----------|-------------|
| `values` | `(values obj ...)` | Returns 0 or more values |
| `call-with-values` | `(call-with-values producer consumer)` | Call producer, pass its values to consumer |

### Binding Forms (Section 4.2.2)

| Form | Syntax | Description |
|------|--------|-------------|
| `let-values` | `(let-values ((formals expr) ...) body)` | Bind multiple values |
| `let*-values` | `(let*-values ((formals expr) ...) body)` | Sequential MV binding |
| `define-values` | `(define-values formals expr)` | Define multiple variables |

## Implementation Plan

### Phase 1: Core Infrastructure

1. **Update `TrampolineState`** to add `ReturnMultiple { vals: ArenaIndex }`

2. **Update `values` special form**:
   ```rust
   fn eval_values(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
       // Evaluate all args, collect into list
       // Return via ReturnMultiple { vals: list }
       // Special case: (values x) can return via Return { val: x } for efficiency
   }
   ```

3. **Update trampoline loop** to handle `ReturnMultiple`

4. **Add `extract_single_value` helper**

5. **Add `Cont::accepts_multiple_values()` method**

### Phase 2: call-with-values

1. **Add `Builtin::CallWithValues`** or handle as special form

2. **Add continuation types**:
   ```rust
   /// After evaluating producer thunk, call it
   /// Stack data: [consumer, env] (2 elements)
   CallWithValuesProducer(usize),
   
   /// After producer returns (possibly MV), apply consumer
   /// Stack data: [consumer] (1 element)  
   CallWithValuesConsumer(usize),
   ```

3. **Implementation flow**:
   ```
   (call-with-values producer consumer)
   1. Evaluate producer -> push CallWithValuesProducer(consumer, env)
   2. Call producer with no args -> push CallWithValuesConsumer(consumer)
   3. Producer returns (possibly MV) -> apply consumer to values as args
   ```

### Phase 3: let-values

1. **Add as special form** in evaluator

2. **Syntax**:
   ```scheme
   (let-values (((a b) (values 1 2))
                ((c)   (values 3)))
     (+ a b c))  ; => 6
   ```

3. **Add continuation**:
   ```rust
   /// After evaluating init expr, bind values to formals
   /// Stack data: [formals, remaining_clauses, body, env, new_env] (5 elements)
   LetValuesBindings(usize),
   ```

4. **Implementation**: Similar to `let` but destructures MV into formals

### Phase 4: let*-values

Same as `let-values` but each binding is evaluated in the environment extended by previous bindings.

### Phase 5: define-values

1. **Syntax**: `(define-values (a b c) (values 1 2 3))`

2. **Implementation**: Evaluate expr, destructure MV, bind each to global env

## Formals Destructuring

Both `let-values` and `define-values` use "formals" which can be:

| Pattern | Matches | Example |
|---------|---------|---------|
| `(a b c)` | Exactly 3 values | `(values 1 2 3)` |
| `(a b . rest)` | 2+ values, rest as list | `(values 1 2 3 4)` → a=1, b=2, rest=(3 4) |
| `args` | All values as list | `(values 1 2 3)` → args=(1 2 3) |
| `()` | Zero values | `(values)` |

Implementation helper:

```rust
/// Bind values to formals pattern, extending environment
/// formals: symbol, nil, or (proper/improper) list of symbols
/// vals: list of values
/// Returns: extended environment
fn bind_formals_to_values(&mut self, formals: ArenaIndex, vals: ArenaIndex, env: ArenaIndex) -> EvalResult {
    match self.lisp.get(formals)? {
        Value::Symbol(_) => {
            // Single symbol - bind to entire list
            self.lisp.env_define(env, formals, vals)
        }
        Value::Nil => {
            // Empty formals - vals must be empty
            if !self.lisp.get(vals)?.is_nil() {
                return Err(EvalError::new(ErrorKind::ArityMismatch, ...));
            }
            Ok(env)
        }
        Value::Cons { .. } => {
            // Destructure list
            self.bind_formals_list(formals, vals, env)
        }
        _ => Err(EvalError::new(ErrorKind::InvalidSyntax, ...))
    }
}
```

## Edge Cases

### Zero Values
```scheme
(call-with-values (lambda () (values)) list)  ; => ()
(let-values ((() (values))) 'ok)  ; => ok
```

### Single Value Optimization
```scheme
;; These should be equivalent and efficient:
(values 42)      ; Can use Return { val } internally
(+ 1 (values 2)) ; Works because single-value context extracts the value
```

### Multiple Values in Single-Value Context
```scheme
(+ 1 (values 2 3))  
;; Options:
;; 1. Error: "multiple values in single-value context"
;; 2. Lenient: use first value (2), result is 3
;; Recommend: lenient for embedded use
```

## Testing

### Basic values
```scheme
(values)           ; => () or #<void>
(values 1)         ; => 1
(values 1 2 3)     ; => depends on context
```

### call-with-values
```scheme
(call-with-values (lambda () (values 1 2)) +)
;; => 3

(call-with-values (lambda () (values 1 2 3)) list)
;; => (1 2 3)

(call-with-values (lambda () (values)) (lambda () 'none))
;; => none

(call-with-values (lambda () 42) (lambda (x) (* x x)))
;; => 1764
```

### let-values
```scheme
(let-values (((a b) (values 1 2))) (+ a b))
;; => 3

(let-values (((a . rest) (values 1 2 3 4))) rest)
;; => (2 3 4)

(let-values ((args (values 1 2 3))) args)
;; => (1 2 3)
```

### let*-values
```scheme
(let*-values (((a b) (values 1 2))
              ((c) (values (+ a b))))
  c)
;; => 3
```

### define-values
```scheme
(define-values (x y) (values 10 20))
(+ x y)
;; => 30
```

### Interaction with other forms
```scheme
;; Single value flows through normally
(+ 1 (call-with-values (lambda () 2) (lambda (x) x)))
;; => 3

;; Multiple values can be nested
(call-with-values
  (lambda () 
    (call-with-values (lambda () (values 1 2)) values))
  +)
;; => 3
```

## Summary of New Continuations

| Continuation | Data | Purpose |
|--------------|------|---------|
| `CallWithValuesProducer(usize)` | [consumer, env] | After eval producer, call it |
| `CallWithValuesConsumer(usize)` | [consumer] | After producer returns, apply consumer |
| `LetValuesBindings(usize)` | [formals, remaining, body, orig_env, new_env] | Bind MV to formals |
| `LetStarValuesBindings(usize)` | [formals, remaining, body, env] | Sequential MV binding |
| `DefineValuesExpr(usize)` | [formals] | After eval, bind to globals |

## Migration Notes

- Existing `(values x y z)` usage that relied on list return will need updates
- Code using `values` result directly in arithmetic etc. will work (extracts first)
- `call-with-values` is the proper way to consume multiple values
