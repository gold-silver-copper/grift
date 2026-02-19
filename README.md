# grift

a lisp interpreter implementing vau calculus, suitable for bare metal devices and embedded use

## Features

- `no_std`, `no_alloc`, `#![forbid(unsafe_code)]`
- Fixed-size arena with free-list allocation and contiguous string storage
- Vau calculus: first-class operatives (fexprs) subsume both functions and macros
- First-class mutable environments with lexical parent chains
- Applicative/operative combiner distinction (Kernel-style)
- Tail-call optimization via trampoline
- Mark-and-sweep garbage collection 
- Immutable pairs, call-by-value evaluation
- Symbol interning
- Checked integer arithmetic

## Usage (Rust API)

```rust
use grift::{Lisp, Value};

let lisp: Lisp<20000> = Lisp::new();

assert_eq!(lisp.eval("(+ 1 2)"), Ok(Value::Number(3)));
assert_eq!(lisp.eval("(car (cons 1 2))"), Ok(Value::Number(1)));
```

The const generic parameter (`20000`) sets the arena capacity in slots.

## Usage (Lisp)

Grift's distinguishing feature is the `vau` operative, which receives its operands
unevaluated along with the caller's environment. `lambda` is derived from `vau`
and `wrap`:

```lisp
;; vau creates an operative (fexpr) — operands are not evaluated
(define! my-quote (vau (x) #ignore x))
(my-quote (+ 1 2))   ; → (+ 1 2), not 3

;; lambda is sugar for (wrap (vau params #ignore body))
(define! double (lambda (x) (* x 2)))
(double 5)            ; → 10

;; An operative that selectively evaluates using the caller's environment
(define! my-if (vau (test then else) e
  (if (eval test e)
    (eval then e)
    (eval else e))))

;; Tail-recursive fibonacci
(define! fib (lambda (n a b)
  (if (= n 0) a
    (fib (- n 1) b (+ a b)))))
(fib 50 0 1)          ; → 12586269025
```

## Architecture

Grift stores all values in a flat `[Cell<Slot<Value>>; N]` array. Each `Value`
variant fits in two machine words plus a tag. The evaluator is a trampoline loop:
tail-position combiners update `expr` and `env` and re-enter the loop rather than
recursing. Garbage collection uses a mark bitmap and mark stack allocated on the
Rust stack (not the arena), triggered when occupancy exceeds 75%. Environments are
the sole mutable type — pairs, symbols, strings, and combiners are all immutable
once allocated. See [ARCHITECTURE.md](ARCHITECTURE.md) for full details.

## Build

```bash
cargo build --workspace
cargo test --workspace
```

Run the benchmark suite:

```bash
cargo run -p grift --example fib_bench --release
```

Run the REPL (requires the `repl` feature):

```bash
cargo run -p grift --features repl
```

## Documentation
- [Kernel Spec](05-07.pdf) -- Revised-1 Report on the Kernel Programming Language
- [$vau the ultimate](jshutt.pdf) -- Fexprs as the basis of Lisp function application or $vau : the ultimate abstraction
- [ARCHITECTURE.md](ARCHITECTURE.md) -- System architecture, arena design, GC, TCO
- [LANGUAGE.md](LANGUAGE.md) -- Language reference: types, primitives, evaluation rules
- [INTERNALS.md](INTERNALS.md) -- Contributor guide: module structure, adding builtins

## License

MIT OR Apache-2.0
