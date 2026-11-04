---
id: 01a1a561-eec4-73c1-a0ad-969f0a3c3f5b
slug: tasks/grift-19
title: "grift in a terminal multiplexer: fux read, fuxth against grift, five spikes"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVVVVVVVVVVVVV"
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift in a terminal multiplexer: fux read, fuxth against grift, five spikes

History, part of [tasks/grift-1](grift-1.md). Outcome: an evidence run, no PR; its reports are the evidence later PRs cite. Run notes: box2 `~/code/notes/projects/grift/mux-2026-11-02/`.

## Brief (`prompts/mux.md`, box2 only, never committed; verbatim)


The owner may replace **fuxth** (fux's Forth: its config, keys, command prompt, saved sessions and
every `fux` command line) with **grift**. Before deciding, they want: fux's source read with that
question in mind, grift measured against fuxth, and five terminal multiplexers built as spikes on
grift + **fux-vt** (fux's terminal emulator crate) + **fuxix** (fux's system-call crate), each finding
what grift needs. This brief is shared by seven agents; your launch message says which section is
yours. Read the common parts and your section. Never ask the user anything: decide, write down why,
continue. Spikes are asked for here: they are working programs that are allowed to be rough, and
none of them is a PR.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Sources (read only, never edit, never build inside them)
- fux at `1d7ba0a6`: `~/.cache/claude-tmp/grift-mux/src/fux` (a detached worktree). Crates: `src/`
  (the client and fux's fuxth embedding, `src/script/`), `fux-server/`, `fux-wire/`, `fux-vt/`,
  `fuxix/`, `fuxth/` (+ `fuxth/core`), `fuxth-run/`. Docs: `README.md`, `VISION.md`, `DECISIONS.md`,
  `docs/`, `fux help TOPIC` (the installed `fux` is the user's; run only `fux help ...` with it).
- grift at `a0eec1c` (PR #195 merged): your own worktree if you have one, else
  `~/.cache/claude-tmp/grift-mux/src/grift`. Read `docs/VISION.md`, `docs/LANGUAGE.md`, `DECISIONS.md`,
  `crates/grift/src/lib.rs` (the embedding API) and `examples/emacs/` (a host).
- Earlier evidence: `~/code/notes/projects/grift/fuxth-vs-grift.md` (a read-only comparison at
  699c95b: sizes, design, `fib 25` 14.9 ms fuxth against 131 ms grift);
  `~/code/notes/projects/grift/apps4-2026-11-02/tally.md` section 8 (an embedding agent's Rust API
  findings); `prompts/earned4.md` "Not in this PR" (what grift still lacks, known).

## Common rules
- Output folder: `~/code/notes/projects/grift/mux-2026-11-02/<your name>/` (create it). Write a
  one-line `status` there first and keep it current; a `report.md` (structure in your section) and,
  for spikes, `friction.md` (one entry per thing grift made hard: what, a reproduction, the cost,
  the workaround, what would fix it). When done: `DONE` in `status`, with your branch head if any.
- Machine: only your worktree, your target dir and your tmp dir (all under
  `~/.cache/claude-tmp/grift-mux/<your name>/`, already exported as `CARGO_TARGET_DIR`/`TMPDIR`); no
  /tmp, no git stash; `-j 4` for every cargo build or test, builds through `$S/cargo-slot`; nothing over
  10 minutes; no benchmark campaigns (one run per measurement, three only for the timing table in the
  performance section); kill what you start by pid (never `pkill -f`).
- fux: you may open panes and tabs **in a workspace you name** (`fux help agents` first) to drive
  your program, and close that workspace at the end. Never type into, kill, split or resize any pane
  you didn't open; never `fux kill-server`.
- Don't change grift's crates (`crates/`) or any fux crate. A spike that can't go on without a grift
  change records it in friction.md as a blocker, works around it in its host if it can, and, only if
  it can't, makes the smallest change in one separate commit titled `grift change: ...`, which then
  counts as a finding.
- Commit only as the git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or
  `--author`. No Co-Authored-By or any AI attribution. Never push, never open a PR.
- Limit: about 2.5 hours. At the limit, write the report with what you have and say what's missing.

## The spikes (agents `config`, `logic`, `layouts`, `automation`, `reactive`)
Your worktree is `~/.cache/claude-tmp/grift-mux/<name>/wt`, branch `spike-mux-<name>` of grift at
a0eec1c. Put your program in `spikes/mux-<name>/` as its own Cargo package with its own `[workspace]`
table (so grift's workspace doesn't take it), path dependencies on `../../crates/grift` and on
`~/.cache/claude-tmp/grift-mux/src/fux/fux-vt` and `.../fuxix` (absolute paths). Read the parts of
fux your design mirrors first (where fux already solved it, follow it unless grift makes something
better: fux's PTY handling, resize, the client's drawing in `src/client.rs`/`src/screens.rs`,
`fux-server`'s model).

**Every spike, done means:**
1. It runs in a real terminal (your own fux tab): starts a shell in a pane, splits into at least two
   panes side by side or stacked, switches focus by keys, sends typed keys to the focused pane's
   program, draws each pane's screen from fux-vt (text, colours and the cursor, enough for `ls
   --color`, `htop` or `vim` to be usable), resizes when the terminal does, and quits cleanly with
   every child reaped. Your design's own part (below) works on top of that.
2. A scripted check (a `check.sh` beside it) drives it through fux (`send-keys`, `capture`) and checks
   what the screen shows; it passes. Quote a captured screen in the report.
3. `report.md`: the design in a paragraph and a diagram of who owns what (Rust or grift); the lines of
   grift and of Rust (non-blank, non-comment); the hot paths and their cost, measured once (per key,
   per output chunk, per redraw, per command, as your design has them), and whether grift is fast
   enough for each; **what grift needs, ranked** (from friction.md: each with its evidence and whether
   it is library grift, a primitive, the embedding API, or the runner); what fuxth would have made
   easier or harder for this design, and what grift made easier; what you'd keep if this design were
   built for real.

**config**: fux's split, with grift where fuxth is. Rust owns the event loop, panes, layout and
drawing; grift is the config file, the key bindings, a command prompt (a key opens a line at the
bottom; the text is a grift program) and a command line (`mux-config run '<grift>'` against a running
instance over a Unix socket). Port a real slice of fux's words to grift operations under grift's
naming rules (split, focus, new tab, send keys, capture, options, bind a key; read
`src/script/words.rs`, `keys.fux`, `client-defaults.fux`, `library.fux`), port fux's default key
table to grift, and compare: each word's fuxth source against yours, and startup cost (a fresh
interpreter with its library and the config, as each `fux` command line pays). This is the closest to
the real replacement; say what the replacement would cost in fux.

**logic**: grift owns the multiplexer. The model (workspaces, tabs, panes as contexts), layout, focus,
key dispatch, the prefix-key state machine and the decision of what to redraw are grift; Rust is a
thin host that gives grift primitives: spawn a program on a PTY, read and write it, feed bytes to a
fux-vt parser, read a screen's cells or rows, draw a rectangle of cells, poll for input and output
with a timeout. Find where grift is too slow (every key and every output chunk passes through grift)
and what the host boundary needs.

**layouts**: a tiling window manager's way: layouts are grift functions from a tree of panes and a
size to rectangles (as xmonad's or dwm's layouts are code): at least tall, wide, grid, a spiral, a
tabbed/monocle one, and a nested user-written layout combinator (written with `vau!` if that reads
better than `lambda!`), switched by keys and recomputed on every split, close and resize. Rust does
the rest. Show a user adding a layout in the config without touching Rust.

**automation**: fux's agents side: a server holding the panes, and a client that sends a grift
program per request over a Unix socket, evaluated in the server (fux's "every command line is a
program"). Give requests: a pane's screen as text and as records, a version per pane that moves on
every change, waiting for text to appear or for a version to move (with a timeout), sending keys,
the exit status of a pane's program, and a journal of events a client can read from a position.
Write three agent-style scripts with it (start a program, wait for its prompt, answer it, check the
result). Each request gets a time budget (grift's interrupt); report how per-request cost, keeping
values between requests, errors returned to the client and concurrency (two clients at once) went.

**reactive**: output-driven hooks. grift handlers run on events (output in a pane matching a pattern,
a program exiting, a bell, a title change, a timer) and can act (notify, write to a pane, split, run a
program, set a status line); a status bar is a grift function of the panes' state, redrawn on change.
Run a noisy program (`yes | head -c 50M`, a build log) through it and measure what the hooks cost per
chunk and whether output keeps up; handlers get a time budget. Report what keeping state between
events and errors inside handlers needed.

## fux read (agent `fuxread`)
Read fux's source with one question: what would replacing fuxth with grift in fux take? Read, not
skim: `src/` (all), `fux-server/src/` (the model, requests, the journal; read every file's structure
and the parts fuxth touches), `fux-wire/` (what crosses the wire), `fuxth/`, `fuxth-run/`, and the
public APIs of `fux-vt` (`src/lib.rs` and its README contract) and `fuxix`. Count lines with a
script. `report.md`:
1. fux in a page: the processes, the model, the wire, who calls what; a diagram.
2. Every place fuxth is in fux: each with file:line, what it does, how values cross (records, lists,
   errors with codes, help text, callbacks, settings, saved sessions, `see`, the command prompt, the
   CLI's argument parsing), and its size.
3. The replacement, designed: what `src/script/` becomes with grift (the words as grift operations,
   records as contexts, errors, help, callbacks), what each user-visible thing looks like before and
   after (a key binding, a config, a command line, a saved session, `fux help`), the lines that go
   and come, the order you'd do it in, and the risks (start-up time per command line, error codes
   and messages, documentation, saved sessions written by today's fux, tests and cassettes).
4. What grift lacks for it, ranked, with file:line evidence on fux's side.
5. fux-vt's and fuxix's API as a spike author needs it (one page each, written early, as
   `api.md` beside your report, so the spike agents can use it: write it first, within your first 40
   minutes, and say so in `status`).
No worktree; you change nothing.

## Performance (agent `perf`)
Compare fuxth and grift as embedded languages, by measurement. Build both in release in your own
target dir, from the source folders above (copy them under your own dir if a build would write
inside them). Each number is one build per side, three runs, the median, with the machine's load
average beside it. In `report.md`:
1. **The languages**: the same programs written idiomatically in each (fuxth's own examples and
   `stdlib.fux` show its idiom; grift's `programs/` show grift's): `fib 25`; a loop of a million
   additions; building and joining a 10,000-element list; a string built from 10,000 pieces; a
   symbol- or name-keyed table with 100,000 reads and writes; sorting 10,000 integers; a closure
   called a million times; raising and catching 10,000 errors.
2. **Embedding**: a fresh interpreter with its library ready (fuxth with fux's bundled `.fux`; grift
   with its library); one host call into the language and one language call into the host (the
   per-call cost, by a loop of a million and the empty loop subtracted); memory of a fresh interpreter;
   what a typical `fux` command line costs end to end in fuxth (parse and run, e.g. `fux str work
   name str sh program run` shapes from `docs/cookbook.md`) against the same done in grift.
3. **fux's own load**: the time to load fux's bundled fuxth (`library.fux`, `keys.fux`,
   `client-defaults.fux`) against a grift translation of them (translate them; a rough faithful one).
4. **Why**: for every row where one is more than 2x the other, the reason in the implementations
   (file:line on each side: fuxth's compiler and threaded code against grift's fexpr machine, values,
   allocation, lookup), and what grift could change without breaking its rules (VISION
   "Implementation": no fast paths, library forms stay pure fexprs, one evaluation path) and how much
   it would gain, estimated, labelled as estimates.
5. **What it means for fux**: which costs a user of fux would notice (per command line, per key, per
   callback), with the numbers.
Scripts and data in your folder; no worktree.

## Synthesis (agent `synthesis`, launched after the others)
Instructions arrive in its launch message.