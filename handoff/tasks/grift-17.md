---
id: 01a1a561-e63a-7d81-ad46-2bbc60ad0236
slug: tasks/grift-17
title: "grift wave 4: write mountains of grift and log what it lacks"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVVVVVVVVVVVV"
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift wave 4: write mountains of grift and log what it lacks

History, part of [tasks/grift-1](grift-1.md). Outcome: an evidence run, no PR; its reports are the evidence later PRs cite. Run notes: box2 `~/code/notes/projects/grift/apps4-2026-11-02/`.

## Brief (`prompts/apps4.md`, box2 only, never committed; verbatim)


You are one of ten agents, each with a domain, each in its own git worktree and fux tab. Earlier
waves wrote one application each; this wave is about **volume**: write a lot of ordinary grift, many
programs, the way a working programmer would, so the everyday costs show up by count. The point is
**evidence of what grift needs**
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
- How wave 3 went: `~/code/notes/projects/grift/apps3-2026-11-01/tally.md` (skim) and one
  `friction.md` there for the shape of a good entry. `programs/` holds 13 real grift programs: read
  two before you start, they are today's idiom. PR #194 removed `define-in!` (use `bind` with the
  parameters quoted), `newline` (use `(display "\n")`), `binds!?`, `inert?`, `ignore?`.
- **Known and verified on today's grift; don't write a new entry for these, but keep a count of each
  time one costs you, with the lines it cost**, in a `known` table at the top of `friction.md`:
  (K1) `parse-error` gives only a line and column, no reason, and an unclosed form is reported at
  the end of the file, not where it opened; (K2) no `load`, so one file per program, and no way for a
  script to know its own path; (K3) catching one kind of error is a `catch!` plus an `if!` and a
  re-raise; (K4) no way to write or read bytes that aren't UTF-8; (K5) library forms (`let!`,
  `conditional!`, `sequence!`, `or!?`, `and!?`) cost several times an `if!` in a hot loop; (K6) no
  sort. grift does not sandbox code (DECISIONS G-74).
- grift in brief: a Lisp built entirely on fexprs (`vau!`); `if!` (both branches), `when!`,
  `conditional!`, `let!`, `define!`, `lambda!` are library code. Values: integers (checked),
  floats, booleans (no truthiness), symbols, pairs, strings (immutable, byte-addressed), contexts
  (the only mutable thing: their bindings), combiners. Errors: `(raise (error (quote! kind) ...))`,
  `catch!`, `finally!`. No continuations, no vectors, no object system (records and objects are
  contexts).

## Rules for every agent
- **Your directory:** `apps/<your-name>/` (given in your launch message). grift code in `.grift`
  files there. Use the stock runner (`cargo run --release -p grift-cli -- apps/<your-name>/PROGRAM.grift
  ARGS`, or the built binary) unless your domain says it needs a host crate; then make one in your directory with its
  own `[workspace]` (like `examples/emacs`) and log each host function with why it had to be Rust.
- **No changes to `crates/`.** The findings must be comparable. When grift blocks you, log it with a
  reproduction, work around it in your program, and say what the workaround cost. A blocker you
  can't get around is a finding: stop that part and say so.
- **Your done list** is in your domain's section below: a closed list of programs. Each program is
  a `.grift` file (or a small directory) under `apps/<your-name>/`, checked by one `test.sh` there
  that runs every program and compares what it prints with what it should (expected values you
  worked out, not output copied from a first run).
- **Volume with quality:** at least 2,500 lines of grift in total (non-blank, non-comment), written
  as a grift programmer would, readable, with grift's naming rules (`docs/VISION.md`, "Names") for
  your own definitions. No generated or padded code: a program earns its place by doing something.
  Don't build your own private standard library to dodge the language; when you'd want a helper,
  write it where you need it and log it, so the count shows what the library lacks.

## The friction log
`~/code/notes/projects/grift/apps4-2026-11-02/<your-name>/friction.md`. One entry per problem, as
you hit it: **tag** (`strings`, `data-structures`, `control-flow`, `errors`, `speed`, `host-api`,
`runner`, `library`, `names`, `reader-printer`, `debugging`, `other`), **what you tried**, **what
happened** (exact error or behaviour), **smallest reproduction** (a grift snippet and the command),
**severity** (`blocker`, `costly`, `annoyance`), **what you did instead** and its cost. Good news
briefly too.

## Report: questions every report answers, with numbers
`~/code/notes/projects/grift/apps4-2026-11-02/<your-name>/report.md`:
1. **The done list:** each program works, partly, or not; total lines; `test.sh`'s result.
2. **Complete?** Could each program be written in grift as it is, with no blocker? If not, exactly
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
8. **Speed:** the slowest three programs (release, one run each, the command and the number), and
   whether each was acceptable.
9. **Top five friction entries,** most costly first, each linking its entry.
10. **Proposals** for grift's core, library or runner, each with evidence (not done).
11. **The known table:** K1-K6 with counts and lines.
12. **Helpers you wrote more than once** across your programs, with how many times: the strongest
    evidence for the library.
13. **What you used:** the ten built-in and library names you used most, with counts, and any name
    you found and chose not to use (why). Names you never needed are evidence for a later
    consolidation pass, so be exact.
Then write `DONE` in the same folder: the head SHA and one line of status.

## The domains
Each has a closed list of programs. Pick your own designs within it; keep every program original.

### algorithms
25 programs: insertion, merge and quick sort on lists; binary search over a sorted list; BFS, DFS,
Dijkstra and topological sort on graphs you represent (say how); union-find; longest common
subsequence, edit distance, knapsack and coin change by dynamic programming (say how you memoise
without vectors); permutations, combinations, power set; the sieve of Eratosthenes; gcd and modular
exponentiation; a KMP string search; Huffman coding of a text and back; a bracket matcher; n-queens.

