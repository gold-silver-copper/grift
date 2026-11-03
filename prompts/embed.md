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
