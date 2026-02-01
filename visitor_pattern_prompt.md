# Visitor Pattern for Continuations

## Goal

Refactor the `step_return` function (currently 665 lines with 30+ match arms) into a cleaner architecture using the visitor pattern. This will improve:

- **Maintainability**: Each continuation handler is isolated
- **Testability**: Handlers can be unit tested independently
- **Extensibility**: Adding new continuation types is straightforward
- **Code organization**: Related code is grouped together

## Current State

The `step_return` function is a massive match statement:

```rust
fn step_return(&mut self, val: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
    let cont = self.pop_cont();
    
    match cont {
        Cont::Done => { /* 3 lines */ }
        Cont::IfBranch(data_start) => { /* 10 lines */ }
        Cont::ApplyForced(data_start) => { /* 100+ lines */ }
        Cont::LambdaFirstBind(data_start) => { /* 8 lines */ }
        Cont::LambdaBindArg(data_start) => { /* 40 lines */ }
        Cont::LetBinding(data_start) => { /* 25 lines */ }
        Cont::LetStarBinding(data_start) => { /* 25 lines */ }
        Cont::LetrecInit(data_start) => { /* 25 lines */ }
        Cont::When(data_start) => { /* 15 lines */ }
        Cont::Unless(data_start) => { /* 15 lines */ }
        // ... 20+ more variants
    }
}
```

**Problems:**
1. Single 665-line function is hard to navigate
2. Adding a new continuation requires modifying this giant match
3. Similar patterns are duplicated (When/Unless, Let/Let*/Letrec)
4. Testing individual handlers requires running the whole evaluator

## Proposed Design

### Option A: Trait-Based Visitor (Most Idiomatic Rust)

Define a trait for handling continuation returns:

```rust
/// Trait for handling a continuation's return value
trait ContHandler<const N: usize> {
    /// Process the returned value and produce the next trampoline state
    fn handle(
        &self,
        eval: &mut Evaluator<'_, N>,
        val: ArenaIndex,
        data_start: usize,
    ) -> Result<Option<TrampolineState>, EvalError>;
}
```

Each continuation type implements this trait:

```rust
struct IfBranchHandler;

impl<const N: usize> ContHandler<N> for IfBranchHandler {
    fn handle(
        &self,
        eval: &mut Evaluator<'_, N>,
        val: ArenaIndex,
        data_start: usize,
    ) -> Result<Option<TrampolineState>, EvalError> {
        let (then_expr, else_expr, env) = eval.unpack_if_branch(data_start);
        let branch = if !eval.is_false(val)? { then_expr } else { else_expr };
        if branch.is_nil() {
            let nil = eval.lisp.nil()?;
            Ok(Some(TrampolineState::Return { val: nil }))
        } else {
            Ok(Some(TrampolineState::Eval { expr: branch, env }))
        }
    }
}
```

**Dispatch via const array:**

```rust
// In continuation.rs or a new handlers.rs
const HANDLERS: [&dyn ContHandler<N>; Cont::VARIANT_COUNT] = [
    &DoneHandler,
    &IfBranchHandler,
    &ApplyForcedHandler,
    // ...
];

// In step_return:
fn step_return(&mut self, val: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
    let cont = self.pop_cont();
    let (handler_idx, data_start) = cont.handler_index_and_data();
    HANDLERS[handler_idx].handle(self, val, data_start)
}
```

**Challenge:** Generic const N makes this tricky with trait objects. May need boxing or monomorphization.

### Option B: Function Pointer Table (Simpler)

Use a function pointer table without traits:

```rust
type ContHandlerFn<const N: usize> = fn(
    &mut Evaluator<'_, N>,
    ArenaIndex,  // val
    usize,       // data_start
) -> Result<Option<TrampolineState>, EvalError>;

impl<'a, const N: usize> Evaluator<'a, N> {
    const CONT_HANDLERS: [ContHandlerFn<N>; Cont::VARIANT_COUNT] = [
        Self::handle_done,
        Self::handle_if_branch,
        Self::handle_apply_forced,
        Self::handle_lambda_first_bind,
        // ... all handlers
    ];

    fn step_return(&mut self, val: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
        let cont = self.pop_cont();
        let (idx, data_start) = cont.index_and_data();
        Self::CONT_HANDLERS[idx](self, val, data_start)
    }

    fn handle_done(
        &mut self,
        _val: ArenaIndex,
        _data_start: usize,
    ) -> Result<Option<TrampolineState>, EvalError> {
        Ok(None)
    }

    fn handle_if_branch(
        &mut self,
        val: ArenaIndex,
        data_start: usize,
    ) -> Result<Option<TrampolineState>, EvalError> {
        let (then_expr, else_expr, env) = self.unpack_if_branch(data_start);
        let branch = if !self.is_false(val)? { then_expr } else { else_expr };
        if branch.is_nil() {
            let nil = self.lisp.nil()?;
            Ok(Some(TrampolineState::Return { val: nil }))
        } else {
            Ok(Some(TrampolineState::Eval { expr: branch, env }))
        }
    }

    // ... more handlers
}
```

