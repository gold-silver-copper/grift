# Architecture

Grift is a vau-calculus Lisp interpreter (Shutt 2010) that runs entirely on a
fixed-size arena allocator with no heap allocation, no standard library, and no
unsafe code. This document describes the system architecture: how memory is
managed, how values are represented, how evaluation works, and why the design
is the way it is.

## Crate Structure

```
grift_arena     Fixed-size arena allocator with free-list, mark-and-sweep GC
grift_unicode   Unicode character operations (case mapping, folding, properties)
grift           Parser, evaluator, value types, Lisp API — depends on grift_arena
```

`grift_arena` is a general-purpose arena with no knowledge of Lisp. `grift`
stores `Value` enums in the arena and implements the language on top of it. The
`grift_unicode` crate is currently a standalone utility not yet wired into the
interpreter.

## Arena Allocator

### Memory Layout

The arena is a flat array of `N` slots, where `N` is a const generic:

```
Arena<Value, N>
┌────────────────────────────────────────────────────────┐
│ slots: [Cell<Slot<Value>>; N]                          │
│ free_head: Cell<usize>       ← head of free list       │
│ len: Cell<usize>             ← count of occupied slots  │
│ gc_enabled: Cell<bool>                                  │
└────────────────────────────────────────────────────────┘
```

Each slot is a tagged union:

```rust
enum Slot<T: Copy> {
    Free { next_free: usize },     // linked-list pointer to next free slot
    Occupied { value: T },         // the stored value
}
```

Free slots form an intrusive singly-linked list. Allocation pops the head of the
free list; deallocation pushes onto it. Both are O(1).

### Interior Mutability

The arena uses `Cell<Slot<T>>` rather than `RefCell`. Since `T: Copy`, the arena
copies values in and out rather than lending references. This eliminates runtime
borrow checking overhead and the possibility of borrow panics, at the cost of
requiring all stored types to implement `Copy`.

### Contiguous Allocation

Strings require contiguous storage for their character data. The arena provides
`alloc_contiguous(count, default)` which performs a linear scan for `count`
consecutive free slots, removes them from the free list in a single pass, and
marks them occupied. This is O(N) in the worst case but is only used for string
allocation.

### Index Design

`ArenaIndex` wraps a `usize` and directly indexes the slot array. Slot 0 is
permanently allocated as `Value::Nil`, and `ArenaIndex::NIL` is a constant
pointing to slot 0. This means nil checks are a comparison against zero with
no arena access required.

## Value Representation

Every Lisp value is a `Value` enum stored in one arena slot. The design
constraint is that each variant can inline at most two `ArenaIndex`-sized
fields (i.e., two `usize` values), keeping the enum small and `Copy`.

```rust
enum Value {
    Nil,                                    // the empty list
    Boolean(bool),                          // #t, #f
    Number(isize),                          // integer
    Char(char),                             // Unicode character
    Symbol(ArenaIndex),                     // -> String value holding the name
    Cons { car: ArenaIndex, cdr: ArenaIndex },
    String { len: usize, data: ArenaIndex },// -> contiguous Char slots
    Operative { params_envparam: ArenaIndex, body_env: ArenaIndex },
    Applicative(ArenaIndex),                // -> inner combiner
    Builtin(BuiltinId),                     // Rust-native primitive (u8 id)
    Environment { bindings: ArenaIndex, parents: ArenaIndex },
    Inert,                                  // #inert — side-effect return value
    Ignore,                                 // #ignore — parameter tree wildcard
}
```

### Why Each Variant Exists

**Nil** — The empty list and false-in-context sentinel. Pre-allocated at slot 0.

**Boolean** — Kernel (and Scheme) distinguish booleans from other types. `#t`
and `#f` are pre-allocated at startup to avoid repeated allocation.

**Number** — Integer arithmetic. Inlined as `isize` (platform-width) for zero
indirection on numeric operations.

**Char** — Stored inline. Used as the element type for String character data
in contiguous arena regions.

**Symbol** — An interned name. Points to a `String` value. Symbol interning
is done by linear scan: `Lisp::symbol()` searches all allocated symbols and
returns an existing one if the name matches, or allocates a new one.

**Cons** — Immutable pair. Car and cdr are both `ArenaIndex`. Pairs are the
backbone of all compound data: lists, parameter trees, environment bindings,
and the AST itself.

**String** — A length plus a pointer to contiguously allocated `Char` slots.
Strings are not interned (symbols are).

**Operative** — The fundamental combiner type (fexpr). Packs four fields into
two `ArenaIndex` values via cons cells: `params_envparam` is `(params . env-param)`
and `body_env` is `(body . closed-env)`. Created by `vau`.

**Applicative** — A wrapper around any combiner that evaluates arguments before
delegating. Created by `wrap`. The `lambda` operative is sugar for
`(wrap (vau params #ignore body))`.

