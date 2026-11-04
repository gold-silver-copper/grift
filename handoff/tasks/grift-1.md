---
id: 01a1a55c-47d2-7d82-9009-7192b7fe2225
slug: tasks/grift-1
title: "Plan and handoff: grift replaces fuxth in fux"
type: task
status: active
priority: high
tags: [plan, fux]
external_refs:
  - system: github
    id: "gold-silver-copper/grift#199"
    label: the handoff PR
    url: "https://github.com/gold-silver-copper/grift/pull/199"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# Plan and handoff: grift replaces fuxth in fux

**Links:** Depends on [tasks/grift-2](grift-2.md); [gold-silver-copper/grift#199](https://github.com/gold-silver-copper/grift/pull/199) (the handoff PR).

The handoff for grift: where it stands, what comes next and in what order, and where everything is. Any session picking grift up starts here: `cd ~/code/grift-kb && git-kb board --json`, then this document.

## Goal
grift replaces fuxth, fux's Forth, inside fux's own architecture: Rust keeps the event loop, panes, layout and drawing; grift becomes the config, key bindings, command prompt, saved sessions and every `fux` command line. The owner's rulings for this are [decisions/grift-in-fux](../decisions/grift-in-fux.md).

## Where grift stands (2026-11-03, `pure` at 7ff695e)
- A Kernel-style Lisp on fexprs (`vau!` is the one primitive operative), in Rust with no dependencies and no `unsafe`. Crates `grift` (core) and `grift-cli` (the Unix runner); 13 programs in `programs/`; the `examples/emacs` editor. Guide `docs/VISION.md`, reference `docs/LANGUAGE.md`, decisions `DECISIONS.md` (append-only, G-1 to G-141).
- The embedding work fux needs is merged: grift #198 ([tasks/grift-2](grift-2.md)). A Rust host can keep grift values, stop a run from another thread, build and read contexts, load named sources with `file:line` errors, and is refused (`stale-value`, `foreign-value`) when it passes a freed value or another interpreter's.
- Gate: `GATE_FMT=1 ~/.claude/skills/agentic-coding/scripts/rust-gate.sh origin/pure` plus `(cd examples/emacs && cargo clippy --all-targets -- -D warnings)`. CI runs one `check` job.

## Next, in order
1. [tasks/grift-4](grift-4.md): ten independent ports of fux onto grift, panels of four models deciding what grift has earned, one grift PR with that set. Ready (its precondition holds); not started.
2. [tasks/grift-3](grift-3.md): an immutable bytes value. Independent of the ports; [tasks/grift-4](grift-4.md) reports whether fux needs it.
3. [tasks/grift-5](grift-5.md): the fux PR that does the replacement, from the brief [tasks/grift-4](grift-4.md) writes.

## Choices left to the owner
1. Start [tasks/grift-4](grift-4.md) now? It is about 50 to 60 agent runs on box2 (ten ports up to 8 hours each), timings on desk.
2. Bytes ([tasks/grift-3](grift-3.md)) before, after or alongside the ports?
3. This KB has no sync remote and isn't in git: it is one directory on alarm (`~/code/grift-kb/.kb`) with no backup. Runs on box2 get the documents they need copied into their brief. A remote would back it up and let box2 sessions read and update it directly.

## How work is run (owner's rules, 2026-11-02 and 2026-11-03)
- Every judgment by a panel of four models: Opus `claude-opus-5-5`, Fable `claude-fable-5-1` (Claude), Sol `gpt-6.1-sol`, Astra `gpt-6-astra` (Codex), through `~/.claude/skills/agentic-coding/scripts/launch-agent.sh`. Never one agent alone deciding.
- Prompts are documents in this KB, not files under `prompts/`.
- Builds and runs on box2 (`ssh box2`, a build machine). Its `~/code/grift` is behind `origin/pure`: fetch first. Speed numbers only from desk, reached from box2 over koh (server `desk`; in `ssh box2 '…'` call it as `~/.cargo/bin/koh`); see `~/code/notes/build-servers.md`.
- Orchestrators do everything operational (restarts, repairs, coordination); the owner is asked only for owner decisions. No Co-Authored-By or any AI attribution anywhere.
- grift is the owner's repo: agents may open and merge PRs when the owner says "merge it".

## Where the evidence is
All of it is on box2's disk only: `~/code/notes` there is a git repo, but every grift folder below is untracked and unpushed. alarm's `~/code/notes` isn't a git repo.
- box2 `~/code/notes/projects/grift/`: `mux-2026-11-02/` (five fux-shaped spikes, the fuxth reading, the synthesis report, `owner-decisions.md`), `apps*-2026-11-0*/` (four waves of programs written in grift and their tallies), `embed-2026-11-03/` (the #198 run), earlier runs.
- box2 `~/code/grift` local branches: `spike-mux-*`, `app4-*`, `embed-opus`, `embed-sol`, `embed-tests`, `port-*-opus`, `embed-sol-port-*`. They are evidence; keep them.
- alarm `~/code/notes/projects/grift/orchestrator-2026-11-02.md`: the review that started the #198 run.
- fuxth itself: fux's `src/script/` (Rust plus `library.fux`, `keys.fux`, `client-defaults.fux`) and the `fuxth/` crate.

## History
Every earlier grift prompt is a completed task here, in run order, each with its brief verbatim and its outcome: [tasks/grift-6](grift-6.md) the rewrite (#187), [tasks/grift-7](grift-7.md) apps wave 1, [tasks/grift-8](grift-8.md) the emacs editor, [tasks/grift-9](grift-9.md) improve (#188), [tasks/grift-10](grift-10.md) apps wave 2, [tasks/grift-11](grift-11.md) improve2 (#190), [tasks/grift-12](grift-12.md) apps wave 3, [tasks/grift-13](grift-13.md) the vision guide (#191), [tasks/grift-14](grift-14.md) `when!` (#192), [tasks/grift-15](grift-15.md) grow (#193), [tasks/grift-16](grift-16.md) consolidate (#194), [tasks/grift-17](grift-17.md) apps wave 4, [tasks/grift-18](grift-18.md) earned4 (#195), [tasks/grift-19](grift-19.md) the fux spikes (mux), then [tasks/grift-2](grift-2.md) embedding (#198). The apps waves, emacs and mux were evidence runs with no PR.

## Open PRs
- grift #196 and #197 only added `prompts/embed.md` and `prompts/bytes.md`. Both briefs now live here ([tasks/grift-2](grift-2.md), [tasks/grift-3](grift-3.md)), so the handoff PR closes both. Any `prompts/` directory left in a grift checkout is a stale copy of these documents, not work to commit.

## Acceptance criteria
- [x] Embedding work merged ([tasks/grift-2](grift-2.md))
- [ ] fux ports and the earned grift PR ([tasks/grift-4](grift-4.md))
- [ ] Bytes value ([tasks/grift-3](grift-3.md))
- [ ] fux runs on grift; fuxth in `fux/archived/` ([tasks/grift-5](grift-5.md))