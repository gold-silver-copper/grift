---
id: 01a1a561-cb48-71d2-8386-07879bc1c499
slug: tasks/grift-8
title: "grift emacs: build an Emacs-style editor whose editor is grift code"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVV"
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift emacs: build an Emacs-style editor whose editor is grift code

History, part of [tasks/grift-1](grift-1.md). Outcome: an evidence run, no PR; its reports are the evidence later PRs cite. Run notes: box2 `~/code/notes/projects/grift/emacs-2026-11-01/`.

## Brief (`prompts/emacs.md`, box2 only, never committed; verbatim)


You are one of three agents, each in its own fux tab and git worktree, each building an
Emacs-style text editor on grift (gold-silver-copper/grift, branch `pure` at `0a0058c`). The
point is twofold: a working editor, and **evidence of what grift gets wrong or makes hard** when a
real program is written in it. The friction log matters as much as the editor. Never ask the user
anything: decide, write down why, continue.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Read first (15 minutes, not more)
- `README.md`, `docs/LANGUAGE.md`, `DECISIONS.md` (G-1..G-16: the owner's decisions; do not reopen
  them, but log evidence against one if you find it), `crates/grift/src/library.grift`,
  `crates/grift-cli/src/main.rs` (how a host registers functions), `crates/grift/src/lib.rs`.
- grift is a tiny Kernel-style Lisp: `$vau` is the only special primitive; `$define!`, `$if`,
  `$lambda`, `$let`, `$cond` are library code. Values: integers, symbols, pairs, immutable strings,
  environments, combiners. Environments are mutable (`$define!` redefines). Errors go to the host as
  `Err` with a code symbol and the values involved; grift can't catch them. Printing a combiner
  shows its source; `(bindings env)` lists names.

## What "Emacs-style" means here
A small Rust host (terminal, files, maybe the text buffer) and an editor that is grift code: the
commands, the keymap, the minibuffer, the echo area, and self-documentation. Everything the user
does is a grift command bound to a key, and the user can redefine any of it while the editor runs.
This is the genre (a Lisp-extensible editor), not a copy of GNU Emacs: no elisp compatibility,
pick the keys below and stop.

## Your variant (where the Rust/grift line is)
Your launch message names your variant. Stay inside it; the three are compared afterwards.
1. **thin**: the host gives grift only bytes and files: read a key (as an integer byte, or a
   symbol for escape sequences if you decide), write bytes/strings to the terminal, terminal size,
   read a file, write a file, and whatever string<->data conversion you need. The buffer,
   redisplay, cursor and keymap are all grift data and code.
2. **buffer**: the host owns the text buffer (a gap buffer or rope, your choice) with point and
   mark, and redisplay (it draws the buffer, the mode line and the echo area each loop). grift owns
   the commands, the keymap, the minibuffer and everything a user would customize. This is the
   real Emacs split.
3. **screen**: the host owns the terminal as a grid of cells (`put-string row col string`,
   `clear`, `move-cursor`, `flush`, key events as symbols like `C-x`, `M-x`, `left`, or a
   one-character string). grift owns the buffer (as grift data of your choosing), redisplay into
   the grid, commands and keymap.

## Done means all of these hold (driven in a real terminal, not just tests)
1. `cargo run -p grift-emacs-<variant> -- FILE` opens FILE (a missing FILE starts empty and creates
   it on save; no other guessed paths). `--init INIT.grift` loads an init file first; no default
   init path.
2. Moving: C-f C-b C-n C-p C-a C-e, arrow keys, M-< M->. Editing: printable characters insert;
   Enter splits the line; DEL and C-d delete; C-k kills to end of line (on an empty line, the
   newline); C-y yanks the last kill. UTF-8 text survives a round trip (a multi-byte character
   moves and deletes as one).
3. C-x C-s saves (exact bytes; the echo area says so). C-x C-c quits, restoring the terminal.
4. M-x reads a command name in the minibuffer and runs it. M-: reads a grift expression, evaluates
   it in the editor's environment and shows the result in the echo area.
