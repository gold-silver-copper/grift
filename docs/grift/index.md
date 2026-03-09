---
title: "Overview"
order: 1
---

# Grift Documentation

# Grift – A Minimalistic Lisp

A `no_std`, `no_alloc` Lisp interpreter built on top of [`arena`],
implementing Kernel-style vau calculus (fexprs).

## Features

- **No-std, no-alloc**: Works in embedded environments with no heap.
  Only `core::` types are used; the crate compiles for bare-metal targets.
- **Arena-allocated**: All values live in a fixed-size [`Arena`](arena::Arena)
  with const-generic capacity. No `Vec`, `String`, or `Box`.
- **Simple API**: Parse and evaluate Lisp expressions in one call via [`Lisp::eval`].
- **Tail-call optimization**: Unbounded recursion in tail position without
  growing the Rust call stack, implemented via a trampoline loop.
- **Mark-and-sweep GC**: Automatic garbage collection triggered on OOM,
  with explicit collection available via `(gc-collect)`.
- **No unsafe code**: `#![forbid(unsafe_code)]` is enforced crate-wide.

## Architecture

The interpreter is split into four internal modules:

- [`value`] — The [`Value`] enum (12 variants) representing all Lisp types.
- `lisp` — The [`Lisp`] struct: arena wrapper, symbol interning, environments.
- `parse` — Recursive-descent S-expression parser.
- `eval` — Evaluator with TCO trampoline, builtin dispatch, and GC integration.

## Example

```rust
use grift::{Lisp, Value};

let lisp: Lisp<20000> = Lisp::new();
let three = lisp.eval("(+ 1 2)");
assert_eq!(three, Ok(Value::Number(3)));
```

## Overview

This build discovered **14 value types**, **13 special forms**, **37 built-in functions**, **14 error variants**, and **4 prelude entries**.

## Pages

- [Types](types.md)
- [Special Forms](special-forms.md)
- [Built-in Functions](builtins.md)
- [Environments](environments.md)
- [Strings](strings.md)
- [Garbage Collection](gc.md)
- [Error Types](errors.md)
- [Examples](examples.md)

