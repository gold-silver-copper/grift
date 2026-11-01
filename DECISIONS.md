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
  program being evaluated when the error happened. The library's own code is skipped (its
  source pairs are the heap's first slots, generation 0), and so is code built around a combiner
  value (as the library builds it), so `($if 1 2 3)` fails `in ($if 1 2 3)`, not inside `$if`.
  The machine keeps the location as a heap `Ref` in a register and in each frame.
- **G-23. Host state is lent, not owned.** `Grift<T>`: the host passes `&mut T` to each `eval`
  and `call`, and host functions get it as a separate argument beside `&mut Grift<T>`, so the
  borrow checker sees both borrows. No globals, no `RefCell`, nothing `'static`; `T` may itself
  borrow. `Grift` with no parameter is `Grift<()>`.
