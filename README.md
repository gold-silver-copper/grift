# grift

grift is a Lisp built entirely on fexprs: `vau!` is its one primitive operative, and there are no
macros and no other special forms. Everything else, `if!` included, is a grift definition you can
read and redefine, in [`library.grift`](crates/grift/src/library.grift). It is both an extension
language for Rust programs, reshaped by their users while they run, and a general scripting
language with a Unix runner.

- [`docs/VISION.md`](docs/VISION.md), the guide: what grift is and the decisions behind it.
- [`docs/LANGUAGE.md`](docs/LANGUAGE.md), the reference: every value, primitive, library form and
  error kind, and where grift differs from Kernel.
- [`DECISIONS.md`](DECISIONS.md): each decision, append-only.

The core crate has no dependencies and no `unsafe` code; it does no I/O, starts no threads and
keeps no global state: the host owns all of that. It uses the memory the allocator gives it and
sets no limits of its own; when the allocator refuses, evaluation returns an `out-of-memory`
error instead of aborting. It reaches the outside world only through functions the host
registers, and every error goes to the host with its kind, its values and where it happened.

## The language on one screen

```lisp
; Values: () #true #false #inert #ignore, integers, floats, symbols, pairs, "strings",
; contexts, operatives, applicatives.

(define! quote! (vau! (x) #ignore x))       ; operands arrive unevaluated
(quote! (add 1 2))                          ; => (add 1 2)

(define! square (lambda! (x) (multiply x x)))  ; lambda! = wrap of a vau!
(map square (list 1 2 3))                   ; => (1 4 9)
square                                      ; => #[applicative #[operative (x) #ignore (multiply x x)]]
(define! square (lambda! (x) (add x x)))    ; only bindings change: redefine freely
(square 5)                                  ; => 10

(let! ((x 2) ((y z) (list 3 x))) (add x (multiply y z)))   ; in order, parameters are trees => 8
(conditional! ((less-than? 1 0) "no") (#true "yes"))       ; => "yes"
(when! (same? 1 1) "same")                  ; => "same"; if! needs both branches; tests are booleans
(divide -7 2)                               ; => -3, Rust's / (remainder: Rust's %)
(add 1.5 (integer-to-float 2))              ; => 3.5; (add 1 1.5) is a type-error
(equal? 0.0 -0.0)                           ; => #true; (same? 0.0 -0.0) => #false

(define! here (current-context))            ; contexts are values
here                                        ; prints its own names: #[context ... here ...]
(evaluate (quote! (add 1 2)) here)          ; evaluate needs a context
(define! n 0)
((lambda! () (define-in! here n (add n 1))))   ; define-in! binds in a given context
(names here)                                ; the names defined at top level, sorted
built-in-names                              ; every primitive and library name
(finally! (first 1) (define-in! here n 0))  ; the cleanup runs, then the error goes on
(catch! (first 1) e (evaluate-in! kind e))  ; => type-error; e binds kind, values, trace
(raise (error (quote! my-kind) 1 2))        ; error makes one, raise raises it
(binds!? here n first nope)                 ; => #false: nope isn't bound
(read-form "(add 1 2) x" 0)                 ; => ((add 1 2) . 9): the form and the offset after it
(value-to-string (list 1.0 "a"))            ; => "(1.0 \"a\")": the printer's text
(string-append "x=" (integer-to-string 42)) ; => "x=42"; offsets count bytes
```

