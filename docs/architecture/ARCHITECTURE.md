# Grift Architecture

This document describes the current Grift implementation as shipped in
`crates/grift/src`. It is intentionally implementation-facing: it explains how
the interpreter is put together, which invariants the runtime relies on, and
which details matter for a compatible reimplementation.

The companion language-level behavior specification is
`docs/architecture/LANGUAGE_SPECIFICATION.md`.

The primary conformance sources for this document are:

- `crates/grift/src/arena.rs`
- `crates/grift/src/value.rs`
- `crates/grift/src/lisp.rs`
- `crates/grift/src/parse.rs`
- `crates/grift/src/eval.rs`
- `crates/grift/src/native.rs`
- `crates/grift/src/prelude.rs`
- `crates/grift/prelude.grift`
- `crates/grift/tests/lisp_tests.rs`

## 1. Top-Level Shape

Grift is a `#![no_std]`, `#![forbid(unsafe_code)]`, arena-allocated Lisp
interpreter with Kernel-style operative/applicative semantics.

The important implementation choices are:

- all runtime objects live in a fixed-capacity `Arena<Value, N>`
- there is no heap allocation and no `alloc` dependency in the interpreter
- strings are linked `CharPair` chains, not heap-backed buffers
- symbols are interned and compare by arena identity
- environments are first-class linked objects
- `vau` creates primitive user-defined combiners
- applicatives are explicit wrappers around other callable values
- evaluation uses a trampoline instead of recursive host-language calls in tail
  position
- garbage collection is mark-and-sweep and is normally triggered only after
  `OutOfMemory`

The library entry point is [`Lisp`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/lisp.rs).
The optional binary in [`main.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/main.rs)
constructs one `Lisp` instance and reuses it across a file run or REPL session,
so top-level state persists across inputs.

## 2. Module Responsibilities

The runtime is split into a small set of modules with fairly sharp boundaries:

- [`arena.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/arena.rs):
  fixed-size free-list allocator, `ArenaIndex`, tracing, and mark/sweep GC
- [`value.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/value.rs):
  `Value` enum and low-level runtime categories
- [`lisp.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/lisp.rs):
  interpreter object, symbol interning, environment operations, formatting,
  and public API
- [`parse.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/parse.rs):
  recursive-descent reader shared by byte-oriented source text and runtime
  string chains
- [`eval.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/eval.rs):
  evaluator, builtin registration, operative/applicative dispatch, TCO, and GC
  retry logic
- [`native.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/native.rs):
  host-function registration and `LispOps` abstraction
- [`prelude.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/prelude.rs):
  bundled prelude source and lazy prelude binding helpers

## 3. Startup and Reserved Slots

`Lisp::new()` constructs a fresh arena, allocates fixed singleton values, then
installs builtin bindings and prelude bindings.

The first ten slots are reserved and their indices are treated as ABI-like
constants throughout the runtime:

| Slot | Constant | Meaning |
| --- | --- | --- |
| 0 | `ArenaIndex::NIL` | nil, empty list, and empty string |
| 1 | `ArenaIndex::TRUE` | `#t` |
| 2 | `ArenaIndex::FALSE` | `#f` |
| 3 | `ArenaIndex::INERT` | `#inert` |
| 4 | `ArenaIndex::IGNORE` | `#ignore` |
| 5 | `ArenaIndex::GROUND_ENV` | builtin-only environment |
| 6 | implicit | cons cell containing the one-element parent list for `GLOBAL_ENV` |
| 7 | `ArenaIndex::GLOBAL_ENV` | user-visible top-level environment |
| 8 | `ArenaIndex::GC_ROOTS` | cons cell whose `car` stores the temporary GC root stack head |
| 9 | `ArenaIndex::INTERN_LIST` | cons cell whose `car` stores the symbol intern list head |

`ArenaIndex::ROOTS` includes all singleton slots above, so they always survive
collection.

Initialization order in `Lisp::new()` is:

1. allocate singleton values and fixed environments
2. allocate the global-parent cons cell at slot 6
3. allocate `GC_ROOTS` and `INTERN_LIST`
4. bind builtins into `GROUND_ENV`
5. bind prelude functions and constants into `GLOBAL_ENV`

## 4. Runtime Value Model

