---
id: 01a1a561-decd-7071-9d13-0dbf5be2a2bc
slug: tasks/grift-15
title: "grift: what wave 2 earned — a reader, a printer to a string, the helpers, `standard-error`, linear unwinding"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVVVVVVVVV"
external_refs:
  - system: github
    id: "gold-silver-copper/grift#193"
    label: the PR, merged
    url: "https://github.com/gold-silver-copper/grift/pull/193"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift: what wave 2 earned — a reader, a printer to a string, the helpers, `standard-error`, linear unwinding

**Links:** [gold-silver-copper/grift#193](https://github.com/gold-silver-copper/grift/pull/193) (the PR, merged).

History, part of [tasks/grift-1](grift-1.md). Outcome: merged as https://github.com/gold-silver-copper/grift/pull/193. Run notes: box2 `~/code/notes/projects/grift/grow-2026-11-01/`.

## Brief (`prompts/grow.md`, box2 only, never committed; verbatim)


You own one PR in gold-silver-copper/grift (default branch `pure`) end to end. Twelve programs were
written in today's grift (wave 2); their evidence is ranked in
`~/code/notes/projects/grift/apps2-2026-11-01/tally.md` (read sections 4, 5 and 7; the agents' own
`report.md`/`friction.md` beside it have the reproductions). The owner chose the items below on
2026-11-01; **do not reopen them**. Technical choices inside them (names under the naming rules,
signatures, Rust structure) are yours: decide, write down why (ledger and PR), continue. Never ask
the user anything. No prototypes or spikes.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Base
`origin/pure` at `27c2aa4`. Worktree `~/.cache/claude-tmp/grift-grow/wt`, branch `grow`. Read
`docs/VISION.md` (all of it, especially Names, Embedding, Errors, Printing, How grift grows),
`docs/LANGUAGE.md`, `DECISIONS.md` (G-1..G-73), `crates/grift/src/` (`read.rs`, `print.rs`,
`evaluate.rs`, `library.grift`), `crates/grift-cli/`.

## Owner decision: grift does not sandbox the code it runs
Hostile code is the embedder's problem, not the language's. grift adds no safety limits that would
limit genuine programs: no depth limit, no trace-length cap, no memory quota, no change to cleanups.
**G-17, G-35 and G-48 stand** (cleanups always run, including for `interrupted`). Record this as a
DECISIONS.md entry citing the wave 2 sandbox findings it declines (tally section 3: attacks 28/31/13
on cleanups, 37 host re-entry, 44/45 memory, forged traces through `bind`), and say it in
`docs/VISION.md` (Embedding or The host boundary): grift does not isolate code from the host or from
other code in the same interpreter; an embedder running untrusted code isolates it itself, e.g. in its
own process. Two sentences, not a section.

## Done means all of these hold
1. **The rule above** in DECISIONS.md and VISION.md.
2. **Unwinding is linear.** Adding each frame's combination to an error's trace as it unwinds no longer
   rescans the whole trace per frame (`locate`, `evaluate.rs`; tally section 3, holes 34/69: 0.9 s at
   128 MiB, 44 s at 1 GiB). The trace means exactly what it means now (G-29: each combination once,
   innermost first). Same for the duplicate-name check in the parameter matcher (hole 33). These are
   speed bugs that honest programs hit (an error raised deep in a recursion); no behaviour changes.
   A test raises from a 300,000-deep non-tail recursion and finishes in bounded time (state the bound
   you measured, with headroom for a loaded machine).
3. **A reader a program can call** (core; tally core 1; 9 of 9 runner programs wrote one, 1,004
   lines, 200-250x slower than Rust's). It reads one form from a string at a byte offset and gives the
   form and the offset after it (so programs can read many forms, count lines and keep source text),
   with the same syntax as the program reader (one reader, not two) and a `parse-error` that says
   where. End of text is distinguishable from a form. Name it under the naming rules (agents reached
   for `read`, `string-to-value`, `read-form`; G-38's rules decide).
4. **A printer to a string** (core; tally core 2; 8 agents, ~210 lines): the text the printer writes
   for any value, as a string, including combiners and contexts as the printer shows them. Agents
   reached for `value-to-string`. Fix along the way if it's the printer's: `(float-to-string 1.0)`
   gives `"1"`, the integer's text (tally finding 4); a float's text must read back as a float.
5. **An applicative `binds?`** (library; 9 agents, two silent wrong answers): `(binds? context symbol)`
   with both evaluated, looking through parents as `binds!?` does.
6. **`member?` and `find-association` fast** (library; 7 agents measured 4-10x slower than nested
   `if!`): plain library grift, no fast path; report before/after on the tally's 4-element case.
7. **The list and string helpers with three or more askers** (library; tally finding 2 and section 5):
   join, filter, split, an integer range, starts-with/ends-with, a byte search, trim, fold, any/all,
   pad, lowercase, a two-list `map` if `map` can take several lists simply. Library grift unless a
   helper is measured unusable in grift (then a primitive, with the measurement). Names by the rules
   (`string-` prefix for strings, `?` for predicates, words spelled out). Sort has two askers: not
   now. Each helper gets a test stating a property, and an entry in LANGUAGE.md.
8. **`standard-error` in the runner** (8 agents; `/dev/stderr` through `create-file` truncated a build
   log): a context of operations beside `standard-input`, with a write operation like a file's.
   The runner's own diagnostics go there if they don't already.
9. **Evidence the forms pay off:** in a scratch worktree of your own (not in the PR), port two
   wave-2 programs onto the new forms — `sheet-graph` (branch `app2-sheet-graph`) and `build-data`
   (`app2-build-data`), both in `~/code/grift` — and report lines before/after, start-up time
   before/after (one run each), and their `test.sh` passing. Also drive the editor
   (`examples/emacs`) briefly in your own fux tab (`fux help agents` first; a workspace you name and
   close at the end): M-: with the reader and the printer, C-g in a loop inside `catch!`, save.
10. **Docs:** LANGUAGE.md lists every new name; VISION.md only where it would otherwise be false.

## Acceptance bands (set before the work; accept without asking)
- Core Rust, non-test lines: grows by at most 300. Library: grows by at most 150 lines.
- `fib 25` within 5% of the base (0.132 s at 27c2aa4; one run each, both built here).
- A band that can't be met without hurting correctness is revised as CLAUDE.md says (original target,
  evidence, new target, cost; listed in the PR).

## Not in this PR (owner)
Sandbox limits of any kind, memory accounting or a host memory limit, changes to cleanups, `load`,
file information, `run-process` changes, trace cutting, catch-by-kind, unbinding, character literals,
sort. Note any you find newly pressing in the PR.

## Rules, gates, machine
As DECISIONS.md and the crate's `[lints]` say: zero dependencies, `std`, `forbid(unsafe_code)`, no
panics in non-test code, no I/O in the core, `try_reserve` for growth, one evaluation path per kind
of form, no fast paths, library forms stay pure fexprs (no caching or expansion). Tests are few and
state properties. Gates, in the worktree, once per push:
```sh
export CARGO_TARGET_DIR=~/.cache/claude-tmp/grift-grow/target TMPDIR=~/.cache/claude-tmp/grift-grow/tmp
GATE_FMT=1 $S/rust-gate.sh origin/pure
(cd examples/emacs && cargo clippy --all-targets -- -D warnings)
```
Only your worktree, target and tmp dirs (scratch worktrees under `~/.cache/claude-tmp/grift-grow/`,
each with its own target dir); no /tmp, no git stash, `-j 8`, builds through `$S/cargo-slot`, nothing
over 10 minutes, kill what you start by pid (never `pkill -f`), never touch the user's sessions or
panes. Helpers that write code run Opus; split work by files if you use any.

## Review
Two reviewers, each in a detached review worktree at the PR head, briefed with base and head SHAs,
this file and no hoped-for verdict ("no findings" is a fine answer):
`$S/launch-agent.sh codex exec -s read-only -m gpt-6.1-sol` and an Opus reviewer
(`$S/launch-agent.sh claude -p --model claude-opus-5-5 --permission-mode plan`). **Never Astra, never
Fable.** A must-fix needs a failing test, a reproduced bug, a broken decision or a name against the
rules. At most 2 rounds; round 2 checks only round 1's must-fixes and what the fixes touched.

## Ledger, status, end
Ledger `~/code/notes/projects/grift/grow-2026-11-01/ledger.md` (base, checklist, decisions with why,
incidents, not verified); read it first if resumed. One-line status in `.../status` beside it. Course
corrections arrive as dated "Update" sections at the end of this file.

Commit only as the git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or
`--author`. No Co-Authored-By or any AI attribution (run `$S/prepush-check.sh origin/pure` before
pushing). PR against `pure`; **don't merge**. PR body: what changed for a grift programmer, a script
author and an embedder, with before/after code (a wave-2 workaround and its replacement); the closed
list with each item's test; the ported programs' numbers; line counts before/after (core, library,
runner, tests); `fib 25` and the deep-unwind timing before/after; decisions made alone; what was not
verified; `cd ~/code/grift && …` commands to try it. When the PR is open, the gate passes at its head
and the review is done, write `DONE` beside the status with the PR URL, head SHA and the gate's
result. Kill everything you started; remove your scratch worktrees; leave the main one.