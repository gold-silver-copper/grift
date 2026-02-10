# Design Document: Flat AST for Grift

## Status

Proposal — not yet implemented.

## Summary

Grift currently interprets S-expressions (cons cells) directly as its AST.
Every evaluation step walks arena-allocated cons cells and string-compares
symbol names against ~24 keywords to dispatch special forms. This document
proposes inserting a **compilation phase** that lowers S-expressions into a
flat, typed AST stored in a contiguous `Vec`-like buffer. The evaluator then
walks the flat AST instead of cons cells, eliminating keyword string matching,
reducing arena pressure, and enabling lexical variable addressing.

## Motivation

### Current architecture

```
Source text
  → Lexer (grift_parser/lexer.rs)
    → Parser (grift_parser/parser.rs)
      → Arena-allocated S-expressions (Value::Cons / Value::Symbol / ...)
        → Trampolined evaluator (grift_eval/evaluator/core.rs)
```

The parser produces a tree of `Value::Cons` cells in `Lisp<N>`'s fixed-size
arena. The evaluator's trampoline loop (`core.rs:645-694`) alternates between
two states:

- `TrampolineState::Eval { expr, env }` — take one evaluation step
- `TrampolineState::Return { val }` — process a return through the
  continuation chain

This design achieves unlimited Scheme-level recursion depth (bounded only by
arena size) with zero Rust stack growth. It is an excellent fit for `no_std`
and embedded targets where stack space is limited.

However, four measurable costs arise from interpreting raw S-expressions:

### Cost 1 — Cons cell walking for dispatch

Every list evaluation in `step_eval` (`core.rs:697-771`) requires:

```rust
let val = self.lisp.get(expr.0)?;        // 1 arena access
// if Cons:
let car = self.lisp.car(expr.0)?;        // 1 arena access
let cdr = self.lisp.cdr(expr.0)?;        // 1 arena access
self.step_eval_list(car, cdr, expr, env)  // → further accesses
```

3-5 arena accesses per list expression before any real work begins.

### Cost 2 — String comparison for special form dispatch

`step_eval_list` (`core.rs:875-1122`) calls `try_dispatch_special_form` (checks
`"if"`) and then `try_dispatch_non_core_form` which runs up to 24 sequential
`symbol_matches` checks:

```rust
if self.lisp.symbol_matches(car, "quote")? { ... }
if self.lisp.symbol_matches(car, "define-syntax")? { ... }
if self.lisp.symbol_matches(car, "let-syntax")? { ... }
// ... 21 more
```

Each `symbol_matches` does character-by-character comparison via arena reads.
Matching `"define-syntax"` (13 chars) costs ~14 arena accesses for a miss.
A regular function call `(f x y)` falls through all 24 checks before reaching
application — that's up to ~200 arena accesses wasted on keyword matching for
ordinary calls.

### Cost 3 — Continuation frame allocation per operation

Each pending operation allocates continuation frames as cons cells in the
arena. A binary operation like `(+ 1 2)` allocates:

| Step | Continuation | Cons cells allocated |
|------|-------------|---------------------|
| Evaluate `+` | `ApplyForced` | 3 (pack3) |
| Evaluate `1` | `BinaryBuiltinFirst` | 4 (pack4) |
| Evaluate `2` | `BinaryBuiltinSecond` | 3 (pack3) |
| **Total** | **3 frames** | **10 cons cells** |

These cons cells are transient — they exist only until the continuation is
popped — but they consume arena capacity and increase GC frequency.

### Cost 4 — Linear environment lookup

Variable lookup (`core.rs:850-867`) walks a linked list of `(name . value)`
bindings:

```rust
loop {
    match self.lisp.get(current)? {
        Value::Nil => return Ok(None),
        Value::Cons { car, cdr } => {
            if let Value::Cons { car: bound_name, cdr: bound_value } = self.lisp.get(car)?
                && self.lisp.symbol_eq(bound_name, name)? {
                return Ok(Some(bound_value));
            }
            current = cdr;
        }
    }
}
```

Cost per lookup at depth N: `N * (2-3 arena accesses + 1 symbol_eq)`. In
nested `let*` forms this becomes significant.

## Design

### New pipeline

```
Source text
  → Lexer (unchanged)
    → Parser (unchanged — produces S-expressions)
      → **Compiler** (new: grift_eval/compiler.rs)
        → Flat AST buffer (new: Vec<AstNode> or arena-backed)
          → Trampolined evaluator (modified to walk AstNode)
```

