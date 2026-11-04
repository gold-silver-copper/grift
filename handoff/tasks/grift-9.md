---
id: 01a1a561-cd67-7450-9e4c-16697ca9e8fc
slug: tasks/grift-9
title: "grift: fix what building three editors on it showed"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVVV"
external_refs:
  - system: github
    id: "gold-silver-copper/grift#188"
    label: the PR, merged
    url: "https://github.com/gold-silver-copper/grift/pull/188"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift: fix what building three editors on it showed

**Links:** [gold-silver-copper/grift#188](https://github.com/gold-silver-copper/grift/pull/188) (the PR, merged).

History, part of [tasks/grift-1](grift-1.md). Outcome: merged as https://github.com/gold-silver-copper/grift/pull/188. Run notes: box2 `~/code/notes/projects/grift/improve-2026-11-01/`.

## Brief (`prompts/improve.md`, box2 only, never committed; verbatim)


You own one PR in gold-silver-copper/grift (default branch `pure`) end to end: read the evidence,
change grift, port one editor onto the result as proof, open the PR, run the review, fix, report.
Full ownership: change anything if it makes grift smaller, clearer or more correct. No prototypes
or spikes. Never ask the user anything: decide, write down why (ledger and PR), continue.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Base and evidence
`origin/pure` at `0a0058c` (PR #187, the rewrite). Your worktree: `~/.cache/claude-tmp/grift-improve/wt`
on branch `improve`. Read first (30 minutes, not more):
- `README.md`, `docs/LANGUAGE.md`, `DECISIONS.md` (G-1..G-16), `crates/grift/src/`.
- On 2026-11-01 three agents each built an Emacs-style editor on grift, splitting it differently
  between Rust and grift. Everything is in `~/code/notes/projects/grift/emacs-2026-11-01/`:
  `summary.md` (the comparison), and per variant `thin/`, `screen/`, `buffer/`: `report.md`
  and `friction.md` (each problem with a reproduction). Their code is on local branches in this
  repository: `emacs-thin` (head fc6f095), `emacs-screen` (ebe0607), `emacs-buffer` (d19fbf7);
  read with `git show emacs-thin:examples/emacs-thin/src/editor.grift` etc. Two of them made the
  same core commit (`11addd3` on emacs-thin, `1f65872` on emacs-screen).
Re-check each finding on the base before acting on it; a finding that doesn't reproduce is
dropped with a line in the ledger.

## Goal: done means all of these hold
The closed list below, each with a test stating the property (not frozen output), plus the
editor port, the bands, the gate and the review.

1. **Host data API.** A host can make a string, make a pair, and take a pair apart
   (names your choice; compare the two existing commits and keep the better). Evidence: all three
   friction logs; the buffer agent printed source text and re-read it with `eval` instead.
2. **Printing a multi-form `$lambda` shows its own body**, not `$sequence`'s source (thin F6;
   the fix is in `library.grift`). Check every library form for the same mistake.
3. **Library basics** every editor rewrote, as grift code in `library.grift`: `null?`, `pair?`
   (and whichever other type predicates the three editors defined), `reverse`, `assoc`,
   `for-each`, `$when`, `$let*`, `$set!` (Kernel's `($set! env formals value)`; buffer F4: a
   `$define!` inside a function silently binds locally), and a remainder (`mod`, derived from
   `div`). Use R-1RK names and derivations where they exist (`05-07.pdf`). Nothing else without a
   reason from the friction logs.
4. **Errors say where they happened.** A machine error names the innermost combination being
   evaluated, or the compound combiner being run, so `$if` on a non-boolean doesn't just say
   `type-error #[primitive select] …` (screen F-5, F-9). Your design; keep `Error` small and keep
   G-4. Show the before/after message in the PR.
5. **The host can call a combiner** with argument values (`Grift::call` or similar) instead of
   building source text (screen F-3, buffer F1).
6. **Host functions get the host's state** without `thread_local!` (screen F-2, buffer F2: 29
   functions each borrowing a global). Your design (a generic parameter, a user-data slot, a
   closure); it must still build for `thumbv6m-none-eabi`, and it must not let a host function
   hold a `&mut Grift` and its state in a way the borrow checker can't see.
7. **Reading:** the string escapes the editors needed (`\e` or a general `\xNN;`, `\t`; thin F11,
   buffer F3), and reading text into data without evaluating it, from the host API at least (thin
   and buffer M-: both went through a host `eval-string`). A grift-level `read` primitive only with
   a reason recorded.
8. **The host can interrupt a running evaluation** (buffer F9: a runaway command can't be stopped
   by C-g; the only way out is killing the editor and losing unsaved work). A flag the host can set
   from a signal handler or another thread, checked at the machine's existing safe point, giving
   the error `interrupted`; grift still sets no limit of its own. This is decided (orchestrator,
   2026-11-01): it is the mechanism G-5 assumes ("limits belong to the host") and adds no limit.
   Record it as G-17. `thumbv6m-none-eabi` has atomic load/store but no compare-and-swap (no
   `alloc::sync::Arc` there): design for that.
