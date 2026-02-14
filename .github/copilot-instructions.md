# Copilot Instructions

## Project Overview

This is a Rust workspace containing a minimal Scheme implementation (named "Grift") built on a custom arena allocator. Grift targets R7RS conformance and is designed to run on embedded and resource-constrained systems.

## Critical Requirements

**This is a `no_std`, `no_alloc`, `no unsafe` Scheme implementation.** Only the REPL crate (`grift_repl`) may use `std`.

### Crate-Specific Rules

| Crate | `#![no_std]` | Can use `std` | Can use `alloc` | `#![forbid(unsafe_code)]` |
|-------|--------------|---------------|-----------------|---------------------------|
| `grift_arena` | ✅ Required | ❌ No | ❌ No | ✅ Yes |
| `grift_parser` | ✅ Required | ❌ No | ❌ No | ✅ Yes |
| `grift_eval` | ✅ Required | ❌ No | ❌ No | ✅ Yes |
| `grift_repl` | ❌ Not required | ✅ Yes | ✅ Yes | ✅ Yes |

### Code Guidelines

1. **No heap allocation in core crates** - The `grift_arena`, `grift_parser`, and `grift_eval` crates must not use `Vec`, `String`, `Box`, or any other heap-allocated types.

2. **No unsafe code** - All crates use `#![forbid(unsafe_code)]`. Do not introduce `unsafe` blocks or functions.

3. **Use the arena allocator** - All dynamic data structures must use `grift_arena` for allocation.

4. **Core library only** - Use `core::` instead of `std::` in no_std crates (e.g., `core::cell::Cell`, `core::option::Option`).

5. **Copy types** - Types stored in the arena must implement the `Copy` trait.

6. **REPL is the exception** - The `grift_repl` crate handles I/O and user interaction, so it may use the standard library.

## R7RS Conformance

Grift aims for full R7RS (Revised⁷ Report on the Algorithmic Language Scheme) conformance:

- **Proper tail calls** - Tail-call optimization via trampolined evaluation ensures constant stack space for tail-recursive programs.
- **First-class continuations** - `call/cc` is supported via arena-based continuation frames.
- **Hygienic macros** - Full `syntax-rules` and `syntax-case` with mark-based hygiene.
- **Library system** - R7RS `define-library`, `import`, and `export` with import modifiers (`only`, `except`, `prefix`, `rename`).
- **Numeric tower** - Integers (`isize`), floating point, rationals, and complex numbers.
- **Standard libraries** - Feature-gated `(scheme base)`, `(scheme write)`, `(scheme char)`, etc.
- **SRFI support** - Feature-gated SRFI libraries loadable via `(import (srfi N))` (e.g., `(import (srfi 64))`).

## Runtime Macro System

Grift implements a **runtime macro system** rather than a compile-time one:

- **Macros are stored as source strings** at compile time via `grift_macros::include_stdlib!()` and parsed/evaluated at runtime.
- **`syntax-rules`** provides pattern-based hygienic macros per R7RS §4.3.2.
- **`syntax-case`** provides procedural hygienic macros with `syntax`, `quasisyntax`, `unsyntax`, and `unsyntax-splicing`.
- **Macro expansion** happens during evaluation, not as a separate phase. The evaluator's `expand.rs` handles all expansion.
- **Gensym-based hygiene** prevents accidental variable capture using generated symbols.

## Library & SRFI Feature Gating

Libraries are compiled into the binary conditionally via Cargo feature flags in `grift_parser`:

```toml
# Include only specific libraries for embedded targets:
[dependencies.grift_parser]
default-features = false
features = ["scheme-base", "scheme-write"]

# Include everything (default):
[dependencies.grift_parser]
features = ["all-libraries"]  # includes all scheme-* and srfi-* features
```

To add a new SRFI library:
1. Create a `.scm` file in `crates/grift_parser/src/lib/srfi/` with a `define-library` form
2. Add a feature flag `srfi-N` in `crates/grift_parser/Cargo.toml`
3. Add the feature to the `all-libraries` list
4. Add a `LibrarySource` entry in `crates/grift_parser/src/libraries.rs`
5. Forward the feature in `crates/grift/Cargo.toml`

## Build Commands

```bash
# Build all crates
cargo build --workspace

# Run the REPL
cargo run -p grift_repl --bin grift

# Run tests
cargo test --workspace
```
