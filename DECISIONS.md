# Decisions

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
  `define-in!` goes for `bind` with the parameters quoted, although G-38 listed it: 4 programs and the
  editor used it, 8 programs and 2 agents' stated choices used `bind`, and it was `bind` with one operand
  evaluated. `binds!?` goes for `binds?`: its only callers were a test program's tests of it. `inert?`
  and `ignore?` go for `same?`: each type has one value (`empty-list?` has the same shape and stays, as
  G-38's own example, in 8 programs). `float-to-string` goes for `value-to-string`, which G-78 made the
  same text. `newline` goes for `(display "\n")`. Kept although few use them: the float functions (G-42),
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
  symbols the core names come from one table. Core Rust: 1,949 to about 1,700 lines, no change in `fib 25`.
- **G-91. `evaluate-with-cleanup` stays beside `call-with-handler`** (G-48). `finally!` over a handler
  that also saw limits could drop a primitive and two frames, but each cleanup would then copy the trace
  into a record and back, which G-76 removed, and every `catch!` would test for limits in grift.
- **G-92. The editor's conversions between strings and code points are grift** (G-57). Its host gives bytes
  and files only; its file loading uses `string-split`, and decoding is per line.
