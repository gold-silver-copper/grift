# Internals

Implementation guide for contributors. Covers module structure, key data types,
hot paths, the macro system, arena allocation patterns, and GC interaction.

## Module Structure

### `grift_arena` (no_std, no dependencies)

```
src/
├── lib.rs       Re-exports and module declarations
├── types.rs     ArenaIndex, ArenaError, ArenaResult, Slot (internal)
├── arena.rs     Arena<T, N> — alloc, free, get, set
├── traits.rs    ArenaDelete, ArenaCopy, Trace
├── gc.rs        Mark-and-sweep: initialize_roots, process_mark_stack, sweep_unmarked
├── iter.rs      ArenaIterator
└── stats.rs     ArenaStats, GcStats
```

All public API types are re-exported from `lib.rs`. The `Slot` enum and
`FREE_LIST_END` sentinel are `pub(crate)`.

### `grift` (no_std, depends on grift_arena)

```
src/
├── lib.rs       Crate root — #![no_std], #![forbid(unsafe_code)], re-exports
├── value.rs     Value enum, BuiltinId, Display impl, accessor macros
├── lisp.rs      Lisp<N> — arena wrapper, symbol interning, env operations, Trace impl
├── parse.rs     Parser — recursive descent S-expression parser
└── eval.rs      Evaluator — TCO trampoline, builtin registration, all operatives/applicatives
```

### `grift_unicode` (no_std, no dependencies)

```
src/
├── lib.rs       Character operations, case mapping, full case folding
└── build.rs     Code generator: parses CaseFolding.txt into a static lookup table
```

Currently standalone. Not yet wired into the interpreter's string operations.

## Key Data Types

### `ArenaIndex`

A `usize` wrapper. Slot 0 is `NIL`. The `is_nil()` check is `self.0 == 0`.
Comparison is by raw index — two indices are equal iff they point to the same
slot. This is the fundamental identity mechanism for the language.

### `Value`

A `Copy` enum occupying (on 64-bit platforms) approximately 24 bytes: a tag
byte plus up to two `usize` fields. The two-field limit is a deliberate design
constraint. Variants that need more data (Operative, String) pack multiple
logical fields into cons cells or contiguous regions.

### `Lisp<N>`

Owns the `Arena<Value, N>` and pre-allocated singleton indices. Provides the
"userland" API: `eval`, `symbol`, `cons`, `car`, `cdr`, environment operations.
The const generic `N` determines the arena capacity at compile time.

### `Evaluator<'a, N>`

Borrows a `&'a Lisp<N>`. Holds the ground environment, standard environment,
and GC root stack. Created fresh for each `Lisp::eval()` call. Contains all
builtin implementations as methods.

### `Parser<'a>`

A cursor over a `&[u8]` byte slice. Stateless between calls — `parse()` reads
one expression and advances the cursor. No intermediate token stream; the parser
directly allocates arena values.

## Hot Paths and Their Optimization

### eval loop

The `Evaluator::eval` method is the performance-critical path. Key
optimizations:

- **Trampoline TCO**: Tail calls update `expr`/`env` and `continue` rather
  than recursing. This means `(countdown 1000000)` uses constant Rust stack.
- **Inline self-eval check**: The match on `Value` starts with `Symbol` and
  `Cons` (the two non-self-evaluating types). Everything else hits the `_`
  arm and returns immediately.
- **GC check at loop head**: `maybe_collect` runs once per eval iteration.
  The 75% threshold avoids running GC on every allocation while preventing
  OOM during deep computation.

### Environment lookup

`env_lookup` has a fast path for the common case (single-parent chain):

```rust
while !cur.is_nil() {
    // Search bindings alist
    // If single parent: cur = first_parent; continue
    // If multi-parent: fall back to DFS
}
```

This avoids any allocation for the common case. The DFS fallback (for
`make-environment` with multiple parents) allocates a visited-set as a cons
list in the arena.

### Argument evaluation

`eval_args` builds the result list in reverse (to avoid O(n) Rust stack depth
from recursive `cons`), then reverses in place by swapping `cdr` pointers.
The reversal is O(n) with no allocation.

### Symbol interning

