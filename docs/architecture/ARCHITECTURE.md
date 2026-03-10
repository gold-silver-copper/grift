# Grift Architecture

This file is the canonical architecture note for Grift. It replaces the older
split notes in `SPEC.md`, `LANGUAGE.md`, `INTERNALS.md`, `MUTATION.md`, and
`GOTCHAS.md`.

The companion language-level specification lives in
`docs/architecture/LANGUAGE_SPECIFICATION.md`.

Everything below is cross-checked against the current implementation in
`crates/grift/src` and the behavior exercised by `crates/grift/tests/lisp_tests.rs`.

## Overview

Grift is a `#![no_std]`, `#![forbid(unsafe_code)]` Lisp interpreter built around
the Kernel/vau model:

- all dynamic values live in a fixed-size `Arena<Value, N>`
- there is no heap allocation
- operatives are the primitive combiners
- applicatives are wrappers that evaluate operands first
- evaluation uses a trampoline for tail calls
- garbage collection is mark-and-sweep, triggered on OOM or by `(gc-collect)`

The crate layout is:

- `crates/grift/src/arena.rs`: fixed-size free-list arena and GC
- `crates/grift/src/value.rs`: `Value`, `BuiltinId`, value predicates/accessors
- `crates/grift/src/lisp.rs`: `Lisp<N>`, symbol interning, environment helpers,
  formatting, string helpers, public API
- `crates/grift/src/parse.rs`: recursive-descent reader over byte slices and
  `CharPair` chains
- `crates/grift/src/eval.rs`: evaluator, builtin registration, tail-call loop,
  GC integration
- `crates/grift/src/native.rs`: Rust native-function registration API
- `crates/grift/src/prelude.rs`: lazily-loaded prelude entries generated from
  `prelude.grift`

## Reserved Arena Slots

`Lisp::new()` allocates slots `0..10` up front and relies on their indices being
stable:

| Slot | `ArenaIndex` | Meaning |
|------|--------------|---------|
| 0 | `NIL` | nil / empty list / empty string |
| 1 | `TRUE` | `#t` |
| 2 | `FALSE` | `#f` |
| 3 | `INERT` | `#inert` |
| 4 | `IGNORE` | `#ignore` |
| 5 | `GROUND_ENV` | builtin-only environment |
| 6 | reserved parent-list cell | cons cell holding the ground parent list for global |
| 7 | `GLOBAL_ENV` | standard user-visible top-level environment |
| 8 | `GC_ROOTS` | cons cell whose `car` is the temporary root stack head |
| 9 | `INTERN_LIST` | cons cell whose `car` is the symbol intern list head |

`ArenaIndex::ROOTS` includes these singleton indices, so they survive every GC
cycle.

## Value Model

The runtime value type is the current `Value` enum in `crates/grift/src/value.rs`:

```rust
enum Value {
    Nil,
    Boolean(bool),
    Number(isize),
    Symbol(ArenaIndex),
    Cons { car: ArenaIndex, cdr: ArenaIndex },
    CharPair { ch: char, cdr: ArenaIndex },
    Operative { params_envparam: ArenaIndex, body_env: ArenaIndex },
    Applicative(ArenaIndex),
    Builtin(BuiltinId),
    Environment { bindings: ArenaIndex, parents: ArenaIndex },
    Inert,
    Ignore,
    Prelude(Prelude),
    Native(NativeFn),
}
```

Important consequences:

- There is no `String` header value anymore. Strings are plain `CharPair` chains.
- There is no dedicated character type. A single-character string is one
  `CharPair { ch, cdr: NIL }`.
- Empty string is represented as `NIL`.
- Prelude functions and registered Rust natives are first-class values and are
  part of the actual runtime model.

### Strings

Strings are singly-linked `CharPair` chains. Semantically, they are list-like
values rather than a distinct header-wrapped type:

```text
"hello"
@100 = CharPair('h', @101)
@101 = CharPair('e', @102)
@102 = CharPair('l', @103)
@103 = CharPair('l', @104)
@104 = CharPair('o', NIL)
```

That design drives a few visible behaviors:

- `""` is exactly `NIL`
- there is no separate character type, so each string element is represented as
  a one-character string node
- `(car "hello")` returns `"h"` as a newly allocated one-character string
- `(cdr "hello")` returns `"ello"` by reusing the tail of the chain
- `(cons (car "h") "ello")` constructs `"hello"`
- `(cdr "x")` returns `()`
- write-mode canonicalizes the shared empty string / empty list value as `()`
- display-mode also renders the shared empty value as `()`
- `cons` does not validate the tail when constructing a `CharPair`, so malformed
  string-like values are possible

### Symbols

`Value::Symbol` points at the head of its name's `CharPair` chain. Symbols are
interned by walking the list stored behind `ArenaIndex::INTERN_LIST`, so equal
names share the same symbol index.

### Operatives, Applicatives, Prelude, Native

Grift has five callable storage forms:

