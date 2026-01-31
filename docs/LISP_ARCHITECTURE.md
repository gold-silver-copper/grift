# Lisp Implementation Architecture

This document describes the architecture, design decisions, and implementation details of the Lisp interpreter built on `pwn_arena`.

## Overview

This is a classic Lisp implementation with modern features:

- **Lexically scoped closures** with proper environments
- **Strict evaluation** - all arguments are evaluated before function application (call-by-value)
- **Full tail-call optimization** via trampolining
- **Mark-and-sweep garbage collection** controllable from Lisp
- **Macros** with quasiquote/unquote
- **no_std compatible** - only the REPL requires `std`

## Crate Organization

```
crates/
├── pwn_arena/       # Arena allocator (no_std, no_alloc)
├── grift_parser/    # Parser, Value type, builtins (no_std)
├── grift_eval/      # Evaluator with trampolined TCO (no_std)
├── grift_repl/      # REPL with I/O (uses std)
├── grift_macros/    # Proc macros for stdlib generation
├── grift/           # Unified re-export crate (no_std by default, std feature optional)
└── pwn_arena_embedded/  # Embedded examples
```

### Dependency Graph

```
                    ┌─────────────────────┐
                    │       grift         │  (unified crate)
                    │  no_std by default  │
                    │  std feature opt-in │
                    └─────────┬───────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                     │
        ▼                     ▼                     ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│  grift_repl   │    │  grift_eval   │    │ grift_parser  │
│   (std)       │    │   (no_std)    │    │   (no_std)    │
└───────┬───────┘    └───────┬───────┘    └───────┬───────┘
        │                    │                    │
        └────────────────────┼────────────────────┘
                             │
                             ▼
                    ┌───────────────┐
                    │   pwn_arena   │
                    │ (no_std, no_alloc)
                    └───────────────┘
                             ▲
                             │
                    ┌───────────────┐
                    │ grift_macros  │
                    │ (proc macros) │
                    └───────────────┘
```

### The `grift` Unified Crate

The `grift` crate is the primary entry point for users. It provides:

- **`no_std`, `no_alloc` by default** — Works on bare metal, WASM, or embedded
- **`std` feature** — Enables REPL and formatting utilities
- **Complete re-exports** — All public APIs from underlying crates

```rust
// Minimal no_std usage
use grift::{Lisp, Evaluator, Value};

let lisp: Lisp<10000> = Lisp::new();
let mut eval = Evaluator::new(&lisp).unwrap();
let result = eval.eval_str("(+ 1 2 3)").unwrap();
```

```bash
# Install and run the REPL
cargo install grift --features std
grift
```

## Value Representation

All Lisp values are stored as a `Value` enum in the arena:

```rust
pub enum Value {
    Nil,                                    // Empty list ()
    True,                                   // #t
    False,                                  // #f
    Number(isize),                          // Integers
    Char(char),                             // Characters
    Cons { car: ArenaIndex, cdr: ArenaIndex },
    Symbol(ArenaIndex),                     // Points to String value
    Lambda { params: ArenaIndex, body_env: ArenaIndex },
    Builtin(Builtin),                       // Optimized primitives
    StdLib(StdLib),                         // Static function reference
    Array { len: usize, data: ArenaIndex }, // Inline length + data pointer
    String { len: usize, data: ArenaIndex },// Inline length + data pointer
    Native { id: usize, name_hash: usize }, // Rust function reference
    Ref(ArenaIndex),                        // Internal reference
    Usize(usize),                           // Internal unsigned int
}
```

### Memory Optimization

The `Value` enum has been optimized to minimize its size.

**IMPORTANT INVARIANT**: No `Value` variant should store more than **two `ArenaIndex`-sized (usize) fields**. This keeps the enum at a fixed 24 bytes (1 discriminant + 2 usizes) on 64-bit systems, ensuring cache-friendly memory layout and predictable performance. If you need to store more data, use indirection through the arena (e.g., `Lambda` stores `body_env` as a cons cell `(body . env)` rather than three separate fields).

Current variant payloads:
- `Cons { car, cdr }` — 2 ArenaIndex ✓
- `Lambda { params, body_env }` — 2 ArenaIndex ✓  
- `Array { len, data }` — 1 usize + 1 ArenaIndex ✓
- `String { len, data }` — 1 usize + 1 ArenaIndex ✓
- `Native { id, name_hash }` — 2 usize ✓

