# Grift

[![Crates.io](https://img.shields.io/crates/v/grift.svg)](https://crates.io/crates/grift)
[![Documentation](https://docs.rs/grift/badge.svg)](https://docs.rs/grift)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

A minimal `no_std`, `no_alloc` Scheme implementation built on a custom arena allocator. Grift demonstrates that you can build a feature-rich, garbage-collected language without requiring heap allocation — perfect for embedded systems, WebAssembly, or environments where `std` is unavailable.

## 📦 Installation

```bash
# Install the REPL
cargo install grift --features std

# Or add to your Cargo.toml for library use (no_std by default)
[dependencies]
grift = "0.1"
```

## 🚀 Quick Start

### As a Library (no_std)

```rust
use grift::{Lisp, Evaluator, Value};

// Create an interpreter with a 10,000-cell arena
let lisp: Lisp<10000> = Lisp::new();
let mut eval = Evaluator::new(&lisp).unwrap();

// Evaluate expressions
let result = eval.eval_str("(+ 1 2 3)").unwrap();
```

### Interactive REPL

```bash
# Run directly
cargo run -p grift --features std

# Or after installing
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

## 🎯 Project Overview

This repository contains:

| Crate | Description | `no_std` |
|-------|-------------|----------|
| **`grift`** | Unified re-export crate (primary entry point) | ✅ default |
| **`pwn_arena`** | Arena allocator with mark-and-sweep GC | ✅ |
| **`grift_parser`** | Lisp parser with symbol interning | ✅ |
| **`grift_eval`** | Trampolined evaluator with proper TCO | ✅ |
| **`grift_repl`** | Interactive REPL | ❌ (uses std) |
| **`grift_macros`** | Proc macros for stdlib generation | N/A |

## ✨ Lisp Features

### Core Language

| Feature | Description |
|---------|-------------|
| **Proper Tail Calls** | Full TCO via trampolining — no stack overflow on deep recursion |
| **Strict Evaluation** | Call-by-value semantics; arguments evaluated before function application |
| **Lexical Closures** | First-class functions with captured environments |
| **Macros** | `defmacro` with `quasiquote`/`unquote` for metaprogramming |
| **Pattern Matching** | `case` for value matching, `cond` for conditionals |
| **Mutation** | `set!`, `set-car!`, `set-cdr!` for imperative programming |
| **Garbage Collection** | Mark-and-sweep GC controllable from Lisp code |

### Built-in Functions

```lisp
; List operations
(car '(1 2 3))         ; => 1
(cdr '(1 2 3))         ; => (2 3)
(cons 1 '(2 3))        ; => (1 2 3)
(list 1 2 3)           ; => (1 2 3)

; Predicates
(null? '())            ; => #t
(pair? '(1 . 2))       ; => #t
(number? 42)           ; => #t
(symbol? 'foo)         ; => #t
(procedure? car)       ; => #t

; Arithmetic
(+ 1 2 3 4)            ; => 10
(- 10 3)               ; => 7
(* 2 3 4)              ; => 24
(/ 100 5)              ; => 20
(mod 17 5)             ; => 2

; Comparison
(< 1 2)                ; => #t
(= 5 5)                ; => #t
(eq 'a 'a)             ; => #t

; Memory management
(gc)                   ; => (marked collected before)
(gc-enable)            ; Enable automatic GC
(gc-disable)           ; Disable automatic GC
(gc-enabled?)          ; => #t or #f
(arena-stats)          ; => (capacity allocated free usage%)

; Arrays (O(1) indexed access)
(define arr (make-array 5 0))  ; Create array of 5 zeros
(array-ref arr 2)              ; => 0 (get element at index 2)
(array-set! arr 2 42)          ; Set element at index 2
(array-length arr)             ; => 5
(array? arr)                   ; => #t
```

### Special Forms

```lisp
; Conditionals
(if condition then-expr else-expr)
(cond (test1 result1) (test2 result2) (else default))
(case key ((datum1) result1) ((datum2 datum3) result2) (else default))

; Definitions
(define x 42)
(define (square x) (* x x))

; Local bindings
(let ((x 1) (y 2)) (+ x y))
(let* ((x 1) (y (+ x 1))) y)

; Sequences
(begin expr1 expr2 ... exprN)

; Short-circuit boolean
(and expr1 expr2 ...)
(or expr1 expr2 ...)

; Iteration
(do ((i 0 (+ i 1)) (sum 0 (+ sum i)))
    ((= i 10) sum))

; Macros
(defmacro unless (cond then else)
  (list 'if cond else then))

; Runtime evaluation
(eval '(+ 1 2))        ; => 3
(apply + '(1 2 3))     ; => 6
```

## 🔥 Design Philosophy

### 1. No Heap, No Problem

The entire Lisp runs on a fixed-size arena allocated at compile time. No `malloc`, no `Box`, no `Vec` in the core. This makes it:

- **Predictable** — Memory usage is bounded and known upfront
- **Portable** — Works on bare metal, WASM, or any `no_std` target
- **Safe** — No undefined behavior from memory allocation failures

### 2. Strict Evaluation with TCO

All arguments are evaluated before function application (call-by-value), with full tail-call optimization:

```lisp
; Arguments evaluated before function call
(define (add x y) (+ x y))
(add (+ 1 2) (* 3 4))  ; => 15 (both args evaluated first)

; Side effects happen immediately
(define count 0)
(cons (begin (set! count 1) 'a) '())
count  ; => 1 (side effect happened during cons)

; Tail-call optimization works for deep recursion
(define (sum n acc)
  (if (= n 0) acc
      (sum (- n 1) (+ acc n))))  ; Args evaluated before call
(sum 10000 0)                    ; => 50005000 (no stack overflow)
```

### 3. Only `#f` is False

Unlike many Lisps, we follow Scheme's truthiness model:

```lisp
(if nil 'yes 'no)     ; => yes (nil is truthy!)
(if '() 'yes 'no)     ; => yes (empty list is truthy!)
(if 0 'yes 'no)       ; => yes (zero is truthy!)
(if #f 'yes 'no)      ; => no  (only #f is false)
```

## 📊 Memory Management from Lisp

Control the garbage collector directly from your Lisp code:

```lisp
; Check arena status
(arena-stats)          ; => (50000 234 49766 0)
                       ;     ^ capacity
                       ;           ^ allocated
                       ;               ^ free
                       ;                     ^ usage%

; Manual GC control
(gc-disable)           ; Pause GC for performance-critical section
; ... allocate many objects ...
(gc-enable)
(gc)                   ; Force collection now
                       ; => (marked collected before)
```

## 🏗️ Architecture

See the detailed architecture documents:

- **[ARENA_ARCHITECTURE.md](./docs/ARENA_ARCHITECTURE.md)** — How the arena allocator works
- **[LISP_ARCHITECTURE.md](./docs/LISP_ARCHITECTURE.md)** — How the Lisp interpreter works

## 📚 Standard Library

The standard library is defined as static Lisp code, parsed on-demand:

```lisp
; List manipulation
(length '(a b c))      ; => 3
(append '(1 2) '(3 4)) ; => (1 2 3 4)
(reverse '(1 2 3))     ; => (3 2 1)
(nth 2 '(a b c d))     ; => c

; Higher-order functions
(map (lambda (x) (* x x)) '(1 2 3 4))  ; => (1 4 9 16)
(filter (lambda (x) (> x 0)) '(-1 2 -3 4))  ; => (2 4)
(fold + 0 '(1 2 3 4 5))  ; => 15

; List generation
(range 0 5)            ; => (0 1 2 3 4)
(take 3 '(a b c d e))  ; => (a b c)

; Utilities
(identity 42)          ; => 42
(constantly 5)         ; Returns a function that always returns 5
```

## ⚠️ Gotchas and Pitfalls

### Nil is NOT False!

```lisp
; WRONG: Don't use nil/empty list as a false value
(if (filter (lambda (x) (> x 10)) '(1 2 3)) 'has-items 'empty)
; If filter finds nothing, it returns '() (nil), which is TRUTHY!
; This will always print 'has-items even when the list is empty!

; RIGHT: Explicitly check for #f or use null?
(if (null? (filter (lambda (x) (> x 10)) '(1 2 3))) 'empty 'has-items)
; Now it correctly checks if the result is empty

; Note: member returns #f (not nil) when not found, so this works:
(if (member 'x '(a b c)) 'found 'not-found)  ; This is correct!
```

### Arena Capacity is Fixed

```lisp
; If you run out of arena space, you get an error
; Solution: Use a larger arena or trigger GC more often
(gc)  ; Reclaim unreachable objects
```

## 🔧 Project Structure

```
grift/
├── crates/
│   ├── grift/           # Unified re-export crate (primary entry point)
│   ├── pwn_arena/       # Core arena allocator (no_std, no_alloc)
│   ├── grift_parser/    # Lisp parser and value types (no_std)
│   ├── grift_eval/      # Trampolined evaluator (no_std)
│   ├── grift_repl/      # Interactive REPL (uses std for I/O)
│   └── grift_macros/    # Proc macros for stdlib generation
├── docs/
│   ├── ARENA_ARCHITECTURE.md
│   ├── LISP_ARCHITECTURE.md
│   └── SCHEME_R7RS_CONFORMANCE.md
└── README.md
```

## 🛠️ Development

```bash
# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Run the REPL in development
cargo run -p grift --features std
```

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📄 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
