# grift

grift is a tiny Lisp in the style of John Shutt's Kernel, built on fexprs. Its one special
primitive is `$vau`, which makes an operative: a combiner that receives its operands unevaluated,
together with the caller's environment. `$if`, `$define!`, `$lambda`, `$let`, `$cond` and the rest
are written in grift itself, in [`library.grift`](crates/grift/src/library.grift).

It is made to be embedded in a Rust program (the host), down to bare-metal devices:

- the core crate is `#![no_std]` with `alloc`, has no dependencies and no `unsafe` code, and
  builds for `thumbv6m-none-eabi` (Cortex-M0);
- it uses the memory the allocator gives it and sets no limits of its own; when the allocator
  refuses, evaluation returns an `out-of-memory` error instead of aborting;
- it reaches the outside world only through functions the host registers;
- every error goes to the host as a code (a symbol) and the values involved.

## The language on one screen

```lisp
; Values: () #t #f #inert #ignore, integers, symbols, pairs, "strings",
; environments, operatives, applicatives, host functions.

($define! $quote ($vau (x) #ignore x))      ; operands arrive unevaluated
($quote (+ 1 2))                            ; => (+ 1 2)

($define! square ($lambda (x) (* x x)))     ; $lambda = wrap of a $vau
(map square (list 1 2 3))                   ; => (1 4 9)
square                                      ; => #[applicative #[operative (x) #ignore (* x x)]]

($define! square ($lambda (x) (+ x x)))     ; environments are mutable: redefine freely
(square 5)                                  ; => 10

($let ((x 2) ((y z) (list 3 4))) (+ x (* y z)))    ; formals are trees => 14
($cond ((<? 1 0) "no") (#t "yes"))                 ; => "yes"
($if (eq? 1 1) "same" "different")                 ; $if needs both branches

(eval ($quote (+ 1 2)) (make-environment ground-environment))   ; eval needs an environment
(bindings (($vau () e e)))                  ; the names defined at top level
(bindings ground-environment)               ; every primitive and library name
(error ($quote my-code) 1 2)                ; stops evaluation; the host gets my-code (1 2)
```

The Rust primitives are `$vau wrap unwrap eval make-environment bind! bindings cons car cdr eq?
select type-of + - * div <? error`. The library adds `$define! $quote list $if $sequence $lambda
not? $and? $or? =? apply map $let $cond length append equal? ground-environment`. Integers are
checked `i64`s. There is no quote shorthand, no mutation of pairs, no continuations and no way to
catch an error inside grift. [`docs/LANGUAGE.md`](docs/LANGUAGE.md) has the details: each
combiner, the error codes, and where grift differs from the Revised⁻¹ Report on Kernel
([05-07.pdf](05-07.pdf)).

## The runner

```sh
cargo run --release -- prog.grift more.grift   # runs files in order, in one environment
cargo run --release < prog.grift               # runs stdin
rlwrap cargo run --release                     # interactive: prints each result
```

`grift FILE...` runs the files; with no files it reads stdin, and when stdin is a terminal it
evaluates each complete input and prints its result. It defines two host functions, `display`
and `newline`. Data goes to stdout, errors to stderr as `SOURCE: CODE VALUE...`. It exits with 0,
1 on a grift error, 2 on a usage error or unreadable input; a closed stdout ends it quietly. It
has no line editor: `rlwrap grift` gives one. Limits belong to the OS: `timeout 10 grift prog`
for time, `prlimit --as=…` or `ulimit -v` for memory.

## Embedding

```rust
use grift::{Error, Grift, Value};

fn twice(g: &mut Grift, args: &[Value]) -> Result<Value, Error> {
    match args {
        [Value::Int(n)] => n.checked_mul(2).map(Value::Int).ok_or(Error {
            code: g.symbol("overflow")?,
            values: args.to_vec(),
        }),
        _ => Err(Error { code: g.symbol("type-error")?, values: args.to_vec() }),
    }
}

fn main() -> Result<(), Error> {
    let mut g = Grift::new()?; // the ground environment and the library
    g.define("twice", twice)?; // a host function, in the top-level environment
    g.eval("($define! f ($lambda (x) (twice (+ x 1))))")?;
    assert_eq!(g.eval("(f 20)")?, Value::Int(42)); // every form; the last one's value
    let mut s = String::new();
    let f = g.eval("f")?;
    assert!(g.write(f, &mut s).is_ok()); // into any core::fmt::Write
    assert_eq!(s, "#[applicative #[operative (x) #ignore (twice (+ x 1))]]");
    Ok(())
}
```

- `Grift::new() -> Result<Grift, Error>`, `eval(&mut self, &str) -> Result<Value, Error>`,
  `write(&self, Value, &mut dyn fmt::Write)`, `define(&mut self, &str, HostFn)`,
  `symbol(&mut self, &str) -> Result<Symbol, Error>`, `string(&self, Value) -> Option<&str>`.
- `HostFn` is `fn(&mut Grift, &[Value]) -> Result<Value, Error>`; it gets evaluated arguments and
  may call `eval` itself.
- `Error { code: Symbol, values: Vec<Value> }`.
- **A returned `Value` stays valid until the next `eval`**, which may collect it. Each heap
  reference carries its slot's generation, so a stale value is refused (`stale-value`, or
  `#[stale]` when written) rather than read as some other object. To keep a value, bind it in
  the environment.

`examples/chip` runs grift on a Cortex-M board image under QEMU (`scripts/qemu.sh`, which needs
`qemu-system-arm`): it installs an allocator, registers one host function and prints over
semihosting.

## Inside

One crate, about 1,200 lines of Rust: values and a heap that is a growable `Vec` of slots
(`heap.rs`), a mark-and-sweep collector, a reader and a printer (`read.rs`, `print.rs`), and the
evaluator (`eval.rs`): an explicit machine loop over a frame stack, so no Rust recursion grows
with a program's nesting, and `eval` in tail position is a proper tail call. The collector runs
only at the top of the machine loop, where the machine's frames, argument stack and current
expression are all the state there is; they are its roots, so nothing is rooted by hand.

[`DECISIONS.md`](DECISIONS.md) records the decisions behind this design. `jshutt.pdf` is Shutt's
thesis, *Fexprs as the basis of Lisp function application*.

## License

MIT OR Apache-2.0