**Builtin** — Rust-native primitive. Wraps a `BuiltinId(u8)` supporting up to
256 builtins. Builtins are operatives at the lowest level — the ones exposed as
applicatives (like `+`, `cons`) are wrapped in `Applicative` at init time.

**Environment** — First-class, mutable environment frame. `bindings` is an
alist (list of `(symbol . value)` pairs), `parents` is a list of parent
environments. The only mutable type in the system: `env_define` replaces the
environment's `bindings` field via `arena.set`.

**Inert** — The return value of side-effecting operatives (`define!`, `set!`).
Per the Kernel specification, these return `#inert` rather than an unspecified
value.

**Ignore** — Used in formal parameter trees to indicate positions whose
argument should be discarded. Written `#ignore`.

### Why No Lambda Variant

Lambda is not a primitive. It is derived: `(lambda params body)` expands to
`(wrap (vau params #ignore body))`. The evaluator never sees a Lambda value;
it sees an `Applicative` wrapping an `Operative`. This follows the vau calculus
directly: the operative is the sole primitive combiner, and the applicative is
a derived wrapper.

### Why Immutable Pairs

Pairs (cons cells) are immutable. There is no `set-car!` or `set-cdr!`. This
simplifies GC (no write barrier needed), enables structural sharing, and matches
the Kernel specification's design philosophy. Environments are the only mutable
type because binding mutation is essential for `define!` and `set!`.

### Why No call/cc

`call/cc` (call with current continuation) requires capturing the entire
evaluation stack as a first-class object. This is incompatible with the
fixed-arena, no-alloc design: continuations would need unbounded storage and
would defeat the purpose of a resource-constrained runtime. The vau calculus
provides sufficient power through first-class environments and operatives
without requiring reified continuations.

## Environment Model

Environments are first-class values (they can be passed to and returned from
combiners). Each environment is a frame with:

- **bindings**: an alist of `(symbol . value)` pairs
- **parents**: a list of parent environments (not a single parent)

Lookup walks the parent chain. The fast path handles the common case of
single-parent chains with zero allocation: it iterates up the chain in a loop.
Multi-parent environments (created by `make-environment`) fall back to a DFS
with cycle detection via a visited-set allocated as a cons list in the arena.

### Ground and Standard Environments

Per the Kernel specification (§3.2), there are two distinguished environments:

1. **Ground environment** — Contains all builtin bindings. Immutable: attempts
   to `define!` or `set!` into it signal an error.
2. **Standard environment** — A child of the ground environment. This is where
   top-level user expressions are evaluated.

This separation ensures that user code cannot shadow or mutate primitive bindings
in a way that corrupts the interpreter.

## Combiner System

The evaluation model has three combiner types:

| Type | Receives | Created by |
|------|----------|------------|
| **Operative** (compound fexpr) | Unevaluated operands + caller environment | `vau` |
| **Applicative** (wrapper) | Evaluated arguments | `wrap` (or `lambda`) |
| **Builtin** (Rust-native operative) | Unevaluated operands + caller environment | `init_builtins` |

When a combination `(f arg1 arg2 ...)` is evaluated:

1. The evaluator evaluates `f` to get a combiner value.
2. If the combiner is an **Operative** or **Builtin**: the operands are passed
   unevaluated, along with the caller's environment. The operative body can
   choose to evaluate them (or not) using `eval`.
3. If the combiner is an **Applicative**: the evaluator first evaluates all
   operands left-to-right, then passes the evaluated arguments to the
   underlying combiner (which may be an Operative, Builtin, or another
   Applicative).

This is the key insight of the vau calculus: macros are unnecessary because
operatives have direct access to unevaluated syntax and the caller's
environment. Fexprs (first-class operatives) subsume both functions and macros
in a single mechanism.

### Why Fexprs Over Macros

Traditional Lisps separate evaluation-time concerns into two mechanisms:
functions (which receive evaluated arguments) and macros (which receive
unevaluated syntax and produce code). The vau calculus unifies these with
operatives, which receive unevaluated operands and the dynamic environment
at call time. This:

- Eliminates the need for a separate macro expansion phase.
- Makes the evaluation model uniform: there is one dispatch mechanism, not two.
- Gives user-defined operatives the same power as special forms.
- Avoids hygiene issues inherent in traditional macro systems, since the
  operative receives the actual caller environment rather than performing
  textual substitution.

## Tail-Call Optimization

The evaluator uses a trampoline loop for TCO. The `eval` method is a `loop {}`
that re-enters itself by mutating `expr` and `env` in place:

