# Task: ten competing designs for tally

You are the **orchestrator**. Launch ten subagents. Each one researches the
most advanced type system it can justify for an ultra-minimal Lisp, then
**fully implements** that language, including as many machine-checked
proofs as possible in **Haskell, Lean 4, Rocq, Bend 2 and Agda**. Each
implementation lives in its own git worktree under
`/Users/kisaczka/Desktop/code/tally/worktrees/`. When all ten are done,
verify their claims and write a comparison.

Nothing about tally's design is settled. `DESIGN.md` (v0.2) is one
candidate among ten, not the answer.

---

## 0. Context every agent must read first

All paths are under `/Users/kisaczka/Desktop/code/tally/`:

- `DESIGN.md`: the current candidate design. QTT over MLTT with inductive
  families, K, lexicographic structural recursion, rule E-total for
  erasure, reference counting at runtime. §16 records an audit of its
  earlier version, which is a good list of pitfalls.
- `VERIFICATION.md`: the verification strategy (assurance ladder, theorems
  T1–T7, the roles of Haskell, Lean, Rocq, Bend 2 and Agda).
- `reading/README.md`: an annotated index of about 45 papers, theses and
  reference code (Kovács's *elaboration-zoo*), already downloaded in
  `reading/`.
- Background: tally grew out of **grift**
  (`/Users/kisaczka/Desktop/code/grift`, branch `vau-core`), an untyped,
  pure, Kernel-style fexpr Lisp in Rust. Its lessons apply:
  - a minimal core with everything else in a library written in the
    language itself;
  - s-expression syntax;
  - an explicit-loop evaluator with no Rust recursion proportional to
    program depth;
  - performance needs attention early in a library-first design.

---

## 1. Ground rules (orchestrator and all agents)

1. **Git identity.** Every commit is authored as
   `gold_silver_copper <stephen.korzen@gmail.com>`, set explicitly:
   `git -c user.name=gold_silver_copper -c user.email=stephen.korzen@gmail.com commit …`.
   No co-author trailers. Check `git log -1 --format='%an <%ae>'` after
   committing. Never push.
2. **Isolation.** Each agent works only inside its own worktree. It never
   modifies another worktree, the main checkout, or `reading/` (copy papers
   into its own worktree if it needs to annotate them). Shared tool caches
   (`~/.elan`, `~/.opam`, `~/.cabal`, `~/.cargo`) may be used but not
   reconfigured.
3. **Honesty about proofs.**
   - A proof counts only if the proof checker accepts it with the command
     listed in `PROOFS.md`.
   - Every unfinished proof is listed explicitly:
     - Lean: `sorry`, or an `axiom` that isn't in the declared list;
     - Rocq: `Admitted`, `admit`, `Axiom`;
     - Agda: `postulate`, `TERMINATING`, anything not checked with
       `--safe`;
     - Bend 2: `@unsafe`, or a `def f?` (termination check off) in a proof
       file.
   - Haskell property tests are *tests*, not proofs, and are labelled as
     such. (Liquid Haskell proofs count as proofs if checked.)
   - Never weaken a theorem statement to make it provable without saying
     so in `PROOFS.md`.
4. **Statements are reviewed.** Machine-produced proof scripts are fine,
   since the checker validates them. But every definition and theorem
   *statement* must be written so a human can review it against the design
   document, with a comment citing the rule or section it formalizes.
5. **No fake completeness.** If something is not implemented, `REPORT.md`
   says so. Stubs, `todo!()` and "coming soon" do not count as implemented.

---

## 2. Phase 0: setup (orchestrator, before launching agents)

1. **Make tally a git repository** (it is a plain folder now):
   - `git init -b main` in `/Users/kisaczka/Desktop/code/tally`;
   - add `worktrees/` to `.gitignore`;
   - commit the existing files (`DESIGN.md`, `VERIFICATION.md`,
     `README.md`, `PROMPT.md`, `reading/`) with the identity above.
2. **Toolchains.** Check each and install it if missing, recording exact
   versions in `TOOLCHAINS.md` (committed on `main`):

   | Tool | Install | Check |
   |---|---|---|
   | Rust (stable, edition 2024) | already present | `cargo --version` |
   | GHC + cabal (+ QuickCheck; Liquid Haskell optional) | `ghcup` | `ghc --version`, `cabal --version` |
   | Lean 4 + Lake | `elan` (stable toolchain) | `lean --version`, `lake --version` |
   | Rocq (+ Equations; MetaRocq optional) | `opam` | `rocq --version` or `coqc --version` |
   | Agda + standard library | `cabal install Agda` or the platform package | `agda --version` |
   | Bend 2 | per <https://github.com/bendlang/bend> README | `bend --version` |

   - Ask the user before any install that needs `sudo` or takes more than
     about 30 minutes.
   - Mathlib is optional; if used, build it once and share the cache.
   - If a toolchain cannot be installed, say so in `TOOLCHAINS.md`. Agents
     still write those proof files, but report them as *unchecked*, which
     never counts as proven.
3. **Create ten worktrees** from `main`, one per direction in §4:

   ```sh
   git -C /Users/kisaczka/Desktop/code/tally worktree add worktrees/NN-slug -b design/NN-slug
   ```

---

## 3. Phase 1: launch the ten agents

Launch all ten **in a single message** so they run in parallel, using the
Agent tool (or a Workflow, if the user has opted into one). Each agent
receives:
- the **agent brief** of §5, verbatim;
- its row from the table in §4 (direction, slug, worktree path, branch);
- the full list of the other nine directions, so it can stay distinct.

Agents start fresh, so the brief must be self-contained. Do not rely on
them seeing this conversation.

While they run (Phase 2):
- **Don't interfere.** Reply only if an agent asks.
- **Restart failures.** If an agent fails or stops early, resume it with
  SendMessage (keeping its context) and its last commit. Never start a
  replacement from scratch unless its worktree is unusable.
- **Report progress.** Tell the user, in one line per completion, which
  agent finished and its headline result.

---

## 4. The ten directions

Each direction is a **starting point**, not a cage. An agent may move to a
better point in the design space if its research says so, but it must:
- record why in `RESEARCH.md`;
- stay clearly distinct from the other nine.

Every language must remain an **ultra-minimal Lisp**: s-expression syntax,
a small trusted kernel, and everything derivable written in the language's
own library.

| NN | Slug | Direction | Must explore | Start reading |
|---|---|---|---|---|
| 01 | `qtt-mltt` | **Quantitative MLTT**: the current `DESIGN.md` v0.2, refined. | QTT 0/1/ω, inductive families, K, erased case + E-total, lexicographic termination, reference counting with reuse | `DESIGN.md`, Atkey 2018, Brady 2021, Abel–Danielsson–Eriksson 2023/2026, Danielsson 2026 |
| 02 | `affine-data` | **Affine dependent types with a Data/Type split** (BendTT-style). | `Type : Type` and negative datatypes kept consistent because running code copies no functions; erased versus running code; no universes and no positivity check | `reading/bendtt-…pdf`, DESIGN §12 item 14 |
| 03 | `cubical` | **Cubical type theory** in a minimal kernel. | Univalence, higher inductive types, computational function extensionality; choose CCHM or cartesian and justify; an erasure story for running code | HoTT book; Cohen–Coquand–Huber–Mörtberg; cooltt/redtt designs (web) |
| 04 | `observational` | **Observational type theory** with definitional proof irrelevance. | Funext, quotients, a strict-proposition sort, decidable conversion; combine with erasure quantities | Altenkirch–McBride–Swierstra 2007; Pujet–Tabareau 2022, 2025 |
| 05 | `lambda-encodings` | **Datatypes from λ-encodings** (Cedille-style, the Calculus of Dependent Lambda Eliminations). | No primitive inductive types: dependent intersections, implicit products and a Kleene-style equality derive induction. The most minimal possible core. | Stump et al. on Cedille/CDLE (web); Coquand–Huet 1988 |
| 06 | `two-level` | **Two-level type theory / staged compilation**: the typed successor of grift's fexprs. | A typed meta level that runs at compile time over a minimal object level, guaranteed staging, macros as typed meta-programs, object-level code with no runtime abstraction overhead | Kovács, *Staged compilation with two-level type theory* (ICFP 2022, web); elaboration-zoo |
| 07 | `effects` | **Dependent types with algebraic effects and handlers.** | Effect rows or graded effects; typed delimited control (successor of grift's prompts); effects in types without breaking consistency | Plotkin–Pretnar 2013; Leijen 2016/2017; Brady on resource-dependent effects (web) |
| 08 | `sized-guarded` | **Type-based termination and productivity.** | Sized types for recursion; guarded recursion (a "later" modality, clocks) for coinduction and infinite processes; servers and streams as total programs | Abel's work on sized types (web); Bizjak et al. on guarded dependent type theory (web); Abel–Altenkirch 2002 |
| 09 | `refinement` | **Refinement types with automated proof** over a dependent core. | SMT-discharged refinements with a small trusted kernel: either check solver certificates, or list the solver as an explicit trusted oracle and justify it; proof automation as the headline | F\* and Liquid Haskell papers (web); Selsam et al. 2020 |
| 10 | `ownership` | **Ownership and borrowing inside dependent types.** | Linear or graded quantities plus borrows and regions; no garbage collector; in-place mutation proven safe; systems programming in a minimal dependent Lisp | Granule (Orchard et al. 2019); Bernardy et al. 2018; Wadler 1990; RustBelt (web) |

Worktree: `/Users/kisaczka/Desktop/code/tally/worktrees/NN-slug`. Branch:
`design/NN-slug`. Language name: `tally-<slug>`.

---

## 5. Agent brief (give this to each agent verbatim, plus its row from §4)

> You are designing and building **tally-<slug>**, one of ten competing
> candidates for *tally*: an ultra-minimal Lisp with the most advanced type
> system that can be justified. Your direction is in the row below. The
> other nine directions are listed after it. Stay clearly distinct from
> them.
>
> **Where you work.**
> - **Your directory:** `/Users/kisaczka/Desktop/code/tally/worktrees/<NN-slug>`,
>   on branch `design/<NN-slug>`. Work only there.
> - **Read-only context:** `../../DESIGN.md`, `../../VERIFICATION.md` and
>   `../../reading/` (including `reading/README.md` and
>   `reading/elaboration-zoo/`). Do not modify them.
> - **Commits:** commit often, every commit authored exactly as
>   `gold_silver_copper <stephen.korzen@gmail.com>` (set with
>   `git -c user.name=… -c user.email=…`), no co-author trailers. Check with
>   `git log -1 --format='%an <%ae>'`. Never push.
>
> **Priorities.** If time runs short, follow this order and record what was
> skipped:
> 1. a working implementation;
> 2. Lean 4 proofs;
> 3. the Haskell reference checker and tests;
> 4. Agda and Rocq proofs;
> 5. Bend 2 proofs;
> 6. performance work.
>
> ### Step 1 — Research (`RESEARCH.md`)
> - Survey your region of the design space: the reading folder plus web
>   search, with papers cited by title, authors and year.
> - Argue what the *most advanced type system* compatible with an
>   *ultra-minimal Lisp* is, from your direction. Advanced means
>   expressiveness and guarantees; minimal means a small trusted kernel and
>   a small core language.
> - Compare it with `DESIGN.md` v0.2 and with at least two existing systems.
> - State the theorems you will try to prove and why they matter.
> - End with a decision and the main risks.
>
> ### Step 2 — Design (`DESIGN.md` in your worktree)
> Write a complete specification:
> - goals and non-goals;
> - core syntax;
> - full typing rules, written formally;
> - definitional equality and how it is decided;
> - the treatment of recursion and termination;
> - erasure or runtime semantics;
> - the surface syntax (s-expressions);
> - the standard library plan;
> - an explicit list of the trusted computing base;
> - metatheory status: what is proven elsewhere, what you will prove, and
>   what remains conjecture;
> - open questions.
>
> Use `../../DESIGN.md` as a model of the expected depth, and its §16 audit
> as a checklist of pitfalls. Audit your own design once before
> implementing, and record the findings at the end of your `DESIGN.md`.
>
> ### Step 3 — Implementation (Rust)
> A Rust workspace in your worktree, `#![forbid(unsafe_code)]` everywhere,
> containing:
> - **Reader and printer** for your s-expression syntax, with source
>   positions.
> - **Kernel** (trusted): your core calculus. Keep it small and give its
>   line count. No Rust recursion proportional to program depth in the
>   evaluator.
> - **Elaborator** from surface syntax to the kernel: implicit arguments,
>   holes, and pattern matching where your design has them.
> - **Execution**: an interpreter for the erased or compiled program,
>   including whatever runtime your design needs (memory management, effect
>   handlers, staging, …).
> - **REPL and file runner.**
> - **Standard library in the language itself**: at least booleans,
>   naturals, lists, equality and its basic lemmas, plus whatever showcases
>   your direction.
> - **Examples**, in `examples/`, demonstrating the headline features:
>   - length-indexed vectors with append;
>   - a verified sort (sortedness and permutation, or as close as your
>     system allows);
>   - one program that only your type system can express or check well.
>
> **"Fully implemented"** means a fresh clone can run
> `cargo build --release`, run the examples through the REPL or file runner,
> and pass `cargo test`. That test suite includes:
> - the kernel *must-reject* tests (paradoxes, soundness counterexamples
>   specific to your design);
> - *must-accept* tests;
> - library tests.
>
> ### Step 4 — Proofs in five languages (`proofs/` and `PROOFS.md`)
> - **Statements first.** Before proving anything, formalize your core
>   calculus (or a clearly scoped fragment) once per language.
> - **Coverage.** Prove as many of the targets below as you can. Aim to
>   cover the important theorems *across* the languages, rather than
>   repeating the same proof five times, unless repetition is what
>   cross-validates something critical.
> - **Directories:** `proofs/lean/`, `proofs/rocq/`, `proofs/agda/`,
>   `proofs/bend/`, `proofs/haskell/`.
>
> Targets, adapted to your system and with justified substitutions:
> - **T1** subject reduction and progress;
> - **T2** consistency;
> - **T3** decidability of type checking or normalization;
> - **T4** soundness of erasure or compilation;
> - **T5** the safety property specific to your direction (e.g. erased
>   matches, staging, effect safety, productivity, borrow safety);
> - **T6** resource or uniqueness guarantees, if your design has them;
> - **T7** a checker written in the proof language, with a soundness proof,
>   that can be run as an oracle against the Rust kernel.
>
> What each language must contain at minimum:
> - **Lean 4:** the core fragment with a mechanized proof of T1 and of at
>   least one of T2/T4/T5. Ideally also the T7 executable checker.
> - **Rocq:** an independent formalization proving a *different* property
>   from Lean's (e.g. confluence via parallel reduction, or consistency via
>   a model or a translation into Rocq), or a port that cross-checks the
>   Lean result. `Equations` may be used.
> - **Agda** (with `--safe`): e.g. intrinsically typed syntax with
>   normalization by evaluation for a fragment, or a model giving
>   consistency.
> - **Haskell:** a reference type checker (an executable specification)
>   with QuickCheck property tests and mutation testing against the Rust
>   kernel. Liquid Haskell proofs are optional. Label all of it as testing
>   unless it is checked proof.
> - **Bend 2:** proofs (laws) about your standard library algorithms, and,
>   if Bend's type theory can express it, a fragment of your calculus with
>   lemmas about it. Record the Bend version and whether `--verdict` was
>   used.
>
> **`proofs/check.sh`** runs every checker and exits non-zero on failure.
> **`PROOFS.md`** inventories every theorem:
> - its statement, in words and as a reference to the formal statement;
> - its language, file and line;
> - its status: proven, partial (with the listed unfinished markers), or
>   unchecked (toolchain missing);
> - the command that checks it;
> - the count of `sorry`, `Admitted`, `postulate` and unsafe markers per
>   language.
>
> ### Step 5 — Tests and benchmarks
> - `cargo test` passes.
> - Differential testing passes: Haskell reference checker versus Rust
>   kernel on the example and test corpus, plus generated and mutated terms.
> - Port the benchmark programs from grift's
>   `crates/grift/examples/fib_bench.rs` on branch `vau-core` that make
>   sense in your language, and report timings.
>
> ### Step 6 — Report (`REPORT.md`) and final message
> `REPORT.md` contains:
> - a one-paragraph summary;
> - what is implemented and what isn't;
> - kernel size;
> - the proof inventory summary;
> - test and benchmark results;
> - known bugs;
> - risks;
> - an honest self-assessment;
> - the **scorecard** below, with evidence for each score.
>
> Commit everything. Your final message to the orchestrator is the
> scorecard plus five lines of headline results.
>
> **Scorecard** (0–5 each, with evidence):
> 1. Expressiveness: what can be stated and proven.
> 2. Minimality: kernel lines, core constructs, trusted base size.
> 3. Soundness evidence: proofs completed, by theorem and language.
> 4. Implementation completeness: against the "fully implemented"
>    definition above.
> 5. Runtime efficiency: benchmarks, memory behaviour.
> 6. Usability: how pleasant the examples are to write, and error message
>    quality.
> 7. Novelty: what this design offers that the other nine don't.

---

## 6. Phase 3: verification and comparison (orchestrator)

When all agents have finished:

1. **Re-run every claim yourself**, in each worktree:
   - `cargo build --release`;
   - `cargo test`;
   - `proofs/check.sh`;
   - the examples.
   Count the unfinished proof markers independently with `grep`, and
   correct any scorecard that doesn't match.
2. **Write `COMPARISON.md`** on `main`:
   - the ten scorecards, with your corrections marked;
   - a theorem-by-language matrix (T1–T7 against Lean, Rocq, Agda, Bend,
     Haskell) for each design;
   - kernel sizes and benchmark tables side by side;
   - what each design does uniquely well, and its fatal flaws if any;
   - which ideas could be combined (e.g. one design's erasure with another's
     termination);
   - a recommendation for tally, with the reasoning.
3. Commit `COMPARISON.md` and `TOOLCHAINS.md` on `main` with the required
   identity. Leave the ten branches and worktrees in place for the user to
   inspect.
4. **Final message to the user:**
   - one table, with one row per design and the scorecard totals;
   - the recommendation;
   - the path to `COMPARISON.md`;
   - anything that needs the user's decision (missing toolchains, failed
     agents, disputed claims).