The compiler is a single-pass tree walk over S-expressions that produces a
flat AST. The evaluator's trampoline loop is preserved but operates on
`AstIdx` references into the flat buffer instead of `ArenaIndex` references
into the cons-cell arena.

### AstNode definition

```rust
/// Index into the flat AST buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AstIdx(u32);

/// A compiled AST node. Stored in a flat Vec<AstNode>.
///
/// Design constraints:
/// - Must be Copy (no heap pointers) for no_std compatibility
/// - Fixed size — no variable-length inline data
/// - References to other nodes use AstIdx
/// - References to runtime values (constants, symbols) use ArenaIndex
#[derive(Clone, Copy, Debug)]
pub enum AstNode {
    // === Literals ===

    /// Self-evaluating constant (number, bool, char, string, nil, void).
    /// Points to the arena-allocated value.
    Const(ArenaIndex),

    // === Variables ===

    /// Variable reference with lexical address.
    /// `depth`: number of environment frames to skip (0 = current).
    /// `slot`: position within that frame.
    /// `name`: original symbol (for error messages and fallback).
    VarRef { depth: u16, slot: u16, name: ArenaIndex },

    /// Global variable reference (not lexically resolved).
    /// Falls back to linear env lookup at runtime.
    GlobalRef { name: ArenaIndex },

    // === Core special forms ===

    /// (if test then else)
    If { test: AstIdx, consequent: AstIdx, alternate: AstIdx },

    /// (lambda (params...) body)
    /// `params`: index of first AstNode::ParamList node
    /// `body`: index of body expression (or Begin node for multi-expr body)
    /// `arity`: number of required parameters
    /// `has_rest`: whether there's a rest parameter
    Lambda { params: AstIdx, body: AstIdx, arity: u16, has_rest: bool },

    /// Parameter list — stored as a linked sequence in the flat buffer.
    /// `name`: the parameter symbol
    /// `next`: next parameter, or AstIdx::NONE
    Param { name: ArenaIndex, next: AstIdx },

    /// (define name value)
    Define { name: ArenaIndex, value: AstIdx },

    /// (set! name value)
    /// Includes lexical address when resolvable.
    Set { depth: u16, slot: u16, name: ArenaIndex, value: AstIdx },

    /// (begin expr1 expr2 ... exprN)
    /// `first`: index of first SeqEntry node
    Begin { first: AstIdx },

    /// Linked list entry for sequences (begin, body, etc.).
    /// `expr`: the expression at this position
    /// `next`: next entry, or AstIdx::NONE for the tail position
    SeqEntry { expr: AstIdx, next: AstIdx },

    /// (quote datum)
    Quote(ArenaIndex),

    // === Function application ===

    /// (func arg1 arg2 ...)
    /// `func`: the function expression
    /// `args`: index of first ArgEntry node
    /// `argc`: number of arguments (for fast arity checks)
    Apply { func: AstIdx, args: AstIdx, argc: u16 },

    /// Linked list entry for argument lists.
    ArgEntry { expr: AstIdx, next: AstIdx },

    // === Derived / additional special forms ===

    /// (eval expr) or (eval expr env)
    Eval { expr: AstIdx, env_expr: AstIdx },

    /// (apply func args-expr)
    ApplySpecial { func: AstIdx, args: AstIdx },

    /// (call/cc proc) / (call-with-current-continuation proc)
    CallCC { proc: AstIdx },

    /// (call-with-values producer consumer)
    CallWithValues { producer: AstIdx, consumer: AstIdx },

    /// (values expr1 expr2 ...)
    Values { first: AstIdx },

    /// (dynamic-wind before body after)
    DynamicWind { before: AstIdx, body: AstIdx, after: AstIdx },

    /// (with-exception-handler handler thunk)
    WithExceptionHandler { handler: AstIdx, thunk: AstIdx },

    /// (raise obj) / (raise-continuable obj)
    Raise { expr: AstIdx, continuable: bool },

    /// (quasiquote template)
    /// Kept as S-expression for now — quasiquote evaluation is complex
    /// and benefits less from flat AST since it's constructing data.
    Quasiquote(ArenaIndex),

    // === Macros (compile-time) ===

    /// (define-syntax name transformer)
    DefineSyntax { name: ArenaIndex, transformer: AstIdx },

    /// (let-syntax ((name transformer) ...) body)
    /// (letrec-syntax ((name transformer) ...) body)
    LetSyntax { bindings: AstIdx, body: AstIdx, is_rec: bool },

    /// (syntax-case stx (literals) clauses)
    /// Kept as S-expression reference — syntax-case patterns are matched
    /// against runtime data structures, not compiled AST.
    SyntaxCase(ArenaIndex),

    /// (syntax template)
    SyntaxTemplate(ArenaIndex),

    /// (define-record-type ...)
    DefineRecordType(ArenaIndex),

    /// (define-library ...)
    DefineLibrary(ArenaIndex),

    /// (import ...)
    Import(ArenaIndex),

    /// (environment ...)
    Environment(ArenaIndex),
}

impl AstIdx {
    /// Sentinel value meaning "no node" (e.g., no else branch).
    pub const NONE: AstIdx = AstIdx(u32::MAX);
}
```

