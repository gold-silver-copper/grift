---
id: 01a1a55c-4812-7b73-b823-01eef26c4307
slug: tasks/grift-4
title: "Ten fux ports onto grift; panels decide what grift earns; one grift PR"
type: task
status: draft
priority: high
tags: [fux, panel]
in:
  tasks/grift-1: "a0VV"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# Ten fux ports onto grift; panels decide what grift earns; one grift PR

**Links:** Depends on [tasks/grift-2](grift-2.md); Blocks [tasks/grift-5](grift-5.md); Implements [decisions/grift-in-fux](../decisions/grift-in-fux.md).

Part of [tasks/grift-1](grift-1.md). Owner decisions: [decisions/grift-in-fux](../decisions/grift-in-fux.md). Needs [tasks/grift-2](grift-2.md) (merged).

Ready to start: the precondition (embedding work merged into `pure`) holds at 7ff695e. Not started; it is about 50 to 60 agent runs on box2 with desk timings, so it needs the owner's go.

Changes since the prompt was written:
- All grift prompts are GitKB documents (owner, 2026-11-03). Stage 6's brief for the real fux replacement becomes [tasks/grift-5](grift-5.md)'s body, not `prompts/grift-replacement.md` in a fux brief PR.
- Stage 5's "rebase every port's fux branch onto the grift PR head" means: point each port's fux worktree's path dependency at a grift worktree of the PR head, without the port's own grift patches.
- The end has one PR (the earned grift PR), so `DONE` carries one PR URL and head SHA, plus the [tasks/grift-5](grift-5.md) update.
- `wait-turn.sh` doesn't exist on box2 or alarm: wait by pid and `DONE` files.
- box2's `~/code/grift` is behind `origin/pure`: fetch before recording the base.
- box2 reaches desk over koh (server `desk`, added 2026-11-03; `~/code/notes/build-servers.md`), so desk timings can be queued from box2.

## Acceptance criteria
- [ ] Ten ports scored by `score.sh`, timings on desk
- [ ] Reading panel tally: the earned needs, the owner list, whether fux needs bytes
- [ ] One grift PR with the earned set, reviewed by four models, gate green, not merged by an agent
- [ ] Stage 5: the ports pass on the PR head without their own grift patches
- [ ] [tasks/grift-5](grift-5.md) holds the brief for the real fux replacement

## Run prompt (`prompts/fux-port-panel.md`, never committed)
# grift: earn what fux needs: ten independent ports of fux onto grift, with panels deciding what grift gains

