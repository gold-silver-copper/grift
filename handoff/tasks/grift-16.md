---
id: 01a1a561-e265-7e63-b7bd-0361a2ddc36e
slug: tasks/grift-16
title: "grift: a programs corpus, then a fundamental consolidation and minimalization pass"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVVVVVVVVVV"
external_refs:
  - system: github
    id: "gold-silver-copper/grift#194"
    label: the PR, merged
    url: "https://github.com/gold-silver-copper/grift/pull/194"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift: a programs corpus, then a fundamental consolidation and minimalization pass

**Links:** [gold-silver-copper/grift#194](https://github.com/gold-silver-copper/grift/pull/194) (the PR, merged).

History, part of [tasks/grift-1](grift-1.md). Outcome: merged as https://github.com/gold-silver-copper/grift/pull/194. Run notes: box2 `~/code/notes/projects/grift/consolidate-2026-11-02/`.

## Brief (`prompts/consolidate.md`, box2 only, never committed; verbatim)


You own one PR in gold-silver-copper/grift (default branch `pure`) end to end. The owner asked on
2026-11-01 for "a further fundamental consolidation minimalization pass", run on the evidence of 22
real programs written in grift (waves 2 and 3). Every technical choice is yours: decide, write down
why (ledger and PR), continue. Never ask the user anything. No prototypes or spikes. The goal is
**less surface**: fewer primitives, fewer library names, fewer lines in the core, the library and the
docs, with grift meaning the same thing to a program that stays in the corpus.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Base and evidence
`origin/pure` at the head given in your launch message. Worktree `~/.cache/claude-tmp/grift-consolidate/wt`,
branch `consolidate`. Read first: `docs/VISION.md` (all; "What complete means" and "How grift grows"
are your rules), `docs/LANGUAGE.md`, `DECISIONS.md` (all entries; don't reopen an owner decision —
the ones marked as the owner's — but entries "made alone" by agents may be revised with evidence),
`crates/grift/src/`, `crates/grift-cli/`, `examples/emacs/`.
The evidence:
- Wave 2: `~/code/notes/projects/grift/apps2-2026-11-01/` (`tally.md`, 12 reports). Programs on the
  branches `app2-<name>` in `~/code/grift` (written before PR #193; sheet-graph and build-data were
  ported onto #193's forms in `~/.cache/claude-tmp/grift-grow/port-sheet.diff` and `port-build.diff`).
- Wave 3: `~/code/notes/projects/grift/apps3-2026-11-01/` (`tally.md`, 10 reports, each with a
  "What you used" section). Programs on the branches `app3-<name>`.

## Done means all of these hold
1. **A programs corpus in the repo**: `programs/<name>/` holding one program per application, about
   14: the best of each wave-2 app (sheet, adventure, build; sandbox only if its host crate is small
   and needs no change outside its directory, else skip it and say why) and each wave-3 app. Pick by
   which reads best and exercises most; say why. Port each onto today's names and forms (the
   wave-2 ones predate #193: replace their hand-written readers, printers and helpers). Each keeps
   its own `test.sh` (or a test the gate runs); **the gate runs the whole corpus** (a test in
   `crates/grift-cli/tests/` or the repo's gate script; capped, under a minute in total). This is
   the closed list VISION's "complete" means; say so in VISION and DECISIONS.
2. **A usage census**: a script (in the repo, small) that counts every built-in and library name's
   uses across `programs/`, `examples/emacs/`, the library and the runner. Its table goes in the
   PR, before and after.
3. **Consolidation, judged one name at a time**, each decision in a table in the PR (name, uses,
   decision, why):
   - **A primitive becomes library grift** when the library version can exist and the corpus's hot
     paths stay usable (measure the ones it touches; a band below).
   - **A library name no program uses** is removed, unless VISION or an owner decision requires it
     (say which). A name used once may be inlined where that reads as well.
   - **Two ways to do one thing** become one (e.g. overlapping list walkers, helpers that are each
     other with an argument swapped, error-kind spellings, duplicate predicates).
   - **The machine, reader, printer and heap** get simpler where it is plainly simpler: fewer cases,
     fewer special forms of data, shared code. One evaluation path per kind of form, no fast paths,
     library forms stay pure fexprs (no caching or expansion).
   - **The runner and the embedding API**: the same rules; the emacs host too.
   - **Docs**: LANGUAGE.md and VISION.md say the same with fewer words where they repeat themselves
     or the code; DECISIONS.md is append-only (record what changed, don't rewrite history).
4. **Negative line counts**, non-blank non-comment, base to head: core Rust non-test, library,
   runner, emacs host, docs. Tests may grow only by the corpus test.
5. **Every program in `programs/`, the editor and the existing tests pass.** Drive the editor in
   your own fux tab (`fux help agents` first; a workspace you name and close at the end): M-:, an
   error, C-g inside `catch!`, C-h f, save.
6. **Renames** only where a name breaks the naming rules (VISION "Names") or two names now mean one
   thing; every rename listed old -> new.

## Acceptance bands (set before the work; accept without asking)
- Core Rust non-test lines: at least 5% fewer than the base. Library: no more lines than the base.
  Docs: fewer.
- `fib 25` within 5% of the base; each corpus program's own timing within 25% of the base (one run
  each, both built here). A primitive moved to the library that misses this stays a primitive, with
  the number in the table.
- A band that can't be met without hurting correctness is revised as CLAUDE.md says (original
  target, evidence, new target, cost; listed in the PR).

## Not in this PR
New features (`load`, file information, catch by kind, sort, parse-error reasons...), unless removing
something makes one of them the simpler replacement; then say so. Sandboxing (G-74).

## Rules, gates, machine
As DECISIONS.md and the crate's `[lints]` say: zero dependencies, `std`, `forbid(unsafe_code)`, no
panics in non-test code, no I/O in the core, `try_reserve` for growth. Tests are few and state
properties. Gates, in the worktree, once per push:
```sh
export CARGO_TARGET_DIR=~/.cache/claude-tmp/grift-consolidate/target TMPDIR=~/.cache/claude-tmp/grift-consolidate/tmp
GATE_FMT=1 $S/rust-gate.sh origin/pure
(cd examples/emacs && cargo clippy --all-targets -- -D warnings)
```
Only your worktree, target and tmp dirs (scratch worktrees under `~/.cache/claude-tmp/grift-consolidate/`,
each with its own target dir); no /tmp, no git stash, `-j 8` (`-j 4` if the load average is over
20), builds through `$S/cargo-slot`, nothing over 10 minutes, kill what you start by pid (never
`pkill -f`), never touch the user's sessions or panes. Helpers that write code run Opus; split work
by files if you use any.

## Review
Two reviewers, each in a detached review worktree at the PR head, briefed with base and head SHAs,
this file and no hoped-for verdict ("no findings" is a fine answer):
`$S/launch-agent.sh codex exec -s read-only -m gpt-6.1-sol` and an Opus reviewer
(`$S/launch-agent.sh claude -p --model claude-opus-5-5 --permission-mode plan`). **Never Astra, never
Fable.** A must-fix needs a failing test, a reproduced bug, a broken decision, a corpus program that
no longer runs, or a name against the rules. At most 2 rounds; round 2 checks only round 1's
must-fixes and what the fixes touched.

## Ledger, status, end
Ledger `~/code/notes/projects/grift/consolidate-2026-11-02/ledger.md` (base, checklist, decisions with
why, incidents, not verified); read it first if resumed. One-line status in `.../status` beside it.
Course corrections arrive as dated "Update" sections at the end of this file.

Commit only as the git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or
`--author`. No Co-Authored-By or any AI attribution (run `$S/prepush-check.sh origin/pure` before
pushing). PR against `pure`; **don't merge**. PR body: what changed for a grift programmer, a script
author and an embedder (before/after code for each removal or rename a program would notice); the
corpus and how the gate runs it; the census before/after; the per-name decision table; line counts
before/after; `fib 25` and the corpus timings before/after; decisions made alone; what was not
verified; `cd ~/code/grift && …` commands to run the corpus, the REPL and the editor. When the PR is
open, the gate passes at its head and the review is done, write `DONE` beside the status with the PR
URL, head SHA and the gate's result. Kill everything you started; remove your scratch worktrees;
leave the main one.

## Update 2026-11-02 (before launch): wave 3's evidence
- Wave 3's tally is at `~/code/notes/projects/grift/apps3-2026-11-01/tally.md`; section 4 is a usage
  census. Its script is `~/.cache/claude-tmp/grift-apps3/census/census.py` (tables `census.tsv`,
  `wave2.tsv`; wave 2 program copies in `wave2/`). Start your own census from it.
- Candidates it names (yours to judge, one at a time): `binds!?` (no use since `binds?`), `select`,
  `evaluate-with-cleanup`, `ignore?`, `inert?`, `string-to-lowercase`, merging `define-in!` into
  `bind`, `read-form` skipping a first `#!` line (deleting the runner's own stripping).
  `wrap`/`unwrap`/`call-with-handler` have one user (meta) but are owner decisions or primitives the
  library needs: keep unless the library no longer needs them.
- **The float functions are an owner decision (G-42, VISION "Close to Rust"): keep them** even
  though no program used most of them.
- Docs and code disagree: after a real out-of-memory a `finally!` cleanup does nothing visible,
  though LANGUAGE.md says cleanups run (tally section 2/3). Make them agree, the simpler way, and
  say which.
- The library's own forms (`let!`, `conditional!`, `sequence!`, `or!?`, `and!?`) cost several times an
  `if!` and 7 of 10 programs rewrote hot loops around them. Making them cheaper by plainly simpler
  library grift is in scope (no fast paths, no caching, no new primitive unless one replaces more
  than it adds); report before/after.