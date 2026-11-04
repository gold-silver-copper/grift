---
id: 01a1a55c-47e8-7803-921e-70f1894d175f
slug: tasks/grift-2
title: "Embedding for real hosts (grift #198, merged)"
type: task
status: completed
priority: high
tags: [embedding]
in:
  tasks/grift-1: "a0"
external_refs:
  - system: github
    id: "gold-silver-copper/grift#196"
    label: the brief
    url: "https://github.com/gold-silver-copper/grift/pull/196"
  - system: github
    id: "gold-silver-copper/grift#198"
    label: the PR, merged
    url: "https://github.com/gold-silver-copper/grift/pull/198"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# Embedding for real hosts (grift #198, merged)

**Links:** Implements [decisions/grift-in-fux](../decisions/grift-in-fux.md); [gold-silver-copper/grift#196](https://github.com/gold-silver-copper/grift/pull/196) (the brief); [gold-silver-copper/grift#198](https://github.com/gold-silver-copper/grift/pull/198) (the PR, merged).

Part of [tasks/grift-1](grift-1.md). Owner decisions: [decisions/grift-in-fux](../decisions/grift-in-fux.md) items 3, 4, 5, 11, 12.

## Outcome
Merged as https://github.com/gold-silver-copper/grift/pull/198 (head e5c9012, merge 7ff695e on `pure`, 2026-11-03; CI and the local gate passed on e5c9012).
- Host API: `keep`/`kept`/`release` (a `Kept` can't be copied), an owned `Interrupter` (a stop reaches only the run in progress), `context`/`bind`/`lookup`/`names` from Rust, `evaluate_in` with source names, `seal_library`, `describe`, `position`, the small conversions.
- Safety: every way in answers `stale-value` for a reclaimed value and `foreign-value` for another interpreter's. `Value` is no longer `Copy` (24 bytes).
- Core vs native primitives: 16 natives, each with its grift definition in `crates/grift/src/natives.grift`, proven equal by `natives_are_their_definitions`; native twins of `if!`, `sequence!`, `string-search`.
- Language: `(bindings c)`, `\r`, `foreign-value`; `file:line` in traces.
- Decisions G-108 to G-141 in grift's DECISIONS.md. G-141 revised the `fib 25` band from at least 1.4x to at least 1.33x (desk measured 1.396x and 1.34x after the G-140 fix).
- G-123: making an interpreter or an interrupter can abort when memory runs out (`Arc::new`; stable Rust has no fallible `Arc`), the one exception to "refused, not aborted".
- The q2 model (every interleaving of the interrupter's model, 0 violating states at the head) still leaves three criteria unmet, as recorded: G-129's per-stop rule (a later stop interrupts a cleanup, by design), the end-of-run race both writers' branches share, and the `asked` flag's completed-before-check reading (not observable; G-125).
- Not verified (from the PR): q6's harness didn't re-score the carried edges; the q2 model is an exhaustive operational model, not a proof; the spike ports were timed on busy box2; the editor was driven by script.
- Run notes: box2 `~/code/notes/projects/grift/embed-2026-11-03/` (ledger, design ballot, adversarial collaborations, red team, prior art, pick, review, q2 model). Scratch port branches on box2: `port-config-opus`, `port-reactive-opus`, `port-automation-opus`, `embed-sol-port-*`.

## Brief (`prompts/embed.md`, from grift #196)
Verbatim. Its ledger path says `embed-2026-11-02`; the run used `embed-2026-11-03`.

# grift: embedding for real hosts — the API fux needs, two safety bugs, core and native primitives, native `if!`

You own one PR in gold-silver-copper/grift (default branch `pure`) end to end. The owner plans to
replace fuxth with grift in fux, in fux's own architecture (Rust owns the loop, grift is the config,
keys, prompt and command line), and five spikes found what grift's embedding lacks. The owner decided
the items below on 2026-11-02; **do not reopen them**. Technical choices inside them (names under
VISION "Names", signatures, Rust structure, file layout) are yours: decide, write down why (ledger and
PR), continue. Never ask the user anything. No prototypes or spikes.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Base and evidence
`origin/pure` at the head in your launch message (a0eec1c or later). Worktree
`~/.cache/claude-tmp/grift-embed/wt`, branch `embed`. Read first: `docs/VISION.md` (all), `docs/LANGUAGE.md`,
`DECISIONS.md` (all; G-5, G-17, G-21, G-22, G-33, G-56 especially), `crates/grift/src/` (all),
`crates/grift-cli/`, `examples/emacs/` (a host).
Evidence, all in `~/code/notes/projects/grift/mux-2026-11-02/`: `owner-decisions.md` (the rulings
below, in full), `synthesis/report.md` (section 2 is the ranked list; its items are cited `S<n>`),
the spikes' `report.md`/`friction.md` (`config/`, `logic/`, `layouts/`, `automation/`, `reactive/`),
`fuxread/report.md` section 4, `perf/report.md` section 4. The spikes' code is on the local branches
`spike-mux-<name>` in `~/code/grift` (`spikes/mux-<name>/`). Also `apps4-2026-11-02/tally.md` section 8.

## Owner rulings (record each as a DECISIONS.md entry citing its evidence)
1. **Kept values (decision 3; amends G-21's wording, not its rule).** `keep`, `kept`, `release` on
   `Grift`: a kept value is a binding in a context the interpreter owns (a root like the top level),
   under a name no program can reach; `release` unbinds it. `Kept` is not `Copy` or `Clone`, so a double
   release or a use after release doesn't compile; a `Kept` from another interpreter is an error.
   Nothing else about collection changes.
2. **The interrupt, fixed, no clock (decision 4; G-5 stands word for word).** The interpreter owns its
   flag (no `&'static`: G-17's reason, the chip target, went with G-25) and hands out an `Interrupter`
   that is `Send` and can be cloned; `stop()` stops only the run in progress when it is called, never a
   later one (a run counter or generation, loads and stores only), and a stop with no run in progress
   does nothing. `interrupted` stays uncatchable by grift code; cleanups still run. Time limits stay
   host policy: show a timer thread doing it in a test, and port the runner and the editor to the new API.
3. **Core and native primitives, separated (decisions 5 and 12).**
   - The primitives grift can't express (`vau!`, `evaluate`, `wrap`, `bind`, `context`, `pair`, `same?`,
     checked arithmetic, the float functions per G-42, ...) live apart from the ones that exist only for
     speed. Judge every current primitive by one question: can a grift definition of it exist that
     behaves the same? Candidates for native: the bitwise operations, `integer-to-string`,
     `string-to-integer`, `lookup`, `read-form`, `value-to-string`. Each verdict goes in a table in the
     PR, with the reason.
   - Every native primitive keeps its **grift definition as its meaning**, in grift source in the repo,
     and **a test proves the two indistinguishable** on generated inputs: the same results, the same
     error kinds and values, the same traces, the same tail calls, the same answers from `type-of`,
     `operative?`/`applicative?` and the printer. A native that can't pass stays library or is a core
     primitive, with the reason.
   - New natives: **`string-search`** (S7: ~1.1 µs a byte as library; fux's waits search screens), and
     **native twins of `if!` and `sequence!`** (reverses G-56 under this rule; G-56 measured 1.5-2.3x).
     A native operative must be indistinguishable too: what a program sees of `if!` (printing,
     `type-of`, passing it as a value, `(unwrap ...)` where that applies) stays as it is now.
   - VISION "Implementation" changes from "no fast paths" to: a library form may have a native twin,
     proven equal and justified by a measurement on `programs/`. Keep "one evaluation path per kind of
     form" true of everything else. No other native twins in this PR (`let!`, `lambda!`, `conditional!`
     are not in: measure and note what they would gain).
   - No change to how calls or contexts are represented (decision 11; G-33, G-34 stand).

## Done means all of these hold
**Bugs (the guarantees, first):**
4. A reclaimed value is always `stale-value`, never another answer: `as_pair`, `as_str` and every
   reader on the Rust side, and every primitive's argument check (S2: `lib.rs:310-320`, `evaluate.rs:847`;
   layouts F1). A failing test first.
5. A `Value` or `Symbol` from another interpreter is detected (an error), never read as an object or
   name of this one (tally §8.7 F17). A failing test first.

**The embedding API (S1, S3, S4, S6, S8; each with a spike's workaround as its before and after):**
6. Contexts built and read without running the machine: make one with a given parent, set, get, list
   names (the same operations as `bind`, `lookup`, `names`, under the same rules).
7. Evaluate source in a given context and define in a given context; **seal** a host's prelude so its
   code counts as library in traces (G-22), as grift's own library does.
8. **Errors say where in the source**: a source name and line for any evaluated text (the runner's files
   and `load`, a host's config file). Positions in a side table from the reader, not inside values
   (VISION "Values" and "Implementation"). The runner's messages and `load`'s trace show `file:line`.
9. The small conversions: a symbol's name, a list from a slice and a slice from a list, a string's
   contents as an owned `String` where borrowing blocks the caller, whether an error is of a kind,
   and one way to describe an error for a person (the editor example and four spikes each wrote one).

**The language (S9, S10):**
10. `\r` among the string escapes; check the rest of the escape set against Rust's and record the result.
11. Reading a record's fields as data: a library function giving a context's own bindings as a
    list a program can use (config C6, automation F4; `#[context ...]` printing stays per G-54).
    Name it by the rules.

**Evidence and docs:**
12. In scratch worktrees of your own (not in the PR), port the **config** spike onto the new API (and
    `reactive`'s handler keeping, and `automation`'s screen records), and report lines before/after and
    the hot paths before/after (config: a bound key, a prompt program; automation: `screen-records`;
    reactive: a hook call). Each spike's `check.sh` passes (drive it in your own fux tab: `fux help
    agents` first, a workspace you name and close at the end).
13. LANGUAGE.md and VISION.md say what changed; the embedding API's docs (`lib.rs`) cover every new
    item with an example; README if it shows the API.

## Acceptance bands (set before the work; accept without asking)
- `fib 25` at least 1.4x faster than the base (native `if!`); every `programs/` program's test.sh no
  slower than the base by more than 5%, and report each one's speed-up. One run each, both built here.
- `crates/grift` non-test Rust grows by at most 900 lines in total; the grift definitions of the
  natives may add up to 200 lines of grift.
- A band that can't be met without hurting correctness is revised as CLAUDE.md says (original target,
  evidence, new target, cost; listed in the PR).

## Not in this PR
A bytes value (its own PR next, `prompts/bytes.md`), pausing evaluations (G-53 stands), call or
context representation changes, any fux change, number bases, `string-to-integer` that doesn't raise,
byte classes, `record`, `vau!` with several bodies, fixed decimals, file append/kind/size, SIGINT
reporting, `run-process` input, sandboxing (G-74). Note any you find newly pressing.

## Rules, gates, machine
As DECISIONS.md and the crate's `[lints]` say: zero dependencies, `std`, `forbid(unsafe_code)`, no
panics in non-test code, no I/O in the core, `try_reserve` for growth. Tests are few and state
properties; the equivalence test is generated, not a list of examples. Gates, in the worktree, once per push:
```sh
export CARGO_TARGET_DIR=~/.cache/claude-tmp/grift-embed/target TMPDIR=~/.cache/claude-tmp/grift-embed/tmp
GATE_FMT=1 $S/rust-gate.sh origin/pure
(cd examples/emacs && cargo clippy --all-targets -- -D warnings)
```
Only your worktree, target and tmp dirs (scratch worktrees under `~/.cache/claude-tmp/grift-embed/`,
each with its own target dir); no /tmp, no git stash, `-j 8` (`-j 4` if the load average is over 20),
builds through `$S/cargo-slot`, nothing over 10 minutes, kill what you start by pid (never `pkill -f`),
never touch the user's sessions or panes. Helpers that write code run Opus; split work by files if you
use any.

## Review
Two reviewers, each in a detached review worktree at the PR head, briefed with base and head SHAs,
this file and no hoped-for verdict ("no findings" is a fine answer):
`$S/launch-agent.sh codex exec -s read-only -m gpt-6.1-sol` and an Opus reviewer
(`$S/launch-agent.sh claude -p --model claude-opus-5-5 --permission-mode plan`). **Never Astra, never
Fable.** Ask them especially for a program that can tell a native twin from its grift definition, and a
way for host code to break a guarantee (immutability, stale values, another interpreter's values,
interrupts reaching a later run). A must-fix needs a failing test, a reproduced bug, a broken decision,
a `programs/` program that no longer runs, or a name against the rules. At most 2 rounds; round 2
checks only round 1's must-fixes and what the fixes touched.

## Ledger, status, end
Ledger `~/code/notes/projects/grift/embed-2026-11-02/ledger.md` (base, checklist, decisions with why,
incidents, not verified); read it first if resumed. One-line status in `.../status` beside it. Course
corrections arrive as dated "Update" sections at the end of this file.

Commit only as the git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or
`--author`. No Co-Authored-By or any AI attribution (run `$S/prepush-check.sh origin/pure` before
pushing). PR against `pure`; **don't merge**. PR body: what changed for an embedder (before/after Rust
for every API item, from a spike), for a grift programmer and for a script author; the core/native table;
the equivalence test and what it generates; the bugs with their failing tests; the ported spikes'
numbers; `fib 25` and the `programs/` timings before/after; line counts before/after; decisions made
alone; what was not verified; `cd ~/code/grift && …` commands to try it. When the PR is open, the gate
passes at its head and the review is done, write `DONE` beside the status with the PR URL, head SHA
and the gate's result. Kill everything you started; remove your scratch worktrees; leave the main one.


## Run prompt (`prompts/embed-panel.md`, never committed; how #198 was built)

# grift: the embedding work, built by panels: what fux needs from grift's Rust API, two safety bugs, core and native primitives

You orchestrate one run on **box2** (`ssh box2`; everything below happens there) that ends in **one
grift PR**. **What** to build is `prompts/embed.md` (grift PR #196; if it isn't on `pure`, read it
with `git show origin/prompt-embed:prompts/embed.md`): its owner rulings 1–3, its done-items 4–13,
its acceptance bands and its "Not in this PR" list all hold, word for word. This file replaces only
its **process** (its "Review" and "Ledger, status, end" sections, and the single agent): every
judgment is made by a panel of four models, the code is written twice and the better version is
picked, and you coordinate and run scripts without judging anything yourself. Never ask the owner
anything: decide, write down why (ledger, PR), continue. Use the agentic coding skill
(`~/.claude/skills/agentic-coding/SKILL.md`, `S=~/.claude/skills/agentic-coding/scripts`),
especially `references/orchestration.md`.

## Base and already decided (do not reopen)
- grift `origin/pure` at the head in your launch message (a0eec1c or later); record the SHA.
- `prompts/embed.md`'s owner rulings; `~/code/notes/projects/grift/mux-2026-11-02/owner-decisions.md`;
  grift's `DECISIONS.md` and `docs/VISION.md`.
- **Models (the owner's choice; it overrides the skill's and launcher's "never Astra, never Fable"
  default):** Opus `claude-opus-5-5`, Fable `claude-fable-5-1` (via `$S/launch-agent.sh claude
  --model …`), Sol `gpt-6.1-sol`, Astra `gpt-6-astra` (via `$S/launch-agent.sh codex exec -m …`).
  Every panel has one seat for each of the four.
- This process is decided; technical choices inside it are the panels'.

`R=~/code/notes/projects/grift/embed-<date>` holds everything below.

## How every panel works
- Each panelist answers **alone first**, in a fixed JSON format, read-only (no edits, builds of the
  PR, commits, pushes or GitHub comments), with base SHA, the inputs named for that panel and
  DECISIONS.md, and no hoped-for verdict ("no findings" and "no preference" are valid).
- Then **one revision round**: each sees the other three answers and may change its own, giving the
  new evidence. A change with no new evidence counts as its first answer.
- **A script, not an agent, tallies** the answers into `$R/<panel>/tally.md`. Three or four of four
  settle a question. **Evidence beats votes**: a failing test, a reproduced bug or a measurement from
  one panelist outranks three opinions. A 2–2 split is settled by the fewer new public items, then the
  fewer lines, and the losing side is recorded as the dissent. Every settled question goes in the
  ledger with its count and strongest dissent; the ones that set a rule go in DECISIONS.md.

## Stage 1: the design ballot
Four planners (one per model) read embed.md, the evidence it lists (all under
`~/code/notes/projects/grift/mux-2026-11-02/`, the `spike-mux-*` branches, the apps4 tally §8) and
`crates/grift/src/` in full, and each answers **every** question below alone, with a concrete Rust or
grift signature, the reason and the evidence:
1. `keep` / `kept` / `release`: signatures, the name scheme of the hidden bindings, how a `Kept` from
   another interpreter is caught.
2. The interrupter: the run counter or generation, what `stop()` loads and stores, how a stop with
   no run in progress does nothing.
3. The interpreter identity check (done-item 5): where the identity lives (in `Value`, in heap
   references, in `Symbol`), what it costs in `Value`'s size and in `fib 25`.
4. The stale-value audit (done-item 4): the list of every Rust-side reader and primitive argument
   check that can turn "freed" into another answer, with `file:line`. Include `same?` on two freed
   strings (`evaluate.rs:842`).
5. The core/native table: every current primitive, `core` or `native`, with the reason.
6. The equivalence test: what it generates (forms, argument shapes, errors, tail positions), how it
   compares results, error kinds and values, traces, tail calls, `type-of`, `operative?` /
   `applicative?` and printing, and how it stays inside the repo's "few tests that state properties".
7. Native `if!` and `sequence!`: how they stay indistinguishable when printed, passed as values,
   given to `unwrap`, and seen in traces.
8. Context building and reading without the machine, evaluating and defining in a given context, and
   sealing a host prelude (G-22).
9. Source positions: the reader's side table, what a position names, how it reaches the runner's
   messages and `load`'s trace.
10. The small conversions and the one error description: names and signatures.
11. The record-fields library function: name and definition.
12. The escape set: `\r`, and the check against Rust's escapes.
13. Anything in embed.md a planner thinks can't be done as written, with evidence (as CLAUDE.md
    says: original target, evidence, new target, cost).

Tally it as above. The settled answers are `$R/design.md`, the one design both writers build.

## Stage 2: failing tests first
Two test writers from different families (Opus and Sol), each alone in a scratch worktree of
`origin/pure`, write failing tests for done-items 4 and 5 and for a stop reaching a later run
(ruling 2). A test counts when it fails on `pure` for the stated reason. The writers in Stage 3 start
from the union of the tests that count.

## Stage 3: two writers
Two writers from different families (Opus and Sol), each in its own worktree
(`~/.cache/claude-tmp/grift-embed/writer-<model>/wt`, branch `embed-<model>`, its own target and tmp
dirs), each build **all of embed.md** from `design.md` and the Stage 2 tests, alone. Each also ports,
in scratch worktrees of its own, the **config** spike, `reactive`'s handler keeping and
`automation`'s screen records onto its own branch, and reports lines and hot-path timings before and
after (embed.md done-item 12). Each spike's `check.sh` passes, driven in a private fux server
(`unset FUX_SOCKET FUX_PANE`, a 0700 `XDG_RUNTIME_DIR`, `FUX_SOCKET` under its own dir). The spike
ports show how the API reads in real host code, and the picking panel uses them.

Each writer runs the gate at its head before it reports `DONE`:
```sh
export CARGO_TARGET_DIR=~/.cache/claude-tmp/grift-embed/writer-<model>/target TMPDIR=~/.cache/claude-tmp/grift-embed/writer-<model>/tmp
GATE_FMT=1 $S/rust-gate.sh origin/pure
(cd examples/emacs && cargo clippy --all-targets -- -D warnings)
```
Commits are local until Stage 5. Every writer's brief says: **no Co-Authored-By or any AI attribution**.

## Stage 4: the red team (can a program tell them apart?)
Four red-teamers (one per model), each given **both** writers' branches read-only, each try alone to
write:
- a grift program whose output differs between a native twin (`if!`, `sequence!`, `string-search`,
  every primitive the table moved to native) and its grift definition;
- Rust host code that breaks a guarantee: immutability, stale values read as live, another
  interpreter's values, a stop reaching a later run.

Each finding is a file that runs, with the command that shows the difference. The orchestrator runs
every finding against both branches (a script) and records which branch it breaks. A working finding
is a must-fix for that branch, and the equivalence test's generator must be extended until it
catches the finding by itself.

## Stage 5: pick, fix, open
- Timings on **desk only** (`~/code/notes/build-servers.md`, `~/ex/grift-embed/`), one job at a time,
  one run each: `fib 25` and every `programs/` test.sh for the base and both branches.
- **Picking panel** (four, one per model) compares the two branches on: embed.md's done-items, the
  red-team results, the bands (lines, timings), the spike ports' readability and line counts, and
  grift's rules. It picks one as the base and lists named parts of the other that are better. The
  base's writer carries those over and fixes its red-team must-fixes, then runs the gate again.
- Push the base branch as `embed` and open the PR against `pure`; **don't merge**. Run
  `$S/prepush-check.sh origin/pure` before every push. The PR body is embed.md's list, plus: both
  writers' branches compared (the pick and its count), the design ballot's settled answers with
  dissents, and the red-team findings and their fixes.

## Stage 6: review panel
Four reviewers (one per model), each in a detached worktree at the PR head, briefed with base and
head SHAs, embed.md, `design.md` and DECISIONS.md, asked especially for a program that tells a
native twin from its grift definition and for host code that breaks a guarantee. A must-fix needs a
failing test, a reproduced bug, a broken decision, a `programs/` program that no longer runs, or a
name against the rules; everything else is a note. At most 2 rounds; round 2 checks only round 1's
must-fixes and what the fixes touched. A must-fix needs the reproduction, not a vote: one reviewer
with a failing test is enough.

## Machine and budget
- box2 is the owner's machine too. Every build through `$S/cargo-slot`, `-j 3` per build; every
  agent through `$S/launch-agent.sh` (exit 75 means no lease: wait and retry). At most 10 read-only
  panelists at once; stagger starts about 40 s.
- Everything under `~/.cache/claude-tmp/grift-embed/`, one target dir per worktree (seed with
  `cp -a --reflink=always`). No /tmp, no git stash, nothing of yours over 10 minutes in the
  foreground, kill by pid (never `pkill -f`). Never touch the owner's fux session or panes.
- Rough count: 4×2 planners + 2 test writers + 2 writers + 4 red-teamers + 4×2 pickers + 4×2
  reviewers ≈ 30 agent runs. Check quota with `$S/fleet-status.sh` before Stage 3.

## Ledger, status, end
- Ledger `$R/ledger.md` (base SHA, stage checklist, every seat with model, pid or session and
  status, settled questions with counts and dissents, incidents, not verified); read it first if
  resumed. One-line status in `$R/status`. Agents write `DONE` at fixed paths; wait by pid or
  `wait-turn.sh`, never by reading screens. Course corrections arrive as dated "Update" sections at
  the end of this file. Report in a few plain lines at each stage's end.
- **The end:** when the PR is open, the gate passes at its head and the review is done, write `DONE`
  in `$R/status` with the PR URL, head SHA and the gate's result, and a one-page `$R/report.md`
  (what changed for an embedder, the pick, the red team, the timings, what wasn't verified, and
  `cd ~/code/grift && …` commands to try it). Kill everything you started; remove every worktree and
  target dir under `~/.cache/claude-tmp/grift-embed/` except the PR branch; check `git worktree list`.

## Update (2026-11-03)
- **You are already on box2** (its hostname is `omarchy`; "box2" is only the laptop's name for it).
  Work locally: no ssh to box2.
- **Timings run on desk, reached from box2 over koh** (box2's client is now on desk's allowlist;
  desk is saved on box2 as the koh server `desk`). This replaces the earlier box2-timing rule of
  this Update. Per `~/code/notes/build-servers.md`: work under `~/ex/grift-embed/` on desk (NTFS:
  no `* " \ | ?` or tabs in file names), `CARGO_TARGET_DIR` and `TMPDIR` there too, cargo as
  `~/.cargo/bin/cargo`, one job at a time, base and branch built there, one run each. For example
  `koh exec desk --sync . -C ex/grift-embed/<name> -- ~/.cargo/bin/cargo build --release -j 4`,
  `koh run desk … --name <job>` with `koh wait desk <job> --timeout 600` for anything long, and
  `koh cp` for results. Never `koh connect` (it shares the owner's session).
- `wait-turn.sh` isn't in this machine's skill scripts: wait on agents by pid (`claude -p`,
  `codex exec`) and their `DONE` files.

## Update (2026-11-03, 14:00)
- The design ballot settled no question by three or four of four: five by a 2-1-1 plurality, eight by
  the tie-break. Weakest: **q2 (the interrupter)** and **q6 (the equivalence test)**, each a 1-1 tie
  after hard evidence removed two answers. Brief the red team (Stage 4), the picking panel (Stage 5)
  and the reviewers (Stage 6) to look hardest at those two: for q2, any interleaving where a stop
  reaches a later run, is lost, or costs the hot path more than measured; for q6, a native twin the
  generated test can't tell from its definition. Keep the briefs free of a hoped-for verdict.
- Writers' deviations from design.md (like writer-opus's q2 change) are judged on their evidence,
  not on agreement with the tally.

## Update (2026-11-03, 14:20): prior art and settling the close calls
Don't reopen design.md. Add these before Stage 4 (start them now, alongside writer-sol):

1. **Prior-art scan** (read-only readers, one per group, models spread across the four; outputs in
   `$R/prior-art/<group>.md`, each with designs, trade-offs and source citations, scoped to q2, q3,
   q5-q7 and q9; the reports are inputs, never the answer):
   - **lua**: Lua's registry (`luaL_ref`/`luaL_unref`), `lua_sethook` count hooks, `_ENV`; mlua's
     `RegistryKey` and its instance-mismatch check.
   - **ml-haskell**: GHC asynchronous exceptions (`throwTo`, `mask`), OCaml safe-point polling and
     signals, OCaml `Bytes` vs `String`, GHC rewrite rules.
   - **scheme-kernel**: R7RS derived expression types, Racket `#%kernel` vs `racket/base` and breaks,
     Guile source properties vs Racket syntax objects, MIT Scheme environments, the Kernel report's
     primitive/library split, **Wand, "The theory of fexprs is trivial" (1998)**, and Shutt's answer
     in his Kernel thesis: what lets a native operative be indistinguishable from its library one.
   - **rust-hosts**: Steel, Rhai, Rune, Gluon, mlua: value lifetimes across calls, interrupts,
     identity checks, and branded lifetimes (GhostCell) as a compile-time alternative for q3.
   - **equivalence**: contextual/observational equivalence in PL theory as it bears on q6, and how
     differential testing of language implementations generates programs.
2. **Adversarial collaboration on the close calls** (each side states what result would change its
   mind; one agent from each side writes the experiment; a script runs it; evidence for this run,
   not added to CI, per the test budget):
   - **q2**: a `loom` (or equivalent exhaustive-interleaving) model of the interrupter, plus a stress
     run of stops at random points, against both writers' branches.
   - **q3**: `Value`'s size and `fib 25` for each identity design, one run each, on desk.
   - **q6**: each writer's generated test scored on whether it catches the red team's findings on
     its own.
   Where the writers' branches took different sides, run differential testing between them too.
3. Give the prior-art reports and these results to the red team (Stage 4), the picking panel and
   the reviewers. Findings that change a design are fixes in the picked branch, with the evidence,
   recorded in DECISIONS.md and the PR.

## Update (2026-11-03, 18:50): re-run the q2 model before merge
PR #198 says q2's interleaving model was not re-run against the final interrupter (its harness reads
the old chain). Close that:
1. Adapt the q2 harness (`adv/q2/`) to the interrupter at the PR head (c85356b): the mailbox list,
   the run-identity guard, the "did a stop find a run" answer and the stop-on-failure rule. Keep
   every criterion both sides wrote, and add one for each red-team interrupt finding the model can
   express. One agent from each side (A and B, as before) checks that the adaptation keeps their
   criteria; a script runs it.
2. If it finds a violation: a failing test first, the fix on `embed` in the PR worktree, the gate
   once, the 17 red-team findings and the review repros again, and one review pass by the four
   reviewers on just that fix (round-2 rules).
3. Record the result in the ledger and `adv/q2/results.md`. Update PR #198's body: move this out of
   "What wasn't verified" and give the states explored and the outcome. The model is evidence for
   this run: don't add it to CI. Don't merge.
4. Then write `DONE` in the status again with the head SHA, and clean up as before.