### Size budget

Each `AstNode` variant should fit in 16 bytes (128 bits) or less:

- `AstIdx` = 4 bytes (`u32`)
- `ArenaIndex` = 4 or 8 bytes (`usize`, platform-dependent)
- Discriminant = 1-2 bytes

On 32-bit targets (embedded, WASM): each node is ~12-16 bytes.
On 64-bit targets: each node is ~16-24 bytes.

For comparison, a single cons cell in the current arena takes 1 slot
(= `size_of::<Value>()` bytes), and representing `(if test then else)` requires
~4 cons cells = 4 slots.

### AST buffer storage

Two options for storing the flat AST, evaluated against grift's constraints:

**Option A: Separate `Vec<AstNode>` (requires `alloc`)**

```rust
pub struct AstBuffer {
    nodes: Vec<AstNode>,
}
```

Pros: Simple, cache-friendly, growable.
Cons: Requires `alloc` crate, breaks pure `no_std` without allocator.

**Option B: Arena-backed flat region (no_std compatible)**

Store `AstNode` values in a dedicated section of the existing arena, or in a
second arena of type `Arena<AstNode, M>`.

```rust
pub struct AstBuffer<const M: usize> {
    nodes: [AstNode; M],
    len: usize,
}
```

Pros: Fully `no_std`, fixed-size, no allocator needed.
Cons: Fixed capacity must be chosen at compile time.

**Recommendation**: Option B with a const generic size parameter, matching the
existing `Lisp<N>` pattern. The evaluator becomes `Evaluator<'a, N, M>` or the
AST buffer is embedded in the existing `Lisp<N>`.

### Compiler implementation

The compiler lives in a new file `grift_eval/src/compiler.rs`. It takes an
`ArenaIndex` (pointing to a parsed S-expression) and produces an `AstIdx`
(pointing into the flat buffer).

```rust
pub struct Compiler<'a, const N: usize, const M: usize> {
    lisp: &'a Lisp<N>,
    buf: &'a mut AstBuffer<M>,
    /// Lexical scope stack for variable resolution.
    /// Each frame is a list of (name, slot) pairs.
    scopes: ScopeStack,
}

impl<'a, const N: usize, const M: usize> Compiler<'a, N, M> {
    pub fn compile(&mut self, expr: ArenaIndex) -> Result<AstIdx, CompileError> {
        let val = self.lisp.get(expr)?;
        match val {
            // Self-evaluating → Const
            Value::Nil | Value::Number(_) | Value::Float(_) |
            Value::Char(_) | Value::String { .. } | Value::True |
            Value::False | Value::Void => {
                self.emit(AstNode::Const(expr))
            }

            // Symbol → VarRef or GlobalRef
            Value::Symbol(_) => self.compile_var_ref(expr),

            // List → dispatch on head
            Value::Cons { .. } => self.compile_list(expr),

            _ => self.emit(AstNode::Const(expr)),
        }
    }

    fn compile_list(&mut self, expr: ArenaIndex) -> Result<AstIdx, CompileError> {
        let car = self.lisp.car(expr)?;
        let cdr = self.lisp.cdr(expr)?;

        // Check if head is a known keyword (via interned symbol identity)
        if let Value::Symbol(_) = self.lisp.get(car)? {
            // O(1) identity check against pre-interned keyword symbols
            if car == self.kw.if_sym {
                return self.compile_if(cdr);
            }
            if car == self.kw.lambda_sym {
                return self.compile_lambda(cdr);
            }
            if car == self.kw.define_sym {
                return self.compile_define(cdr);
            }
            // ... etc for all ~24 keywords
        }

        // Default: function application
        self.compile_apply(car, cdr)
    }

    fn emit(&mut self, node: AstNode) -> Result<AstIdx, CompileError> {
        self.buf.push(node)
    }
}
```