The Rust primitives are `vau! wrap unwrap evaluate evaluate-with-cleanup call-with-handler raise
context bind names pair first rest same? select type-of add subtract multiply divide remainder
less-than? power square-root sine cosine tangent exponential natural-logarithm floor ceiling round
truncate absolute-value integer-to-float float-to-integer string-byte-length string-byte
byte-substring string-append code-point-to-string symbol-to-string string-to-symbol
integer-to-string string-to-integer float-to-string string-to-float read-form value-to-string`. The
library adds
`define! quote! list if! sequence! lambda! current-context evaluate-in! not? empty-list? boolean?
inert? ignore? integer? float? symbol? pair? string? context? operative? applicative? and!? or!?
apply any? all? map for-each when! conditional! let! finally! catch! define-in! error length append
reverse equal? greater-than? less-than-or-equal? greater-than-or-equal? minimum maximum
find-association member? binds!? binds? filter fold range string-search string-starts-with?
string-ends-with? string-split string-join string-trim string-pad-left string-pad-right
string-to-lowercase built-in-names`.

## The runner

```sh
cargo run --release -- script.grift an argument   # runs a script, which gets the arguments
cargo run --release < program.grift               # runs stdin
rlwrap cargo run --release                        # interactive: prints each result
```

`grift SCRIPT ARGUMENT...` runs the script; a first line starting with `#!` is skipped, so
`#!/usr/bin/env grift` makes a script a command. With no script it reads the program from stdin,
and when stdin is a terminal it evaluates each complete input and prints its result. Data goes
to stdout, errors to stderr as `SOURCE: KIND VALUE...` and the combinations it happened in:
`stdin: type-error #[primitive select] 1 in (if! 1 2 3)`. It exits with 0, 1 on a grift error or
a failed write (`output-error`), 2 on a usage error or unreadable input, or the status the program
gave `exit`; a closed pipe on stdout ends it quietly. It has no line editor: `rlwrap grift` gives
one. Limits belong to the OS: `timeout 10 grift script` for time, `prlimit --as=…` for memory.

For scripting, the program gets a small fixed set of operations, and the core does none of this:

| Operation | Meaning |
|---|---|
| `(display v ...)`, `(newline)` | Writes to stdout: strings without quotes, everything else as it prints. |
| `standard-input` | A context of operations: `(read-line)`, the next line with its newline, `""` at the end (Rust's `BufRead::read_line`); `(read-all)`, the rest. A program read from stdin can't read it again (`closed`). |
| `standard-error` | A context of operations: `(write string)`, as a file's, to stderr: a message before `(exit 2)`, say. |
| `(open-file path)` | A context of operations on the file, opened for reading: `read-line`, `read-all`, `close`, and `path`. |
| `(create-file path)` | A context of operations on the file, created or emptied for writing: `(write string)`, `close`, and `path`. |
| `command-line-arguments` | The list of the arguments after the script, as strings. |
| `(environment-variable name)` | Its value, a string; `unset-variable` when it has none. |
| `(exit status)` | Ends the run with `status`, 0 to 255. The program stops at its next step, as when the host interrupts it: no `catch!` can keep it going, and its cleanups run. |
| `(run-process program argument ...)` | Runs the program to its end, with no input, and gives a context binding `status` (its exit code, or `#inert` if a signal ended it), `output` and `error-output` (what it wrote, as strings). |

Call an operation a context holds by looking it up there: `((evaluate-in! write f) "text")`. The
operations call host functions that are bound only in a context no program can reach, so a
program reaches a file only through a context it was given; closing is explicit, and using a file
after it is closed is the error `closed`. The runner's error kinds: `file-error` (the path and the
system's message), `encoding-error` (bytes that aren't UTF-8), `closed` (the path),
`unset-variable` (the name), `process-error` (the program and the message), `output-error`,
`stdout-closed`.

```lisp
#!/usr/bin/env grift
; Copies standard input to the file named by the first argument, numbering the lines.
(define! (path) command-line-arguments)
(define! out (create-file path))
(define! read-line (evaluate-in! read-line standard-input))
(define! copy
  (lambda! (n)
    (let! ((line (read-line)))
      (when! (not? (same? line ""))
        ((evaluate-in! write out) (string-append (integer-to-string n) " " line))
        (copy (add n 1))))))
(copy 1)
((evaluate-in! close out))
(display (evaluate-in! output (run-process "wc" "-l" path)))
```