Specific optimizations:

1. **Lambda** - Stores only a single `ArenaIndex` pointing to a linked structure `(params . (body . env))` in the arena. This reduces Lambda's payload from 24 bytes (3 × ArenaIndex) to 8 bytes (1 × ArenaIndex).

2. **StdLib** - Uses a simple tuple variant `StdLib(StdLib)` with just the function enum. Function bodies are parsed on each call from static strings.

3. **Array/String** - Store length inline in the Value variant for O(1) access. The `data` pointer points directly to the first element (no length header in arena). Empty arrays/strings have `len=0` and `data == NIL`.

4. **Lambda** - Stores `params` and `body_env` inline. The `body_env` points to a cons cell `(body . env)`.

This design optimizes for common operations (length queries, iteration) while keeping arena usage minimal.

### Further Memory Optimization Opportunities

Several additional techniques could further reduce memory usage:

1. **Tagged pointers** - Use the lower bits of ArenaIndex for type tags, eliminating the discriminant byte in some cases.

2. **Compact Array/String representation** - For small arrays (≤2 elements) or short strings (≤7 chars), store data inline in the Value variant itself.

3. **Intern table optimization** - Use a hash table with open addressing instead of an alist, reducing memory per interned symbol.

4. **Deduplicate Numbers** - Intern small integers (e.g., -128 to 127) similar to how Python does, reducing allocations.

5. **Compress environment alists** - Use more compact representations for environments, such as arrays of (symbol, value) pairs.

6. **Use u32 for ArenaIndex** - If the arena capacity is always < 4 billion, use `u32` instead of `usize` to halve index sizes on 64-bit systems.

### Reserved Slots

The first few arena slots are reserved for singletons:

| Slot | Value | Purpose |
|------|-------|---------|
| 0 | `Nil` | Singleton empty list |
| 1 | `True` | Singleton boolean true |
| 2 | `False` | Singleton boolean false |
| 3 | `Cons` | Intern table reference |

This ensures `(eq nil nil)` and `(eq #t #t)` always return true (same ArenaIndex).

### Symbol Interning

Symbols are interned to ensure identity equality:

```lisp
(eq 'foo 'foo)  ; => #t (same ArenaIndex)
```

The intern table is stored in the arena and acts as a GC root, keeping all interned symbols alive.

Symbols use contiguous string storage for efficiency:
- The `chars` field points to a `Value::String` containing the symbol name
- The String contains contiguous `Value::Char(c)` slots
- This uses less memory than a linked list of characters

### Vectors

Vectors (R7RS Section 6.8) provide O(1) indexed access to values stored contiguously in the arena:

```lisp
(define vec (make-vector 5 0))  ; Vector of 5 zeros
(vector-set! vec 2 42)          ; Set element at index 2
(vector-ref vec 2)              ; => 42
(vector-length vec)             ; => 5
#(1 2 3)                        ; Vector literal syntax
```

Vectors use contiguous storage for efficient access:
- The `data` field points directly to the first element (no length slot like symbols)
- Length is stored in the `Value::Array` variant itself for O(1) access
- Elements are stored at consecutive arena slots: data+0, data+1, ..., data+(len-1)
- O(1) read and write operations via direct index calculation
- Efficient memory layout for cache-friendly access

## Evaluation Strategy

### Strict Evaluation (Call-by-Value)

This Lisp uses **strict evaluation**: all arguments are evaluated before a function is applied. This is the standard evaluation strategy used by most programming languages.

**Key characteristics**:
- All function arguments are evaluated before the function body executes
- `cons` evaluates both car and cdr before constructing the pair
- Side effects in arguments happen immediately
- Enables efficient tail-call optimization

```lisp
; Arguments are evaluated before function application
(define (add x y) (+ x y))
(add (+ 1 2) (+ 3 4))  ; Both args evaluated first, then add is called

; Side effects happen immediately
(define count 0)
(define lst (cons (begin (set! count 1) 'a) '()))
count  ; => 1 (side effect happened during cons)

; Tail-call optimization works properly
(define (sum n acc)
  (if (= n 0) acc
      (sum (- n 1) (+ acc n))))  ; Args evaluated before recursive call
(sum 10000 0)  ; => 50005000 (no stack overflow)
```