- `Operative`: user-created `vau` closures
- `Applicative`: explicit wrapper that evaluates operands before delegation
- `Builtin`: primitive callable core identified by `BuiltinId`
- `Prelude`: lazily parsed lambda source loaded from `prelude.grift`
- `Native`: user-registered Rust function pointers

Only `Operative` and `Builtin` are primitive combiners. Applicative builtins are
bound as `Applicative(Builtin(_))`. `Prelude` and `Native` are normally reached
through `Applicative` wrappers.

## Environment Model

An environment is:

```rust
Value::Environment {
    bindings: ArenaIndex, // alist of (key . value) pairs, normally symbols
    parents: ArenaIndex,  // list of parent environments
}
```

Key properties:

- `GROUND_ENV` contains builtins only
- `GLOBAL_ENV` is a child of `GROUND_ENV`
- prelude bindings and user globals live in `GLOBAL_ENV`
- child environments do not copy parent bindings; they just reference parents
- closures capture environment object identity rather than a snapshot of
  bindings
- `make-environment` can create multi-parent environments
- `make-empty-environment` creates a parentless environment

Lookup behavior in `env_lookup`:

1. Search the current frame's `bindings` alist.
2. If there is exactly one parent, iterate upward without allocation.
3. If there are multiple parents, fall back to DFS with an arena-allocated
   visited set.
4. Fail with `ArenaError::UnboundVariable` if nothing matches.

Mutation behavior:

- `env_define` prepends a new `(key . value)` binding into the current frame
  and errors with `AlreadyDefined` if that frame already binds the same key
- `env_set` mutates an existing binding in the target frame only and errors with
  `UnboundVariable` if the symbol is only present in a parent

The ground environment is effectively immutable to user code because no builtin
exposes `GROUND_ENV`. The implementation relies on that invariant; the operative
paths use `debug_assert!` rather than a runtime guard.

## Evaluation Model

`eval_step` implements the whole evaluator:

- `Symbol` values perform environment lookup
- `Cons` values are combinations
- everything else is self-evaluating

### Combination Dispatch

For a form `(f arg1 arg2 ...)`:

1. Evaluate `f`.
2. Dispatch on the resulting value:
   - `Builtin`: pass operands through unchanged
   - `Operative`: pass operands through unchanged and invoke with caller env
   - `Applicative`: evaluate operands left-to-right, then call the wrapped value
   - anything else: `NotCallable`

When an applicative unwraps, the inner value can currently be:

- `Operative`
- `Builtin`
- `Applicative`
- `Prelude`
- `Native`

Builtin inner values keep their own calling convention: builtin operatives
receive raw operands, while builtin applicative cores consume the already
evaluated argument list. This is what makes `wrap` and `apply` work uniformly
across builtin and user-defined callables.

`Prelude` is special: the source lambda is parsed and evaluated on demand, then
unwrapped to its underlying operative before invocation.

### Tail Calls

Tail positions do not recurse in Rust. Operatives such as `if`, `begin`, `cond`,
`and`, `or`, `let`, and user-defined combiners update `expr`/`env` and return
control to the trampoline loop.

### Garbage Collection

GC is not threshold-based anymore. The current behavior is:

- evaluation retries an operation after `ArenaError::OutOfMemory`
- before collecting, the evaluator restores the saved GC-root stack head
- collection roots include `ArenaIndex::ROOTS`, the current `expr`, the current
  `env`, and whatever temporary roots are on the GC root stack
- `(gc-collect)` runs collection unconditionally and returns the number of
  reclaimed objects

This matches the comments and logic in `crates/grift/src/eval.rs` and
`crates/grift/src/arena.rs`.

## Current Language Surface

This is the implemented language surface in `define_builtins!`, not the older
superset described by the split docs.

### Operatives

The operative set is:

- `quote`
- `if`
- `define!`
- `fn!`
- `set!`
- `lambda`
- `begin`
- `cond`
- `and`
- `or`
- `let`
- `vau`
- `current-environment`

### Applicatives

The builtin applicative set is:

- `cons`
- `+`, `-`, `*`, `/`
- `=`, `<`, `>`, `<=`, `>=`
- `car`, `cdr`, `list`
- `null?`, `not`, `pair?`, `number?`, `symbol?`, `boolean?`, `inert?`,
  `ignore?`
- `eq?`, `equal?`
- `eval`, `wrap`, `unwrap`
- `operative?`, `applicative?`
- `make-environment`, `make-empty-environment`, `environment?`
- `gc-collect`
- `error`
- `apply`
- `raw-read-string`, `raw-display-to-string`, `raw-write-to-string`

The global environment also receives prelude functions and any Rust natives
registered through `Lisp::register_native`.

## Semantic Notes That Matter

### `define!`

Implemented form:

```lisp
(define! definiend expr)
```

Behavior:

- evaluates `expr` immediately
- validates `definiend` as a Kernel-style parameter tree
- binds into the current frame only
- rejects duplicate bindings in the same frame with `AlreadyDefined`
- supports destructuring because it uses `match_ptree`

