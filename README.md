# grift

grift is a minimal, pure, type-strict Lisp built on John Shutt's vau
calculus. It follows Kernel's evaluation model and names, with a much
smaller core:

- **One primitive operative.** `$vau` is the only combiner that receives
  its operands unevaluated and is implemented in Rust. `$if`, `$define!`,
  `$lambda`, `$let`, `$cond`, `$sequence` and every other special form are
  written in grift itself.
- **No mutation.** Pairs, strings and environments never change once
  observed. Definitions are *declarations*: values that a body applies to
  its environment to get a new child environment.
- **Strict types.** No truthiness, no punning, typed errors from every
  primitive, an enforced `$` sigil for operatives, opaque encapsulated
  types, and higher-order contracts with blame.
- **Delimited control.** Multi-prompt delimited continuations
  (`make-prompt`, `push-prompt`, `with-sub-cont`, `push-sub-cont`) replace
  `call/cc`. Errors, effect handlers, generators and dynamic binding are
  library code on top of them.

The Rust core is 44 primitives, an evaluator, a reader/printer and a
garbage collector. It is `#![no_std]` (it needs `alloc`), has no
dependencies and no `unsafe` code.

## Example

```lisp
;; Operatives receive their operands unevaluated, plus the caller's
;; environment. There is no quote; it is a one-liner.
($define! $quote ($vau (x) #ignore x))
($quote (+ 1 2))                       ; => (+ 1 2)

;; $lambda is library code over $vau and wrap.
($define! square ($lambda (x) (* x x)))
(map square (list 1 2 3))              ; => (1 4 9)

;; Definitions never mutate: redefining shadows, and closures keep the
;; binding they saw.
($define! n 1)
($define! get-n ($lambda () n))
($define! n 2)
(list n (get-n))                       ; => (2 1)

;; Mutual recursion is one definition with a definiend tree.
($define! (even?? odd??)
  (list ($lambda (k) ($if (=? k 0) #t (odd?? (- k 1))))
        ($lambda (k) ($if (=? k 0) #f (even?? (- k 1))))))

;; Delimited control.
($reset (+ 1 ($shift k (list (k 1) (k 10)))))   ; => (2 11)

;; Errors are delivered to a handler, which may resume the computation.
(with-error-handler ($lambda (e) (error-object-resume e 41))
                    ($lambda () (+ 1 (car 5))))  ; => 42

;; Contracts with blame.
($define/contract! inc (-> integer? integer?) ($lambda (x) (+ x 1)))
(inc "a")   ; contract-violation: a caller of inc broke the contract
```

## Rust API

```rust
use grift::{Ctx, Error, Grift, Value};

let mut g = Grift::new();
g.eval("($define! square ($lambda (x) (* x x)))").unwrap();
let v = g.eval("(square 12)").unwrap();
assert_eq!(g.write(v), "144");

// Natives are applicatives that receive evaluated arguments.
fn double(_: &mut Ctx, args: &[Value]) -> Result<Value, Error> {
    match args {
        [Value::Int(n)] => Ok(Value::Int(n * 2)),
        _ => Err(Error::new("type-error", "double expects one integer")),
    }
}
g.register_native("double", double).unwrap();
let v = g.eval("(double 21)").unwrap();
assert_eq!(g.write(v), "42");
```

`Grift::new` loads the library. Cloning a booted interpreter is much
cheaper than booting a new one. Values returned to the host stay valid
until the next evaluation unless pinned with `Grift::pin`.

## Running

```sh
cargo run -p grift --features repl            # REPL (prompt: Λ>)
cargo run -p grift --features repl -- a.grift # run files
cargo test --workspace                         # Rust and grift test suites
cargo run -p grift --example fib_bench --release
```

The REPL and file runner provide two natives, `print` and `newline`. They
are the only side effects available to programs.

## Layout

- `crates/grift/src/` — the Rust core: values, heap and collector, reader,
  printer, evaluator, primitives.
- `crates/grift/lib/` — the library, in grift, loaded in order at startup.
- `crates/grift/tests/grift/` — the grift-level test suite; assertion
  helpers are written in grift (`_harness.grift`).
- `docs/architecture/LANGUAGE_SPECIFICATION.md` — the language.
- `docs/architecture/ARCHITECTURE.md` — the implementation.

## References

- [05-07.pdf](05-07.pdf): the Revised⁻¹ Report on the Kernel Programming
  Language.
- [jshutt.pdf](jshutt.pdf): John Shutt, *Fexprs as the basis of Lisp
  function application; or $vau: the ultimate abstraction*.
- Dybvig, Peyton Jones & Sabry, *A Monadic Framework for Delimited
  Continuations*.
- Kiselyov, Shan & Sabry, *Delimited Dynamic Binding*.
- Findler & Felleisen, *Contracts for Higher-Order Functions*.

MSRV is Rust 1.85 (edition 2024).

## License

MIT OR Apache-2.0
