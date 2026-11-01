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
  - The error value is the list `(code values at)`: `values` the list of the error's values, `at`
    the list of where it happened (G-29). A formals tree takes it apart:
    `($lambda ((code values at)) ...)`. No new type (G-9).
  - `interrupted` is never caught (G-17: the host's limit must hold, and a guard in a loop must
    not swallow C-g). **`out-of-memory` is never caught either:** it is the allocator's limit, the
    same kind of limit as the host's interrupt (G-5); a handler would need memory to run, so
    catching it invites a loop that fails the same way; and the host already gets it with the
    interpreter still usable, which is where recovery from a full heap belongs.
  - `guard` is a primitive with a frame of its own, beside `eval-protected`'s: the machine's
    unwinding has to stop somewhere, and only the machine can stop it.
  - Its handler is called as a tail call; a non-combiner `thunk` or `handler` is a `type-error`
    raised at once, not caught by the guard being made.
  - `(raise e)` raises a caught error again as it was, locations included. Added after the
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
