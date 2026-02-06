# Grift Code Review & Architecture Analysis

Comprehensive analysis of the Grift codebase covering architecture, performance, memory management, and code quality. Suggestions are prioritized by impact (🔴 High, 🟡 Medium, 🟢 Low).

## Table of Contents

- [1. Architecture & Design](#1-architecture--design)
  - [1.1 Module Organization](#11-module-organization)
  - [1.2 Separation of Concerns](#12-separation-of-concerns)
  - [1.3 Design Patterns](#13-design-patterns)
  - [1.4 AST Representation](#14-ast-representation)
- [2. Performance Optimizations](#2-performance-optimizations)
  - [2.1 Allocation Hotspots](#21-allocation-hotspots)
  - [2.2 Recursive Implementations & TCO](#22-recursive-implementations--tco)
  - [2.3 Environment & Symbol Table Lookup](#23-environment--symbol-table-lookup)
  - [2.4 Unnecessary Indirection](#24-unnecessary-indirection)
- [3. Memory Management](#3-memory-management)
  - [3.1 Lifetime Annotations & Borrowing](#31-lifetime-annotations--borrowing)
  - [3.2 Potential Leaks & Reference Cycles](#32-potential-leaks--reference-cycles)
  - [3.3 Garbage Collection Strategy](#33-garbage-collection-strategy)
- [4. Code Quality](#4-code-quality)
  - [4.1 Idiomatic Rust Patterns](#41-idiomatic-rust-patterns)
  - [4.2 Error Handling](#42-error-handling)
  - [4.3 Type System Improvements](#43-type-system-improvements)
  - [4.4 Visibility & API Design](#44-visibility--api-design)
  - [4.5 Dead Code & Unused Dependencies](#45-dead-code--unused-dependencies)
  - [4.6 Macro Expansion Improvements](#46-macro-expansion-improvements)
  - [4.7 Lisp-Specific Optimizations](#47-lisp-specific-optimizations)

---

## 1. Architecture & Design

### 1.1 Module Organization

**Current Structure:**

```
crates/
├── grift_arena/       # Arena allocator + mark-and-sweep GC
├── grift_parser/      # Lexer, parser, symbol interning, Value types
├── grift_eval/        # Trampolined evaluator, builtins, special forms
├── grift_repl/        # Interactive REPL (only crate using std)
├── grift_macros/      # Proc macros for include_stdlib!
├── grift/             # Unified re-exports
└── grift_arena_embedded/ # Hardware-specific arena access
```

**Dependency Graph:**

```
grift_arena ─────────────────────────────────┐
    │                                        │
grift_macros                                 │
    │                                        │
grift_parser ← (arena, macros)              │
    │                                        │
grift_eval ← (arena, parser)               │
    │                                        │
grift_repl ← (arena, parser, eval, embedded)│
    │                                        │
grift ← (arena, parser, eval, repl?)       ─┘
```

**Assessment:** ✅ The dependency graph is clean and acyclic. The layered architecture enforces proper abstraction boundaries — the arena knows nothing about Lisp, the parser knows nothing about evaluation, and only the REPL depends on `std`.

#### 🟡 Suggestion: Separate `Value` types from `grift_parser`

The `grift_parser` crate currently owns the `Value` enum, `Builtin` enum, `StdLib` enum, and the `Lisp` context. This means the parser crate defines runtime types (closures, continuations, native functions) that are only meaningful during evaluation.

Consider extracting a `grift_types` or `grift_core` crate:

```
grift_core/    # Value, Builtin, StdLib, Lisp context
grift_parser/  # Only lexing + parsing, produces ArenaIndex AST nodes
grift_eval/    # Evaluator, uses grift_core types
```

This would make `grift_parser` a pure parsing crate and keep runtime concerns in a shared types crate.

### 1.2 Separation of Concerns

**Assessment:** ✅ Generally strong separation, with a few blurred boundaries.

| Boundary | Status | Notes |
|----------|--------|-------|
| Arena ↔ Parser | ✅ Clean | Arena is generic, parser uses it via `Lisp` wrapper |
| Lexer ↔ Parser | 🟡 Merged | Both in `parser.rs` — single `Parser` struct handles tokenization and tree building |
| Parser ↔ Evaluator | ✅ Clean | Parser produces arena indices, evaluator consumes them |
| Evaluator ↔ Builtins | ✅ Clean | `builtins.rs` handles dispatch, `core.rs` handles evaluation loop |
| Evaluator ↔ Special Forms | ✅ Clean | `forms.rs` handles continuation return for special forms |
| Evaluator ↔ Macros | ✅ Clean | `expand.rs` handles macro expansion, integrated via `CONT_MACRO_RESULT` |

#### 🟢 Suggestion: Extract a dedicated lexer/tokenizer

The parser combines tokenization and tree-building in a single pass. While efficient, extracting a `Tokenizer` or `Lexer` struct that produces a token stream would improve testability and enable future features (e.g., syntax highlighting, incremental parsing):

```rust
// Conceptual separation
pub struct Lexer<'a> { input: &'a [u8], pos: usize, line: usize, col: usize }
pub enum Token { LParen, RParen, Symbol(&'a [u8]), Number(isize), String, ... }

pub struct Parser<'a, const N: usize> {
    lexer: Lexer<'a>,
    lisp: &'a Lisp<N>,
}
```

In a `no_std` context, this would need to avoid allocating a token buffer — the lexer could be an iterator yielding tokens on demand.

### 1.3 Design Patterns

**Patterns Currently Used:**

| Pattern | Where | Assessment |
|---------|-------|------------|
| Arena/Pool allocator | `grift_arena` | ✅ Excellent fit for Lisp runtime |
| Interior mutability (Cell) | `Arena<T, N>` | ✅ Avoids RefCell overhead |
| Trampoline/CPS | `grift_eval` | ✅ Eliminates stack overflow |
| Free-list allocator | `Arena::alloc` | ✅ O(1) allocation |
| Symbol interning | `Lisp::intern` | ✅ Identity-based comparison |
| Reserved slots | `Lisp::new()` | ✅ Avoids repeated allocation of singletons |

#### 🔴 Suggestion: Replace continuation dispatch with a table-driven approach

The evaluator's `step_return()` function in `forms.rs` is a massive match statement over 41 `CONT_*` constants. This can be restructured as a dispatch table:

```rust
type ContHandler<const N: usize> = fn(
    &mut Evaluator<N>,
    val: ArenaIndex,
    data: ArenaIndex,
    env: ArenaIndex,
) -> Result<Option<TrampolineState>, EvalError>;

const CONT_HANDLERS: &[ContHandler<N>] = &[
    handle_cont_done,          // 0
    handle_cont_apply_forced,  // 1
    handle_cont_if_branch,     // 2
    // ...
];

fn step_return(&mut self, val: ArenaIndex) -> Result<...> {
    let (cont_type, data, env) = self.pop_cont()?;
    if cont_type < CONT_HANDLERS.len() {
        CONT_HANDLERS[cont_type](self, val, data, env)
    } else {
        Err(EvalError::unknown_continuation(cont_type))
    }
}
```

**Benefits:** Eliminates the large match, enables O(1) dispatch, and makes adding new continuation types easier. Note: Rust's const generics make function pointer arrays with `<const N: usize>` tricky — this may require a macro to generate the table for a specific `N`.

#### 🟡 Suggestion: Builder pattern for continuation construction

Continuation packing (`pack2`, `pack3`, `push_cont`) is repeated hundreds of times with inline comments describing the data layout. A builder would make intent clearer and reduce errors:

```rust
// Current (repeated ~150 times in forms.rs/core.rs):
let data = self.lisp.pack3(then_expr, else_expr, env)?;
self.push_cont(CONT_IF_BRANCH, data, env)?;

// Proposed:
self.cont()
    .if_branch(then_expr, else_expr)
    .in_env(env)
    .push()?;
```

### 1.4 AST Representation

**Current Design:** The `Value` enum uses a fixed-size representation where every variant fits within two `ArenaIndex` fields plus a discriminant:

```rust
pub enum Value {
    Nil, Void, True, False,        // Zero-field singletons
    Number(isize),                  // Inline integer
    Char(char),                     // Inline character
    Cons { car: ArenaIndex, cdr: ArenaIndex },
    Symbol(ArenaIndex),
    Lambda { params: ArenaIndex, body_env: ArenaIndex },
    Array { len: usize, data: ArenaIndex },
    String { len: usize, data: ArenaIndex },
    // ... ~16 total variants
}
```

**Assessment:** ✅ This is a well-designed tagged union. Inlining `car`/`cdr` directly in `Cons` (rather than pointing to separate slots) halves the allocation cost for pairs — the fundamental building block of Lisp.

#### 🟡 Suggestion: Consider NaN-boxing for number-heavy workloads

If the runtime needs to support floating-point numbers in the future, NaN-boxing can encode both floats and tagged pointers in a single 64-bit word, eliminating the need for the enum discriminant overhead entirely. This is a common optimization in production Lisp/JS runtimes.

#### 🟢 Suggestion: Compress singleton variants

`Nil`, `Void`, `True`, and `False` are pre-allocated in reserved arena slots (0–3). The `Value` enum still has variants for them, which costs discriminant space. If `Value` were restructured to use a single `Immediate(u8)` variant for all singletons, the enum could potentially shrink:

```rust
pub enum Value {
    Immediate(u8),  // 0=Nil, 1=Void, 2=True, 3=False
    Number(isize),
    Cons { car: ArenaIndex, cdr: ArenaIndex },
    // ...
}
```

This is a minor optimization since `Value` is `Copy` and already fits in a few words, but it reduces variant count and simplifies pattern matching for truthiness checks.

---

## 2. Performance Optimizations

### 2.1 Allocation Hotspots

#### 🔴 Environment extension allocates a cons cell per binding

Every `env_extend` call allocates a new `Cons` cell in the arena:

```rust
// crates/grift_eval/src/evaluator/core.rs
pub(super) fn env_extend(&mut self, env: ArenaIndex, name: ArenaIndex, value: ArenaIndex)
    -> Result<ArenaIndex, EvalError>
{
    let binding = self.lisp.cons(name, value)?;   // 1 allocation
    let new_env = self.lisp.cons(binding, env)?;   // 1 allocation
    Ok(new_env)
}
```

For a function call with `n` arguments, this allocates `2n` arena slots just for the environment. In tight loops (e.g., recursive fibonacci), this creates significant allocation pressure.

**Mitigation:** Consider a bulk `env_extend_many` that allocates all bindings contiguously:

```rust
pub(super) fn env_extend_many(
    &mut self, env: ArenaIndex, bindings: &[(ArenaIndex, ArenaIndex)]
) -> Result<ArenaIndex, EvalError> {
    let mut current_env = env;
    for &(name, value) in bindings {
        current_env = self.env_extend(current_env, name, value)?;
    }
    Ok(current_env)
}
```

Or even better, use contiguous allocation for the binding list to improve cache locality.

#### 🟡 Contiguous allocation uses first-fit scan

`alloc_contiguous(count)` in `arena.rs` performs a linear scan for consecutive free slots. For large arrays or strings this can be O(N) where N is arena capacity. Consider maintaining a secondary free list for contiguous blocks or using a buddy allocator for bulk allocations.

### 2.2 Recursive Implementations & TCO

**Assessment:** ✅ The evaluator already implements full trampolining — there is zero Rust stack recursion in the evaluation loop. This is a major architectural win.

**Trampoline loop** (`core.rs`):
```rust
fn trampoline(&mut self, initial: TrampolineState) -> EvalResult {
    let mut state = initial;
    let mut step_count: usize = 0;
    loop {
        step_count = step_count.wrapping_add(1);
        // Periodic GC check
        state = match state {
            TrampolineState::Eval { expr, env } => self.step_eval(expr, env)?,
            TrampolineState::Return { val } => match self.step_return(val)? {
                Some(next) => next,
                None => return Ok(val),
            },
        };
    }
}
```

#### 🟡 Suggestion: Optimize GC check interval

The current GC trigger uses modular arithmetic every 500 steps:

```rust
if step_count % GC_CHECK_INTERVAL == 0 {
    let stats = self.lisp.arena_stats();
    if stats.allocated >= stats.capacity * GC_THRESHOLD_PERCENT / 100 {
        self.gc_with_state(&state);
    }
}
```

**Issues:**
- `%` (modulo) on every iteration is a division — use bitwise AND with a power-of-2 interval instead (e.g., `step_count & 511 == 0` for 512-step intervals).
- The 60% threshold is static. Consider adaptive thresholds that increase after successful GC cycles with low collection rates.

```rust
// Faster check with power-of-2 interval
const GC_CHECK_MASK: usize = 511; // 512-step interval
if step_count & GC_CHECK_MASK == 0 {
    // ...
}
```

### 2.3 Environment & Symbol Table Lookup

#### 🔴 Linear environment lookup is O(n) per variable access

The environment is an association list (linked list of `(name . value)` pairs). Every variable lookup walks the entire chain:

```rust
// crates/grift_eval/src/evaluator/core.rs
pub(super) fn env_lookup(&self, env: ArenaIndex, name: ArenaIndex) -> EvalResult {
    let mut current = env;
    loop {
        match self.lisp.get(current)? {
            Value::Cons { car, cdr } => {
                if let Value::Cons { car: bound_name, cdr: bound_value } = self.lisp.get(car)?
                    && self.lisp.symbol_eq(bound_name, name)?
                {
                    return Ok(bound_value);
                }
                current = cdr;
            }
            _ => break,  // Fall through to global lookup
        }
    }
    // Global environment lookup (another O(n) scan)
    self.global_env_lookup(name)
}
```

**Impact:** In deeply nested scopes or long `let*` chains, each variable reference pays O(depth) cost. For loops or recursive functions, this compounds significantly.

**Mitigation strategies (in order of complexity):**

1. **Most-recently-used cache:** Cache the last few successful lookups in a small fixed-size array. Lisp programs tend to access the same few variables repeatedly in tight loops.

2. **Indexed environments:** Instead of alist chains, use contiguous array segments in the arena. Compile-time analysis can assign each variable a numeric index within its scope, enabling O(1) lookup:

   ```rust
   // Instead of (name . value) alist:
   // Environment = (frame_ptr . parent_env)
   // Frame = contiguous arena slots [val0, val1, val2, ...]
   // Lookup = arena.get(frame_base + var_index)
   ```

3. **Hash-based environment:** Use the existing arena to store a simple open-addressing hash table for each scope frame. This trades some allocation overhead for O(1) amortized lookup.

#### 🟡 Symbol interning is also an alist scan

The intern table stores `(string_idx . symbol_idx)` pairs as an alist. Every `intern()` call walks this list to check for an existing symbol. For programs with many distinct symbols, this becomes a bottleneck.

Consider a hash-table-based intern table using the arena. Since symbols are never garbage collected (the intern table is always a GC root), this table would only grow.

### 2.4 Unnecessary Indirection

#### 🟡 Native function lookup ignores stored hash

The `Value::Native { id, name_hash }` variant stores a precomputed name hash, but `NativeRegistry::lookup()` performs a linear string comparison scan:

```rust
// crates/grift_eval/src/native.rs
pub fn lookup(&self, name: &str) -> Option<NativeFn<N>> {
    for entry in &self.entries[..self.count] {
        if let Some(e) = entry {
            if e.name == name {        // String comparison
                return Some(e.func);
            }
        }
    }
    None
}
```

The `name_hash` field should be used for fast rejection:

```rust
pub fn lookup(&self, name: &str) -> Option<NativeFn<N>> {
    let target_hash = Self::hash_name(name);
    for entry in &self.entries[..self.count] {
        if let Some(e) = entry {
            if e.name_hash == target_hash && e.name == name {
                return Some(e.func);
            }
        }
    }
    None
}
```

#### 🟢 Double arena reads in hot paths

Several hot paths perform two sequential `arena.get()` calls where one could suffice. For example, `env_lookup` reads the cons cell, then reads the car of the cons cell. While each `get()` is O(1), the Cell-based access pattern prevents the compiler from optimizing across calls. Consider helper methods like `get_car_cdr()` that return both in one call.

---

## 3. Memory Management

### 3.1 Lifetime Annotations & Borrowing

**Assessment:** ✅ The codebase makes excellent use of Rust's lifetime system.

**Key patterns:**

- **`Cell<Slot<T>>` for interior mutability:** The arena uses `Cell` instead of `RefCell`, avoiding runtime borrow checking overhead. This is possible because `T: Copy`.

- **Parser borrows input:** `Parser<'a>` borrows the input byte slice, avoiding copies.

- **Evaluator borrows Lisp context:** `Evaluator` owns the `Lisp` context, keeping lifetime management simple — no dangling references.

- **No `'static` lifetime abuse:** All lifetime parameters are properly scoped.

#### 🟢 Suggestion: Consider `#[inline]` on arena access methods

`Arena::get()` and `Arena::set()` are the hottest functions in the runtime. While the compiler likely inlines them, explicit `#[inline]` annotations guarantee it across crate boundaries:

```rust
// crates/grift_arena/src/arena.rs
#[inline]
pub fn get(&self, index: ArenaIndex) -> Result<T, ArenaError> { ... }

#[inline]
pub fn set(&self, index: ArenaIndex, value: T) -> Result<(), ArenaError> { ... }
```

The `Lisp` wrapper in `grift_parser` already uses `#[inline]` on its delegation methods (`nil()`, `car()`, `cdr()`), but the underlying `Arena` methods should also be annotated.

### 3.2 Potential Leaks & Reference Cycles

**Assessment:** ✅ The arena-based design inherently prevents traditional memory leaks and reference cycles.

**Why cycles are safe:**

- All values are stored in a fixed-size arena. Even if Scheme code creates circular structures (`(let ((x (list 1))) (set-cdr! x x))`), the arena simply holds the indices.
- The mark-and-sweep GC correctly handles cycles — it marks reachable objects from roots, and unreachable cyclic structures are swept regardless of internal references.

**Potential concern: Intern table growth**

The intern table is always a GC root, so interned symbols are never collected. In long-running programs that dynamically create symbols (e.g., via `string->symbol`), the intern table grows monotonically. This is standard behavior for Lisp implementations but could exhaust arena space in embedded contexts with small arenas.

#### 🟢 Suggestion: Optional weak interning

For embedded use cases, consider a mode where rarely-used symbols can be garbage collected. This would require removing them from the intern table during GC sweep if they have no other references.

### 3.3 Garbage Collection Strategy

**Current Implementation:** Mark-and-sweep with:

- **Periodic trigger:** Every 500 evaluation steps, check if allocation exceeds 60% capacity
- **Reactive trigger:** On `OutOfMemory` error, run GC and retry the allocation
- **Roots:** Current continuation stack, eval state, global environment, intern table
- **Mark phase:** Iterative depth-first traversal with 16-element batch processing
- **Sweep phase:** Single linear pass freeing unmarked slots

**Assessment:** ✅ The GC is well-designed for the `no_std` constraint. The iterative mark phase (instead of recursive) prevents stack overflow. The batch processing in the mark phase is a good compromise between memory usage and throughput.

#### 🔴 Suggestion: GC root tracking is fragile

The `gc_with_state()` method manually extracts roots from the current trampoline state:

```rust
fn gc_with_state(&mut self, state: &TrampolineState) {
    let mut roots = [self.lisp.nil_index(), ...; MAX_ROOTS];
    // Manually add: global_env, current_cont, intern_table, state.expr/env/val
}
```

If a new field is added to `Evaluator` or `TrampolineState` without updating `gc_with_state`, live objects could be incorrectly collected. Consider a `GcRoots` trait:

```rust
trait GcRoots {
    fn trace_roots<F: FnMut(ArenaIndex)>(&self, tracer: F);
}

impl<const N: usize> GcRoots for Evaluator<N> {
    fn trace_roots<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        tracer(self.global_env);
        tracer(self.current_cont);
        tracer(self.macro_env);
        // ... all roots in one place
    }
}
```

#### 🟡 Suggestion: Increase mark-phase batch size or make it dynamic

The 16-element batch in the mark phase means objects with many children (large arrays, deep cons chains) require multiple re-traces. Consider increasing to 64 or making the batch size proportional to arena capacity:

```rust
// Instead of fixed 16:
const MARK_BATCH_SIZE: usize = if N > 4096 { 64 } else { 16 };
```

#### 🟡 Suggestion: Generational collection for long-running programs

For the REPL or long-running embedded applications, a simple two-generation scheme could reduce GC pause times:

- **Nursery:** Recently allocated objects (high mortality rate)
- **Tenured:** Objects that survived N collections

This is a significant undertaking but would improve throughput for interactive use.

---

## 4. Code Quality

### 4.1 Idiomatic Rust Patterns

**Assessment:** ✅ The codebase is generally idiomatic, with strong use of:

- `Result<T, E>` for all fallible operations
- `?` operator for error propagation
- Pattern matching (including `if let` chains)
- `const` generics for arena sizing
- `#![no_std]` with `core::` imports
- `Copy` semantics for arena values
- `#![forbid(unsafe_code)]` on all crates

#### 🟡 Suggestion: Use `core::num::NonZeroUsize` for `ArenaIndex`

`ArenaIndex(usize)` uses slot 0 as `NIL`. Using `NonZeroUsize` for non-nil indices would enable niche optimization in `Option<ArenaIndex>`, making `Option<ArenaIndex>` the same size as `ArenaIndex`:

```rust
// Current: Option<ArenaIndex> = 16 bytes (8 + discriminant + padding)
// With NonZero: Option<ArenaIndex> = 8 bytes (0 represents None)
```

This requires careful handling since NIL maps to slot 0, but the optimization is valuable for types that store optional indices.

#### 🟢 Suggestion: Use `core::fmt::Display` for `Value` formatting

The REPL has custom `format_value()` functions. Implementing `Display` for `Value` (in a wrapper that carries the arena context) would enable standard formatting:

```rust
pub struct DisplayValue<'a, const N: usize> {
    value: ArenaIndex,
    lisp: &'a Lisp<N>,
}

impl<const N: usize> core::fmt::Display for DisplayValue<'_, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // ... formatting logic
    }
}
```

### 4.2 Error Handling

**Assessment:** ✅ Error handling is solid with dedicated error types per crate.

**Error hierarchy:**

```
ArenaError (grift_arena)
  ├── OutOfMemory
  ├── InvalidIndex
  └── TraceError

ParseError (grift_parser)
  ├── UnexpectedEof, UnexpectedChar, UnmatchedParen
  ├── NumberOverflow, OutOfMemory (wraps ArenaError)
  ├── InvalidHashLiteral, InvalidCharLiteral
  └── UnterminatedString, VectorLiteralTooLarge

EvalError (grift_eval)
  ├── ArenaError (wraps ArenaError)
  ├── TypeError, UndefinedVariable, NotCallable
  ├── InvalidForm, WrongArgCount, DivisionByZero
  └── ParseError (wraps ParseError)
```

#### 🟡 Suggestion: Reduce `EvalError` size

`EvalError` contains multiple optional fields (`message`, `expected`, `got`, `arg_info`, `parse_error`) most of which are `None` for any given error. This makes each error ~88+ bytes. Consider using a more compact representation:

```rust
pub enum EvalErrorDetail {
    Simple(&'static str),
    Type { expected: &'static str, got: &'static str },
    ArgCount(ArgCountInfo),
    Parse(ParseError),
}

pub struct EvalError {
    pub kind: ErrorKind,
    pub expr: ArenaIndex,
    pub detail: EvalErrorDetail,
}
```

This reduces the common case (simple errors) to ~32 bytes.

#### 🟢 Suggestion: Add `#[cold]` to error construction paths

Error paths are rarely taken. Marking them `#[cold]` helps the compiler optimize the hot path:

```rust
#[cold]
pub fn type_error(expr: ArenaIndex, expected: &'static str, got: &'static str) -> EvalError {
    // ...
}
```

### 4.3 Type System Improvements

#### 🟡 Suggestion: Use newtypes for environment and expression indices

Currently, environments, expressions, and values are all `ArenaIndex`. This allows accidental misuse (passing an environment where an expression is expected). Newtypes would catch these at compile time:

```rust
#[derive(Copy, Clone)]
pub struct EnvRef(ArenaIndex);

#[derive(Copy, Clone)]
pub struct ExprRef(ArenaIndex);

impl Evaluator {
    fn env_lookup(&self, env: EnvRef, name: ArenaIndex) -> EvalResult { ... }
    fn step_eval(&mut self, expr: ExprRef, env: EnvRef) -> ... { ... }
}
```

This is a large refactor but would eliminate an entire class of bugs. The newtypes can `Deref` to `ArenaIndex` for backward compatibility.

#### 🟢 Suggestion: Typed continuation constants

The `CONT_*` constants are `usize` values. Using an enum would provide exhaustiveness checking:

```rust
#[repr(usize)]
pub enum ContType {
    Done = 0,
    ApplyForced = 1,
    IfBranch = 2,
    // ...
}
```

This ensures the match in `step_return()` covers all cases and makes adding new continuations compiler-checked.

### 4.4 Visibility & API Design

**Assessment:** 🟡 Mixed visibility practices.

**Findings:**

| Crate | Public API | Internal visibility | Assessment |
|-------|-----------|-------------------|------------|
| `grift_arena` | 46 pub functions | Flat — everything `pub` | ⚠️ Over-exposed |
| `grift_parser` | Well-structured | `pub(crate)` on internals | ✅ Good |
| `grift_eval` | `pub(super)` on evaluator methods | Clean module hierarchy | ✅ Good |
| `grift_repl` | Minimal pub API | Most functions private | ✅ Good |

#### 🟡 Suggestion: Audit `grift_arena` public API

The arena exposes 46 public functions. Many are utility methods (`find`, `any`, `all`, `count_where`, `for_each`) that may not be needed by downstream crates. Consider:

```rust
// Keep pub: alloc, dealloc, get, set, collect_garbage, stats, validate
// Make pub(crate): find, any, all, count_where, for_each, allocated_indices
```

This follows the principle of minimal public API surface.

### 4.5 Dead Code & Unused Dependencies

**Findings:**

- **3 `#[allow(dead_code)]` annotations** in `grift_eval` (intentional, documented)
- **0 `todo!()` or `unimplemented!()`** in production code
- **0 `clone()` calls** in core crates (excellent for a `Copy`-based architecture)
- **0 `unsafe` blocks** (enforced by `#![forbid(unsafe_code)]`)
- **~16 `unwrap()` calls** in non-test code (mostly in REPL/CLI where panics are acceptable)

#### 🟢 Suggestion: Replace `unwrap()` in parser/eval with proper error handling

While the REPL can reasonably panic, any `unwrap()` in `grift_parser` or `grift_eval` should be replaced with `?` or explicit error handling to prevent panics in library usage:

```rust
// Before:
let value = self.lisp.get(idx).unwrap();

// After:
let value = self.lisp.get(idx)?;
```

### 4.6 Macro Expansion Improvements

#### 🟡 `include_stdlib!` proc macro parses Scheme at compile time

The `grift_macros` crate includes a Scheme parser that runs during compilation to transform `.scm` files into Rust enum variants. This duplicates parsing logic from `grift_parser`.

**Current approach:** `include_stdlib!` reads `.scm` files, strips comments, extracts `define` forms, and converts names (e.g., `set-car!` → `SetCar`).

**Assessment:** This is reasonable for `no_std` — the stdlib definitions need to be embedded as static data. However:

1. The comment-stripping logic in the proc macro is simpler than the parser's and could miss edge cases (e.g., `#|...|#` block comments, `#;` datum comments).
2. The name conversion (`scheme-name` → `RustName`) is duplicated between the proc macro and runtime dispatch.

#### 🟢 Suggestion: Share name conversion logic

Extract the `scheme-name` → `RustName` conversion into a shared utility (possibly a build script or a small `grift_util` crate) to avoid drift between compile-time and runtime name resolution.

#### 🟢 Suggestion: Deduplicate bounds-checking in arena macros

`impl_get_contiguous!` and `impl_set_contiguous!` both contain identical `@max` helper rules. Extract the bounds check into a shared macro:

```rust
macro_rules! arena_bounds_check {
    ($self:expr, $base:expr, $max_offset:expr) => {
        let end = $base.checked_add($max_offset)
            .ok_or(ArenaError::InvalidIndex)?;
        if end >= $self.capacity() {
            return Err(ArenaError::InvalidIndex);
        }
    };
}
```

### 4.7 Lisp-Specific Optimizations

#### 🔴 Suggestion: Optimize common special forms

**`if` expressions** currently push a continuation frame for every branch. For the common case where both branches are self-evaluating (constants), this overhead is unnecessary:

```rust
// Fast path for (if cond #t #f) or (if cond 1 2)
if is_self_evaluating(then_expr) && is_self_evaluating(else_expr) {
    // Evaluate condition, then directly return the appropriate value
    // No continuation frame needed
}
```

#### 🟡 Suggestion: Inline common builtins

Frequently called builtins like `car`, `cdr`, `cons`, `null?`, and `+` could be recognized during evaluation and dispatched directly without going through the generic builtin argument-collection mechanism:

```rust
// In step_eval_list, before generic function application:
if let Value::Symbol(name) = self.lisp.get(head)? {
    if let Some(builtin) = self.recognize_inline_builtin(name, arg_count) {
        return self.apply_inline_builtin(builtin, args, env);
    }
}
```

#### 🟡 Suggestion: Proper tail calls in `begin` blocks

Verify that the last expression in a `begin` block is evaluated in tail position (without pushing a new continuation). The current implementation uses `CONT_BEGIN_SEQ` — confirm that when the remaining expression list is empty, the trampoline transitions directly to `Eval` without a new continuation push.

#### 🟢 Suggestion: Constant folding

Expressions like `(+ 1 2)` where all arguments are compile-time constants could be folded during parsing or macro expansion. This is especially valuable for stdlib definitions.

#### 🟢 Suggestion: Symbol lookup caching per scope

Since environments are immutable (new bindings create new frames), a scope can cache its most recently looked-up binding. This turns repeated access to the same variable from O(n) to O(1) after the first lookup.

---

## Summary: Prioritized Recommendations

### 🔴 High Impact

| # | Recommendation | Section | Effort |
|---|---------------|---------|--------|
| 1 | Optimize environment lookup (indexed environments or caching) | [2.3](#23-environment--symbol-table-lookup) | High |
| 2 | Table-driven continuation dispatch | [1.3](#13-design-patterns) | Medium |
| 3 | GC root tracking via trait | [3.3](#33-garbage-collection-strategy) | Low |
| 4 | Fast-path for common special forms | [4.7](#47-lisp-specific-optimizations) | Medium |

### 🟡 Medium Impact

| # | Recommendation | Section | Effort |
|---|---------------|---------|--------|
| 5 | Power-of-2 GC check interval | [2.2](#22-recursive-implementations--tco) | Low |
| 6 | Hash-based native function lookup | [2.4](#24-unnecessary-indirection) | Low |
| 7 | Compact `EvalError` representation | [4.2](#42-error-handling) | Medium |
| 8 | Newtypes for env/expr indices | [4.3](#43-type-system-improvements) | High |
| 9 | Separate `Value` types from parser crate | [1.1](#11-module-organization) | High |
| 10 | Inline common builtins | [4.7](#47-lisp-specific-optimizations) | Medium |
| 11 | Dynamic mark-phase batch size | [3.3](#33-garbage-collection-strategy) | Low |
| 12 | Audit `grift_arena` public API surface | [4.4](#44-visibility--api-design) | Low |
| 13 | Builder pattern for continuations | [1.3](#13-design-patterns) | Medium |
| 14 | Hash-based intern table | [2.3](#23-environment--symbol-table-lookup) | Medium |

### 🟢 Low Impact

| # | Recommendation | Section | Effort |
|---|---------------|---------|--------|
| 15 | `#[inline]` on arena `get`/`set` | [3.1](#31-lifetime-annotations--borrowing) | Low |
| 16 | `#[cold]` on error constructors | [4.2](#42-error-handling) | Low |
| 17 | Extract dedicated lexer | [1.2](#12-separation-of-concerns) | Medium |
| 18 | `NonZeroUsize` for `ArenaIndex` | [4.1](#41-idiomatic-rust-patterns) | Medium |
| 19 | Typed continuation enum | [4.3](#43-type-system-improvements) | Medium |
| 20 | Constant folding in parser | [4.7](#47-lisp-specific-optimizations) | Medium |
| 21 | Share name conversion between proc macro and runtime | [4.6](#46-macro-expansion-improvements) | Low |
| 22 | `Display` impl for `Value` | [4.1](#41-idiomatic-rust-patterns) | Low |
| 23 | Compress singleton `Value` variants | [1.4](#14-ast-representation) | Low |
| 24 | Replace `unwrap()` in library crates | [4.5](#45-dead-code--unused-dependencies) | Low |
| 25 | Deduplicate arena macro bounds checks | [4.6](#46-macro-expansion-improvements) | Low |
