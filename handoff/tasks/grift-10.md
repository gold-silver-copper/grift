---
id: 01a1a561-cfb8-7b33-96fa-54538d639520
slug: tasks/grift-10
title: "grift apps, wave 2: write a real program in today's grift and log what it lacks"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVVVV"
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift apps, wave 2: write a real program in today's grift and log what it lacks

History, part of [tasks/grift-1](grift-1.md). Outcome: an evidence run, no PR; its reports are the evidence later PRs cite. Run notes: box2 `~/code/notes/projects/grift/apps2-2026-11-01/`.

## Brief (`prompts/apps2.md`, box2 only, never committed; verbatim)


You are one of twelve agents. Four applications are each written three ways, one agent per
version, each in its own git worktree and fux tab. The point is **evidence of what grift needs**
and whether grift is **complete** for these programs (`docs/VISION.md`, "What complete means"):
the program matters, and the friction log matters as much. Never ask the user anything: decide,
write down why, continue.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Read first (20 minutes, not more)
- **`docs/VISION.md`** (what grift is and why), `docs/LANGUAGE.md` (every name), `DECISIONS.md`
  (don't reopen decisions; log evidence for or against one), `crates/grift/src/library.grift`,
  `crates/grift-cli/src/main.rs` and `crates/grift-cli/src/runner.grift` (the runner's Unix set:
  `standard-input`, `open-file`, `create-file`, `command-line-arguments`, `environment-variable`,
  `exit`, `run-process`), and `crates/grift-cli/tests/count-words.grift` (a script in today's
  grift).
- How wave 1 went: `~/code/notes/projects/grift/apps-2026-11-01/tally.md` (skim) and one
  `friction.md` there for the shape of a good entry. Wave 1 ran on older grift with older names;
  many of its findings are fixed.
- grift in brief: a Lisp built entirely on fexprs (`vau!`); `if!` (both branches), `when!`,
  `conditional!`, `let!`, `define!`, `lambda!` are library code. Values: integers (checked),
  floats, booleans (no truthiness), symbols, pairs, strings (immutable, byte-addressed), contexts
  (the only mutable thing: their bindings), combiners. Errors: `(raise (error (quote! kind) ...))`,
  `catch!`, `finally!`. No continuations, no vectors, no object system (records and objects are
  contexts).

## Rules for every version
- **Your directory:** `apps/<your-name>/` (given in your launch message). grift code in `.grift`
  files there. Use the stock runner (`cargo run --release -p grift-cli -- apps/<your-name>/main.grift
  ARGS`) unless your version says it needs a host crate; then make one in your directory with its
  own `[workspace]` (like `examples/emacs`) and log each host function with why it had to be Rust.
- **No changes to `crates/`.** The findings must be comparable. When grift blocks you, log it with a
  reproduction, work around it in your program, and say what the workaround cost. A blocker you
  can't get around is a finding: stop that part and say so.
- **Your done list** is in your app's section below: a closed list, each item checked by running the
  program (quote the output) or by a test file your directory runs.
- **Keep it a real program**, written as a grift programmer would, with grift's naming rules
  (`docs/VISION.md`, "Names") for your own definitions too.

## The friction log
`~/code/notes/projects/grift/apps2-2026-11-01/<your-name>/friction.md`. One entry per problem, as
you hit it: **tag** (`strings`, `data-structures`, `control-flow`, `errors`, `speed`, `host-api`,
`runner`, `library`, `names`, `reader-printer`, `debugging`, `other`), **what you tried**, **what
happened** (exact error or behaviour), **smallest reproduction** (a grift snippet and the command),
**severity** (`blocker`, `costly`, `annoyance`), **what you did instead** and its cost. Good news
briefly too.

## Report: questions every report answers, with numbers
`~/code/notes/projects/grift/apps2-2026-11-01/<your-name>/report.md`:
1. **The done list:** each item works (quote the output), partly, or not.
2. **Complete?** Could this program be written in grift as it is, with no blocker? If not, exactly
   what blocked it.
3. **Lines:** grift code lines, and how many are general helpers the library lacks (list them);
   Rust host lines if any, each host function with why.
4. **Errors and control:** did `catch!`, `finally!`, `raise` and `error` cover what you needed? What
   else would have saved code, and how many lines?
5. **Data:** did contexts work as your records and tables? Floats? Strings? What did you miss, with
   the code it would replace?
6. **Names:** which names did you guess wrong, or reach for and not find (list them, with what you
   tried first)? Which read well?
7. **The runner** (if you used it): what was missing or awkward in its Unix set?
8. **Speed:** one timing of your hot path (release, one run, the command and the number), and
   whether it was acceptable.
9. **Top five friction entries,** most costly first, each linking its entry.
10. **Proposals** for grift's core, library or runner, each with evidence (not done).
Then write `DONE` in the same folder: the head SHA and one line of status.

## The applications

### sheet: a spreadsheet engine (a Unix tool)
Cells hold grift expressions; `main.grift SHEET-FILE` loads a sheet, reads commands from stdin
(set a cell, show a cell, print the grid, save), recalculates, and prints. Done:
1. A sheet file of cells (`A1 3`, `B1 (add A1 2.5)`, ...) loads; the grid prints aligned, integers
   and floats formatted.
2. Setting a cell recalculates every cell that depends on it, and only those (show how you know).
3. A cycle (`A1` -> `B1` -> `A1`) is an error naming the cells in the cycle; the rest of the sheet
   still works.
4. An error in one cell's formula (e.g. `(divide 1 0)`, a type error) shows in that cell only.
5. Range functions over a column or a rectangle (`sum`, `average`, `count`), and a user-defined
   function in the sheet file used by cells.
6. Saving writes the sheet file back so loading it again gives the same values.
Versions:
- **sheet-contexts:** the sheet is a context; each cell is a binding; formulas evaluate in it.
- **sheet-graph:** an explicit dependency graph and a recalculation in dependency order.
- **sheet-lazy:** cells are lazy, memoised values invalidated when something they read changes.

### adventure: an original text adventure with a world-building language
An original small game (your own setting and story; not a recreation of an existing game). The
player types commands on stdin. Done:
1. At least six places, items you can take and drop, an inventory, and directions between places.
2. A parser for `verb`, `verb noun` and `verb noun preposition noun` (`unlock door with key`),
   with helpful messages for words it doesn't know.
3. A puzzle that needs two items used in the right order, and an ending.
4. Saving and loading the game state to a file, and a scripted playthrough (`main.grift <
   walkthrough.txt`) that reaches the ending.
5. A mistake in the world definition (a direction to a place that doesn't exist, an item in two
   places) is an error naming what is wrong and where, before play starts.
Versions:
- **adventure-language:** the world is written in a world-building language made of operatives
  (`place!`, `item!`, `verb!`, ...).
- **adventure-tables:** the world is plain data (lists) that an engine interprets.
- **adventure-contexts:** every place and item is a context holding its own operations and state.

### build: a make-like build tool (a Unix tool)
`main.grift [-n] [TARGET...]` reads a build file in the current directory and brings targets up to
date by running commands (`run-process`). Done:
1. Rules: a target, its inputs, and the commands that make it; a default target.
2. Only out-of-date targets are rebuilt (show it: a second run does nothing; touching one input
   rebuilds only what depends on it). Decide how you tell what is out of date and say why.
3. `-n` prints what would run without running it.
4. A cycle, a missing input with no rule, and a failing command each stop with a clear message and a
   non-zero exit status; a failing command's output is shown.
5. Variables and a function written in the build file (e.g. turning `a.c` into `a.o`).
6. Build a real small project with it (e.g. a few C or Rust files, or text files through `sed`),
   and show the commands that ran.
Versions:
- **build-language:** rules are written in a build language made of operatives.
- **build-data:** rules are plain data the tool interprets.
- **build-hashes:** out-of-date is decided by content (a hash of each input, kept in a state file),
  not times.

### sandbox: a host that runs untrusted grift plugins (a Rust host crate)
A Rust program embedding grift that loads plugin files and gives each one only what it is allowed.
Done:
1. Each plugin runs in its own context with a chosen set of capabilities (at least: logging, a
   key-value store scoped to that plugin, and reading the current time), and nothing else.
2. A plugin can't see or change another plugin, the host's state outside its capabilities, or the
   built-in context; it can't forge a capability.
3. A plugin that runs forever is stopped after a time limit (the interrupt flag) and the host goes
   on; a plugin that raises an error is reported and the host goes on.
4. The host calls functions a plugin defines (an event, e.g. `on-message`) with values, and gets
   values back.
5. At least 20 escape attempts as test plugins (reach the built-in context, redefine library names
   to intercept the host, find a host function, forge a store, run forever inside `catch!`, exhaust
   memory, ...), each with its result. Every attempt fails safely or is logged as a finding.
Versions:
- **sandbox-contexts:** capabilities are bindings in a restricted context built for each plugin.
- **sandbox-capabilities:** capabilities are contexts of operations handed to a plugin's entry
  function, which gets nothing else.
- **sandbox-attacker:** build the host too, but spend at least half your time attacking it: 40+
  escape attempts, and for each one that works, the smallest reproduction and whether the hole is in
  your host or in grift.

## Time and machine
- Time limit: 75 minutes of work. At the limit, stop and write the report with what holds.
- Keep `~/code/notes/projects/grift/apps2-2026-11-01/<your-name>/status` as one line.
- Your worktree, `CARGO_TARGET_DIR` and `TMPDIR` are in your launch message; use only those. No
  /tmp, no git stash. Builds `-j 4`, tests `--test-threads 4`, through `$S/cargo-slot`. Nothing over
  10 minutes. Kill what you start by pid (never `pkill -f`).
- If you drive a program interactively, use your own fux workspace (named in your launch message;
  `fux help agents` first) and close it at the end. Never touch other panes.

## Git
Commit on your branch in your worktree as you go (plain English messages). Commit only as the
git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or `--author`. No
Co-Authored-By or any AI attribution. **Don't push and don't open a PR.**