### Option C: Macro-Generated Match (Least Invasive)

Keep the match but generate it from a declarative definition:

```rust
macro_rules! define_cont_handlers {
    ($(
        $variant:ident $(($data:ident))? => $handler:expr
    ),* $(,)?) => {
        fn step_return(&mut self, val: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
            let cont = self.pop_cont();
            match cont {
                $(
                    Cont::$variant$(($data))? => $handler,
                )*
            }
        }
    };
}

// Usage:
define_cont_handlers! {
    Done => Ok(None),
    
    IfBranch(data_start) => {
        let (then_expr, else_expr, env) = self.unpack_if_branch(data_start);
        let branch = if !self.is_false(val)? { then_expr } else { else_expr };
        if branch.is_nil() {
            let nil = self.lisp.nil()?;
            Ok(Some(TrampolineState::Return { val: nil }))
        } else {
            Ok(Some(TrampolineState::Eval { expr: branch, env }))
        }
    },
    
    // ... more handlers
}
```

## Recommended Approach: Option B (Function Pointer Table)

**Rationale:**
1. No trait object complexity with generic N
2. Functions can be in separate module files
3. Const array means no runtime dispatch overhead
4. Each handler is a separate, testable function
5. Works well with `no_std`

## Implementation Plan

### Phase 1: Add Infrastructure to Cont

```rust
// In continuation.rs

impl Cont {
    /// Get the variant index (for dispatch table) and data_start
    pub fn index_and_data(&self) -> (usize, usize) {
        match self {
            Cont::Done => (0, 0),
            Cont::IfBranch(d) => (1, *d),
            Cont::ApplyForced(d) => (2, *d),
            Cont::BuiltinForceArg(d) => (3, *d),
            Cont::BinaryBuiltinFirst(d) => (4, *d),
            Cont::BinaryBuiltinSecond(d) => (5, *d),
            Cont::LambdaFirstBind(d) => (6, *d),
            Cont::LambdaBindArg(d) => (7, *d),
            Cont::LetBinding(d) => (8, *d),
            Cont::LetStarBinding(d) => (9, *d),
            Cont::LetrecInit(d) => (10, *d),
            Cont::When(d) => (11, *d),
            Cont::Unless(d) => (12, *d),
            Cont::EvalExpr(d) => (13, *d),
            Cont::CondTest(d) => (14, *d),
            Cont::And(d) => (15, *d),
            Cont::Or(d) => (16, *d),
            Cont::BeginSeq(d) => (17, *d),
            Cont::CaseKey(d) => (18, *d),
            Cont::DoInit(d) => (19, *d),
            Cont::DoTestResult(d) => (20, *d),
            Cont::DoBody(d) => (21, *d),
            Cont::DoStep(d) => (22, *d),
            Cont::ApplyFirst(d) => (23, *d),
            Cont::ApplySecond(d) => (24, *d),
            Cont::ValuesCollect(d) => (25, *d),
            Cont::DefineValue(d) => (26, *d),
            Cont::SetValue(d) => (27, *d),
            Cont::NativeArgsCollect(d) => (28, *d),
            Cont::QuasiquoteCar(d) => (29, *d),
            Cont::QuasiquoteCdr(d) => (30, *d),
            Cont::QuasiquoteUnquoteWrap => (31, 0),
            Cont::QuasiquoteNestedWrap => (32, 0),
            Cont::QuasiquoteSplice(d) => (33, *d),
            Cont::QuasiquoteSpliceAppend(d) => (34, *d),
        }
    }
    
    pub const VARIANT_COUNT: usize = 35;
}
```

### Phase 2: Create Handler Module

Create `evaluator/cont_handlers.rs`:

```rust
//! Continuation handler functions for the visitor pattern.
//!
//! Each handler processes a returned value for a specific continuation type.

use super::*;

/// Handler function type
pub type ContHandlerFn<const N: usize> = fn(
    &mut Evaluator<'_, N>,
    ArenaIndex,
    usize,
) -> Result<Option<TrampolineState>, EvalError>;

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Dispatch table for continuation handlers
    pub const CONT_HANDLERS: [ContHandlerFn<N>; Cont::VARIANT_COUNT] = [
        Self::handle_done,           // 0
        Self::handle_if_branch,      // 1
        Self::handle_apply_forced,   // 2
        // ... all 35 handlers
    ];
}
```

