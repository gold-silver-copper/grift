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
    Number(isize),                          // Integers
    Char(char),                             // Characters
    Cons { car: ArenaIndex, cdr: ArenaIndex },
    Symbol { chars: ArenaIndex },           // Points to String value
    Lambda { data: ArenaIndex },            // Points to (params . (body . env))
    Builtin(Builtin),                       // Optimized primitives
    StdLib { func: StdLib, cache: ArenaIndex }, // NULL or (body . params)
    Array { data: ArenaIndex, len: usize }, // Contiguous value storage
    String { data: ArenaIndex, len: usize }, // Contiguous char storage
    Native { id: usize, name_hash: usize }, // Rust function reference
}
```

### Memory Optimization

The `Value` enum has been optimized to minimize its size (24 bytes on 64-bit systems):

1. **Lambda** - Stores only a single `ArenaIndex` pointing to a linked structure `(params . (body . env))` in the arena. This reduces Lambda's payload from 24 bytes (3 × ArenaIndex) to 8 bytes (1 × ArenaIndex).

2. **StdLib** - Uses a single `cache` field that is either NULL (not yet parsed) or points to a cons cell `(body . params)`. This reduces StdLib's payload from 17+ bytes to 9 bytes.

The trade-off is that accessing Lambda or StdLib fields requires additional arena lookups:
- `Lisp::lambda_parts(idx)` extracts `(params, body, env)` from a Lambda
- `Lisp::stdlib_cache(idx)` gets cached `(body, params)` from a StdLib

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

8. **Lazy stdlib parsing** - Currently cached after first call, but the StdLib enum could be stored once globally instead of per-value.

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

### Arrays

Arrays provide O(1) indexed access to values stored contiguously in the arena:

```lisp
(define arr (make-array 5 0))  ; Array of 5 zeros
(array-set! arr 2 42)          ; Set element at index 2
(array-ref arr 2)              ; => 42
(array-length arr)             ; => 5
```

Arrays use contiguous storage for efficient access:
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
    Null, Pairp, Numberp, Booleanp, Procedurep, Symbolp,
    EqP, EqvP, EqualP,
    Add, Sub, Mul, Div, Modulo, Remainder,
    Lt, Gt, Le, Ge, NumEq,
    Not, Display, Newline, Error,
    SetCar, SetCdr,
    MakeArray, ArrayRef, ArraySet, ArrayLength, Arrayp,
    Gc, GcEnable, GcDisable, GcEnabledP, ArenaStats,
}
```

Adding a new builtin:
1. Add variant to `define_builtins!` macro in `lisp_parser`
2. Implement evaluation in `apply_builtin` in `lisp_eval`

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
- The `StdLib` value stores a `cache` field (NULL until first call, then `(body . params)`)
- First call parses the body and caches it via `Lisp::set_stdlib_cache()`
- Subsequent calls reuse the cached AST via `Lisp::stdlib_cache()`

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

### 1. Nil is Truthy

In this Lisp, only `#f` is false. `nil`/`()` is the empty list and is truthy:

```lisp
(if nil 'yes 'no)   ; => yes
(if '() 'yes 'no)   ; => yes
(if #f 'yes 'no)    ; => no (only #f is false)
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