```rust
pub fn eval(&mut self, mut expr: ArenaIndex, mut env: ArenaIndex) -> ArenaResult<ArenaIndex> {
    loop {
        match self.lisp.get(expr)? {
            Value::Symbol(_) => return self.lisp.env_lookup(env, expr),
            Value::Cons { car, cdr } => {
                // ... dispatch combiner ...
                // For tail calls: update expr and env, then `continue`
                // For non-tail calls: `return` the result
            }
            _ => return Ok(expr), // self-evaluating
        }
    }
}
```

Operative builtins signal their intent via `TailAction`:

```rust
enum TailAction {
    Return(ArenaResult<ArenaIndex>),  // non-tail: return immediately
    Continue,                          // tail: re-enter the eval loop
}
```

The `tail_continue!` and `non_tail!` macros generate the appropriate
`TailAction` for operative implementations. Tail-position operatives like `if`,
`begin`, `cond`, `and`, `or`, and `let` set `*expr` and `*env` and return
`TailAction::Continue`. Non-tail operatives like `define!`, `lambda`, and `vau`
return `TailAction::Return`.

## Garbage Collection

### Mark-and-Sweep

The arena implements mark-and-sweep GC via the `Trace` trait. The algorithm:

1. **Root initialization**: Mark all root indices and push them onto a mark stack.
2. **Mark phase**: Iteratively pop the mark stack, trace each object's children
   via `Trace::trace_with_arena`, and mark/push any unmarked children. Uses
   batching (16 children per iteration) to bound per-step work.
3. **Sweep phase**: Linear scan of all slots; free any that are occupied but
   not marked.

The mark bitmap and mark stack are both `[_; N]` arrays allocated on the Rust
stack (not the arena), so GC itself requires no arena allocation.

### GC Roots

The evaluator maintains a shadow stack of GC roots as a cons-list in the arena
(`gc_roots`). Before any allocation-followed-by-eval sequence (where the eval
might trigger GC), the evaluator pushes live values onto this root stack:

```rust
self.push_root(cdr);    // protect operands
self.push_root(env);    // protect environment
let result = self.eval(car, env)?;
self.pop_roots(2);      // restore
```

When GC triggers, the root set includes:
- The current `expr` and `env`
- `ground_env` and `global_env`
- The `gc_roots` shadow stack
- Pre-allocated singleton indices (`true_idx`, `false_idx`, `inert_idx`, `ignore_idx`)

### GC Trigger

GC is triggered when arena occupancy exceeds 75%:

```rust
fn maybe_collect(&self, expr: ArenaIndex, env: ArenaIndex) {
    if self.lisp.arena.len() > self.lisp.arena.capacity() * 3 / 4 {
        self.collect_garbage(expr, env);
    }
}
```

GC can be disabled at runtime via `arena.set_gc_enabled(false)` for batch
operations or debugging.

### String Tracing

Strings require special GC treatment because their character data is stored in
contiguous slots that must all be marked. The `Trace` implementation for `Value`
overrides `trace_with_arena` to read the string's `len` field from the arena and
trace each character slot individually. The basic `trace` method only traces the
`data` pointer (the first character slot), which is sufficient for the basic
reference-following but not for keeping all character slots alive.

## Data Flow

Source string to result:

```
"(+ 1 2)"
    │
    ▼
┌─────────┐   S-expression tokenization
│ Parser   │   Recursive descent on byte slice
│          │   Produces arena-allocated cons tree
└────┬─────┘
     │  ArenaIndex (root of AST)
     ▼
┌──────────┐
│ Evaluator │   Trampoline loop with TCO
│           │   Symbol lookup → env chain walk
│           │   Combination → combiner dispatch
│           │   GC triggered at 75% occupancy
└────┬──────┘
     │  ArenaIndex (result)
     ▼
┌──────────┐
│ arena.get │   Copy Value out of arena
└────┬──────┘
     │  Value
     ▼
  Value::Number(3)
```

The parser operates directly on a `&[u8]` slice of the input. It allocates
cons cells, numbers, symbols, strings, and booleans in the arena as it parses.
There is no intermediate AST representation — the parsed S-expression is
itself the AST, stored as arena-allocated cons trees.

The evaluator walks this tree, dispatching on `Value` variants. Symbols trigger
environment lookup. Cons cells trigger combiner dispatch. Everything else is
self-evaluating.

## Pre-allocated Singletons

To avoid allocation on every boolean or nil result, the `Lisp` struct
pre-allocates five values at startup:

| Slot | Value | Field |
|------|-------|-------|
| 0 | `Nil` | `ArenaIndex::NIL` (constant) |
| 1 | `Boolean(true)` | `true_idx` |
| 2 | `Boolean(false)` | `false_idx` |
| 3 | `Inert` | `inert_idx` |
| 4 | `Ignore` | `ignore_idx` |

Returning these common values is then a matter of returning the stored index
with no arena allocation.
