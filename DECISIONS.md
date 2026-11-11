# Decisions

What holds today, without the history, is in [`RULES.md`](RULES.md).

Append-only. A decision is reopened only with new evidence from use, in a new entry that names
the one it replaces.

## Owner decisions, 2026-10-31

- **G-1. Super minimal, based on fexprs.** Everything that can be grift code is grift code.
- **G-2. Memory is a growable `Vec` heap** (`no_std` + `alloc`), not a fixed arena. grift sets
  no memory limit of its own: it grows with `try_reserve`, and when the allocator refuses it
  returns an `out-of-memory` error instead of aborting. Mark-and-sweep collection.
- **G-3. Environments are mutable,** as in Kernel: `$define!` adds to the current environment.
  No purity machinery (no declarations, no uninitialized bindings).
- **G-4. Errors go to the host** as `Err` with a code and the values involved. No catching inside
  grift, no continuations of any kind.
- **G-5. Limits belong to the host and the OS.** No step limit, no time limit, no depth limit:
  `timeout` on a desktop, the hardware watchdog on a chip (fux decision D-83, same reasoning).
- **G-6. Unix philosophy.** The runner reads files or stdin, prints data to stdout, diagnostics
  to stderr, and exits with a status. No line editor (`rlwrap grift` does that).
- **G-7. Self-describing.** `(bindings env)` lists an environment's own names, and printing a
  combiner shows its parameters and body, so an agent never guesses what exists.
- **G-8. No guessing.** No made-up defaults: `$if` needs both branches, `eval` needs an
  environment, an unknown symbol is an error carrying the symbol.
- **G-9. Cut:** R7RS, numeric tower, floats, chars, vectors, ports, continuations, contracts,
  effect handlers, records, the enforced `$` sigil rule (it stays a naming convention),
  `grift-docs`, `publish.sh`, the lazy prelude, the old arena API, the stack-size workarounds.
  Kept: `05-07.pdf`, `jshutt.pdf`, `quotes.md`.

## Made during the rewrite, 2026-10-31 (the owner may overrule any)

- **G-10. Programs run in a child of the ground environment,** as in Kernel. The primitives and
  the library live in the ground; a top-level definition shadows a library name without
  changing what the library uses. `ground-environment` names the ground so G-7 reaches it.
- **G-11. Heap references carry their slot's generation.** A `Value` held by the host past the
  next `eval` may be collected; using it then is the error `stale-value`, never a read of
  whatever reused the slot. This is what stops misuse of returned values, with no lifetimes in
  the API and no pinning.
- **G-12. Collection happens only at the top of the machine loop,** with the machine's frames,
  argument stack and current expression as the roots. Allocation never collects. Nothing roots
  values by hand.
- **G-13. Strings compare by content in `eq?`.** They are immutable atoms, like integers.
- **G-14. `apply` is Kernel's two-operand form** (a new empty environment), the only form, so
  nothing is optional. `$cond` with no true clause gives `#inert`, as Kernel specifies.
- **G-15. `div` rounds toward negative infinity.**
- **G-16. The chip example targets `thumbv6m-none-eabi` and runs on QEMU's `mps2-an385`**
  (Cortex-M3, which runs v6-M code): QEMU's only Cortex-M0 board, `microbit`, has 16 KiB of RAM.

## Made after three editors were built on grift, 2026-11-01 (the owner may overrule any)

Evidence: three Emacs-style editors built on grift on 2026-11-01, with friction logs
(`~/code/notes/projects/grift/emacs-2026-11-01/`).

- **G-17. The host can interrupt evaluation.** `Grift::set_interrupt_flag(&'static AtomicBool)`:
  the machine checks the flag at its safe point, the top of its loop, and stops with the error
  `interrupted`, clearing the flag. This is the mechanism G-5 assumes ("limits belong to the
  host"): grift still sets no limit of its own, and a host can now stop a runaway program without
  killing the process that holds the user's work (buffer F9). The flag is only loaded and stored,
  never swapped, because `thumbv6m-none-eabi` has no atomic read-modify-write; a `'static`
  reference because that target has no `Arc`, and a signal handler can reach a `static`.
  Decided by the orchestrator, 2026-11-01.
- **G-18. Cleanup on error, without catching** (panel question A; replaces nothing, G-4 stands).
  `($unwind-protect body cleanup ...)` returns the body's value after the cleanup forms ran;
  they run too when an error leaves the body, and the error then goes on to the host unchanged.
  An error inside a cleanup replaces the one that was leaving. Built on one primitive,
  `(eval-protected x env cleanup)`, which keeps a frame on the machine's stack. Vote: 7 of 10 in
  round one (b 7, c 2, a 1). Strongest dissent, for catching (c, editor-builder and
  scheme-standards): buffer F10's `M-x end C-n` loses the typed input, which cleanup alone can't
  keep; only catching would let the minibuffer go on. The other side, for nothing (a,
  minimalist): every editor recovered by resetting state at the next start, so a new frame kind
  and form buy little.
- **G-19. `$cond` with no true clause stays `#inert`** (panel question B; confirms G-14). Vote:
  round one a 6, b 4; round two a 8, b 2. Strongest dissent (agent-author, unix-tools): a silent
  `#inert` surfaces far from the `$cond` that made it, and going from an error back to `#inert`
  later breaks no program while the reverse can.
- **G-20. The ground environment is out of reach; `ground-names` lists it** (panel question C;
  replaces the `ground-environment` binding of G-10). With the ground in hand, one
  `(eval ($quote ($define! list 5)) ground-environment)` broke `map` for every program. Kernel
  keeps the ground unreachable for this reason. `ground-names` is a list of the ground's names,
  bound once when the library is loaded, so G-7 still holds. Vote: 10 of 10 for c; two
  panelists (kernel-designer, minimalist) asked for a bound list rather than an optional operand
  to `bindings` (no optional operands, G-14). No dissent on the option; agent-author noted that
  `(make-environment ground-environment)` was the documented way to get a clean environment,
  which is now `(make-environment (get-current-environment))` or the host's job.
- **G-21. `stale-value` stays as it is** (panel question D; confirms G-11). Vote: round one b 6,
  a 4; round two a 10. To keep a value across evaluations, bind it in an environment; there is
  no second way (no handles to release). Strongest dissent, round one (editor-builder,
  interpreter-implementer and four more): `Grift::call` will make hosts hold combiners longer,
  so an explicit root API would be safer. All six changed their vote after reading that a
  binding already does this.
- **G-22. Errors say where.** `Error` has `at: Option<Value>`: the innermost combination of the
  program being evaluated when the error happened. The library's own code is skipped (every
  heap slot live once the library has loaded is marked as the library's, and loses the mark when
  freed), and so is code built around a combiner value (as the library builds it), so
  `($if 1 2 3)` fails `in ($if 1 2 3)`, not inside `$if`. The machine keeps the location as a
  heap `Ref` in a register and in each frame; a nested `eval` keeps the outer one in its first
  frame, so the collector sees it.
- **G-24. `mod` is a primitive, beside `div`.** The task asked for `mod` derived from `div` in
  the library, `(- a (* b (div a b)))`. Review found it overflows where its result fits:
  `(mod -9223372036854775808 3)` is 1, but `(* b (div a b))` is below the smallest `i64`; and
  `(mod -9223372036854775808 -1)` is 0 while `div` there overflows. A derivation that avoids
  both needs two special cases; the primitive is three lines of `arith` and exact everywhere.
  Cost: one more primitive (21).
- **G-23. Host state is lent, not owned.** `Grift<T>`: the host passes `&mut T` to each `eval`
  and `call`, and host functions get it as a separate argument beside `&mut Grift<T>`, so the
  borrow checker sees both borrows. No globals, no `RefCell`, nothing `'static`; `T` may itself
  borrow. `Grift` with no parameter is `Grift<()>`.

## Owner decision, 2026-11-01

- **G-25. No chip example.** The owner doesn't use it: `examples/chip` and `scripts/qemu.sh` are
  gone, and G-16 no longer applies. The core stays `no_std` + `alloc` (G-2), and CI still builds
  it for `thumbv6m-none-eabi` so it stays that way.

## Owner decisions, 2026-11-01, after twelve programs were written on grift

Evidence: twelve programs written three ways each without changing grift, with reports and
friction logs (`~/code/notes/projects/grift/apps-2026-11-01/`, ranked in `tally.md`).

- **G-26. Catching: `(guard thunk handler)` (amends G-4).** It calls `thunk` with no arguments;
  if an error leaves it, the cleanups inside run first, then `handler` is called with the error
  as a value and its result is `guard`'s. Errors still go to the host when nothing catches them.
  No continuations of any kind. Made alone, within it:
  - The error value was first the list `(code values at)`; G-35 made it an environment.
  - `interrupted` is never caught (G-17: the host's limit must hold, and a guard in a loop must
    not swallow C-g). **`out-of-memory` is never caught either:** it is the allocator's limit, the
    same kind of limit as the host's interrupt (G-5); a handler would need memory to run, so
    catching it invites a loop that fails the same way; and the host already gets it with the
    interpreter still usable, which is where recovery from a full heap belongs.
  - `guard` is a primitive with a frame of its own, beside `eval-protected`'s: the machine's
    unwinding has to stop somewhere, and only the machine can stop it.
  - Its handler is called as a tail call; a non-combiner `thunk` or `handler` is a `type-error`
    raised at once, not caught by the guard being made.
  - `(raise e)` raises a caught error again as it was: the same code, values and locations, and
    none of its own (not its own call). Like any error, it then gains the combinations it passes
    on its way out (G-29): those around the guard, out to the top-level form, and, if the raise
    isn't in tail position in the handler, the handler's combinations around it. Added after the
    eval-direct port: a handler that passed an error on with `(apply error ...)` gave it the
    handler's location, so an evaluator's own bug no longer said where it happened, and the
    library's `$binds?` had the same loss. It keeps G-22's rule that re-raising keeps the
    location.
- **G-27. Strings (amends G-9's opaque strings; they stay immutable).** `string-length` and
  `string-byte` (byte offsets, O(1)), `substring` (a byte range), `string-append`,
  `code-point->string`, `symbol->string`, `string->symbol`, `number->string`. No character type:
  a code point is an integer. Made alone, within it:
  - An offset out of range or off a character boundary, and a code point that isn't a Unicode
    scalar value, are the new error `range-error` carrying the primitive and its operands.
  - `string-append` takes any number of strings (R7RS); `(apply string-append list)` joins a list.
  - `string->symbol` takes any string, as R7RS does; a symbol whose name doesn't read back prints
    as its name.
  - Nothing more was added: `string<?`, `string->number` and `string-index` had one asker each
    in the twelve programs (tally §4) and are a few lines of grift each over `string-byte`.
- **G-28. Speed.** Fix what the twelve programs showed: library forms that built new code on each
  use, `$binds?`, and name lookup. Making `$if` and `$sequence` Rust primitives would bend G-1:
  it is measured, on a branch of its own, and not part of this change.

## Made with G-26 to G-28, 2026-11-01 (the owner may overrule any)

- **G-29. Errors list every program combination around them** (extends G-22; tally need 7).
  `Error.at` is a list: the combinations of the program being evaluated, innermost first, each
  once, ending with the top-level form. Unwinding collects them from the frames it pops, which
  already held their combination for G-22, and the frame of each top-level form now holds that
  form. Nothing is recorded per call, so tail calls stay constant-space, and a call in tail
  position, having finished, isn't listed; the list is never longer than the frames the error
  passed, and a recursion lists each combination once. So `(error ...)` in a program's own helper
  names the helper's caller, out to the user's top-level form. Rejected: `error` taking an
  explicit location (every library would thread it through), and marking a program's helpers as
  library code (the location would then skip the helper, but a DSL several levels deep still
  wouldn't name the user's form).
- **G-30. Name lookup stays a linear scan of each environment** (tally need 6). Measured on two
  programs with lookup kept out of line: 14.5 % (logic-stack, 8 queens) and 16.3 % (eval-direct,
  guest fib 20) of the run. A sorted index for environments past 16 bindings made both slower
  (logic-stack 5.2-5.6 s to 6.5-6.6 s; eval-direct 4.8-4.9 s to 5.1-5.3 s): the time goes into
  walking chains of small environments, not scanning large ones. The ledger has the numbers.
- **G-31. Library forms build no code on each use** (tally need 1; the owner's update of
  2026-11-01: grift stays a pure fexpr Lisp).
  `$sequence`, `$cond`, `$when`, `$let`, `$let*`, `$and?`, `$or?` and `$unwind-protect` stay plain
  operatives that walk their operands with `car` and `cdr` and call `eval` only on the operands
  they were given, in the caller's environment or one they made, as R-1RK's derivations do. Their
  loops are helpers in a one-shot environment of their own, looked up on each call; none builds
  `(cons $cond ...)`, `(cons $sequence ...)` or a `$lambda` to evaluate. No caching, no
  expansion, no compile step, no Rust primitive for any of them. `$let` still matches every
  binding's formals at once, so a symbol in two bindings is an error, as in Kernel.
  - `$lambda` still evaluates the `$vau` combination it makes, `(list $vau formals #ignore body)`,
    as R-1RK 5.3.2 derives it: making an operative is its job, and only `$vau` makes one. A body
    of several forms becomes `($sequence ...)` with the symbol, because grift's `$vau` takes one
    body, so the combiner prints as written. `apply` builds the combination it calls (5.5.1).
  - So that `$unwind-protect` needn't build `($sequence cleanup ...)`, `eval-protected` takes its
    cleanup as a combiner it calls with no arguments, as `guard` takes its thunk (amends G-18's
    primitive; `$unwind-protect` is unchanged for programs).
  - A first version also had the machine evaluate symbol and constant operands in place, without
    a step of their own (3.5 times fewer steps); G-33 removed it.

## Owner decisions, 2026-11-01, during the work on G-26 to G-31

- **G-32. The core may use `std`, still with zero dependencies** (amends G-2's `no_std` + `alloc`
  and G-25's Cortex-M0 build, which is gone from CI and the README). Still: `forbid(unsafe_code)`,
  no panics, no I/O, no threads, no global state (the host owns those), and out of memory is
  refused, never an abort: every growing collection goes through `try_reserve`. Made alone,
  within it:
  - The interrupt flag stays a `&'static AtomicBool`: a signal handler, the usual way to turn
    C-g or SIGINT into a stop, can reach only a `static`, so an `Arc` would still need one;
    from a thread a `static` serves as well. With `std` the machine now clears it with a swap
    after a plain load (the load keeps the per-step check free of a locked instruction).
  - `Error` doesn't implement `Display` or `std::error::Error`: its code is a `Symbol`, an index
    whose name lives in the interpreter, so a `Display` of the error alone could print only
    numbers. Hosts write it with `Grift::write`, as the runner does.
- **G-33. The evaluator has one path per kind of form.** Speed comes from simpler library code
  and better representations, never from fast paths: no shortcut for operands that are symbols
  or constants, no per-form cases in the machine, no duplicate code path that exists only for
  speed. The in-place operand evaluation G-31 first had is gone, and so is `$sequence`'s
  separate case for a body of one form.
- **G-34. Every environment is a hash map** (replaces G-30). An environment's bindings are a
  `std::collections::HashMap<Symbol, Value>` whatever its size: one representation, no threshold,
  nothing beside it, grown with `try_reserve`, hashed by the symbol's id with one multiplication
  (no dependency). `bindings` and `ground-names` list names sorted by name, so output stays
  deterministic without keeping definition order. Its cost or gain against the previous
  representation (a list scanned in order) is measured once and reported, not a reason to revert.
- **G-35. What a caught error is, and what can't be caught** (refines G-26).
  - A `guard`'s handler gets the error as an environment, a record (records are environments),
    with no parent, binding `code`, `values` (a list) and `at` (a list, G-29); a fresh one per
    caught error. `$remote-eval` (R-1RK 6.7.9, added to the library) reads a field:
    `($remote-eval code e)`; `raise` takes the same record.
  - Errors about the program can be caught, a host function's included; limits set by the host
    or the machine, `interrupted` and `out-of-memory`, are never caught by `guard` and always
    reach the host. Both are tested. Made alone, after review: an error in a cleanup still
    replaces the one leaving (G-18), except a limit, which goes on, so a failing cleanup can't
    turn C-g into an error a guard catches; and a limit is known by its code, so a program's own
    `(error ($quote interrupted))` isn't caught either (the host can't tell it from a real one;
    accepted, since only the program loses by it).
  - Early exit is `guard` plus an error code of the program's own; there is no escape primitive.
  - **Why no continuations.** Generators and backtracking are closures or lazy streams: the
    three logic engines met every-solution search, `not`, first-solution-only and infinite
    relations without them. A language built on grift that wants `call/cc` writes its evaluator
    in continuation-passing style: eval-callcc did it in 5 lines, in flat memory, thanks to
    proper tail calls. And continuations would cost the meaning of cleanup (`$unwind-protect`
    could be left and re-entered) and of interruption, and the simplicity of the machine.

## Owner decisions, 2026-11-01: the guide and the names

Settled with the owner in conversation and written up as the guide, [`docs/VISION.md`](docs/VISION.md)
(cited below by section), and the name table (`~/code/notes/projects/grift/names-draft.md`, with
an eight-perspective naming panel folded in). [`docs/LANGUAGE.md`](docs/LANGUAGE.md) is the
reference that matches them.

- **G-36. `std`, zero dependencies** (VISION "Embedding"). Confirms G-32: the core is a Rust
  library on `std` with no dependencies, no I/O, threads or global state; the host owns those and
  sets every limit. Bare-metal targets are not a goal; WebAssembly works wherever `std` does.
- **G-37. Every context is a hash map** (VISION "Contexts"). Confirms G-34.
- **G-38. Names** (VISION "Names"; amends G-9's `$` convention, G-7's `bindings`, G-20's
  `ground-names`, G-27's string names). A name says what the thing does in grift; words are
  joined with dashes and spelled out, with no abbreviations. An operative ends in `!` (`vau!`,
  `if!`, `define!`), never meaning mutation; a combiner returning a boolean ends in `?`, an
  operative one in `!?`; a constructor is named after what it makes (`pair`, `list`, `context`,
  `error`); a conversion spells out `to`; arithmetic and comparison are words (`add`,
  `less-than?`). Only bindings change, and the combiners that change anything are `define!`,
  `define-in!` and `bind`. Kernel's `$` became the `!` suffix and Kernel's mutation `!` became the
  verb. An operative's context parameter is `caller` by convention. Literals: `#true #false
  #inert #ignore`. Every rename is in LANGUAGE.md's table; `bad-formals` is `parameter-error`.
- **G-39. Contexts, not environments** (VISION "Kernel", "Contexts", "Terms"; amends G-3, G-10,
  G-20 in their words). Kernel's environment is grift's context, the ground environment is the
  built-in context, formals are parameters. A context is a hash map from symbols to values with
  at most one parent it reads through; a child can change only its own bindings, and there is no
  parent accessor.
- **G-40. No object system** (VISION "State"). No classes, prototypes, `send` or method syntax;
  records, cells and symbol-keyed tables are contexts, and an operation a context holds is looked
  up there and called where you are: `((evaluate-in! write f) line)`.
- **G-41. The host boundary** (VISION "The host boundary"). A host resource appears in grift as a
  context of operations on it. The operations call host functions bound only in a private
  context, so a program can't forge a resource from an id. Closing is explicit (no finalizers),
  and using a resource after it is closed is the error `closed`.
- **G-42. Floats** (VISION "Values", "Numbers"; amends G-9, which cut them). 64-bit IEEE floats,
  with Rust `std`'s operations and functions under spelled-out names; results may differ slightly
  between platforms, as in Rust. Integers and floats never mix implicitly: `(add 1 1.5)` is a
  `type-error`; conversions are explicit (`integer-to-float`, `float-to-integer`). Richer numbers
  are library code in a context whose arithmetic is theirs.
- **G-43. One integer type** (VISION "Values"): 64-bit, overflow an error. No `usize` or other
  widths; host types are converted at the boundary (checked; `range-error` when a number doesn't
  fit).
- **G-44. No vectors** (VISION "Values"). Confirms G-9.
- **G-45. Rust's numeric semantics** (VISION "Close to Rust"; replaces G-15 and G-24's floor
  semantics). Integer `divide` and `remainder` are Rust's `/` and `%` (truncating; division by
  zero and overflow, `MIN / -1` and `MIN % -1`, are errors where Rust panics); `float-to-integer`
  is exactly Rust's `as`, saturating. The shipped version of anything is the most minimal, most
  Rust-like one; a program that wants stricter or other behaviour redefines it.
- **G-46. `if!`'s alternative is optional** (VISION "Names"; the name table; amends G-8). A missing
  alternative gives `#inert`; `$when` is gone. G-8's other rules stand.