[`crates/grift-cli/tests/count-words.grift`](crates/grift-cli/tests/count-words.grift) is a longer
script, run by the runner's tests.

## Embedding

```rust
use grift::{Error, Grift, Value};

/// The host's state: here, how often `twice` ran. The host lends it to each `evaluate` and `call`.
struct Board {
    calls: u32,
}

fn twice(g: &mut Grift<Board>, board: &mut Board, args: &[Value]) -> Result<Value, Error> {
    board.calls += 1;
    match args {
        [Value::Integer(n)] => n.checked_mul(2).map(Value::Integer).ok_or_else(|| g.error("overflow", args)),
        _ => Err(g.error("type-error", args)),
    }
}

fn main() -> Result<(), Error> {
    let mut board = Board { calls: 0 };
    let mut g = Grift::new()?; // the built-in context and the library
    let f = g.host_function("twice", twice)?; // an applicative, bound nowhere yet
    g.define("twice", f)?; // in the top-level context
    g.evaluate(&mut board, "(define! f (lambda! (x) (twice (add x 1))))")?;
    assert_eq!(g.evaluate(&mut board, "(f 20)")?, Value::Integer(42)); // the last form's value
    let f = g.evaluate(&mut board, "f")?;
    assert_eq!(g.call(&mut board, f, &[Value::Integer(1)])?, Value::Integer(4)); // values, not text
    assert_eq!(board.calls, 2);
    let hi = g.string("hi")?; // data made by the host
    let list = g.pair(hi, Value::EmptyList)?;
    assert_eq!(g.as_pair(list), Some((hi, Value::EmptyList)));
    let forms = g.read("(f 1) (first 1)")?; // read without evaluating
    let mut s = String::new();
    assert!(g.write(forms, &mut s).is_ok()); // into any std::fmt::Write
    assert_eq!(s, "((f 1) (first 1))");
    let e = g.evaluate(&mut board, "(f (quote! x))").unwrap_err();
    let mut trace = String::new();
    assert!(e.trace.iter().try_for_each(|&v| g.write(v, &mut trace)).is_ok());
    assert_eq!(trace, "(add x 1)(twice (add x 1))(f (quote! x))"); // innermost first
    Ok(())
}
```

- `Grift<T = ()>`: `T` is the host's state. The host owns it and lends it, `&mut T`, to each
  `evaluate` and `call`, which pass it to host functions. A host function gets it as an argument
  of its own beside `&mut Grift<T>`, so the borrow checker sees both borrows; no globals.
- `Grift::new() -> Result<Grift<T>, Error>`; `evaluate(&mut self, &mut T, &str) -> Result<Value,
  Error>`; `call(&mut self, &mut T, Value, &[Value]) -> Result<Value, Error>` (Kernel's `apply`:
  an applicative's combiner gets the values as its operands, in a new empty context);
  `read(&mut self, &str) -> Result<Value, Error>` (the list of forms, nothing evaluated);
  `write(&self, Value, &mut dyn fmt::Write)`; `host_function(&mut self, &str, HostFunction<T>) ->
  Result<Value, Error>` (an applicative, bound nowhere); `define(&mut self, &str, Value)` (binds
  in the top-level context); `context(&mut self) -> Result<Value, Error>` (a new child of the
  built-in context, a private top level); `symbol(&mut self, &str) -> Result<Symbol, Error>`;
  `string(&mut self, &str)`, `pair(&mut self, Value, Value)`, both `-> Result<Value, Error>`; `as_str(&self,
  Value) -> Option<&str>`, `as_pair(&self, Value) -> Option<(Value, Value)>`; `error(&mut self,
  &str, &[Value]) -> Error`; `set_interrupt_flag(&mut self, &'static AtomicBool)`.
- `HostFunction<T>` is `fn(&mut Grift<T>, &mut T, &[Value]) -> Result<Value, Error>`; it gets
  evaluated arguments and may call `evaluate` or `call` itself.
- **The host boundary.** A host resource (a file, a buffer, a socket) appears in grift as a
  context of operations on it, never as a number a program could forge. Keep the host functions
  that take a resource's number private: don't `define` them; hand them to grift code, evaluated
  in a private `context()`, that makes the operations as closures over them, as the runner's
  [`runner.grift`](crates/grift-cli/src/runner.grift) does. Closing is explicit; using a resource
  after it is closed is the error `closed`.
- `Error { kind: Symbol, values: Vec<Value>, trace: Vec<Value> }`. `trace` lists the
  combinations of the program that were being evaluated, innermost first, each once, out to the
  top-level form; the library's own code is skipped, so `(if! 1 2 3)` fails at `(if! 1 2 3)`,
  not inside `if!`. A call in tail position has finished, so it isn't listed.
- **Interrupting.** After `g.set_interrupt_flag(&STOP)` with `static STOP: AtomicBool`, setting
  `STOP` from another thread or a signal handler stops the evaluation at its next step with the
  error `interrupted` (cleanups run) and clears the flag. grift sets no limit of its own; this is
  how the host applies one.
- **A returned `Value` stays valid until the next `evaluate` or `call`**, which may collect it.
  Each heap reference carries its slot's generation, so a stale value is refused (`stale-value`,
  or `#[stale]` when written) rather than read as some other object. To keep a value, bind it in
  a context. A value a host function makes stays valid until it returns: allocation never
  collects.