Key design decisions:

1. **Keyword dispatch uses interned symbol identity** — The compiler
   pre-interns all keyword symbols (`"if"`, `"lambda"`, `"define"`, etc.) at
   startup, storing their `ArenaIndex` values. Dispatch is then a single
   `ArenaIndex == ArenaIndex` comparison (integer equality) instead of
   character-by-character string matching.

2. **Lexical variable resolution** — The compiler maintains a `ScopeStack`
   tracking which variables are in scope at each nesting level. When it sees
   a symbol, it searches the scope stack to resolve `(depth, slot)`. If found,
   it emits `VarRef { depth, slot, name }`; otherwise `GlobalRef { name }`.

3. **Macro expansion happens before compilation** — The existing `expand()`
   pass runs first, producing fully expanded S-expressions. The compiler then
   only sees core forms and applications.

### Lexical scope tracking

```rust
/// Maximum nesting depth for lexical scopes.
const MAX_SCOPE_DEPTH: usize = 64;
/// Maximum bindings per scope frame.
const MAX_BINDINGS_PER_SCOPE: usize = 64;

struct ScopeStack {
    frames: [ScopeFrame; MAX_SCOPE_DEPTH],
    depth: usize,
}

struct ScopeFrame {
    bindings: [(ArenaIndex, u16); MAX_BINDINGS_PER_SCOPE], // (name, slot)
    count: usize,
}

impl ScopeStack {
    /// Look up a variable. Returns (depth, slot) or None.
    fn resolve(&self, name: ArenaIndex) -> Option<(u16, u16)> {
        for d in (0..self.depth).rev() {
            for i in 0..self.frames[d].count {
                if self.frames[d].bindings[i].0 == name {
                    let relative_depth = (self.depth - 1 - d) as u16;
                    return Some((relative_depth, self.frames[d].bindings[i].1));
                }
            }
        }
        None
    }
}
```

This is fixed-size and `no_std` compatible. The limits (64 depth, 64 bindings)
are generous for typical Scheme code.

### Evaluator changes

The evaluator's `step_eval` changes from pattern-matching on `Value` to
pattern-matching on `AstNode`:

```rust
// BEFORE (current):
fn step_eval(&mut self, expr: ExprRef, env: EnvRef) -> Result<TrampolineState, EvalError> {
    let val = self.lisp.get(expr.0)?;
    match val {
        Value::Cons { .. } => {
            let car = self.lisp.car(expr.0)?;
            let cdr = self.lisp.cdr(expr.0)?;
            self.step_eval_list(car, cdr, expr, env) // → 24 symbol_matches checks
        }
        Value::Symbol(_) => { /* linear env lookup */ }
        _ => Ok(TrampolineState::Return { val: expr.0 })
    }
}

// AFTER (proposed):
fn step_eval(&mut self, expr: AstIdx, env: EnvRef) -> Result<TrampolineState, EvalError> {
    match self.ast.get(expr) {
        AstNode::Const(val) => Ok(TrampolineState::Return { val }),

        AstNode::VarRef { depth, slot, .. } => {
            let val = self.env_lookup_indexed(env, depth, slot)?;
            Ok(TrampolineState::Return { val })
        }

        AstNode::GlobalRef { name } => {
            let val = self.env_lookup(self.global_env, name)?;
            Ok(TrampolineState::Return { val })
        }

        AstNode::If { test, consequent, alternate } => {
            self.cont(ContType::IfBranch, env)
                .ast_data(consequent, alternate, env.0)?;
            Ok(TrampolineState::Eval { expr: test, env })
        }

        AstNode::Apply { func, args, argc } => {
            self.cont(ContType::ApplyForced, env)
                .ast_data(args, argc, env.0)?;
            Ok(TrampolineState::Eval { expr: func, env })
        }

        // ... one arm per AstNode variant
    }
}
```

The continuation types mostly stay the same, but their data fields can now
reference `AstIdx` values (which are `u32`) instead of packing `ArenaIndex`
cons chains. This reduces arena allocation per continuation push.

### Environment representation for indexed lookup

For `VarRef { depth, slot }` to work, environments need indexed access:

```rust
/// A flat environment frame for indexed variable access.
/// Stored in the arena as: Array { len, data } where data points
/// to a contiguous block of ArenaIndex values.
fn env_lookup_indexed(&self, env: EnvRef, depth: u16, slot: u16) -> EvalResult {
    let mut frame = env.0;
    for _ in 0..depth {
        // Each frame is (values_array . parent_frame)
        frame = self.lisp.cdr(frame)?;
    }
    let values = self.lisp.car(frame)?;
    self.lisp.array_ref(values, slot as usize)
}
```

This requires changing how `lambda` binds parameters: instead of extending
the environment with a linked list of `(name . value)` pairs, it creates an
`Array` of values and conses it onto the environment chain.

**Fallback**: `GlobalRef` lookups and `set!` on global variables still use
the existing linear-scan `env_lookup`. Only lexically-resolved variables
benefit from O(1) access.

### Interaction with macros

Macros present the main complexity:

1. **`define-syntax` / `syntax-case`** — Macro transformers are Scheme
   procedures that operate on S-expression syntax objects at runtime. They
   cannot be compiled to flat AST because their input and output are
   arbitrary S-expressions.

2. **Expansion before compilation** — The compiler assumes macro expansion
   has already happened. The pipeline becomes:

   ```
   parse → expand → compile → evaluate
   ```

3. **`eval` at runtime** — When Scheme code calls `(eval expr)`, the
   evaluator must parse + expand + compile the expression on the fly. This
   is the same cost as today (parse + expand + eval) plus the compilation
   step, but compilation is a single fast pass.

4. **Syntax-case forms in compiled code** — `syntax-case`, `syntax`, and
   related forms are compiled to `AstNode::SyntaxCase(ArenaIndex)` which
   stores a reference to the original S-expression. At evaluation time, the
   evaluator falls back to the existing S-expression-based pattern matcher.
   This is a pragmatic choice: syntax-case is used at macro-definition time,
   not in hot loops.

### Interaction with `call/cc`

The current design stores continuations as arena-linked-list frames, enabling
O(1) `call/cc` capture (save one pointer). This is preserved in the flat AST
design:

- Continuations still live in the arena as `ContFrame` values
- The only change is that continuation data may reference `AstIdx` values
  (the "what to evaluate next" field) instead of `ArenaIndex` S-expressions
- `call/cc` capture remains O(1) — save the `current_cont` pointer

No changes to the continuation architecture are required.

### Interaction with GC

The flat AST buffer is **not** arena-allocated — it's a separate fixed-size
array. This means:

- AST nodes are **not** garbage collected (they're static for the lifetime
  of the compiled code)
- `ArenaIndex` values inside `AstNode` (constants, symbol names) must be
  treated as **GC roots** — the GC must scan the AST buffer to find live
  arena references
- The existing `GcRoots` trait is extended to trace AST buffer contents

```rust
impl<const M: usize> GcRoots for AstBuffer<M> {
    fn trace_roots(&self, tracer: &mut dyn FnMut(ArenaIndex)) {
        for node in &self.nodes[..self.len] {
            node.trace_arena_refs(tracer);
        }
    }
}
```

## Migration strategy

### Phase 1 — Keyword interning (no AST changes)

Before building the flat AST, intern all special-form keywords at `Lisp::new()`
time and store their `ArenaIndex` values. Replace `symbol_matches(car, "if")`
with `car == self.kw_if`. This is a standalone optimization that can ship
independently.

**Files changed**: `grift_core/src/lisp.rs`, `grift_eval/src/evaluator/core.rs`
**Risk**: Low — behavioral no-op, only changes dispatch speed.

### Phase 2 — AST buffer and compiler

Add `AstBuffer`, `AstNode`, and `Compiler`. The compiler takes expanded
S-expressions and produces flat AST. Add a `compile()` method to `Evaluator`.

**Files added**: `grift_eval/src/compiler.rs`, `grift_eval/src/ast.rs`
**Files changed**: `grift_eval/src/evaluator/mod.rs` (new module)
**Risk**: Medium — new code, but doesn't change existing behavior yet.

### Phase 3 — Dual-mode evaluator

Add a parallel `step_eval_ast` path that evaluates `AstNode`. The existing
`step_eval` (S-expression path) is preserved. A flag or separate entry point
selects which path is used.

**Files changed**: `grift_eval/src/evaluator/core.rs`, `grift_eval/src/evaluator/forms.rs`
**Risk**: Medium — new evaluation path, but old path remains as fallback.

### Phase 4 — Indexed environments

Change `lambda` application to create array-based environment frames for
O(1) indexed variable access. The old linked-list path is kept for
`GlobalRef` and fallback cases.