The runtime data model is exactly the `Value` enum in
[`value.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/value.rs):

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

- every runtime object fits in one arena slot
- larger structures are assembled out of linked arena nodes
- there is no dedicated string header object
- there is no dedicated character type
- prelude entries and host-native functions are first-class runtime values

### 4.1 Strings

Strings are singly linked `CharPair` chains terminated by `NIL`.

```text
"cat"
@a = CharPair('c', @b)
@b = CharPair('a', @c)
@c = CharPair('t', NIL)
```

This representation is observable:

- empty string is exactly `NIL`
- a one-character string is one `CharPair` whose `cdr` is `NIL`
- `(car string)` allocates a fresh one-character string
- `(cdr string)` reuses the tail of the existing chain
- `pair?` treats non-empty strings as pair-like
- formal parameter tree matching does not treat strings as cons cells

### 4.2 Symbols

`Value::Symbol` stores the head of the symbol name's `CharPair` chain.
`Lisp::symbol` interns names by walking the list stored behind
`ArenaIndex::INTERN_LIST`.

Symbol equality in environments is therefore pointer equality on the symbol
object, not repeated string comparison during lookup.

### 4.3 Callables

Grift has five callable storage forms:

- `Builtin`: builtin combiner core identified by `BuiltinId`
- `Operative`: user-created `vau` closure
- `Applicative`: wrapper around another runtime value
- `Prelude`: static prelude entry containing a function name and lambda source
- `Native`: host function pointer

Calling convention depends on the outer runtime form:

- `Builtin` used directly: raw operands plus caller environment if the builtin
  is operative; evaluated arguments otherwise
- `Operative`: raw operand object plus caller environment
- `Applicative`: evaluate operands left-to-right, then invoke the wrapped value
- `Prelude`: normally stored behind an `Applicative`; each call reparses and
  reevaluates its lambda source, then unwraps to the inner operative
- `Native`: normally stored behind an `Applicative`; receives already evaluated
  argument list

## 5. Environment Representation

An environment is:

```rust
Value::Environment {
    bindings: ArenaIndex,
    parents: ArenaIndex,
}
```

`bindings` is an association list of binding pairs. Each binding pair is a
`Cons { car: key, cdr: value }`, and the binding list itself is a cons list of
those pairs.

`parents` is a proper list of parent environments.

Observable properties:

- `GROUND_ENV` contains builtin bindings only
- `GLOBAL_ENV` is a child of `GROUND_ENV`
- prelude bindings and user top-level definitions live in `GLOBAL_ENV`
- child environments never copy parent bindings
- closures capture environment object identity, not a snapshot of values

### 5.1 Lookup

`env_lookup` has two paths:

- zero or one parent: iterate upward without allocating
- multiple parents: do left-to-right depth-first search using an arena-allocated
  visited set

Multi-parent lookup is cycle-tolerant because already visited environments are
skipped.

### 5.2 Mutation

Only two user-visible operations mutate environments:

- `define!`: prepend a new binding in the current frame
- `set!`: mutate an existing binding pair in the explicitly supplied frame

`env_define` rejects same-frame duplicates with `AlreadyDefined`.
`env_set` never searches parents and rejects missing local bindings with
`UnboundVariable`.

## 6. Reader Architecture

The parser in
[`parse.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/parse.rs)
is a single recursive-descent implementation over a small `CharSource` trait.

There are two concrete sources:

- `SliceSource<'a>`: reads external source text one byte at a time
- `ChainSource<'a, N>`: reads characters from an existing `CharPair` chain

This distinction matters:

- ordinary `eval` is byte-oriented and does not decode UTF-8 into Unicode
  scalar values before tokenization
- `raw-read-string` is character-oriented because it walks runtime `char`
  values
- parse errors from `SliceSource` report real 1-based `(line, col)`
- parse errors from `ChainSource` report `(0, 0)`

The parser directly recognizes:

- proper lists
- dotted pairs
- quote shorthand
- strings with `\n`, `\t`, `\r`, `\\`, `\"`
- booleans
- `#inert`
- `#ignore`
- base-10 signed integers that fit `isize`
- symbols

Unknown string escapes raise `InvalidArgument`, not `ParseError`.

## 7. Evaluation Architecture