### Short-Circuit Evaluation

Special forms like `if`, `and`, and `or` use short-circuit evaluation:

```lisp
; 'if' only evaluates the selected branch
(if #t 'yes (error "never reached"))  ; => yes

; 'and' stops at first false
(and #f (error "never reached"))  ; => #f

; 'or' stops at first true
(or #t (error "never reached"))  ; => #t
```

## Trampolined Evaluation

The evaluator uses continuation-passing style with an explicit stack, avoiding Rust stack recursion:

```rust
enum TrampolineState {
    Eval { expr: ArenaIndex, env: ArenaIndex },
    Return { val: ArenaIndex },
}

enum Cont {
    Done,
    IfBranch { then_expr, else_expr, env },
    ApplyArgs { args_expr, env, call_expr },
    // ... many more continuations
}
```

### Evaluation Loop

```rust
loop {
    match state {
        Eval { expr, env } => {
            // Analyze expr, push continuations, set new state
        }
        Return { val } => {
            // Pop continuation and process
            match self.pop_cont() {
                Cont::Done => return Ok(val),
                Cont::IfBranch { then_expr, else_expr, env } => {
                    // Choose branch based on val
                }
                // ... handle other continuations
            }
        }
    }
}
```

This design enables:
- Unlimited recursion depth (bounded only by arena size)
- Proper tail-call optimization (tail calls don't push continuations)
- GC safety (all live values are on the continuation stack, which is a root set)

## Special Forms

Special forms are handled directly by the evaluator, not as functions:

| Form | Description |
|------|-------------|
| `quote` | Return expression unevaluated |
| `if` | Conditional (only selected branch evaluated) |
| `cond` | Multi-way conditional |
| `case` | Pattern matching on values |
| `lambda` | Create closure |
| `define` | Define variable or function |
| `set!` | Mutate variable binding |
| `let` | Parallel local bindings |
| `let*` | Sequential local bindings |
| `letrec` | Recursive local bindings |
| `letrec*` | Sequential recursive local bindings |
| `begin` | Sequence of expressions |
| `and`/`or` | Short-circuit boolean operations |
| `when`/`unless` | Convenience conditionals |
| `do` | Iteration loop |
| `quasiquote` | Template with unquote |
| `eval` | Runtime evaluation |
| `apply` | Apply function to argument list |
| `values` | Return multiple values |

## Built-in Functions

Builtins are optimized primitives stored as enum variants. The complete list includes:

**List Operations**: `car`, `cdr`, `cons`, `list`

**Type Predicates**: `null?`, `pair?`, `number?`, `boolean?`, `procedure?`, `symbol?`, `char?`, `string?`, `vector?`, `integer?`, `exact?`, `inexact?`, `exact-integer?`

**Equality**: `eq?`, `eqv?`, `equal?`

**Arithmetic**: `+`, `-`, `*`, `/`, `modulo`, `remainder`, `quotient`, `abs`, `max`, `min`, `gcd`, `lcm`, `expt`, `square`

**Numeric Predicates**: `zero?`, `positive?`, `negative?`, `odd?`, `even?`

**Rounding**: `floor`, `ceiling`, `truncate`, `round`

**Comparison**: `<`, `>`, `<=`, `>=`, `=`

**Boolean**: `not`

**I/O**: `display`, `newline`, `error`

**Mutation**: `set-car!`, `set-cdr!`

**Vectors** (R7RS Section 6.8): `vector?`, `make-vector`, `vector`, `vector-length`, `vector-ref`, `vector-set!`, `vector->list`, `list->vector`, `vector-fill!`, `vector-copy`

**Characters**: `char?`, `char=?`, `char<?`, `char>?`, `char<=?`, `char>=?`, `char->integer`, `integer->char`, `char-upcase`, `char-downcase`

**Strings**: `string?`, `make-string`, `string`, `string-length`, `string-ref`, `string-set!`, `string=?`, `string<?`, `string>?`, `string<=?`, `string>=?`, `string-append`, `string->list`, `list->string`, `substring`, `string-copy`

**GC Control**: `gc`, `gc-enable`, `gc-disable`, `gc-enabled?`, `arena-stats`

Adding a new builtin:
1. Add variant to `define_builtins!` macro in `grift_parser`
2. Implement evaluation in `apply_builtin` in `grift_eval`

## Standard Library

The stdlib is defined in `stdlib.scm` and processed by the `include_stdlib!` macro:

**Advantages**:
- Easy to read and maintain
- No arena cost for function definitions
- Parsing overhead is minimal

**Implementation**:
- The `StdLib` value is a simple tuple variant containing just the `StdLib` enum
- Each call parses the body from the static string
- Parsed AST is temporary and GC'd after evaluation


## Quasiquote

`quasiquote` enables template-based code generation:

```lisp
(define x 5)
`(a b ,x)           ; => (a b 5)
`(1 ,@'(2 3) 4)     ; => (1 2 3 4)
```

| Syntax | Long Form | Description |
|--------|-----------|-------------|
| `` `expr `` | `(quasiquote expr)` | Return structure mostly unevaluated |
| `,expr` | `(unquote expr)` | Evaluate this sub-expression |
| `,@expr` | `(unquote-splicing expr)` | Evaluate and splice list into surrounding list |

Both the shorthand syntax (`` ` ``, `,`, `,@`) and the long form (`quasiquote`, `unquote`, `unquote-splicing`) are supported.

