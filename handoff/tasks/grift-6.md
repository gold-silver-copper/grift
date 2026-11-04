---
id: 01a1a561-c796-7ff3-aac6-67cce9ec0583
slug: tasks/grift-6
title: "grift: a super-minimal Lisp built on fexprs"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVV"
external_refs:
  - system: github
    id: "gold-silver-copper/grift#187"
    label: the PR, merged
    url: "https://github.com/gold-silver-copper/grift/pull/187"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift: a super-minimal Lisp built on fexprs

**Links:** [gold-silver-copper/grift#187](https://github.com/gold-silver-copper/grift/pull/187) (the PR, merged).

History, part of [tasks/grift-1](grift-1.md). Outcome: merged as https://github.com/gold-silver-copper/grift/pull/187. Run notes: box2 `~/code/notes/projects/grift/rewrite-2026-10-31/`.

## Brief (`prompts/rewrite.md`, box2 only, never committed; verbatim)


You own one PR in gold-silver-copper/grift (default branch `pure`) end to end: read the old code,
build the new grift from scratch, verify it in the real REPL, open the PR, run the review, fix,
report. Full ownership: change anything if it makes grift smaller, clearer or more correct. No
prototypes or spikes. Never ask the user anything: decide, write down why (ledger and PR), continue.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Base
`origin/pure` at `6b29d95`. Your worktree is `~/.cache/claude-tmp/grift-rewrite/wt` on branch
`rewrite`. Open PRs #185 (growable Vec arena) and #186 (`vau-core`, a pure vau rewrite) are earlier
attempts: read them for ideas and pitfalls, say "Supersedes #185 and #186" in the PR, never edit
them. The facts below were checked on the base; re-check what you rely on. Where you find better,
do it and say why in the PR.

Background worth reading (15 minutes, not more):
- the audit of `pure` and the direction: `~/code/notes/projects/grift/rewrite-2026-10-31/direction.md`;
- `vau-core`'s lessons: `git show origin/vau-core:README.md`, its `docs/architecture/ARCHITECTURE.md`
  (speed numbers), and `~/code/notes/projects/grift/PROMPT-fexpr-lisps.md` §0 "Lessons from grift";
- `05-07.pdf` (the Revised⁻¹ Report on Kernel) for names and derivations; `jshutt.pdf` for the model.

## Goal
grift is a tiny Kernel-style Lisp whose one special primitive is `$vau`. It is embedded in a Rust
program (the host), including on bare-metal devices. It uses the memory the allocator gives it,
reaches the outside world only through functions the host registers, and says what went wrong with
a code. Done means all of these hold:
- the language below, and nothing more;
- a desktop runner where you define a function at the REPL, redefine it, and call a host function;
- the core crate builds for `thumbv6m-none-eabi` (Cortex-M0), and a small firmware example runs a
  grift program on a Cortex-M board image (see "The chip");
- the size bands below, the gate, and the review.

## Already decided by the owner (do not reopen)
Recorded in this conversation on 2026-10-31; this file is the owner's yes. Write them into a new
`DECISIONS.md` (append-only from now on) as G-1 onward.
1. **Super minimal, based on fexprs.** Everything that can be grift code is grift code.
2. **Memory is a growable `Vec` heap** (`no_std` + `alloc`), not a fixed arena. grift sets no memory
   limit of its own: grow with `try_reserve`, and when the allocator refuses, return an
   `out-of-memory` error instead of aborting. Mark-and-sweep collection.
3. **Environments are mutable,** as in Kernel: `$define!` adds to the current environment. No purity
   machinery (no declarations, no uninitialized bindings).
4. **Errors go to the host** as `Err` with a code and the values involved. No catching inside grift,
   no continuations of any kind.
5. **Limits belong to the host and the OS.** No step limit, no time limit, no depth limit:
   `timeout` on a desktop, the hardware watchdog on a chip. (fux decision D-83, same reasoning.)
6. **Unix philosophy.** The runner reads files or stdin, prints data to stdout, diagnostics to
   stderr, and exits with a status. No line editor (`rlwrap grift` does that); no rustyline.
7. **Self-describing.** `(bindings env)` lists an environment's own names, and printing a combiner
   shows its parameters and body, so an agent never guesses what exists.
8. **No guessing.** No made-up defaults: `$if` needs both branches, `eval` needs an environment, an
   unknown symbol is an error carrying the symbol.
9. **Cut:** R7RS, numeric tower, floats, chars, vectors, ports, continuations, contracts, effect
   handlers, records, the enforced `$` sigil rule (it stays a naming convention), `grift-docs`,
   `publish.sh`, the lazy prelude, the old arena API, the stack-size workarounds
   (`.cargo/config.toml`, `opt-level = 1`). Keep `05-07.pdf`, `jshutt.pdf` and `quotes.md`.

## The language
**Values:** `()`, `#t`, `#f`, `#inert`, `#ignore`, checked `i64` integers, symbols (interned), pairs,
strings (immutable atoms), environments, operatives (fexprs), applicatives, host functions.

**Rust primitives** (about 20; every one but `$vau` is an applicative). Names follow Kernel:
- `$vau`: `($vau formals eformal body)`; formals is a tree of symbols, `#ignore`, `()` and pairs,
  checked when the operative is made (no duplicates);
- `wrap`, `unwrap`, `eval`, `make-environment` (zero or one parent), `bind!` (`(bind! env formals
  value)`, the same tree matching as `$vau`), `bindings`;
- `cons`, `car`, `cdr`, `eq?`, `select` (`(select bool x y)`, type error unless a boolean), `type-of`
  (returns a symbol: `pair`, `integer`, …);
- `+`, `-`, `*`, `div`, `<?` (binary, checked: `overflow`, `division-by-zero`);
- `error` (`(error code-symbol value ...)`).
Anything more needs a reason in a comment and in the PR (e.g. a string primitive the chip example
needs). If something proves underivable, add it and say why.

**The library** (grift code, one file embedded with `include_str!`, evaluated by `Grift::new`):
`$quote`, `list`, `$define!`, `$if`, `$sequence`, `$lambda`, `$let`, `$cond`, `$and?`, `$or?`,
`not?`, `=?`, `apply`, `map`, `append`, `length`, `equal?`. Write the boot order so each form only
uses what came before it; comment where a definition comes from R-1RK.

**Errors:** the codes are symbols: `unbound-symbol`, `type-error`, `arity-error`, `not-a-combiner`,
`bad-formals`, `overflow`, `division-by-zero`, `out-of-memory`, `parse-error` (with line and column),
plus any code a program passes to `error`. Every error carries the values involved.

**Evaluation:** an explicit machine loop; no Rust recursion that grows with the program's nesting
(reader, printer, `equal?`-style walks and GC marking included). Proper tail calls through `eval` in
tail position, so library `$if`, `$sequence`, `$cond` and `$let` loop in constant space. Garbage
collection only at safe points, with every live value reachable from the machine's own state, so no
code roots values by hand and a native can't see a collected value. Make that true by construction.

## The host API
`Grift::new() -> Result<Grift, Error>`, `eval(&mut self, src: &str) -> Result<Value, Error>` (all
forms; the environment persists across calls), writing a value to a `String`, and registering a host
function `fn(&mut Grift, &[Value]) -> Result<Value, Error>`. State in the PR how long a returned
`Value` stays valid and what stops misuse. No `LispOps`-style duplicate trait.

## The runner and the chip
- **Runner** (std, a separate binary target or crate; the core crate stays dependency-free):
  `grift FILE...` runs files; with no files it reads stdin, printing each top-level result when stdin
  is a terminal. It registers `display` and `newline`. Exit 0, 1 on a grift error (code and values
  on stderr), 2 on a usage error. A closed stdout ends it quietly.
- **The chip:** a firmware example outside the core crate (its own small crate, excluded from the
  default workspace build if that's simpler) for a Cortex-M board that QEMU emulates. It installs an
  allocator, embeds grift, registers one host function, evaluates a program that defines and
  redefines a function, and prints the results over semihosting. Keep it under about 100 lines.
  QEMU is not installed on this machine and needs sudo: build it, add `scripts/qemu.sh` that runs it,
  and report "QEMU run not verified" in the PR. Don't install system packages.

## Acceptance bands (accept without asking)
- Rust core, non-test code lines (no blanks, no comments): at most 1,500 (`pure` has 3,280).
  Library: at most about 300 lines. If a band is impossible without hurting correctness, revise it as
  CLAUDE.md says (original, evidence, new target, cost; listed in the PR).
- Speed is not a gate. Measure one recursive `fib 25` once on `pure` and once on the new grift, in
  the same place, release builds; report both. "Not benched" is fine for anything else.

## Rules for the code
- Core crate: `#![no_std]` + `extern crate alloc`, no dependencies, `#![forbid(unsafe_code)]`, edition
  2024. Non-test code must not panic: deny `clippy::unwrap_used`, `expect_used`, `panic`,
  `indexing_slicing`, `unreachable` in `[lints]` (allow them in tests).
- Tests are few and state properties, never frozen output: each primitive and library form, each
  error code, a 1,000,000-iteration tail loop through `$if`/`$sequence` in bounded heap, reading,
  printing and evaluating 100,000-deep nesting without a stack overflow, GC under sustained
  allocation, redefinition at top level, `bindings`, `out-of-memory` from a refusing allocator if you
  can make one without unsafe (else say how it was checked).
- Rewrite `README.md` (what grift is, the language on one screen, the host API, `rlwrap grift`) and
  one `docs/LANGUAGE.md` (primitive vs library table, error codes, deviations from R-1RK with
  reasons). Delete the old `docs/architecture/`. Update `.github/copilot-instructions.md` or delete
  it. CI: trigger on `pure`, run fmt, clippy `-D warnings`, tests, and the `thumbv6m-none-eabi`
  build. Keep `rust-version` only if you checked it; otherwise remove it and say so.
- Commit messages: plain English, what the program now does.

## Gates (exact commands, in the worktree)
```sh
export CARGO_TARGET_DIR=~/.cache/claude-tmp/grift-rewrite/target TMPDIR=~/.cache/claude-tmp/grift-rewrite/tmp
GATE_FMT=1 $S/rust-gate.sh origin/pure
cargo build -p grift --target thumbv6m-none-eabi -j 8
```
plus the firmware example's build. Gate once per push, not after every edit.

## Machine rules
Your worktree, `CARGO_TARGET_DIR` and `TMPDIR` above only. No /tmp, no git stash, nothing over 10
minutes or all cores (`-j 8`, `--test-threads 8`), kill what you start by pid (never `pkill -f`),
start helpers with `$S/launch-agent.sh` (it unsets `*_API_KEY`), never touch the user's sessions,
panes or config. Builds go through `$S/cargo-slot`.

## Review
Two reviewers on other models, since this is a whole rewrite: `$S/launch-agent.sh codex exec -s
read-only -m gpt-6.1-sol` and the same with `gpt-6-astra`, each in a detached review worktree at the
PR head, briefed with the base and head SHAs, this file's "Already decided" list, and no hoped-for
verdict; "no findings" is a fine answer. Never Fable. A must-fix needs a failing test, a reproduced
bug or a broken decision; anything else is a note. At most 2 rounds; round 2 checks only round 1's
must-fixes and what the fixes touched.

## Ledger and status
Ledger: `~/code/notes/projects/grift/rewrite-2026-10-31/ledger.md` (base, phase checklist, decisions
with why, incidents, not verified). Read it first if resumed. Keep a one-line status in
`~/code/notes/projects/grift/rewrite-2026-10-31/status` (what you're doing now). Course corrections
arrive as dated "Update" sections at the end of this file.

## Git and the PR
Commit only as the git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or
`--author`. No Co-Authored-By or any AI attribution in commits or the PR (the guard hooks check;
also run `$S/prepush-check.sh origin/pure` before pushing). Open the PR against `pure`. **Don't
merge:** the orchestrator who started you checks the REPL and merges. PR body: what changed for a
user; the host API before/after; a capability-changes table (old `pure` features and their fate);
line counts before/after (core, library, tests); the fib numbers; decisions you made alone; choices
left to the user; what was not verified; `cd ~/code/grift && cargo run …` to try it.

## At the end
When the PR is open, the gate passes at its head and the review is done, write
`~/code/notes/projects/grift/rewrite-2026-10-31/DONE` containing the PR URL, the head SHA and the
gate's result at that head. Report in plain words. Kill everything you started; leave the worktree.