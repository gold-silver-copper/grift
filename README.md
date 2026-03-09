# grift

Grift is a small Kernel-inspired Lisp interpreter for constrained Rust
environments. It implements first-class operatives (`vau`), applicatives,
first-class environments, and tail-call optimization on top of a fixed-size
arena with no heap allocation and no `unsafe` code.

The project is aimed at embedded or resource-bounded settings where a normal
allocator-backed Lisp runtime would be a poor fit, while still preserving a
substantial part of the vau-calculus execution model.

## Highlights

- `#![no_std]`, `no_alloc`, `#![forbid(unsafe_code)]`
- Fixed-capacity arena with free-list allocation
- Mark-and-sweep garbage collection, triggered on OOM and available explicitly
  via `(gc-collect)`
- Kernel-style operative/applicative model
- First-class mutable environments with lexical parent lists
- Tail-call optimization via a trampoline evaluator
- Immutable pairs and `CharPair`-based string storage
- Symbol interning
- Checked integer arithmetic
- Rust native-function registration through `Lisp::register_native`
- Lazy prelude loading from `prelude.grift`

## What It Implements

Grift is not a Scheme clone. The language model is centered on `vau`, where an
operative receives its operands unevaluated together with the caller's
environment.

That gives the runtime three important properties:

- user-defined control forms are first-class values rather than a separate macro
  system
- `lambda` is derived from the operative model rather than being primitive
- environments are explicit runtime values that can be captured, passed around,
  and selectively exposed

The current builtin surface includes:

- operatives such as `quote`, `if`, `define!`, `fn!`, `set!`, `begin`, `cond`,
  `and`, `or`, `let`, `vau`, and `current-environment`
- applicatives such as arithmetic, list primitives, equality, `eval`, `wrap`,
  `unwrap`, environment constructors, GC control, and raw read/write helpers

## Rust API

```rust
use grift::{Lisp, Value};

let lisp: Lisp<20_000> = Lisp::new();

assert_eq!(lisp.eval("(+ 1 2)"), Ok(Value::Number(3)));
assert_eq!(lisp.eval("(car (cons 1 2))"), Ok(Value::Number(1)));
```

The const generic parameter sets the arena capacity in slots. A larger value
gives the interpreter more space for code, data, environments, temporary
evaluation state, and garbage-collector working room.

Native Rust functions can be exposed as Lisp applicatives:

```rust
use grift::{ArenaIndex, ArenaResult, Lisp, LispOps, extract_arg};

fn double(lisp: &dyn LispOps, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (n, _rest): (isize, ArenaIndex) = extract_arg(lisp, args)?;
    lisp.number(n * 2)
}

let lisp: Lisp<20_000> = Lisp::new();
lisp.register_native("double", double).unwrap();

assert_eq!(lisp.eval("(double 21)"), Ok(grift::Value::Number(42)));
```

## Lisp Example

```lisp
;; Operatives receive operands unevaluated.
(define! my-quote
  (vau (x) #ignore x))

(my-quote (+ 1 2))   ; => (+ 1 2)

;; lambda can be defined in terms of vau.
(define! lambda
  (vau (formals . body) e
    (wrap
      (eval (cons 'vau
                  (cons formals
                        (cons #ignore body)))
            e))))

(define! double
  (lambda (x)
    (* x 2)))

(double 5)           ; => 10

;; The caller environment can be captured explicitly.
(define! my-if
  (vau (test then else) e
    (if (eval test e)
        (eval then e)
        (eval else e))))

;; Tail recursion runs through the trampoline evaluator.
(fn! fib (n a b)
  (if (= n 0)
      a
      (fib (- n 1) b (+ a b))))

(fib 50 0 1)         ; => 12586269025
```

## Runtime Model

Every runtime value lives inside `Arena<Value, N>`. The evaluator does not rely
on heap allocation, and the runtime keeps a small set of reserved singleton
slots for values such as `NIL`, booleans, `#inert`, `#ignore`, the ground
environment, the global environment, GC roots, and the symbol intern list.

Current implementation details that matter:

- strings are stored as linked `CharPair` chains rather than contiguous buffers
- empty string is represented internally as `NIL`
- user code runs in the global environment, which is a child of the builtin
  ground environment
- GC is currently demand-driven: the evaluator collects when allocation hits
  `OutOfMemory`

More detail is in [docs/architecture/ARCHITECTURE.md](docs/architecture/ARCHITECTURE.md).

## Build And Test

```bash
cargo build --workspace
cargo test --workspace
```

Run the benchmark example:

```bash
cargo run -p grift --example fib_bench --release
```

Run the REPL:

```bash
cargo run -p grift --features repl
```

## Toolchain

The workspace MSRV is **Rust 1.85** (edition 2024), as declared in
`Cargo.toml`.

## Further Reading

- [docs/architecture/ARCHITECTURE.md](docs/architecture/ARCHITECTURE.md): current architecture and language notes
- [docs/grift/index.md](docs/grift/index.md): generated API-oriented documentation
- [05-07.pdf](05-07.pdf): Revised-1 Report on the Kernel Programming Language
- [jshutt.pdf](jshutt.pdf): "vau: the ultimate abstraction"

## License

MIT OR Apache-2.0
