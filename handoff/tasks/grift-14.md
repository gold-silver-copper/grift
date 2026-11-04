---
id: 01a1a561-db79-7df1-a97a-9ffd2c0d5e0c
slug: tasks/grift-14
title: "grift: `if!` requires its else again, `when!` for one branch, a simpler `conditional!`"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVVVVVVVV"
external_refs:
  - system: github
    id: "gold-silver-copper/grift#192"
    label: the PR, merged
    url: "https://github.com/gold-silver-copper/grift/pull/192"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift: `if!` requires its else again, `when!` for one branch, a simpler `conditional!`

**Links:** [gold-silver-copper/grift#192](https://github.com/gold-silver-copper/grift/pull/192) (the PR, merged).

History, part of [tasks/grift-1](grift-1.md). Outcome: merged as https://github.com/gold-silver-copper/grift/pull/192. Run notes: box2 `~/code/notes/projects/grift/when-2026-11-01/`.

## Brief (`prompts/when.md`, box2 only, never committed; verbatim)


You own one PR in gold-silver-copper/grift (default branch `pure`) end to end. Owner decision,
2026-11-01 (do not reopen): **`if!` requires both branches again; a one-branch form `when!` takes
a test and a body; `conditional!` is rewritten more directly.** This reverses G-46/G-56's optional
else (record the reversal in DECISIONS.md, citing the evidence: `fib 25` 0.13 s -> 0.21 s from the
optional else, PR #191; Racket and Kernel require the else; every Lisp has `when`). Never ask the
user anything; decide technical details yourself and write down why.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Base
`origin/pure` at `0ac17f4`. Worktree `~/.cache/claude-tmp/grift-when/wt`, branch `when`. Read
`docs/VISION.md`, `docs/LANGUAGE.md`, `DECISIONS.md` (G-36..G-70), `crates/grift/src/library.grift`.

## Done means all of these hold
1. `if!` is one operative with a fixed parameter list `(test consequent alternative)`, as the old
   `$if` was: anything else is an `arity-error`. The test must be a boolean (no truthiness).
2. `when!`: `(when! test body ...)` evaluates the body forms in order, the last as a tail call, when
   the test is `#true`; gives `#inert` when it is `#false`; a non-boolean test is a type error.
   No `unless!` (the growth rule).
3. `conditional!` is rewritten more directly: no helper call that makes a new context per clause,
   no repeated checks; it walks the clauses in the caller's context with the fewest combiner calls
   you can manage while staying simple, uniform library grift (no caching, no compile step, no fast
   path in the machine, no new primitive). Same meaning as now. Look at `let!`, `finally!`,
   `catch!` and `run` while there, since they share helpers, and simplify where it is plainly
   simpler.
4. Every use of a one-armed `if!` (library, runner's `runner.grift`, tests, `examples/emacs`, docs,
   README) becomes `when!` or gets its else. `docs/VISION.md` (Values, the `if!` mentions),
   `docs/LANGUAGE.md` and DECISIONS.md say the new rule.
5. Tests state the properties: `if!` arity, `when!` both ways and its tail call (a 1,000,000-step
   loop through `when!` in bounded heap), `conditional!` unchanged in meaning (first true clause,
   `#inert` when none, non-boolean test is a type error, body of several forms).
6. Speed (one run each, release, say the load): `fib 25` on the base and the head, and a
   `conditional!`-heavy loop against the same loop as nested `if!`s, both before and after. Expect
   `fib 25` back near 0.13 s; report what you get.
7. The editor (`examples/emacs`) still meets its finish line; drive it briefly in your own fux tab
   (`fux help agents` first; a workspace you name and close at the end).

## Rules, gates, machine
As DECISIONS.md and the crate's `[lints]` say. Gates, in the worktree:
```sh
export CARGO_TARGET_DIR=~/.cache/claude-tmp/grift-when/target TMPDIR=~/.cache/claude-tmp/grift-when/tmp
GATE_FMT=1 $S/rust-gate.sh origin/pure
(cd examples/emacs && cargo clippy --all-targets -- -D warnings)
```
Only your worktree, target and tmp dirs; no /tmp, no git stash, `-j 8`, nothing over 10 minutes,
kill what you start by pid (never `pkill -f`), never touch the user's sessions or panes.

## Review
One reviewer: `$S/launch-agent.sh codex exec -s read-only -m gpt-6.1-sol` in a detached review
worktree at the PR head, briefed with base and head SHAs, this file and no hoped-for verdict. Never
Astra or Fable. A must-fix needs a failing test, a reproduced bug or a broken decision. At most 2
rounds.

## Git, PR, end
Commit only as the git-config identity; never `-c user.*` or `--author`. No Co-Authored-By or any
AI attribution (run `$S/prepush-check.sh origin/pure` before pushing). PR against `pure`; **don't
merge**. PR body: what changed, before/after code, the closed list with tests, the speed numbers,
line counts, decisions made alone, what was not verified, a `cd ~/code/grift && …` command. Status
line in `~/code/notes/projects/grift/when-2026-11-01/status`; at the end write `DONE` there with the
PR URL, head SHA and the gate's result. Kill everything you started; leave the worktree.