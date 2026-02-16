# Grift

[![Crates.io](https://img.shields.io/crates/v/grift.svg)](https://crates.io/crates/grift)
[![Documentation](https://docs.rs/grift/badge.svg)](https://docs.rs/grift)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

A minimal `no_std`, `no_alloc` forbid unsafe pure lazy LISP built on a custom arena allocator. Grift is perfect for embedded systems, WebAssembly, or environments where `std` is unavailable.

## 📦 Installation

```bash
# Install the REPL
cargo install grift --features std

# Or add to your Cargo.toml for library use (no_std by default)
[dependencies]
grift = "1.4"
```
