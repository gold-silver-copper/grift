# Copilot Instructions

## Project Overview

This is a Rust workspace containing a minimal Scheme implementation (named "Grift") built on a custom arena allocator.

## Critical Requirements

**This is a `no_std`, `no_alloc` Scheme implementation.** Only the REPL crate (`grift_repl`) may use `std`.

### Crate-Specific Rules

| Crate | `#![no_std]` | Can use `std` | Can use `alloc` |
|-------|--------------|---------------|-----------------|
| `grift_arena` | ✅ Required | ❌ No | ❌ No |
| `grift_parser` | ✅ Required | ❌ No | ❌ No |
| `grift_eval` | ✅ Required | ❌ No | ❌ No |
| `grift_repl` | ❌ Not required | ✅ Yes | ✅ Yes |

### Code Guidelines

1. **No heap allocation in core crates** - The `grift_arena`, `grift_parser`, and `grift_eval` crates must not use `Vec`, `String`, `Box`, or any other heap-allocated types.

2. **Use the arena allocator** - All dynamic data structures must use `grift_arena` for allocation.

3. **Core library only** - Use `core::` instead of `std::` in no_std crates (e.g., `core::cell::Cell`, `core::option::Option`).

4. **Copy types** - Types stored in the arena must implement the `Copy` trait.

5. **REPL is the exception** - The `grift_repl` crate handles I/O and user interaction, so it may use the standard library.

## Build Commands

```bash
# Build all crates
cargo build --workspace

# Run the REPL
cargo run -p grift_repl --bin grift

# Run tests
cargo test --workspace
```