9. **The editor as proof.** Port the thin editor (`emacs-thin`: a host that moves only bytes and
   files, everything else in grift) to `examples/emacs/` on the new API: delete every helper the
   library now provides and every workaround items 1, 5, 6, 7 and 8 remove, and make C-g interrupt a
   runaway command (bind a runaway via M-:, press C-g, keep editing, the buffer intact). Keep its
   8-item finish line from `~/code/grift/prompts/emacs.md` working; drive it in your own fux tab
   (`fux help agents` first, a workspace you name, closed at the end) and quote the screens in the
   PR. Report its Rust and grift line counts before (thin: 315 Rust, 577 grift) and after.
   Keep crossterm's `use-dev-tty` feature (a burst of keys stalls without it).

## The owner-level questions: a ten-agent panel decides
Per the skill (`references/playbooks.md`, "Decision during autonomous work"): ten fresh read-only
panelists across Opus, `gpt-6.1-sol` and `gpt-6-astra` (never Fable), started with
`$S/launch-agent.sh`, each from one perspective of this field (e.g. Kernel/fexpr language design,
embedded Rust, editor builder, Unix tools author, interpreter implementer, a newcomer writing their
first grift program, …). Brief them with the friction-log evidence and G-1..G-16, no hoped-for
answer; lettered ballot; 7 of 10 settles a question, else a second round after reading every
round-1 reason, decided by majority. Record each result in `DECISIONS.md` with the vote and the
strongest dissent. One panel, all questions on one ballot:
- **A. Cleanup on error.** G-4 says no catching. Evidence: buffer F10 and screen F-10 (an error
  in the minibuffer throws away the input; grift can't restore its own state). Options at least:
  (a) nothing changes; (b) a form that runs cleanup when an error passes through and lets the
  error continue (not catching); (c) catching (reopens G-4).
- **B. `$cond` with no matching clause:** `#inert` (today) or an error (G-8, no guessing)?
- **C. Keep the `ground-environment` binding?**
- **D. Keep the `stale-value` error code** (G-11) as it is?
Implement what the panel decides. A choice to reopen G-4 needs the panel's 7 of 10.

## Acceptance bands (accept without asking)
- Rust core, non-test code lines (no blanks, no comments): at most 1,500 (base: 1,180). Library:
  at most about 300 (base: 76).
- Speed is not a gate. Measure `fib 25` once on the base and once on the head, release builds,
  same place; and the ported editor's per-key time once on a 2,000-line file. "Not benched" for
  anything else. If item 4 costs more than 10% on `fib 25`, say so and why it's worth it.

## Rules for the code
As in `DECISIONS.md` and the crate's `[lints]`: `no_std` + `alloc`, no dependencies in the core,
`forbid(unsafe_code)`, non-test code must not panic. Tests are few and state properties. Update
`README.md` (host API section, with before/after code for every API change) and `docs/LANGUAGE.md`
(the new library forms, error codes, any R-1RK deviation). Commit messages: plain English, what the
program now does.

## Gates (exact commands, in the worktree)
```sh
export CARGO_TARGET_DIR=~/.cache/claude-tmp/grift-improve/target TMPDIR=~/.cache/claude-tmp/grift-improve/tmp
GATE_FMT=1 $S/rust-gate.sh origin/pure
cargo build -p grift --target thumbv6m-none-eabi -j 8
```
plus the chip example's and the editor example's builds and tests. Gate once per push, not after
every edit.

## Machine rules
Your worktree, `CARGO_TARGET_DIR` and `TMPDIR` above only. No /tmp, no git stash, nothing over 10
minutes or all cores (`-j 8`, `--test-threads 8`), kill what you start by pid (never `pkill -f`),
start helpers with `$S/launch-agent.sh`, never touch the user's sessions, panes or config. Builds go
through `$S/cargo-slot`. At most 10 panelists or reviewers running at once.

## Review
Two reviewers on other models (core language and API changes): `$S/launch-agent.sh codex exec -s
read-only -m gpt-6.1-sol` and the same with `gpt-6-astra`, each in a detached review worktree at the
PR head, briefed with the base and head SHAs, DECISIONS.md, this file's closed list and no hoped-for
verdict; "no findings" is a fine answer. Never Fable. A must-fix needs a failing test, a reproduced
bug or a broken decision; anything else is a note. At most 2 rounds; round 2 checks only round 1's
must-fixes and what the fixes touched.

## Ledger and status
Ledger: `~/code/notes/projects/grift/improve-2026-11-01/ledger.md` (base, checklist, decisions with
why, the panel result, incidents, not verified). Read it first if resumed. One-line status in
`~/code/notes/projects/grift/improve-2026-11-01/status`. Course corrections arrive as dated
"Update" sections at the end of this file.

## Git and the PR
Commit only as the git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or
`--author`. No Co-Authored-By or any AI attribution in commits or the PR (the guard hooks check;
also run `$S/prepush-check.sh origin/pure` before pushing). Open the PR against `pure`. **Don't
merge:** the orchestrator who started you checks the editor and REPL and merges. PR body: what
changed for a user and an embedder; the host API before/after; the closed list with each item's
test and evidence; the panel results; line counts before/after (core, library, tests, editor Rust,
editor grift); the fib and per-key numbers; decisions you made alone; choices left to the user;
what was not verified; `cd ~/code/grift && cargo run …` commands to try the REPL and the editor.

## At the end
When the PR is open, the gate passes at its head and the review is done, write
`~/code/notes/projects/grift/improve-2026-11-01/DONE` containing the PR URL, the head SHA and the
gate's result at that head. Report in plain words. Kill everything you started; leave the worktree.