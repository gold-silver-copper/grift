# grift

grift is a tiny Lisp in the style of John Shutt's Kernel, built on fexprs. Its one special
primitive is `$vau`, which makes an operative: a combiner that receives its operands unevaluated,
together with the caller's environment. `$if`, `$define!`, `$lambda`, `$let`, `$cond` and the rest
are written in grift itself, in [`library.grift`](crates/grift/src/library.grift).

It is made to be embedded in a Rust program (the host):

- the core crate has no dependencies and no `unsafe` code; it does no I/O, starts no threads and
  keeps no global state: the host owns all of that;
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

($define! here (get-current-environment))  ; environments are values
(eval ($quote (+ 1 2)) here)                ; eval needs an environment
($define! n 0)
(($lambda () ($set! here n (+ n 1))))       ; $set! changes a binding in a given environment
(bindings here)                             ; the names defined at top level
ground-names                                ; every primitive and library name
"tab\tescape\x1b;[0m"                       ; escapes \\ \" \n \t and \xHEX;
($unwind-protect (car 1) ($set! here n 0))  ; the cleanup runs, then the error goes on
(guard ($lambda () (car 1))                 ; an error leaving the thunk is the handler's
       ($lambda (e) ($remote-eval code e))) ;   e binds code, values, at => type-error
($binds? here n car nope)                   ; => #f: nope isn't bound
(string-append "x=" (number->string 42))    ; => "x=42"; offsets count bytes
(error ($quote my-code) 1 2)                ; stops evaluation; the host gets my-code (1 2)
```

The Rust primitives are `$vau wrap unwrap eval eval-protected make-environment bind! bindings
cons car cdr eq? select type-of + - * div mod <? error guard raise string-length string-byte
substring string-append code-point->string symbol->string string->symbol number->string`. The
library adds `$define! $quote list $if $sequence $lambda get-current-environment not? null? pair?
symbol? string? integer? environment? $and? $or? =? min max apply map for-each $cond $when $let
$let* $unwind-protect $set! length append reverse equal? assoc member? $binds? ground-names`.
Integers are checked `i64`s. There is no quote shorthand, no mutation of pairs and no
continuations; `guard` catches an error, but never the host's interrupt.
[`docs/LANGUAGE.md`](docs/LANGUAGE.md) has the details: each combiner, the error codes, and where
grift differs from the Revised⁻¹ Report on Kernel ([05-07.pdf](05-07.pdf)).

## The runner

```sh
cargo run --release -- prog.grift more.grift   # runs files in order, in one environment
cargo run --release < prog.grift               # runs stdin
rlwrap cargo run --release                     # interactive: prints each result
```

`grift FILE...` runs the files; with no files it reads stdin, and when stdin is a terminal it
evaluates each complete input and prints its result. It defines two host functions, `display`
and `newline`. Data goes to stdout, errors to stderr as `SOURCE: CODE VALUE...`. It exits with 0,
1 on a grift error or a failed write (`output-error`), 2 on a usage error or unreadable input; a
closed pipe on stdout ends it quietly. Unfinished input at the end is a `parse-error`. An
error names the combination it happened in: `stdin: type-error #[primitive select] 1 in ($if 1 2
3)`. It has no line editor: `rlwrap grift` gives one. Limits belong to the OS: `timeout 10 grift prog`
for time, `prlimit --as=…` or `ulimit -v` for memory.

## Embedding

```rust
use grift::{Error, Grift, Value};

/// The host's state: here, how often `twice` ran. The host lends it to each `eval` and `call`.
struct Board {
    calls: u32,
}

fn twice(g: &mut Grift<Board>, board: &mut Board, args: &[Value]) -> Result<Value, Error> {
    board.calls += 1;
    match args {
        [Value::Int(n)] => n.checked_mul(2).map(Value::Int).ok_or_else(|| g.error("overflow", args)),
        _ => Err(g.error("type-error", args)),
    }
}