### Phase 3: Extract Handlers (One by One)

Move each match arm to its own function. Start with simple ones:

```rust
// Simple handlers first
impl<'a, const N: usize> Evaluator<'a, N> {
    fn handle_done(
        &mut self,
        _val: ArenaIndex,
        _data_start: usize,
    ) -> Result<Option<TrampolineState>, EvalError> {
        Ok(None)
    }

    fn handle_eval_expr(
        &mut self,
        val: ArenaIndex,
        data_start: usize,
    ) -> Result<Option<TrampolineState>, EvalError> {
        let env = self.unpack_eval_expr(data_start);
        Ok(Some(TrampolineState::Eval { expr: val, env }))
    }

    fn handle_when(
        &mut self,
        val: ArenaIndex,
        data_start: usize,
    ) -> Result<Option<TrampolineState>, EvalError> {
        let (body, env) = self.unpack_when(data_start);
        let test_passed = !self.is_false(val)?;
        if test_passed {
            let begin = self.lisp.symbol("begin")?;
            let new_expr = self.lisp.cons(begin, body)?;
            Ok(Some(TrampolineState::Eval { expr: new_expr, env }))
        } else {
            let nil = self.lisp.nil()?;
            Ok(Some(TrampolineState::Return { val: nil }))
        }
    }
    
    // ... continue for all handlers
}
```

### Phase 4: Update step_return

```rust
fn step_return(&mut self, val: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
    let cont = self.pop_cont();
    let (idx, data_start) = cont.index_and_data();
    Self::CONT_HANDLERS[idx](self, val, data_start)
}
```

### Phase 5: Split into Files (Optional)

```
evaluator/
├── mod.rs
├── cont_handlers/
│   ├── mod.rs           # Dispatch table
│   ├── control_flow.rs  # Done, If, When, Unless, And, Or, Begin
│   ├── binding.rs       # Let, Let*, Letrec, Define, Set
│   ├── apply.rs         # ApplyForced, Lambda*, Builtin*, Native*
│   ├── iteration.rs     # Do*, Case
│   └── quasiquote.rs    # All quasiquote handlers
```

## Testing Strategy

With handlers as separate functions, they can be unit tested:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_if_branch_true() {
        let mut lisp = Lisp::<1024>::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Setup: create condition result (true), then/else branches
        let then_expr = lisp.number(42).unwrap();
        let else_expr = lisp.number(0).unwrap();
        let env = eval.global_env();
        
        // Pack continuation data
        let data_start = eval.pack_if_branch(then_expr, else_expr, env).unwrap();
        
        // Test: handle with true value
        let true_val = lisp.true_val().unwrap();
        let result = eval.handle_if_branch(true_val, data_start).unwrap();
        
        // Verify: should evaluate then branch
        match result {
            Some(TrampolineState::Eval { expr, .. }) => {
                assert_eq!(expr, then_expr);
            }
            _ => panic!("Expected Eval state"),
        }
    }
}
```

## Migration Strategy

1. **Add `index_and_data()` to Cont** - Non-breaking
2. **Create handler functions alongside existing code** - Non-breaking
3. **Create dispatch table** - Non-breaking
4. **Switch `step_return` to use dispatch** - Single commit, test thoroughly
5. **Remove old match arms** - Cleanup

## Trade-offs

### Pros
- Each handler is isolated and testable
- Adding new continuations is straightforward
- Code is organized by functionality
- Easier to understand individual handlers
- Can add per-handler documentation

### Cons
- Function call overhead (mitigated by inlining)
- More boilerplate (handler function signatures)
- Dispatch table must be kept in sync with Cont variants
- Slightly more complex architecture

## Alternatives Considered

1. **Keep giant match**: Works but increasingly unmaintainable
2. **Trait objects**: Complex with generic N, requires boxing
3. **Enum dispatch macro**: Less flexible, still one file
4. **Separate Evaluator per Cont**: Too much code duplication

## Success Criteria

- [ ] All 273 tests pass
- [ ] `step_return` reduced to ~10 lines
- [ ] Each handler is a separate function
- [ ] Handlers are grouped logically in modules
- [ ] No performance regression (benchmark)
- [ ] New continuation can be added by:
  1. Adding variant to `Cont`
  2. Adding to `index_and_data()`
  3. Adding handler function
  4. Adding to dispatch table
