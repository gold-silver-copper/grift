# Copilot Instructions

## Project Overview

This is a Rust workspace containing a LISP implementation (named "Grift") built on a custom arena allocator and inspired by vau calculus. Grift is designed to run on embedded and resource-constrained systems.

## Critical Requirements

**This is a `no_std`, `no_alloc`, `no unsafe` LISP implementation.** Only the tests and REPL may use `std`.


### Code Guidelines

1. **No heap allocation in core crates** - The crates must not use `Vec`, `String`, `Box`, or any other heap-allocated types.

2. **No unsafe code** - All crates use `#![forbid(unsafe_code)]`. Do not introduce `unsafe` blocks or functions.

3. **Use the arena allocator** - All dynamic data structures must use `grift_arena` for allocation.

4. **Core library only** - Use `core::` instead of `std::` in no_std crates (e.g., `core::cell::Cell`, `core::option::Option`).

5. **Copy types** - Types stored in the arena must implement the `Copy` trait.

6. **REPL is the exception** - The repl handles I/O and user interaction, so it may use the standard library.




## Build Commands

```bash
# Build all crates
cargo build --workspace

# Run the bench
cargo run -p grift --features std --example fib_bench --release

# Run tests
cargo test --workspace
```