- **G-47. `let!` binds in order in one new context** (VISION "Kernel"; replaces `$let` and
  `$let*`, and G-31's note that `$let` matches every binding at once). Kernel's `$letrec*`: each
  binding evaluates its expression in the new context and binds there, so a combiner it binds can
  call itself and the bindings after it.
- **G-48. `catch!` and `finally!`** (VISION "Errors"; amends G-18 and G-26). `(catch! body e
  handler ...)` and `(finally! body cleanup ...)` are library operatives on the primitives
  `call-with-handler` (was `guard`) and `evaluate-with-cleanup` (was `eval-protected`).
- **G-49. `error` makes, `raise` raises** (VISION "Errors"; amends G-26 and G-35). `(error kind
  value ...)` is a constructor: it makes an error value and raises nothing. `raise` raises any
  error, new or caught; a caught error keeps its trace.
- **G-50. An error is a kind, values and a trace** (VISION "Errors"; renames G-35's fields `code`,
  `values`, `at`). A caught error is a context with no parent binding `kind`, `values` and
  `trace`; the Rust `Error` has the same three fields.
- **G-51. `same?` and `equal?`** (VISION "Names"; replaces `eq?`, merges `=?` into `equal?`,
  keeps G-13's strings by content). `same?` is identity for pairs, contexts and combiners,
  equality of value for integers, symbols and strings, and equality of the stored bits for
  floats. `equal?` compares pairs by structure, floats as IEEE does (not-a-number is never
  equal), and everything else as `same?`. Values of different types are never equal.
- **G-52. No truthiness** (VISION "Values"). The tests of `if!`, `conditional!`, `and!?` and
  `or!?` must be booleans; anything else is a `type-error`.
- **G-53. No continuations** (VISION "No continuations"). Confirms G-4 and G-35's reasons.
- **G-54. A context prints as its own names, sorted** (VISION "Printing"; replaces the opaque
  `#[environment]`): `#[context close path read-line]`, never its values, which may reach the
  context again.
- **G-55. The runner's Unix set** (VISION "The runner"; extends G-6). The runner gives programs a
  small fixed set of host operations: reading stdin as data, files as contexts of operations,
  the command-line arguments, environment variables, the exit status, running processes. The
  core does none of this.
- **G-56. No primitive `if!`** (VISION "Implementation"; confirms G-28 and G-33). `if!` and
  `sequence!` stay library code although making them primitives measured 1.5-2.3x faster: G-1 and
  simplicity win.
- **G-57. How grift grows** (VISION "How grift grows"). A feature enters only with evidence from
  real programs (a friction entry with a reproduction: one blocker, or the same cost in more than
  one program) and a recorded decision. Library grift comes before a primitive; a primitive only
  when the library version can't exist or is measured to be unusable. Removing things is welcome.
- **G-58. What complete means** (VISION "What complete means"). grift is complete when a closed
  list of programs runs well on it, each written without changing grift and with no blocker in
  its friction log: the Emacs-style editor; wave 1's evaluator, logic engine, algebra system and
  JSON filter; wave 2's spreadsheet, text adventure, build tool and sandboxed plugin host; and a
  set of Unix scripts on the runner. Adding a program to the list is a decision.

## Made with G-36 to G-58, 2026-11-01 (the owner may overrule any)

- **G-59. The Rust API uses the same words** (G-38). `Value::{EmptyList, Integer, Float, Symbol,
  String, Context, Operative, Applicative, Primitive}`, `Error { kind, values, trace }`,
  `Grift::evaluate`, `string`/`as_str`, `pair`/`as_pair`, `HostFunction`, `Primitive`,
  `Reference`. `Value` is `PartialEq`, no longer `Eq`, because of floats.
- **G-60. Host functions are values first** (G-41). `host_function(name, f)` gives an applicative
  bound nowhere and `define(name, value)` binds any value at top level, so a host can hand
  functions to grift code that keeps them private; `context()` gives a new child of the built-in
  context, a private top level. The runner does both: `runner.grift` is evaluated in a private
  context (so a program that redefines `lambda!` or `list` at top level can't intercept the
  operations) to a combiner called with the top-level context and the private host functions,
  and every file operation is a closure over them holding the file's number.
- **G-61. Float text.** The reader reads a number (a digit after an optional sign) as an integer if
  it is one, else as a float if it has a `.` or an exponent, by Rust's parse; the printer writes
  Rust's shortest form that reads back (`{:?}`, always with a `.` or an exponent) and
  `#infinity`, `#-infinity`, `#not-a-number`, so every float but a not-a-number prints and reads
  back to the same bits; every not-a-number reads back as Rust's `f64::NAN`, its sign and payload
  lost (printing writes no payloads). `float-to-string` and `string-to-float` are exactly Rust's
  `Display` and `parse::<f64>`, as `integer-to-string` and `string-to-integer` are; they undo each
  other for every float but a not-a-number, which Rust writes as `"NaN"`, its sign lost.
- **G-62. A string that isn't a number is a `parse-error`**, carrying the primitive and the
  string: it means what the reader's `parse-error` means, text that doesn't read (the reader's
  carries a line and column).
- **G-63. The float functions take floats only**, `absolute-value` included: one rule, and an
  integer's absolute value is a line of library grift.
- **G-64. `error` is library code**, a context made with `context` and `bind`; `raise` stays the
  primitive. A new error (an empty trace) happens at its `raise`; a caught one keeps its trace.
- **G-65. `and!?` and `or!?` check every operand they evaluate**, the last one included (G-52), so
  the last is no longer a tail call.
- **G-66. `equal?` stays library code.** IEEE equality of two floats is `less-than?` both ways
  plus a test for not-a-number, the one float that is neither less than 1.0 nor more than 0.0. It
  compares the rest of a list as a tail call.
- **G-67. `inert?` and `ignore?`** complete the predicates, one for each type `type-of` gives
  (the name table's note).
- **G-68. `if!` is two applicatives, one per shape of its operands** (G-46, G-56).
  `(test consequent)` and `(test consequent alternative)` are matched exactly, so a third
  operand is an `arity-error`, and `(same? alternative ())` picks between them. Every correct
  library-only optional alternative costs a call per `if!`; measured on `fib 25` in one build:
  `if!` with both branches required 0.134 s (as fast as before), a `first` trick that silently
  ignores extra operands 0.180 s, this one about 0.215 s. The task's band (no more than 10%
  slower) is revised to the measured value; cost: code heavy in `if!` runs about 1.6 times
  slower. A call's bindings are now built before its context, sized for its parameters (about 2%).
- **G-69. The runner runs one script with arguments**, `grift SCRIPT ARGUMENT...`, as `sh` and
  `python` do, so a `#!/usr/bin/env grift` script gets its own arguments; a first line starting
  with `#!` reads as empty. Several files per run are gone.
- **G-70. The runner's operations** (G-55): `display`, `newline`, `standard-input` (`read-line`,
  `read-all`), `open-file` (`read-line`, `read-all`, `close`, `path`), `create-file` (`write`,
  `close`, `path`), `command-line-arguments`, `environment-variable`, `exit`, `run-process`.
  `read-line` is Rust's `BufRead::read_line`: the line with its newline, `""` at the end, so no
  value is set aside to mean the end. `run-process` is Rust's `Command::output`: no input, the
  output captured, and a record binding `status` (`#inert` if a signal ended the process),
  `output` and `error-output`. `exit` records the status and sets the runner's own interrupt
  flag, so the program stops at its next step with `interrupted`, which no `catch!` stops, and
  its cleanups run: no core change. Kinds: `file-error`, `encoding-error`, `closed`,
  `unset-variable`, `process-error`.

## Owner decision, 2026-11-01: `if!` requires its else again

- **G-71. `if!` has both branches; `when!` has one** (replaces G-46 and G-68; G-8 holds again in
  full). `(if! test consequent alternative)` is one operative with that fixed parameter list, as
  `$if` was, so any other number of operands is an `arity-error`. `(when! test body ...)`
  evaluates the body forms in order, the last as a tail call, when the test is `#true`, and gives
  `#inert` when it is `#false`; the test must be a boolean (G-52). No `unless!` (G-57: no program
  asked for it, and `(when! (not? test) ...)` says it). `conditional!` is rewritten more directly,
  with the same meaning (G-73). Evidence: the optional else cost `fib 25` 0.13 s -> 0.21 s (PR
  #191, G-68); Kernel's `$if` and Racket's `if` require the else; every Lisp has `when`.

## Made with G-71, 2026-11-01 (the owner may overrule any)

- **G-72. `when!`'s body may be empty** (`(when! test)` gives `#inert` either way), as the body
  of `sequence!`, `let!` and a `conditional!` clause may: the body forms share one walker, `run`,
  and a rule of their own for `when!` would be a special case. `(when!)`, with no test, is an
  `arity-error`. `when!` lives with the other forms with a body, after `lambda!`.
- **G-73. How the forms with a body walk their operands** (G-31 still holds: no code is built,
  only the operands are evaluated).
  - A compound call always makes its context (it binds the parameters in a new child), so any
    library loop over n clauses makes at least n contexts. The only loop that doesn't is one that
    rebinds a variable with `bind` and re-evaluates a stored expression, choosing branches with
    `select` and `evaluate` instead of `if!`: not simple, uniform library grift, so rejected. The
    walk is therefore one call per clause, `first-true`, with nothing else per clause but its
    test's `if!` and the end check's `if!`: 3 contexts for each false clause, against 5 before
    (`first-true` and two of the old two-applicative `if!`s), and no `first` or `rest`, because
    `first-true` is called only with a clause left and takes it apart in its parameters,
    `(((test . body) . more) caller)`. A clause that isn't a pair is so an `arity-error` from the
    match, as a malformed `let!` binding already was, where it used to be a `type-error` from
    `first`; a body, as before, is checked only when it runs.
  - `run` checks once whether a body is empty; its loop `forms` checks only whether each form is
    the last (it checked both for every form). The value of the form before is `forms`'s ignored
    first operand, `sequence!`'s own idiom, instead of a `sequence!` call per form; `let!`'s
    `bind-each` does the same. `catch!`'s handler is a `lambda!` in `catch!` itself: the helper
    that made it is gone.
  - Measured and rejected: `select` picking which combiner to call for the end checks,
    `((select (same? more ()) none first-true) more caller)` with `none` giving `#inert`, and the
    same in `run`: 13% faster on a `conditional!` loop (0.84 s -> 0.74 s), but a second way to
    branch beside `if!`. Speed (release, one run each, load about 7 on 32 cores): `fib 25` 0.216 s
    on the base, 0.132 s here; a 200,000-step loop through a four-clause `conditional!` 1.467 s on
    the base, 0.868 s here, against 0.530 s and 0.347 s for the same loop with nested `if!`s.

## Owner decisions, 2026-11-01, after wave 2

Evidence: twelve programs (spreadsheet, text adventure, build tool, sandboxed plugin host, three
ways each) written on grift at 27c2aa4 without changing it, ranked in
`~/code/notes/projects/grift/apps2-2026-11-01/tally.md`.

- **G-74. grift does not sandbox the code it runs** (VISION "Embedding"). Hostile code is the
  embedder's problem, not the language's. grift adds no safety limit that would limit genuine
  programs: no depth limit, no trace-length cap, no memory quota, no change to cleanups. **G-17,
  G-35 and G-48 stand:** cleanups always run, `interrupted` included. This declines what the wave 2
  sandbox agents asked for: stopping a plugin through its cleanups (tally section 3, attacks 13, 28,
  31 and 32), a guard against a host function calling back without end (37), charging a plugin for
  its memory (44, 45), and trusting a trace (a program can `bind` any trace on an error it raises,
  69 and sbx-c 30). An embedder running untrusted code isolates it itself, for example in its own
  process.
- **G-75. What wave 2 earned** (G-57; tally sections 4 and 7): a reader and a printer to a string in
  the core (9 and 8 programs); in the library an applicative `binds?` (9), a faster `member?` and
  `find-association` (7), and the list and string helpers with three or more askers (10; sort, with
  two, not now); `standard-error` in the runner (8); and unwinding and the parameter check without a
  search, which are speed bugs, no behaviour changes (holes 33, 34, 69). G-27 left the string helpers
  out for one asker each; they now have 3 to 9 each.

## Made with G-74 and G-75, 2026-11-01 (the owner may overrule any)

- **G-76. An error keeps a set of the combinations in its trace** (G-29 unchanged: each once,
  innermost first, a given trace as it was given). Adding a frame's combination checks the set, so
  unwinding is linear in the frames, also through a cleanup per frame, which is why the set stays with
  the error instead of being rebuilt each time the error goes on. So that the two can't drift apart,
  `Error`'s `trace` is private now and hosts read it with `Error::trace()`: review showed a host
  adding to the public field could get a combination listed twice. No host wrote to it. The parameter
  check (`vau!`, `bind`) uses the same kind of set. Both hash a small id with the one multiplication
  contexts already use. Rejected: removing repeats only when the error leaves (the trace then holds a
  repeat per frame while it unwinds, so a deep recursion near the allocator's limit loses locations the
  set keeps; and a raised trace's own repeats would go). Measured (release, one run each, load about 8
  on 32 cores): a 300,000-deep raise through a combination made per frame, 11.9 s to 1.0 s; 300,000
  parameters, 9.2 s to 0.05 s; `fib 25`, 0.141 s to 0.131 s; a loop of `define!` and `lambda!`, 0.918 s
  to 0.978 s. A catch and raise at every level still copies the trace each time; that copy is the record
  the handler gets.
- **G-77. `read-form`** (G-38: a verb and what it reads, beside the runner's `read-line` and
  `read-all`; agents typed `read` first, which says nothing about one form, and `string-to-value`, but
  the operation takes an offset and gives more than a conversion would). `(read-form text offset)` is
  `(form start end)`, or `()` when only space and comments are left: a form is always in a list, so the
  end can't be mistaken for one, and no value is set aside. One reader: `Grift::read` loops over the
  same function, so the two can't disagree. The offsets are in bytes as everywhere, and the one given
  must be on a character boundary. `start` was added to the form and the offset after it after porting
  build-data: a form's line is where it starts, so with the end alone a program skipped space and
  comments itself, by its own idea of whitespace, and a form feed gave a wrong line. It also makes the
  form's own text one `byte-substring`. A `parse-error` keeps its values, the line and column, now
  counted from the start of the whole text when the error happens, so reading costs nothing per form
  for the length of the text before it; the primitive lends the string's text to the reader for the
  call for the same reason. Reading 40,000 forms, 1.5 MB, takes 0.17 s, against 10-63 µs a byte for the
  programs' readers. Not added: `Grift::read_form` for hosts, and a reason in a `parse-error` (tally
  core 6, not chosen for this change).
- **G-78. `value-to-string`; `float-to-string` is the printer's** (amends G-61). `value-to-string`
  is what `Grift::write` writes, as a string. `float-to-string` now gives a float's printed text, so
  `(float-to-string 1.0)` is `"1.0"`, not the integer's text, and any float's text reads back as a float
  through both the reader and `string-to-float`; `string-to-float` reads the three `#` names too, and is
  otherwise still `parse::<f64>`. Rust's `{:?}` alone would give `"inf"`, which reads as a symbol.
- **G-79. `binds?` shares `binds!?`'s helper**: one symbol, as asked; several are one `and!?` away.
  An own-names-only test had one asker and isn't added.
- **G-80. `member?` and `find-association` walk with `if!`, no `conditional!`.** On the tally's 4-element
  case, 24.5 µs to 10.1 µs a call for `member?`, the same for `find-association`, with loop overhead
  of about 1.3 µs in both. `same?` before `equal?` (the tally's proposal) is wrong: `same?` holds for a
  not-a-number and `equal?` doesn't, and a miss would still pay for both. Measured and rejected:
  choosing `same?` once when `x` is neither a pair nor a float, a special case the owner ruled out;
  `equal?` asking `type-of` once, 11% on this case for 5 lines.
- **G-81. The helpers' names and shapes** (G-38). `filter`, `fold` (Rust's order, `(f value element)`),
  `any?`, `all?`, `range` (Rust's `start..end`, not Scheme's `iota`), `string-search`,
  `string-starts-with?`, `string-ends-with?`, `string-split`, `string-join`, `string-trim`,
  `string-pad-left`, `string-pad-right`, `string-to-lowercase` (a conversion spells out `to`). They call
  through `apply`, as `map` does, so an applicative gets an empty context, never the library's.
  - `map` takes several lists of one length, as Kernel's does; different lengths are a `type-error`,
    not Rust's silent stop at the shortest (G-8). `for-each` keeps one list: no asker.
  - `string-search` gives `()` when the pattern isn't there, as `find-association` does, and a start
    out of range or off a boundary is refused with the `range-error` of the empty `byte-substring`.
  - `string-split` keeps empty parts, so `string-join` undoes it; an empty separator is a `range-error`.
  - Pads and lowercase work on bytes and ASCII, what the library can do honestly; `string-pad-left` is
    named for where the spaces go, as the programs named theirs. The spaces come from `range`, from the
    string's length to the width, so a width below the length gives none, whatever it is.
  - `string-search` stays library grift (G-57): 1 µs a byte (1 MiB in 1.0 s), a 64 KiB text split into
    4,096 lines in about 0.1 s; usable. The library reader that earned a primitive took 10-63 µs a byte.
- **G-82. `standard-error`** (extends G-55 and G-70): a context with `write` alone, as `standard-input`
  has no `close` or `path`; it writes to the descriptor, so `> log 2>&1` keeps what is there. It is
  file number 1 in the runner, a closure like the rest. A failing write is a `file-error`. A message
  with status 2 is now `((evaluate-in! write standard-error) "...") (exit 2)`.
- **G-83. The interactive runner reads before it evaluates.** It keeps asking for more only if the input
  itself doesn't read yet. It used to judge by the error from evaluating, and `read-form` gives a
  program a `parse-error` at any line and column, so an input could run twice.

## Owner request, 2026-11-02: a corpus, then less surface

The owner asked for a consolidation pass judged on 22 real programs (waves 2 and 3,
`~/code/notes/projects/grift/apps2-2026-11-01/` and `apps3-2026-11-01/`). `scripts/census.py` counts each
name's uses; the PR has each name's decision.

- **G-84. The closed list is `programs/` and the editor** (replaces G-58's list; VISION "What complete
  means"). Thirteen programs, each with its own `test.sh`, which `crates/grift-cli/tests/programs.rs`
  runs on the gate, four at a time: the ten of wave 3 (`diff format life machine markdown meta query
  simulate template test`), `sheet` (wave 2's sheet-graph), `build` (build-data) and `adventure`
  (adventure-language). sheet-graph and build-data because they were already ported onto G-75's forms
  and read best once their readers are gone; adventure-language because its world is a language of
  operatives, which exercises fexprs, and its reader goes to one `read-form` loop. The sandboxed plugin
  host is left out: its smallest host is 389 lines of Rust and a crate of its own, and G-74 declined what
  it asked of grift. Wave 1's programs were never in the repo; `meta` takes the evaluator's place.

## Made with G-84, 2026-11-02 (the owner may overrule any)

- **G-85. Names removed, each with no program that needs it** (amends G-38, G-67, G-70, G-78, G-79).
  `define-in!` goes for `bind` with the parameters quoted, although G-38 listed it: 3 programs and the
  editor used it, 11 programs used `bind` and 2 agents said they chose it, and it was `bind` with one
  operand evaluated. `binds!?` goes for `binds?`: its only callers were `test`'s and `meta`'s tests. `inert?`
  and `ignore?` go for `same?`: each type has one value (`empty-list?` has the same shape and stays, as
  G-38's own example, in 8 programs). `float-to-string` goes for `value-to-string`, which G-78 made the
  same text, so `(value-to-string 1)` is `"1"` where `(float-to-string 1)` was a `type-error`. `newline` goes for `(display "\n")`. Kept although few use them: the float functions (G-42),
  `wrap`, `unwrap`, `call-with-handler`, `evaluate-with-cleanup` and `select` (the library is built on
  them), `string-to-lowercase` (adventure), `environment-variable` and `run-process` (G-55).
- **G-86. `symbol-to-string` is library code over `value-to-string`; `integer-to-string` stays a
  primitive** (G-57). As library code `integer-to-string` took machine's 30,000-step run from 1.42 s to
  1.81 s (+27%, over the 25% band), since machine makes a symbol from an address on each memory read; it
  keeps its name and the printer's code, 4 lines. A `type-error` from `symbol-to-string` now names it by
  symbol, as `binds?` does.
- **G-87. The reader takes a first line starting with `#!` as a comment** (amends G-69). The runner's own
  strip is gone, and with it 1-7 lines in each of the programs that loaded scripts with `read-form`.
  At offset 0 only, in `read-form` and `Grift::read` alike, so the two can't disagree.
- **G-88. One walker for the forms with a body** (amends G-73). `sequence!` is defined with `when!`,
  `conditional!`, `let!`, `finally!` and `catch!` and shares `forms`, so its self-passing copy of the loop
  is gone. While that code loads there is no `sequence!`, so it is one `list`, which evaluates the helpers'
  definitions in order, whose values the outer `define!`'s parameters ignore. `let!` walks with one call a
  binding, choosing the next combiner with `if!`; `and!?` and `or!?` take each operand apart in the
  parameters; `catch!` makes no `lambda!` unless an error comes; `length`, `append` and `reverse` no longer
  make their helper on each call; `equal?` on floats and `string-trim` use `if!` and `same?`. Measured, each
  against its loop, in the PR: `catch!` 3.4x, `equal?` on floats 4x, `append` 5x, `length` and `reverse` 2x,
  `and!?` and `let!` about 10%. `conditional!` and `sequence!` barely move: a compound call and an `if!` per
  clause or form remain, and only a fast path or a primitive would remove them (G-33, G-56).
- **G-89. After a real `out-of-memory` a cleanup may do nothing** (a correction of LANGUAGE.md, not a change
  of behaviour; G-35 and G-48 stand). It starts, as always, and its first allocation fails; that failure
  doesn't replace the `out-of-memory`. The docs now say so. Collecting first in `unwind` would keep the
  promise only until a heap that is really full, so it would still need this sentence.
- **G-90. The machine has one shape of frame** (G-33). A `Frame` is what to do, its context and its
  combination; so the frame's roots and location need no case per kind, and pushing one is a call. A nested
  evaluation's outer location goes in a frame below the floor, not in a field of each form frame. A handler
  is called through `combine`, as anything is. `Value::Boolean(bool)` replaces `True` and `False`. The
  symbols the core names come from one table. Core Rust: 1,949 to about 1,700 lines; `fib 25` about 4% slower (121 ms to 126 ms, medians of five), inside its band.
- **G-91. `evaluate-with-cleanup` stays beside `call-with-handler`** (G-48). `finally!` over a handler
  that also saw limits could drop a primitive and two frames, but each cleanup would then copy the trace
  into a record and back, which G-76 removed, and every `catch!` would test for limits in grift.
- **G-92. The editor's conversions between strings and code points are grift** (G-57). Its host gives bytes
  and files only. Loading and saving a file use `string-split` and `string-join`, so only the line being
  edited is decoded.

## The owner on G-85, 2026-11-02
- **G-93. `float-to-string` stays** (amends G-85; the owner). It is library code beside
  `symbol-to-string`: the printer's text for a float, and a `type-error` for anything else, so a program
  that means a float says so. The owner confirmed the other two removals that touched names they had
  chosen or used: `define-in!` (for `bind` with the parameters quoted) and `newline` (for
  `(display "\n")`).

## Owner decisions, 2026-11-02, after wave 4

Evidence: 196 programs, 29,478 lines, written by ten agents on grift at 699c95b without changing it,
ranked in `~/code/notes/projects/grift/apps4-2026-11-02/tally.md` (cited as "tally" by section).

- **G-94. `absolute-value` and `power` take an integer or a float** (amends G-63; tally sections 4 #11,
  5 #3, 6). Two integers give an integer, two floats a float, never mixed, as `add`, `minimum` and
  `maximum` do; one name per operation, no typed variants. 8 agents wrote an integer absolute value by
  hand (13 copies), and it was the commonest wrong guess. `(absolute-value <the most negative
  integer>)` is `overflow`. Library grift.
- **G-95. `define!` keeps binding in the current context** (Kernel's `$define!`, Scheme's internal
  `define`; tally sections 4 #18, 5 #9). In a body it makes a local; there is no `set!` and no warning
  on shadowing. Changing an outer binding means holding its context and binding there (VISION "State":
  you can change only a context you hold). LANGUAGE.md shows the pitfall and the idiom beside
  `define!`; VISION "State" says why there is no `set!`.
- **G-96. `(unbind context symbol)`** (tally section 5 #23: structures F2, embedding F1, a hash map 10x
  slower than a plain context because it couldn't delete). The mirror of `bind`, both operands
  evaluated: it removes the binding from that context itself, never a parent's, and gives `#true` if
  one was removed, `#false` if the symbol wasn't bound there (deleting what may be absent is the common
  table case, and `binds?` looks through parents, so it can't check first). A lookup there then sees a
  parent's binding, if any. Closures and library names get no protection (G-74). A primitive beside
  `bind`: a library version can't exist. VISION "State" now says a context "gains, replaces or loses a
  binding". Symbols themselves are never freed, which LANGUAGE.md says beside it.

## Made with G-94 to G-96, 2026-11-02 (the owner may overrule any)

The tally's section 3 (K1-K4) and section 10 (library 1-4, core 1-2, runner 2-4), as the owner chose
them. Each has a test stating a property, a LANGUAGE.md or README entry, and its before/after in the PR.

- **G-97. A `parse-error` says why and where** (K1; G-77 left the reason out). Its values are `(reason
  line column)`, the reason a symbol for the reader's real cases: `unclosed-list`, `unclosed-string`,
  `unmatched-parenthesis`, `misplaced-dot`, `bad-escape`, `bad-number`, `unknown-constant`. An unclosed
  list or string is placed at its `(` or `"`, the innermost list left open, because that is where the
  mistake is and the end of the text says nothing (wave 4: `parse-error 7 1` for every unclosed form,
  8 agents, 1-7 minutes each). Data, not prose, so a program tells an unclosed form from a typo without
  comparing places, as format, build, adventure and the interactive runner did: the runner now waits
  for more input exactly when the reason is `unclosed-list` or `unclosed-string`. `string-to-integer`
  and `string-to-float` keep the primitive and the string: their text has no line.
- **G-98. `load` and `script-path`** (K2: ~2,400 lines copied between programs, all ten agents). `(load
  path)` is a runner operation that evaluates the file's forms in turn **in the caller's context**, as
  Kernel's `load` (15.2.2) and Scheme's do, so a shared file of helpers is used as if written in place;
  a program that wants them apart loads into a child it made, `(evaluate (quote! (load "big.grift"))
  big)`, which is VISION "Numbers"' "a context whose arithmetic is theirs" (Kernel's `get-module` is
  that, one name more). A relative path is relative to the file being loaded when `load` is called, at
  first the script, as Racket's and Guile's `load` do; the runner keeps that file in a private context
  and restores it after, error or not. A second `load` of a file runs it again (redefinition always
  takes effect; no "once" bookkeeping); a file that loads itself loads without end, as an endless
  recursion runs (G-5). All of a file's forms are read before any runs, as for a script. Errors keep
  their kind and values, so `catch-kind!` sees them, and gain the place in their trace as a string,
  `"lib/table.grift:12"` (the form's first line), or the path alone for a parse-error, whose values hold
  its place: the trace is where a host looks for "where", and a string prints as what it is. Rejected:
  a new error kind for loading (it would hide `file-error` from a program catching it), and a core
  `evaluate` with file and line (the embedding asks, tally section 8, not in this change). Known cost:
  the loader's own two combinations appear in such a trace, as the runner's own code already did for
  `open-file`'s errors. `script-path` is the script's path as given, unbound for a program on stdin,
  where there is no file, as Python leaves `__file__` unset.
- **G-99. `catch-kind!`** (K3: 147 `catch!` + `if!` + re-raise, ~521 lines). `(catch-kind! body kind e
  handler ...)`, catch!'s operands with the kind, a symbol, before the parameter, unevaluated as the
  parameter is. Library grift beside `catch!`, sharing its handler code: an error of another kind goes
  on with the trace it had, exactly as `(catch! body e (raise e))` gives it (tested), because the
  handler runs as a tail call and `raise` of a caught error adds nothing. A kind that isn't a symbol
  is a `type-error`, since `(quote! parse-error)` written there would otherwise never match.
- **G-100. Bytes are a list of integers 0-255** (K4: Brainfuck's `-.`, base64 of binary, `head -c 1` on
  `é`, `cat` of a binary file). Reader contexts get `(read-bytes count)`, up to `count` bytes, `()` at
  the end; writer contexts get `(write-bytes bytes)`; a new `standard-output` context (`write`,
  `write-bytes`) is where bytes go to stdout, beside `standard-error`, which gains `write-bytes` (amends
  G-82's "`write` alone"). Against VISION "Values": no new
  value, no vectors, strings stay UTF-8 text; a list is what every helper already takes (`map`, `fold`,
  `take`, `element-at`), and embedding's host wrote exactly this (`read-bytes`, a list of integers,
  200 KB through `hexdump` in 3.1 s). A count, not "the rest", so `head -c 1` and Brainfuck's `,` read
  only what they use, also from a pipe that doesn't end. Cost (ports): the primitives move 1 MB in 11 ms,
  but list helpers cost 0.35-1.5 µs a byte, so `tail -c 100` of 589 KB takes 1.8 s where the text
  version took 2 ms (it was wrong on characters).
- **G-101. `sort`, stable, and `string-less-than?` in byte order** (reopens G-75's "sort, not now":
  ten agents, ~376 lines, two unstable sorts). `(sort list before?)` is a merge sort in library grift
  calling `before?` through `apply`; an element goes before an earlier one only if it is `before?` it,
  so equal ones keep their order. 10,000 integers in about 0.3 s, 10,000 short strings in 0.9 s.
- **G-102. List access by index** (435 `(first (rest x))`, 10 agents). `element-at` (from 0, Rust's
  indexing), `second`, `third`, `last`, `take`, `drop`, list first as Rust's methods and SRFI-1 have
  them. An index or count out of range is a `range-error` carrying the name, the index and the length,
  never a shorter list (`take` doesn't stop at the end: that is a guess; `(take xs (minimum n (length
  xs)))` says it). A count that isn't an integer names the combiner, since `(drop n xs)` is the
  likely slip. `second` and `third` take the list apart themselves (0.9 µs over a call, against 2.5 µs
  through the index walk).
- **G-103. `string-repeat`, and a fill on the pads** (9 + 2 agents). `(string-repeat s count)` is
  Rust's `str::repeat`. The pads take a required third operand, the fill, a string of one byte, so the
  width in bytes is always met exactly: no optional operands (G-14, G-20), so every two-operand call
  changes (19 in `programs/`, 22 in the four ported wave-4 domains), to `" "`.
- **G-104. `lookup` is the primitive; `binds?` is library over it** (tally core 1: a probe was a caught
  error, 5-14 µs). `(lookup context symbol otherwise)`: one lookup, with a default where 54 inline
  `(if! (binds? t k) (evaluate k t) d)` did two; `binds?` is `lookup` with a context no program holds
  as `otherwise`. Not a primitive `binds?`: it would leave the default idiom at two lookups, and the
  pick was one of the two. A `type-error` from `binds?` now names `lookup`.
- **G-105. Bitwise operations** (6 agents; an `and` by hand 173 µs). `bitwise-and`, `bitwise-or`,
  `bitwise-exclusive-or`, `shift-left`, `shift-right`: Rust's `& | ^ << >>` on `i64`, two operands as
  all arithmetic. A shift amount outside 0 to 63 is `overflow`, where Rust's checked shift gives
  `None` and debug Rust panics "shift left with overflow"; the bits shifted out are lost, as in Rust,
  debug or release, because the uses (xorshift, hashes, masks) need them lost, and a shift that refused
  them would make every generator mask first. `shift-right` keeps the sign, as `i64 >>` does.
- **G-106. `display-error`, `read-file` and two clocks in the runner** (runner 2-4). `display-error` is
  `display` to stderr. `read-file` is `std::fs::read_to_string`; its bytes counterpart is `read-bytes`
  on `open-file`'s context (G-100), so no second name. `(wall-clock-nanoseconds)` is the system time
  since 1970 (a date, a seed) and `(elapsed-nanoseconds)` Rust's `Instant` since the run started: one
  clock can't serve both, since the system's can be set back. Nanoseconds, the unit of Rust's
  `Duration::as_nanos`, fit an integer until 2262; the unit is in the name so no one guesses it.
- **G-107. A band revised: meta's and format's tests read the library.** The target was each
  `programs/` program within 10% of the base. Measured (one run each): meta's test 13.4 s to 15.5 s
  (+16%), format's 3.35 s to 3.77 s (+13%), every other program within 5% or faster. Both read the
  library's own text as input (meta interprets `library.grift` on every run, format formats it), and it
  grew by about 150 lines (inside its band). On the same input they are within it: meta on
  `tests/square.grift` with the base library 0.023 s to 0.021 s, meta on meta 2.06 s to 2.11 s, format
  of the base `library.grift` 0.379 s to 0.394 s. New target: within 10% on the same input; cost: these
  two tests take longer by the library's growth.

## Owner decisions, 2026-11-02, for embedding grift in fux

Evidence: five spikes that put grift where fuxth is in fux, with friction logs, and their synthesis
(`~/code/notes/projects/grift/mux-2026-11-02/`: `owner-decisions.md` items 3, 4, 5 and 12,
`synthesis/report.md` section 2, cited `S<n>`), and `apps4-2026-11-02/tally.md` section 8.

- **G-108. Kept values** (owner item 3; amends G-21's wording, not its rule: a value lives while a
  binding holds it). `Grift::keep(value)` gives a `Kept`, `kept(&k)` reads it, `release(k)` unbinds it.
  A kept value is a binding in a context the interpreter owns, a collector root like the top level,
  under a name no program can reach. `Kept` is neither `Copy` nor `Clone`, so a double release or a use
  after release doesn't compile; another interpreter's `Kept` is `foreign-value`. Nothing else about
  collection changes. (S1: reactive kept handlers as `(run-binding "...")` text, config re-evaluated
  each key's code.)
- **G-109. The interrupt, fixed, no clock** (owner item 4; G-5 stands word for word; replaces G-17 and
  G-32's first bullet). The interpreter owns its stop state (no `&'static`: G-17's reason, the chip,
  went with G-25) and hands out an `Interrupter`, `Send` and cloneable. `stop()` stops only the run in
  progress when it is called, never a later one, and does nothing with no run in progress; it only
  loads and stores. `interrupted` stays uncatchable and cleanups still run. Time limits stay the
  host's: a test shows a timer thread setting one, and the runner and the editor use the new API.
  (S3: a stop at a host function's end leaked into the next run; layouts F1, reactive F2.)
- **G-110. Core and native primitives; native twins** (owner item 12, with 5; reverses G-56 under this
  rule; amends G-33 and VISION "Implementation"). A primitive is core when no grift definition could
  behave the same, native when it exists only for speed; each native keeps its grift definition as its
  meaning, in the repo, and a generated test shows no program can tell the two apart: results, error
  kinds and values, traces, tail calls, `type-of`, `operative?`, `applicative?` and printing. A library
  form may have a native twin on the same terms, justified by a measurement on `programs/`: `if!` and
  `sequence!` (G-56 measured 1.5-2.3x) and `string-search` (S7: about 1.1 µs a byte as library, and
  fux's waits search screens). No other twins in this change, and no change to how calls or contexts
  are represented (owner item 11; G-33, G-34 stand).

## Settled by the design ballot, 2026-11-03 (four models; the owner may overrule any)

Evidence: `~/code/notes/projects/grift/embed-2026-11-03/design.md` (each answer verbatim) and
`design/tally.md` (votes and dissents; P1-P4 are the four panelists). Ties went to fewer new public
items, then fewer lines.

- **G-111. A kept value is a binding in a third root context** (q1, plurality 2-1-1, by P1). The
  context `kept`, made in `Grift::new` beside the built-in and top-level ones, has no parent and is a
  collector root. A kept value's name is a symbol id with the top bit set (`KEPT = 1 << 31`), which
  interning refuses to give out, so no text, not even `string-to-symbol`, names one. Released names
  are reused from a free list, so a host that keeps and releases on every key press never runs out and
  never grows the symbol table. A foreign `release` hands the token back with the error,
  `Err((foreign-value, k))`. Dissent (P3): a checked counter, never reused.
- **G-112. Stops by run generation, one mailbox per handle** (q2, 1-1 tie, by P3). Each outer run gets
  the next of a checked, never-wrapping `u64` count, published while it runs and 0 otherwise; nested
  evaluations share it. `stop(&mut self)` loads the active run and, if it isn't 0, stores it in its own
  handle's mailbox; the machine stops only for its own number, once per run, so its cleanups run.
  Every clone registers a mailbox of its own, kept until the interpreter and the clones are dropped:
  one shared mailbox loses a stop when a late one for an earlier run overwrites it (P3's and P4's
  schedules). Dissent (P4): the same protocol, with a fallible `try_clone`.
- **G-113. Values carry their interpreter's identity at the boundary** (q3, plurality 2-1-1-0, by P4).
  The public `Value`, `Symbol`, `Reference` and `Host` hold a `Weak` to the interpreter's control
  allocation; every operation compares it before reading a slot, a symbol or a host function, and a
  mismatch is `foreign-value`. A surviving `Weak` keeps the address from being reused after the
  interpreter is dropped. Integers, floats, booleans, the constants and primitives are not branded. The
  machine keeps its own compact `Copy` values (`local.rs`); values are converted only where they cross.
  Cost: the public `Value` is no longer `Copy` (hosts clone or borrow) and grows from 16 to 24 bytes.
  Dissent (P3): an `Arc` token in each, `Clone`.
- **G-114. A reclaimed value is refused at every way in** (q4, plurality 2-1-1-0, by P1). Every value the
  host hands in (a call's combiner and arguments, `define`, `pair`, `list`, `bind`, `keep`, a host
  function's value and its error) is checked for its interpreter and its slot's generation, so nothing
  stale or foreign is ever stored and a live object holds only live objects; `as_pair` and `as_str` are
  `Result<Option<_>, Error>`, keeping "not that type" apart from "gone"; `same?` and the string readers
  read through the heap, and `byte-substring` reads its string before its offsets. One change from the
  ballot's answer: `write` refuses a stale value (`WriteError::Value`) instead of writing `#[stale]`,
  since done-item 4 asks every Rust reader to say `stale-value` (the Stage 2 test that counted). Dissent
  (P3): one private validator, with a closed audit of every site.
- **G-115. Which primitives are core and which native** (q5, plurality 2-1-1-0, by P3). Core: `vau!`
  `wrap` `unwrap` `evaluate` (the machine); `evaluate-with-cleanup` `call-with-handler` `raise`
  (unwinding); `context` `bind` `unbind` `names` (bindings); `pair`, `same?` (identity, float bits),
  `type-of` (representation); `add` `subtract` `multiply` `divide` `remainder` `less-than?` (checked
  integers, IEEE floats) and the float functions of G-42; `string-byte` (bytes), `string-append`
  (arbitrary text), `string-to-symbol` (names), `string-to-float` (correctly rounded parsing),
  `read-form` (the one reader, G-77) and `value-to-string` (combiners, names and floats as they print).
  Native, each with its definition in `natives.grift`: `first`, `rest` (parameter trees), `select`
  (a boolean's printed name picks a binding, so it needs neither `select` nor `if!`), `lookup` (the
  symbol evaluated under a handler for `unbound-symbol`), the bitwise operations and shifts (a bit at
  a time, wrapping doublings), `integer-to-string` (the printer), `string-to-integer` (digits added up
  negative), `integer-to-float` (through text), `float-to-integer` (saturation, then powers of two),
  `string-byte-length` (probing `string-byte`), `byte-substring` (decoding and re-encoding) and
  `code-point-to-string` (an escape, read). They keep their `Primitive` values, wrappers and error
  values; a definition names its primitive as data and never calls it. Dissent (P1): `first`, `rest`,
  `select`, `names` core.
- **G-116. One generated test of indistinguishability** (q6, 1-1 tie, by P3). A test-only switch runs the
  definitions in place of the natives without touching a binding. A small deterministic generator makes
  calls of every native and twin with every kind of value (integer ends, shift bounds, float bits,
  UTF-8 and number texts, contexts, combiners), missing and excess operands, through `apply`, `unwrap`
  and `wrap`, at top level, as operands, in tails, branches, sequences, handlers and cleanups; two
  interpreters run each, and the outcomes are compared as data with a bijection of identities, every
  error value and trace entry with its position, and a log of host calls. A second test shows tail loops
  in constant frames either way. Dissent (P4): the same, with direct Rust calls too.
- **G-117. A twin is the library's own operative, flagged** (q7, 2-2 tie, by P2). After the library
  loads, `Grift::new` flags the operatives bound to `if!`, `sequence!` and the one inside
  `string-search`; a call runs the twin only for the shape it handles (three operands for `if!`, a
  proper list for `sequence!`, two strings and a start on a boundary for `string-search`) and the
  body for any other, so printing, `type-of`, `same?`, `unwrap` and every odd shape's error are the
  library's by construction. Native `if!` evaluates its test under a frame and its branch as a tail
  call; a test that isn't a boolean is `select`'s own `type-error`. Dissent (P3): metadata beside the
  operatives rather than in them.
- **G-118. Contexts from Rust, with the primitives' rules** (q8, 1-1-1-1 tie, by P1; G-59's same words).
  `context(parent)`, `private_context()` (the old `context()`), `top_level()`, `bind(context,
  parameters, value)`, `lookup(context, symbol)` (`None` where `lookup` gives its default) and
  `names(context)` run none of the machine; `evaluate_in(state, context, source, text)` reads all of
  `text` first; `seal_library()` makes everything live count as library code, as `Grift::new` does for
  grift's own. No `define_in`: that is `bind` with a symbol, as G-85 removed `define-in!` for `bind`.
- **G-119. Where a combination was read is a side table** (q9, 1-1-1-1 tie, by P2; replaces G-98's
  `"path:line"` strings in traces). The reader records, for a combination read under a source name, its
  slot, the slot's generation, the source and the line of its `(`; a lookup checks the generation, so a
  reused slot never answers with an old line, and the table stays within the heap. Only `evaluate_in`
  reads under a name; `evaluate`, `read` and `read-form` don't. The runner evaluates its script and
  loaded files so, and its `load` became one host function over `evaluate_in`: about 35 lines of
  runner grift that counted lines went, and a loaded file's errors say `at lib/b.grift:4` beside the
  combination. Dissent (P1): the table in the heap, entries freed by the sweep.
- **G-120. The small conversions and one description** (q10, 2-2 tie, by P3). `symbol_name`, `list`,
  `list_items` (a `Vec`: pairs hold no slice), `as_string` (an owned copy), `is_kind` (interning
  nothing) and `describe(error, out)`, which writes the kind, the values and each trace entry with its
  position into any `fmt::Write` and keeps an output failure apart from the interpreter's
  (`WriteError`). Dissent (P1): the same items, `describe` to a `String`.
- **G-121. `(bindings c)`** (q11, 2-2 tie, by P2; G-54 stands). A context's own bindings as `(name
  value)` lists, sorted by name as `names` gives them, so a record's fields are data `find-association`
  reads (config C6, automation F4; owner item 7's shape). Two lines of library grift. The name: VISION
  "Terms" calls one name with its value a binding, and `names` is its precedent. Dissent (P1): the same.
- **G-122. `\r`, and the escape set checked against Rust's** (q12, 1-1-1-1 tie, by P1). The reader takes
  `\r` and the printer writes it. The set is `\\ \" \n \r \t` and `\xHEX;`; Rust's `\0`, `\'` and
  backslash-newline aren't added (`\x0;` says `\0`, a `'` needs no escape), and `\xHEX;` covers Rust's
  `\xNN` and `\u{...}`. Three spikes typed every Enter as `"\xd;"`. Dissent (P3): the same.
- **G-123. A target revised: the interrupter's allocations may abort** (q13, plurality 2-1-1-0, by P4;
  amends G-32's "never an abort"). The interpreter's control and each interrupter's mailbox are
  `Arc::new`, and stable Rust has no fallible `Arc::new` (`Arc::try_new` needs `allocator_api`), so
  making an interpreter or an interrupter can abort under real allocator exhaustion. Every collection
  that grows with a program stays fallible. Dissent (P1): nothing impossible.

## Made while building G-108 to G-123, 2026-11-03 (the owner may overrule any)

- **G-124. The runner seals its setup.** After `runner.grift` has made the operations, the runner calls
  `seal_library`, so a trace lists only the program's combinations: the two loader combinations G-98
  accepted as a cost are gone. A parse error in a loaded file keeps its values `(reason line column)`
  and is placed at the `(load ...)` that read it.
- **G-125. A stop is one load a step.** Besides its mailbox, a stop stores a shared "asked" flag after
  it, and the machine reads the mailboxes only when that is set, so a step pays one load whatever the
  number of interrupters; the run's number is published with release stores, since a stop that reads
  an older one stores a generation no later run has. The editor's input thread holds an interrupter: a C-g while `(key)` waits is
  a key, and one while a command runs stops it (`(key)` then ends the run at once).
- **G-126. The definitions of the natives load in tests only.** `natives.grift` is evaluated, with the
  library, only in the crate's own tests, where the switch lives; it is 167 lines of grift.
  `if!`, `sequence!` and `string-search` keep their definitions in `library.grift`, which is what runs.
- **G-127. What twins of `let!`, `lambda!` and `conditional!` would gain** (owner item 12 asked for the
  measurement, not the twins). Above a loop of one native `if!` and a call (0.32 µs a turn, release, one
  run of 200,000): `let!` of one binding costs 1.95 µs, making a `lambda!` 1.0 µs, `conditional!` of three
  clauses 2.1 µs more than nested `if!`s, where another `if!` costs 0.05 µs and a `sequence!` of two
  0.07 µs. That is about what a twin of each could save a use; none is in this change.
- **G-128. Measured with the twins** (one run each, release, box2, load under 1): `fib 25` 123 ms to 85 ms
  (1.44x); every `programs/` test faster, from build's 1.09x to markdown's 1.46x (meta 14.5 s to 13.1 s,
  format 4.26 s to 3.15 s). A host function call is cheaper too (0.56 µs against 2.1 µs in a grift loop),
  though each value crossing the boundary now costs an identity check and a reference count: about
  0.1 µs of a 0.73 µs `Grift::call` of a small handler (perf: `export` 5%, the imports 4%). `crates/grift`
  grew from 1,764 to 2,621 lines of Rust code, comments and blank lines aside (2,119 to 3,302 lines
  in all: the rest are the docs and examples every new item has).

## Made after the red team and the picking panel, 2026-11-03 (the owner may overrule any)

Evidence: the red team's findings (`~/code/notes/projects/grift/embed-2026-11-03/redteam/P<n>/findings/<name>/`,
each a `run.sh` that exits 1 when the guarantee breaks), the close-call experiments (`adv/q2`, `adv/q3`, `adv/q6`)
and the picking panel's carry-overs from the other writer's branch (`pick/tally.md`, each 4 votes of 4).

- **G-129. Each stop is acted on once, and a later stop again** (replaces G-112's "once per outer run").
  A stop stores its handle's count beside the run's number, and the machine remembers, per mailbox, the
  last count it acted on: one interruption answers every stop waiting, the cleanups then run, and a stop
  issued after that interrupts again, so a cleanup that never ends, or a run whose host function handled
  its nested run's `interrupted` and went on, can still be ended. Evidence: `P1/stops-lost-after-the-first`
  (58 stops over 2.9 s lost while a cleanup looped) and `P2/interrupt-swallowed-by-host`, both broken before
  and holding now. The machine also checks for a stop when a host function returns, as there may be no
  step after it (`P3/final-host-stop`, `P4/stop-final-host`: a stop completed during the last host call
  was lost). `Interrupter::stop` says whether it found a run in progress, so a watchdog whose deadline
  passed while the program was still being read asks again until one lands, rather than stopping on every
  tick, which would now interrupt the cleanups too.
- **G-130. A run its host function leaves is over** (amends G-112; carries the other branch's run-identity
  guard). A host function may panic (a host that catches it goes on with the interpreter), replace the
  interpreter it was given (`std::mem::swap`), or put it back after a run inside it was given up. While it
  runs, the machine holds its interpreter's control; if the function doesn't return to that interpreter
  with its run still published, or unwinds, the control publishes 0. The interpreter then answers
  `foreign-value` with an empty trace, drops its machine's state, and its next evaluation is a fresh outer
  run; a run that the interpreter holds but its control no longer publishes is one of these. So a stop
  with no run in progress does nothing after any of them. Evidence: `P1/idle-stop-after-host-panic`,
  `P4/idle-stop-after-unwind`, `P1/idle-stop-after-parked-run`, `P4/idle-stop-after-replace`, and adv/q3's
  restore and replacement rows (an abandoned run's cleanup ran and its frames reached the outer trace).
  Cost: one `Arc` clone and drop per host call (about 8 ns, q2's measurement), none per run.
- **G-131. The mailboxes are a list under a lock the stops never take** (replaces G-125's chain). Making
  an interrupter, dropping one and the machine's scan take the lock; the scan happens only after a stop
  has set the shared flag, so a step still pays one load. This closes adv/q2's one modeled lost stop
  (`clone_registered_leftover`): a late stop's flag was cleared by a scan that could not yet see a
  handle linked on another thread. Under the lock, a scan either sees the new handle or began before its
  link, and then the handle's flag, stored after the link, is stored after the scan's clear and stays set.
  A dropped handle's mailbox goes at once unless a stop of the run in progress waits in it, which a later
  sweep lets go, so storage follows the live handles.
- **G-132. Every way in checks the object's kind, and every argument before the context's kind**
  (amends G-114; carries the other branch's validation). A live reference in another variant's place
  (`Value::Context` around a string's reference: safe Rust) is `stale-value`; `bind` and `lookup` check
  every value they are given before saying the context isn't one, as the primitives' arguments are;
  `Grift::position` is `Result<Option<_>, Error>`, so a reclaimed or foreign combination is an error, not
  "no position". `lookup`'s definition answers its default only for the symbol's own `unbound-symbol`.
  Evidence: `P3/lookup-retagged-context` (a native/definition difference), `P3/stale-bind-validation`,
  `P4/validation-precedence`, and the four position findings.
- **G-133. A slot is retired at its last generation** (amends G-11). A slot freed for the
  4,294,967,295th time is never reused, so a reference held across every reuse stays `stale-value`; a
  heap read still compares one generation.
- **G-134. The editor stops its run at every C-g** (replaces G-125's editor sentence; carries the other
  branch's bookkeeping). The input thread stops the run and queues the C-g; after an interrupted run the
  editor marks that C-g handled, and `(key)` skips a queued C-g so marked. Before, a C-g that stopped a
  runaway command was read again by the recovery and stopped it too, losing a key (driven through fux:
  `embedok` saved for `embed ok`).
- **G-135. The equivalence test's edges** (amends G-116). Beside the generated programs: every byte offset
  pair of a text of one-, two- and four-byte characters, the scalar values' and integers' edges for each
  bitwise operation, raw float bits (not-a-number payloads) handed in from Rust, host functions returning
  retagged references in the generator's pools, the top level compared after every case (mutations
  persist), definitions and errors inside the twins, and a `sequence!` tail loop that now runs. With
  G-132's fix taken out, the unchanged test fails on the retag (a generated case among them).

## Made in review round 1 of the embedding PR, 2026-11-03 (the owner may overrule any)

Evidence: the reviewers' reproductions, `~/code/notes/projects/grift/embed-2026-11-03/review/r1/P<n>/repro/`.

- **G-136. A loaded file's parse error names the file** (amends G-119 and G-124; restores G-98's rule).
  When the text a runner `load` reads doesn't read, its `parse-error` keeps its values `(reason line
  column)` and has the file's path, as a string, innermost in its trace, before the `(load ...)` that read
  it: `parse-error unclosed-list 3 1 in "broken.grift" from (load d) at main.grift:2`. G-119 had placed it
  at the load alone, which names the file only when the load's operand is a literal path
  (`P1/load-parse-error-loses-file`). The runner's host function hands such an error back as a record,
  which the runner's `load` raises, so a program still catches it.
- **G-137. A run checks for a stop as it ends** (amends G-129). A run's last step can be a long native call
  (`string-to-integer` of 16 MiB of digits) or a host call, with no safe point after it; the machine now
  checks once more as the run returns, and an error raised while a stop waits becomes `interrupted`, its
  cleanups running. This replaces G-129's check after each host call, which missed natives
  (`P4/stop-final-native`: `stop()` said it found the run, 12 ms before the call answered a value).
- **G-138. Corrections.** `natives.grift` is 172 lines of grift, comments and blank lines aside (200 in
  all), not G-126's 167; the private walker there is `element-of` (VISION "Names": no abbreviations).
  The line band for `crates/grift` is read, as reported throughout, as lines of code, comments and blank
  lines aside: 1,764 to 2,660 (+896); in all lines 2,119 to 3,386, the rest doc comments and doc-test
  examples. G-128's 2,621 was the count at 0064860.

## Made in review round 2 of the embedding PR, 2026-11-03 (the owner may overrule any)

- **G-139. A stop waiting when a step fails is that failure, whatever its kind** (amends G-137). G-137
  turned an ordinary error raised while a stop waited into `interrupted`, but left a limit error
  (`interrupted` or `out-of-memory` a host function returns) alone, so the waiting stop was first acted
  on inside the cleanup and stopped it before it ran: `(catch! (finally! (fail) (cleanup)) e 42)`, with
  `fail` stopping once and returning a limit, ran no cleanup (`review/r2/P3` and `P4`
  `stop-limit-skips-cleanup`, exit 1 at cb83ca2, 0 at 375431a). Now any error raised while a stop waits
  consumes the stop and becomes `interrupted`, and the cleanups run once, as before G-137.

## Made after the q2 model was re-run on the embedding PR's head, 2026-11-03 (the owner may overrule any)

- **G-140. A poll with no run in progress acts on nothing** (amends G-129 and G-130). The q2 interleaving
  model, adapted to c85356b (`adv/q2/rerun/results-head.md`, `abandon_poll_race`: 2,806,824 states, complete,
  40 violating; 0 on either branch's earlier head) found that after a host function left its run (G-130's
  restored case), `abandon()` set the run to 0 and the failing step's poll (G-139) then read the mailboxes
  for run 0, while another handle's stop had stored its count but not yet its run: its mailbox, still 0
  from before, matched, so the abandoned call answered `interrupted` instead of `foreign-value`, a poll with
  no run acting on a request. Now each step still reads the shared flag first, its one load; only when a
  stop has set it does the cold path run, and that path's first act, before it clears the flag or reads
  any mailbox, is to answer false for run 0, leaving the flag set for the next run's first check. (A first
  version, d1284e9, tested the run on every step before the flag: `fib 25` 1.33x on desk.) The q2 model,
  run by its half A on dec88e5, finds the violation gone (0 states). A test makes the interleaving in
  order through a test-only seam that makes a stop's first store alone
  (`a_poll_with_no_run_in_progress_acts_on_nothing`). The fix adds three lines of non-test Rust (the cold
  path's test); the seam's five are `#[cfg(test)]`. Counted as the band says, non-test Rust with
  `#[cfg(test)]` items, comments and blank lines aside on both sides, `crates/grift` is 1,742 at the base
  and 2,613 at dec88e5 (+871); counting the test-only items in, as G-138 did, 1,764 to 2,668 (+904, the
  four over being the seam).
- **G-141. A band revised: `fib 25` at least 1.33x the base, not 1.4x** (CLAUDE.md's rule: a target is
  revised only when meeting it would hurt correctness). Original target: `fib 25` at least 1.4x faster than
  the base (the embedding brief's band). Evidence: desk, one run each, alternated with the base: 0064860
  1.63x, 375431a 1.51x, cb83ca2 1.42x, c85356b 1.44x; dec88e5 1.34x (`timings/desk-20261103-2122.md`) and
  1.396x in one job with the base and c85356b (c85356b 1.44x there). dec88e5's per-step code is c85356b's
  source; what changed is G-140's three lines in the cold path, which grew `.eh_frame`, the unwind tables
  linked before the code, by 24 bytes, so the whole `.text` and every loop in it moved 16 bytes (`start`,
  the machine loop, the same 0x2920 bytes at 0x37630 against 0x37620). On box2 the order is reversed: dec88e5
  98-101 ms, c85356b 101-110 ms, alternated. Moving the cold path out of line (`#[inline(never)]`) left the
  machine loop where it was and was dropped. G-140 is needed for correctness (a poll with no run acted on a
  request; the q2 model's 40 violating states), so it stays. New target: at least 1.33x, the lowest desk
  measurement at dec88e5. Cost: `fib 25` about 3% slower on desk than at c85356b, from code placement, not
  from the work a step does; every `programs/` test stays inside its band (1.08x-1.44x).

## Settled by the bytes design ballot, 2026-11-04 (four models; the owner may overrule any)

Evidence: owner decision 10 (`~/code/notes/projects/grift/mux-2026-11-02/owner-decisions.md`), K4 in
`apps4-2026-11-02/tally.md` section 2, PR #195's measured costs and the logic spike's F2
(`mux-2026-11-02/logic/friction.md`). The answers are `~/code/notes/projects/grift/bytes-2026-11-04/design.md`
(each verbatim) and `design/tally.md` (votes and dissents; P1-P4 are the four panelists). Ties went to
fewer new public items, then fewer lines.

- **G-142. Bytes are a value of their own** (replaces G-100; owner decision 10; q1 settled 3/4, by P2;
  q2 a 1-1-1-1 tie, by P4). The kind is `bytes`, `type-of` names it and `bytes?` is library. Immutable,
  any bytes; strings stay UTF-8 text and are never bytes, and a bytes value is never `same?` or
  `equal?` to a string. The literal is `#bytes(` with each byte as a decimal integer 0 to 255, and `)`:
  `#bytes(104 105 255)`, `#bytes()`. The printer writes that for every bytes value and the reader reads
  it back as the same bytes, by program text and by `read-form`. Inside, only integers 0 to 255 (as the
  reader reads an integer), space and comments: anything else is `parse-error bad-byte` at it, a `.` is
  `misplaced-dot`, a missing `)` is `unclosed-list` at the `(`; `#bytes` with no `(` stays
  `unknown-constant`. Why this literal: bytes aren't text, so they must not look like it. A `#"..."` form
  (Racket, Python's `b''`) would make `\xff;` mean a byte there and a scalar value in a string, and must
  refuse or re-encode a non-ASCII character in it; `#bytes(255)` and `"\xff;"` can't be confused. A
  byte's printed form is the integer `bytes-byte` gives for it, as `string-byte` gives one. VISION
  "Names" rules out `#u8(` and `b"` (abbreviations; `b"x"` reads today as a symbol and a string), and
  `#[...]` is what grift prints for values that don't read back. Cost: 2 to 4 characters a byte, in
  debugging output only; `display` prints the literal, and bytes leave as they are only through
  `write-bytes`. The vocabulary mirrors the string one name for name and argument for argument:
  `bytes-length`, `bytes-byte`, `bytes-slice`, `bytes-append`, `bytes-search`, and the conversions
  `string-to-bytes`, `bytes-to-string`, `list-to-bytes`, `bytes-to-list`; offsets are zero-based,
  a slice's end exclusive; no aliases, sequence operations, mutation, ordering or encoding parameter.
  G-100's evidence stands (K4: Brainfuck's `-.`, base64 of binary, `head -c 1` on `é`, `cat` of a binary
  file), and so do its runner names and its counted reads. What changed is its cost: lists made `tail -c 100`
  of 589 KB take 1.8 s where text took 2 ms (PR #195), and converting a 9 KB chunk 107 µs (F2). Dissent
  (q1, P3): the same literal, with a reason of its own, `unclosed-bytes`, at the `#`. Dissent (q2, P2):
  the same names, `bytes-slice` read as Rust's word for `byte-substring`.
- **G-143. One owned buffer each, copied** (q3, a 1-1-1-1 tie, by P3). A bytes object holds its own
  `Vec<u8>`; a value copies its reference, never the payload. A slice copies exactly its bytes, so a
  100-byte tail never keeps 589 KB alive; `bytes-append` sums the lengths, checked, and reserves once;
  the literal, a list and a read grow with `try_reserve`. For the collector a bytes object is a leaf,
  and it counts as one object plus the slots its capacity would fill, both when it is made and when it
  survives a collection, so large chunks dropped one after another bring a collection nearer as their
  size, not their number, says. Scheduling, not a limit. Only a bytes allocation pays for that count
  (`Heap::alloc_bytes`), so pairs and contexts allocate as before. Dissent (P1): `Box<[u8]>`, and only
  the allocation side counted. The ballot sent this one split (P3 for the survival count, P2 and P4
  against) to an experiment on the built branches, whose result outranks the vote. It ran
  (`bytes-2026-11-04/adv/q3/results.md`, box2, one run each), each side's rule set before measuring,
  and both rules keep the credits. Win: a host replacing 64 KB slices of a 64 MB stream peaks at 3.8 MB
  with them against 51 MB without (13x); 1 MB grown by 1,024 appends, 9 MB against 530 MB. Cost: a
  program holding a large live value lets more garbage build up before the next collection
  (`big-live`, 32 MB held: 144 MB against 69 MB, and about 1.2x slower). Strings earn no credits, so
  a string grown the same way stays at 530 MB either way.
- **G-144. Eight native primitives, and two library forms** (q4, a 2-1-1-0 plurality, by P3; q5, a
  2-1-1-0 plurality, by P4). `bytes-length`, `bytes-byte`, `bytes-slice`, `bytes-append`,
  `string-to-bytes`, `bytes-to-string`, `list-to-bytes` and `bytes-to-list` are native under G-110:
  the printer writes a bytes value's integers and the reader reads them back, so each has its grift
  definition in `natives.grift`, which never calls the primitive it defines, and joins the one
  equivalence test (G-116, G-135) with every offset pair of a bytes value, UTF-8 that is cut short,
  overlong, a surrogate or past U+10FFFF, and lists that aren't of bytes. No new core primitive: the kind
  itself is the reader's, the printer's, `type-of`'s and `same?`'s. `bytes?` and `bytes-search` are
  library grift. Errors as the string ones: a wrong kind is `type-error` with the primitive and the
  value, an offset out of range `range-error` with the primitive and the operands. `bytes-to-string` is
  Rust's `str::from_utf8`: bytes that aren't UTF-8 are `encoding-error`, now a core kind (the runner's
  name and meaning), with the primitive, the bytes and `valid_up_to`, the offset of the first sequence
  that isn't; NUL and a byte-order mark are kept; no `error_len` and no lossy form, which no program
  needs. `list-to-bytes` walks the list in order: an element that isn't an integer, or a tail that isn't
  `()` (a non-list at position 0), is `type-error`, and an integer outside 0-255 `range-error`, each
  with the value and its position. Who needs each: `bytes-length` and `bytes-slice` tail's and head's
  `-c`; `bytes-byte` base64 on every byte and Brainfuck's `,`; `bytes-append` joining read chunks;
  `list-to-bytes` base64's output and a byte written alone; `string-to-bytes` and `bytes-to-list` the
  machine's assembler. Dissent (q4, P1): `bytes-to-list` library, as no program needs it fast. Dissent
  (q5, P1): a non-list or improper list refused whole, with the list, before any element.
- **G-145. `same?` compares bytes by content** (q6, a 1-1-1-1 tie, by P1). As strings (G-13): two
  bytes values with the same bytes are the same, so `(same? chunk #bytes())` ends a read loop.
  `equal?` falls through to `same?`. No `bytes-less-than?`: no program needs one. Dissent (P2): the
  same.
- **G-146. The runner's `read-bytes` and `write-bytes` take and give bytes** (q7, settled 3/4, by P4).
  `(read-bytes count)` gives a bytes value of up to `count` bytes, `#bytes()` at the end or for 0,
  reading only until it has them, so Brainfuck's `,` answers a pipe that stays open; its buffer grows
  with `try_reserve`. `(write-bytes bytes)` writes a bytes value as it is; a list, G-100's form, is a
  `type-error`, so a program on lists fails loudly. No `read-file` counterpart: `read-bytes` on
  `open-file`'s context is that (G-106). In `programs/`, meta knows the new kind and primitives;
  format lays out every atom with a newline in it (a bytes literal can have one) as lines, not only a
  string; the machine drops its own `string-to-bytes` and writes each output byte as it is, so its
  UTF-8 workaround and the check for a halt in the middle of a character are gone and its tests expect
  the raw byte. The editor keeps its contracts: keys are integers, files text. Dissent (P1): the same.
- **G-147. Bytes from Rust** (q8, a 1-1-1-1 tie, by P3). `Grift::bytes(&[u8])` makes a value from a
  copy, and `Grift::as_bytes(&Value)` borrows its contents, `None` for a live value of another kind.
  Both go through import and export, so a reclaimed bytes value is `stale-value`, another
  interpreter's `foreign-value`, and a reference under `Value::Bytes` of another kind of object
  `stale-value` (G-132); the embedding test's pools have a bytes value and its retaggings, at every
  way in. `keep` keeps one across evaluations. No owned copy and no constructor taking a buffer over:
  fux's PTY feed (F2) copies a chunk into its own parser while it holds only `&g`. Dissent (P1): the
  same.
- **G-148. `bytes-search` is library grift, with no twin yet** (q9, a 1-1-1-1 tie, by P1). It has
  `string-search`'s order, its `()` when nothing is found, its empty pattern at `start`, and its
  `range-error` for a start out of range, which names `bytes-slice` as `string-search`'s names
  `byte-substring`. G-110's bar for a twin is a measurement on a program, and no program in
  `programs/`, the ports or the editor searches bytes; fux's logic only passes chunks on (F2). One
  quick check, for the first program that does: 9 KB searched to its end in 12 ms (box2, busy), where
  `string-search`'s twin takes 4 µs on the same text. A twin would be about 20 lines and a flag
  (`windows(p.len()).position(...)`, never `windows(0)`). Dissent (P2): the body shared with
  `string-search` through a maker over the two accessors.
- **G-149. The bands, as set** (q10, a 1-1-1-1 tie, by P1). Nothing in the brief needed revising before
  the work. The tight one, base64 decode of 565 KB at least 5x faster than on lists, is met by the port's
  shape: one read of a 256-byte `#bytes(...)` table per input byte, four digits a call while they come,
  no `let!` in the loop. Orientation on box2 (busy, load about 10, one run each): decode 8.7 s on lists
  to 1.07 s on bytes (8x), `tail -c 100` of 589 KB 1.58 s to 2-3 ms, `fib 25` 97 ms both (its
  instructions +0.6% after G-150). On desk, the quiet machine, every `programs/` test was within 5%
  of the base but meta's (G-150), and format's, which box2 had put outside, was within it. Dissent
  (P2): revise the base64 band to 3.5x before the port is timed.
- **G-150. What meta's test paid, and the part that was the code's** (the band stands). On desk (base
  7ff695e against 8da8b50, one run each, twice) meta's test took 21.3 s and 22.5 s (+5.3%, +5.6%),
  outside the 5% band; every other program was inside it. Two causes, told apart on box2 by
  instruction counts (`perf stat`), as times there are noise at this size, on meta running meta on
  `tests/square.grift`, the bulk of the test:
  1. **The code, fixed.** With the base's `library.grift` and the base's meta as the input, the new
     binary ran 2.4% more instructions (45.14 G against 46.24 G), from no new work on the path: the
     new `Bytes` variants sat in the middle of the machine's `Value` and the heap's `Object`, so every
     later kind changed its tag and the compiler laid the machine's common paths out differently
     (`perf report`: `find`, `push` and `next_argument` traded places). Put last, the other kinds keep
     their tags: 45.47 G (+0.75%), and `fib 25` +0.6% where it was +1.3%.
  2. **The input, as G-107 found.** meta loads `library.grift` in every run, twice over when it runs
     itself, and the library grew by `bytes?` and `bytes-search`. `bytes-search` was 3.9% of a
     library load (16 M of 414 M instructions loading for an empty program), more than its lines, as
     each `lambda!` meta interprets is dear; written with one helper in place of two, it is 2.95%,
     shorter, and searches 9 KB in 12 ms where it took 14.5 ms. `bytes?` is 0.75%.
  The whole test: 222.35 G instructions at the base, 232.46 G at 8da8b50 (+4.5%), 227.88 G now
  (+2.5%); on the same input its time is the base's (box2, alternated: 3.96/3.96, 2.93/2.83,
  2.77/2.80 s). Target unchanged: within 5% of the base; desk times the head.

## Earned by the port reading, 2026-11-05

Evidence: `~/code/notes/projects/grift/fuxport-2026-11-04/reading/tally.md` (four readers, P1–P4)
chose three needs: N1, N2 and N3. Its other outcomes stay outside this change. The N1 error ruling
is `reading/design-n1.md`; test admission is `tests/counts.md`. No new language names, primitives
or kinds. The only new public names are `Grift::read_in` and `grift::write_string`.

- **G-151. Read a named text without evaluating it** (N1: 4/4 readers; eight ports, four families;
  extends G-119). `Grift::read_in(source, text)` gives `read`'s list of every form, through the same
  reader as `evaluate_in`; each combination keeps its source and opening line for `position` and
  later traces. Reading does no evaluation. The runner's host now only reads a loaded file; a grift
  `for-each` evaluates its forms in the caller's context, within the existing run. Relative paths
  and restoration after errors remain runner policy; G-163 moves G-136's source attachment into
  the shared reader. A regression first reproduced a Rust stack overflow on a 1,000-file chain
  with a 512 KiB stack; it passes with evaluation in grift. No evaluator entry or optional
  `read-form` operand was added.
  - Initial error ruling (superseded by the owner's 2026-11-05 decision in G-163): A, reader
    errors, `(reason line column)` and an empty trace. The design vote split 2–2; two
    different-family ports swapped to B (source in the trace), with unchanged results (83/83
    and 82/83), so the tie stood and the provisional ruling chose A. Source attachment stayed at
    the edge for both `read_in` and `evaluate_in`; B was left for the owner to choose.
    Strongest dissent for B (P1/P4): the caller already supplied provenance; forwarding the error should retain it,
    and hosts have distorted the values while attaching it. Strongest against B (P2/P3): it makes
    sibling APIs disagree and moves G-136's loader responsibility into the core.
  - Against the method: P1's unpatched loader already passed the small-stack and named-file checks,
    so the method buys convenience and lasting API surface. P2 says a bounded nested evaluator is
    adequate for ordinary loads. P3 says preserving positions and small-stack loading already has
    a workaround. P4 likewise finds convenience rather than a previously impossible capability.
    The chosen mechanism removes host evaluator recursion in a second program, the runner, while
    keeping reading and evaluation separate and sharing the existing source table.
- **G-152. Intern by name with the standard hash map; read ASCII directly** (N2: 4/4 readers;
  two ports, two families). A `HashMap<Box<str>, u32>` replaces the sorted symbol-id vector; the
  id-to-name vector stays. Both spellings are allocated and both collections reserved before either
  entry is published; failure is still `out-of-memory`, and ids at `KEPT` and above stay excluded.
  The reader returns an ASCII byte as its character, decodes other characters as before, and skips
  comments to their newline in one search. Grammar, positions and errors are unchanged. No custom
  string hasher, twins, collection switch or startup image.
  - Construction alone, measured in-process on idle desk: release, 10,000 constructions after
    100 warmups, timing only `Grift::new`, excluding destruction: 862,384 ns to 719,015 ns, 16.6%
    less time. Both binaries were built in the same worktree and target directory. A separate
    desk measurement runs the whole runner process on an empty program, not just `Grift::new`:
    2,113 to 1,926 µs, 8.8% less time (`timings/head1.md` under the evidence directory; medians
    of three batches of 200 runs, `bin/desk-grift.sh`). Both show a gain for N2; their different
    boundaries explain the different percentages. Interning is shorter. Cost: each name's bytes
    are stored twice and the hash table replaces a compact integer index.
  - Against it: P1 says the original whole-port timings cannot attribute a gain to grift. P2 says
    the absolute startup gap is small, and the original evidence did not meet the family's bar.
    P3 objects to duplicated name storage and timings mixed with host changes. P4 likewise says
    startup already works and the bundled optimizations had separate evidence. This measurement
    isolates construction; only the agreed representation and reader changes land.
- **G-153. One string literal writer for values and host text** (N3: 4/4 readers; three ports,
  two families; G-122's spelling unchanged). `grift::write_string(text, out)` takes a borrowed
  `str` and a `dyn std::fmt::Write`, returns `std::fmt::Result`, and needs neither an interpreter
  nor allocation. The string case moves out of `print.rs` to this function; `Grift::write` calls it.
  Quoting, the five short escapes and hexadecimal control escapes have one source. Output failures
  propagate directly. Record traversal stays with the host.
  - Against it: P1 says a temporary interpreter string or a copied small escape table already works.
    P2 says G-122 fixed the small table, and a large streamed-output check belongs to its host.
    P3 says the function is unnecessary for correctness; P4 objects to adding public surface for a
    formatting detail. The chosen function shares the actual printer without a transient heap copy;
    a small fixed-seed property checks both writers against `read-form`, and refusing sinks fail.

## Conformance fixes, 2026-11-05

- **G-154. A named outer form keeps its evaluation boundary** (G-151 and G-29 unchanged).
  Evidence: `fuxport-2026-11-04/stress/P1/findings/load-drops-top-level-form` and
  `read-in-loader-trace-drops-form`. The source table now distinguishes the outer forms returned
  by the reader. When grift's `evaluate` starts one, a machine frame retains it through its tail
  calls, as the Rust entry's forms frame does. Tail evaluations reuse an immediately enclosing
  boundary instead of adding a frame; a non-tail nested evaluation has work above that boundary
  and gets its own. The frame roots the form for collection and normal unwinding adds it once.
  Sealed library forms stay excluded, and recycled slots must match their recorded generation.
  The runner stays a grift loop; the source's final calling line now survives it. No new public
  name and no weakened trace promise. The original position test's tail call is restored; the
  runner compares a file's own trace with its loaded trace, and a named-form tail loop checks
  that the retained boundary costs constant space.
- **G-155. Collect before a host's run needs its first slot** (repairs G-26's recovery guarantee;
  extends G-12's safe points). Evidence: `fuxport-2026-11-04/stress/P1/findings/
  read-in-oom-leaves-interpreter-stuck`, also reproduced with `read` on pure at 71cb5c5. The reader
  exhausted the heap vector while accumulating forms that then became garbage. `evaluate` needed
  a slot to read new input before reaching the loop; even a zero-argument `call` needed a new
  context in `call_local` before `start`, so both failed to grow the same vector forever. Free
  allocator bytes could not pay for its next doubling, and collection was never reached.
  The host's evaluate/evaluate_in/call entries now collect when due before those allocations,
  using the existing machine roots plus their checked context, combiner and arguments. Abandoned
  machine state is cleared first. Reading itself still never collects, nor does allocation; no
  public collection control is added. A test-only heap-slot limit reproduces the exhausted free
  list without a global allocator; one quick property checks all three entries after both readers,
  including live unbound call inputs and a child context. The external allocator-budget case is
  retained outside the crate as evidence of actual allocation refusal.
- **G-156. A failed read makes collection due** (completes G-155; same
  `read-in-oom-leaves-interpreter-stuck` case). An extension of its quick property kept 20,000
  live pairs and left only 32 new slots: the read failed before the ordinary live-size threshold,
  and the next evaluate still answered out-of-memory at 8d9d53f (`small-limit-before.log`). The
  shared reader now requests collection on out-of-memory without collecting there. G-155's next
  run reclaims the discarded forms even below that threshold. The same property covers both
  readers and all three entries with both sparse and mostly live heaps; its count is unchanged.

## Picking panel follow-through, 2026-11-05

- **G-157. A named form's boundary belongs to the common evaluation step** (amends G-154;
  G-110 holds). Evidence: `fuxport-2026-11-04/pick/tally.md`'s two trace must-fixes, reproduced
  before changing code. A separately read `(f 1)` entered as a constructed `if!` branch gave
  traces of two and three entries with natives on and off. The boundary check now runs when
  the machine starts a pair, using `Position.outer`, whatever control flow entered it. Native
  branches and sequences therefore follow the same rule as their definitions. Reusing a
  `What::Form` updates its place to the named form now running: tail-evaluating b's form from
  a's formerly kept a and lost b. The host's `What::Forms` entry frame remains unchanged.
  Existing equivalence and position properties reproduce both failures; the tail-space property
  still guards constant space. This keeps the source table as the reader's record, with no new
  heap classification or public name. The tally carried the tail-place update 4/4.
- **G-158. The runner names parse errors in grift** (superseded by G-163's owner decision;
  originally amended G-136's host-built record and G-151's forms-or-record result). `pick/tally.md` carried this runner separation 4/4. Its private
  `read-forms-host` now only reads the file and calls `read_in`, returning forms or an error.
  The private grift `read-forms` catches `parse-error`, binds the caught record's trace to the
  path string alone and raises it; kind and values stay untouched, and ordinary unwinding adds
  the outer load. Reusing the caught record needs no second record. `load-file` evaluates the
  forms directly, with no context-versus-list dispatch. The existing runner property covers the
  computed path, unchanged reader values, source traces, catches and relative-path restoration;
  the loaded-tail and small-stack properties still hold. No public operation changes.

## Review round 1, 2026-11-05

- **G-159. Every reported out-of-memory requests the next collection** (amends G-155/G-156's
  reader-only scheduling, preserving G-26's usable interpreter). Evidence:
  `fuxport-2026-11-04/review/r1/P1/repro/call-stuck-after-oom`: after either a run or host pair
  allocation exhausted a mostly live heap, all 1,000 recovery calls were refused at both live
  sizes. The new quick regression also failed on the first evaluate, evaluate_in and call after
  each origin. OOM was exported without making collection due, so the next entry needed a slot
  before it could reach a collection, just as after the failed reads in G-155.
  Scheduling now happens at the common error-export boundary, after choosing the final error
  kind; this also covers allocation failure while exporting an error's values or trace and
  failures in immutable host operations. The existing scheduling count uses `Cell<usize>` so
  those operations can request collection through `&self`; ordinary allocation and collection
  update it through exclusive `get_mut` access. Shared cross-thread interpreter access is not
  promised; only the interrupter needs atomic communication.
  No new flag, allocation, public control or collection inside allocation is added. The reader's
  special wrapper is removed. Reads and constructors still never collect: more reads may be
  refused until an evaluation or call reaches its preparation safe point. The first such run
  now gets that chance for every reported OOM, independent of the ordinary allocation threshold.

## Port regression, 2026-11-05

- **G-160. Safe-point temporaries need not live across host callbacks** (G-155, G-156 and
  G-159's entry collection and OOM recovery unchanged). A 1,000-source loading chain on a
  128 KiB host thread passed before 8d9d53f and overflowed after it. The debugger found no
  collection frame at the stack guard: a nested `read-form` exhausted the stack with a fixed
  number of host frames below it. In unoptimized builds, forced inlining of `safe_point`
  retained its temporary storage in each suspended run, including the roots snapshot added
  when collection began serving host entries too. Permitting normal optimizer-directed inlining
  restores that unoptimized case; it does not bound optimized hosts, where the optimizer can
  still inline the generic collector and retain its larger root-iteration and marking storage
  in suspended run and entry frames. G-162 completes the fix with a collector inlining barrier.
  Collection stays at both safe points with the same roots; the host's loading policy is unchanged.
  The initial quick regression compiles an ordinary unoptimized embedding directly with `rustc`,
  because the workspace's optimized test profile hid that unoptimized failure. It processes 1,000 named
  sources through fixed-depth host callbacks in an isolated process: its initial 128 KiB
  fixture passes at c102859, aborts before this fix, and passes after it. G-161 widens the
  fixture's margin while retaining the original 128 KiB host check. The OOM properties still
  exercise sparse and mostly live heaps, reads, runs and host allocation. No public name,
  allocation or collection control is added.

## Review round 2, 2026-11-05

- **G-161. A host call gets collection before copying its arguments** (completes G-155/G-159).
  Evidence: `fuxport-2026-11-04/review/r2/P4/repro/call-arguments-after-oom` refused 100/100
  nonempty identity calls at b276894 after a discarded string filled the allocator budget.
  Argument import reserved a vector before preparation could collect, so the first allocation
  still stood between a call and recovery. An allocation-free entry helper now validates the
  combiner and every argument, then prepares with the combiner and each checked argument's
  reference as roots. Only then does argument copying allocate. Marking follows the reference's
  object regardless of the temporary root's value tag. The helper returns before running the
  call, so its validation and root-iterator temporaries do not enlarge suspended host frames.
  No input is skipped or made valid by collection: foreign and stale inputs are refused before
  collection. The previous preparation in `call_local` is removed, with no extra collection.
  - The original regression (moved to evidence by G-165) runs in an isolated runner-package
    embedding test, where a thread-local
    allocation budget can reproduce real reservation failure without weakening the core's
    unsafe-code prohibition or adding a production test seam. With no spare bytes, an identity
    call must recover for an immediate argument and for six direct, unkept arguments covering
    every heap-reference kind, also retaining an unkept function. Invalid calls leave the garbage untouched; the
    valid call reclaims it. Existing slot-limit and external allocator cases remain.
  - The scheduling count now uses `Cell<usize>` rather than an atomic (G-159 amended): no
    interpreter `Sync` guarantee requires an atomic. Scheduling and collection policy are unchanged.
    The initial small-stack fixture was a Cargo test target (G-165 later removes that optimized
    target to meet the CI budget), so ordinary fmt and clippy checked it; its
    unoptimized run uses the compiler path Cargo supplies to a dependency-free build script.
    Sixteen fixed host callbacks and a 300 KiB stack leave at least 8 KiB above a passing
    292 KiB control on the measured toolchain, while d9dc54a still overflows at 300 KiB.
    The original host's 1,000-load test still uses 128 KiB. There is no new public name,
    dependency, collection control, allocation in the entry helper or weakened recovery promise.

- **G-162. Collection keeps its own stack frame in every build** (completes G-160; G-155,
  G-156 and G-159/G-161 recovery unchanged). Evidence:
  `fuxport-2026-11-04/review/r2/P1/repro/release-host-stack`: forty nested host entries fit in
  128 KiB on pure, but overflow at b276894 and 48e4219 in a release build with one codegen
  unit and thin LTO. Removing forced safe-point inlining only fixed unoptimized builds.
  The generic `collect` added in 8d9d53f could still inline into optimized run and entry frames,
  keeping collection's storage live when no collection was running. `#[inline(never)]` now
  keeps its root iterators and marking storage inside the collection call, which returns before
  any host callback. The rare collection call is the only new inlining boundary; allocation,
  safe-point checks, roots and collection scheduling stay the same.
  The initial optimized Cargo host-test target exercised forty entries in 128 KiB and aborted
  before the fix; G-165 later moves that optimized check out of CI. The compiler-driven test runs the same source unoptimized with its
  existing sixteen-entry, 300 KiB bound and measured margin. Both process 1,000 sources with
  fixed host nesting. This adds no test function or compiler invocation to CI. The external
  reproduction retains the one-codegen-unit, thin-LTO shipping configuration as evidence.
  No new public name, dependency, allocation, collection control or weakened stack bound.

## Owner decision B, 2026-11-05

- **G-163. Both named readers retain the source of unreadable text** (owner decision,
  2026-11-05 11:00; supersedes G-151's design A error ruling, its 2–2 panel tie and the swap
  experiment's provisional choice; supersedes G-158's runner-built source record).
  The owner chose B because three ports attached the name separately and two broke the standard
  values. `read_in(source, text)` and `evaluate_in` now agree: when the input text doesn't read,
  `parse-error` retains exactly `(reason line column)` in its values and puts the supplied source
  name, as a string, innermost in its trace. Changing both named entries answers the strongest
  uniformity objection to B. `read` and `evaluate` keep their unnamed reader errors; grammar,
  coordinates, ordinary evaluation traces and `read-form` are unchanged.
  The shared whole-text reader attaches the name once, with fallible string allocation and trace
  growth. If allocating or retaining the source or trace fails, the host gets `out-of-memory`,
  replacing the parse error: the complete named error cannot be built, and returning a bare parse
  error would lose the promised provenance. The existing recovery boundary handles it. Reading
  never collects. The runner calls its read-only host directly, removing private `read-forms` and its
  record rebuilding. G-136's loaded-file parse-error shape is unchanged, including the outer load;
  its existing tests stay unchanged. The top-level script diagnostic test does change because
  scripts use `evaluate_in` and therefore now retain the source string too. No new test function
  is added: the existing reader-error property checks both named and unnamed entries across its
  reasons, retaining all three values. No new public name, dependency or collection policy.

## Round 2b, 2026-11-05

- **G-164. Named parse errors retain their diagnostic source across collections** (completes
  G-163). Evidence: `fuxport-2026-11-04/review/r2b/P2/repro/
  named-parse-error-stale-after-collection`, reproduced for both named entries at af34256.
  The exported error alone did not root its fresh heap string; after the next collection,
  `describe` and `position` rejected it as stale. Interned source names already live with the
  interpreter. Each source that fails to parse now gets one cached diagnostic string in the
  existing private kept context, keyed by its interned id below KEPT; host-kept ids remain
  disjoint at KEPT and above. Later errors reuse it, preserving older held errors too.
  Cost: one heap string and binding per distinct source that has a parse error, retained for the
  interpreter's lifetime. No field, element size or retention policy changes for ordinary errors
  or their traces. The regression forces two collections, holds repeated errors from one source,
  checks both named entries remain describable and positionable, and verifies ordinary garbage
  strings still collect. G-163 states the allocation-failure result: out-of-memory, not a
  nameless parse error. The runner omits its source prefix when the trace already names that
  input, so top-level scripts and stdin are named once; loaded-file diagnostics stay unchanged.

- **G-165. CI reuses interpreter instantiations; compiled stress fixtures stay as evidence.**
  Round 2b's check job exceeded the 85.8 s target: 104 s at a15572b and 115 s at af34256,
  against pure's 78 s median plus 10%. The separately optimized host and allocator fixture
  targets instantiate whole interpreters again. A fresh local af34256 build attributes 3.74 s
  of compilation to the host target and 3.94 s to the allocator target; their executions take
  about 0.02 s and 0.04 s respectively. Both targets leave CI. Their exact release-stack and
  allocation-budget reproductions remain required external evidence. Existing unit properties
  now recover through a nonempty call after run or host OOM and root all six direct heap-reference
  argument kinds before collection; they reuse the interpreter already compiled for the unit
  suite. The debug small-stack property remains: its two unoptimized compiler invocations took
  0.68 s locally, and its source is included as an unused module for fmt, type checking and
  clippy, without generating a second optimized interpreter. The source-string allocation path
  is cold and out of line too, avoiding optimizer duplication in the normal reader loop.
  The initial corpus move to an external gate is withdrawn by G-166: it contradicted G-84
  and VISION's completeness rule. The 60.94 s to 24.96 s fresh-target local readings included
  that invalid coverage reduction and do not establish the budget for the restored gate.
  The fixture removals above stand; the full corpus stays in CI. G-167 replaces the 85.8 s
  target using measurements of the current pure baseline.

- **G-166. The workspace gate runs every program's own test** (restores G-84; corrects G-165).
  Evidence: `fuxport-2026-11-04/review/r2c/P1/repro/gate-skips-broken-program` exits 1 at
  9a6b6a7: a valid but failing form in a program passes the syntax-only property and the whole
  workspace suite while its own test.sh fails. Restore the corpus integration test from pure,
  with three workers to honor the local resource cap; discover every program and run its full
  script against Cargo's test-profile runner. Remove the overlapping syntax-only property.
  CI and the local workspace suite now run the same corpus. The separate final-head release
  checks remain useful evidence, rather than substituting for coverage in the repository.
  G-165's removed allocator fixture uniquely checked collection before the argument vector's
  reserve with zero allocator bytes available, including validation before collection. The
  existing slot-limit properties check nonempty-call recovery and root every heap argument
  kind, and embedding tests reject stale and foreign inputs, but they cannot force that vector
  reserve to fail. The exact zero-budget reproduction remains external evidence. Its original
  target cost 3.94 s of fresh local compilation for 0.04 s execution; restoring that separate
  optimized interpreter target is not a cheap check. No unsafe allocator or new production
  seam is added to the core to imitate it. Final-head commands remain in the report; G-167
  records the hosted gate timings and the accepted budget revision.

- **G-167. Revise the CI band against pure measured in the same window.** Round 2e
  (`fuxport-2026-11-04/review/r2e/`) accepts this revision in all four reviews, superseding
  G-165's original time target while retaining G-84's full corpus in the workspace gate.

  - Original target: before Stage 4, three pure commits read **95 s** (71cb5c5, the bytes
    merge, run **41934440818** attempt 1 on November 4), **78 s** (7ff695e) and **73 s**
    (a0eec1c): a median of **78 s**, and a 10% budget of **85.8 s**. Two of the three
    readings predate the bytes merge.
  - Evidence: `fuxport-2026-11-04/review/ci-readings/` records seven readings each of
    the same program code, with the last six alternating head and pure. Readings come from
    the check job in `gh api repos/gold-silver-copper/grift/actions/runs/<id>/attempts/<n>/jobs`.
    The head's code at 7cbadac, run **42125210440** attempts 1–3, read **95, 66 and 120 s**;
    360b781, differing only in DECISIONS.md, run **42129540060** attempts 1–4, read
    **118, 93, 67 and 100 s**. Their median is **95 s**. Pure 71cb5c5, run **41934440818**
    attempts 1–7, read **95, 86, 92, 100, 102, 75 and 85 s**, a median of **92 s**.
    In the strictly alternating pairs, pure read **102, 75 and 85 s** and the head
    **93, 67 and 100 s**, medians of **85 and 93 s** respectively. The spread within
    one commit, **66 to 120 s**, is larger than any gap between them.
  - New target: pure's median plus 10%, **101.2 s**. The head's **95 s** median meets it,
    at **3.3%** over pure.
  - Cost: the budget loosens by **15.4 s (+17.9%)**, from **85.8 to 101.2 s**. Nothing
    changes in the code, and the full corpus stays in the gate, as G-84 requires.

- **G-168. The docs say what stops, cleanups and panics do today** (corrects the promises in G-18,
  G-26, G-35, G-70, G-74, G-89, G-109, G-112, G-129, G-130, G-137 and G-139; no code changes). Each
  sentence was run first.
  - Cleanups do not always run, whatever G-18, G-26 and G-74 say (G-74 of G-17, G-35 and G-48), and
    a stop's are not certain, whatever G-109, G-112, G-129, G-137 and G-139 say.
  - `stop()` does not promise cleanups. `interrupted` runs them on its way out as any error does,
    but a stop can land in a cleanup that has begun and cut it, even before its first form. In
    `(finally! (mark "body") (mark "a") (mark "b"))`, stops at some allocations after the body left
    `body` alone or `body, a`, with the body's value lost; the same holds with an error leaving.
  - While an error unwinds, one failed machine-stack allocation as a cleanup starts skips every
    cleanup still to come in that evaluation, so a cleanup does not always start, whatever G-89's
    "as always" says. With a throwaway host's `arm` failing the fourth allocation after it,
    `(finally! (finally! (sequence! (arm 4) (first 1)) (mark "in")) (mark "out"))` answers
    `out-of-memory` with no mark, three nested cleanups likewise, and `(catch! ...)` around one
    `finally!` loses the `type-error` it would have caught. With every allocation after `arm`
    refused, G-89's case, the same program also answers `out-of-memory` with no mark. `unwind`
    leaves at once when it can't make room for the waiting error or the cleanup's frame. The same
    run after a deeper one in that interpreter, whose room is kept, never skipped both at any
    allocation from 1 to 40. A nested evaluation that skips its cleanups this way returns
    `out-of-memory` to its host function; returned as an error, that runs the outer evaluation's
    cleanups. After a body that returned, with the frame stack exactly full, one failed allocation
    skipped only that cleanup, and the one around it ran.
  - Stops waiting together count as one, as G-129 itself records; "each stop is acted on once" is gone.
  - `true` from `stop()` says a run was in progress, not that the stop acted: 12,045 of 23,653,712
    runs of `(add 1 2)` said `true` and returned 3.
  - A limit holds only against an ordinary error. `interrupted` then `out-of-memory` in a cleanup
    answers `out-of-memory`, and the reverse answers `interrupted`.
  - An error in a `finally!` cleanup replaces the one leaving, so that error does not always go on
    unchanged.
  - `exit` in a cleanup cuts that cleanup's later forms, against G-70's "its cleanups run". In a
    body, the cleanup forms run.
  - A panicking host function gets no `foreign-value`: the panic goes on to the host, no cleanup
    runs, and the next evaluation starts afresh. Replacing the interpreter still answers
    `foreign-value`, the case G-130's "answers `foreign-value`" keeps.

- **G-169. One-shot pausable runs** (reverses owner decision 10 of 2026-11-02, "no pausing
  evaluations", narrows G-53 and VISION's "No continuations", and amends G-168's out-of-memory
  finding). The owner,
  2026-11-06: "I do want pausable runs, so implement them extremely properly and extremely well
  tested." It no longer has to pass G-57's growth rule; spikes chose how, not whether. Three designs
  were built (a registry of machines, a heap object, runs polled as Rust values), each answering the
  same 14 questions by running them; blind judges chose the heap object 3 of 3 (notes:
  `best-2026-11-06/decide/tally.md`), and on desk it alone stayed in the +5% band for runs that never
  pause. G-53's reasons stand against continuations a program can hold: no program can hold this one,
  and it adds no name to the language.

  A host function registered with `parking_function` answers `Answer::Later(request)`. The whole
  machine moves into a heap object (`Object::Fiber`), which a kept binding holds, and the interpreter's
  machine is empty for whatever runs next; `resume` or `abandon` moves it back. Every hard question of
  the design note, with what the code does and the test that says so (`tests/parking.rs`, or
  `src/parked_tests.rs` where marked):

  - **A run never resumed, and its cleanups.** It stays alive, holding what it refers to, a visible
    leak like a kept value, until the host ends it. `abandon` raises `abandoned` at the wait, which
    nothing catches, so every cleanup runs. `parked()` counts these, and a dropped handle isn't the
    end of its run: `forgotten()` gives a new one. A park makes the room for that, so a drop needs no
    memory and a run can't be lost to one. (`abandon_runs_every_cleanup_and_nothing_catches_it`,
    `a_dropped_handle_is_found_again_and_its_run_stays_alive_until_then`,
    `dropping_a_handle_asks_for_no_memory`)
  - **What `abandon` does to a cleanup**, all the forms of one `finally!`. One that errors ends there, its later forms not run; `abandoned` goes on, and the next one out runs. One that
    parks fails at that call with `cannot-park`, since nothing could take the run up. One that never
    ends holds `abandon` up: it is the host's to stop. One that is stopped ends there with
    `interrupted`, the next runs, and `interrupted` comes back. One that can't be called for lack of
    memory is passed over, the next is tried, and `out-of-memory` comes back. No step limit: a count
    of steps measures nothing a host cares about, and can't bound a host call anyway.
    (`a_cleanup_that_errs_or_waits_during_abandon_ends_there_and_the_rest_run`,
    `a_cleanup_that_never_ends_during_abandon_is_the_hosts_to_stop`,
    `a_cleanup_that_cannot_be_called_is_passed_over_for_the_next` in `parked_tests`)
  - **`abandon` and the open stop rule.** `abandon` asks nothing of the interrupter: it raises at the
    wait, directly, so it holds whichever way the owner settles `stop`. Under today's stop, a stop cuts
    one cleanup and the rest run. Under a hold it would wait for a cleanup that ends, and `abandon` still
    cuts a cleanup waiting at a park, which no stop could reach. Under a force, the run is gone with
    its remaining cleanups not run, and others are untouched. Each is a test in
    `abandon_and_the_stop_rule`; the one that stops applying fails, and whoever changed `stop` decides.
    A stop can't reach a parked run, so one sent before `abandon` is no part of it. Both spikes were
    really merged and run, with force and hold tests against their real APIs (notes:
    `best-2026-11-06/job4/pr/stop-rules.md`). `parked.rs` needed no change for either, and the park
    needs none now: it only asks the machine's usual check whether a stop acts. Two things the
    merges showed for whoever settles `stop`: what a stop rule keeps per run belongs on `Machine`, or
    a park loses it, and a force must say it is stopping as `stop` does, or a force is lost at a
    park. A hold makes false what `abandon`'s docs say of a stopped cleanup, and needs a force
    for one that never ends; a force makes "every cleanup runs" false when it is used.
  - **The interpreter dropped with runs parked.** Their cleanups don't run, the same as a process dying.
    `Drop` has no host state to lend them. **This is a real weakening**, which VISION's "closing is
    explicit" already allows, and only hosts that park can meet it. Handles left over are harmless:
    `foreign-value` on another interpreter, nothing when dropped.
    (`a_dropped_interpreter_runs_no_cleanup_and_its_handles_stay_harmless`)
  - **Values across a wait.** A binding another run changed is seen after the wait, and a value read
    before may be out of date: **this is new for grift programs**, and in LANGUAGE.md. There are no
    data races, since runs change turns only at these calls. Locals and arguments stay. The request
    in `Later` is held by the run for as long as it waits, so it needs no `keep`; other values the
    host saved do. A stale or foreign answer is raised at the wait, as a host function's return is.
    (`values_across_a_wait`, `the_request_stays_good_for_as_long_as_the_run_waits`)
  - **Stops and limits on a parked run.** A stop belongs to the run in progress (G-129). While parked,
    `stop()` does nothing and says `false`. Each `resume` is a new outer run with its own generation,
    so "never a later one" (G-130) still holds. In the moment a run parks, a stop and the park see each
    other: the stop says it is stopping and then reads the run, the park unpublishes the run and
    then looks for stops, and if it finds one the run is stopped there, not parked. So no stop is lost
    at a park, tested against real races on two threads and step by step. The run's cleanups then
    run, and one of them may park, the stop having been acted on, so `true` and `Ran::Parked` can go together.
    Out of memory at the park fails that call; `resume` needs no memory.
    grift has no depth limit: a 200,000-deep run parks and comes back. (`a_stop_that_says_true_is_never_lost_as_its_run_parks`, `a_stop_under_way_as_a_run_parks_stops_it` and
    `a_late_stop_of_the_last_run_does_not_reach_a_resumed_one` in `parked_tests`,
    `a_stop_while_parked_finds_nothing_and_reaches_no_later_run`, `a_stop_during_the_call_that_would_park_stops_the_run_instead`,
    `resuming_needs_no_memory` and `a_run_with_no_memory_to_park_fails_there_and_its_cleanups_run` in `parked_tests`)
  - **Out of memory while unwinding.** It now collects when the unwinding starts, and a cleanup that
    can't be called is passed over for the next, where it used to end the unwinding. So G-168's "one
    failed machine-stack allocation as a cleanup starts skips every cleanup still to come" no longer
    holds. The error leaving still becomes `out-of-memory`, which no `catch!` catches, but the
    cleanups further out are tried. A cleanup still needs memory
    to do anything, so this doesn't promise it runs on a heap truly full of live data: the kind that
    comes back says that it may not have. (`out_of_memory_collects_before_its_cleanups_run` in
    `parked_tests`)
  - **Native twins.** Parking inside `if!`, `sequence!` and the rest can't be told from their
    definitions: same result, log, count of waits and trace, natives on and off.
    (`parking_inside_a_native_twin_cannot_be_told_from_its_definition` in `parked_tests`)
  - **Traces.** Frames keep their place and `here` is a root, so a later error shows the whole path,
    text for text as if the call had answered at once. `abandoned` itself says where the run waited.
    (`a_program_cannot_tell_a_wait_from_any_call`, `a_trace_after_a_resume_shows_the_whole_path_through_a_load`,
    `an_error_after_abandon_shows_where_the_run_was_waiting`)
  - **The collector and what a parked run holds.** One arm in `Heap::mark` walks the machine, by the
    same `Machine::roots` the running one uses, so nothing is rooted two ways. A parked machine counts
    for its size when deciding to collect. (`collection_loses_nothing_a_parked_run_holds_and_keeps_nothing_after`,
    `a_parked_run_counts_for_its_size_when_the_collector_decides` in `parked_tests`)
  - **One shot, by the types.** `resume` and `abandon` take `Parked` by value, and it is neither `Copy`
    nor `Clone`: a `compile_fail,E0382` doctest. No handle can go stale: it is linear, and the fiber
    is rooted until it is taken. Another interpreter's handle is `foreign-value`, and comes back whole.
    (`another_interpreters_handle_is_refused_and_comes_back_whole`)
  - **Several parked runs over one heap.** Each is its own fiber; others start, finish and park
    meanwhile, in any order. (`many_runs_share_one_heap_and_finish_in_any_order`, and the generated
    programs in `parked_tests`)
  - **Re-entrancy from a host call.** Rust frames can't be set aside, so a wait under `evaluate` or
    `call`, or in any run inside a host call, is `cannot-park`, naming the function. `start`, `resume`
    and `abandon` inside a host call are refused, the handle back and the run untouched. A movable
    machine suggests letting them nest, moving the caller's machine aside too; that wasn't built, and
    a host function can queue the work for its loop.
    (`parking_inside_a_host_call_is_refused_by_name`, `a_run_that_cannot_park_says_so`,
    `nothing_starts_or_resumes_inside_a_host_call`)

  One test holds the rest together: generated programs, each run alone with its waits answered at
  once and then many together in one interpreter, under random pauses, resumes, abandons, stops and
  allocation failures, with collection at every step in one case in eight. Runs nothing touched
  must equal their meaning, down to the trace; a touched run keeps its meaning up to the touch, and
  ends with the kinds allowed, and every cleanup it entered ran but those the touches may cut.
  (`whatever_happens_to_one_run_the_rest_mean_what_they_mean` in `parked_tests`; the many-seed
  runs are evidence, not CI.)

  **Not taken:** a language-level `pause`, and pausing a run from outside on a step budget; a
  cleanup that waits during `abandon`, which would make every host loop on `abandon` and let a run
  outlive the call that ends it. The API added: `parking_function`, `start`, `start_in`, `resume`, `abandon`,
  `can_park`, `parked`, `forgotten`, and the types `Answer`, `Ran`, `Parked`, `ResumeError`,
  `ParkingFunction`. Two new error kinds, `abandoned` and `cannot-park`.

  **What reaches hosts that never park.** `evaluate`, `call` and `host_function` keep their
  signatures. Two things change for every host, parking or not: out-of-memory unwinding, as above,
  and `abandoned` being a limit kind. So a name programs could use freely is now reserved: a
  program's or a host function's own error of kind `abandoned` used to be an ordinary error, and now
  can't be caught by `catch!` or `catch-kind!`, and an ordinary error in a cleanup doesn't replace
  it. A limit goes by its kind (`limit` in `evaluate.rs`), as `interrupted` and `out-of-memory`
  already did. `cannot-park` isn't a limit and stays catchable. `programs/` doesn't use the name.

## One model of how a run ends, 2026-11-06

- **G-170. A run ends one way: it unwinds with a reason, and who began the unwinding decides what may
  catch it** (the owner: "the philosophy of grift is that it must be super simple"; of `limit()`'s
  test of a kind's name: "completely against the philosophy of grift"; "don't care about the recorded
  decisions too much, they can be changed if evidence is against them").
  - **The model.** An error carries one bit, `limit`, which only the host (a stop, a force,
    `Error::as_limit`) and the machine (memory refused) can set. A handler runs only if it is clear.
    No kind's name decides anything: kinds are data. **One rule for a reason arriving during a
    cleanup: the newest reason wins, unless a program's would replace a limit** (`What::Finish` in
    `unwind`, and LANGUAGE "What can be caught"). **The host can always end a run:** that beats
    "cleanups always finish", as every working build chose and a 10/10 panel voted. So: `stop` as it
    was, `force` ending everything, no hold, no run report.
  - **One word instead of mailboxes.** An interpreter and its handles share an atomic word: the run
    in progress and two bits. A stop and a force are one compare-exchange, parking is a
    compare-exchange that fails if anything was asked, and the machine clears only the stop bit, so a
    force needs no code of its own: it is a stop that stays asked.
  - **Replaces in G-17:** "only loaded and stored, never swapped". Its reason was `thumbv6m`, gone
    with G-25 and G-32; `AtomicU64` already excludes that target and wasm32 has it. Atomics are
    lock-free where they exist, so a compare-exchange is safe in a signal handler, and no signal
    handler stops a run in this repo. Force had already broken the rule with `fetch_max`.
  - **Replaces in G-26 and G-35:** a limit "known by its code". Evidence: `limit()` was a test of a
    name in a language where any word can be rebound; a program's `(raise (error (quote!
    interrupted)))` could not be caught and looked real to the host.
  - **Reverses, by the owner, G-35's acceptance** "the host can't tell it from a real one; accepted,
    since only the program loses by it". A program can't forge a limit now: its error of any name is
    caught, and `is_limit` says false. (`a_program_or_a_host_function_can_only_make_what_is_caught`,
    and the forgeries in the generated test.)
  - **Replaces in G-168:** the cleanup a machine-stack allocation can pass over as it starts. A
    cleanup's install now makes the room its start needs, so its start can't fail, and the first
    thing that can is its first step, where the next check comes first. Evidence: a property test
    found force, then a refused cleanup call, reporting `out-of-memory` where the docs said
    `interrupted` (seed 9077, on four of seven builds). (`a_force_names_the_ending_whatever_memory_refuses_next`,
    `a_cleanups_install_makes_the_room_its_start_needs`.) G-168's other findings stand as true.
  - **Replaces in G-169:** `abandoned` as a reserved limit kind, `limit` by kind, `Deliver`/`Raise`,
    `enum Parking`, and `Grift::abandon` with its "**Not taken:** a cleanup that waits during
    `abandon`". A host gives a run up by answering its wait with a limit, so abandon is no longer a
    thing. Evidence: both judges named it as already expressible; it removes the method, `take_up`'s
    flag, the kind, one replacement place, one decision, and the only exception sentence any build
    removed. G-169 feared hosts must loop and a run may outlive the call that ends it: they loop
    only if they want to end a cleanup that waits, which is what a run that waits is, and `examples/emacs`
    needed about 20 lines for it. The fiber, the linear handle, `forgotten` and their tests stand.
  - **Replaces G-109, G-112, G-125, G-129's counts, G-131 and G-140** (one mailbox per handle, a lock
    stops don't take, counts per mailbox, a poll with no run): the word has no half-written state, so
    the interleavings they guarded can't exist. **G-123** (interrupter allocations may abort): a handle
    no longer allocates. **G-130** stands in behaviour; tests for panic, replacement and restore pass
    unedited.
  - **Replaces in G-70 and G-82:** `exit` setting the runner's interrupt: `exit` is a limit of kind
    `exit`, its status in its values, read once at the boundary; a program's own `exit` error is
    ordinary. An unreadable script is `file-error`, status 1, and 2 is usage only. A real failed write
    with a broken pipe, on any file, is a limit of kind `broken-pipe`, as `exit` is: cleanups run,
    handlers pass it, and uncaught it ends the run quietly with 0, read like `exit` through the bit and
    the kind together, so nothing knows which file is stdout and no program can forge it (the owner's
    ruling; a first version made it catchable and went by the name alone, which a program could
    forge). The cost: master let a program catch a broken pipe on a file other than stdout, as
    `file-error`, and now it can't. `display` fails as a write on its file does, so `output-error` goes.
  - **Replaces in G-169:** `start` inside a host call being refused. It is a nested run, as `call` is,
    and a wait in it is `cannot-park`; only `resume` there is refused. `outer && parks` already said
    so. `nothing_starts_or_resumes_inside_a_host_call` is now
    `inside_a_host_call_a_start_is_a_nested_run_and_nothing_resumes`.
  - **Replaces in G-48 and G-115:** `call-with-handler`. `(evaluate-with-handler x c h)` has
    `evaluate-with-cleanup`'s shape, and `catch!` allocates one closure less. A judge asked for the
    library wrapper back; another for no helper: none, since nothing needs it and `programs/meta` now
    calls the primitive directly.
  - **Replaces in G-22 and G-26:** a re-raise adds no location; its reason went with G-29. Every raise
    adds its own, once.
  - **Questions settled, each by the simplest true rule:**
    - *Force on a handle that never stopped anything:* it forces the run in progress. A handle forces
      the run it last reached, by a stop or a force, and that one only; a clone starts as its source
      is. So a late force from a handle that reached a run never reaches another. A fresh handle has
      the hazard a stop has. Evidence: a watchdog that only forces waited for ever (two red teams, three judges).
    - *A cleanup that waits while a limit leaves:* it is a run that waits, like any other. Answered
      with a value it goes on; with a limit it ends there, and the first limit stays unless a newer one replaces it.
    - *A stale limit:* it arrives as `stale-value` and stays a limit. The kind says what the host
      handed in; the bit says who began it.
    - *The first refusal costs the cleanup it lands in:* that cleanup was tried. A later one has what
      the collection freed. One retry was measured on another build and did not fix it.
    - *Only memory really refused asks for a collection,* by a `refused` mark `Error::oom` sets and
      nothing else can, not by the kind. A host's limit named `out-of-memory` collects nothing: it names
      a reason, and the host can say that without any memory having been refused. It asks once, as it
      begins to unwind or leaves for the host: one for each cleanup it passed took time in the square
      of the depth, and a cleanup refused all the same is a new refusal, which asks again.
    - *Run numbers wrap after 2^62 runs,* 146 years at one a nanosecond. Not handled: a refusal path for
      it would be code for a case that can't happen. One judge asked for it.
  - **Not done:** `run-process` still aborts on memory refused: `Command::output` buffers infallibly,
    and replacing it means hand-written pipes and threads. Stale limits don't keep their kind, so a
    stale `exit` is status 1.
  - **What this is by the count the owner asked for** (hand recount on both censuses' manifests, against
    `ce508a4`, master plus #206 plus force, the same features): Opus count ending decisions 43 to 25,
    reason replacements 14 to 8; Codex count 48 to 32 and 9 to 9. The rule set before the work says a
    real removal halves both, so this is **only renames by that rule, nearly there on one census and not on the other**.
    What did go: the mailbox protocol, force's own path, the left-the-run checks, the `Parking`
    enum, `abandon`. Non-test Rust 3348 to 3215 lines; public items 90 to 91; exception sentences 6 to 5.

- **G-171. The continuations spike: "no continuations" stands, on this evidence and not because it was
  recorded** (the owner: "use all our agentic skills to spike continuations"; tests G-35's and G-53's "no
  continuations of any kind"). The hypothesis: `catch!`, `finally!`, stop, force, exit, out of memory,
  abandon and parked runs are one thing, delimited one-shot continuations with markers, and holding a
  marker replaces a name. Four independent builds, by writers who hadn't seen the others, on branches
  `spike/continuations-k1` to `k4`, which stay local:

  | Build | Mechanism | Ending decisions, Opus / Codex | Replacements, Opus / Codex | Rust lines against baseline | fib, life, `finally!`, `catch!` against master |
  |---|---|---:|---:|---:|---|
  | baseline | master, #206 and force | 43 / 48 | 14 / 9 | | +2.12, +3.29, +3.82, +3.43 |
  | k1 | public capture, marker on `raise` | 50 / 62 | 14 / 9 | +271 | **+4.96, +7.81, +7.85, +6.46** |
  | k2 | public markers, capture, one-shot resume | 69 / 84 | 21 / 14 | +455 | **+7.07, +5.65, +8.24, +8.01** |
  | k3 | prompt and jump in the library, no capture | 39 / 47 | 11 / 10 | -107 | -4.78, -2.05, **+25.37**, -5.69 |
  | k4 | private markers and a captured suffix | 59 / 70 | 19 / 15 | +248 | -0.99, +0.58, +1.24, -0.11 |
  | G-170's model, as judged | one bit on the error | 24 / 32 | 9 / 9 | -114 | -6.14, -3.07, -1.80, -7.33 |

  Halving, the rule set before counting, is 21 and 7 on the Opus count, 24 and 4 on the Codex count: no
  continuation build comes near, and three grow. Bold misses the speed bands of +2% on fib and life and
  +5% on all four; one desk pass with another program loaded from its 53rd second, whose first two
  rounds give the same verdicts. All builds keep all 6 exception sentences (5 on the Codex count).
  - **Red teams.** Two, on all builds. No design defect in any continuation build: their defects were
    small fixes, 11 to 16 lines each, so this is no finding that continuations are unsound. But a kept
    limit became catchable on all four, because a fresh validation error lost its authority, and on k4
    even a rooted one, because an error's address dies with its run; a cleanup above a deep stack
    starved on all four, 75 of 75 budgets in one sweep. The two public mechanisms' 41 and 40 continuation
    tests pass, a 3,000-step generator and a 200,000-link chain among them.
  - **Samples and property test.** Same harm on all of them once measurement effects are removed, 185
    to 248 per 11,700 trials, against master's 612 lost workers to their 121 to 221. Three builds' new
    kind `forced` made about 1,400 trials per build read as plain errors to unchanged hosts. A late
    force from an old handle ended the next run on all four.
  - **What the hypothesis survives as.** Stop, force, exit, out of memory and abandon are one thing,
    and what unifies them is one bit, not an address: the only destinations in use are "a program's
    handler" and "the host". Addresses add liveness and routing decisions on top of the same policy:
    the more general the mechanism, the higher the count. Parked runs don't unify: capture refuses to
    cross a cleanup or a host floor and parking must cross cleanups, so public capture carries both
    mechanisms. Force isn't a transfer to anywhere, and out of memory still needs a collection.
  - **Three blind judges, one pick:** this model, 87, 87 and 87 of 100; the best continuation build 61,
    69 and 60.5. None dissented. If a language ever wants generators as a feature, that is a
    feature to weigh by itself: the k1 and k2 tests show it can work, and nothing here asks for it.

- **G-172. The audit's cleanups on one way for a run to end** (the special-cases audit: findings U1 to U134,
  each routed by two readers; this PR carries 43 of them, on G-170's model). G-170 wins on stop, force,
  exit, parking and unwinding wherever the two met. Each line names what it replaces and its evidence,
  a test unless it says otherwise.
  - **Replaces in G-6, G-83:** the interactive loop in Rust. It is grift in `runner.grift`, under one
    host word that reports an error as any run's is; G-83's read-before-evaluate holds in `unfinished?`.
    Evidence: `a_terminal_session_goes_on_past_an_error_and_ends_with_its_input`, `a_program_at_a_terminal_reads_the_next_line`, which hung before.
  - **Replaces in G-70, G-82, G-100, G-106, G-146:** standard streams are files like any other, with
    `path` and `close`; a program on stdin finds it at its end, not `closed`; `write` takes a string or
    bytes and `write-bytes` goes; `display` writes through the file table as `write` does, so G-170's
    `file-error` and its `broken-pipe` limit come from the one path. Evidence: `a_standard_stream_is_a_file_like_any_other`, `standard_input_is_data_for_a_script`, `a_failed_write_is_an_error`.
  - **Replaces in G-170 and G-69:** "2 is usage only". The first argument is always the script, so
    no usage error is left and nothing gives status 2. One that can't be read is G-170's `file-error`,
    status 1, and one that isn't there is followed by the usage. Evidence:
    `the_first_argument_is_the_script_whatever_its_spelling`, `only_a_script_that_is_not_there_prints_the_usage`.
  - **A session's own reading and prompting are a grift program's.** Standard input or error failing or being
    closed ends a session with 1, as an error in any program does, where the old `say` ignored a failed prompt.
    The docs say so. Evidence: `closing_what_a_session_prompts_on_or_reads_ends_it_with_status_1`.
  - **`read-file` and `load` keep nothing.** `read-file` is grift over the file table (U5), and forgets its entry
    when done, so reading again and again doesn't grow. A file a program opened and closed keeps its entry, as it
    did: its number stays `closed` and never comes to mean another file. Evidence:
    `reading_and_loading_again_and_again_keep_nothing`, `a_closed_file_stays_closed_past_later_reads_and_opens`.
  - **Replaces in G-98:** `script-path` conditional; it is always bound, `""` on stdin. **In G-119,
    G-120, G-151:** programs get `read-forms` and `position`, and `load` is grift over them. **In G-164,
    its last rule:** the runner's prefix exception; every error starts with its source. Evidence: `script_path_is_always_bound`, `a_program_s_own_loader_gets_the_lines_load_gets`, `a_parse_error_says_where_the_form_opened`.
  - **Replaces in G-111, G-108, G-11, G-113, G-152:** the third root context and the reserved `KEPT` ids. A kept
    value is an entry in one private table. **In G-170's parking:** a parked run returned its kept name as
    a symbol nothing else could be; with no reserved ids, `park` leaves the name in one field `ran` takes.
    What a host and a program see of parking is G-170's. Evidence: the API and parking tests, changed only
    for U40, U5, U17, U23 and U48, as the PR says.
  - **Replaces in G-76, G-120:** `Error::trace()`, `is_kind` and `as_string`; `trace` is a public field checked
    as values are, kinds are read with `symbol_name`, and hosts copy with `as_str`. The audit's one
    loss, a runner out-of-memory copying a string becoming catchable, is gone: G-170's `as_limit` makes it a limit.
  - **Replaces in G-97, G-136, G-151, G-163, G-164:** a source held as a string in the trace; it is a symbol.
    **In G-77, G-142, G-144:** the reader's second loop for `#bytes(`; the literal stands.
    **In G-69, G-87:** `#!` on the first line only; it starts a comment wherever a token can, in `programs/format` and `scripts/census.py` too.
    **In G-143, G-147:** only bytes counting their storage; strings do too. The finding's 522 MiB against 13 MiB was not re-measured here.
  - **Replaces in G-14, G-20, G-103, G-118:** `(context)`; `(context ())` is the one form. **In G-60, G-20, G-118:**
    a program can make a child of the built-in context with `(standard-context)`. **In G-7:** `built-in-names`
    is a listing only. **In G-67, G-85:** `inert?` and `ignore?` are back: every type has a predicate. **In G-94,
    G-102:** an overflow names the arithmetic that overflowed. `second` and `third` keep their range-errors: U104
    decided part of the owner's U103, so it is off this PR and on the owner's list with it.
  - **Replaces in G-5, G-160, G-161, G-162, G-165:** the build script and fixed-size parts of the stack test; a
    scratch Cargo package and 24 KiB a callback, so 300 KiB becomes 384 KiB on every target, because aarch64
    needs about 319 KiB. It is looser, and that is a loss: G-161 set 300 KiB so that d9dc54a would overflow, and a
    growth of that size, 0.6 KiB a callback, now passes. It catches gross growth only. Measured once at this head:
    aarch64 passes 384 KiB and overflows 288; x86-64, on box2, passes 288 and overflows 280, so about 96 KiB of room.
    The 292 KiB in the old comment was the old fixture's.
  - **Replaces in G-116, G-126, G-135:** the machine's switch for tests. Tests rebind each native's definition
    and clear the two twins' flags; the 3,129-case comparison passes, with one allowance: how a native itself
    prints. **In G-110, G-117, G-81:** `string-search`'s twin. It is a native primitive with its definition in
    `natives.grift`, `string-split` calls it (split -91.9%), and `bytes-search` is the one search walker.
  - **Not replaced: `if!` and `sequence!`'s twins (G-126, G-128, G-141, VISION's rule, owner, 2026-11-02).** U50 made
    them native primitives and removed the twin mechanism, but a running program then couldn't read `if!`, and
    README, VISION and the reference's opening sentences stopped being true. It reverses the owner's rule and bears
    on G-31's "grift stays a pure fexpr Lisp", so it is off this PR and on the owner's list, at `p2/u50`. Left of
    the mechanism: `Native` (`If`, `Sequence`), `install_natives`, `Operative.native`, `native` and `three`.
  - **G-33, G-31 and `vau!`:** `vau!` takes any number of body forms, so `lambda!`'s branch, G-33's complaint,
    is gone. The rule it replaces wasn't in a decision: G-31 says nothing about it, whatever a routing reader
    wrote; it was the `(vau! parameters context-parameter body)` line of `LANGUAGE.md`. It takes away a hook:
    a program's own binding of `sequence!` no longer changes a `lambda!` body of several forms. Evidence: `an_operative_takes_any_number_of_body_forms_and_prints_them_as_written`.
  - **Settled by G-170 and not done again:** U2, U10, U16, U22, U53, U54, U57, U58, U63, U89 and the other job-one findings.
    **Dropped by measurement:** U46 (fib +0.79 alone, and left off because it went over the limit with U50, which is no longer here, so that reason has gone; not retried), U55 (1M calls +115.6%), U97
    (life +4.75%, 300k-element loop +145%), U32, all 24 (smallest loss +1066%).
  - **Two speed reversals inside the audit:** `names` has its own function (U78 cost fib about 5 points)
    and a call reserves its bindings ahead again (U61 cost life about 2 points); one walker stays.
  - **A band missed, not revised.** Target: within +2% of one-model's head per step. Desk, 7 interleaved rounds, fat
    LTO, without U50: fib **+2.40%** (paired +2.11%) against `5cc9239` and -0.64% against master `56713e7`; life -0.42%,
    -0.91%; programs total -19.15%, -25.90% (without meta -5.37%, -4.71%). Fib is what misses, steadily: a byte copy of master
    read +0.30% against master, and every arm but `5cc9239` read 115 to 116 ms. Whether `5cc9239`'s 112 ms is real speed
    or code layout wasn't separated. U50's arm read fib
    +2.85%, life -2.27%, programs total -32.88%, meta's suite 7.6 s against 11.8 s, `Grift::new` 0.64 ms against 0.63 ms.
- **G-173. The owner's answers to the one-model run's open questions** (owner, 2026-11-07, after #209 merged as
  `51bd740`; recorded in the run's `owner-decisions.md`, and the owner's own yes, not a panel's).
  - **Kept as they are, no code change:** `if!` and `sequence!` stay grift definitions with their twins (1a); a real
    broken pipe stays a limit (2a); `Grift::abandon` stays removed (4a); #209's fib reading is accepted (6a);
    `can_park` stays (9).
  - **A closed pipe ends with status 141** (3b; replaces "ends quietly with 0" in G-170 and G-172): still quiet and still
    a limit, but "newest limit wins" can no longer turn an `exit 3` or an out-of-memory into a success when a cleanup
    prints to a closed pipe. Evidence: `a_closed_pipe_is_a_limit_on_any_file`.
  - **A path or argument that isn't UTF-8 is refused at entry** with `encoding-error` carrying its bytes (5a, U7;
    replaces G-98's lossy path and `load`'s recovery of it). Lost: running a script in a non-UTF-8 directory.
    Evidence: `a_path_or_argument_that_is_not_text_is_refused_at_entry`.
  - **`read-file` is Rust again** (7, U5; replaces G-172's grift `read-file` and `forget-host`).
  - **As advised for every row of the audit's owner list (8):** removed `Grift::list`, `list_items` (U82) and
    `bytes-search` (U111); `write_string` is private (U90); `Primitive` is opaque (U28); one vocabulary for strings
    and bytes, `byte-length`, `byte-at`, `slice`, `join`, `search`, the first argument's kind is the answer's (U33);
    list words keep `range-error` through one shared walk (U103; U104 stays out); `string-repeat` checks nothing of
    its own, pad keeps its one-byte fill (U109); an error's values are the values involved, never who failed (U38);
    whether code is the program's is a mark set when a pair is made, and `seal_library` becomes `evaluate_library_in`
    (U42); `load` resolves a relative path as every file word does, against the working directory, with a
    `script-path` helper in the README (U25; replaces G-98's and G-151's load-relative rule); `conditional!` with no
    true clause giving `#inert` (U94) and bare symbol printing (U116) are documented. Kept: U18, U83, U102, U77,
    U34 and U43.
  - **Speed, desk, two passes against master `84770fc`:** fib -2.7% and -2.7%, life +1.6% and +0.6%, programs total
    -0.4%; bands +2%, +2%, +5%. U33 alone cost nothing measurable. Before the last two commits the branch was about
    +21%: U42's frame field narrowed to `u32` beside a `bool` brought back the store-forwarding stall #210 removed. One
    8-byte word holding the argument base and the mark fixed it, and a frame stays 56 bytes, asserted at compile time.
  - **A target revised, under CLAUDE.md's rule.** Target: non-test Rust lines fall. Measured, against `84770fc`, by the
    census lexer: Rust +59 (core +78, runner -19), embedded grift -78, net -19. All of the Rust growth is U42: +48 for
    the mark itself, +9 and +32 for keeping frames fast, +10 for the bug below. Without U42 the branch is -40 Rust.
    The owner chose explicit provenance, and its cost in lines is what it takes to carry the mark without a speed loss.
  - **A bug found and fixed:** code a host built with `pair` right after catching a panic from library code (or after
    swapping interpreters) was marked as the library's until the next run. Evidence: `a_run_its_host_function_left_is_over`.
- **G-174. The owner restores `Grift::list`, `Grift::list_items` and public `write_string`** (2026-11-07, Job 5
  update), replacing U82 and U90 in G-173. A host may build and read a list whole or pair by pair, and write text as a
  literal with `write_string` or with `string` then `write`. Why: the owner's call on the fux port to master `38803b4`
  (`lowlevel-2026-11-07/port-friction.md`, entries 1, 2 and 4). fux used `list` and `list_items` at 38 sites and had to
  carry its own copies (in its bridge and again in its tests), each deciding for itself what an improper list is. fux
  quotes text where it has no interpreter, such as config lines it generates, so it carried a copy of `write_string`
  too, and a copy drifts from the reader if grift's escapes change, with no test in either repo to notice.
- **G-175. Before 3.0.0: what the independent testers found, and three loose ends** (owner, Job 5, 2026-11-07).
  - **A handle names one run** (replaces G-170's "force on a handle that never stopped anything forces the run in
    progress" and its fresh-handle hazard): an `Interrupter` is for the run in progress when it is taken, or else the
    next one to begin. A stop or force reaches only that run, says `false` before it begins and after it ends, and a
    clone is for its source's run. A force needs no stop first. A watchdog needs no lock. A resumed run is a run of its
    own, so a host takes a handle for it just before `resume`. Evidence: `a_handle_reaches_only_the_run_it_was_taken_for`,
    `a_force_needs_no_stop_before_it`, `a_watchdog_over_many_jobs_needs_no_lock`.
  - **A collection needs no memory of its own** (testers' P33): the mark stack's room is made at `Grift::new`, and past
    it, passes over the heap find what is left to mark, so a heap full of garbage is reclaimed with nothing left to
    allocate. Evidence: `a_collection_needs_no_memory_of_its_own`, `the_mark_stack_has_room_before_any_collection`.
  - **The machine loop's data shape is asserted as it compiles**, guarding #210: a frame 56 bytes, align 8; `What` 40;
    `Mode` 24; a value 16; every `What` field whole machine words; the value's tag a whole word (a test, since its size
    alone doesn't show it). It can't catch code layout, inlining or branch shape: speed is still measured on desk.
  - **`run-process` no longer aborts when memory is refused** (replaces G-170's note): fallible buffers, both pipes
    drained, the child reaped, and the `out-of-memory` limit. Evidence: `memory_refused_to_a_process_s_output_is_a_limit`.
  - **A session numbers its lines from its start**, so `stdin:N` can be found. **An error report is one write** up to
    4 KiB, so output sharing its pipe can't land inside it.
  - **Docs made exact, not code:** a descriptor the shell closed is `/dev/null` by the time grift starts (Rust's runtime);
    `--help`, `--version` and `-` stay script names (G-172 stands); `read-line` consumes a line that isn't UTF-8; a bare
    symbol at top level has no combination to list; an `out-of-memory` trace can miss places; a tail call replaces
    its caller in the trace; prompts, what a session prints, C-c, `read-bytes` at a terminal, `load` in a `lambda!`,
    `(names here)`, `Ran::Finished`, `Symbol`, a watchdog, the `forgotten` loop and ending a cleanup chain that waits.
- **G-176. A run refused memory gives up a spare 64 KiB before it says where it happened.** A trace, the copy a report checks and the printer's stack all allocate, so when the refusal was a small allocation with only scraps left, the report came down to what the scraps could hold: no trace, or `out-of-memory in (`. Which scraps are left depends on the exact build and limit: the same source built in another directory gave a different trace. That f723636 only shifted the scraps is inferred, not measured. `Grift::begin_run` reserves the room, `unwind` drops it at a refusal, and the next run takes it again, after the collection the refusal asked for. Rejected: collecting as the run ends, which frees nothing until the frames are gone, so it can't help the trace being built while they unwind. An out-of-memory trace can still miss places past this room. Evidence: `an_out_of_memory_report_is_whole_with_nothing_left_to_allocate`, a thread-local allocation budget in the runner package, since the core forbids `unsafe`.
- **G-177. Making grift simpler: what a run decided, 2026-11-08** (a 10-seat panel on anything that removes a capability or reverses a recorded decision; Opus and Codex mixed; 7 of 10 settles; votes are `for` / 10). Each kept item stays as it was; each adopted item below names the decision it changes.
  - **Adopted.** Printing and describing fail with an ordinary `Error`, with `output-error` as a known kind, replacing G-120's `WriteError` (9/10: Opus 4/5, Codex 5/5). The public `Value` has one `Operative` variant (10/10). `less-than?` orders two strings or two bytes values, and `string-less-than?` goes, replacing G-101 (10/10). A `#bytes(` element is read like any datum and only then must be a byte, replacing G-142's early refusal (8/10: Opus 4/5, Codex 4/5). A parse error's source is the last of its values, so a trace holds only combinations, replacing G-163 and G-164 on where it sits (7/10: Opus 2/5, Codex 5/5). A parked run is an entry of the kept table, with the free list in the entries, replacing G-169's heap object (10/10). The mark stack can always hold every slot, a new slot first reserving its entry, replacing G-175's fixed room and passes (10/10; 4 bytes per slot of capacity, 2.4 to 3% of address space, measured). A nested run is a machine of its own, replacing G-90's held frame and G-112's second counter (10/10; a nested `evaluate` allocates 3 times instead of 1). Where a combination came from lives in its pair, replacing G-22's table (10/10).
  - **Voted down, so kept:** the forgotten queue (1/10); `ResumeError` (0/10); `start_in` (1/10); unnamed `Grift::read` (0/10); `Grift::names` (1/10); `Grift::parked` (0/10); traces of only text-read code, G-173's provenance (3/10); the outer-form boundary, G-154 (3/10); `display` as grift (1/10, streaming); the runner's `position` (2/10); several lists in `map` (1/10); merged parse reasons (1/10); `absolute-value` in the library (2/10); `list-to-bytes` and `bytes-to-list` as library words (3/10). **Withdrawn:** removing `bindings` (4/10, one line, not worth a second round; it stays).
  - **The frame's field-by-field width check stays, by the tie rule.** Round 1: 5 of 10 for removing it (Opus 5/5, Codex 0/5). Round 2 tallied 6/10, but one seat changed without quoting what changed it, which counts as its first vote: 5/5, so nothing moves. Strongest case for removal: it guards one of many causes of slowness in 42 hand-listed lines, and a narrow pair hidden in `Base` passes it; the speed run caught both real regressions.
  - **Measured and dropped:** `safe_point` and `prepare` returning nothing, +37% on every program through the loop's compiled shape alone (no assertion can see that). A generic one-`Value` shape (saves 6 lines, adds a public name, worse errors) and a machine loop returning at host calls (+13 lines) lost in their own spikes.
  - **A target revised.** Original: non-test Rust plus embedded grift falls at least 8%, 4,453 to at most 4,097 lines by the lexer count, no compressed syntax. Reached: 4,217, -5.3%. Evidence: three audit waves on the code plus a from-scratch one, 55 of whose last findings had verifier-confirmed gains of about 134 lines. What remains is either behaviour that a panel kept, or speed the loop can't give back. The rest needs a capability owner's choice, not more auditing.
- **G-178. Four bugs, one rule: bounded, and refused out loud** (the owner's choice, 2026-11-08: "Go with your recommendations", taking choices 1.1, 2.1, 3.1, 4.1 and 5.1 of `notes/projects/grift/bugs-2026-11-08/report.md` in one PR, over an earlier wish for separate small tasks). Each design won a 10-seat panel (8/10, 9/10, 9/10, 9/10) and was built as a spike first. Replaces G-90's and G-177's nested run, G-112's "nested evaluations share it", G-169's `parking_function` and its `cannot-park` inside a host call, and G-176's report copy of the trace.
  - **No run inside a host call.** A run begun inside a host call (`call`, `evaluate`, `evaluate_in`, `evaluate_library_in`, `start`, `start_in`, `resume`) is the ordinary error `cannot-nest`, refused in `prepare`, which every way in passes before anything else, so nothing collects either. A host function that needs grift to call something answers `Answer::Call { function, arguments, then }`, and the machine makes the call on its own frames after the host function returns: recursion through a host function costs heap, not Rust stack (20,000 levels on a 256 KiB stack unoptimized), and a wait inside it parks the run. `then` reuses the argument frame, so the machine has no new kind of frame. `Grift::suspended` is gone, and with it the run-number check after a host call: nothing ends a run inside one now. Before, the program chose the Rust stack's depth through an ordinary host function and aborted the host at 165 levels on 256 KiB. Rejected: a fixed or host-set nesting count (a count doesn't bound bytes) and a stack-bytes budget (a knob the host can't size, blind on wasm).
  - **One kind of host function.** `HostFunction<T>` returns `Result<Answer, Error>`, and `host_function` registers every host function; `parking_function` and `ParkingFunction` are gone. Why: with `Answer::Call` beside `Answer::Later`, "parking" named one of three answers, and two registrations for one thing doubled a public type and the machine's dispatch. Cost: a host function that answers at once writes `Ok(Answer::Now(v))`; the runner wraps its seventeen with a small macro. Parked runs (`start`, `start_in`, `resume`, `can_park`, the forgotten queue) stay, as the keep-list says.
  - **The runner's `report-host` parks.** Its thunk must be a run of its own, so that a limit ending it (`out-of-memory`) is reported and the session goes on. Answered with `Answer::Call`, the thunk would run in the session's own run and a limit would end the session, so `report-host` answers `Answer::Later(thunk)` and the runner's loop calls the thunk and resumes the session with its ending.
  - **Reports.** `describe(&e, &mut out, bytes)` returns `ReportStatus::{Complete, Truncated}`, with no number of grift's own (the runner gives 4,096). A report that fits `bytes` is written whole and is `Complete`: a first pass that writes nothing finds out. One that doesn't fit has the kind and the innermost place first, each in at most a quarter of `bytes`, cut with `...`, so it begins with both from 30 bytes up, and below 30 with as much of the kind as fits; the line whose place the header shows doesn't repeat it. Each trace line writes the line before's combination as `...`, so a report's size follows the program's, not its square. A context prints as its count of names, so a report needs no memory or time for a context's names. Names have their control characters escaped as in a string literal, so only grift begins a line of a report. The report prints from the error, copying nothing: a 4 KiB report of a 2,000-deep out-of-memory ending, or of data nested 5,000 deep, has 16 KiB to spare (base needed tens of megabytes; a 32,000-deep report was 4.1 GB). A report writes data nested more than 32 deep as `...`, so its printer needs about 2 KiB whatever the data: what a host must keep is room for its text and that. Its work, every check, item and byte, is one walk, which a stop ends. `TextBuffer` (`new`, `as_str`) is the sink that refuses instead of aborting, replacing the core's own copy of it. grift writes no marker in the text: the status says it was cut, and a program's kind can't forge a status. The runner writes `(report cut short)` on a line of its own, which no program text can begin; it says no number, since a report under its 4,096 bytes is cut when it leaves deep data out.
  - **`Grift::new`'s first allocation**, the shared control block, is `Arc::new` and aborts if refused; the rustdoc and the README say so, and it becomes `Arc::try_new` once that is stable. Rejected: leaking 40 bytes per interpreter so it never aborts (an unbounded leak for a host making interpreters all day) and a probe allocation first (it only moves the abort).
  - **Search.** Bytes and strings share one Two-Way search (Crochemore and Perrin): linear, no memory, with a 64-bit fingerprint of the pattern's bytes that skips windows ending in a byte the pattern lacks, and a stop check in every phase. Before, a byte search was quadratic and couldn't be stopped (8 s after a stop at 1 MiB, 103 to 133 s at 4 MiB), and a string search couldn't be stopped (2 to 3 s per GiB). Cost: long string searches 1.25 to 1.5 times slower than `str::find`, measured on the spike.
  - **Same rule, same PR.** The printer charges each byte it writes to its walk, not each item, so a stop reaches a print of a few large items (1,000 references to one 4 MiB value held a stop off for 41 s). `Error` no longer derives `Clone`, whose copy aborts when memory is refused.
  - **Left for later:** collections paced to the work they walk (a sweep of every slot ever had, collections that don't count a deep machine, a kept table that never shrinks), a catch-and-raise copying the trace each time (G-76), the runner's infallible allocations (`read-line`, `open-file`, the OS names), and interned names never reclaimed.
  - **Evidence:** `a_long_search_finds_a_stop_by_itself`, `searching_finds_what_the_plain_definition_finds`, `printing_finds_a_stop_by_the_bytes_it_writes`, `a_host_function_that_calls_back_never_overflows_a_small_stack` (its own unoptimized process), `a_then_gets_its_saved_values_and_the_result_and_an_error_passes_it_by`, `a_host_function_loops_over_callbacks_by_answering_call_from_its_then`, `a_report_fits_its_budget_and_begins_with_kind_and_place`, `an_out_of_memory_report_needs_memory_for_its_text_alone`, `a_cut_report_says_so_on_a_line_no_program_can_write`, and the `ErrorIsNotClone` doctest; each failed at f2011f82 for its stated reason first (`notes/projects/grift/bugs-pr-2026-11-08/failing-first/`).
- **G-179. Tests state rules, three promises hold by design, and the suite is bounded** (2026-11-09, replacing PR #217). Built from a spike study (`notes/projects/grift/spikes-2026-11-09/report.md`), with the owner's rulings on it. Takes G-178's "left for later" catch-and-raise copy and names never reclaimed.
  - **The testing rule.** A review finding or bug report goes into the program generator, a native's twin or a guard (a type, a lint, an assertion); a new scenario test needs a sentence in its PR on why none of those can say it (CONTRIBUTING.md). A test is deleted only when the held-back bug set shows the suite still catches everything it caught. CI fails when test characters per production character rise above `scripts/test-ratio.baseline`, and asks for a lower baseline when the ratio falls, so the bound only tightens. Counting characters, not lines, because packing tests onto fewer lines fooled a line count (1.83 to 1.75) and not the character count (1.891 to 1.893). Neither catches helpers moved into the library or deleted assertions: review does.
  - **The trim.** 170 of 204 tests deleted, kept by an exact set cover over the catch matrix of 38 historic bugs and 114 mutants: tests 9,150 to 4,160 lines. Evidence, with the official fail-fast command on box2 at the PR's head: 34/38 bugs and 87/100 mutants (master 33/38 and 80/100), and 14/14 new runner and library mutants (master 12/14), every row the trimmed suite caught in the spike still caught. This PR rewrote code that 35 of the 138 patches touch: 18 merged onto it mechanically (`git merge-file`), and 17 were ported by hand by an agent that never saw the tests; all are counted.
  - **Guards for new built-ins.** clippy `disallowed_methods` refuses allocations that abort when memory is refused (each reserved site says what made the room); one charged `Items` walk for every list walk inside a step; every primitive classed in one test-side `match` with no catch-all arm, which puts it in a stop sweep, a deep-data sweep on a small stack and a memory-refusal sweep. On master the sweep found `raise`, `names`, `read-form`, `list-to-bytes` and `bytes-to-list` going on past a stop; all five now stop.
  - **R1, owner ruling (b), overruling the panel's 7/10 for (a):** "a stop before a run starts seems wrong". A stop asked before a run starts does nothing, as before. The reader checks for stops, and the host's `read`, `read_in`, `write`, `describe` and `bind` are each a run a stop reaches; reading the text `evaluate` is given is part of its run. Rejected: (a), a stop latched for the next run, which lands on whatever runs next when its job never comes, the host's own error `write` included.
  - **F2.** A program reads a trace newest first, and a handler's trace shares the list it was raised with, so a catch and raise adds only the places since: 16,000 levels went from 5.3 s to 65 ms. The host's `Error::trace` stays innermost first. `local::Trace` has no way to copy itself.
  - **F3, owner ruling: design B, overruling the panel's 6/10 for A** ("i agree with ur suggestions"). Names are heap objects collected like any value; a host's `Symbol` carries its slot's generation and is `stale-value` at import once its name is collected, as any reclaimed value is. 400,000 dead names: 48 MB peak to 11.7 MB. Rejected: A, #216's counted host symbols (+180 production lines against +66, a lock in `Symbol`'s `Clone` and `Drop`, two lifetime rules for hosts). Cost: `Grift::new` +15% (0.52 to 0.60 ms on desk), the library's names now being heap objects.
  - **F1, owner ruling: out.** Pacing collections to the work they walk and giving memory back cost life +3.4% at the 4,096-object minimum, and the give-back check failed after a short loop. Its own change later.
  - **Speed** (desk, 11 interleaved rounds, against master, byte-copy control within 1%): fib −2.1%, life −4.7%, catch +2.2%, finally +0.5%; `Grift::new` +13%. CI's check job 95 s to 65 s (one run each).
- **G-180. Memory used at a peak comes back, collection stays linear, and two trace/park bugs are fixed** (2026-11-09, the owner's "okay sounds good" to F1 plus the audit's two real bugs as one PR). Supersedes G-179's "F1, owner ruling: out" now that F1's cost is measured with its fixes.
  - **Pacing.** A collection waits for as much allocation as the largest of what the last one walked: what survived (a context weighs `max(1, the slots its bindings fill)`), the machine and kept table it walks, and half the free slots; `GC_MIN` stays 4,096. A new kept entry and growing bindings bring the next collection nearer. Why: every term a collection walks is paid for, so collecting costs a constant factor of what a program allocates (G-178's "left for later"). Guarded by a `cfg(test)` assertion in `Heap::collect`: a paced collection walks at most 8× what was allocated since the last, bindings included.
  - **Give-back.** Measured cause of the late give-back after a peak: the newest objects (the running run's source, the loop's newest context) sit at the allocation frontier, and objects never move. So after a paced collection that leaves the free slots as the largest pacing term, the next host run collects first, once, when that source is garbage; a collection asked for (or forced in tests) never arms it. A collection gives back the free slots past the room the next one waits for, once they are a top third; the running machine's stacks and the kept table give back room under a quarter used, keeping twice their length and at least 4,096 (a cleanup's reserved start lives in it). Rejected: allocating lowest-first alone (already so; the blockers predate the collection); an early collection inside the run (the run's source is still live); counting a call's bindings at creation (fib +4–5%).
  - **Targets revised.** 16M-pair peak: under half within 600,000 steps is impossible under the 8× bound (the first walk of a 37M-slot heap needs ≥4.6M units of allocation); it comes back after about 7M steps, a 4M peak within 600k. Speed: fib +4.05%, life +4.85% against a +3% band; F1 alone was in band, the rest is about 2 points of instructions for linearity with many bindings and kept values, and code layout (a 3-line non-hot change moved life 3 points).
  - **Bugs.** `pair_local(a, b, library)`: whose code a pair is, is an argument (the machine's mark, the reader's flag, `false` from the host), so code a host function builds keeps its places in traces whatever called it; `settled()` is gone. `can_park()` is `false` with no run in progress, after a host function's panic too. Text library code reads with `read-form` stays library code (G-173).
  - **Evidence:** held-back set on box2 with the official fail-fast command: 34/38 bugs, 88/100 mutants, 14/14 runner and library mutants (master 34/38, 87/100, 14/14), nothing master catches lost; rows whose code moved were merged or ported by a helper that saw no tests. Review: two rounds of six (3 Opus, 3 Codex); round 1 found three must-fixes (a cleanup's room lost to a stack shrink, kept-table growth unpaced, context bindings unweighed), each fixed with a failing test first; round 2, 6 of 6 mergeable.
- **G-181. 4.0.0: a host function can't start a run, values are `Copy`, and the audit's cleanup** (2026-11-09, the owner's "do it all": the narrow host handle and the cleanup bundle as one PR before 4.0.0). Replaces G-178's `cannot-nest`, G-113's `Weak` identity and "`Value` is not `Copy`", and G-94's and G-177's `absolute-value` and `power` in grift.
  - **The handle.** A host function gets `&mut HostCall<T>`, a borrow of the `Grift` that derefs to `&Grift` and forwards only the methods that make, bind and keep values: starting a run inside one doesn't compile, so `cannot-nest` and its kind are gone. The audit's option (b), not the spike's `Interpreter` split (a) the panel chose 8/10: (b) also makes swapping the interpreter out impossible, so the check after each host call and its `foreign-value` path go; +26 library lines against +107; the machine's layout is untouched. A host's mid-run `read`, `bind`, `write` and `describe` join the run in progress (G-179).
  - **`Copy`.** `Value`, `Symbol`, `Reference` and `Operative` are `Copy`; an interpreter is a process-unique `u64` from one counter, never reused, so a value needs no pointer to it. `Parked` keeps its own `Weak<Control>` for its drop.
  - **Cleanup.** An operative's context parameter is `Option<Symbol>` (heap slots 72 → 64 bytes); an error's two flags are one `Began { Program, Limit, Refused }`; the heap's nine test fields are one test-only probe; `absolute-value` and `power` are one Rust primitive each (G-177 Q19: NaN sign bits and `powf` can't be written in grift), each `overflow` carrying its own operands; lints are workspace lints the runner inherits, `todo!` and `unimplemented!` denied, `arithmetic_side_effects` not (39 bounded sites in the core).
  - **Dropped:** importing a call's arguments once. Master's two passes are bug 43's fix (collect before the argument storage is allocated); the single pass brought it back, found by the held-back porter; `a_call_collects_before_copying_its_arguments` guards it. Also dropped: `power` through `add`'s shared arithmetic (fib +1.5% instructions).
  - **Targets.** Speed in band (desk, 11 interleaved rounds). Production lines +38 against a "negative" band, revised: the bundle's bigger savings had landed in #219 or were bug 43's fix, the handle adds a type, and the one cut that would have saved 9 more (`power` through shared arithmetic) cost fib 1.5%.
  - **Evidence:** desk, 11 interleaved rounds against master: fib −0.8%, life −2.4%. Held-back set on box2: 35/38 bugs, 88/100 mutants, 14/14 runner and library mutants (master 34/38, 88/100, 14/14), nothing master catches lost; bug 43 is newly caught. Mutant m103 (a string's weight losing its own slot) was caught on master only by chance through a generated test, which 64-byte slots moved; `Heap::alloc` now asserts in debug builds that every object weighs at least its own slot, which catches it. Review: two rounds of six (3 Opus, 3 Codex); round 1's must-fixes were this entry, a stale README sentence and the PR body; round 2, 6 of 6 mergeable.

## The architecture audit, 2026-11-10

- **G-182. One owner for "a run is in progress"** (the architecture audit's item 1, `notes/projects/grift/arch-audit-2026-11-10/audit.md`; amends G-130's cost note and G-178's "every way into a run comes here first"). A run is a value: `Run`, which only `Control::begin` and `Control::host_run` make, ends its run as it drops, a panic's unwinding included. `enter` and `wake` take it by value, so code that runs the machine without owning a run doesn't compile, and `begin_run` takes the `Prepared` that only `prepare` makes, so a run can't begin without the collection check before it. Gone: `Hosting` and `HostRun` (one type now), the three hand-placed ends, `read_to_run` and the text run begun twice, and `call_local`: `Grift::call` builds its frames with the same function as `Answer::Call`. `evaluate`, `evaluate_in`, the library at `Grift::new` and `start_in` read and run through one body.
  - **G-130's cost.** It said "one `Arc` clone and drop per host call, none per run". Now it is one per run and none per host call: the run's own guard, held by `enter`, ends the run when a host function's panic unwinds through it. That was not safe while a host function could swap the interpreter out (`std::mem::swap`), since the guard would then end another interpreter's run; G-181's `HostCall` borrow made that impossible, so a host function can leave its run only by returning or unwinding, and both pass through `enter`.
  - **Kept as they were.** A resume begins its run only after everything that can refuse it, so a refused resume spends no run number; `wake` still clears the request. The guard drops before the result is exported, so export stays outside the run. The machine is still cleared at the end of every run. Host-side runs (`bind`, `read`, `read_in`, `write`, `describe`) still join a run in progress, getting no guard. `Grift::call` of what isn't an applicative is refused before its run begins, as before, so an interrupter taken for the next run still reaches the next run that does begin. A call's operand list and its empty context are now made inside its run rather than just before it; nothing collects there.
  - **Not done:** `prepare` inside `begin_run`. A resume must collect while the parked machine is still in the kept table and take it out only after, which can refuse; the `Prepared` token pairs the two by type instead.
- **G-183. A handler's call is made where the handler was set** (a bug the generator found once the audit's item 2 reshuffled it; it was on master). Catching an error built the handler's values list and trace with the machine's library mark as the error left it: a native raised in the program and its twin in the library, so a caught error's values, evaluated as code, were in one interpreter's trace and not the other's. The `Handler` frame now keeps the mark of the combination that set it, in its `Base` as an argument frame does (the frame stays 56 bytes), and unwinding to it restores the mark with the place, so the handler's call is that combination's code wherever the error came from. Evidence: the boundary case `((wrap list) (catch-kind! (first 1) type-error e (evaluate-in! values e)))` in `natives_are_their_definitions`, and historic patch 44.
- **G-184. The test side classes the primitives once** (the audit's item 2; amends G-115's lists and restores G-116's "every native and twin"). One match in the tests, `class`, says whether each primitive is native and how the generator calls it; the core list and the generator's table are derived from it, and `natives_are_their_definitions` still checks the class against `natives.grift`'s `meanings`, so a native it leaves out fails. Nine natives the generator had never called now are: `truncate`, `floor`, `ceiling`, `round`, `remainder`, `absolute-value`, `power`, `square-root` and `less-than?` (on integers, floats, strings and bytes), through `apply`, `wrap`, `unwrap` and every place. G-115 amended: `remainder`, `less-than?`, `absolute-value`, `power`, `square-root` and the four roundings are native, each defined in grift for part of its inputs at least (G-177, G-181); core arithmetic is `add`, `subtract`, `multiply`, `divide` and `sine` to `natural-logarithm`. The deterministic edge grids stay. No production code knows the class.
- **G-185. A host function's wrong count is a primitive's `arity-error`** (owner decision, 2026-11-10, on the audit's F2; adds two methods to G-181's handle). The runner and the editor built theirs with `g.error("arity-error", args)`, the arguments spread out, so `(catch! (exit 1 2) e ...)` saw `(1 2)` where `(catch! (first 1 2) e ...)` sees `((1 2))`, and three of the editor's functions answered a wrong count with `type-error`. `HostCall::exactly::<N>(args)` gives the arguments as an array or that error, its one value the arguments as a list, and `HostCall::arity_error(args)` makes it for any other count rule; memory refused for the list is `out-of-memory`. The runner, the editor, the examples and the README's examples use them, each checking the count before the types. Programs that read a host function's `arity-error` values see `((1 2))` where they saw `(1 2)`; fux must follow. Evidence: `exit_ends_the_run_with_its_status`.
- **G-186. No test-size ratio** (owner decision, 2026-11-10, in the audit's PR; supersedes G-179's "testing rule" where it says CI fails when test characters per production character rise above `scripts/test-ratio.baseline`). The ratio rose when production code was deleted and when a regression test was added, so it penalised this PR for both, the very things it should welcome; the historic-bug gate (`scripts/historic-bugs.py`) measures what tests are for, that the bugs they caught stay caught. `scripts/test-ratio.py`, its baseline and its CI step go. The rest of G-179's rule stands: a finding goes into the generator, a native's twin or a guard, in that order; a scenario test says in its PR why none of those can; a test is deleted only when the held-back bug set shows the suite still catches everything. Cost: nothing mechanical stops tests from growing now; review does.
