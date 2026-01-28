# Lisp Implementation Architecture

This document describes the architecture, design decisions, and implementation details of the Lisp interpreter built on `pwn_arena`.

## Overview

This is a classic Lisp implementation with modern features:

- **Lexically scoped closures** with proper environments
- **Hybrid lazy/strict evaluation** - lazy by default, strict in tail position
- **Full tail-call optimization** via trampolining
- **Mark-and-sweep garbage collection** controllable from Lisp
- **Macros** with quasiquote/unquote
- **no_std compatible** - only the REPL requires `std`

## Crate Organization

```
crates/
├── pwn_arena/     # Arena allocator (no_std, no_alloc)
├── lisp_parser/   # Parser, Value type, builtins (no_std)
├── lisp_eval/     # Evaluator with trampolined TCO (no_std)
├── lisp_repl/     # REPL with I/O (uses std)
└── lisp_macros/   # Proc macros for stdlib generation
```

### Dependency Graph

```
pwn_arena ← lisp_parser ← lisp_eval ← lisp_repl
                ↑
           lisp_macros
```

## Value Representation

All Lisp values are stored as a `Value` enum in the arena:

```rust
pub enum Value {
    Nil,                                    // Empty list ()
    True,                                   // #t
    False,                                  // #f
    Number(i64),                           // Integers
    Char(char),                            // Characters
    Cons { car: ArenaIndex, cdr: ArenaIndex },
    Symbol { chars: ArenaIndex, len: usize },
    Lambda { params: ArenaIndex, body: ArenaIndex, env: ArenaIndex },
    Thunk { expr: ArenaIndex, env: ArenaIndex, cached: ArenaIndex },
    Memo { func: ArenaIndex, cache: ArenaIndex },
    Builtin(Builtin),                      // Optimized primitives
    StdLib { func: StdLib, cached_body: ArenaIndex, cached_params: ArenaIndex },
    Array { data: ArenaIndex, len: usize }, // Contiguous value storage
}
```

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
- The `chars` field points to a length slot followed by character slots
- Each character is stored as `Value::Char(c)`
- This uses less memory than a linked list of characters

### Arrays

Arrays provide O(1) indexed access to values stored contiguously in the arena:

```lisp
(define arr (make-array 5 0))  ; Array of 5 zeros
(array-set! arr 2 42)          ; Set element at index 2
(array-ref arr 2)              ; => 42
(array-length arr)             ; => 5
```

Arrays use contiguous storage for efficient access, with the same memory layout as strings:
- The `data` field points to a contiguous block: `[Number(len), elem0, elem1, ..., elem(len-1)]`
- Length is stored in the first slot as a Number value (consistent with string layout)
- Length is also cached in the `Value::Array` variant for O(1) access without dereferencing
- Elements are stored at consecutive arena slots: data+1, data+2, ..., data+len
- O(1) read and write operations via direct index calculation
- Efficient memory layout for cache-friendly access

## Evaluation Strategy

### Hybrid Lazy/Strict

We use a hybrid approach that combines the benefits of lazy and strict evaluation:

**Lazy (call-by-need)**:
- `cons` is lazy - arguments become thunks
- Enables infinite data structures
- Values are memoized after first evaluation

**Strict (call-by-value)**:
- Tail calls evaluate arguments strictly
- Enables proper tail-call optimization
- Builtins force arguments in strict positions (arithmetic, comparisons, etc.)

```lisp
; LAZY: cons wraps arguments in thunks
(define (ones) (cons 1 (ones)))  ; Works! Infinite stream
(car (ones))                      ; Forces only the car

; STRICT: Tail calls evaluate strictly
(define (sum n acc)
  (if (= n 0) acc
      (sum (- n 1) (+ acc n))))  ; Args evaluated before recursive call
```

### Thunk Representation

Thunks represent delayed computations:

```rust
Thunk {
    expr: ArenaIndex,    // Unevaluated expression
    env: ArenaIndex,     // Environment for evaluation
    cached: ArenaIndex,  // NULL until forced, then cached result
}
```

When forced:
1. Evaluate `expr` in `env`
2. Store result in `cached`
3. Return the cached value

Subsequent accesses return `cached` directly (memoization).

## Trampolined Evaluation

The evaluator uses continuation-passing style with an explicit stack, avoiding Rust stack recursion:

```rust
enum TrampolineState {
    Eval { expr: ArenaIndex, env: ArenaIndex },
    Force { idx: ArenaIndex },
    Return { val: ArenaIndex },
}

enum Cont {
    Done,
    Force,
    IfBranch { then_expr, else_expr, env },
    ApplyForced { args_expr, env, call_expr },
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
        Force { idx } => {
            // If thunk, push CacheThunk continuation and eval
            // Otherwise, return the value
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
| `if` | Conditional (lazy in branches) |
| `cond` | Multi-way conditional |
| `case` | Pattern matching on values |
| `lambda` | Create closure |
| `define` | Define variable or function |
| `set!` | Mutate variable binding |
| `let` | Parallel local bindings |
| `let*` | Sequential local bindings |
| `begin` | Sequence of expressions |
| `and`/`or` | Short-circuit boolean operations |
| `do` | Iteration loop |
| `quasiquote` | Template with unquote |
| `eval` | Runtime evaluation |
| `apply` | Apply function to argument list |
| `defmacro` | Define macro |

## Built-in Functions

Builtins are optimized primitives stored as enum variants:

```rust
pub enum Builtin {
    Car, Cdr, Cons, List,
    Atom, Eq, Null, Pairp, Numberp, Booleanp, Procedurep, Symbolp,
    Add, Sub, Mul, Div, Mod,
    Lt, Gt, Le, Ge, NumEq,
    Not, Print, Display, Newline, Error,
    Memoize, Gensym, SetCar, SetCdr,
    MakeArray, ArrayRef, ArraySet, ArrayLength, Arrayp,
    Gc, GcEnable, GcDisable, GcEnabledP, ArenaStats,
}
```

Adding a new builtin:
1. Add variant to `define_builtins!` macro in `lisp_parser`
2. Implement evaluation in `apply_builtin_with_forced_args` in `lisp_eval`

## Standard Library

The stdlib is defined as static Lisp code, not hardcoded ASTs:

```rust
define_stdlib! {
    Length("length", ["lst"], "(if (null? lst) 0 (+ 1 (length (cdr lst))))"),
    Map("map", ["f", "lst"], "(if (null? lst) '() (cons (f (car lst)) (map f (cdr lst))))"),
    // ...
}
```

**Advantages**:
- Easy to read and maintain
- No arena cost for function definitions
- Parsing overhead is minimal

**Implementation**:
- The `StdLib` value stores cached parsed body/params
- First call parses the body and caches it
- Subsequent calls reuse the cached AST

## Macro System

Macros are implemented as a simple expansion phase:

```rust
macros: [(ArenaIndex, ArenaIndex, ArenaIndex); MAX_MACROS],
macro_count: usize,
```

Each macro stores `(name, params, body)`.

### Macro Expansion

Before evaluation, expressions are checked for macro calls:

1. If the car is a symbol matching a macro name:
   - Bind macro parameters to the unevaluated arguments
   - Evaluate the macro body in this environment
   - Replace the original expression with the result
   - Re-expand (in case macro produces another macro call)

### Quasiquote

`quasiquote` enables template-based macro bodies:

```lisp
(defmacro unless (cond then else)
  `(if ,cond ,else ,then))
```

- `` ` `` (quasiquote) - Return structure mostly unevaluated
- `,` (unquote) - Evaluate this sub-expression
- `,@` (unquote-splicing) - Splice list into surrounding list

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

### Thunk Evaluation Stack Limits

Thunk forcing uses Rust stack recursion, limiting deep lazy chains:

**Trade-off**: Deeply nested thunks (e.g., long chains of lazy `cons` calls) can overflow the Rust stack.

**Mitigation**: Use tail-recursive patterns for deep recursion; avoid deeply nested lazy structures. Consider forcing intermediate values to break up long thunk chains.

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

**Gotcha**: `(if nil 'yes 'no)` returns `'yes`!

## Gotchas

### 1. Lazy Side Effects

Side effects in lazy positions may not execute when expected:

```lisp
(define count 0)
(define x (cons (begin (set! count 1) 'a) '()))
count  ; => 0 (cons is lazy!)
(car x)
count  ; => 1 (now the side effect ran)
```

### 2. Macro Hygiene

`gensym` should be used to avoid variable capture:

```lisp
(defmacro swap (a b)
  (let ((temp (gensym)))
    `(let ((,temp ,a))
       (set! ,a ,b)
       (set! ,b ,temp))))
```

### 3. Intern Table is Always Reachable

All interned symbols are GC roots. If you create many unique symbols, they won't be collected.

### 4. StdLib Re-parsing (Now Cached)

StdLib functions cache their parsed body after first call. The initial parse happens once per function.

## Performance Considerations

1. **Use tail recursion** - Proper TCO means tail calls don't consume stack
2. **Batch allocations** - GC runs when explicitly triggered or when `alloc_or_gc` is used
3. **Disable GC for batch ops** - `(gc-disable)` during many allocations, then `(gc-enable)` and `(gc)`
4. **Prefer builtins** - Builtins are faster than equivalent lambdas
5. **Memoize expensive functions** - Use `(memoize fn)` for functions with repeated calls