5. Live extension: with M-: (or an init file) define a new command and bind it to a key, and
   redefine an existing command; both take effect without a restart.
6. Self-describing: C-h f shows a command's source (grift prints combiners with their body); C-h b
   lists the key bindings.
7. Errors never kill the editor: a grift error from a command (e.g. `(car 1)` via M-:) shows its
   code and values in the echo area and editing continues. A Rust panic or a lost terminal is a bug.
8. Typing feels instant on a 2,000-line file (say whether it does; one rough timing is enough, no
   benchmark campaign).

## Where you may write
- Your crate: `examples/emacs-<variant>/` (its own `[workspace]` like `examples/chip`, or added to
  the root workspace's `exclude`; your choice). Rust edition 2024. The host may use crates
  (a terminal crate such as crossterm is fine); keep it small. Non-test Rust must not panic (no
  unwrap/expect/indexing; deny the clippy lints as `crates/grift/Cargo.toml` does).
- The grift side lives in `.grift` files in your crate, loaded with `include_str!`.
- **The core (`crates/grift`)**: change it only when the editor can't reasonably be built
  otherwise (example: the host API has no way to make a string or a pair from Rust). Each core
  change is its own commit, is minimal, keeps `crates/grift`'s gate passing, and gets a friction
  entry saying why. Don't refactor the core and don't add language features for convenience:
  write the convenience in grift first and log what it cost.
- Nothing outside your worktree.

## The friction log (the main output besides the editor)
`~/code/notes/projects/grift/emacs-2026-11-01/<variant>/friction.md`. One entry per problem, as
you hit it, not reconstructed at the end:
- **what you tried** (one line), **what happened** (the exact error or behaviour), **smallest
  reproduction** (a grift snippet and the command that runs it, or a Rust snippet against the host
  API), **severity**: `blocker` (needed a core change or can't be done), `costly` (a workaround
  of more than a few lines or a real slowdown), `annoyance`; **what you did instead**.
- Include good news briefly too: what worked better than expected.
Kinds of thing worth logging: missing host-API pieces; string handling; performance of grift code
(measure once: what was slow, how slow); error messages that didn't say enough; the library
missing something every program needs; tail calls or GC surprises; decisions (G-n) that hurt or
helped, with the evidence.

## Report (when done, or at the time limit)
`~/code/notes/projects/grift/emacs-2026-11-01/<variant>/report.md`:
- the done list above, each item: works (how you checked it in the real terminal, quoting the
  screen), partly, or not;
- lines: Rust host (code lines), grift editor (code lines), core changes (diff stat);
- the host functions you added, each with one line on why it had to be host code;
- the top five friction items, most costly first, each linking its entry;
- what you'd change in grift's core or library, as proposals with evidence (not done).
Then write `DONE` in the same folder (head SHA, one line on status).

## Time and machine
- Time limit: 2 hours of work. At the limit, stop, write the report with what holds, and finish.
- Keep `~/code/notes/projects/grift/emacs-2026-11-01/<variant>/status` as one line: what you're
  doing now.
- Your worktree, `CARGO_TARGET_DIR` and `TMPDIR` are given in your launch message; use only those.
  No /tmp, no git stash. Builds: `-j 4`, tests `--test-threads 4`, through `$S/cargo-slot`. Nothing
  over 10 minutes. Kill what you start by pid (never `pkill -f`).
- Drive your editor in your own fux tab (`fux help agents` first), with your own workspace name
  given in your launch message; never touch other panes. Use a scratch file under your `TMPDIR` as
  the file you edit, and check saved bytes with `cmp` or `od -c`. Close your tabs at the end
  (`fux str <your-workspace> workspace kill` only for the workspace you created, or kill your panes
  by id).

## Git
Commit on your branch in your worktree as you go (plain English messages: what the program now
does). Commit only as the git-config identity (`gsc <stephen.korzen@gmail.com>`); never
`-c user.*` or `--author`. No Co-Authored-By or any AI attribution. **Don't push and don't open a
PR**: the orchestrator compares the three and decides what goes upstream.