`examples/emacs` is an Emacs-style editor whose editor is grift code: the host moves bytes and
files, and C-g interrupts a runaway command.

### Host API changes, before and after (2026-11-01, the guide and the new names)

```rust,ignore
// before
g.define("twice", twice)?;
let v = g.eval(&mut state, "(car (cons 1 2))")?;
let s = g.make_string("hi")?;
let p = g.cons(s, Value::Nil)?;
if let Some((car, cdr)) = g.uncons(p) { /* ... */ }
if let Some(text) = g.string(v) { /* ... */ }
match v { Value::Int(n) => {} Value::Sym(s) => {} Value::Env(r) => {} _ => {} }
report(e.code, &e.values, &e.at);
// after
let f = g.host_function("twice", twice)?; // bound nowhere yet: define it, or keep it private
g.define("twice", f)?;
let v = g.evaluate(&mut state, "(first (pair 1 2))")?;
let s = g.string("hi")?;
let p = g.pair(s, Value::EmptyList)?;
if let Some((first, rest)) = g.as_pair(p) { /* ... */ }
if let Some(text) = g.as_str(v) { /* ... */ }
match v { Value::Integer(n) => {} Value::Float(x) => {} Value::Symbol(s) => {} Value::Context(r) => {} _ => {} }
report(e.kind, &e.values, &e.trace);
```

`HostFn` is `HostFunction`, `Prim` is `Primitive`, `Ref` is `Reference`, and `Value`'s variants
are spelled out: `EmptyList`, `Integer`, `Float` (new), `Symbol`, `String`, `Context`,
`Operative`, `Applicative`, `Primitive`. `Value` is `PartialEq` but no longer `Eq` (floats).

## Inside

One crate, about 1,800 lines of Rust: values and a heap that is a growable `Vec` of slots
(`heap.rs`), contexts that are hash maps from symbols to values, a mark-and-sweep collector, a
reader and a printer (`read.rs`, `print.rs`), and the evaluator (`evaluate.rs`): an explicit
machine loop over a frame stack, so no Rust recursion grows with a program's nesting, and
`evaluate` in tail position is a proper tail call. The collector runs only at the top of the
machine loop, where the machine's frames, argument stack, errors waiting for a cleanup and
current expression are all the state there is; they are its roots, so nothing is rooted by hand.
The same point checks the host's interrupt flag.

`jshutt.pdf` is Shutt's thesis, *Fexprs as the basis of Lisp function application*.

## License

MIT OR Apache-2.0