`Lisp::symbol()` does a linear scan of all arena slots to find an existing
symbol with the same name. This is O(arena size) and a potential bottleneck
for workloads that create many symbols. A hash table would be faster but
would require either unsafe code or heap allocation.

## Macro System

The codebase uses Rust `macro_rules!` macros extensively to generate
repetitive code. These are compile-time macros, not Lisp macros.

### `define_builtins!`

The central registration macro in `eval.rs`. Takes a declarative table of
operatives and applicatives and generates:

1. **BuiltinId constants**: `const op_quote: BuiltinId = BuiltinId(0);` etc.
2. **`init_builtins` method**: Loops through the table, binding each name in
   the ground environment. Operatives are bound as raw `Builtin` values;
   applicatives are wrapped in `Applicative`.
3. **`apply_operative_builtin` dispatch**: A match on `BuiltinId` that routes
   to the corresponding operative method.
4. **`apply_builtin_pure` dispatch**: Same for applicative builtins.

The macro uses a recursive `@ids` sub-macro to assign sequential `u8` IDs
starting from 0.

### `tail_continue!` and `non_tail!`

Control flow macros for operative implementations:

- `tail_continue!({ ... })` — Wraps a block in an IIFE. If the block
  completes with `Ok(())`, returns `TailAction::Continue` (re-enter eval
  loop). If it returns `Err(e)`, returns `TailAction::Return(Err(e))`.
- `non_tail!({ ... })` — Wraps a fallible block and returns
  `TailAction::Return(result)`.

### `type_predicate!`

Generates a variadic type predicate method:

```rust
type_predicate!(builtin_nullp, Value::Nil);
// Generates a method that walks the argument list and checks each value
// against the pattern Value::Nil, returning #t or #f.
```

### `fold_numbers!`

Generates a fold over a variadic argument list using a checked arithmetic
operation:

```rust
fold_numbers!(self, args, 0, checked_add)
// Folds over args starting at 0, applying isize::checked_add
```

### `cmp_builtin!`

Generates a two-argument numeric comparison:

```rust
cmp_builtin!(builtin_lt, <);
// Generates: extract two numbers, return boolean of a < b
```

### `pair_builtin!`

Generates a single-argument pair accessor:

```rust
pair_builtin!(builtin_car, car);
// Generates: extract pair from first arg, call self.lisp.car
```

### `value_accessor!`

In `value.rs`, generates type-safe extraction methods on `Value`:

```rust
value_accessor!(as_number -> isize, Value::Number(n) => n);
// Generates: fn as_number(self) -> Result<isize, ArenaError>
```

### `impl_from_value!`

Generates `From<T> for Value` implementations in bulk:

```rust
impl_from_value!(bool => Boolean, isize => Number, char => Char, BuiltinId => Builtin);
```

## Arena Allocation Patterns

### Two-field packing

When a Value variant needs more than two fields, the extra fields are packed
into cons cells. The Operative variant stores four logical fields in two:

```
Operative {
    params_envparam: (params . env-param),   // cons cell
    body_env: (body . closed-env),           // cons cell
}
```

Extraction uses `vau_parts()` which reads two cons cells to recover all four
fields.

### String storage

Strings are stored as a header slot pointing to a linked list of character slots:

```
String { data: @100 }
  @100: Char { ch: 'h', cdr: @101 }
  @101: Char { ch: 'e', cdr: @102 }
  @102: Char { ch: 'l', cdr: @103 }
  @103: Char { ch: 'l', cdr: @104 }
  @104: Char { ch: 'o', cdr: NIL }
```

Each `Char` node inlines a `cdr` pointer to the next character.
The final character's `cdr` points to `NIL`.

### Alist environments

Environment bindings are association lists (lists of pairs):

```
Environment {
    bindings: ((sym1 . val1) (sym2 . val2) ...),
    parents: (parent-env1 parent-env2 ...)
}
```

New bindings are prepended. Lookup is O(n) in the number of bindings in each
frame, which is acceptable for small frames. Shadowing works naturally: the
most recent binding is found first.

## GC Interaction

### When GC can trigger

