---
id: 01a1a561-ea40-75a3-8ab5-fa0d72134bd2
slug: tasks/grift-18
title: "grift: what wave 4 earned — the owner's two rulings, the four open issues, and the helpers every program wrote"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVVVVVVVVVVVV"
external_refs:
  - system: github
    id: "gold-silver-copper/grift#195"
    label: the PR, merged
    url: "https://github.com/gold-silver-copper/grift/pull/195"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift: what wave 4 earned — the owner's two rulings, the four open issues, and the helpers every program wrote

**Links:** [gold-silver-copper/grift#195](https://github.com/gold-silver-copper/grift/pull/195) (the PR, merged).

History, part of [tasks/grift-1](grift-1.md). Outcome: merged as https://github.com/gold-silver-copper/grift/pull/195. Run notes: box2 `~/code/notes/projects/grift/earned4-2026-11-02/`.

## Brief (`prompts/earned4.md`, box2 only, never committed; verbatim)


You own one PR in gold-silver-copper/grift (default branch `pure`) end to end. Ten agents wrote 196
programs (29,478 lines) in grift at 699c95b (wave 4); the evidence is ranked in
`~/code/notes/projects/grift/apps4-2026-11-02/tally.md` (read all of it; sections 3, 4, 5 and 10 are the
work). The agents' own `report.md`/`friction*.md` beside it have the reproductions, and their programs
are on the branches `app4-<domain>` in `~/code/grift` (`apps/<domain>/`). The owner chose the items below
on 2026-11-02; **do not reopen them**. Technical choices inside them (names under VISION "Names",
signatures, library or primitive by the growth rule, Rust structure) are yours: decide, write down why
(ledger and PR), continue. Never ask the user anything. No prototypes or spikes.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Base
`origin/pure` at the head given in your launch message (699c95b or later). Worktree
`~/.cache/claude-tmp/grift-earned4/wt`, branch `earned4`. Read first: `docs/VISION.md` (all; "Names",
"State", "The runner", "What complete means", "How grift grows" are your rules), `docs/LANGUAGE.md`,
`DECISIONS.md` (all entries; owner decisions stand, entries "made alone" by agents may be revised with
evidence), `crates/grift/src/`, `crates/grift-cli/`, `programs/`, `scripts/census.py`, `examples/emacs/`.

## Owner rulings (2026-11-02), recorded as DECISIONS.md entries citing the tally
1. **`absolute-value` and `power` take an integer or a float**, as `add`, `minimum` and `maximum` do:
   two integers give an integer, two floats a float, never mixed. This amends G-63 (an agent decision;
   8 agents wrote integer absolute value by hand, 13 copies, and it was the commonest wrong guess).
   No typed variants (`absolute-value-integer` and the like): one name per operation, as everywhere
   else in grift's arithmetic. `(absolute-value <most negative integer>)` raises `overflow`; `power`
   with an integer exponent below zero on integers is an error that says so (your choice of kind,
   by the existing kinds). Library grift unless measured unusable.
2. **`define!` keeps binding in the current context** (Kernel's `$define!`, Scheme's internal
   `define`): inside a function body it makes a local. No `set!`, no warning on shadowing. Changing an
   outer binding means holding its context and binding there, which follows VISION "State" ("you can
   change only a context you hold"). Document it: LANGUAGE.md beside `define!` shows the pitfall and
   the idiom; VISION "State" says in one sentence why there is no `set!`. The idiom:
   ```lisp
   (define! counter 0)
   (define! here (current-context))
   (define! bump! (lambda! () (bind here (quote! counter) (add counter 1))))
   ```

## Done means all of these hold
Each item gets a test that states a property (few tests), an entry in LANGUAGE.md, and the
before/after a program would write in the PR body.

**The four open issues (K1-K4, tally section 3):**
1. **K1, `parse-error` says why.** It names the reason (an unclosed list, an unclosed string, an
   unexpected `)`, a bad escape, a bad number and the like: the reader's real cases, as error data, not
   prose only) and, for an unclosed list or string, where it **opened**, not the end of the file (today
   an unclosed form is `parse-error 7 1`). Same for `read-form` and the runner's message.
2. **K2, `load` and the script's path.** A program can run another file's forms relative to its own
   file, and knows its own path. Decide the shape (a runner operation, its context: the caller's or a
   new child; what a second `load` of the same file does; how its errors name the file and line) and
   write why. ~2,400 lines were copied between programs for lack of it; numeric's bignum (346 lines)
   couldn't be shared at all.
3. **K3, catch one kind.** Catching one kind of error without the `catch!` + `if!` + re-raise
   pattern (147 times, ~521 lines). Library grift over `catch!` unless that can't keep the trace of
   a re-raised error intact; the other kinds pass through untouched, with their traces as they were.
4. **K4, bytes that aren't UTF-8.** A program can read a file or standard input that isn't UTF-8 and
   write any byte 0-255 (Brainfuck's `-.`, base64 decode of binary, `head -c 1` on `é`, `cat` of a
   binary file: tally section 2). Strings stay UTF-8 text. Choose the representation by the evidence
   (a list of integers, a new value, operations on a file context...) and say why against "Values"
   in VISION; no vectors unless you show nothing smaller works.

**Library (tally section 10, library 1-4):**
5. **`sort`**, stable, `(sort list before?)`, fast enough for 10,000 elements (state the time), and
   **`string-less-than?`** in byte order. This reopens G-75's "sort, not now" with ten agents' evidence.
6. **List access:** `element-at`, `take`, `drop`, `last`, `second`, `third` (435 uses of
   `(first (rest x))`). An index out of range is an error carrying the index, never a guess.
7. **`string-repeat`**, and a fill string on the pads: `(string-pad-left "7" 2 "0")` gives `"07"`.
8. **Ruling 1** above (`absolute-value`, `power`).

**Core (tally section 10, core 1-2):**
9. **A lookup that doesn't raise on a miss.** Today `binds?` is a caught error, 5-14 µs a probe
   against ~0.6 µs for a lookup (9 agents). Either `binds?` becomes the primitive (no new name) or a
   `(lookup context symbol otherwise)` primitive with `binds?` in the library over it; pick one, not
   both. Report the tally's probe timing before/after, and algorithms' sieve.
10. **Bitwise operations on integers** (6 agents; a hand-written `and` costs 173 µs): and, or,
    exclusive or, shifts left and right, named by the rules (spelled out; agents proposed
    `bitwise-and`, `bitwise-or`, `bitwise-exclusive-or`, `shift-left`, `shift-right`). Rust's own
    semantics for checked integers: a shift by 64 or more, or one that loses bits a checked operation
    would refuse, raises; say which you chose and why.

**Runner (tally section 10, runner 2-4):**
11. **`display-error`**: `display`'s values, written to standard error (~560 lines in 128 programs).
    Before: `((evaluate-in! write standard-error) (string-append "wc: " name ": " (integer-to-string n) "\n"))`;
    after: `(display-error "wc: " name ": " n "\n")`.
12. **`read-file`**: a file's whole text in one call (67 copies; wave 3 had 6 more). Its bytes
    counterpart if item 4 needs one.
13. **A clock**: the current time (wall clock, as an integer of a unit you choose and state) and a
    monotonic one for timing if one value can't serve both; a seed for a pseudo-random generator can
    come from it. No random generator in the runner: with item 10 it is library-sized grift in a
    program.

**Docs and evidence:**
14. **Ruling 2** in LANGUAGE.md and VISION.md; the tally's section 10 "Docs, no change" items in
    LANGUAGE.md where they're true: a context of combiners as the fast dispatch; symbols are never freed;
    the library forms' cost against `if!` (with today's measured numbers).
15. **The census**: `scripts/census.py`'s table over `programs/`, `examples/emacs/`, the library and the
    runner, before and after, in the PR.
16. **Evidence the items pay off:** in scratch worktrees of your own (not in the PR), port three wave-4
    programs that exercise most of the items (suggestions: unix's `head`/`tail` for K4 and
    `display-error`, algorithms' sieve for item 9, numeric's statistics or text's base64), and report
    lines before/after, timing before/after (one run each, both built here) and their `test.sh`
    passing. Every program in `programs/` still passes, using new names where they read better (each
    such change listed).
17. **The editor** (`examples/emacs`): drive it briefly in your own fux tab (`fux help agents` first;
    a workspace you name and close at the end): M-: with `sort` and a bitwise operation, an unclosed
    form showing K1's reason, C-g in a loop inside `catch!`, save.

## Acceptance bands (set before the work; accept without asking)
- Core Rust, non-test lines: grows by at most 350. Library: grows by at most 200 lines. Runner (Rust
  and grift together): grows by at most 250.
- `fib 25` within 5% of the base (one run each, both built here). Each `programs/` program's own
  timing within 10% of the base or faster.
- A band that can't be met without hurting correctness is revised as CLAUDE.md says (original target,
  evidence, new target, cost; listed in the PR).

## Not in this PR
The embedding API asks (tally section 8), `unbind` (changes VISION "State": the owner's call; list it
under "choices left to the owner" with its evidence), a radix on integer text, `\r`, `string-to-integer`
that doesn't raise, byte classes, `record`, `vau!` with several bodies, float fixed decimals, file
append/kind/size, SIGINT reporting, `run-process` input, trace shortening, sandboxing (G-74). Note any
you find newly pressing in the PR. If an item above makes an existing name redundant, removing it is
in scope (with the census showing it, listed old -> new).

## Rules, gates, machine
As DECISIONS.md and the crate's `[lints]` say: zero dependencies, `std`, `forbid(unsafe_code)`, no
panics in non-test code, no I/O in the core, `try_reserve` for growth, one evaluation path per kind
of form, no fast paths, library forms stay pure fexprs (no caching or expansion). Tests are few and
state properties. Gates, in the worktree, once per push:
```sh
export CARGO_TARGET_DIR=~/.cache/claude-tmp/grift-earned4/target TMPDIR=~/.cache/claude-tmp/grift-earned4/tmp
GATE_FMT=1 $S/rust-gate.sh origin/pure
(cd examples/emacs && cargo clippy --all-targets -- -D warnings)
```
Only your worktree, target and tmp dirs (scratch worktrees under `~/.cache/claude-tmp/grift-earned4/`,
each with its own target dir); no /tmp, no git stash, `-j 8` (`-j 4` if the load average is over 20),
builds through `$S/cargo-slot`, nothing over 10 minutes, kill what you start by pid (never `pkill -f`),
never touch the user's sessions or panes. Helpers that write code run Opus; split work by files if you
use any (e.g. core, library, runner).

## Review
Two reviewers, each in a detached review worktree at the PR head, briefed with base and head SHAs,
this file and no hoped-for verdict ("no findings" is a fine answer):
`$S/launch-agent.sh codex exec -s read-only -m gpt-6.1-sol` and an Opus reviewer
(`$S/launch-agent.sh claude -p --model claude-opus-5-5 --permission-mode plan`). **Never Astra, never
Fable.** A must-fix needs a failing test, a reproduced bug, a broken decision, a `programs/` program
that no longer runs, or a name against the rules. At most 2 rounds; round 2 checks only round 1's
must-fixes and what the fixes touched.

## Ledger, status, end
Ledger `~/code/notes/projects/grift/earned4-2026-11-02/ledger.md` (base, checklist, decisions with why,
incidents, not verified); read it first if resumed. One-line status in `.../status` beside it. Course
corrections arrive as dated "Update" sections at the end of this file.

Commit only as the git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or
`--author`. No Co-Authored-By or any AI attribution (run `$S/prepush-check.sh origin/pure` before
pushing). PR against `pure`; **don't merge**. PR body: what changed for a grift programmer, a script
author and an embedder, with before/after code for each item (a wave-4 workaround and its replacement);
the closed list with each item's test; the ported programs' numbers; the census before/after; line
counts before/after (core, library, runner, tests, docs); `fib 25`, the `binds?` probe and the corpus
timings before/after; decisions made alone; choices left to the owner; what was not verified;
`cd ~/code/grift && …` commands to try it. When the PR is open, the gate passes at its head and the
review is done, write `DONE` beside the status with the PR URL, head SHA and the gate's result. Kill
everything you started; remove your scratch worktrees; leave the main one.

## Update 2026-11-02 (after launch): `unbind` is in (the owner)
Owner ruling 3, recorded as a DECISIONS.md entry citing the tally (section 5 #23: structures F2,
embedding F1; a context-based hash map 10x slower than a plain context because it can't delete):
3. **`(unbind context symbol)`**, the mirror of `bind`: both evaluated; removes the binding from that
   context itself, never a parent's. Gives `#true` if a binding was removed, `#false` if the symbol
   wasn't bound there (deleting what may be absent is the common table case, and `binds?` looks
   through parents so it can't pre-check). After it, a lookup in that context sees a parent's
   binding of the name if there is one: one line in LANGUAGE.md. Closures and library names get no
   protection (G-74).
   VISION "State" changes from "a context gains a binding or has one replaced" to "gains, replaces or
   loses a binding"; the rest of "State" stands.
   It is a primitive beside `bind` (a library version can't exist): a context's bindings are a
   `HashMap` (`heap.rs` `Bindings`), so it is `remove`, and the removed value becomes collectable.
   Test: removed, the value collected (heap count or a `finally!`-free property you choose),
   a parent's binding revealed, `#false` on absent. Report structures' hash map (branch
   `app4-structures`) rewritten to delete instead of marking, timing before/after (one run each).
   Note in LANGUAGE.md beside it that symbols themselves are never freed (the symbol table only
   grows), so a table keyed by computed symbols still grows the symbol table.
   The core band grows by 30 lines for this.