You orchestrate one run on **box2** (`ssh box2`; everything below happens there). The owner plans to
replace fuxth, fux's Forth, with grift, in fux's own architecture: Rust owns the event loop, panes,
layout and drawing; grift is the config, key bindings, command prompt, saved sessions and every `fux`
command line. Ten agents each port fux onto grift on their own, patching grift wherever it blocks
them. Panels of four models then decide which patches grift has truly earned, two writers build that
set as **one grift PR**, and the ten ports check that it is enough. The run ends by writing the brief
for the real fux replacement. You coordinate and run scripts; **every judgment is made by a panel,
never by you or by one agent alone**. Never ask the owner anything: decide, write down why (ledger),
continue. Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`,
`S=~/.claude/skills/agentic-coding/scripts`), especially `references/orchestration.md`.

## Preconditions (check first; stop and report if one fails)
- The embedding work (`prompts/embed.md`) has merged into grift's `pure`. Base every grift worktree
  on `origin/pure` at that point and record the SHA. If it hasn't merged, stop: this run would only
  rediscover what that PR fixes.
- fux base: `origin/master` of gold-silver-copper/fux; record the SHA.
- The bytes work (`prompts/bytes.md`) is not a precondition. The run reports whether fux needs it.

## Already decided (do not reopen)
- `~/code/notes/projects/grift/mux-2026-11-02/owner-decisions.md`, all of it. For the ports: a fux
  command line is one quoted grift program with shell arguments after it as strings (1a); no
  fuxthisms, so fux's names and error kinds follow grift's conventions (6); grift data, not JSON (7);
  no converter for sessions saved by today's fux (8a); fuxth, fuxth-run and fuxed move to
  `fux/archived/` (9).
- grift's `DECISIONS.md` and `docs/VISION.md`; fux's `DECISIONS.md` and `VISION.md`. A port may
  propose reopening one only with evidence from its port, as a blocker entry.
- **Models for this run (the owner's choice; it overrides the skill's and launcher's "never Astra,
  never Fable" default):** Opus `claude-opus-5-5`, Fable `claude-fable-5-1` (both via
  `$S/launch-agent.sh claude --model …`), Sol `gpt-6.1-sol` and Astra `gpt-6-astra` (both via
  `$S/launch-agent.sh codex exec -m …`). Every panel in this run has one seat for each of the four.
- The run's design is decided: ten ports, panels of the four models, one grift PR. Don't redesign it.
  Technical choices inside it are yours.

## Evidence to give every agent (by path, not pasted)
`~/code/notes/projects/grift/mux-2026-11-02/` (`synthesis/report.md`, `fuxread/`, `perf/`, the five
spike folders' `report.md`/`friction.md`), the local branches `spike-mux-*` in `~/code/grift`, the
merged embedding PR's body, and the wave 4 tally `~/code/notes/projects/grift/apps4-2026-11-02/tally.md`.
fuxth lives in fux's `src/script/` (Rust plus `library.fux`, `keys.fux`, `client-defaults.fux`) and
the `fuxth/` crate.

## Stage 1: the yardstick and the port brief (before any port starts)
1. Write `$R/yardstick/` (`R=~/code/notes/projects/grift/fuxport-<date>`): one script `score.sh PORT`
   that runs on a port and prints one line per check, plus a summary. It covers:
   - fux's own tests that reach the script layer (find them: `tests/config.rs`, `cli.rs`,
     `cookbook.rs`, `agents.rs` and whatever else calls into `src/script/`), run with fux's
     `~/code/notes/projects/fux/live/gate.sh` caps;
   - the bundled fuxth code (`library.fux`, `keys.fux`, `client-defaults.fux`) ported, every binding
     and default reachable;
   - the five spikes' `check.sh` behaviours that apply to fux's architecture;
   - a driven check in a private fux server: attach, split, bind and press a key, run a prompt
     program, save and restore a session, `fux help`, an error that names file and line;
   - the timing points, measured later on desk: one command line to its first request, attaching
     with all bundled code loaded, a warm bound key, a search of a 120×40 screen for text.
2. Write `$R/port-rules.md`, the brief every port gets: the shared rules, the strategy slot, the
   blocker log format, the scope, the stop conditions (all below).
3. **Brief panel:** four read-only reviewers (one per model) read the yardstick and port-rules alone
   and list must-fixes (each with a concrete failure: a check that can't fail, a step a port can't
   do, a contradiction with a decision). Fix what three or four agree on, or what one shows with
   evidence; record the rest. One round.

## Stage 2: ten ports
**Seats** (strategy × model; each strategy gets two model families, so a need can be traced to the
approach or to the model):

| Strategy | What it means | Wave 1 | Wave 2 |
|---|---|---|---|
| **literal** | each fuxth word becomes a grift operation of the same meaning | Opus | Sol |
| **idiomatic** | config, keys and command line redesigned as grift would express them | Fable | Opus |
| **grift-only** | fux's Rust may change only to swap fuxth's interpreter for grift's; every gap must show up in grift or in grift code | Astra | Opus |
| **minimal-grift** | patch grift only when no workaround in grift code or fux's Rust exists | Sol | Opus |
| **speed-first** | keep every timing point at least as fast as fuxth | Opus | Fable |

Five Opus, two Fable, two Sol, one Astra. Start wave 2 when wave 1's builds have settled, or as
leases and `cargo-slot` allow; the waves differ only in start time.

**Each port:**
- Its own fux worktree (branch `fuxport-<strategy>-<model>`) and grift worktree (same name), with
  fux depending on that grift by path; its own target dir per worktree (seed with
  `cp -a --reflink=always` from a warm one), its own `TMPDIR`, its own private fux server for driven
  checks (`unset FUX_SOCKET FUX_PANE`, a 0700 `XDG_RUNTIME_DIR`, `FUX_SOCKET` under its own dir).
- **Independent:** it reads none of the other ports' branches, logs or status. Tell it so, and keep
  its worktrees out of the others' briefs.
- **Scope:** what fuxth does today, in order: config loading and settings; key bindings; the command
  prompt; every `fux` command line; saved sessions; help; errors. Ports go as far as they can; the
  long tail of commands after the first six areas is optional.
- **Grift patches are evidence, not finished design:** one commit per need on its grift branch, the
  smallest change that unblocks it, plus one entry in `$R/ports/<seat>/blockers.jsonl`:
  ```json
  {"id":"<seat>-<n>","area":"keys|config|prompt|cli|sessions|help|errors|other",
   "need":"one sentence, in grift's terms","blocked":"what couldn't be done, with file:line",
   "workaround":"what was tried first, and why it wasn't enough (or null)",
   "kind":"blocks|slow|awkward","patch":{"commit":"<sha>","files":["…"],"added":N,"removed":N} or null,
   "measured":"numbers if kind is slow, else null","checks":["yardstick checks that need it"]}
  ```
  A blocker solved inside fux or in grift code still gets an entry, with `"patch": null`.
- Commits stay local: **no pushes, no PRs** (these branches are disposable evidence). Commit as the
  git-config identity; no Co-Authored-By or any AI attribution.
- **Stops** at whichever comes first: `score.sh` passes; 8 hours; or three hours without a new
  passing check. It ends with `$R/ports/<seat>/report.md`: what works, what doesn't, line counts
  (fux Rust removed and added, grift code written, grift patch lines), and its own top three needs.
  Then it writes `DONE`.

**Scoring (scripts, no panel):** run `score.sh` on every finished port. Queue the timing points on
desk (`~/code/notes/build-servers.md`; `~/ex/<task>/` there), one port at a time, one run each,
fuxth's numbers taken the same way. Record a **novelty count** per port: how many needs no earlier
finished port had logged. Put the results in `$R/scores.md`.

## Stage 3: the reading panel (what grift has earned)
Four readers, one per model, each read **all ten** reports, blocker logs and grift patch diffs
alone. Each writes `$R/reading/r1/<model>.json`: needs grouped across ports, and for each group:
the entries it covers, the ports and model families that hit it, a verdict (`grift`: grift should
gain it; `fux`: it belongs in fux's Rust or fux's grift code; `owner`: it needs a decision only the
owner can make; `drop`), the proposed design if `grift`, and the evidence. Then one revision round:
each reader sees the other three files and may change verdicts, giving the new evidence for each
change. A change with no new evidence counts as its round-1 answer.

**A script, not an agent, tallies it** (`$R/reading/tally.md`). A need is **earned** when:
- at least three of the four readers say `grift`; and
- ports from at least two model families logged it, or one port showed it with a reproduced failing
  check that no workaround fixes.

Evidence beats votes: a reproduced failure from one reader outranks three opinions. A 2–2 split and
every `owner` verdict go to "choices left to the owner", built as nothing. Each reader's strongest
dissent is kept beside the verdict.

**Design disagreements:** where the earned need has two or more competing designs from the ports
and the readers don't agree 3–4 on one, a design panel (four, one per model) rules, given both
ports' patches. If it splits 2–2, run the swap: each of the two ports adopts the other's design in a
scratch branch of its grift worktree and reruns `score.sh`. The design that keeps both ports passing
wins; still tied, it goes to the owner list.

The reading panel also sets, before any writing, the **acceptance bands** for the grift PR (non-test
Rust lines in `crates/grift` and lines of library grift, from the earned patches' sizes plus at most
50%), and answers: does fux need a bytes value (`prompts/bytes.md`)?

## Stage 4: the earned grift PR (two writers, one PR)
- Two writers from different families (Opus and Sol), each in its own grift worktree from the
  recorded `pure` SHA, each building **the whole earned set** from `tally.md` alone, written fresh
  (port patches are evidence, never copied). Each follows grift's rules: zero dependencies, `std`,
  `forbid(unsafe_code)`, no panics in non-test code, `try_reserve`, names under VISION "Names", the
  core/native rule from the embedding PR, a failing test first for every bug, DECISIONS.md entries
  citing the tally, LANGUAGE.md/VISION.md/`lib.rs` docs.
- **Picking panel** (four, one per model) compares the two branches against the tally, the bands
  and grift's rules, and picks one as the base, listing parts of the other that are better. The
  base's writer carries those over. A 2–2 pick goes to the smaller diff that passes the gate.
- Gate at the head, once per push, in the writer's worktree:
  ```sh
  export CARGO_TARGET_DIR=$R/writer-<model>/target TMPDIR=$R/writer-<model>/tmp
  GATE_FMT=1 $S/rust-gate.sh origin/pure
  (cd examples/emacs && cargo clippy --all-targets -- -D warnings)
  ```
  `fib 25` and every `programs/` test.sh within 5% of the base, timed on desk.
- Open the PR against `pure`; **don't merge**. Review panel: four reviewers (one per model), each in
  a detached worktree at the PR head, briefed with base and head SHAs, the tally and DECISIONS.md, no
  hoped-for verdict ("no findings" is fine). A must-fix needs a failing test, a reproduced bug, a
  broken decision, a `programs/` program that no longer runs, or a name against the rules. At most 2
  rounds; round 2 checks only round 1's must-fixes and what the fixes touched.

## Stage 5: proof with the ports themselves
Rebase every port's fux branch onto the grift PR head, **dropping its own grift patches**, and adapt
only its grift code and fux code to the PR's names (a script plus one fixing agent per port, the
port's own model). Rerun `score.sh`. Report per port: checks passing before and after, and every
failing check with the need behind it. If any check that passed before now fails because a need is
missing, run Stage 3 once more on just those entries, fix in the same PR, and rerun. At most one
such loop; what's still missing goes in the PR under "not earned yet".

## Stage 6: the brief for the real fux replacement
A panel of four, one per model, reads the scores, the reports and Stage 5 and picks the port to
start from (or "write fresh from the patterns of ports X and Y"), with reasons, 3 of 4 to settle,
otherwise the highest Stage 5 score. Then write `prompts/grift-replacement.md` in a fux worktree on
branch `prompt-grift-replacement`, in the style of grift's `prompts/embed.md` (the skill's
`references/prompt-template.md`), targeting the grift PR's API by name. Have the brief panel review
it once. Open it as a brief PR against `master` whose body holds the context, as grift PR #196 did;
don't merge.

## Machine and budget
- box2 is the owner's machine too. Every build through `$S/cargo-slot`, `-j 3` per build, every
  agent through `$S/launch-agent.sh` (it holds a lease and refuses with exit 75 when full: wait and
  retry, don't force). Read-only panelists: at most 10 running at once. Stagger starts about 40 s.
- Each agent's worktrees, target and tmp dirs under `~/.cache/claude-tmp/fuxport/<seat>/`. Never
  share a target dir. No /tmp, no git stash, nothing of yours over 10 minutes in the foreground,
  kill by pid (never `pkill -f`). Never touch the owner's fux session or panes; driven checks use
  private servers; agents you show the owner get your own tab in a workspace you name and close.
- Speed numbers come from desk only, one job at a time.
- Rough count: 4 + 10 + 4×2 + up to 4 + 2 + 4 + up to 8 + up to 10 + 4 + 4 ≈ 50–60 agent runs.
  Before launching the ten ports, check every account's quota with `$S/fleet-status.sh`; if the
  ports can't all fit before reset, run wave 1 now and wave 2 after the reset, and say so.

## Ledger, status, report
- Ledger `$R/ledger.md` (base SHAs, stage checklist, every seat with model, pid or session, status,
  decisions with why, incidents, not verified); read it first if resumed. One-line status in
  `$R/status`. Each agent writes `DONE` at a fixed path; wait for exits by pid or `wait-turn.sh`,
  never by reading screens. Course corrections arrive as dated "Update" sections at the end of this file.
- Every brief you give an agent that may commit says: **no Co-Authored-By or any AI attribution**.
  Run `$S/prepush-check.sh` before every push.
- Report unprompted at each stage's end, in a few plain lines (done / running / blocked / ETA).
- **The end:** write `DONE` in `$R/status` with both PR URLs and head SHAs, then a report of at
  most a page in `$R/report.md`: the earned needs with their counts, the owner list, the novelty
  curve (did ports 6–10 still find new needs?), each port's score, the timings against fuxth, whether
  fux needs bytes, what wasn't verified, and `cd ~/code/grift && …` / `cd ~/code/fux && …` commands
  to try it. Kill everything you started, remove every worktree and target dir under
  `~/.cache/claude-tmp/fuxport/` (keep the port branches: they are the evidence), check
  `git worktree list` in both repos.