GC is checked at the top of each `eval` loop iteration via `maybe_collect`.
It will only actually collect when arena occupancy exceeds 75%. GC can also
be triggered manually via `Lisp::collect_garbage`.

### Root discipline

Any `ArenaIndex` that is live across an `eval` call (which may allocate and
trigger GC) must be pushed onto the root stack:

```rust
self.push_root(cdr);    // protect before eval
self.push_root(env);
let result = self.eval(car, env)?;
self.pop_roots(2);      // restore after eval
```

Failure to root a value can cause it to be collected mid-evaluation. The
root stack is itself a cons list in the arena, so pushing a root requires
one cons allocation.

### Trace implementation

The `Trace` impl for `Value` in `lisp.rs` traces all `ArenaIndex` fields:

- Cons, Operative, Environment: trace both fields
- Applicative: trace the inner combiner
- Symbol: trace the string index
- String: trace all character data slots (via `trace_with_arena`)
- Nil, Boolean, Number, Char, Builtin, Inert, Ignore: no references

The `trace_with_arena` override for String is critical: the basic `trace`
method only traces the `data` pointer, but the GC needs to keep all `len`
character slots alive.

## How to Add a New Builtin

1. **Choose operative or applicative**. Operatives receive unevaluated args;
   applicatives receive evaluated args.

2. **Add to `define_builtins!`** in `eval.rs`:
   ```rust
   operatives {
       "my-op" => op_my_op => op_my_op,
   }
   // or
   applicatives {
       "my-fn" => bi_my_fn => builtin_my_fn,
   }
   ```

3. **Implement the method** on `Evaluator`:
   - For operatives: `fn op_my_op(&mut self, args: ArenaIndex, expr: &mut ArenaIndex, env: &mut ArenaIndex) -> TailAction`
   - For applicatives: `fn builtin_my_fn(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex>`

4. **Use macros** for common patterns: `type_predicate!`, `cmp_builtin!`,
   `pair_builtin!`, `fold_numbers!`.

5. **Root discipline**: If your implementation calls `self.eval()`, push any
   live values onto the root stack first.

## How to Add a New Value Variant

1. **Add the variant** to `enum Value` in `value.rs`. Respect the two-field
   constraint: at most two `ArenaIndex`-sized fields per variant.

2. **Update `type_name()`** to return a name string.

3. **Update `is_self_evaluating()`** — most new types should be
   self-evaluating (return true).

4. **Update `is_immutable()`** — if the type has value-based `eq?` semantics,
   add it to the immutable set.

5. **Update `Display`** to print the type.

6. **Update `Trace`** in `lisp.rs` — add tracing for any `ArenaIndex` fields
   in the new variant. If the variant stores metadata in the arena (like
   String's length), override `trace_with_arena`.

7. **Add accessor** via `value_accessor!` if needed.

8. **Add type predicate** via `type_predicate!` and register it in
   `define_builtins!`.

## Invariants

These must be maintained for correctness:

1. **Slot 0 is always Nil**. `ArenaIndex::NIL` (which is 0) must always resolve
   to `Value::Nil`. The `Lisp::new()` constructor allocates Nil at slot 0
   before anything else.

2. **Symbols are interned**. Two symbols with the same name must have the same
   `ArenaIndex`. `Lisp::symbol()` enforces this via linear scan.

3. **Ground environment is immutable**. `define!` and `set!` check for
   `env == self.ground_env` and reject mutation.

4. **GC roots span eval calls**. Any `ArenaIndex` live across a recursive
   `eval` call must be pushed onto the root stack. Forgetting this causes
   use-after-free (in the arena sense: the slot gets recycled).

5. **Parameter trees are validated**. Before `vau` or `define!` binds
   parameters, the tree is checked for cycles and duplicate symbols.

6. **All arena types are Copy**. The arena uses `Cell` for interior mutability,
   which requires `T: Copy`.

7. **No unsafe code**. Both `grift` and `grift_arena` use `#![forbid(unsafe_code)]`.

8. **No heap allocation in core crates**. `Vec`, `String`, `Box`, and
   `alloc::` are forbidden. Only the REPL binary (behind the `repl` feature)
   uses `std`.