The evaluator in
[`eval.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/eval.rs)
uses a trampoline centered on `eval_expr` and `eval_step`.

At a high level:

1. parse one or more expressions
2. evaluate them sequentially in `GLOBAL_ENV`
3. return the last result, or `#inert` for empty input

`eval_step` dispatches by runtime type:

- `Symbol`: environment lookup
- `Cons`: combination evaluation
- everything else: self-evaluating

### 7.1 Combination Dispatch

For a combination `(f arg1 arg2 ...)`:

1. evaluate `f`
2. dispatch on the result

Dispatch rules:

- `Builtin`: call builtin operative path on raw operands
- `Operative`: invoke user operative with raw operand object
- `Applicative`: evaluate operands once, left-to-right, then invoke wrapped
  value
- anything else: `NotCallable`

When an applicative unwraps, the inner value may itself be:

- `Operative`
- `Builtin`
- `Applicative`
- `Prelude`
- `Native`

That is what makes `wrap`, `unwrap`, and `apply` uniform across builtin and
user-defined combiners.

### 7.2 Tail Calls

Tail positions are implemented by mutating `expr` and `env` and returning to
the trampoline loop instead of recursing in Rust.

The builtin operative paths that intentionally preserve tail position are:

- `if`
- `begin`
- `cond`
- `and`
- `or`
- `let`
- user-defined operatives after argument matching

### 7.3 Formal Parameter Tree Matching

Formal parameter trees are validated eagerly by `validate_ptree` and matched by
`match_ptree`.

Validation checks:

- only symbols, `#ignore`, `NIL`, and cons cells are allowed
- duplicate symbols are rejected
- cyclic trees are rejected

`vau` adds one more check: the environment parameter must be a symbol or
`#ignore`, and it may not duplicate a symbol from the parameter tree.

### 7.4 GC Integration

The evaluator uses a temporary root stack stored behind `ArenaIndex::GC_ROOTS`.
During evaluation, important intermediate values are pushed onto that list.

On `OutOfMemory`:

1. restore the saved GC root stack head for the current evaluation step
2. collect garbage using:
   - `ArenaIndex::ROOTS`
   - the current `expr`
   - the current `env`
   - anything on the temporary GC root stack
3. retry the evaluation step

If collection frees nothing, `OutOfMemory` propagates.

`(gc-collect)` bypasses the OOM trigger and forces an unconditional collection.

## 8. Builtin and Prelude Installation

Builtin registration is generated by the `define_builtins!` macro in
[`eval.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/eval.rs).

The macro defines:

- the `BuiltinId` constants
- `init_builtins`
- builtin dispatch tables for operative and applicative call conventions

Registration policy:

- builtin operatives are stored as raw `Builtin` values in `GROUND_ENV`
- builtin applicatives are stored as `Applicative(Builtin(_))`

Prelude installation happens in `Lisp::init_prelude()`:

- operative prelude entries are stored as raw `Prelude(form)` values in `GLOBAL_ENV`
- applicative prelude entries are stored as `Applicative(Prelude(form))`
- non-lazy top-level prelude forms are evaluated eagerly during startup
- prelude entries are not compiled once and cached; they are reparsed and
  reevaluated on each invocation

At startup, the runtime walks `prelude.grift` form-by-form using the normal
reader:

- top-level `fn!` forms are installed as lazy applicative prelude bindings
- top-level `(define! name (vau ...))` forms are installed as lazy operative
  prelude bindings
- top-level `(define! name (lambda ...))` forms are installed as lazy
  applicative prelude bindings
- all other top-level forms are evaluated eagerly in `GLOBAL_ENV`

## 9. Formatting and Raw String Helpers

Formatting lives in `Lisp::fmt_value` and supports two modes:

- write mode via `write_value`
- display mode via `display_value`

Formatting is preceded by structure validation:

- symbols must point to well-formed `CharPair` chains
- strings must be well-formed `CharPair` chains
- lists are recursively validated

The same validation is reused by:

- `write_value`
- `display_value`
- `LispOps::write_value`
- `LispOps::display_value`
- `raw-write-to-string`
- `raw-display-to-string`

Because empty string and `NIL` are the same object, both write mode and display
mode render the empty string as `()`.

`raw-read-string` reuses the parser on a `ChainSource` and requires the entire
runtime string to contain at most one expression plus trailing whitespace or
comments.

## 10. Host Integration

Host functions use the interface in
[`native.rs`](/Users/kisaczka/Desktop/code/pwn_arena/crates/grift/src/native.rs).

Important pieces:

- `NativeFn`: erased function pointer type
- `LispOps`: trait object exposing the public interpreter surface without the
  arena-size const generic
- `FromLisp` / `ToLisp`: host conversion traits
- `register_native!`: macro for ergonomic native bindings

Registered natives are stored as `Native` values wrapped in an
`Applicative`, then defined in `GLOBAL_ENV`.

## 11. Reimplementation Notes

A compatible reimplementation in C or another host language should preserve at
least these architectural facts:

- fixed singleton identities for nil/booleans/inert/ignore
- linked-node strings and interned symbols
- first-class environments with explicit parent lists
- separate operative and applicative calling conventions
- byte-oriented external parsing and char-oriented `raw-read-string`
- eager formal-parameter validation
- depth-first multi-parent lookup
- trampoline-style tail execution
- OOM-driven GC retry behavior
- reparsed prelude function calls

## 12. Confirmed Oddities

These are implementation oddities worth keeping in mind because they are easy
to miss when reading only the public API:

- empty string is identical to `NIL`
- `pair?`, `car`, and `cdr` treat non-empty strings as pair-like, but formal
  parameter matching does not
- `apply` never evaluates the supplied argument list
- `wrap` does not check whether the wrapped value is callable
- prelude functions are resolved from source text on every call
- the external reader and `raw-read-string` are observably different for
  non-ASCII input

If this document and the implementation disagree, the code in
`crates/grift/src` and the behavior covered by `crates/grift/tests/lisp_tests.rs`
are authoritative.