## Garbage Collection Integration

### GC Roots

During evaluation, the following are GC roots:
- Global environment
- Current expression being evaluated
- Continuation stack (all stored ArenaIndices)
- Intern table (always preserved)

### GC from Lisp

```lisp
(gc)            ; Trigger collection, returns (marked collected before)
(gc-enable)     ; Enable automatic GC
(gc-disable)    ; Disable automatic GC  
(gc-enabled?)   ; Check if GC is enabled
(arena-stats)   ; Returns (capacity allocated free usage%)
```

## Design Trade-offs

### Fixed Continuation Stack

**Trade-off**: Continuation stack has fixed size (`MAX_CONT_DEPTH = 1024`).

**Rationale**: Avoids heap allocation for the continuation stack.

**Mitigation**: Very deep non-tail calls will hit this limit. Refactor to use tail recursion.

### No First-Class Continuations

**Trade-off**: No `call/cc` or `shift/reset`.

**Rationale**: First-class continuations are complex to implement with trampolining and would require heap allocation.

### Truthiness: Only #f is False

**Trade-off**: Differs from many Lisps where `nil` is false.

**Rationale**: Follows Scheme semantics. Separating "empty list" from "false" is cleaner.

**Gotcha**: `(if '() 'yes 'no)` returns `'yes`! The empty list is truthy.

## Gotchas

### 1. Empty List is Truthy

In this Lisp, only `#f` is false. The empty list `'()` is truthy:

```lisp
(if '() 'yes 'no)   ; => yes (empty list is truthy!)
(if 0 'yes 'no)     ; => yes (zero is truthy!)
(if #f 'yes 'no)    ; => no (only #f is false)
```

### 2. Nested Quasiquote Semantics

Nested quasiquotes follow standard Scheme semantics where the depth counter determines which unquotes are evaluated:

```lisp
(define x 5)
`(a `(b ,x))        ; => (a (quasiquote (b (unquote x)))) - inner ,x NOT evaluated
`(a `(b ,,x))       ; => (a (quasiquote (b (unquote 5)))) - outer unquote evaluates x
```

### 3. Intern Table is Always Reachable

All interned symbols are GC roots. If you create many unique symbols, they won't be collected.

### 4. StdLib Re-parsing

StdLib functions parse their body from static strings on each call. This means recursive stdlib calls will allocate new AST nodes each time. For performance-critical recursive operations, consider using larger arenas or implementing critical functions as builtins.

## Performance Considerations

1. **Use tail recursion** - Proper TCO means tail calls don't consume stack
2. **GC is automatic** - The evaluator runs GC periodically when memory pressure is high
3. **Disable GC for batch ops** - `(gc-disable)` during many allocations, then `(gc-enable)` and `(gc)`
4. **Prefer builtins** - Builtins are faster than equivalent lambdas
