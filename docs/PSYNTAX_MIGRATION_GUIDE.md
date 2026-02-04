# psyntax.scm Migration Guide

## Document Purpose

This guide provides a comprehensive roadmap for replacing Grift's current `syntax-case` implementation with the psyntax.scm reference implementation from Guile. It analyzes the architectural differences, compatibility challenges, and provides a step-by-step migration strategy suitable for Grift's unique `no_std`, `no_alloc` constraints.

**Target Audience**: Developers familiar with both Scheme macro systems and Rust systems programming.

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Current State Analysis](#current-state-analysis)
3. [psyntax.scm Architecture](#psyntaxscm-architecture)
4. [Compatibility Analysis](#compatibility-analysis)
5. [Migration Strategy](#migration-strategy)
6. [Implementation Roadmap](#implementation-roadmap)
7. [Testing Strategy](#testing-strategy)
8. [Risk Assessment](#risk-assessment)
9. [Appendices](#appendices)

---

## Executive Summary

### Current Situation

Grift currently implements a **functional but simplified** procedural macro system with `syntax-case`:

- **Location**: `crates/grift_eval/src/evaluator/expand.rs` and `forms.rs`
- **Features**: Pattern matching, syntax objects with marks, template transcription, procedural transformers
- **Architecture**: Continuation-based evaluator in Rust with arena allocation
- **Status**: ✅ Complete for core procedural macros (Phases 1-6)

### What is psyntax.scm?

**psyntax.scm** is the reference implementation of R6RS/R7RS hygienic macros from Guile:

- **Origin**: Chez Scheme (R. Kent Dybvig et al., 1992)
- **License**: GNU LGPL 3.0
- **Size**: 3,196 lines of Scheme
- **Scope**: Complete syntax expander with full hygiene, module system integration, and bootstrapping

### Key Recommendation

⚠️ **Direct port of psyntax.scm to Grift is NOT RECOMMENDED** due to fundamental architectural incompatibilities:

1. **Dynamic allocation**: psyntax assumes unlimited heap; Grift uses fixed arena pools
2. **Module system**: psyntax deeply integrates with Guile's module system; Grift is standalone
3. **Self-hosting**: psyntax requires a Scheme interpreter to run; Grift is Rust-based
4. **Complexity**: 3,196 lines is excessive for Grift's minimalist embedded design

### Alternative Approach

This document recommends a **selective enhancement strategy**:

- Extract **specific algorithms** from psyntax (pattern matching, wrap management)
- Adapt to **Grift's arena-based memory model**
- Preserve **current continuation-based architecture**
- Add **incremental improvements** (better hygiene, nested ellipsis, error reporting)

---

## Current State Analysis

### Grift's Existing Implementation

#### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Grift Evaluator Core                      │
│                  (Continuation-based)                        │
└───────────────────────────┬─────────────────────────────────┘
                            │
        ┌───────────────────┴───────────────────┐
        │                                       │
        ▼                                       ▼
┌───────────────────┐              ┌───────────────────────┐
│  Macro Expansion  │              │  Special Forms        │
│   (expand.rs)     │              │    (forms.rs)         │
├───────────────────┤              ├───────────────────────┤
│ • syntax-rules    │              │ • syntax-case         │
│ • procedural      │              │ • syntax (template)   │
│ • pattern match   │              │ • let-syntax          │
│ • template trans. │              │ • with-syntax         │
└───────────────────┘              └───────────────────────┘
        │                                       │
        └───────────────────┬───────────────────┘
                            │
                            ▼
                  ┌─────────────────────┐
                  │   Syntax Objects    │
                  │   (Value::Syntax)   │
                  ├─────────────────────┤
                  │ expr: ArenaIndex    │
                  │ marks: ArenaIndex   │
                  │ subst: ArenaIndex   │
                  └─────────────────────┘
                            │
                            ▼
                  ┌─────────────────────┐
                  │  Arena Allocator    │
                  │  (Fixed Capacity)   │
                  └─────────────────────┘
```

#### Key Data Structures

**Syntax Object** (`crates/grift_parser/src/value.rs`):
```rust
pub enum Value {
    // ...
    Syntax {
        expr: ArenaIndex,     // Underlying expression
        marks: ArenaIndex,    // List of marks for hygiene
        subst: ArenaIndex,    // Substitution environment (alist)
    },
    // ...
}
```

**Pattern Matching** (`crates/grift_eval/src/evaluator/expand.rs`):
- Uses recursive descent matching
- Supports: literals, wildcards (`_`), ellipsis (`...`), pattern variables
- Captures bindings into association lists
- Limited nested ellipsis support (known bug documented in Phase 7)

**Template Transcription** (`crates/grift_eval/src/evaluator/forms.rs`):
- `syntax` form expands templates using pattern bindings
- Substitutes pattern variables with captured values
- Handles ellipsis repetition (single-level)
- Applies marks for hygiene

**Procedural Macros** (`crates/grift_eval/src/evaluator/expand.rs`):
- Transformers are `(lambda (x) ...)` procedures
- Input: syntax object representing macro use
- Output: transformed syntax object
- Evaluation uses simplified eval-for-macro-expansion

#### Limitations

| Limitation | Impact | Workaround |
|------------|--------|------------|
| **Nested ellipsis** `((a ...) ...)` | Complex patterns fail | Use recursive helpers |
| **Wrap algorithm** | Simplified marks-only | Acceptable for current needs |
| **Module system** | No integration | Not needed for embedded use |
| **Error messages** | Generic | Adequate for debugging |
| **Pattern coverage** | Limited to common cases | Sufficient for standard macros |

---

## psyntax.scm Architecture

### Structure Overview

The psyntax.scm file (3,196 lines) is organized into these major sections:

```
Lines    Section                    Purpose
─────────────────────────────────────────────────────────────────
  1-500  Infrastructure             Bootstrap, constructors, matching
501-1000 Wraps & Scope Management   Identifier resolution, hygiene
1001-1500 Expression Expansion      Core expansion dispatcher
1501-2000 Core Form Transformers    lambda, let, quote, if, etc.
2001-2500 Pattern Matching Engine   syntax-case dispatch
2501-3196 Public API & Utilities    User-facing interface
```

### Core Data Structures

#### 1. Syntax Objects (Built-in Guile Primitive)

```scheme
(make-syntax expression wrap module sourcev)
```

- **expression**: Symbol or datum
- **wrap**: Scope information (see below)
- **module**: Compilation context (Guile module)
- **sourcev**: Source location vector `[filename line column]`

**Contrast with Grift**: Grift's `Value::Syntax` omits module and sourcev; uses simpler marks list.

#### 2. Wrap - Two-Level Scope Representation

```scheme
;; Fundamental data structure for hygiene
<wrap> ::= (marks . substs)

;; Example:
((m1 m2) . (ribcage1 'shift ribcage2))
 ^^^^^^     ^^^^^^^^^^^^^^^^^^^^^^^^^
 marks      substitutions
```

**Marks**: List of unique identifiers (scope markers)
- Generated via `gensym`
- Compared using `eq?` (pointer equality)
- Added when expanding macro output
- Removed when matching original input

**Substitutions**: List of ribcages or shift markers
- Ribcage: Maps symbols to labels in a scope
- Shift marker: Represents module boundary crossing

**Contrast with Grift**: Grift uses marks-only; no substitution layer. This simplification works for local hygiene but limits cross-module macro interaction.

#### 3. Ribcage - Symbol-to-Label Mapping

```scheme
(make-ribcage symnames marks labels)
```

Two representations:

**Vector form** (efficient, immutable):
```scheme
#(#(sym1 sym2 sym3)          ; symbols
  #((m1 m2) (m1) (m2))       ; per-symbol marks
  #(label1 label2 label3))   ; unique labels
```

**List form** (incremental growth):
```scheme
((sym1 sym2) ((m1 m2) (m1)) (label1 label2))
```

**Contrast with Grift**: Grift uses simple association lists `((sym . label) ...)` in substitution environment.

#### 4. Environment - Lexical Bindings

```scheme
<environment> ::= ((label . binding)*)

<binding> ::=
  | (macro . procedure)              ; Macro transformer
  | (syntax . (var . level))         ; Pattern variable
  | (lexical . var)                  ; Lambda/let variable
  | (ellipsis . identifier)          ; Custom ellipsis
  | (global)                         ; Global reference
  | (displaced-lexical)              ; Out-of-scope error
```

**Contrast with Grift**: Grift stores pattern bindings separately using `#:pattern-bindings` key in environment. Simpler but less comprehensive.

### Key Algorithms

#### 1. Identifier Resolution (`id-var-name`, lines 583-665)

**Purpose**: Resolve an identifier to its binding, considering all scope information.

**Algorithm**:
```
1. Extract (marks . substs) from identifier's wrap
2. For each ribcage in substs:
   a. Search for symbol in ribcage.symnames
   b. If found and marks match → return label
3. If no local binding found:
   a. Check module context
   b. Return syntax object (top-level) or symbol (global)
```

**Mark comparison** (`same-marks?`):
- Recursively compares mark lists
- Uses `eq?` for mark identity (pointer equality in Guile)

**Grift equivalent**: `lookup_in_substitution` walks simple alist; marks checked separately.

#### 2. Pattern Matching (`$sc-dispatch`, lines 2726-2876)

**Purpose**: Match syntax against pattern, capturing pattern variables.

**Pattern Types**:
```scheme
()                      ; Empty list
'any                    ; Capture single item
(each-any)              ; Repetition: (x* ...)
#(each <pattern>)       ; Iterated pattern
#(each+ <p1> <p2*> <p3>) ; Mixed: before, middle*, after
#(free-id <key>)        ; Literal keyword
#(atom <obj>)           ; Constant datum
#(vector <p>)           ; Vector pattern
```

**Dispatch logic**:
- Recursively walks input expression
- Accumulates captures in nested list structure
- Returns `#f` on mismatch; capture tree on success

**Grift equivalent**: `match_pattern` in `expand.rs` with similar recursion but limited pattern coverage.

#### 3. Macro Output Rebuilding (`rebuild-macro-output`, lines 1394-1431)

**Purpose**: Add hygiene marks to macro-generated syntax.

**Algorithm**:
```
rebuild-macro-output(x, new-mark):
  if x is syntax object:
    if x has anti-mark:
      return x with anti-mark removed  ; Original input
    else:
      return x with new-mark added     ; Macro output
  if x is pair:
    return (rebuild car, rebuild cdr)
  if x is vector:
    return #(rebuild elements)
  else:
    return x
```

**Anti-mark mechanism**: When macro is invoked, its input is marked with anti-mark `(cons 'anti 'mark)`. During output rebuilding:
- Anti-mark cancels with new mark → original binding preserved
- No anti-mark → new mark applied → fresh identifier

**Grift equivalent**: Simpler; applies marks during template transcription without anti-mark subtlety.

#### 4. Two-Pass Body Expansion (`expand-body`, lines 1437-1601)

**Purpose**: Handle internal definitions in lambda/let/module bodies.

**Pass 1 - Collection**:
```scheme
(scan body env)
  → (ids labels var-ids vars val-exps body-exps)
```
- Collects `define` and `define-syntax` forms
- Generates unique labels for all identifiers
- Stores expression thunks (delayed expansion)
- Handles `begin` splicing

**Pass 2 - Expansion**:
```scheme
(expand-body-expressions val-exps env)
```
- All definitions now visible in extended environment
- Expands all expressions with forward references available
- Allows mutual recursion in macros

**Grift equivalent**: Supports internal `define` in lambda (Phase 8) but simpler single-pass approach.

### Module System Integration

**Critical Dependency**: psyntax assumes Guile's module system:

```scheme
;; Global binding registration
(global-extend type sym val)
  → (module-define! (current-module) sym val)

;; Macro transformer evaluation
(top-level-eval x mod)
  → (primitive-eval x)  ; Uses Guile evaluator

;; Unique ID generation
(module-generate-unique-id! mod)
```

**Grift status**: No module system; single global environment. This is **acceptable** for embedded use but means:
- No namespace isolation
- All bindings are global or lexical
- No separate compilation units

---

## Compatibility Analysis

### Fundamental Incompatibilities

#### 1. Memory Model ❌ CRITICAL

| Aspect | psyntax.scm | Grift |
|--------|-------------|-------|
| **Allocation** | Unlimited heap via `cons` | Fixed arena (10K-100K cells) |
| **Growth strategy** | Dynamic expansion | Pre-allocated, panic on exhaustion |
| **Sharing** | Structural sharing via pointers | Copy-heavy to avoid lifetimes |
| **Cycles** | GC handles cycles | Manual cycle avoidance |

**Impact**: psyntax's unbounded data structures (wrap lists, environment chains, ribcage vectors) incompatible with fixed arena.

**Mitigation**: 
- Impose maximum wrap depth (e.g., 32 levels)
- Use stack-allocated buffers for temporaries
- Arena bump allocator for persistent structures

#### 2. Recursion Depth ❌ CRITICAL

| Aspect | psyntax.scm | Grift |
|--------|-------------|-------|
| **Pattern matching** | Recursive descent (unlimited) | Stack-based (limited) |
| **Wrap traversal** | Recursive `join-wraps` | Iterative alternatives needed |
| **Expansion** | Mutually recursive helpers | Trampolined evaluation |

**Impact**: Deep macro nesting or complex patterns could overflow Rust stack in `no_std` environment.

**Mitigation**:
- Trampoline all recursive algorithms
- Tail-call optimization via explicit loops
- Depth limits with clear error messages

#### 3. Dynamic Evaluation ❌ HIGH

| Aspect | psyntax.scm | Grift |
|--------|-------------|-------|
| **Transformer execution** | `(primitive-eval transformer)` | Grift evaluator (self-hosted) |
| **Module loading** | `(use-modules ...)` dynamic | Static compilation only |
| **Top-level define** | Mutates module environment | Immutable after init |

**Impact**: psyntax expects live Scheme evaluator; Grift's evaluator is the target, not the tool.

**Mitigation**:
- Grift already self-hosts macro expansion ✅
- Use existing `eval_for_macro_expansion` pathway
- No external dependencies needed

#### 4. Identifier Equality ⚠️ MEDIUM

| Aspect | psyntax.scm | Grift |
|--------|-------------|-------|
| **Mark comparison** | `eq?` pointer equality | Need explicit IDs |
| **Unique generation** | `(vector 'tmp)` fresh object | `gensym_counter` integer |
| **Ribcage lookup** | Hash-consed vectors | Arena indexes |

**Impact**: Guile's `eq?` relies on object identity; Grift needs explicit unique IDs.

**Mitigation**:
- Use integer-based gensym (already implemented) ✅
- Mark comparison via integer equality
- Acceptable substitute for pointer equality

#### 5. Source Tracking ⚠️ LOW

| Aspect | psyntax.scm | Grift |
|--------|-------------|-------|
| **Source info** | `sourcev` vector in syntax objects | Not currently tracked |
| **Error reporting** | Line/column annotations | Generic error messages |
| **Debug output** | `syntax->datum` with props | Raw datum conversion |

**Impact**: Nice-to-have feature; not critical for functionality.

**Mitigation**:
- Future enhancement: add optional source tracking
- Use `ArenaIndex` to store metadata separately
- Low priority

### Compatible Features

#### 1. Pattern Matching ✅ HIGH COMPATIBILITY

**psyntax pattern types** map directly to Grift:

| psyntax Pattern | Grift Equivalent | Status |
|----------------|------------------|--------|
| `'any` | Pattern variable | ✅ Supported |
| `(each-any)` | `...` ellipsis | ✅ Supported |
| `#(free-id k)` | Literal keyword | ✅ Supported |
| `#(atom obj)` | Constant match | ✅ Supported |
| `#(each+ ...)` | Mixed ellipsis | ⚠️ Partial (nested bug) |
| `#(vector p)` | Vector patterns | ⚠️ Limited |

**Recommendation**: Extract `$sc-dispatch` algorithm with simplifications for arena allocation.

#### 2. Template Transcription ✅ HIGH COMPATIBILITY

**psyntax template expansion** similar to Grift:

| Feature | psyntax | Grift | Status |
|---------|---------|-------|--------|
| **Variable substitution** | `gen-ref` | `lookup_in_bindings` | ✅ Same concept |
| **Ellipsis iteration** | Recursive `gen` | Iterative `expand_ellipsis` | ✅ Equivalent |
| **Hygiene marks** | Anti-mark + new mark | Mark addition | ✅ Simplified |
| **Nested patterns** | Level tracking | Limited support | ⚠️ Enhancement needed |

**Recommendation**: Borrow level-tracking algorithm for nested ellipsis fix.

#### 3. Wrap Management ⚠️ MEDIUM COMPATIBILITY

**psyntax wrap operations**:
- `join-wraps`: Combine two wraps (union of marks, concatenate substs)
- `extend-wrap`: Add new ribcage to front
- `id-var-name`: Resolve identifier using wrap

**Grift simplification**:
- Marks-only (no subst layer)
- Simpler lookup via alist

**Trade-off**: Grift's approach sacrifices cross-module hygiene for simplicity. **Acceptable** for embedded use without modules.

**Recommendation**: Keep current approach; document limitations.

---

## Migration Strategy

### Recommended Approach: **Selective Enhancement**

Given the incompatibilities, a **full psyntax port is impractical**. Instead, extract specific improvements:

### Phase 1: Algorithm Extraction (2-3 weeks)

**Goal**: Adapt psyntax algorithms to Grift's architecture.

#### 1.1 Enhanced Pattern Matching

**Extract from**: `$sc-dispatch` (lines 2726-2876)

**Adapt for Grift**:
```rust
// New pattern types
pub enum Pattern {
    Any,                           // Capture variable
    EachAny,                       // (x ...)
    Each(Box<Pattern>),            // #(each p)
    EachPlus {                     // #(each+ before mid* after)
        before: Vec<Pattern>,
        mid: Vec<Pattern>,
        after: Vec<Pattern>,
    },
    FreeId(ArenaIndex),            // Literal keyword
    Atom(ArenaIndex),              // Constant
    Vector(Box<Pattern>),          // #(p)
    Empty,                         // ()
}

impl Pattern {
    fn dispatch(
        &self,
        expr: ArenaIndex,
        lisp: &Lisp,
    ) -> Result<Option<Captures>, EvalError> {
        // Iterative dispatch to avoid stack overflow
        // Returns None if no match, Some(captures) on success
    }
}
```

**Benefits**:
- Support for `#(each+ ...)` mixed patterns
- Fixes nested ellipsis bug (documented issue)
- Clearer error messages on pattern mismatch

#### 1.2 Improved Hygiene Tracking

**Extract from**: `rebuild-macro-output` (lines 1394-1431)

**Adapt for Grift**:
```rust
// Add anti-mark support to Value::Syntax
pub enum Value {
    Syntax {
        expr: ArenaIndex,
        marks: ArenaIndex,     // Now includes anti-marks
        subst: ArenaIndex,
        anti_mark: bool,       // NEW: Flag for original input
    },
    // ...
}

impl Evaluator {
    fn rebuild_macro_output(
        &mut self,
        expr: ArenaIndex,
        new_mark: ArenaIndex,
    ) -> EvalResult {
        // Apply anti-mark cancellation logic
        // Preserve original bindings vs. introduce fresh
    }
}
```

**Benefits**:
- More robust hygiene (matches Scheme spec)
- Prevents accidental variable capture
- Better behavior with recursive macros

#### 1.3 Nested Ellipsis Support

**Extract from**: Template level tracking (lines 2060-2104)

**Adapt for Grift**:
```rust
// Track ellipsis nesting depth during template expansion
pub struct EllipsisLevel {
    depth: usize,
    bindings: Vec<ArenaIndex>,  // Captured values at this level
}

impl Evaluator {
    fn expand_template_with_levels(
        &mut self,
        template: ArenaIndex,
        levels: &[EllipsisLevel],
        current_depth: usize,
    ) -> EvalResult {
        // Recursively expand with depth tracking
        // Handle ((a ...) ...) correctly
    }
}
```

**Benefits**:
- Fixes documented Phase 7 bug
- Enables complex macros like `match`
- Maintains compatibility with existing simple cases

### Phase 2: Enhanced Error Reporting (1 week)

**Extract from**: `syntax-violation` (lines 3130-3140), source tracking

**Adapt for Grift**:
```rust
// Optional source metadata (no_std compatible)
pub struct SourceInfo {
    // Store in separate arena vector, indexed by ArenaIndex
    file: Option<&'static str>,  // Static string (no alloc)
    line: u32,
    column: u32,
}

impl EvalError {
    fn with_source(self, info: SourceInfo) -> Self {
        // Attach source info to error
        // Display in error messages for debugging
    }
}
```

**Benefits**:
- Better developer experience
- Easier macro debugging
- Matches Scheme implementations

### Phase 3: Testing & Validation (1-2 weeks)

**Test suite extraction**:
- Translate psyntax test cases to Grift format
- Focus on:
  - Hygiene edge cases
  - Nested ellipsis patterns
  - Complex macro interactions
- Ensure backward compatibility with existing macros

**Benchmarking**:
- Measure arena usage increase
- Profile expansion performance
- Validate no regressions in core evaluator

### Phase 4: Documentation (1 week)

**Update documents**:
- `HYGIENIC_MACROS_IMPLEMENTATION.md` - Add Phase 10: psyntax enhancements
- `EXTENDING_SCHEME_MACROS.md` - Document new pattern types
- `SCHEME_R7RS_CONFORMANCE.md` - Mark improved syntax-case coverage

**Add examples**:
- Showcase nested ellipsis
- Demonstrate improved hygiene
- Complex macro patterns now possible

---

## Implementation Roadmap

### Timeline: 5-7 Weeks

```
Week 1-2: Pattern Matching Enhancement
  - Implement Pattern enum
  - Port $sc-dispatch logic
  - Adapt to arena allocation
  - Unit tests for each pattern type

Week 3: Hygiene Improvements
  - Add anti-mark support
  - Implement rebuild-macro-output
  - Test hygiene edge cases

Week 4: Nested Ellipsis
  - Add level tracking to template expansion
  - Fix documented Phase 7 bug
  - Integration tests

Week 5: Error Reporting
  - Design SourceInfo structure
  - Integrate with error system
  - Update error messages

Week 6: Testing & Validation
  - Port psyntax test suite
  - Benchmark performance
  - Regression testing

Week 7: Documentation & Polish
  - Update all documentation
  - Code examples
  - Final review
```

### Priority Matrix

| Enhancement | Impact | Effort | Priority | Rationale |
|------------|--------|--------|----------|-----------|
| **Nested ellipsis** | HIGH | MEDIUM | **P0** | Fixes documented bug |
| **Enhanced patterns** | MEDIUM | HIGH | **P1** | Enables advanced macros |
| **Anti-mark hygiene** | MEDIUM | LOW | **P1** | Spec compliance |
| **Error reporting** | LOW | LOW | **P2** | Nice-to-have |
| **Source tracking** | LOW | MEDIUM | **P3** | Future work |

### Backward Compatibility

**Critical constraint**: All existing Grift macros must continue working.

**Strategy**:
- Add new pattern types alongside existing
- Default to current behavior
- Opt-in to new features via explicit patterns
- Extensive regression testing

---

## Testing Strategy

### Test Categories

#### 1. Unit Tests (Pattern Matching)

```scheme
;; Each+ pattern with nested ellipsis
(define-syntax complex-pattern
  (lambda (stx)
    (syntax-case stx ()
      ((_ a ... (b ...) ... c)
       ; Should capture:
       ;   a: (1 2 3)
       ;   b: ((4 5) (6 7))
       ;   c: 8
       #'(list 'a a ... 'b (list b ...) ... 'c c)))))

(test-equal
  (complex-pattern 1 2 3 (4 5) (6 7) 8)
  '(a 1 2 3 b (4 5) (6 7) c 8))
```

#### 2. Integration Tests (Hygiene)

```scheme
;; Anti-mark preserves original binding
(let ((x 1))
  (let-syntax ((m (lambda (stx)
                    (syntax-case stx ()
                      ((_ e) #'(let ((x 2)) e))))))
    (m (+ x 10))))
;; Expected: 11 (outer x), not 12 (inner x)
```

#### 3. Regression Tests

Run all existing Grift macro tests:
- Standard library macros (`macros.scm`)
- User-defined macros from examples
- Edge cases from Phase 1-9

#### 4. Benchmark Tests

```rust
#[bench]
fn bench_pattern_dispatch_simple(b: &mut Bencher) {
    // Measure simple pattern matching overhead
}

#[bench]
fn bench_pattern_dispatch_nested(b: &mut Bencher) {
    // Measure nested ellipsis performance
}

#[bench]
fn bench_macro_expansion_complex(b: &mut Bencher) {
    // End-to-end macro expansion
}
```

### Success Criteria

- ✅ All existing tests pass (100% regression-free)
- ✅ New nested ellipsis tests pass (Phase 7 bug fixed)
- ✅ Hygiene tests match R7RS spec behavior
- ✅ Performance within 10% of current implementation
- ✅ Arena usage increase < 20%

---

## Risk Assessment

### Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| **Arena exhaustion** | MEDIUM | HIGH | Add configurable depth limits; comprehensive testing |
| **Stack overflow** | LOW | HIGH | Trampoline all recursion; impose pattern depth limit |
| **Breaking changes** | MEDIUM | CRITICAL | Extensive regression testing; feature flags |
| **Performance regression** | LOW | MEDIUM | Benchmarking; optimize hot paths |
| **Complexity creep** | MEDIUM | MEDIUM | Code review; maintain simplicity principle |

### Project Risks

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| **Scope creep** | HIGH | MEDIUM | Strict adherence to selective enhancement strategy |
| **Timeline overrun** | MEDIUM | LOW | Phased approach; MVP first |
| **Maintenance burden** | MEDIUM | MEDIUM | Thorough documentation; clear code structure |

### Risk Mitigation: Feature Flags

Use Cargo features for gradual rollout:

```toml
[features]
default = ["enhanced-patterns"]
enhanced-patterns = []  # New pattern types
anti-mark-hygiene = []  # Anti-mark support
source-tracking = []    # Optional source info
```

---

## Appendices

### Appendix A: psyntax.scm Structure Reference

**Complete Section Breakdown**:

```
Section 1: Bootstrap & Infrastructure (lines 1-500)
├── eval-when compile: module setup
├── define-expansion-constructors: Tree-IL node constructors
├── define-expansion-accessors: Field accessors
├── simple-match: Pattern matching macro framework
├── top-level-eval / local-eval: Evaluator interface
├── global-extend: Module binding registration
└── sourcev utilities: Source location management

Section 2: Wraps & Hygiene Core (lines 501-1000)
├── make-wrap / wrap-marks / wrap-subst: Wrap constructors
├── make-ribcage / ribcage? / ribcage-*: Ribcage operations
├── same-marks?: Mark comparison for scope resolution
├── id-var-name: Identifier → label resolution
├── free-id=? / bound-id=?: Identifier equality predicates
└── join-wraps / extend-wrap: Wrap manipulation

Section 3: Expression Expansion Dispatcher (lines 1001-1500)
├── syntax-type: 23-value type classification
├── expand: Main expansion dispatcher
├── expand-macro: Macro invocation with anti-mark
├── expand-body: Two-pass body expansion (internal defines)
└── expand-local-syntax: let-syntax / letrec-syntax

Section 4: Core Form Transformers (lines 1501-2000)
├── expand-lambda: Lambda with formals validation
├── expand-lambda-case: Multi-case lambda (case-lambda)
├── expand-quote / expand-quote-syntax: Data quotation
├── expand-syntax: Template transcription with ellipsis
├── expand-if / expand-when / expand-unless: Conditionals
└── expand-let / expand-letrec*: Binding forms

Section 5: Pattern Matching Engine (lines 2001-2500)
├── expand-syntax-case: syntax-case dispatcher
├── convert-pattern: User pattern → internal pattern
├── $sc-dispatch: Core pattern matcher (8 pattern types)
├── ellipsis?: Check for ellipsis identifier
└── gen-clause: Generate output from matched pattern

Section 6: Public API & Standard Macros (lines 2501-3196)
├── sc-expand: Top-level expansion entry point
├── identifier? / syntax->datum / datum->syntax: Syntax API
├── free-identifier=? / bound-identifier=?: Public predicates
├── syntax-local-binding: Introspection API
├── with-syntax: Pattern variable binding macro
├── syntax-rules: Declarative macro definition
├── quasiquote: Template-based code generation
├── include / include-from-path: File inclusion
└── identifier-syntax / define*: Utility macros
```

### Appendix B: Grift Implementation Files Reference

**Current Macro System Files**:

```
crates/grift_eval/src/evaluator/
├── mod.rs              Main evaluator structure
│   ├── macro_env: ArenaIndex     Macro definitions
│   ├── gensym_counter: usize     Unique ID generation
│   └── methods: expand_macro, lookup_macro
│
├── core.rs             Main eval loop & continuations
│   ├── eval(): Macro expansion check
│   ├── define-syntax handler
│   └── Continuation dispatch
│
├── forms.rs            Special form handlers
│   ├── step_eval_syntax_case()   Pattern matching
│   ├── step_eval_syntax()        Template transcription
│   ├── step_eval_let_syntax()    Local macros
│   └── syntax object builders
│
└── expand.rs           Macro expansion engine
    ├── expand_macro()            Main expander
    ├── match_pattern()           Pattern matcher
    ├── expand_template()         Template expander
    ├── eval_for_macro_expansion() Procedural macro eval
    └── Pattern/Template structs

crates/grift_parser/src/
├── value.rs            Value types
│   └── Value::Syntax { expr, marks, subst }
│
└── lisp.rs             Lisp primitives
    ├── syntax()         Syntax constructor
    ├── syntax_parts()   Syntax accessor
    └── eqv()            Equality check

crates/grift_eval/src/evaluator/macros.scm
└── Standard macro library (642 lines)
```

### Appendix C: Algorithm Complexity Comparison

| Operation | psyntax | Grift Current | Proposed |
|-----------|---------|---------------|----------|
| **Pattern match** | O(n * m) where n=input, m=pattern | O(n * m) | O(n * m) |
| **Identifier resolution** | O(w * r) where w=wrap depth, r=ribcage size | O(s) where s=subst length | O(w * s) |
| **Template expansion** | O(t * b) where t=template, b=bindings | O(t * b) | O(t * b * d) for depth d |
| **Wrap joining** | O(w1 + w2) | N/A (no wraps) | N/A |
| **Macro expansion** | O(1) hash lookup | O(n) alist scan | O(n) alist scan |

**Recommendation**: For production use, consider hash table for macro_env (acceptable in `std` mode, trade-off in `no_std`).

### Appendix D: Memory Budget Analysis

**Current Grift Syntax Object**:
```
Value::Syntax {
    expr: 8 bytes (ArenaIndex)
    marks: 8 bytes (ArenaIndex → list)
    subst: 8 bytes (ArenaIndex → alist)
}
= 24 bytes per syntax object
```

**With enhancements**:
```
Value::Syntax {
    expr: 8 bytes
    marks: 8 bytes
    subst: 8 bytes
    anti_mark: 1 byte (bool)
    padding: 7 bytes
}
= 32 bytes per syntax object (+33%)

Alternative (no padding):
Store anti_mark in marks list as sentinel
= 24 bytes (no increase)
```

**Recommendation**: Use sentinel approach to avoid size increase.

### Appendix E: Glossary

| Term | Definition |
|------|------------|
| **Anti-mark** | Special mark added to macro input; cancels with output mark to preserve original bindings |
| **Arena** | Fixed-size memory pool for allocation without garbage collection |
| **Continuation** | Representation of remaining computation; enables trampolining |
| **Ellipsis** | Pattern/template repetition operator `...` |
| **Hygiene** | Macro property preventing accidental variable capture |
| **Label** | Unique identifier for a binding (psyntax); replaces symbol for scope tracking |
| **Mark** | Scope identifier added during macro expansion for hygiene |
| **Pattern variable** | Variable in macro pattern that captures matched sub-expressions |
| **Ribcage** | Mapping from symbols to labels within a scope (psyntax) |
| **Substitution** | Environment mapping pattern variables to captured values (Grift) |
| **Syntax object** | Wrapper around expression with scope/hygiene metadata |
| **Trampoline** | Technique to convert recursion to iteration using explicit stack |
| **Transformer** | Procedure that transforms macro use into expansion |
| **Wrap** | Two-level scope representation: marks + substitutions (psyntax) |

### Appendix F: Further Reading

**Academic Papers**:
- Dybvig, Hieb, Bruggeman. "Syntax Abstraction in Scheme" (1992)
  - Original psyntax algorithm
  - <http://www.cs.indiana.edu/~dyb/pubs/LaSC-5-4-pp295-326.pdf>

- Dybvig. "The Scheme Programming Language, 4th Ed." (2009)
  - Chapter 8: Syntactic Extension
  - Comprehensive syntax-case reference

- Kohlbecker et al. "Hygienic Macro Expansion" (1986)
  - Foundational hygiene algorithm
  - LFP '86 proceedings

**Implementation References**:
- Guile manual: <https://www.gnu.org/software/guile/manual/html_node/Macros.html>
- Racket Guide: <https://docs.racket-lang.org/guide/pattern-macros.html>
- Chez Scheme: <https://cisco.github.io/ChezScheme/csug9.5/syntax.html>

**Grift Internal Docs**:
- `docs/HYGIENIC_MACROS_IMPLEMENTATION.md` - Phase 1-9 implementation log
- `docs/EXTENDING_SCHEME_MACROS.md` - Advanced macro patterns
- `docs/ARENA_ARCHITECTURE.md` - Memory management details
- `docs/LISP_ARCHITECTURE.md` - Evaluator design

---

## Conclusion

### Summary of Recommendations

1. **DO NOT** attempt a full port of psyntax.scm (3,196 lines) to Grift
   - Fundamental architectural incompatibilities
   - Excessive complexity for embedded use case
   - Module system dependencies not applicable

2. **DO** selectively extract these enhancements:
   - ✅ `$sc-dispatch` pattern matching algorithm
   - ✅ Anti-mark hygiene mechanism
   - ✅ Nested ellipsis level tracking
   - ✅ Improved error reporting

3. **MAINTAIN** these Grift design principles:
   - ✅ `no_std`, `no_alloc` compatibility
   - ✅ Fixed arena allocation model
   - ✅ Continuation-based evaluation
   - ✅ Minimalist, embedded-friendly approach

4. **PRIORITIZE** backward compatibility:
   - ✅ Extensive regression testing
   - ✅ Feature flags for gradual rollout
   - ✅ Performance benchmarking

### Expected Outcomes

After implementing the recommended selective enhancements:

- **Functionality**: Nested ellipsis bug fixed; advanced macro patterns enabled
- **Compatibility**: 100% backward compatible with existing Grift code
- **Performance**: <10% overhead; <20% arena usage increase
- **Maintainability**: Clear code structure with comprehensive documentation
- **Spec Compliance**: Closer alignment with R7RS syntax-case behavior

### Final Assessment

The psyntax.scm file serves as an **excellent reference** for understanding the full R6RS/R7RS macro system, but a **selective enhancement approach** is the pragmatic strategy for Grift. This balances improved functionality with maintaining Grift's core design principles and constraints.

**Total effort**: 5-7 weeks for phased implementation
**Risk level**: Medium (mitigated by testing strategy)
**Value proposition**: High (fixes known bugs, enables advanced macros)

---

**Document Version**: 1.0  
**Last Updated**: 2026-02-04  
**Author**: Grift Development Team  
**Status**: Ready for Review