**Files changed**: `grift_eval/src/evaluator/core.rs`, `grift_eval/src/evaluator/forms.rs`
**Risk**: Medium-high — changes environment representation.

### Phase 5 — Default to flat AST

Make the compiled path the default. The S-expression evaluator remains
available for `eval`, macro expansion, and as a fallback.

**Files changed**: `grift_eval/src/evaluator/core.rs`
**Risk**: Low once phases 2-4 are stable.

## Expected performance impact

| Operation | Current cost | With flat AST | Speedup |
|-----------|-------------|---------------|---------|
| Dispatch `(if ...)` | ~14 arena accesses (symbol_matches) | 1 enum match | ~14x |
| Dispatch `(f x y)` (regular call) | ~200 arena accesses (fall-through) | 1 enum match | ~200x |
| Variable lookup (depth 3) | 9-12 arena accesses | 1 array index | ~10x |
| `(+ 1 2)` continuation alloc | 10 cons cells | 0 cons cells (AstIdx refs) | saves 10 allocs |
| `(lambda (x) x)` creation | parse params list | pre-compiled | — |
| `(eval expr)` at runtime | same as today | same + compile step | slightly slower |
| Macro expansion | same as today | same (pre-compilation) | no change |

The speedup is most dramatic for **regular function calls** (the common case),
which currently pay the full keyword-matching cost for no benefit.

Overall expected improvement: **3-5x** on typical Scheme programs, based on
the proportion of evaluation time spent in dispatch and environment lookup.

## Risks and trade-offs

### Increased code complexity

The evaluator grows a parallel evaluation path. Maintaining two interpreters
(S-expression and flat AST) increases the surface area for bugs.

**Mitigation**: The extensive existing test suite (r5rs, syntax, continuations,
etc.) provides strong regression coverage. Both paths must produce identical
results.

### Memory usage

The AST buffer is a separate fixed-size allocation alongside the arena.
For embedded targets with tight memory budgets, this is an additional cost.

**Mitigation**: The buffer size `M` is a const generic, so users control
the allocation. A reasonable default of `M = N / 2` (half the arena size)
covers typical programs.

### `no_std` compatibility

The design uses only fixed-size arrays and the existing arena — no `alloc`
dependency. All new types are `Copy`. The `ScopeStack` uses inline arrays
with fixed bounds.

### Loss of homoiconicity for compiled code

Once compiled, the flat AST is no longer an S-expression that Scheme code
can inspect or modify. This only matters for `eval` and macro expansion,
which continue to operate on S-expressions.

### Compilation overhead

The compile step adds latency to `eval_str()`. However, compilation is a
single O(n) pass over the S-expression tree — cheaper than the existing
`expand()` macro-expansion pass, which may iterate multiple times.

## Alternatives considered

### Bytecode VM

A full bytecode compiler with stack/register VM would yield larger speedups
(5-20x) but requires significantly more implementation effort:

- Instruction encoding and decoding
- Stack frame management
- Tail call as jump optimization
- `call/cc` requires serializing the VM stack (complex)
- Much larger deviation from current architecture

The flat AST is a natural stepping stone: if bytecode is desired later, the
compiler can be extended with a `lower_to_bytecode(AstIdx) -> Vec<Op>` pass.

### JIT compilation

Out of scope for a `no_std` embedded Scheme. Mentioned for completeness.

### Optimize current evaluator only (no flat AST)

Keyword interning (Phase 1) alone gives a 2-3x improvement on dispatch-heavy
code. Combined with the existing binary-builtin fast paths, this may be
sufficient for some use cases. However, it cannot address the O(n) environment
lookup cost or the per-operation continuation allocation overhead.

## Open questions

1. **AST buffer lifetime** — Should compiled ASTs be long-lived (cached
   across multiple `eval_str` calls) or transient (recompiled each time)?
   Caching saves compilation cost but requires invalidation when
   `define`/`set!` changes global bindings that affect compilation.

2. **Interaction with `syntax-case` fenders** — Fender expressions in
   `syntax-case` clauses are evaluated at macro-expansion time. Should they
   be compiled to flat AST, or left as S-expressions?

3. **Tail position tracking** — The compiler could mark tail-position
   expressions to enable the evaluator to skip continuation allocation.
   This is a natural extension but adds complexity to the compiler.

4. **Debug information** — Should the flat AST store source location
   information for error messages? The current S-expression approach
   implicitly preserves source structure; the flat AST would need explicit
   source maps.
