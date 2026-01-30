# Grift

[![Crates.io](https://img.shields.io/crates/v/grift.svg)](https://crates.io/crates/grift)
[![Documentation](https://docs.rs/grift/badge.svg)](https://docs.rs/grift)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

**A minimal `no_std`, `no_alloc` Scheme implementation with arena-based garbage collection.**

Grift is a clean, portable Scheme interpreter that runs anywhere — from embedded microcontrollers to WebAssembly. It requires no heap allocator and has zero external dependencies.

## ✨ Highlights

| Feature | Description |
|---------|-------------|
| **`no_std` + `no_alloc`** | Runs on bare metal, WASM, or constrained environments |
| **Arena Allocation** | Fixed memory footprint known at compile time |
| **Mark-and-Sweep GC** | Garbage collection controllable from Scheme |
| **Proper Tail Calls** | Full TCO via trampolining — no stack overflow |
| **Lexical Closures** | First-class functions with captured environments |
| **R7RS-Inspired** | Scheme semantics with only `#f` as false |

## 🚀 Quick Start

### As a Library (no_std)

```rust
use grift::{Lisp, Evaluator};

// Create an interpreter with a 10,000-cell arena
let lisp: Lisp<10000> = Lisp::new();
let mut eval = Evaluator::new(&lisp).unwrap();

// Evaluate expressions
let result = eval.eval_str("(+ 1 2 3)").unwrap();

// Define and call functions
eval.eval_str("(define (square x) (* x x))").unwrap();
let result = eval.eval_str("(square 7)").unwrap();  // => 49
```

### Interactive REPL

Install with the `std` feature for a full REPL:

```bash
cargo install grift --features std
grift
```

```scheme
Grift Lisp
> (define (factorial n)
    (if (= n 0) 1
        (* n (factorial (- n 1)))))
> (factorial 10)
3628800

> (map (lambda (x) (* x x)) '(1 2 3 4 5))
(1 4 9 16 25)

> (arena-stats)
(50000 127 49873 0)  ; (capacity allocated free usage%)
```

## 📦 Installation

### Cargo.toml

```toml
# Minimal no_std usage (default)
[dependencies]
grift = "0.1"

# With REPL support
[dependencies]
grift = { version = "0.1", features = ["std"] }
```

## 🎯 Features

### Default (no_std, no_alloc)

The default configuration requires no standard library and no heap allocator. Perfect for:

- Embedded systems (ARM Cortex-M, RISC-V, etc.)
- WebAssembly modules
- Bootloaders and kernels
- Any `#![no_std]` context

### `std` Feature

Enables the interactive REPL and formatting utilities:

```rust
use grift::{run_repl, format_value, value_to_string};
```

## 📖 Language Guide

### Literals

```scheme
42          ; Integer
#t #f       ; Booleans (only #f is false!)
'symbol     ; Symbol
"hello"     ; String
#\a #\space ; Characters
'(1 2 3)    ; List
#(1 2 3)    ; Vector
```

### Special Forms

```scheme
(define x 42)                    ; Variable definition
(define (f x) (* x x))           ; Function definition
(lambda (x) (* x x))             ; Anonymous function
(if cond then else)              ; Conditional
(cond (test1 e1) ... (else e))   ; Multi-way conditional
(let ((x 1) (y 2)) (+ x y))      ; Local binding
(let* ((x 1) (y x)) (+ x y))     ; Sequential binding
(begin e1 e2 ... en)             ; Sequence
(and e1 e2 ...)                  ; Short-circuit and
(or e1 e2 ...)                   ; Short-circuit or
(quote x) or 'x                  ; Quote
(quasiquote x) or `x             ; Quasiquote
(set! var val)                   ; Mutation
```

### Built-in Functions

```scheme
; Pairs & Lists
(cons 1 2)         ; => (1 . 2)
(car '(a b))       ; => a
(cdr '(a b))       ; => (b)
(list 1 2 3)       ; => (1 2 3)

; Predicates
(null? '())        ; => #t
(pair? '(1))       ; => #t
(number? 42)       ; => #t
(symbol? 'x)       ; => #t
(procedure? car)   ; => #t
(eq? 'a 'a)        ; => #t

; Arithmetic
(+ 1 2 3)          ; => 6
(- 10 3)           ; => 7
(* 2 3 4)          ; => 24
(/ 10 2)           ; => 5
(modulo 17 5)      ; => 2
(abs -5)           ; => 5
(max 1 5 3)        ; => 5
(expt 2 10)        ; => 1024

; Comparison
(< 1 2)            ; => #t
(= 5 5)            ; => #t

; Strings
(string-length "hello")     ; => 5
(string-append "a" "b")     ; => "ab"
(substring "hello" 1 3)     ; => "el"

; Characters
(char->integer #\A)         ; => 65
(char-upcase #\a)           ; => #\A

; Memory
(gc)               ; Force garbage collection
(arena-stats)      ; => (capacity allocated free usage%)
```

### Standard Library

```scheme
; Higher-order functions
(map f lst)                 ; Apply f to each element
(filter pred lst)           ; Keep elements where pred is true
(fold f init lst)           ; Left fold
(for-each f lst)            ; Apply f for side effects

; List utilities
(length lst)                ; List length
(append a b)                ; Concatenate lists
(reverse lst)               ; Reverse list
(range 0 5)                 ; => (0 1 2 3 4)

; Math (integer arithmetic)
(sqrt 16)                   ; => 4 (integer square root)
(expt 2 10)                 ; => 1024
(gcd 12 8)                  ; => 4
```

## ⚠️ Important: Truthiness

**Only `#f` is false!** This follows R7RS Scheme semantics:

```scheme
(if '() 'yes 'no)   ; => yes  (empty list is truthy!)
(if 0 'yes 'no)     ; => yes  (zero is truthy!)
(if #f 'yes 'no)    ; => no   (only #f is false)
```

## 🏗️ Architecture

Grift is built as a stack of focused crates:

```
┌─────────────────────────────────────────────────────────┐
│  grift (unified re-export crate)                        │
├─────────────────────────────────────────────────────────┤
│  grift_repl [std]  │  grift_eval  │  grift_parser       │
├────────────────────┴───────────────┴────────────────────┤
│  pwn_arena (no_std arena allocator with GC)             │
└─────────────────────────────────────────────────────────┘
```

All crates except `grift_repl` are `no_std` and `no_alloc`.

## 📚 Documentation

- [API Documentation](https://docs.rs/grift)
- [Architecture Guide](docs/LISP_ARCHITECTURE.md)
- [R7RS Conformance Status](docs/SCHEME_R7RS_CONFORMANCE.md)

## 🔧 Building from Source

```bash
# Clone the repository
git clone https://github.com/gold-silver-copper/grift
cd grift

# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Run the REPL
cargo run -p grift --features std

# Run the minimal example
cargo run -p grift --example minimal
```

## 📄 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
