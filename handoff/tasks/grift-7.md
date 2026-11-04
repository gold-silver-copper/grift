---
id: 01a1a561-c955-7243-b5e2-9a6452672a03
slug: tasks/grift-7
title: "grift apps, wave 1: write a real program in grift and log what grift lacks"
type: task
status: completed
priority: low
tags: [history, prompt]
in:
  tasks/grift-1: "a0VVVVV"
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# grift apps, wave 1: write a real program in grift and log what grift lacks

History, part of [tasks/grift-1](grift-1.md). Outcome: an evidence run, no PR; its reports are the evidence later PRs cite. Run notes: box2 `~/code/notes/projects/grift/apps-2026-11-01/`.

## Brief (`prompts/apps.md`, box2 only, never committed; verbatim)


You are one of twelve agents. Four applications are each written three ways, one agent per
version, each in its own git worktree and fux tab. The point is **evidence of what grift needs**:
the program matters, and the friction log matters as much. Never ask the user anything: decide,
write down why, continue.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Read first (15 minutes, not more)
- `README.md`, `docs/LANGUAGE.md`, `DECISIONS.md` (G-1..G-25: the owner's and earlier decisions;
  don't reopen them, but log evidence for or against one), `crates/grift/src/library.grift`,
  `crates/grift-cli/src/main.rs`.
- How an earlier round went: `~/code/notes/projects/grift/emacs-2026-11-01/summary.md` and one
  `friction.md` there, for the shape of a good entry.
- grift in brief: a Kernel-style Lisp; `$vau` is the only special primitive; `$if`, `$define!`,
  `$lambda`, `$let`, `$cond` are library code. Values: integers (checked `i64`), symbols, pairs,
  immutable opaque strings (no string operations), environments, combiners. No vectors, hash
  tables, characters or floats (cut on purpose, G-9). Errors go to the host with a code, the values
  and the location; grift can't catch them, only run cleanup with `$unwind-protect` (G-4, G-18).
  No continuations.

## Rules for every version
- **Your directory:** `apps/<your-name>/` (given in your launch message). grift code in `.grift`
  files there. Run it with the stock runner if you can:
  `cargo run --release -p grift-cli -- apps/<your-name>/main.grift` (it registers only `display`
  and `newline`). If your app needs more from the outside world (reading stdin as data, files,
  time), make a small host crate in your directory with its own `[workspace]` (like
  `examples/emacs`), and log each host function you add with why it had to be Rust.
- **No changes to `crates/`.** The findings must be comparable across twelve agents. When grift
  blocks you, log it with a reproduction, work around it in your program, and say what the
  workaround cost. A blocker you can't get around is a finding too: stop that part and say so.
- **Your done list** is in your app's section below: a closed list. Each item is checked by running
  the program (quote the output) or by a grift test file your directory runs.
- **Keep it a real program:** structured as a grift programmer would write it, comments where a
  reader needs them, not a pile of workarounds.

## The friction log (the main output besides the program)
`~/code/notes/projects/grift/apps-2026-11-01/<your-name>/friction.md`. One entry per problem, as
you hit it, not reconstructed at the end:
- **tag** (one of: `strings`, `data-structures`, `control-flow`, `errors`, `speed`, `host-api`,
  `library`, `reader-printer`, `debugging`, `other`), **what you tried**, **what happened** (the
  exact error or behaviour), **smallest reproduction** (a grift snippet and the command that runs
  it), **severity** (`blocker`, `costly`: a workaround of more than a few lines or a real slowdown,
  `annoyance`), **what you did instead** and what it cost (lines, time).
- Good news briefly too: what fexprs, first-class environments or redefinition made easy.

## Questions every report answers, with numbers
`~/code/notes/projects/grift/apps-2026-11-01/<your-name>/report.md`:
1. **The done list:** each item works (quote the output), partly, or not.
2. **Lines:** grift code lines (no blanks or comments), and how many of them are general helpers
   the library lacks (list them); Rust host lines if any, and each host function with why.
3. **Control flow, the open question:** for your program, would each of these have saved code or
   made it clearer, and roughly how many lines: (a) catching an error inside grift, (b) a one-shot
   escape (early exit from a deep computation), (c) delimited continuations, (d) full re-entrant
   `call/cc`? Point to the code each would replace. "None" is a fine answer.
4. **Data:** would vectors, hash tables, characters, or string operations have helped, and which
   operations exactly? Give the smallest set you'd want, with the code it would replace.
5. **Speed:** one timing of your hot path (release build, one run, the command and the number), and
   whether it was acceptable. If not, what was slow (`perf` or a smaller timing to show it), but no
   benchmark campaign.
6. **Top five friction entries,** most costly first, each linking its entry.
7. **Proposals** for grift's core or library, each with its evidence (not done).
Then write `DONE` in the same folder: the head SHA and one line of status.

## The applications

### eval: a Kernel evaluator written in grift
A metacircular evaluator: grift code that evaluates a guest language which is itself a Kernel
subset. Guest programs are grift data (quoted lists in your files). Done:
1. Guest values and primitives: integers, symbols, pairs, `()`, booleans, `#inert`, `#ignore`,
   environments, operatives, applicatives; `$vau`, `wrap`, `unwrap`, `eval`, `make-environment`,
   `$define!`, `$if`, `cons`, `car`, `cdr`, `eq?`, `+`, `-`, `*`, `<?`.
2. A guest prelude written in the guest language with `$vau`: `$lambda`, `list`, `$sequence`,
   `$cond`, `$let`, `map`; guest `fib 20` and a guest `map` over a 1,000-element list run.
3. Guest errors (unbound symbol, wrong type, not a combiner) come out as guest errors with the
   guest's values, and the evaluator itself never fails with a host-grift error on a guest mistake.
4. A guest program 10,000 calls deep in tail position runs in bounded host memory.
5. Timing: guest `fib 20` against the same in grift directly.
Versions:
- **eval-direct:** a straightforward recursive evaluator.
- **eval-callcc:** the guest also has re-entrant `call/cc` (continuation-passing style in the host
  grift); show early exit, a generator, and re-entering a continuation twice.
- **eval-stepper:** a tracing, stepping evaluator: print each combination with its depth, a
  breakpoint on a named guest function, a trace limited to a depth, and a replay of the last N steps
  after a guest error.

### logic: a logic programming engine
Facts, rules, unification with logic variables, backtracking, all solutions. Done:
1. Facts and rules in a small DSL, queries with variables, every solution listed.
2. A family-tree database with recursive `ancestor`; `append/3` run backwards (all splits of a
   list); `member`.
3. N-queens: 6 queens gives 4 solutions, 8 queens gives 92 (time it).
4. Negation as failure (`not`), and a way to stop after the first solution.
5. A useful error for a malformed rule or query (what, where), not a crash in the engine.
Versions:
- **logic-closures:** success and failure continuations as closures.
- **logic-stack:** an explicit choice-point stack (no closures for control).
- **logic-streams:** solutions as lazy streams (promises built from closures).

### algebra: symbolic differentiation and simplification
Expressions as lists. Done:
1. Differentiate `+ - * /`, integer powers, `sin cos exp ln`, with the chain rule.
2. Simplify to a canonical form: constants folded, `0` and `1` identities, like terms collected;
   simplifying twice gives the same result as once (checked on 30 expressions).
3. Print in infix with the fewest parentheses that keep the meaning (`x^2*sin(x) + 3`).
4. Check derivatives numerically: compare against a finite difference at a few points using exact
   rationals (pairs of integers), no floats.
5. Overflow: say what happens on large coefficients (grift integers are checked `i64`): find where
   it overflows and what a user sees.
Versions:
- **algebra-rules:** a rewrite-rule DSL (pattern variables, conditions) built with fexprs; the
  derivative and simplifier are rule sets.
- **algebra-plain:** plain recursive functions over the expression shape.
- **algebra-exact:** a polynomial and rational-function normal form (sorted term lists, exact
  rational coefficients) as the canonical form.

### json: a jq-like JSON filter (a Unix tool)
`<binary> FILTER < input.json` prints the result as indented JSON; errors on stderr; exit 0, 1 on a
data error, 2 on a usage error; a closed stdout ends it quietly. This needs a host crate (grift can't
read stdin as data otherwise). Done:
1. Parse all of JSON: objects, arrays, strings with every escape including `\uXXXX` and surrogate
   pairs, numbers (no floats in grift: decide how to keep `1.5e3` exact and say why), `true`,
   `false`, `null`.
2. Filters: `.`, `.key`, `.[n]`, `.[]`, `|`, `keys`, `length`, `select(.key == value)`, `map(f)`.
3. A malformed input names the line and column; a bad filter names the position in the filter.
4. Round trip: printing the parse of a 1 MB file and parsing that again gives the same value; time
   parsing and printing it.
5. Unicode survives (`"日本語"`, emoji via surrogate pairs) byte-exact.
Versions:
- **json-codes:** the host hands grift input as a list of code points and takes output as code
  points; nothing else about strings.
- **json-combinators:** the same host interface; the parser is built from parser combinators.
- **json-strprims:** the host adds the smallest set of string primitives you judge necessary
  (decide which, and justify each against the other two versions' approach).

## Time and machine
- Time limit: 60 minutes of work. At the limit, stop and write the report with what holds.
- Keep `~/code/notes/projects/grift/apps-2026-11-01/<your-name>/status` as one line.
- Your worktree, `CARGO_TARGET_DIR` and `TMPDIR` are given in your launch message; use only those.
  No /tmp, no git stash. Builds `-j 4`, tests `--test-threads 4`, through `$S/cargo-slot`. Nothing
  over 10 minutes. Kill what you start by pid (never `pkill -f`).
- If you drive a program interactively, use your own fux workspace (named in your launch message;
  `fux help agents` first) and close it at the end. Never touch other panes.

## Git
Commit on your branch in your worktree as you go (plain English messages: what the program now
does). Commit only as the git-config identity (`gsc <stephen.korzen@gmail.com>`); never
`-c user.*` or `--author`. No Co-Authored-By or any AI attribution. **Don't push and don't open a
PR.**