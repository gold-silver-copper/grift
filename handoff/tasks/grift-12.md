---
id: 01a1a561-d51a-7990-8ac8-6f60f529e63c
slug: tasks/grift-12
title: "grift apps, wave 3: write a real program in today's grift and log what it lacks"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVVVVVV"
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift apps, wave 3: write a real program in today's grift and log what it lacks

History, part of [tasks/grift-1](grift-1.md). Outcome: an evidence run, no PR; its reports are the evidence later PRs cite. Run notes: box2 `~/code/notes/projects/grift/apps3-2026-11-01/`.

## Brief (`prompts/apps3.md`, box2 only, never committed; verbatim)


You are one of ten agents. Ten applications, one agent each, each in its own git worktree and fux
tab. The point is **evidence of what grift needs**
and whether grift is **complete** for these programs (`docs/VISION.md`, "What complete means"):
the program matters, and the friction log matters as much. Never ask the user anything: decide,
write down why, continue.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Read first (20 minutes, not more)
- **`docs/VISION.md`** (what grift is and why), `docs/LANGUAGE.md` (every name), `DECISIONS.md`
  (don't reopen decisions; log evidence for or against one), `crates/grift/src/library.grift`,
  `crates/grift-cli/src/main.rs` and `crates/grift-cli/src/runner.grift` (the runner's Unix set:
  `standard-input`, `standard-error`, `open-file`, `create-file`, `command-line-arguments`, `environment-variable`,
  `exit`, `run-process`), and `crates/grift-cli/tests/count-words.grift` (a script in today's
  grift).
- How wave 2 went: `~/code/notes/projects/grift/apps2-2026-11-01/tally.md` (skim) and one
  `friction.md` there for the shape of a good entry. PR #193 since added `read-form`,
  `value-to-string`, `binds?`, `standard-error` and list and string helpers (`filter`, `fold`,
  `range`, `string-split`, `string-join`, ...); don't log those as missing. Not yet there: `load`,
  file information, a reason in `parse-error`, catch by kind, sort. grift does not sandbox code
  (DECISIONS G-74).
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
`~/code/notes/projects/grift/apps3-2026-11-01/<your-name>/friction.md`. One entry per problem, as
you hit it: **tag** (`strings`, `data-structures`, `control-flow`, `errors`, `speed`, `host-api`,
`runner`, `library`, `names`, `reader-printer`, `debugging`, `other`), **what you tried**, **what
happened** (exact error or behaviour), **smallest reproduction** (a grift snippet and the command),
**severity** (`blocker`, `costly`, `annoyance`), **what you did instead** and its cost. Good news
briefly too.

## Report: questions every report answers, with numbers
`~/code/notes/projects/grift/apps3-2026-11-01/<your-name>/report.md`:
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
11. **What you used:** the ten built-in and library names you used most, with counts, and any name
    you found and chose not to use (why). Names you never needed are evidence for a later
    consolidation pass, so be exact.
Then write `DONE` in the same folder: the head SHA and one line of status.

## The applications
Every app is a Unix tool on the stock runner unless it says otherwise. Keep each done list closed:
each item checked by running the program (quote the output) or by your `test.sh`.

### format: a formatter for grift source
`main.grift FILE` prints FILE re-indented in one consistent style. Done:
1. Formatting any `.grift` file in this repo gives text that reads back to the same forms
   (`read-form`; check all of them) and formatting twice changes nothing.
2. Comments are kept, in place.
3. Long forms break across lines by a rule you state (width 100); short ones stay on one line.
4. A file that doesn't read is an error naming where, and the file is left alone.
5. `--check` exits 1 and names the files that would change.

### meta: a grift interpreter written in grift
An evaluator for a grift subset, itself in grift: its own contexts, `vau!`-style operatives,
`wrap`, applicatives, and the forms the subset needs. Done:
1. It runs `fib`, a closure counter, and a user-defined operative written in the subset.
2. Its operatives see their operands unevaluated and the caller's context, as grift's do.
3. Errors in the interpreted program are reported by the interpreter, naming the form.
4. Tail calls in the interpreted program run in bounded memory (say how).
5. It runs a 30-line program from this repo (or one you write) unchanged; report the slowdown
   against grift itself.

### test: a test runner for grift programs
`main.grift FILE...` loads test files where tests are written with operatives you design
(`test!`, `expect-equal!`, ...). Done:
1. Each test runs alone: a failing or raising test doesn't stop the others.
2. A failure shows the test's name, the expression, the expected and the actual values.
3. A summary line and the exit status (0 all passed, 1 otherwise).
4. Selecting tests by name on the command line.
5. Write 20+ tests for some library names with it, including tests that fail on purpose.

### diff: a line diff of two text files
`main.grift OLD NEW` prints a unified-style diff. Done:
1. A correct diff (shortest edit script by the longest common subsequence or Myers; say which).
2. Hunks with context lines and `@@ -a,b +c,d @@` headers.
3. Identical files print nothing and exit 0; different files exit 1; a missing file exits 2 with
   a message on standard error.
4. Two 2,000-line files with 50 scattered changes in acceptable time (measure; say what limits it).
5. Applying your diff with `patch` gives the new file (show it).

### query: a CSV query tool
`main.grift QUERY FILE.csv` where the query is grift forms you design (select columns, filter
rows, group and aggregate, sort). Done:
1. CSV with quoted fields, embedded commas and quotes, and a header row.
2. Select and rename columns; filter by a predicate over named columns.
3. Group by a column with `count`, `sum`, `average`, `minimum`, `maximum`.
4. Sort by one or more columns, ascending or descending, numbers as numbers.
5. Output as aligned text or as CSV; a column that doesn't exist is an error naming it.
6. A 10,000-row file in acceptable time (measure).

### markdown: Markdown to HTML
`main.grift < FILE.md` prints HTML. Done:
1. Headings, paragraphs, emphasis and strong, inline code, code blocks, links.
2. Nested lists (ordered and unordered, two levels).
3. Block quotes; HTML special characters escaped.
4. A 20-case test file with the expected HTML for each.
5. A 1,000-line document in acceptable time (measure).

### machine: an assembler and a virtual machine for a toy CPU
Your own instruction set (registers, memory as bytes, jumps, calls, a stack). `main.grift
assemble FILE.asm`, `main.grift run FILE.asm`. Done:
1. An assembler with labels, constants and helpful errors (line, what's wrong).
2. A VM that runs the program; output through an instruction that writes a byte.
3. Programs: hello, a loop summing 1..1000, recursive factorial, a sort of 100 numbers in memory.
4. A trace mode printing each instruction and the registers.
5. Instructions per second on the summing loop (measure), and what limits it.

### life: a cellular automaton
Conway's rules on a bounded or wrapping grid. `main.grift PATTERN-FILE GENERATIONS` prints
generations as text. Done:
1. Reading a pattern file (text, `.` and `#`); the grid size from the file or a flag.
2. A glider moves correctly; a blinker oscillates; a block stays (shown and tested).
3. Wrapping edges as an option.
4. A 100x100 grid for 100 generations in acceptable time (measure; say which data structure you
   chose and why, since grift has no vectors).
5. Detecting a still or repeating state and stopping with a message.

### simulate: a discrete-event simulation
A bank (or another system of your choice) with customers arriving at random times, queues and
tellers. `main.grift CONFIG-FILE`. Done:
1. An event queue ordered by time (a priority queue you build); events schedule more events.
2. A seeded random number generator written in grift (so runs repeat); exponential arrival times
   with floats.
3. Statistics: average wait, longest queue, teller use, printed at the end.
4. The config (tellers, rates, duration, seed) read from a file of grift forms.
5. 100,000 events in acceptable time (measure).

### template: a template engine
`main.grift TEMPLATE DATA` where DATA is a file of grift forms and TEMPLATE is text with
substitutions, conditionals, loops and includes in a syntax you design. Done:
1. Substitution of values by name and by path (`user.name`), escaped for HTML by default.
2. Conditionals and loops over lists, with the loop item and index.
3. Includes of other templates, with an error for an include cycle.
4. User-defined filters written in grift in the data file (e.g. `upper`).
5. An error in a template names the template file, line and what is wrong.

## Time and machine
- Time limit: 75 minutes of work. At the limit, stop and write the report with what holds.
- Keep `~/code/notes/projects/grift/apps3-2026-11-01/<your-name>/status` as one line.
- Your worktree, `CARGO_TARGET_DIR` and `TMPDIR` are in your launch message; use only those. No
  /tmp, no git stash. The machine is busy with other work: builds `-j 2`, tests `--test-threads 2`, through `$S/cargo-slot`. Nothing over
  10 minutes. Kill what you start by pid (never `pkill -f`).
- If you drive a program interactively, use your own fux workspace (named in your launch message;
  `fux help agents` first) and close it at the end. Never touch other panes.

## Git
Commit on your branch in your worktree as you go (plain English messages). Commit only as the
git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or `--author`. No
Co-Authored-By or any AI attribution. **Don't push and don't open a PR.**