# Copilot Instructions

## Project Overview

grift is a minimal, pure, type-strict Lisp built on John Shutt's vau
calculus (Kernel). The Rust crate `crates/grift` is the core: values, a
Vec-backed heap with mark-and-sweep GC, the reader and printer, the
evaluator and 44 primitives. Everything else is written in grift itself in
`crates/grift/lib/*.grift`. See `docs/architecture/` for the language
specification and the architecture.

## Critical Requirements

1. **no_std with alloc.** The core crate uses `core::` and `alloc::` only
   (`Vec`, `Box`, `String`, `BTreeMap` are fine). `std` is allowed only in
   tests, examples and the REPL binary. It must build for
   `thumbv6m-none-eabi`.
2. **No unsafe code.** `#![forbid(unsafe_code)]`.
3. **No dependencies in the core crate** (`rustyline` is optional, for the
   REPL only).
4. **Minimal core.** Do not add a Rust primitive for anything that can be
   written in grift. `$vau` is the only primitive operative. A new
   primitive needs a written justification.
5. **Purity.** No value may change once a program can observe it: no
   mutable pairs, no mutable environments, no cells. Definitions are
   declarations that produce new environments.
6. **Type discipline.**
   - No truthiness and no punning; every primitive checks arity and types.
   - Operatives must be bound to `$`-prefixed names, everything else to
     names without `$`. Library code must obey this.
   - `eq?` refuses to compare two structured values of the same kind.
7. **Library order matters.** Environments are immutable, so a library
   definition can only refer to bindings defined before it. Mutually
   recursive helpers must be defined together, with one `$define!` over a
   definiend tree.
8. **MSRV 1.85.** Avoid newer syntax such as let-chains.

## Build Commands

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo build --target thumbv6m-none-eabi -p grift
cargo run -p grift --features repl
cargo run -p grift --example fib_bench --release
```
