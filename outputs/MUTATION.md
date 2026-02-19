# Grift Mutation Surface Audit

A comprehensive audit of every mutation point in the Grift codebase,
documenting what is mutable, what is immutable, and the security
guarantees that follow.

---

## 1. Mutation Policy

Environments are the only user-mutable type in Grift. All other
values — pairs, symbols, numbers, strings, booleans, operatives,
applicatives, inert, ignore — are permanently immutable after
allocation. No user-facing operation can modify them.

The arena allocator (`grift_arena`) provides a `set()` method that can
overwrite any slot, but the evaluator restricts its use to environment
bindings and internal bookkeeping. The immutability of non-environment
types is enforced by the absence of mutation operations in the
evaluator, not by a runtime guard in the arena itself.

---

## 2. User-Facing Mutation Operations

Two operations perform user-visible mutation. Both are operatives
(they receive unevaluated arguments) and both return `#inert`.

### `define!`

**Syntax:** `(define! symbol expr)` or `(define! (fn name params...) body...)`

**What is mutated:**

- If the symbol already exists in the current environment frame, the
  binding's cons cell is overwritten via `arena.set(binding, Cons {
  car: name, cdr: new_value })`. Only the cdr (value) changes; the
  car (symbol) remains the same.

- If the symbol does not exist, a new binding pair is consed onto the
  bindings list, and the environment slot itself is overwritten via
  `arena.set(env, Environment { bindings: new_bindings, parents })`
  to point to the extended list.

**Scope of mutation:** Single frame only. `define!` never walks
parent environments. It operates exclusively on the current
evaluation environment (`*env`).

**Ground environment protection:** Before any mutation, `op_define`
checks whether `*env == ArenaIndex::GROUND_ENV`. If so, it returns
`Err(ArenaError::ImmutableEnvironment)` immediately. This prevents
user code from redefining builtins.

**Closure observation:** Because environments are shared by reference,
any closure that closes over the same environment frame will observe
the redefinition. If a closure captures a frame and a later `define!`
overwrites a binding in that frame, the closure sees the new value on
its next evaluation.

### `set!`

**Syntax:** `($set! env-expr symbol value-expr)`

**What is mutated:** The binding cons cell in the target environment
is overwritten via `arena.set(binding, Cons { car: name, cdr:
new_value })`. Only the value (cdr) of an existing binding changes.

**Scope of mutation:** Single frame only. `set!` does not walk parent
chains. It searches only the target environment's own bindings list.
If the symbol is not found in that frame, it signals
`UnboundVariable` — it never creates a new binding.

**Ground environment protection:** `op_set` checks whether
`target_env == ArenaIndex::GROUND_ENV` before performing any
mutation. If so, it returns `Err(ArenaError::ImmutableEnvironment)`.

**Closure observation:** Identical to `define!`. All closures sharing
the target frame observe the updated binding immediately.

---

## 3. Internal Mutation (Not User-Visible)

Six `arena.set()` calls exist in the production codebase. Two are
user-facing (documented above in `env_define` and `env_set`). The
remaining four are internal-only.

### `reverse_list` — argument list reversal

**Location:** `crates/grift/src/eval.rs`, `reverse_list()`

**What it does:** During applicative evaluation, `eval_args` builds
the evaluated argument list by consing each result onto the front of
an accumulator. This produces the arguments in reverse order. After
evaluation completes, `reverse_list` walks the list and swaps each
cons cell's cdr pointer via `arena.set(list, Cons { car, cdr: prev })`
to reverse it in place.

**Why it is safe:** The cons cells being mutated were just allocated
by `eval_args` in the same evaluation step. They have not been
returned to user code, stored in any environment, or made reachable
from any user-visible data structure. By the time the applicative's
body executes, the reversal is complete and the cells are in their
final, immutable state.

### `set_gc_roots_head` — GC root stack management

**Location:** `crates/grift/src/eval.rs`, `set_gc_roots_head()`

**What it does:** Overwrites the `ArenaIndex::GC_ROOTS` sentinel slot
(a well-known arena index allocated at startup) to maintain a stack
of values that must survive garbage collection. The slot is a cons
cell whose car points to the current root chain head.

**Why it is safe:** `GC_ROOTS` is an internal bookkeeping slot. It is
not bound in any environment and is not reachable from user code.
There is no builtin that exposes or returns this index. The mutation
is purely interpreter infrastructure.

### Symbol interning — intern list maintenance

**Location:** `crates/grift/src/lisp.rs`, `symbol()`

**What it does:** When a new symbol is created, it is prepended to
the global intern list. The `ArenaIndex::INTERN_LIST` sentinel slot's
car is overwritten via `arena.set(ArenaIndex::INTERN_LIST, Cons {
car: new_head, cdr })` to point to the updated list head.

**Why it is safe:** `INTERN_LIST` is an internal bookkeeping slot
allocated at startup. It is not bound in any environment and is not
accessible from user code. The mutation maintains the symbol
deduplication table, ensuring that `(eq? 'foo 'foo)` returns `#t`.

### Ground environment initialization

**Location:** `crates/grift/src/eval.rs`, `register_builtin()`

**What it does:** During `Lisp::new()`, each builtin is registered
by calling `env_define(ArenaIndex::GROUND_ENV, sym, val)`, which
mutates the ground environment's bindings list. This is the same
`arena.set()` path used by user-facing `define!`.

**Why it is safe:** This mutation occurs only during interpreter
construction, before any user code is evaluated. Once construction
completes, the ground environment is protected by the
`ImmutableEnvironment` check in both `op_define` and `op_set`.

---