Function definition sugar is not embedded in `define!`; it lives in `fn!`.

### `fn!`

Implemented form:

```lisp
(fn! name params body ...)
```

It is sugar for defining a named lambda in the current environment. It supports
multiple body expressions by wrapping them in `begin`.

### `set!`

Implemented form:

```lisp
(set! env-expr symbol expr)
```

This is a major place where the old docs drifted. Current behavior is:

- `env-expr` is evaluated first and must yield an environment
- only a single symbol target is accepted
- the symbol must already exist in that environment's own frame
- parent environments are not searched
- no new binding is created

Scheme-style `(set! x 2)` is not supported.

### `lambda` and `vau`

`lambda` is still implemented as wrapped `vau`, but the actual constructor path
is `Lisp::lambda`, which stores an operative with `env_param = NIL` and then
wraps it as an applicative.

`vau` behavior is:

- `params` must be a valid parameter tree
- `env-param` must be a symbol or `#ignore`
- duplicate parameter names are rejected
- `env-param` may not duplicate a symbol already used in `params`
- multiple body forms are wrapped in `begin`

### `let`

Two forms are implemented:

```lisp
(let ((name expr) ...) body ...)
(let name ((param init) ...) body ...) ; named let
```

Named `let` is implemented directly in `op_let`; it is not just documentation.
The initializer expressions are evaluated in the outer environment, then the
recursive function is invoked with those already-evaluated values.

### `if`, `cond`, `and`, `or`

Boolean contexts are strict: the tested value must be an actual boolean.

Current details:

- `(if test consequent)` returns `()` when the test is false
- `cond` treats a symbol named `else` specially and returns `()` if no clause
  matches
- `and` and `or` currently require at least two operands; fewer operands return
  `InvalidArgument`
- `and` and `or` short-circuit, but because they insist on booleans they behave
  as boolean operators, not general truthy/falsy operators

### `eval` and environments

`eval` supports both:

```lisp
(eval expr)
(eval expr env)
```

The one-argument form evaluates in `GLOBAL_ENV`.

`current-environment` returns the live environment object, not a snapshot.

### Equality

`eq?` is implemented as:

1. true if the two indices are identical
2. otherwise, for values that `Value::is_immutable()` marks immutable, compare
   the stored `Value`

That means `eq?` is not a general structural predicate. In particular, use
`equal?` for lists and strings.

`equal?` is structural for:

- cons cells
- `CharPair` chains

and identity-only for environments.

## Mutation Surface

User-visible mutation is intentionally narrow:

- `define!` mutates an environment by replacing its `bindings` pointer
- `set!` mutates an existing `(symbol . value)` binding pair in place

Other `arena.set()` usage is runtime bookkeeping:

- updating `GC_ROOTS`
- updating `INTERN_LIST`
- reversing freshly built `Cons` or `CharPair` chains in place
- environment initialization during builtin/prelude setup

There is still no user-visible pair mutation:

- no `set-car!`
- no `set-cdr!`
- strings are immutable after construction

## Reader and Formatting Notes

The parser currently supports:

- proper and dotted lists
- quote shorthand `'x`
- string escapes: `\n`, `\t`, `\r`, `\\`, `\"`
- line comments starting with `;`
- booleans `#t`, `#f`, plus reader aliases `#true`, `#false`
- `#inert` and `#ignore`
- signed base-10 integers

The raw string builtins are accurate documentation targets because they are used
in tests:

- `(raw-read-string str)` parses one expression from a `CharPair` chain
- `(raw-display-to-string obj)` formats with display semantics
- `(raw-write-to-string obj)` formats with write semantics

## Practical Gotchas

These are the current sharp edges worth remembering:

- `define!` does not redefine; same-frame rebinding is an error
- `fn!` is separate from `define!`
- `set!` requires an explicit environment argument
- `set!` only updates the target frame, never a parent frame
- empty string is literally `NIL`
- `pair?`, `car`, and `cdr` treat non-empty strings as `CharPair` chains
- `and`/`or` are not variadic identity forms right now; they error on fewer
  than two operands
- `cond` with no matching clause returns `()`, not `#inert`
- `make-empty-environment` really is empty, which makes it useful for sandboxed
  `eval`
- `current-environment` returns a live mutable environment reference
- `error` is currently a stub that always signals `InvalidArgument`

## What Changed Relative To The Old Split Docs

The deleted files were inaccurate in several concrete ways. The merged document
corrects them:

- strings are `CharPair` chains, not `String { data }` headers
- `Value` also includes `Prelude` and `Native`
- GC is OOM-triggered, not threshold-triggered
- `set!` takes an explicit environment and does not search parents
- `fn!` and named `let` both exist
- `and`/`or` currently reject arities below two
- `cond` with no match returns `()`
- top-level user code runs in `GLOBAL_ENV`; `GROUND_ENV` is for builtin setup

If this file and the code ever disagree, the code in `crates/grift/src` is the
source of truth.
