# Copilot Instructions

## Project Overview

This is a Rust workspace containing a minimal Lisp implementation built on a custom arena allocator.

## Critical Requirements

**This is a `no_std`, `no_alloc` Lisp implementation.** Only the REPL crate (`lisp_repl`) may use `std`.

### Crate-Specific Rules

| Crate | `#![no_std]` | Can use `std` | Can use `alloc` |
|-------|--------------|---------------|-----------------|
| `pwn_arena` | ✅ Required | ❌ No | ❌ No |
| `lisp_parser` | ✅ Required | ❌ No | ❌ No |
| `lisp_eval` | ✅ Required | ❌ No | ❌ No |
| `lisp_repl` | ❌ Not required | ✅ Yes | ✅ Yes |

### Code Guidelines

1. **No heap allocation in core crates** - The `pwn_arena`, `lisp_parser`, and `lisp_eval` crates must not use `Vec`, `String`, `Box`, or any other heap-allocated types.

2. **Use the arena allocator** - All dynamic data structures must use `pwn_arena` for allocation.

3. **Core library only** - Use `core::` instead of `std::` in no_std crates (e.g., `core::cell::RefCell`, `core::option::Option`).

4. **Copy types** - Types stored in the arena must implement the `Copy` trait.

5. **REPL is the exception** - The `lisp_repl` crate handles I/O and user interaction, so it may use the standard library.

## Build Commands

```bash
# Build all crates
cargo build --workspace

# Run the REPL
cargo run -p lisp_repl

# Run tests
cargo test --workspace
```
