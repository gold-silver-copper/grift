---
id: 01a1a561-d239-79e2-bb3e-aaa2101cd56a
slug: tasks/grift-11
title: "grift: speed, catching, strings — what twelve real programs showed"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVVVVV"
external_refs:
  - system: github
    id: "gold-silver-copper/grift#190"
    label: the PR, merged
    url: "https://github.com/gold-silver-copper/grift/pull/190"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift: speed, catching, strings — what twelve real programs showed

**Links:** [gold-silver-copper/grift#190](https://github.com/gold-silver-copper/grift/pull/190) (the PR, merged).

History, part of [tasks/grift-1](grift-1.md). Outcome: merged as https://github.com/gold-silver-copper/grift/pull/190. Run notes: box2 `~/code/notes/projects/grift/improve2-2026-11-01/`.

## Brief (`prompts/improve2.md`, box2 only, never committed; verbatim)


You own one PR in gold-silver-copper/grift (default branch `pure`) end to end: read the evidence,
change grift, prove it on the twelve programs, open the PR, run the review, fix, report. Full
ownership: change anything if it makes grift smaller, clearer, faster or more correct. No
prototypes or spikes except the one measurement in item 8. Never ask the user anything: decide,
write down why (ledger and PR), continue.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Base and evidence
`origin/pure` at `f280f5b`. Your worktree: `~/.cache/claude-tmp/grift-improve2/wt` on branch
`improve2`. Read first (30 minutes, not more):
- `README.md`, `docs/LANGUAGE.md`, `DECISIONS.md` (G-1..G-25), `crates/grift/src/`.
- On 2026-11-01 twelve agents wrote four programs three ways each without changing grift: a Kernel
  evaluator in grift (`eval-direct`, `eval-callcc`, `eval-stepper`), a logic engine
  (`logic-closures`, `logic-stack`, `logic-streams`), symbolic algebra (`algebra-rules`,
  `algebra-plain`, `algebra-exact`), a jq-like JSON filter (`json-codes`, `json-combinators`,
  `json-strprims`). **Start with `~/code/notes/projects/grift/apps-2026-11-01/tally.md`** (the
  ranked findings, every claim cited); each program's `report.md` and `friction.md` are beside it.
  Their code is on local branches `app-<name>` in this repository, under `apps/<name>/`, each with
  its own run or test script.
Re-check each finding on the base before acting on it; a finding that doesn't reproduce is dropped
with a line in the ledger.

## Owner decisions, 2026-11-01 (do not reopen; write them into DECISIONS.md as G-26 onward)
1. **Catching (amends G-4):** one primitive, `(guard thunk handler)`: it calls `thunk` with no
   arguments; if an error leaves it, the `$unwind-protect` cleanups inside run first, then
   `handler` is called with the error as a value (code, values and location, readable from grift;
   your representation) and its result is `guard`'s result. Errors still go to the host when nothing
   catches them. `interrupted` is never caught (G-17: the host's limit must hold; a `guard` in a
   loop must not swallow C-g); decide `out-of-memory` with a reason. No continuations of any kind.
2. **Strings (amends G-9's opaque strings; they stay immutable):** `string-length` and
   `string-byte` (byte offsets, O(1)), `substring` (a byte range, an error off a character
   boundary), `string-append`, `code-point->string`, `symbol->string`, `string->symbol`,
   `number->string`. This is json-strprims' set (tally §4) plus the symbol and number conversions
   six programs needed. Anything more (`string<?`, `string->number`, …) only with evidence from the
   twelve programs, recorded.
3. **Speed:** fix what the evidence shows (items 1–3 below) in this PR. Making `$if` and
   `$sequence` Rust primitives would bend G-1: **measure it only** (item 8); it is not part of the PR.

## Goal: done means all of these hold
Each item has a test stating the property (not frozen output).
1. **`$cond`, `$let` and every other library form stop rebuilding code on each use** (tally need
   1: 10 of 12 programs; a 3-clause `$cond` cost 18–32 µs against under 1 µs for nested `$if`).
   Check every form in `library.grift` for the same pattern. Measure one `$cond`-heavy and one
   `$let`-heavy program before and after.
2. **`$binds?`** (Kernel's; tally need 5, 6 of 12).
3. **Name lookup:** measure where lookup time goes on two of the programs (tally need 6: up to 30%
   in logic-stack). Change the representation only if the whole-program gain is clear; record the
   numbers either way.
4. **Errors say where in the program they came from,** including an `(error …)` raised inside the
   program's own helper functions (tally need 7: today it points at the `error` call, not the
   caller). Your design (e.g. the chain of active program combinations); keep `Error` small and
   unbounded growth out (tail calls must stay constant-space).
5. **`guard`**, per owner decision 1, with `docs/LANGUAGE.md` explaining it beside
   `$unwind-protect`.
6. **The strings**, per owner decision 2.
7. **The twelve programs on the new grift.** For each: extract it from its branch into your
   scratch space (`git archive app-<name> apps/<name> | tar -x -C …`, under your `TMPDIR`), run its
   own test or run script against the new grift unchanged, and record pass/fail and its hot-path
   time before (base) and after (head), release builds, one run each, in one table. A program that
   breaks because of an intended change (e.g. it defined its own `$binds?` or `guard`) is fixed
   minimally and noted. Then **port three** to the new features and report the lines removed:
   `eval-direct` (`guard`, `$binds?`), `json-strprims` (its Rust string functions replaced by the
   core primitives: report the Rust lines removed), `algebra-rules` (symbol ordering through
   `symbol->string`). Commit each port on a new local branch `app-<name>-v2` (its own worktree under
   your folder; no push).
8. **Measurement only:** on a throwaway local branch, make `$if` and `$sequence` Rust primitives
   and time the twelve programs' hot paths against the PR head. Put the table and the core line cost
   in the PR under "For the owner: primitive `$if`", with your recommendation. Don't include it in
   the PR; delete the throwaway branch at the end.
9. **The editor** (`examples/emacs`) still meets its finish line (`prompts/emacs.md`): drive it in
   your own fux tab (`fux help agents` first; a workspace you name and close at the end), including
   the C-g interrupt inside a `guard` (C-g must still stop `(guard ($lambda () (spin)) …)`).

## Acceptance bands (set before the work; accept without asking)
- Rust core, non-test code lines (no blanks, no comments): at most 1,750 (base: 1,470; the owner's
  two additions need room). Library: at most about 300 (base: 119).
- Speed: `fib 25` and the twelve hot paths no slower than the base by more than 5% (one run each, a
  second run only to confirm a regression). Items 1–3 are expected to make most of them faster.
- If a band can't be met without hurting correctness, revise it as CLAUDE.md says (original target,
  evidence, new target, cost; listed in the PR).

## Rules for the code
As in `DECISIONS.md` and the crate's `[lints]`: `no_std` + `alloc`, no dependencies in the core,
`forbid(unsafe_code)`, non-test code must not panic. Tests are few and state properties. Update
`README.md` (with before/after code for any host API change) and `docs/LANGUAGE.md` (new primitives
and forms, error codes, R-1RK deviations). Commit messages: plain English, what the program now does.

## Gates (exact commands, in the worktree)
```sh
export CARGO_TARGET_DIR=~/.cache/claude-tmp/grift-improve2/target TMPDIR=~/.cache/claude-tmp/grift-improve2/tmp
GATE_FMT=1 $S/rust-gate.sh origin/pure
cargo build -p grift --target thumbv6m-none-eabi -j 8
(cd examples/emacs && cargo clippy --all-targets -- -D warnings)
```
Gate once per push, not after every edit.

## Machine rules
Your worktree, `CARGO_TARGET_DIR` and `TMPDIR` above only (the port worktrees and the throwaway
branch under the same folder, each with its own target dir). No /tmp, no git stash, nothing over
10 minutes or all cores (`-j 8`, `--test-threads 8`), kill what you start by pid (never
`pkill -f`), start helpers with `$S/launch-agent.sh`, never touch the user's sessions, panes or
config. Builds go through `$S/cargo-slot`. Timings: run them when `uptime` shows load under 16 and
say the load beside each table.

## Review
Two reviewers: `$S/launch-agent.sh codex exec -s read-only -m gpt-6.1-sol` and an Opus reviewer
(`$S/launch-agent.sh claude -p --model claude-opus-5-5 --permission-mode plan`). **Never Astra, never
Fable.** Each in a detached review worktree at the PR head, briefed with the base and head SHAs,
DECISIONS.md, this file's owner decisions and closed list, and no hoped-for verdict; "no findings" is
a fine answer. A must-fix needs a failing test, a reproduced bug or a broken decision; anything else
is a note. At most 2 rounds; round 2 checks only round 1's must-fixes and what the fixes touched.

## Ledger and status
Ledger: `~/code/notes/projects/grift/improve2-2026-11-01/ledger.md` (base, checklist, decisions with
why, measurements, incidents, not verified). Read it first if resumed. One-line status in
`~/code/notes/projects/grift/improve2-2026-11-01/status`. Course corrections arrive as dated
"Update" sections at the end of this file.

## Git and the PR
Commit only as the git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or
`--author`. No Co-Authored-By or any AI attribution in commits or the PR (the guard hooks check;
also run `$S/prepush-check.sh origin/pure` before pushing). Open the PR against `pure`. **Don't
merge:** the orchestrator checks and merges. PR body: what changed for a grift programmer and an
embedder; before/after code; the closed list with each item's test and evidence; the twelve-program
table (pass/fail, before/after times, load); the three ports' line deltas; "For the owner: primitive
`$if`"; line counts before/after (core, library, tests); decisions made alone; choices left to the
user; what was not verified; `cd ~/code/grift && …` commands to try it.

## At the end
When the PR is open, the gate passes at its head and the review is done, write
`~/code/notes/projects/grift/improve2-2026-11-01/DONE` with the PR URL, the head SHA and the gate's
result at that head. Report in plain words. Kill everything you started; leave the worktree.

## Update 2026-11-01 (owner): grift stays a pure fexpr Lisp
Item 1 means rewriting the library forms, not changing what they are. `$cond`, `$let`, `$let*`,
`$when`, `$and?`, `$or?`, `$lambda` and the rest stay plain `$vau` operatives (or `wrap`ped ones)
that receive their operands unevaluated and walk them with `car`/`cdr`, calling `eval` only on the
operands they were given, in the caller's environment, as R-1RK's derivations do. They stop
building new source (`(cons $cond …)`, `(cons $sequence …)`, `$let` expanding to a `$lambda` call)
only to `eval` it. Not allowed: caching or memoizing expansions, any compile or pre-processing step,
turning operands into another form ahead of time, or new Rust primitives for these forms (item 8's
primitive `$if`/`$sequence` is a measurement on a throwaway branch only). Redefining `$if`, `eval`
or anything a form uses must still take effect on the next call.

## Update 2026-11-01 (owner): std allowed, still zero dependencies; hashed environments
1. **The core may use `std`** (amends G-2's `no_std` + `alloc`, and G-25's Cortex-M0 build; record
   it as an owner decision in DECISIONS.md). Drop `#![no_std]` and `extern crate alloc`, use `std`
   paths, and remove the `thumbv6m-none-eabi` build from the gate, CI and README. Still: **zero
   dependencies**, `forbid(unsafe_code)`, no panics, and the core still does no I/O, starts no
   threads and keeps no global state (the host owns all of that). Out of memory is still refused,
   never an abort: every growing collection goes through `try_reserve`. Where `std` makes something
   simpler, use it (e.g. `Error` implementing `std::error::Error` and `Display`; the interrupt flag
   as an `Arc<AtomicBool>` if that is simpler for hosts than `&'static`, your call with a reason).
   One commit for the switch itself, separate from the rest.
2. **Item 3, if you change environment lookup, use this design:** an environment keeps its bindings
   in definition order (a `Vec`, as now, so `bindings` stays deterministic) plus a
   `std::collections::HashMap<Symbol, index>` built only once it grows past a small size (measure
   the threshold; most call frames have 1–3 names and a scan beats hashing there), grown with
   `try_reserve`, with a trivial hasher for symbol ids (no dependency). The ground and top-level
   environments are the ones every global lookup ends in: measure them first. To programs nothing
   changes but speed.

## Update 2026-11-01 (owner): every environment is a hash map
This replaces part 2 of the previous update. **Every environment's bindings are a
`std::collections::HashMap<Symbol, Value>`**, no matter its size: one representation, no threshold,
no Vec beside it. Grown with `try_reserve`; a trivial hasher for symbol ids (no dependency).
`bindings` (and `ground-names`) list names **sorted by name**, so output stays deterministic
without keeping definition order. Measure `fib 25` and the twelve hot paths against the previous
representation once and report the difference in the PR; this is decided, so a cost is reported,
not a reason to revert (if it is large, say what causes it and what would reduce it). Record it in
DECISIONS.md as an owner decision.

## Update 2026-11-01 (owner): a simple, uniform implementation; no fast paths
The owner wants the implementation fairly simple and consistent, with no special-case
optimizations. Concretely:
- **Remove the "operands evaluated in place" change** (`next_arg`'s loop over `at_once`, and looking
  up and combining a symbol in combiner position at once): every operand and every combination is
  evaluated by the same machine step, one path for each kind of thing. Same for anything similar
  you added: no fast paths, no duplicate code paths that exist only for speed, no per-form special
  cases in the machine.
- What stays: the library forms rewritten to walk their operands (that is simpler grift, not an
  optimization), the hash-map environments (one representation), and fixes to real waste.
- Speed bands: measure again after removing the shortcut. If `fib 25` or a hot path ends up more than
  5% slower than the base, report it with the cause instead of adding a shortcut back; the owner
  prefers a simple machine.
Add to DECISIONS.md as an owner decision: the evaluator has one path per kind of form; speed comes
from simpler library code and better representations, never from fast paths.

## Update 2026-11-01 (owner): what a caught error is, and what can't be caught
1. **The handler of `guard` receives the error as an environment** (records are environments),
   with `code`, `values` and `at` bound in it; a fresh one per caught error. Not a list. Show
   reading it in docs/LANGUAGE.md (e.g. `($remote-eval code e)`, or whatever library accessor you
   add, with `$remote-eval` defined as in R-1RK if you need it).
2. **The rule for what can be caught:** errors about the program can be caught (including errors
   from host functions); limits set by the host or the machine, `interrupted` and
   `out-of-memory`, are never caught by `guard` and always reach the host. Test both.
3. **Early exit is `guard` plus the program's own error code;** no separate escape primitive. Say
   so in docs/LANGUAGE.md.
4. Record these as owner decisions in DECISIONS.md, with the reason there are no continuations:
   generators and backtracking are closures or lazy streams (the three logic engines); a language
   built on grift that wants `call/cc` writes its evaluator in continuation-passing style
   (eval-callcc: 5 lines, flat memory, thanks to proper tail calls); continuations would cost the
   meaning of cleanup and of interruption, and the simplicity of the machine.