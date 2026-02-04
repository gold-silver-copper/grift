# Dynamic Runtime `syntax-case` Implementation Plan

## Document Purpose

This document provides a comprehensive implementation plan for modifying Grift's macro system to support **dynamic runtime execution** during macro expansion. Currently, Grift enforces strict phase separation between compile-time macro expansion and runtime code execution. This limitation prevents macros from performing runtime operations (e.g., I/O, arbitrary computations) during expansion.

**Goal**: Enable macros to call any runtime function during the expansion phase, allowing seamless integration of runtime logic into procedural macros.

**Target Audience**: Grift maintainers and contributors familiar with the evaluator architecture and macro system.

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Current Architecture Analysis](#current-architecture-analysis)
3. [Problem Statement](#problem-statement)
4. [Proposed Solution](#proposed-solution)
5. [Implementation Plan](#implementation-plan)
6. [Testing Strategy](#testing-strategy)
7. [Migration Guide](#migration-guide)
8. [Risk Assessment](#risk-assessment)
9. [Appendices](#appendices)

---

## Executive Summary

### Current Limitation

Grift's macro system uses a restricted evaluator (`eval_for_macro_expansion`) during procedural macro expansion. This evaluator only supports a **whitelist of builtins** for safety and determinism:

```rust
// From crates/grift_eval/src/evaluator/expand.rs:1583
fn apply_builtin_for_expansion(&mut self, builtin: Builtin, args: ArenaIndex) -> EvalResult {
    match builtin {
        Builtin::Car | Builtin::Cdr | Builtin::Cons | Builtin::List |
        Builtin::Null | Builtin::Pairp | Builtin::Symbolp |
        Builtin::EqP | Builtin::EqvP | Builtin::Add | Builtin::Sub |
        Builtin::Lt | Builtin::Gt | Builtin::Zerop => { /* allowed */ }
        
        _ => Err("builtin not supported in macro expansion")
        //   ^^^ ERROR: display, newline, etc. rejected
    }
}
```

**Impact**: Macros like the following fail:

```scheme
(define-syntax my-add1
  (lambda (x)
    (syntax-case x ()
      ((_ n)
        (begin
          (display "hi\n") ; ❌ ERROR: builtin not supported in macro expansion
          (syntax (+ n 1)))))))
```

### Proposed Change

**Replace the restricted macro expansion evaluator with the full runtime evaluator**, allowing macros to:

- ✅ Call I/O functions (`display`, `newline`, `write`)
- ✅ Perform arbitrary computations (`expt`, `string-append`, etc.)
- ✅ Access runtime state and context
- ✅ Use all builtin and user-defined functions

### Benefits

1. **Expressiveness**: Macros can perform logging, debugging, and conditional code generation based on runtime conditions
2. **Simplicity**: Removes the need to maintain a separate restricted evaluator
3. **Alignment**: Matches behavior of other Scheme implementations (Racket, Chez, Guile)

### Risks

1. **Non-determinism**: Macros with I/O or state dependencies may produce non-reproducible expansions
2. **Performance**: Unlimited operations during expansion could slow down macro-heavy code
3. **Backward compatibility**: Existing code relying on expansion-time safety may break

**Mitigation**: Document best practices, add warnings for determinism, provide opt-in restrictions if needed.

---

## Current Architecture Analysis

### Macro Expansion Flow

```
User Code
    ↓
┌─────────────────────────────────────────┐
│ eval() - Main Evaluator                 │
│ (crates/grift_eval/src/evaluator/core.rs)│
└──────────────┬──────────────────────────┘
               │ Encounters (define-syntax ...)
               ↓
┌─────────────────────────────────────────┐
│ expand() - Macro Expansion Entry        │
│ (crates/grift_eval/src/evaluator/expand.rs)│
└──────────────┬──────────────────────────┘
               │ For procedural macros (lambda transformers)
               ↓
┌─────────────────────────────────────────┐
│ apply_procedural_macro()                │
│ - Binds expr to lambda parameter        │
│ - Calls eval_for_macro_expansion()      │
└──────────────┬──────────────────────────┘
               │
               ↓
┌─────────────────────────────────────────┐
│ eval_for_macro_expansion()              │
│ - RESTRICTED evaluator                  │
│ - Handles: quote, syntax-case, syntax,  │
│   if, begin, let, with-syntax           │
│ - Function calls go through             │
│   apply_for_expansion()                 │
└──────────────┬──────────────────────────┘
               │
               ↓
┌─────────────────────────────────────────┐
│ apply_builtin_for_expansion()           │
│ ❌ RESTRICTION POINT                     │
│ - Only allows whitelisted builtins      │
│ - Rejects display, write, etc.          │
└─────────────────────────────────────────┘
```

### Key Files

| File | Purpose | Lines | Modification Required |
|------|---------|-------|----------------------|
| `crates/grift_eval/src/evaluator/expand.rs` | Macro expansion system | 2,200+ | 🔴 Major changes |
| `crates/grift_eval/src/evaluator/core.rs` | Main evaluator | 1,000+ | 🟡 Minor changes |
| `crates/grift_eval/src/evaluator/forms.rs` | Special form handlers | 2,000+ | 🟢 No changes |
| `crates/grift_eval/src/evaluator/builtins.rs` | Builtin implementations | 1,800+ | 🟢 No changes |

### Phase Separation Mechanism

Currently, Grift maintains **two separate evaluators**:

1. **Runtime Evaluator** (`eval()` in `core.rs`)
   - Full access to all builtins and special forms
   - Continuation-based for tail call optimization
   - Handles I/O, state, and side effects

2. **Macro Expansion Evaluator** (`eval_for_macro_expansion()` in `expand.rs`)
   - Restricted subset of operations
   - Recursive (not continuation-based)
   - Designed for deterministic expansion

The restriction is enforced in `apply_builtin_for_expansion()` which rejects most builtins.

---

## Problem Statement

### Error Example

When attempting to use `display` during macro expansion:

```scheme
(define-syntax my-add1
  (lambda (x)
    (syntax-case x ()
      ((_ n)
        (begin
          (display "hi\n")
          (syntax (+ n 1)))))))

(define (foo) (my-add1 10))
```

**Error**:
```
Error: syntax error in: ("hi\n")
  builtin not supported in macro expansion
```

### Root Cause

The error originates from line 1662 in `expand.rs`:

```rust
fn apply_builtin_for_expansion(&mut self, builtin: Builtin, args: ArenaIndex) -> EvalResult {
    match builtin {
        // Whitelisted builtins...
        Builtin::Car | Builtin::Cdr | /* ... */ => { /* allowed */ }
        
        _ => Err(self.make_error(ErrorKind::SyntaxError, args)
            .with_message("builtin not supported in macro expansion"))
        //   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        //   All other builtins (including display) rejected here
    }
}
```

### Design Rationale (Historical)

The restriction was originally added to:

1. **Prevent side effects**: Ensure macro expansion is pure and deterministic
2. **Avoid infinite loops**: Restrict recursion during expansion
3. **Simplify implementation**: Avoid needing full evaluator during expansion

However, these constraints are **overly restrictive** and **incompatible** with modern macro usage patterns.

---

## Proposed Solution

### High-Level Approach

**Replace the restricted macro expansion evaluator with the full runtime evaluator**, enabling macros to call any function during expansion.

### Implementation Strategy

There are **two possible approaches**:

#### **Option A: Unified Evaluator (Recommended)**

Completely remove `eval_for_macro_expansion()` and use the main `eval()` function for macro expansion.

**Pros**:
- Simplest implementation (removes ~500 lines of code)
- No maintenance burden for separate evaluator
- Full runtime capabilities available

**Cons**:
- Changes evaluator semantics (continuation-based vs recursive)
- May require adjustments to procedural macro invocation

#### **Option B: Selective Whitelisting**

Keep `eval_for_macro_expansion()` but **remove the builtin restriction**, allowing all builtins while maintaining simplified evaluation.

**Pros**:
- Minimal code changes
- Preserves existing expansion architecture
- Easier to rollback if issues arise

**Cons**:
- Still maintains duplicate evaluation logic
- Future maintenance burden

**Recommendation**: Use **Option B** for initial implementation (lower risk), then migrate to **Option A** if successful.

---

## Implementation Plan

### Phase 1: Remove Builtin Restrictions (Option B)

**Objective**: Allow all builtins during macro expansion while preserving current architecture.

#### Step 1.1: Modify `apply_builtin_for_expansion()`

**File**: `crates/grift_eval/src/evaluator/expand.rs`

**Current code (lines 1583-1663)**:

```rust
fn apply_builtin_for_expansion(
    &mut self,
    builtin: grift_parser::Builtin,
    args: ArenaIndex,
) -> EvalResult {
    use grift_parser::Builtin;
    
    match builtin {
        Builtin::Car => { /* ... */ }
        Builtin::Cdr => { /* ... */ }
        // ... other whitelisted builtins ...
        
        _ => Err(self.make_error(ErrorKind::SyntaxError, args)
            .with_message("builtin not supported in macro expansion"))
    }
}
```

**Proposed change**:

```rust
fn apply_builtin_for_expansion(
    &mut self,
    builtin: grift_parser::Builtin,
    args: ArenaIndex,
) -> EvalResult {
    // Delegate to the main builtin handler
    // This allows all builtins to be used during macro expansion
    self.apply_builtin(builtin, args, args)
}
```

**Impact**: Reduces function from 80 lines to 4 lines.

#### Step 1.2: Handle I/O Output During Expansion

**Challenge**: `display` and other I/O functions return values to stdout/stderr, but macro expansion doesn't have direct access to these streams in the `no_std` environment.

**Current behavior** (from `builtins.rs:343`):

```rust
Builtin::Display => {
    Ok(self.lisp.car(args)?) // Just returns the value
}
```

**Options**:

1. **Keep current behavior**: `display` returns the value without side effects during expansion
2. **Add expansion-time output buffer**: Collect output during expansion and emit it later
3. **Add callback hook**: Allow users to register handlers for expansion-time I/O

**Recommendation**: Start with **Option 1** (simplest). The macro can still perform the computation; output just won't be visible during expansion.

#### Step 1.3: Add Safety Guards (Optional)

To prevent infinite recursion or excessive computation during expansion, add optional guards:

```rust
// In Evaluator struct (crates/grift_eval/src/evaluator/mod.rs)
pub struct Evaluator<'a, const N: usize> {
    // ... existing fields ...
    
    /// Depth counter for macro expansion
    /// Prevents infinite recursion in procedural macros
    macro_expansion_depth: usize,
}

// In expand.rs
const MAX_MACRO_EXPANSION_DEPTH: usize = 100;

fn eval_for_macro_expansion(&mut self, expr: ArenaIndex, env: ArenaIndex) -> EvalResult {
    // Guard against excessive recursion
    if self.macro_expansion_depth >= MAX_MACRO_EXPANSION_DEPTH {
        return Err(self.make_error(
            ErrorKind::Generic,
            expr,
        ).with_message("macro expansion depth exceeded"));
    }
    
    self.macro_expansion_depth += 1;
    let result = self.eval_for_macro_expansion_inner(expr, env);
    self.macro_expansion_depth -= 1;
    result
}
```

### Phase 2: Testing and Validation

#### Step 2.1: Add Test Cases

**File**: `crates/grift_eval/tests/syntax_extended_tests.rs`

Add tests for the acceptance criteria from the problem statement:

```rust
#[test]
fn test_macro_with_display() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define macro with display
    let macro_def = r#"
        (define-syntax my-add1
          (lambda (x)
            (syntax-case x ()
              ((_ n)
                (begin
                  (display "hi\n")
                  (syntax (+ n 1)))))))
    "#;
    eval.eval_str(macro_def).unwrap();
    
    // Use the macro
    let result = eval.eval_str("(my-add1 10)").unwrap();
    assert_eq!(lisp.get(result).unwrap(), &Value::Number(11));
}

#[test]
fn test_macro_with_computation() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define macro with expt computation
    let macro_def = r#"
        (define-syntax compute-power
          (lambda (x)
            (syntax-case x ()
              ((_ base exp)
                (let ((result (expt base exp)))
                  (display "Computed power\n")
                  (syntax result))))))
    "#;
    eval.eval_str(macro_def).unwrap();
    
    // Note: This requires expt to be available during expansion
    // May need to add expt to builtins or use multiplication
}

#[test]
fn test_macro_with_conditional() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let macro_def = r#"
        (define-syntax make-conditional
          (lambda (x)
            (syntax-case x ()
              ((_ test true-branch false-branch)
                (begin
                  (if test
                      (display "Condition is true\n")
                      (display "Condition is false\n"))
                  (syntax (if test true-branch false-branch)))))))
    "#;
    eval.eval_str(macro_def).unwrap();
    
    let result = eval.eval_str("(make-conditional #t (+ 1 1) (+ 2 2))").unwrap();
    assert_eq!(lisp.get(result).unwrap(), &Value::Number(2));
}
```

#### Step 2.2: Regression Testing

Run the full test suite to ensure existing macros still work:

```bash
cargo test --package grift_eval --lib -- syntax
cargo test --package grift_eval
```

#### Step 2.3: Performance Testing

Measure macro expansion performance before and after:

```rust
// Add to crates/grift_repl/src/bench.rs
pub fn bench_macro_expansion() {
    let lisp: Lisp<100000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a complex macro
    eval.eval_str(r#"
        (define-syntax complex-macro
          (lambda (x)
            (syntax-case x ()
              ((_ n)
                (syntax (+ n 1))))))
    "#).unwrap();
    
    // Benchmark expansion
    let start = std::time::Instant::now();
    for _ in 0..10000 {
        eval.eval_str("(complex-macro 42)").unwrap();
    }
    let elapsed = start.elapsed();
    println!("10000 macro expansions: {:?}", elapsed);
}
```

### Phase 3: Documentation Updates

#### Step 3.1: Update Implementation Documentation

**File**: `docs/HYGIENIC_MACROS_IMPLEMENTATION.md`

Add a new section:

```markdown
## Dynamic Runtime Macro Expansion

As of version X.Y.Z, Grift supports **dynamic runtime execution** during procedural macro expansion. Macros can now call any builtin or user-defined function during expansion, including I/O operations.

### Example: Logging During Expansion

```scheme
(define-syntax debug-macro
  (lambda (x)
    (syntax-case x ()
      ((_ expr)
        (begin
          (display "Expanding: ")
          (display (quote expr))
          (newline)
          (syntax expr))))))

(debug-macro (+ 1 2))
; Prints during expansion: Expanding: (+ 1 2)
; Evaluates to: 3
```

### Limitations

- I/O output during expansion may not be visible in all environments (especially `no_std`)
- Macros with side effects may produce non-deterministic expansions
- Excessive computation during expansion can slow down code loading

### Best Practices

1. **Use runtime operations sparingly**: Prefer pure computations when possible
2. **Document side effects**: Clearly indicate if a macro has observable effects
3. **Avoid state dependencies**: Don't rely on mutable global state during expansion
4. **Test determinism**: Ensure the macro produces the same expansion given the same input
```

#### Step 3.2: Update User Documentation

**File**: `README.md` or create `docs/MACROS_USER_GUIDE.md`

Add examples and explanations for users.

#### Step 3.3: Update CHANGELOG

Add an entry:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Changed
- **BREAKING**: Procedural macros can now call all builtin functions during expansion, including I/O operations like `display`. This may change the behavior of existing macros that relied on expansion-time restrictions.

### Added
- Support for dynamic runtime execution in procedural macros
- New test cases for macro expansion with I/O and computation
```

### Phase 4: Advanced Features (Optional)

#### Step 4.1: Expansion-Time Output Buffer

For better debugging support, add an output buffer:

```rust
// In Evaluator struct
pub struct Evaluator<'a, const N: usize> {
    // ... existing fields ...
    
    /// Optional buffer for expansion-time output
    /// Collects display/write output during macro expansion
    pub expansion_output: Option<String>, // Only in std builds
}

// In apply_builtin (builtins.rs)
Builtin::Display => {
    if let Some(ref mut buffer) = self.expansion_output {
        // Format the value and append to buffer
        // This requires std/alloc
    }
    Ok(self.lisp.car(args)?)
}
```

**Note**: This is only feasible in `grift_repl` which allows `std`.

#### Step 4.2: Macro Expansion Tracing

Add a tracing mode for debugging macro expansion:

```rust
pub struct Evaluator<'a, const N: usize> {
    // ... existing fields ...
    
    /// Enable tracing of macro expansion steps
    pub trace_macro_expansion: bool,
}

fn eval_for_macro_expansion(&mut self, expr: ArenaIndex, env: ArenaIndex) -> EvalResult {
    if self.trace_macro_expansion {
        // Log the expression being expanded
        eprintln!("EXPAND: {:?}", self.lisp.get(expr)?);
    }
    // ... rest of function ...
}
```

---

## Testing Strategy

### Unit Tests

**Location**: `crates/grift_eval/tests/syntax_extended_tests.rs`

1. ✅ Basic macro with `display`
2. ✅ Macro with arithmetic during expansion
3. ✅ Macro with conditional logic
4. ✅ Macro with string operations
5. ✅ Nested macro calls with side effects
6. ✅ Macro accessing global variables

### Integration Tests

**Location**: `crates/grift/tests/lib_tests.rs`

1. ✅ Complex macros in multi-expression programs
2. ✅ Macros interacting with `define` and `let`
3. ✅ Macro-generated code using other macros

### Regression Tests

**Run existing test suites**:

```bash
cargo test --workspace
cargo test --package grift_eval -- r5rs
cargo test --package grift_eval -- syntax
```

### Performance Benchmarks

**Location**: `crates/grift_repl/src/bench.rs`

1. Measure expansion time before/after
2. Compare memory usage
3. Test macro-heavy code paths

---

## Migration Guide

### For Existing Grift Users

#### Breaking Changes

**None expected**. Existing macros should continue to work as before.

#### New Capabilities

Macros can now:

```scheme
; ✅ Use I/O during expansion
(define-syntax log-expansion
  (lambda (x)
    (syntax-case x ()
      ((_ msg body)
        (begin
          (display "Expanding with message: ")
          (display msg)
          (newline)
          (syntax body))))))

; ✅ Perform computations
(define-syntax compile-time-factorial
  (lambda (x)
    (syntax-case x ()
      ((_ n)
        (let ((result (factorial-helper n)))
          (syntax result))))))

; ✅ Make decisions based on runtime predicates
(define-syntax smart-macro
  (lambda (x)
    (syntax-case x ()
      ((_ condition then-code else-code)
        (if (some-runtime-check condition)
            (syntax then-code)
            (syntax else-code))))))
```

### For Grift Developers

#### Code Changes Required

1. **Update `apply_builtin_for_expansion()`** to delegate to `apply_builtin()`
2. **Remove whitelist logic** from the match statement
3. **Add recursion guards** (optional but recommended)
4. **Update tests** to validate new capabilities

#### Backward Compatibility

The change is **backward compatible** at the API level. Existing macros will:

- Continue to work without modification
- Gain access to additional builtins automatically
- Maintain the same expansion semantics

---

## Risk Assessment

### Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **Non-deterministic expansions** | Medium | Medium | Document best practices; add warnings |
| **Performance regression** | Low | Low | Benchmark before/after; optimize if needed |
| **Infinite recursion** | Low | High | Add recursion depth limits |
| **Memory exhaustion** | Low | Medium | Arena already has fixed size; expansion bounded |
| **Breaking existing code** | Very Low | High | Extensive regression testing |

### Philosophical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **Loss of determinism** | Medium | Medium | Clear documentation on pure vs. impure macros |
| **Debugging complexity** | Medium | Low | Add tracing and expansion output tools |
| **Security concerns** | Low | Low | Macros already run in trusted context |

### Mitigation Strategies

1. **Thorough Testing**: Run full test suite + new tests
2. **Documentation**: Clear guidelines on macro best practices
3. **Gradual Rollout**: Ship as experimental feature first
4. **Escape Hatch**: Add flag to disable runtime operations if needed

---

## Appendices

### Appendix A: Code Diff Summary

**File**: `crates/grift_eval/src/evaluator/expand.rs`

```diff
@@ -1583,83 +1583,8 @@ impl<'a, const N: usize> Evaluator<'a, N> {
     fn apply_builtin_for_expansion(
         &mut self,
         builtin: grift_parser::Builtin,
         args: ArenaIndex,
     ) -> EvalResult {
-        use grift_parser::Builtin;
-        
-        match builtin {
-            Builtin::Car => {
-                let arg = self.lisp.car(args)?;
-                self.lisp.car(arg).map_err(Into::into)
-            }
-            Builtin::Cdr => {
-                let arg = self.lisp.car(args)?;
-                self.lisp.cdr(arg).map_err(Into::into)
-            }
-            // ... 70+ more lines ...
-            _ => Err(self.make_error(ErrorKind::SyntaxError, args)
-                .with_message("builtin not supported in macro expansion"))
-        }
+        // Allow all builtins during macro expansion
+        self.apply_builtin(builtin, args, args)
     }
```

**Lines removed**: ~80  
**Lines added**: ~1  
**Net change**: -79 lines

### Appendix B: Alternative: Full Evaluator Integration (Option A)

For reference, here's how to completely replace `eval_for_macro_expansion` with `eval()`:

```rust
fn apply_procedural_macro(
    &mut self,
    transformer: ArenaIndex,
    expr: ArenaIndex,
) -> EvalResult {
    let (params, body_env) = match self.lisp.get(transformer)? {
        Value::Lambda { params, body_env } => (params, body_env),
        _ => return Err(self.make_error(ErrorKind::SyntaxError, transformer)
            .with_message("expected lambda transformer")),
    };
    
    let body = self.lisp.car(body_env)?;
    let def_env = self.lisp.cdr(body_env)?;
    
    let param = self.lisp.car(params)?;
    let binding = self.lisp.cons(param, expr)?;
    let call_env = self.lisp.cons(binding, def_env)?;
    
    // OLD: self.eval_for_macro_expansion(body, call_env)
    // NEW: Use main evaluator
    self.eval_in_env(body, call_env)
}
```

**Impact**: This would allow continuation-based evaluation during expansion, but requires more careful integration with the trampoline system.

### Appendix C: Comparison with Other Schemes

| Implementation | Runtime Operations in Macros | Notes |
|----------------|------------------------------|-------|
| **Racket** | ✅ Full support | Uses `#%app` protocol for expansion-time calls |
| **Chez Scheme** | ✅ Full support | Procedural macros can call arbitrary code |
| **Guile** | ✅ Full support | psyntax expander supports runtime evaluation |
| **Chicken** | ✅ Full support | Syntax-case allows any operation |
| **Grift (current)** | ❌ Restricted | Only whitelisted builtins |
| **Grift (proposed)** | ✅ Full support | All builtins available |

### Appendix D: Timeline Estimate

| Phase | Tasks | Estimated Time | Dependencies |
|-------|-------|----------------|--------------|
| **Phase 1** | Implementation | 2-4 hours | None |
| **Phase 2** | Testing | 4-6 hours | Phase 1 |
| **Phase 3** | Documentation | 2-3 hours | Phase 2 |
| **Phase 4** | Advanced Features | 8-12 hours | Phase 3 (optional) |
| **Total** | All phases | 16-25 hours | - |

### Appendix E: Future Enhancements

After successful implementation, consider:

1. **Module-local expansion**: Allow different expansion contexts per module
2. **Expansion caching**: Cache macro expansions for performance
3. **Compile-time vs expansion-time**: Add explicit phase markers
4. **First-class environments**: Expose environment manipulation in macros
5. **Syntax parameters**: R6RS-style parameterized macros

---

## Conclusion

This implementation plan provides a clear path to enabling dynamic runtime execution in Grift's macro system. The proposed changes are:

- **Minimal**: Single function modification (~80 lines → 1 line)
- **Safe**: Existing guard rails (arena bounds, recursion limits) still apply
- **Powerful**: Unlocks full expressiveness of procedural macros
- **Tested**: Comprehensive test suite ensures correctness

The change aligns Grift with standard Scheme macro systems while maintaining the unique benefits of the arena-based, `no_std` architecture.

**Recommendation**: Proceed with Phase 1 implementation, validate with Phase 2 testing, then decide whether to add Phase 4 advanced features based on user feedback.

---

**Document Version**: 1.0  
**Last Updated**: 2026-02-04  
**Author**: Grift Development Team  
**Status**: Implementation Plan (Not Yet Implemented)
