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
((lambda! () (bind here (quote! n) (add n 1)))) ; bind binds in a given context
(names here)                                ; the names bound at top level, sorted (the runner's operations too)
built-in-names                              ; every primitive and library name
(finally! (first 1) (bind here (quote! n) 0)) ; the cleanup runs, then the error goes on
(catch! (first 1) e (evaluate-in! kind e))  ; => type-error; e binds kind, values, trace
(raise (error (quote! my-kind) 1 2))        ; error makes one, raise raises it
(binds? here (quote! nope))                 ; => #false: nope isn't bound
(lookup here (quote! nope) 0)               ; => 0: a lookup with a default; unbind removes one
(catch-kind! (first 1) type-error e 0)      ; => 0; any other kind goes on
(sort (list 3 1 2) less-than?)              ; => (1 2 3), stable
(element-at (list 1 2 3) 2)                 ; => 3; take, drop, second, third, last
(read-form " (add 1 2) x" 0)                ; => ((add 1 2) 1 10): the form, its start and end
(value-to-string (list 1.0 "a"))            ; => "(1.0 \"a\")": the printer's text
(join "x=" (integer-to-string 42)) ; => "x=42"; offsets count bytes
(bytes-to-string #bytes(104 105))           ; => "hi"; bytes that aren't UTF-8 are encoding-error
```

The Rust primitives are `vau! wrap unwrap evaluate evaluate-with-cleanup evaluate-with-handler raise
context bind unbind lookup names pair first rest same? select type-of add subtract multiply divide
remainder less-than? bitwise-and bitwise-or bitwise-exclusive-or shift-left shift-right
square-root sine cosine tangent exponential natural-logarithm floor ceiling round truncate
integer-to-float float-to-integer byte-length byte-at slice search join
code-point-to-string string-to-symbol integer-to-string string-to-integer string-to-float read-form
value-to-string string-to-bytes bytes-to-string
list-to-bytes bytes-to-list`. The library adds `define! quote! list if! lambda! sequence! when! conditional!
let! finally! catch! catch-kind! current-context evaluate-in! not? empty-list? inert? ignore? boolean? integer?
float? symbol? pair? string? bytes? context? operative? applicative? and!? or!? apply any? all? map
for-each record error length append reverse equal? greater-than? less-than-or-equal?
greater-than-or-equal? minimum maximum absolute-value power find-association member? binds?
symbol-to-string float-to-string filter fold range sort element-at second third last take drop
string-starts-with? string-ends-with? string-split string-join string-trim
string-less-than? string-repeat string-pad-left string-pad-right string-to-lowercase
bindings standard-context built-in-names`.

## The runner

```sh
cargo run --release -- script.grift an argument   # runs a script, which gets the arguments
cargo run --release < program.grift               # runs stdin
rlwrap cargo run --release                        # interactive: prints each result
```

`grift SCRIPT ARGUMENT...` runs the script; `#!/usr/bin/env grift` makes it a command, since the
reader takes `#!` as starting a comment, as `;` does. The first argument is always the script,
whatever its spelling: `grift --help`, `grift --version` and `grift -` look for files of those names,
and one that isn't there is a `file-error` followed by the usage, status 1. With no script it reads
the program from stdin. Data goes to stdout, errors to stderr as `SOURCE: KIND VALUE...` and the
combinations it happened in, each with its file and line: `stdin: type-error 1 in (if! 1 2 3) at
stdin:1`. It exits with 0, 1 on a grift error, a failed write (`file-error`) or a script it can't
read, 141 on a write to a pipe no one reads, or the status the program gave `exit`. A script path or
argument that isn't UTF-8 is refused before anything runs, as `grift: encoding-error` with its bytes:
text can't hold it, and a lossy copy would name another file. A standard stream the shell closed
(`grift script >&-`) is open on `/dev/null` by the time grift starts, as Rust's runtime makes it, so
it is `/dev/null`: what is written to it is lost, reading it ends at once, and nothing fails.

When stdin is a terminal, grift is a session. It prompts `> ` on stderr, and `  ` while a form is
still open, reads lines until the input holds only whole forms, evaluates them in turn and prints
the last one's value on stdout: `1 2 3` prints `3`. An input that doesn't read runs none of its forms;
an error stops the rest of its input. Lines are numbered from the session's first, so `at stdin:12` is
the session's twelfth line, not counting lines a program read. A program's `read-line` takes the next
line typed, and `read-bytes` what it asks for of it, the rest of the line staying for the next read:
`(read-bytes 3)` then `abcdef` gives `#bytes(97 98 99)`, and `def` is the next input. Errors are shown
and the session goes on. A pipe closing ends it with 141; otherwise it ends 0 unless the program calls
`exit`, input ends inside a form, or standard input or error fails or is closed by `close`, which ends
it with 1. C-c ends it as it ends any program, its definitions with it: the runner handles no
signal. It has no line editor: `rlwrap grift` gives
one. Limits belong to the OS: `timeout 10 grift script` for time, `prlimit --as=…` for memory.

For scripting, the program gets a small fixed set of operations, and the core does none of this:

| Operation | Meaning |
|---|---|
| `(display v ...)` | Writes to stdout: strings without quotes, everything else as it prints. |
| `(display-error v ...)` | The same, to stderr: `(display-error "wc: " name ": " n "\n")`. |
| `standard-input` | A context of operations, as `open-file`'s, with `path` `"standard-input"`: `(read-line)`, the next line with its newline, `""` at the end (Rust's `BufRead::read_line`); `(read-all)`, the rest; `(read-bytes count)`, the next `count` bytes, fewer at the end, as a bytes value, `#bytes()` at the end; it waits only for those, so it reads a pipe that stays open. A program read from stdin has read it to its end, so reads give `""` and `#bytes()`. A line or rest that isn't UTF-8 is `encoding-error` and is read all the same: the next read starts after it, so text that may not be UTF-8 is read with `read-bytes`. |
| `standard-output`, `standard-error` | Contexts of operations, as `create-file`'s: `(write data)`, a string or a bytes value, to stdout or stderr, and `path` and `close`. After `close`, `display` or `display-error` is the error `closed` too; the descriptor stays open, and the runner's own report of an error still prints. |
| `(open-file path)` | A context of operations on the file, opened for reading: `read-line`, `read-all`, `read-bytes`, `close`, and `path`. |
| `(create-file path)` | A context of operations on the file, created or emptied for writing: `(write data)`, a string's or a bytes value's bytes as they are, `close`, and `path`. |
| `(read-file path)` | The file's whole text. |
| `(read-forms source text)` | The list of the text's forms, nothing evaluated; each combination keeps `source` and its line, so an error's trace says `at source:12`. A text that doesn't read is the `parse-error` naming `source`. |
| `(position form)` | A combination's source and line, `("lib.grift" 12)`, if it was read with one; `()` for anything else. |
| `(load path)` | Evaluates the forms of the file in turn in the caller's context, as if they were written there, and gives `#inert`. Inside a `lambda!` that is the body's own context, gone when it returns; a wrapper that loads where it is called is a `vau!`: `(define! use (vau! (path) caller (evaluate (list load path) caller)))`. A relative path is relative to the working directory, as for every other file operation; a file beside the script is `(load (beside-script "lib.grift"))` with `(define! beside-script (lambda! (path) (string-join (reverse (pair path (rest (reverse (string-split script-path "/"))))) "/")))`. All the forms are read first, so nothing runs if one doesn't read; each `load` reads and runs the file again. Each combination of the file knows its line, so an error's trace says `at lib/table.grift:12` beside it, as for the script's own. A file that loads itself, directly or not, loads without end, as an endless recursion runs. |
| `command-line-arguments` | The list of the arguments after the script, as strings. |
| `script-path` | The script's path, as given; `""` when the program is stdin. |
| `(wall-clock-nanoseconds)`, `(elapsed-nanoseconds)` | Integers: the nanoseconds since 1970-01-01 00:00 UTC by the system's clock, which can be set back (a date; a seed); the nanoseconds since the run started, by a clock that never goes back (timing). |
| `(environment-variable name)` | Its value, a string; `unset-variable` when it has none. |
| `(exit status)` | Ends the run with `status`, 0 to 255, by a limit of kind `exit`: no `catch!` can keep it going. A `finally!` whose body it leaves runs its cleanup forms; a cleanup that calls `exit` ends there, its later forms cut, and the last `exit` says the status. |
| `(run-process program argument ...)` | Runs the program to its end, with no input, and gives a context binding `status` (its exit code, or `#inert` if a signal ended it), `output` and `error-output` (what it wrote, as strings). |

Call an operation a context holds by looking it up there: `((evaluate-in! write f) "text")`. The
operations call host functions that are bound only in a context no program can reach, so a
program reaches a file only through a context it was given; closing is explicit, and using a file
after it is closed is the error `closed`. The runner's error kinds: `file-error` (the path and the
system's message), `encoding-error` (bytes that aren't UTF-8), `closed` (the path),
`unset-variable` (the name), `process-error` (the program and the message). `display` and
`display-error` fail as a write on their file does. A write to a pipe no one reads, on any file,
ends the run as `exit` does, by a limit of kind `broken-pipe`: no `catch!` can keep it going, each
`finally!` runs its cleanup forms, and the run ends quietly with status 141, as a shell says of a
program SIGPIPE ended: `grift script | head` is quiet, and a run that ended this way never reports
success. A program's own error of that name is only that, and uncaught is status 1. Memory refused while reading, a process's output included, is the limit `out-of-memory`;
`run-process` still reads both of the process's pipes to their ends and waits for it to end.

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
        ((evaluate-in! write out) (join (integer-to-string n) " " line))
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
    let top = g.top_level();
    g.evaluate_in(&mut board, &top, "init.grift", "(define! f (lambda! (x) (twice (add x 1))))")?;
    assert_eq!(g.evaluate(&mut board, "(f 20)")?, Value::Integer(42)); // the last form's value
    let f = g.evaluate(&mut board, "f")?;
    assert_eq!(g.call(&mut board, &f, &[Value::Integer(1)])?, Value::Integer(4)); // values, not text
    assert_eq!(board.calls, 2);
    let hi = g.string("hi")?; // data made by the host
    let list = g.pair(hi.clone(), Value::EmptyList)?; // a list is pairs, built from its end
    assert_eq!(g.as_pair(&list)?, Some((hi, Value::EmptyList)));
    let forms = g.read("(f 1) (first 1)")?; // read without evaluating
    let mut s = String::new();
    g.write(&forms, &mut s).unwrap(); // into any std::fmt::Write
    assert_eq!(s, "((f 1) (first 1))");
    let e = g.evaluate_in(&mut board, &top, "user.grift", "\n(f (quote! x))").unwrap_err();
    let mut text = String::new();
    g.describe(&e, &mut text).unwrap(); // innermost first, with the source and line
    assert_eq!(text, "type-error x in (add x 1) at init.grift:1\n  from (twice (add x 1)) at init.grift:1\n  from (f (quote! x)) at user.grift:2");
    let kept = g.keep(f)?; // alive across evaluations until released
    g.release(kept).map_err(|(e, _)| e)?;
    Ok(())
}
```

- `Grift<T = ()>`: `T` is the host's state. The host owns it and lends it, `&mut T`, to each
  `evaluate` and `call`, which pass it to host functions. A host function gets it as an argument
  of its own beside `&mut Grift<T>`, so the borrow checker sees both borrows; no globals.
- **Evaluating:** `evaluate(&mut T, &str)` at top level; `evaluate_in(&mut T, &context, source,
  &str)` in a given context, each combination keeping its line in `source`, so errors say
  `at config.grift:12`; `call(&mut T, &f, &[Value])` (Kernel's `apply`: an applicative's combiner
  gets the values as its operands, in a new empty context); `read(&str)` or
  `read_in(source, text)`, the list of forms, nothing evaluated. `read_in` keeps each
  combination's source and opening line for later evaluation. If `read_in` or `evaluate_in`
  cannot read the text, its `parse-error` keeps `(reason line column)` in the values and the
  supplied source name as a symbol innermost in the trace. `read` and `evaluate` keep an empty
  trace for unreadable text.
- **Contexts, without running anything:** `context(Some(&parent))` or `context(None)` (a record);
  `private_context()`, a new child of the built-in context, a private top level; `top_level()`;
  `bind(&context, &parameters, value)`, as the primitive: `symbol(name)` gives a `Symbol`, and one
  name is `Value::Symbol(g.symbol("x")?)`;
  `lookup(&context, &symbol)`, `None` where nothing binds it; `names(&context)`, sorted;
  `define(name, value)` at top level. `evaluate_library_in`, as `evaluate_in`, for a host's own
  grift code such as a prelude: its code, and the code it builds as it runs, count as library code,
  as grift's own: traces skip it. Code a program reads or builds is the program's.
- **Values:** `string`, `bytes(&[u8])`, `pair`, `symbol`; read with `as_str`, `as_bytes`,
  `as_pair`, `symbol_name`, each `Result<Option<_>, Error>` or `Result<_, Error>`, so a reclaimed
  value is `stale-value`, never a wrong answer; `write(&v, out)`. A list is built with `pair` from
  its end and read with `as_pair` from its start. Host text is written as a string literal the
  same way: `string`, then `write`. A `Value::Primitive` is opaque: a host compares and writes it,
  and can't name or make one.
- `HostFunction<T>` is `fn(&mut Grift<T>, &mut T, &[Value]) -> Result<Value, Error>`; it gets
  evaluated arguments and may call `evaluate` or `call` itself.
- **The host boundary.** A host resource (a file, a buffer, a socket) appears in grift as a
  context of operations on it, never as a number a program could forge. Keep the host functions
  that take a resource's number private: don't `define` them; hand them to grift code, evaluated
  in a `private_context()`, that makes the operations as closures over them, and bind only what
  that code makes where programs look: `(define! open (lambda! (path) (let! ((n (open-host path)))
  (record (quote! (read close)) (list (lambda! () (read-host n)) (lambda! () (close-host n)))))))`.
  The runner's [`runner.grift`](crates/grift-cli/src/runner.grift) does this for its files. Closing is explicit; using a resource
  after it is closed is the error `closed`.
- `Error { kind: Symbol, values: Vec<Value>, trace: Vec<Value>, .. }`. Its values are the values
  involved, never who failed: `(first 1)` gives `type-error 1`. Its trace lists the
  combinations of the program that were being evaluated, innermost first, each once, out to the
  top-level form; the library's code, read or built, is skipped, so `(if! 1 2 3)` fails at
  `(if! 1 2 3)`, not inside `if!`. A call in tail position has finished, so it isn't listed. For unreadable
  input to `read_in` or `evaluate_in`, the trace starts with the supplied source symbol; the
  values stay `(reason line column)`. `position(&at)` (`Result<Option<_>, Error>`) is a
  combination's source and line; `describe(&e, out)` writes the whole error for a person;
  `symbol_name(&e.kind)` reads its kind; `error(kind, &[Value])` makes one.
- **Interrupting.** `g.interrupter()` gives an `Interrupter`, `Send`, `Sync` and cloneable, for one
  run: the run in progress when it is taken, or, taken between runs, the next to begin; a clone is
  for the same run. `stop()` from any thread stops that run at its next step, inside a long walk of a
  tree, or as its last host call returns, with the error `interrupted`, and says whether it was in
  progress. `true` doesn't say the stop acted: a run about to end can still return its value.
  `interrupted` runs cleanups on its way out as any error does, but a stop can also land in a cleanup
  that has begun, and cut it, even before its first form. Before its run begins, after it ends, and
  in any other run, a stop does nothing and says `false`, so no handle, however late, reaches a run it
  wasn't taken for: a watchdog takes one as its job begins and needs no lock. Stops waiting together
  count as one; a later stop stops the run again, even in a cleanup that doesn't end. `force()` ends
  the handle's run however its cleanups behave, with no stop needed first: each is cut before its first
  form, at every level, and the run ends with `interrupted`, whatever else fails on the way.
  `examples/stops.rs` shows stop, force, `is_limit` and giving a run up: `cargo run -p grift --example stops`. grift sets no limit of its own; a timer thread holding an interrupter is how the
  host applies one. A host function that panics ends the run it was called from: the panic goes on
  to the host, no cleanup runs, and the next evaluation starts afresh. One that replaces the
  interpreter it was given ends the run with `foreign-value`.
- **Limits.** `e.is_limit()` says the host or the machine began an error, not a program: handlers
  pass it, a program's error in a cleanup doesn't replace it, and it stays one through nested runs.
  A host function makes its own with `g.error(kind, values).as_limit()`, and reads `e.kind` and
  `e.values` when the run has ended: the runner's `exit` carries its status this way. It says the host
  began the ending, not that your stop did: a newer limit in a cleanup replaces it, so a host that gives
  scripts a function making limits trusts its own record of what it asked for. A host function that
  swallows a nested run's stop and returns lets the outer run go on; it can't hide a force.
- **Waiting.** A host function can answer "not yet", so a script waits for an event in a call that
  looks like any other while the host's loop goes on, with no thread held. See below.
- **A returned `Value` stays valid until the next `evaluate`, `call`, `start` or `resume`**, which may collect it.
  `evaluate_in` does too. `keep(value)` gives a `Kept` that holds it until `release(kept)`;
  `kept(&k)` reads it. Every
  value carries its interpreter's identity and each heap reference its slot's generation, so a
  stale value is `stale-value` and another interpreter's is `foreign-value`, everywhere, never
  read as some other object. A value a host function makes stays valid until it returns
  or starts a nested `evaluate`, `evaluate_in` or `call`: allocation never collects, a nested run may.

### A watchdog

A deadline per job is a thread holding an interrupter and the loop's record of which job is
running. The watchdog stops only while that record names its job, under the record's lock; the
loop clears the record under the same lock as the run returns, so a late stop can't reach the next
job. `stop()` says `false` while no run is in progress, as when the deadline passes before the job
has begun, so the watchdog asks again until the job runs or is over.

```rust
use grift::{Error, Grift};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Error> {
    let mut g: Grift = Grift::new()?;
    let running = Arc::new(Mutex::new(None::<u32>)); // the job on the CPU, by the loop's count
    let (deadlines, due) = mpsc::channel::<(u32, Duration)>();
    let (mut handle, watched) = (g.interrupter(), Arc::clone(&running));
    let watchdog = thread::spawn(move || {
        for (job, after) in due {
            thread::sleep(after);
            loop {
                let now = watched.lock().unwrap();
                if *now != Some(job) || handle.stop() {
                    break; // the job is over, or stopped
                }
                drop(now);
                thread::yield_now(); // not begun yet: ask again
            }
        }
    });
    let jobs = ["(define! spin (lambda! () (spin)))", "(spin)", "(add 1 2)"];
    let mut ended = Vec::new();
    for (job, src) in (0..).zip(jobs) {
        *running.lock().unwrap() = Some(job);
        deadlines.send((job, Duration::from_millis(20))).unwrap();
        let result = g.evaluate(&mut (), src);
        *running.lock().unwrap() = None; // before the next job begins
        ended.push(result.map_err(|e| e.is_limit()));
    }
    drop(deadlines);
    watchdog.join().unwrap();
    assert!(matches!(ended[..], [Ok(_), Err(true), Ok(grift::Value::Integer(3))]));
    Ok(())
}
```

A force after a grace period goes in the same place: the watchdog sleeps again and, if the record
still names its job, calls `force()` under the lock.

### Waiting: pausable runs

Register the function with `parking_function`, and have it answer `Answer::Later(request)`.
`start` calls a combiner and `start_in` runs source text, each in a run that can wait. A run that
ends gives `Ran::Finished(value)`; one that comes to such a call is set aside, and you get a
`Ran::Parked(parked)`. Your loop goes on: other runs start, finish and park over the same heap,
sharing bindings. When the event comes, `resume(state, parked, Ok(value))` makes that call return
`value` (or `Err(error)` raise `error`), and the script carries on from there, with every `catch!`
and `finally!` around the call still in place.

```rust
use grift::{Answer, Error, Grift, Parked, Ran, Value};