## 4. What Cannot Be Mutated

The following types are permanently immutable after allocation. No
user-facing operation modifies them.

### Pairs (Cons)

There is no `set-car!`, `set-cdr!`, or any other pair mutation
operation. Once a cons cell is allocated, its car and cdr are fixed
for the cell's lifetime. The only exception is the internal
`reverse_list` function, which mutates freshly allocated cells before
they become user-visible.

### Strings (CharPair)

There is no `string-set!` or character mutation operation. Strings
are stored as linked lists of `CharPair { ch, cdr }` nodes. Once
allocated, the character sequence is immutable.

### Symbols

Symbols are interned and identity-compared. Once a symbol is created,
its name (a pointer to a CharPair chain) cannot be changed. Two
symbols with the same name always resolve to the same arena index.

### Numbers

Numbers are value types stored directly in the arena slot. There is
no mutation operation on numbers — arithmetic produces new values.

### Booleans

`#t` and `#f` are singleton constants at well-known arena indices
(`ArenaIndex::TRUE` and `ArenaIndex::FALSE`). They cannot be
overwritten or mutated.

### Operatives

An operative closes over a parameter tree, an environment formal, a
body expression, and a static environment. None of these fields can
be modified after creation. The closed-over environment can be
mutated (it is an environment), but the operative's reference to it
— and the body, params, and env-formal — are fixed.

### Applicatives

An applicative is a wrapper around a combiner (typically an
operative). The wrapper itself cannot be changed — there is no
operation to rewrap or unwrap-and-replace an applicative's underlying
combiner.

### Inert

`#inert` is a singleton constant at `ArenaIndex::INERT`. It is
the return value of side-effecting operations like `define!` and
`set!`. It cannot be mutated.

### Ignore

`#ignore` is a singleton constant at `ArenaIndex::IGNORE`. It is
used in parameter trees to discard bindings. It cannot be mutated.

---

## 5. Security Properties

The mutation policy produces the following security guarantees.

### No aliased mutable data

Two references to the same pair always see the same data forever.
Because cons cells are immutable after allocation, there is no
"spooky action at a distance" through shared mutable pairs. If two
closures hold a reference to the same list, neither can modify it.
This eliminates an entire class of aliasing bugs common in mutable
Lisps.

### No code mutation

Operative bodies are pair trees (S-expressions). Since pairs are
immutable, an operative's behavior cannot be changed after creation.
This eliminates the attack vector that `copy-es-immutable` defends
against in Kernel (R⁻¹RK §4.7.2). In Grift, there is no need for
defensive copying because the original is already immutable.

### No string mutation

Symbol names and string data cannot be modified. A symbol's identity
is stable for its lifetime. There is no way to change a symbol's name
after interning, which means symbol-based dispatch is reliable and
tamper-proof.

### Controlled mutation scope

`define!` only affects the current frame. `set!` only affects a frame
the caller holds a reference to. Neither walks parent chains for
mutation. Parent environments are visible (readable via variable
lookup) but not mutable unless the caller explicitly holds a
reference to the parent frame and uses `set!` on it.

### Ground environment protection

The ground environment containing all builtins is immutable. User
code cannot redefine `+`, `if`, `vau`, or any primitive. Both
`op_define` and `op_set` check for `ArenaIndex::GROUND_ENV` and
return `ImmutableEnvironment` before performing any mutation. Users
can shadow builtins in child environments but cannot destroy them.

### Sandbox capability

`make-environment` with no parents creates a truly empty scope.
`make-environment` with a selected parent creates a scope with only
inherited bindings. The host can construct restricted environments
that expose a subset of operations. Since the ground environment is
immutable and parent chains are fixed at construction, a sandboxed
environment cannot escalate its privileges. A sandbox that does not
inherit `eval`, `set!`, or `make-environment` cannot gain access to
those operations.

---

## 6. Known Limitations and Caveats

### `define!` overwrite

Closures sharing a frame see redefinitions. This is by design but
can surprise users. A closure that references a function which is
later redefined via `define!` will call the new version on subsequent
invocations. This is consistent with Kernel semantics but differs
from languages with lexical immutability.

### Environment references are capabilities

If user code obtains a reference to an environment via
`current-environment` or vau's env-param, it can mutate that
environment with `set!`. The security boundary is who gets the
reference, not what operations exist. A vau that binds its caller's
environment gives the operative the ability to mutate the caller's
scope. Careful use of `#ignore` as the env-formal (or using `lambda`
instead of raw `vau`) avoids leaking environment references.

### GC root stack mutation

The internal GC root stack is maintained via `arena.set()` on the
well-known `ArenaIndex::GC_ROOTS` slot. This is not user-accessible
but is a mutation that exists in the implementation. It is modified
on every `push_root` and `pop_roots` call during evaluation.

### `reverse_list` mutation

Freshly allocated cons cells are mutated during argument evaluation.
This is safe because the cells are not yet reachable from user code,
but it means the implementation is not purely functional internally.
The mutation window is limited to the `eval_args` / `reverse_list`
sequence and is invisible to user programs.

### Symbol intern list mutation

The `ArenaIndex::INTERN_LIST` slot is mutated every time a new
symbol is interned. This is interpreter bookkeeping and not
user-accessible, but it is a persistent mutation in the arena.

### No runtime enforcement

The immutability of non-environment types is enforced by the absence
of mutation operations, not by a runtime check. If a future code
change accidentally adds an `arena.set()` call on a non-environment
slot reachable from user code, the invariant breaks silently.
Consider adding debug assertions or an `is_environment` guard in
`arena.set()` in the future to catch accidental violations during
development.
