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
    Symbol { chars: ArenaIndex },           // Points to String value
    Lambda { data: ArenaIndex },            // Points to (params . (body . env))
    Builtin(Builtin),                       // Optimized primitives
    StdLib(StdLib),                        // Static function reference
    Array { data: ArenaIndex },            // data[0]=Number(len), data[1..]=elements
    String { data: ArenaIndex },           // data[0]=Number(len), data[1..]=chars
    Native { id: usize, name_hash: usize }, // Rust function reference
}
```

### Memory Optimization

The `Value` enum has been optimized to minimize its size:

1. **Lambda** - Stores only a single `ArenaIndex` pointing to a linked structure `(params . (body . env))` in the arena. This reduces Lambda's payload from 24 bytes (3 × ArenaIndex) to 8 bytes (1 × ArenaIndex).

2. **StdLib** - Uses a simple tuple variant `StdLib(StdLib)` with just the function enum. Function bodies are parsed on each call from static strings.

3. **Array/String** - Store only a `data` pointer. The length is stored in the arena at `data[0]` as `Value::Number(len)`, with elements/characters starting at `data[1]`.

The trade-off is that accessing Lambda fields requires additional arena lookups:
- `Lisp::lambda_parts(idx)` extracts `(params, body, env)` from a Lambda

This is an example of the classic space/time trade-off: we save memory at the cost of extra indirection.

### Further Memory Optimization Opportunities

Several additional techniques could further reduce memory usage:

1. **NaN-boxing** - Use the NaN space in 64-bit floats to encode values inline. This could pack small integers, symbols, and singletons without arena allocation.

2. **Tagged pointers** - Use the lower bits of ArenaIndex for type tags, eliminating the discriminant byte in some cases.

3. **Compact Array/String representation** - For small arrays (≤2 elements) or short strings (≤7 chars), store data inline in the Value variant itself.

4. **Intern table optimization** - Use a hash table with open addressing instead of an alist, reducing memory per interned symbol.

5. **Deduplicate Numbers** - Intern small integers (e.g., -128 to 127) similar to how Python does, reducing allocations.

6. **Compress environment alists** - Use more compact representations for environments, such as arrays of (symbol, value) pairs.

7. **Use u32 for ArenaIndex** - If the arena capacity is always < 4 billion, use `u32` instead of `usize` to halve index sizes on 64-bit systems.

8. **Stdlib caching** - Currently stdlib function bodies are parsed on each call. A global cache could store parsed ASTs to avoid repeated parsing.

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

### Complex Numbers (stdlib)

Complex numbers are represented as tagged lists: `(complex real imag)`. This provides full complex arithmetic without modifying the `Value` enum.

```lisp
; Create complex numbers
(make-rectangular 3 4)           ; => (complex 3 4)
(make-polar 5 0.785)             ; => (complex 3.54... 3.54...)

; Access components
(real-part (make-rectangular 3 4))  ; => 3
(imag-part (make-rectangular 3 4))  ; => 4
(magnitude (make-rectangular 3 4))  ; => 5.0
(angle (make-rectangular 3 4))      ; => 0.927...

; Arithmetic
(complex-add z1 z2)
(complex-sub z1 z2)
(complex-mul z1 z2)
(complex-div z1 z2)

; Other operations
(complex-conjugate z)
(complex-exp z)
(complex-log z)
(complex-sqrt z)
```

### Fractions/Rationals (stdlib)

Fractions are represented as tagged lists: `(fraction numerator denominator)`. Fractions are automatically simplified to lowest terms.

```lisp
; Create fractions (automatically simplified)
(make-fraction 6 4)              ; => (fraction 3 2)
(make-fraction -6 4)             ; => (fraction -3 2)

; Access components
(numerator (make-fraction 6 4))  ; => 3
(denominator (make-fraction 6 4)); => 2

; Arithmetic
(fraction-add f1 f2)
(fraction-sub f1 f2)
(fraction-mul f1 f2)
(fraction-div f1 f2)

; Comparison
(fraction-eq? f1 f2)
(fraction-lt? f1 f2)
(fraction-le? f1 f2)
(fraction-gt? f1 f2)
(fraction-ge? f1 f2)

; Other operations
(fraction-negate f)
(fraction-reciprocal f)
(fraction-abs f)
(fraction->number f)             ; Convert to number (exact if possible)
```

## Quasiquote

`quasiquote` enables template-based code generation:

```lisp
(define x 5)
(quasiquote (a b (unquote x)))  ; => (a b 5)
```

- `(quasiquote ...)` - Return structure mostly unevaluated
- `(unquote ...)` - Evaluate this sub-expression
- `(unquote-splicing ...)` - Splice list into surrounding list

Note: The shorthand syntax (`` ` `` for quasiquote, `,` for unquote) is not currently supported in the parser.

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

### 2. Quasiquote Requires Full Syntax

The shorthand syntax (`` ` `` and `,`) is not currently supported. Use the full form:

```lisp
; Use this:
(quasiquote (a b (unquote x)))

; Not this (currently unsupported):
; `(a b ,x)
```

### 3. Intern Table is Always Reachable

All interned symbols are GC roots. If you create many unique symbols, they won't be collected.

### 4. StdLib Re-parsing

StdLib functions parse their body from static strings on each call. This means recursive stdlib calls will allocate new AST nodes each time. For performance-critical recursive operations, consider using larger arenas or implementing critical functions as builtins.

## Performance Considerations

1. **Use tail recursion** - Proper TCO means tail calls don't consume stack
2. **Batch allocations** - GC runs when explicitly triggered or when `alloc_or_gc` is used
3. **Disable GC for batch ops** - `(gc-disable)` during many allocations, then `(gc-enable)` and `(gc)`
4. **Prefer builtins** - Builtins are faster than equivalent lambdas