/// A script's `(sleep ms)`. The value in `Later` says what's wanted: the run holds it, and
/// `Parked::request` gives it to the loop. Do the work there, since a run may not get to park.
fn sleep(g: &mut Grift<Vec<String>>, _: &mut Vec<String>, args: &[Value]) -> Result<Answer, Error> {
    match args {
        [ms] => Ok(Answer::Later(ms.clone())),
        _ => Err(g.error("arity-error", args)),
    }
}

fn say(g: &mut Grift<Vec<String>>, said: &mut Vec<String>, args: &[Value]) -> Result<Value, Error> {
    let [name] = args else {
        return Err(g.error("arity-error", args));
    };
    said.extend(g.as_str(name)?.map(str::to_owned));
    Ok(Value::Inert)
}

fn main() -> Result<(), Error> {
    let mut g: Grift<Vec<String>> = Grift::new()?;
    let mut said = Vec::new();
    let f = g.parking_function("sleep", sleep)?;
    g.define("sleep", f)?;
    let f = g.host_function("say", say)?;
    g.define("say", f)?;
    g.evaluate(
        &mut said,
        "(define! job (lambda! (name ms)
           (finally!
             (sequence! (sleep ms) (say name) (sleep ms))
             (say (join name \" closed\")))))",
    )?;

    // An event loop: one thread, nothing blocks. Timers stand for what a real host waits on.
    let mut timers: Vec<(i64, Parked)> = Vec::new();
    let mut now = 0;
    let wake_at = |now, ran, timers: &mut Vec<(i64, Parked)>| {
        if let Ran::Parked(p) = ran {
            let Value::Integer(ms) = *p.request() else { return };
            timers.push((now + ms, p));
        }
    };
    let top = g.top_level();
    for script in ["(job \"a\" 30)", "(job \"b\" 10)", "(job \"c\" 20)"] {
        wake_at(now, g.start_in(&mut said, &top, "job", script)?, &mut timers);
    }
    while let Some(next) = (0..timers.len()).min_by_key(|&i| timers[i].0) {
        let (at, parked) = timers.swap_remove(next);
        now = at;
        wake_at(now, g.resume(&mut said, parked, Ok(Value::Inert))?, &mut timers);
    }
    assert_eq!(said, ["b", "c", "b closed", "a", "c closed", "a closed"]);

    // A run whose handle was dropped is found again and given up, its cleanups run. A cleanup
    // that waits comes back as `Ran::Parked`; dropped, it is found again in the next turn.
    drop(g.start_in(&mut said, &top, "job", "(job \"d\" 10)")?);
    while let Some(parked) = g.forgotten() {
        let given_up = g.error("given-up", &[]).as_limit();
        let _ = g.resume(&mut said, parked, Err(given_up));
    }
    assert_eq!((said.last().map(String::as_str), g.parked()), (Some("d closed"), 0));
    Ok(())
}
```

- **One shot, by the type.** `Parked` is neither `Copy` nor `Clone`, and `resume` takes it by value,
  so a second resume doesn't compile.
- **Giving a run up** is an answer like any other: `resume(state, parked, Err(g.error("given-up", &[]).as_limit()))`.
  Nothing catches a limit, every `finally!` runs, and the run ends with it, or with another limit
  that replaced it. A cleanup that errors ends there, and the next one out runs. A cleanup that
  waits comes back as `Ran::Parked`, a run that waits like any other: answer it with a value and it
  goes on, or with a limit and it ends there. One that never ends is yours to stop or force; each
  `resume` being a run of its own, take a handle for it just before the `resume`: one taken before it
  waited does nothing and says `false`. A cleanup that installs a new cleanup that
  waits comes back each time, and neither reaches it while it waits: have your parking function call
  `force()` before it answers `Later`, which ends the run there, as in the example below.
- **A run you never finish** stays alive, holding what it refers to, as a kept value does: its
  cleanups are still owed. `parked()` counts these. Dropping the handle doesn't end it, and
  `forgotten()` gives a new handle, not saying which run it is: put what identifies a job in its
  request. **Dropping the interpreter** with runs parked runs none of their
  cleanups, the same as a process dying.
- **Stops and limits.** A stop reaches its handle's run while it is in progress. A parked run isn't:
  `stop()` then does nothing and says `false`. Each `resume` is a run of its own for stops. One thread
  runs one run at a time, so your loop goes on only while runs are parked. A watchdog over several jobs
  takes a handle as each job begins or resumes, and the loop keeps the deadlines of parked jobs,
  answering them with a limit. A stop that reaches a run
  as it would park stops it instead, so no stop is lost at a park. The run's cleanups then run, and
  one of them may park, the stop having been acted on: `true` and `Ran::Parked` can go together.
  Cleanups after out of memory have what a collection frees. `resume` makes no heap object to begin;
  refused the little it does need, the run waits still and the handle comes back.
- **What can't wait.** A run under `evaluate` or `call`, or any run inside a host call, has Rust
  frames that can't be set aside. A waiting call there is the error `cannot-park`, its trace saying which call,
  and `can_park()` lets a host function do something else instead.
  `start` inside a host call is such a run. `resume` there is refused (`cannot-park`), the handle
  coming back in `ResumeError::parked` with the run untouched: queue it for your loop. Another
  interpreter's handle comes back the same way, with `foreign-value`, an error of the interpreter you
  called.
- **Hosts that never park** keep the signatures of `evaluate`, `call` and `host_function`, but two
  things reach every host. Memory refused makes one collection due, before the nearest cleanup's first
  step, and a cleanup refused memory all the same is passed over for the next, which that new refusal
  collects for. And a kind's name
  means nothing: a program's own `interrupted` or `out-of-memory` is caught like any error, and only
  `is_limit()` says one is real.
- **`resume`'s error** says which: `parked: Some(handle)` if nothing happened, `None` if the run
  went on and ended with this error. `?` turns it into the plain `Error`, a returned handle going
  where a dropped one goes.

Ending a run whose every cleanup waits again: the host's state says the host is ending it, and the
parking function forces the run before it answers.

```rust
use grift::{Answer, Grift, Ran, Value};

fn main() -> Result<(), grift::Error> {
    let mut g: Grift<bool> = Grift::new()?; // the state: whether the host is ending this run
    let wait = g.parking_function("wait", |g, ending, _| {
        if *ending {
            g.interrupter().force();
        }
        Ok(Answer::Later(Value::Inert))
    })?;
    g.define("wait", wait)?;
    g.evaluate(&mut false, "(define! chain (lambda! () (finally! (wait) (chain))))")?;
    let top = g.top_level();
    let Ran::Parked(p) = g.start_in(&mut false, &top, "job", "(chain)")? else { unreachable!() };
    let given_up = g.error("given-up", &[]).as_limit();
    let Ok(Ran::Parked(p)) = g.resume(&mut false, p, Err(given_up)) else { unreachable!() };
    // Given up, its cleanup waits again, and would each time. Forced at the wait, it ends.
    let given_up = g.error("given-up", &[]).as_limit();
    let Err(ended) = g.resume(&mut true, p, Err(given_up)) else { unreachable!() };
    assert!(ended.parked.is_none() && ended.error.is_limit());
    assert_eq!(g.symbol_name(&ended.error.kind)?, "interrupted");
    assert_eq!(g.parked(), 0);
    Ok(())
}
```

`examples/emacs` is an Emacs-style editor whose editor is grift code: the host moves bytes and
files, and C-g interrupts a runaway command. It runs real shell commands (`M-!`) by waiting: each is
a run with a `finally!` around its wait, and the editor stays responsive. `programs/` holds whole programs on the runner, each
with its own `test.sh`. `cargo test --workspace` runs all of those scripts against the test-profile
runner, so the local suite and CI both exercise the corpus.

## Inside

One crate, about 3,000 lines of Rust: values and a heap that is a growable `Vec` of slots
(`heap.rs`), contexts that are hash maps from symbols to values, a mark-and-sweep collector, a
reader and a printer (`read.rs`, `print.rs`), and the evaluator (`evaluate.rs`): an explicit
machine loop over a frame stack, so no Rust recursion grows with a program's nesting, and
`evaluate` in tail position is a proper tail call. The collector runs at the top of the machine
loop and before a host's evaluation or call needs its first heap slot. The machine's frames,
argument stack, endings waiting for a cleanup, current expression and checked call inputs are its
roots. Allocation never collects, and a collection needs no memory: its mark stack's room is made
with the interpreter, and what doesn't fit there is found by passes over the heap, so a heap full of
garbage with nothing left to allocate is reclaimed all the same. A run then needs only what its own
work needs: heap slots, which that frees, and a little besides (a new name, a source's line
numbers, a host function's copy of its arguments). An object the heap has no room for makes a collection due, before
the next run needs room to read its input or build its call context, as does any memory refused
that unwinds a run. Reads and constructors do not collect: more reads may be refused until an
evaluation or call provides that recovery point.
The same point checks for a stop. Values cross to the host as the public `Value` (`lib.rs`),
which carries the interpreter's identity; the machine keeps its own compact copy (`local.rs`).
`if!` and `sequence!` are library code with a native twin: the library's own
operative, run in Rust when its operands have the shape the twin handles.

`jshutt.pdf` is Shutt's thesis, *Fexprs as the basis of Lisp function application*.

## License

MIT OR Apache-2.0
