# grift

Grift is a small Kernel-inspired Lisp interpreter for constrained Rust
environments. It implements first-class operatives (`vau`), applicatives,
first-class environments, and tail-call optimization on top of a growable
arena with stable handles and no `unsafe` code.

The library remains suitable for `no_std` environments, while using Rust's
`alloc` crate for growable object storage. The final binary or embedded runtime
must provide a global allocator.

## Highlights

- `#![no_std]`, `alloc`, `#![forbid(unsafe_code)]`
- Growable `Vec`-backed arena with free-list slot reuse
- Mark-and-sweep garbage collection at a configurable soft slot watermark,
  with allocator-failure fallback and explicit collection via `(gc-collect)`
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

- operatives such as `quote`, `if`, `define!`, `set!`, `begin`, `cond`, `and`,
  `or`, `let`, `vau`, and `current-environment`
- applicatives such as arithmetic, list primitives, equality, `eval`, `wrap`,
  `unwrap`, environment constructors, GC control, and raw read/write helpers

The bundled prelude also provides derived forms such as `lambda` and `fn!`,
loaded lazily like the rest of the prelude.

`apply` and `wrap` operate uniformly over first-class callables, including
builtin operatives such as `if`.

## Rust API

```rust
use grift::{Lisp, Value};

let lisp = Lisp::new();

assert_eq!(lisp.eval("(+ 1 2)"), Ok(Value::Number(3)));
assert_eq!(lisp.eval("(car (cons 1 2))"), Ok(Value::Number(1)));
```

Arena capacity is no longer selected at compile time. Object storage grows as
needed, subject to the allocator supplied by the embedding program. Evaluation
collects near a soft logical-slot watermark; the watermark increases when the
reachable working set needs more room.

Native Rust functions can be exposed as Lisp applicatives:

```rust
use grift::{ArenaIndex, ArenaResult, Lisp, LispOps, extract_arg};

fn double(lisp: &dyn LispOps, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (n, _rest): (isize, ArenaIndex) = extract_arg(lisp, args)?;
    lisp.number(n * 2)
}

let lisp = Lisp::new();
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

(apply if (list #t 1 2)) ; => 1
((wrap if) #t 1 2)       ; => 1

;; Tail recursion runs through the trampoline evaluator.
(fn! fib (n a b)
  (if (= n 0)
      a
      (fib (- n 1) b (+ a b))))

(fib 50 0 1)         ; => 12586269025
```

## Runtime Model

Every runtime value lives inside `Arena<Value>`, whose authoritative slot store
is an allocator-backed `Vec`. Runtime references are `ArenaIndex` handles
rather than vector references, so vector reallocation does not invalidate live
objects. Garbage collection can reclaim an unreachable slot and later reuse
its index. Rust code retaining an index across another evaluation must pass it
to `eval_with_roots` or `eval_to_index_with_roots`. The runtime also keeps a
small set of reserved singleton slots for values such as `NIL`, booleans,
`#inert`, `#ignore`, the ground environment, the global environment, GC roots,
and the symbol intern list.

Current implementation details that matter:

- strings are stored as linked `CharPair` chains rather than contiguous buffers
- language-level lists remain linked `Cons` cells; the storage migration does
  not change proper-list or dotted-pair semantics
- empty string is represented internally as `NIL`
- user code runs in the global environment, which is a child of the builtin
  ground environment
- GC runs near a configurable soft slot watermark, retains an
  allocator-failure fallback, and can also be invoked explicitly

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
