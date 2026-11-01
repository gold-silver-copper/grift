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