fn main() -> Result<(), Error> {
    let mut board = Board { calls: 0 };
    let mut g = Grift::new()?; // the ground environment and the library
    g.define("twice", twice)?; // a host function, in the top-level environment
    g.eval(&mut board, "($define! f ($lambda (x) (twice (+ x 1))))")?;
    assert_eq!(g.eval(&mut board, "(f 20)")?, Value::Int(42)); // every form; the last one's value
    let f = g.eval(&mut board, "f")?;
    assert_eq!(g.call(&mut board, f, &[Value::Int(1)])?, Value::Int(4)); // values, not text
    assert_eq!(board.calls, 2);
    let hi = g.make_string("hi")?; // data made by the host
    let pair = g.cons(hi, Value::Nil)?;
    assert_eq!(g.uncons(pair), Some((hi, Value::Nil)));
    let forms = g.read("(f 1) (car 1)")?; // read without evaluating
    let mut s = String::new();
    assert!(g.write(forms, &mut s).is_ok()); // into any core::fmt::Write
    assert_eq!(s, "((f 1) (car 1))");
    let e = g.eval(&mut board, "(f ($quote x))").unwrap_err();
    let mut at = String::new();
    assert!(e.at.iter().try_for_each(|&v| g.write(v, &mut at)).is_ok());
    assert_eq!(at, "(+ x 1)(twice (+ x 1))(f ($quote x))"); // innermost first
    Ok(())
}
```

- `Grift<T = ()>`: `T` is the host's state. The host owns it and lends it, `&mut T`, to each
  `eval` and `call`, which pass it to host functions. A host function gets it as an argument of
  its own beside `&mut Grift<T>`, so the borrow checker sees both borrows; no globals.
- `Grift::new() -> Result<Grift<T>, Error>`; `eval(&mut self, &mut T, &str) -> Result<Value,
  Error>`; `call(&mut self, &mut T, Value, &[Value]) -> Result<Value, Error>` (Kernel's `apply`:
  an applicative's combiner gets the values as its operands, in a new empty environment);
  `read(&mut self, &str) -> Result<Value, Error>` (the list of forms, nothing evaluated);
  `write(&self, Value, &mut dyn fmt::Write)`; `define(&mut self, &str, HostFn<T>)`;
  `symbol(&mut self, &str) -> Result<Symbol, Error>`; `make_string(&mut self, &str)`,
  `cons(&mut self, Value, Value)`, both `-> Result<Value, Error>`; `string(&self, Value) ->
  Option<&str>`, `uncons(&self, Value) -> Option<(Value, Value)>`; `error(&mut self, &str,
  &[Value]) -> Error`; `set_interrupt_flag(&mut self, &'static AtomicBool)`.
- `HostFn<T>` is `fn(&mut Grift<T>, &mut T, &[Value]) -> Result<Value, Error>`; it gets
  evaluated arguments and may call `eval` or `call` itself.
- `Error { code: Symbol, values: Vec<Value>, at: Vec<Value> }`. `at` lists the combinations of
  the program that were being evaluated, innermost first, each once, out to the top-level form;
  the library's own code is skipped, so `($if 1 2 3)` fails at `($if 1 2 3)`, not inside `$if`. A
  call in tail position has finished, so it isn't listed; the list is never longer than the
  frames the error passed.
- **Interrupting.** After `g.set_interrupt_flag(&STOP)` with `static STOP: AtomicBool`, setting
  `STOP` from another thread or a signal handler stops the evaluation at its next step with the
  error `interrupted` (cleanups run) and clears the flag. grift sets no limit of its own; this is
  how the host applies one. A `'static` flag, because a signal handler can reach only a
  `static`; from a thread, a `static` serves as well.
- **A returned `Value` stays valid until the next `eval` or `call`**, which may collect it. Each
  heap reference carries its slot's generation, so a stale value is refused (`stale-value`, or
  `#[stale]` when written) rather than read as some other object. To keep a value, bind it in an
  environment. A value a host function makes stays valid until it returns: allocation never
  collects.

`examples/emacs` is an Emacs-style editor whose editor is grift code: the host moves
bytes and files, and C-g interrupts a runaway command.

### Host API changes, before and after

Where an error happened (2026-11-01, second round): every program combination around it, not
just the innermost.

```rust,ignore
// before
if let Some(at) = e.at { g.write(at, &mut out)?; }
// after: innermost first, out to the top-level form
for &at in &e.at { g.write(at, &mut out)?; }
let innermost = e.at.first();
```

The runner prints the innermost on the error's line and each one around it on a line of its own:

```text
stdin: not-int a in (error ($quote not-int) x)
  from (+ 1 (check x))
  from (run ($quote a))
```

From the first round, 2026-11-01:

Host state, without a global:

```rust,ignore
// before: no way to reach the host's state but a global
thread_local!(static EDITOR: RefCell<Editor> = ...);
fn point(g: &mut Grift, args: &[Value]) -> Result<Value, Error> {
    EDITOR.with(|e| match e.try_borrow_mut() { ... })
}
g.eval("(point)")?;
// after: the state is lent to eval and passed to every host function
fn point(g: &mut Grift<Editor>, ed: &mut Editor, args: &[Value]) -> Result<Value, Error> {
    Ok(Value::Int(ed.point))
}
g.eval(&mut editor, "(point)")?;
```

Making data, and errors:

```rust,ignore
// before: print a literal and read it back; build an error by hand
let v = g.eval(&format!("\"{}\"", escape(text)))?;
Err(Error { code: g.symbol("file-error")?, values: args.to_vec() })
// after
let v = g.make_string(text)?;
let list = g.cons(v, Value::Nil)?;
if let Some((first, rest)) = g.uncons(list) { /* ... */ }
Err(g.error("file-error", args))
```

Calling grift, reading, interrupting:

```rust,ignore
// before: source text for every call; eval-string to read M-: input; kill the process
g.eval(&format!("(handle-key ($quote {key}))"))?;
// after
let handle = g.eval(&mut state, "handle-key")?;
g.call(&mut state, handle, &[key])?;
let forms = g.read(input)?;
static STOP: AtomicBool = AtomicBool::new(false);
g.set_interrupt_flag(&STOP); // another thread: STOP.store(true, Relaxed)
```

## Inside

One crate, about 1,650 lines of Rust: values and a heap that is a growable `Vec` of slots
(`heap.rs`), environments that are hash maps from symbols to values, a mark-and-sweep collector, a reader and a printer (`read.rs`, `print.rs`), and the
evaluator (`eval.rs`): an explicit machine loop over a frame stack, so no Rust recursion grows
with a program's nesting, and `eval` in tail position is a proper tail call. The collector runs
only at the top of the machine loop, where the machine's frames, argument stack, errors waiting
for a cleanup and current expression are all the state there is; they are its roots, so nothing
is rooted by hand. The same point checks the host's interrupt flag.

[`DECISIONS.md`](DECISIONS.md) records the decisions behind this design. `jshutt.pdf` is Shutt's
thesis, *Fexprs as the basis of Lisp function application*.

## License

MIT OR Apache-2.0