### structures
20 data structures, each with operations and tests: a queue (two lists), a deque, a binary heap and
a pairing heap, a binary search tree, a red-black or AVL tree, a trie, a hash map over contexts, a
set over sorted lists, an association-list map, a persistent list with structure sharing, a ring
buffer as a context, an interval tree, a priority queue, a graph with adjacency, a matrix as lists,
a sparse vector, a bitset over integers, a rope over strings, an LRU cache.

### unix
20 Unix tools on the stock runner, each a script with flags and errors to standard error and a
non-zero status: `wc`, `head`, `tail`, `cat -n`, `grep -F` (with `-i -v -c -n`), `sort` (with `-n -r
-u -k`), `uniq -c`, `cut -d -f`, `tr`, `rev`, `nl`, `fold`, `fmt`, `expand`, `comm`, `join`, `tee`,
`seq`, `basename`/`dirname`, `xargs` (with `run-process`). Check each against the real tool on the
same input.

### interpreters
10 languages implemented in grift, each with a test file of programs in that language: a Forth, a
stack VM with an assembler, an infix calculator (precedence, parentheses, variables), a regular
expression matcher (`. * + ? | ()` and classes), a JSON parser and printer, an INI or TOML-subset
reader, a Brainfuck interpreter, a tiny BASIC (LET, PRINT, IF, GOTO), a lambda calculus reducer, a
Prolog-style unifier with backtracking.

### fexprs
20 programs that use `vau!` for what only fexprs do: a `match!` pattern matcher, a `record!` definer
(constructor, fields, predicate), a `state-machine!` DSL, an `assert!` that shows the failing
expression, `trace!` that logs calls, lazy streams with `delay!`/`force`, generators by closures,
`unless!`/`while!`/`for!` loops defined by you, a `with-context!` scope, a `let-values!`, a
`memoise!`, a query language over lists, a test DSL, a units-of-measure arithmetic, a
`define-enum!`, an embedded logic DSL, a `contract!` that checks arguments, a `pipe!` threading
form, a `case!` on symbols, a macro-like `rewrite!` over forms. Say for each whether a plain
function would have done.

### numeric
15 programs: arbitrary-precision integers over lists (add, multiply, divide, print; 100!), rationals
(normalised, arithmetic, printing), complex numbers, matrices (multiply, transpose, determinant,
inverse by Gaussian elimination with floats), primes (Miller-Rabin), statistics (mean, median,
variance, a histogram), Newton's method and bisection, numerical integration (Simpson), a random
number generator (xorshift on checked integers: say how you handle overflow), Fibonacci by matrix
power, continued fractions, base conversion, a Mandelbrot set in text.

### puzzles
12 original puzzles and games played or solved by the program: a sudoku solver (backtracking), a
maze generator and solver, tic-tac-toe with minimax, connect-four with a depth-limited search, the
15-puzzle with A*, a word-ladder search, a nonogram solver for small grids, a cryptarithm solver,
peg solitaire, Towers of Hanoi with a printout, a knight's tour, a Game of Life variant of your own
rules. Original designs; no recreation of a specific commercial game.

### text
20 text programs: a word-frequency report, a Caesar and a Vigenère cipher, run-length encoding,
base64 encode and decode, a URL encoder, roman numerals both ways, number to English words, a
spelling suggester by edit distance, a line wrapper with justification, a CSV-to-table printer, a
diff of two words, a palindrome and anagram finder, a simple template filler, a slugifier, a
Markdown table aligner, a log-line parser with a summary, a soundex, a word-wrap-aware text
centerer, a `printf`-like formatter (`%d %s %5.2f`), a tokenizer for a C-like language.

### everyday
50 short tasks of the kind programmers write every day, 20-80 lines each: FizzBuzz, leap years,
temperature conversion, a grade calculator, a shopping-list total from a file, a to-do list with a
save file, a bank account as a context, a phone book, a timer formatter, a date-difference
calculator (your own calendar code), a password checker, a unit converter, a tip splitter, a
dice-roll statistics run, a loan amortisation table, a binary/hex converter, a word guessing game,
a quiz from a file, a matrix printer, a recipe scaler, an inventory with reorder alerts, a simple
budget report, ... (choose the rest; keep them varied).

### embedding
6 Rust programs embedding grift (each a host crate in your directory with its own `[workspace]`,
like `examples/emacs`): a config loader (grift as the config language, values to Rust structs, good
errors), a rules engine (rules in grift, facts from Rust), a game-scripting host (entities as
contexts, scripts as event handlers), a CLI with grift plugins for subcommands, a REPL with host
functions and an interrupt on Ctrl-C, a calculator service reading requests on stdin. Log each host
function and why it had to be Rust, and every place the `Grift` API was awkward (before/after code
you'd want).

## Time and machine
- Time limit: 2 hours of work. At the limit, stop and write the report with what holds; leave 15
  minutes for the report.
- Keep `~/code/notes/projects/grift/apps4-2026-11-02/<your-name>/status` as one line.
- Your worktree, `CARGO_TARGET_DIR` and `TMPDIR` are in your launch message; use only those. No
  /tmp, no git stash. Builds `-j 4`, tests `--test-threads 4`, through `$S/cargo-slot`. Nothing over
  10 minutes. Kill what you start by pid (never `pkill -f`).
- If you drive a program interactively, use your own fux workspace (named in your launch message;
  `fux help agents` first) and close it at the end. Never touch other panes.

## Git
Commit on your branch in your worktree as you go (plain English messages). Commit only as the
git-config identity (`gsc <stephen.korzen@gmail.com>`); never `-c user.*` or `--author`. No
Co-Authored-By or any AI attribution. **Don't push and don't open a PR.**