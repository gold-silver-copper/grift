# grift architecture

This document describes the implementation in `crates/grift`. The
language is specified in [LANGUAGE_SPECIFICATION.md](LANGUAGE_SPECIFICATION.md).

## Overview

```
src/value.rs    Value: 16-byte Copy enum of immediates and heap handles
src/symbol.rs   symbol interning (never collected)
src/heap.rs     the heap: Vec of object slots, free list, mark-and-sweep
src/ctx.rs      Ctx = heap + symbols + lookup cache; value helpers
src/env.rs      environments, lookup, parameter-tree matching
src/read.rs     the reader
src/print.rs    the printer (write and display)
src/machine.rs  the evaluator: an explicit loop over heap frames
src/prims.rs    the 44 primitives
src/lib.rs      Grift: boot, top-level fold, natives, pinning
src/main.rs     REPL and file runner (feature `repl`)
lib/*.grift     the library, loaded in order at boot
```

The crate is `#![no_std]` with `alloc`, `#![forbid(unsafe_code)]`, has no
dependencies (`rustyline` is optional, for the REPL binary only) and builds
for `thumbv6m-none-eabi`. MSRV is 1.85.

## Values

`Value` is a `Copy` enum of 16 bytes:

- **Immediates:** `Nil`, `Bool`, `Inert`, `Ignore`, `Int(i64)`,
  `Char(char)`, `Sym(SymId)`, and `Prim(PrimId)` (the operative core of a
  primitive).
- **Heap values:** `Obj(ObjId)`, a `u32` index into the heap. The object
  in that slot determines the value's type.

Integers and symbols never allocate.

## Heap

`Heap` is a `Vec<Object>` with a free list threaded through `Free` slots,
plus a parallel mark vector. The `Object` variants are:

- **Data:** `Pair`, `Str(Box<str>)`, `Encap(tag, value)`.
- **Environments:** `Log`, `Env` (a view of a log), `SmallEnv`.
- **Combiners:** `Operative`, `Applicative`, `PrimData` (a primitive
  closing over data, e.g. an encapsulation type's constructor), `Native`.
- **Control:** `Declaration`, `Prompt`, `SubCont`, `Frame`.

Objects are 64 bytes.

Objects are immutable once a program can see them. The heap performs
three internal writes, none observable:

- the initialization of recursive bindings;
- appending to the tip of a log (no existing view can see the new
  entries);
- truncating log entries that no live view can see.

### Environments

Environments must be immutable, yet long chains of top-level definitions
must stay cheap to search. Two representations meet both needs.

**Logs and views.** A `Log` is an append-only vector of `(symbol, value)`
entries with a parent environment. An `Env` is a *view* `(log, len)`: it
sees the first `len` entries of the log, then its parent.

- **Extending at the tip:** if a view ends at the end of its log,
  extending it appends to the log and returns a longer view; every
  existing view is unaffected.
- **Extending anywhere else:** a new log is started.

So a sequence of top-level definitions is one log, not a chain of frames.
Logs with 16 or more entries get an index (latest position per symbol,
plus a "previous entry for the same symbol" link per entry). Finding the
latest binding visible in a view is then O(1) plus a short walk past
later redefinitions.

**Small environments.** An operative call binds few symbols. A `SmallEnv`
holds up to three bindings inline, plus the environment parameter as a
bare object id, so a call allocates one object and a lookup in the call
frame touches one slot. Larger frames use a fresh log.

**Lookup cache.** Below the first frame, lookups are memoized per
`(environment, symbol)` in a 4096-entry direct-mapped cache. That frame is
usually the call's own bindings, so the cached key is the closure's static
environment, which is the same object for every call of that closure.
Environments never change, so a resolved binding stays valid; the cache is
cleared at every collection because object ids are reused.
Uninitialized bindings are never cached.

**Recursive bindings.** `environment-bind-recursive` appends entries with
no value (`None`), evaluates the initializer under an `InitRec` frame, and
fills them in. If a reinstated sub-continuation reaches that frame again,
the entries are already filled, and the result is `reinitialized-binding`.

## Garbage collection

Mark-and-sweep, with an explicit mark stack: there is no recursion, so
long lists are safe.

**When.** Collection runs only at the top of the machine loop: when
allocations since the last collection exceed the threshold, or when the
host forces one. At that point every live value is reachable from:

- the control register (expression, environment, value or operands);
- the current continuation (the top frame);
- values pinned by the host and the host's environments;
- the primitive environment and `error-prompt`.

Primitives and natives run between safe points, so they never observe a
collection and need no rooting.

**Logs.** A log is traced only up to the longest view of it that is still
live. Entries past that point can never be seen again and are truncated.
This reclaims bindings appended by views that have since become garbage.
Entry vectors of collected logs are pooled for reuse.

**Threshold.** The next threshold is the largest of 64Ki allocations,
twice the live count, and the number of free slots. A sweep visits the
whole heap, so this keeps sweep cost per allocation bounded even after a
deep recursion has grown the heap.

## The machine

`Machine::run` is a loop over a control register:

```rust
enum Ctl {
    Eval(Value, ObjId),          // evaluate an expression in an environment
    Ret(Value),                  // return a value to the current frame
    Apply(Value, Value, ObjId),  // apply a combiner to an operand tree
    HostErr(Value),              // stop with an error payload for the host
}
```

**Frames.** The continuation is a chain of immutable `Frame` objects
linked by parent pointers:

- `Combine`: a combination's head (itself a combination) is being
  evaluated;
- `Args`: applicative operands are being evaluated left to right;
- `InitRec`: a recursive initializer is being evaluated;
- `Prompt`: a delimiter;
- `HostError` and `CheckEnv`: used by the host error prompt and by
  `apply-declaration`.

No Rust recursion grows with program depth, so deep recursion in grift
uses heap frames and cannot overflow the Rust stack.

**Tail calls.** Applying a combiner replaces the control register without
pushing a frame. An operative's body, and an `eval` called in tail
position, therefore run in constant continuation space.

**Applicatives.** Operands that are not combinations (symbols and
constants) are evaluated inline without a frame. If every operand is
simple, the arguments go to the combiner straight from a scratch buffer.
Arguments of a compound operative are matched against its parameter tree
directly from that buffer.

**The sigil rule.** When a combination's head is a symbol, the machine
checks it in one branch: each symbol records whether its name starts with
`$`.

### Delimited control

- **`push-prompt`** pushes a `Prompt` frame and calls the thunk.
- **`with-sub-cont`** walks the frame chain to the innermost prompt with
  the given tag. It records the segment from the top frame down to that
  prompt as a `SubCont { top, bottom }` object (O(1), since frames are
  immutable and shared), sets the continuation to the prompt's parent,
  and calls `f`.
- **`push-sub-cont`** copies the segment's frames onto the current
  continuation, which makes sub-continuations safe to reinstate any number
  of times, then calls the thunk.

### Errors

A failing primitive produces a `Fault` (kind, message, irritants). The
machine finds the innermost `error-prompt` frame, captures the
sub-continuation up to it, and returns `(k kind message irritants)` to the
prompt's parent: exactly what `with-sub-cont` would do.

Every top-level run is wrapped in a *host* error prompt:

- a fault reaching it becomes `Err(Error)` for the host;
- a library `raise-error` reaching it runs its `f` under a `HostError`
  frame, whose returned payload is decoded into the same `Error`.

Natives return `Result<Value, Error>`; an `Err` becomes a fault and is
delivered the same way.

## The host interface

- **Boot.** `Grift::new` builds the primitive environment, evaluates the
  library files in order into the ground environment (each top-level form
  that yields a declaration has it applied), and opens a standard
  environment for user code.
- **`eval`.** Reads all forms and pins them. Each form runs under a fresh
  host error prompt; declarations are applied and the resulting
  environment replaces the top-level environment.
- **Cloning.** `Clone` copies the whole heap, which is far cheaper than
  booting: about 2.7k live objects after boot.

## Performance

Everything above `$vau` is library code, so a `$if` is an operative call,
and `+` is a library applicative folding the binary primitive. The `stats`
feature counts machine steps (`Grift::steps`), which is the most reliable
measure on a loaded machine. Some costs:

| Operation | Steps |
|---|---|
| `(+ 1 2)` | 49 |
| `($if #t 1 2)` | 14 |
| `($cond (#f 1) (#t 2))` | 93 |
| `(car (list 1))` | 9 |

Machine throughput is roughly 20–40 million steps per second.

`cargo run -p grift --example fib_bench --release` runs the 34 programs of
the previous arena-based implementation's benchmark and compares them with
its averages. On the development machine, which was under other load, the
total was 11.0 s against 4.2 s, about 2.6 times slower overall:

- **Slower:** programs dominated by `$if`, `$cond` and arithmetic, which
  were Rust special forms before (fib-naive(30) 5.9 s vs 2.5 s;
  ackermann(3,7) 3.5 s vs 0.87 s).
- **Faster:** list-heavy programs (list-length, foldl, append, reverse:
  2–3 times faster), thanks to unboxed integers and the cheaper heap.

The test suite runs millions of steps, so the workspace builds `grift`
with `opt-level = 2` in the dev profile. This is for speed only; the
evaluator never needs a larger stack.

## Testing

- `tests/grift/*.grift`: the language-level suite, run by
  `tests/grift_suite.rs`. Each file is evaluated in a clone of a booted
  interpreter after `_harness.grift`, which defines `assert-equal`,
  `assert-true`, `assert-false` and `$assert-error` in grift. It covers
  every public library combiner, every type-discipline rule,
  1,000,000-iteration tail-call loops through library forms, delimited
  control, errors, effects, contracts, records, strings, rationals and GC.
- `tests/api.rs`: the embedding API, natives, pinning, the top-level fold,
  cloning, bounded heap growth, reader/printer, and decoding of host errors.
