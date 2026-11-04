---
id: 01a1a561-d82c-77b3-a75e-be02b41598cd
slug: tasks/grift-13
title: "grift: ship the vision guide, the new names and the decisions behind them"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVVVVVVV"
external_refs:
  - system: github
    id: "gold-silver-copper/grift#191"
    label: the PR, merged
    url: "https://github.com/gold-silver-copper/grift/pull/191"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift: ship the vision guide, the new names and the decisions behind them

**Links:** [gold-silver-copper/grift#191](https://github.com/gold-silver-copper/grift/pull/191) (the PR, merged).

History, part of [tasks/grift-1](grift-1.md). Outcome: merged as https://github.com/gold-silver-copper/grift/pull/191. Run notes: box2 `~/code/notes/projects/grift/vision-2026-11-01/`.

## Brief (`prompts/vision.md`, box2 only, never committed; verbatim)


You own one PR in gold-silver-copper/grift (default branch `pure`) end to end: write the guide into
the repo, rename the language, add what the guide decided, port the editor, verify in the real
REPL, open the PR, run the review, fix, report. The owner settled every decision below in
conversation on 2026-11-01; **do not reopen them**. Technical choices inside them are yours: decide,
write down why (ledger and PR), continue. Never ask the user anything. No prototypes or spikes.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Base and sources of truth
`origin/pure` at `b13be4c`. Your worktree: `~/.cache/claude-tmp/grift-vision/wt` on branch `vision`.
Read first:
- **`~/code/notes/projects/grift/vision-draft.md`**: the guide, owner-approved. It is the spec.
- **`~/code/notes/projects/grift/names-draft.md`**: every name, old -> new, and the naming rules.
  It is the spec for names.
- `~/code/notes/projects/grift/naming-panel/` (eight reviews behind the names; background only).
- `README.md`, `docs/LANGUAGE.md`, `DECISIONS.md` (G-1..G-35), `crates/grift/src/`,
  `crates/grift-cli/src/main.rs`, `examples/emacs/`.
Where the guide and the table disagree, the table wins for names and the guide for behaviour; note
any disagreement in the ledger and the PR.

## Done means all of these hold
1. **`docs/VISION.md`** is the guide (drop the draft header and its last "Also in the PR" section;
   keep the wording unless it would be false about the code you ship, and list any change you make
   in the PR). `README.md` links it; `docs/LANGUAGE.md` becomes the reference that matches it (the
   name table, every primitive and library form, every error kind).
2. **DECISIONS.md** gains one append-only entry per owner decision, citing the guide section:
   std/zero dependencies (already G-32, cite), hash-map contexts (already, cite), the names (rules
   and the renames), contexts instead of environments, no object system, the host boundary (private
   host functions, explicit close, `closed`), floats, one integer type, no vectors, Rust's
   `divide`/`remainder` and `as`, `if!` with an optional else (amends G-8), `let!` (`letrec*`;
   replaces `$let`/`$let*`), `catch!`/`finally!` on `call-with-handler`/`evaluate-with-cleanup`,
   `error` as a constructor and `raise` for every error, error fields `kind`/`values`/`trace`,
   `same?`/`equal?` (floats: bits / IEEE; different types never equal), no truthiness, no
   continuations, contexts print their names, the runner's Unix set, no primitive `if!`, how grift
   grows, what complete means. Mark which earlier entries each amends.
3. **Every name in the table**, in the core, the library, the runner, the editor, tests and docs.
   Grep for every old name afterwards (`\$[a-z]`, `->`, `car`, `cdr`, `cons`, `eq?`, `eval`,
   `environment`, `guard`, `bindings`, `ground`, `formals`, `#t`, `#f`, `+`, `<?`, …) and list the
   leftovers you kept on purpose (there should be almost none). Library code follows the naming
   conventions too (an operative's context parameter is `caller`).
4. **The behaviour the guide decided**, each with a test that states a property (not frozen output):
   - floats: reader and printer (round-trip; Rust's float formatting), `add`… on two floats,
     `type-error` on mixing, `integer-to-float`, `float-to-integer` exactly Rust's `as`, the Rust
     `std` functions under their spelled-out names, `float-to-string`, `string-to-float`;
   - `divide`/`remainder` as Rust's `/`/`%` (errors: division by zero, overflow);
   - `string-to-integer` (and `-float`): a bad string is an error with a kind you choose and record;
   - `if!` with an optional else; `let!` binding in order in one new context, so a combiner it binds
     can call itself and later bindings; `$when`, `$let*` gone;
   - `error` makes an error value; `raise` raises a new or a caught one; a caught error is a context
     with no parent binding `kind`, `values`, `trace`; re-raising keeps the trace;
   - `same?`/`equal?` exactly as the guide says, including floats and different types;
   - the comparison family; the type predicates for every `type-of` result;
   - `names` (own names, sorted), `built-in-names`, `current-context`, `evaluate-in!`, `binds!?`
     (through parents), `define-in!`;
   - a context prints as its sorted names (`#[context a b c]`);
   - no truthiness anywhere (tests of `if!`, `conditional!`, `and!?`, `or!?`).
5. **The runner's Unix set** (in `crates/grift-cli`, never the core), designed by the guide's
   host-boundary rule: reading stdin as data, files as contexts of operations (opening gives a
   context of operations such as reading, writing and closing; using it after close is `closed`;
   host functions bound only in a private context, so ids can't be forged), the command-line
   arguments, environment variables, the exit status, running a process. Names follow the rules.
   Keep it small: the fewest operations that let a Unix script read input, write output and files,
   and run a command. A test per operation; drive one real script end to end.
6. **The editor** (`examples/emacs`) on the new names, still meeting its finish line
   (`prompts/emacs.md`), driven in your own fux tab (`fux help agents` first; a workspace you name
   and close at the end), including C-g stopping a runaway command inside `catch!`.
7. **The stale notes**: replace `~/code/notes/projects/grift/README.md` and
   `~/code/notes/projects/grift/docs/architecture/` (they describe the old vau-core design) with a
   short README pointing at `docs/VISION.md` and the run notes. (Outside the repo; not in the PR;
   say what you did.)

## Acceptance bands (set before the work; accept without asking)
- Rust core, non-test code lines: at most 2,000 (base 1,694; floats need room). Library: at most
  about 350 (base 154). The runner: as small as the Unix set allows; report its lines.
- `fib 25` no more than 10% slower than the base (one run each, a second only to confirm).
- If a band can't be met without hurting correctness, revise it as CLAUDE.md says (original target,
  evidence, new target, cost; listed in the PR).

## Rules for the code
As DECISIONS.md and the crate's `[lints]` say: zero dependencies in the core, `std`,
`forbid(unsafe_code)`, no panics in non-test code, no I/O, threads or global state in the core,
`try_reserve` for growth. The implementation stays uniform: one evaluation path per kind of form, no
fast paths. Tests are few and state properties. Commit messages: plain English, what the program
now does.

## Gates (exact commands, in the worktree)
```sh
export CARGO_TARGET_DIR=~/.cache/claude-tmp/grift-vision/target TMPDIR=~/.cache/claude-tmp/grift-vision/tmp
GATE_FMT=1 $S/rust-gate.sh origin/pure
(cd examples/emacs && cargo clippy --all-targets -- -D warnings)
```
Gate once per push, not after every edit.

## Machine rules
Your worktree, `CARGO_TARGET_DIR` and `TMPDIR` above only. No /tmp, no git stash, nothing over 10
minutes or all cores (`-j 8`, `--test-threads 8`), kill what you start by pid (never `pkill -f`),
start helpers with `$S/launch-agent.sh`, never touch the user's sessions, panes or config. Builds go
through `$S/cargo-slot`. Helpers that write code run Opus; split work by files if you use any.

## Review
Two reviewers: `$S/launch-agent.sh codex exec -s read-only -m gpt-6.1-sol` and an Opus reviewer
(`$S/launch-agent.sh claude -p --model claude-opus-5-5 --permission-mode plan`). **Never Astra,
never Fable.** Each in a detached review worktree at the PR head, briefed with the base and head
SHAs, `vision-draft.md`, `names-draft.md`, this file's closed list and no hoped-for verdict; "no
findings" is a fine answer. A must-fix needs a failing test, a reproduced bug, a broken decision or
a name that contradicts the table. At most 2 rounds; round 2 checks only round 1's must-fixes and
what the fixes touched.

## Ledger and status
Ledger: `~/code/notes/projects/grift/vision-2026-11-01/ledger.md` (base, checklist, decisions with
why, incidents, not verified). Read it first if resumed. One-line status in
`~/code/notes/projects/grift/vision-2026-11-01/status`. Course corrections arrive as dated
"Update" sections at the end of this file.

## Git and the PR
Commit only as the git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or
`--author`. No Co-Authored-By or any AI attribution in commits or the PR (the guard hooks check;
also run `$S/prepush-check.sh origin/pure` before pushing). Open the PR against `pure`. **Don't
merge:** the orchestrator checks and merges. PR body: what changed for a grift programmer, an
embedder and a script author; a before/after program (the same small program in old and new
grift); the closed list with each item's test; the runner's Unix set with an example script; line
counts before/after (core, library, runner, tests, editor); `fib 25` before/after; decisions you
made alone; choices left to the owner; what was not verified; `cd ~/code/grift && …` commands to
try the REPL, a script and the editor.

## At the end
When the PR is open, the gate passes at its head and the review is done, write
`~/code/notes/projects/grift/vision-2026-11-01/DONE` with the PR URL, the head SHA and the gate's
result at that head. Report in plain words. Kill everything you started; leave the worktree.