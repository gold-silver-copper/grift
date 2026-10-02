# Task: ten competing pure, embeddable fexpr Lisps

You are the **orchestrator**. Launch ten subagents. Each one researches,
designs and **fully implements** a different Lisp that is:

- **fexpr-based:** `$vau` operatives and first-class environments are the
  core abstraction, in the family of John Shutt's vau calculus and Kernel;
- **fully dynamic:** no static type system rejects programs;
- **purely functional:** no observable mutation anywhere in the language;
  effects reach the outside world only through an explicit host interface;
- **interpreted, never compiled:** no bytecode, no JIT, no closure
  compilation, no ahead-of-time translation. The evaluator interprets the
  data that programs are made of, which is what fexprs require anyway;
- **embeddable in Rust programs:** a small, dependency-free,
  `#![no_std]` + `alloc` library crate with a clean host API, resource
  limits and sandboxing;
- **ultra-minimal yet ultra-powerful:** very few Rust primitives, with
  everything else written in the language's own library, and abstraction
  power beyond conventional Lisps.

Each agent also writes as many machine-checked proofs about its language as
possible in **Haskell, Lean 4, Rocq, Bend 2 and Agda**. Each language lives
in its own git worktree under `/Users/kisaczka/Desktop/code/grift/worktrees/`.
When all ten are done, verify their claims and write a comparison.

---

## 0. Context every agent must read first

All paths are under `/Users/kisaczka/Desktop/code/grift/`, branch
`vau-core`:

- **grift itself**, the reference point, which already meets most of these
  constraints:
  - `#![no_std]` + `alloc`, no dependencies;
  - 44 primitives, with `$vau` the only primitive operative;
  - `$if`, `$define!`, `$lambda`, `$let`, `$cond` and the rest are library
    code in grift;
  - immutable environments, with definitions as declarations;
  - multi-prompt delimited control, and errors as an effect;
  - contracts with blame;
  - an explicit-loop machine over heap frames, and mark-and-sweep GC.

  Read:
  - `README.md`;
  - `docs/architecture/LANGUAGE_SPECIFICATION.md`, including §11, deviations
    from Kernel;
  - `docs/architecture/ARCHITECTURE.md`, including its performance numbers;
  - `crates/grift/src/` (about 3,200 lines);
  - `crates/grift/lib/*.grift` (about 1,400 lines);
  - `crates/grift/tests/`;
  - `crates/grift/examples/fib_bench.rs`.
- `05-07.pdf`: the Revised⁻¹ Report on the Kernel Programming Language
  (R-1RK).
- `jshutt.pdf`: Shutt's PhD thesis, *Fexprs as the basis of Lisp function
  application; or $vau: the ultimate abstraction*, with the vau-calculi and
  their confluence results.
- `prompt.md`: the specification grift was built from.
- `reading/`: shared papers downloaded in Phase 0 (§2).

**Lessons from grift that apply to every design:**
- **Speed.** Everything-in-the-library is elegant but slow: `(+ 1 2)` takes
  about 50 machine steps, and the benchmark suite runs 2.6× slower than the
  previous interpreter, which had native special forms. Without compilation,
  speed must come from interpreter design: environment representation,
  lookup caches, frame layout, cheap primitive dispatch.
- **Definitions.** Immutable environments make forward references and
  mutual recursion awkward. grift needs a tree-shaped `$define!` for mutual
  recursion.
- **Quoting.** No `$quote` makes symbol data clumsy.
- **The `$` sigil rule** caught real bugs.
- **No Rust recursion** may grow with program depth.

---

## 1. Ground rules (orchestrator and all agents)

1. **Git identity.** Every commit is authored as
   `gold_silver_copper <stephen.korzen@gmail.com>`, set explicitly:
   `git -c user.name=gold_silver_copper -c user.email=stephen.korzen@gmail.com commit …`.
   No co-author trailers. Check `git log -1 --format='%an <%ae>'` after
   committing. Never push.
2. **Isolation.** Each agent works only inside its own worktree. It never
   modifies another worktree, the main grift checkout, or `reading/`.
   Shared tool caches (`~/.elan`, `~/.opam`, `~/.cabal`, `~/.cargo`) may be
   used but not reconfigured.
3. **Honesty about proofs.**
   - A proof counts only if its checker accepts it with the command listed
     in `PROOFS.md`.
   - Every unfinished proof is listed explicitly:
     - Lean: `sorry`, or an `axiom` that isn't declared;
     - Rocq: `Admitted`, `admit`, `Axiom`;
     - Agda: `postulate`, `TERMINATING`, anything not checked with
       `--safe`;
     - Bend 2: `@unsafe`, or a `def f?` in a proof file.
   - Haskell property tests are *tests*, not proofs, and are labelled as
     such. (Liquid Haskell proofs count if checked.)
   - Never weaken a theorem statement to make it provable without saying
     so.
4. **Statements are reviewed.** Machine-produced proof scripts are fine,
   since the checker validates them. Every definition and theorem
   *statement* must be reviewable by a human against the language
   specification, with a comment citing what it formalizes.
5. **No fake completeness.** Unimplemented features are reported as such.
   Stubs and `todo!()` do not count.
6. **The hard constraints of the task:**
   - purity;
   - interpretation only;
   - the embedding requirements of §5;
   - everything else "non-negotiable" in §5.

   An agent that finds its direction impossible under them reports that,
   rather than relaxing them.

---

## 2. Phase 0: setup (orchestrator, before launching agents)

1. **Repository.**
   - Confirm `/Users/kisaczka/Desktop/code/grift` is clean on `vau-core`.
   - Add `worktrees/` and `reading/` to `.git/info/exclude` (local, not
     committed), so grift's history is untouched.
2. **Shared reading.** Download into `reading/` and verify each PDF: check
   the first page's text, or render it if the text doesn't extract.
   - Never get around bot protection. If a paper has no free copy, list it
     with a link instead.
   - Write `reading/README.md` with a one-line note per file on what to read
     it for.
   - Copy in `05-07.pdf` and `jshutt.pdf`.
   - Start with:
     - **Fexprs and reflection:**
       - Wand, *The Theory of Fexprs is Trivial* (1998);
       - Pitman, *Special Forms in Lisp* (1980);
       - Smith, *Reflection and Semantics in Lisp* (1984);
       - Friedman & Wand, *Reification: Reflection without Metaphysics*
         (1984);
       - Wand & Friedman, *The Mystery of the Tower Revealed* (1988).
     - **Intensional computation:** Jay & Given-Wilson, *A Combinatory
       Account of Internal Structure* (2011); Jay, pattern-calculus
       papers.
     - **Capabilities:** Miller, *Robust Composition* (PhD thesis, 2006).
     - **Control and effects:**
       - Dybvig, Peyton Jones & Sabry, *A Monadic Framework for Delimited
         Continuations* (2007);
       - Kiselyov, Shan & Sabry, *Delimited Dynamic Binding* (2006);
       - Plotkin & Pretnar, *Handling Algebraic Effects* (2013).
     - **Laziness:** Launchbury, *A Natural Semantics for Lazy Evaluation*
       (1993); Sestoft, *Deriving a Lazy Abstract Machine* (1997).
     - **Data structures:** Okasaki, *Purely Functional Data Structures*
       (PhD thesis, 1996); Bagwell, *Ideal Hash Trees* (2001); Hickey, *A
       History of Clojure* (HOPL 2020).
     - **Logic programming:** Hemann & Friedman, *μKanren* (2013); Byrd,
       *Relational Programming in miniKanren* (PhD thesis, 2009).
     - **Totality:** Turner, *Total Functional Programming* (2004).
     - **Semantics and memory:**
       - Danielsson, *Operational Semantics Using the Partiality Monad*
         (2012);
       - Felleisen & Hieb, *The Revised Report on the Syntactic Theories of
         Sequential Control and State* (1992);
       - Morrisett, Felleisen & Harper, *Abstract Models of Memory
         Management* (1995).
3. **Toolchains.** Check each and install it if missing, recording exact
   versions in `worktrees/TOOLCHAINS.md`:

   | Tool | Install | Check |
   |---|---|---|
   | Rust (stable, edition 2024) plus the `thumbv6m-none-eabi` target | present / `rustup target add` | `cargo --version` |
   | GHC + cabal (+ QuickCheck; Liquid Haskell optional) | `ghcup` | `ghc --version` |
   | Lean 4 + Lake | `elan` | `lean --version` |
   | Rocq (+ Equations) | `opam` | `rocq --version` or `coqc --version` |
   | Agda + standard library | `cabal install Agda` or a package | `agda --version` |
   | Bend 2 | per <https://github.com/bendlang/bend> README | `bend --version` |

   - Ask the user before any install that needs `sudo` or takes more than
     about 30 minutes.
   - A toolchain that cannot be installed is recorded as such. Agents still
     write those proofs, but report them as *unchecked*, never as proven.
4. **Baseline.** Run
   `cargo run -p grift --example fib_bench --release` on `vau-core` and save
   the output to `worktrees/BASELINE.md`. Every agent measures against the
   same machine and the same numbers.
5. **Worktrees.** Create ten, one per direction in §4:

   ```sh
   git -C /Users/kisaczka/Desktop/code/grift worktree add worktrees/NN-slug -b fexpr/NN-slug vau-core
   ```

   Each starts as a copy of grift. An agent may build on grift's code
   (reader, heap, GC, machine, test harness) or replace it entirely. Its
   final tree contains only its own language, and it says in `REPORT.md`
   what it reused.

---

## 3. Phase 1: launch the ten agents

Launch all ten **in a single message** so they run in parallel, using the
Agent tool (or a Workflow, if the user has opted into one). Each agent
receives:
- the **agent brief** of §5, verbatim;
- its row from §4;
- the list of the other nine directions.

Agents start fresh, so the brief must be self-contained.

While they run (Phase 2):
- **Don't interfere.** Reply only if an agent asks.
- **Restart failures.** If an agent fails or stops early, resume it with
  SendMessage (keeping its context) from its last commit.
- **Report progress.** Tell the user, in one line per completion, which
  agent finished and its headline result.

---

## 4. The ten directions

All ten are pure, interpreted, embeddable fexpr Lisps. They differ in
**where their power comes from**. Each direction is a starting point: an
agent may move within it if its research says so, but it must:
- record why in `RESEARCH.md`;
- stay clearly distinct from the other nine.

| NN | Slug | Source of power | Must explore | Start reading |
|---|---|---|---|---|
| 01 | `pure-vau` | **grift, perfected**: the reference design with its pain points fixed. | Recursive definition groups without mutation; symbol literals; error locations and backtraces; interpreter-only speed work (lookup caching, frame layout, cheap primitive dispatch) to close the 2.6× gap; the most polished embedding API | grift's docs; `jshutt.pdf` |
| 02 | `vau-calculus` | **The semantics is a calculus**: the language *is* an explicit-evaluation vau-calculus. | Shutt's vau-calculi as the normative definition; a non-trivial equational theory despite Wand's result; every interpreter shortcut justified by a proven equation; programs that reason about programs equationally | `jshutt.pdf` (calculus chapters); Wand 1998 |
| 03 | `effects` | **All effects are algebraic effects**, and the Rust host is the outermost handler. | Effect handlers derived from multi-prompt delimited control, as the *only* way to do I/O, state simulation, generators, async and errors; the host handles unhandled effects; execution can be suspended and resumed across host calls | Plotkin–Pretnar 2013; Dybvig et al. 2007; Kiselyov et al. 2006 |
| 04 | `capabilities` | **Environments as capabilities**: safe scripting of untrusted code. | No ambient authority; the host grants exactly the environments a script may use; attenuation, membranes and revocation built purely from operatives; confinement of untrusted operatives; resource limits as part of the security story | Miller 2006; R-1RK on `$let-safe`, `$remote-eval` |
| 05 | `intensional` | **Code inspection as a first-class, safe operation**: a factorisation or pattern core even smaller than `$vau`. | An SF-calculus or pattern-calculus core from which `$vau`, quoting and environments are *derived*; operatives that decompose code and data uniformly; generic programming over structure | Jay & Given-Wilson 2011; Jay's pattern calculus |
| 06 | `persistent` | **A rich pure data model**: persistent collections and value semantics at the core. | Persistent vectors, hash maps and sets (HAMT-style) with structural sharing as built-in values; structural equality and hashing; a lossless data mapping to and from Rust types (a serde-like bridge) for embedding; operatives over data-driven DSLs | Okasaki 1996; Bagwell 2001; Hickey 2020 |
| 07 | `lazy` | **Call-by-need fexprs**: laziness and operatives together. | Non-strict evaluation with sharing (thunk update as an unobservable internal write); infinite structures; how operatives and laziness interact (do you still need operatives for control forms?); space-leak control; a lazy machine with proper tail behaviour | Launchbury 1993; Sestoft 1997; Shutt on evaluation order |
| 08 | `reflective` | **A first-class, customizable evaluator**: a pure reflective tower without compilation. | Programs obtain and extend their own `eval`/`apply`; per-environment evaluation strategies; reify and reflect between levels, all pure; tower semantics with bounded cost per level | Smith 1984; Friedman–Wand 1984; Wand–Friedman 1988 |
| 09 | `relational` | **Logic programming as a library of operatives**: pure relations, unification and search. | Logic variables, unification and interleaving search (μKanren-style) built from a minimal pure core; relational interpreters and program synthesis written in the language; fexprs as the syntax layer for relations | Hemann–Friedman 2013; Byrd 2009 |
| 10 | `total` | **Every program terminates**: a total fexpr Lisp for safe configuration and scripting. | A dynamic, fexpr-based language in which evaluation always terminates *without* fuel. Rule out unbounded recursion through `eval`, self-application and operatives, e.g. by stratified evaluation, structural recursion only, or termination by construction, while keeping operatives useful. Prove termination. | Turner 2004; Dhall's design (web); Wand 1998 |

Worktree: `/Users/kisaczka/Desktop/code/grift/worktrees/NN-slug`. Branch:
`fexpr/NN-slug`. Language name: `grift-<slug>`.

---

## 5. Agent brief (give this to each agent verbatim, plus its row from §4)

> You are designing and building **grift-<slug>**, one of ten competing
> **pure, interpreted, embeddable, fexpr-based Lisps** in the family of
> Shutt's vau calculus and Kernel. Your direction is in the row below, and
> the other nine directions are listed after it. Stay clearly distinct from
> them.
>
> **Where you work.**
> - **Your directory:** `/Users/kisaczka/Desktop/code/grift/worktrees/<NN-slug>`,
>   on branch `fexpr/<NN-slug>`. It starts as a copy of grift `vau-core`.
>   Reuse grift's code or replace it; your final tree contains only your
>   language.
> - **Read-only context:** `../../README.md`, `../../docs/architecture/`,
>   `../../05-07.pdf` (R-1RK), `../../jshutt.pdf` (Shutt's thesis),
>   `../../reading/` (see its `README.md`) and `../BASELINE.md` (the
>   benchmark baseline). Do not modify them.
> - **Commits:** commit often, every commit authored exactly as
>   `gold_silver_copper <stephen.korzen@gmail.com>` (set with
>   `git -c user.name=… -c user.email=…`), no co-author trailers. Check with
>   `git log -1 --format='%an <%ae>'`. Never push.
>
> **Non-negotiable properties.**
> - **Fexpr-based:** `$vau` operatives and first-class environments (or, in
>   direction 05, a core from which they are derived) are the central
>   abstraction.
> - **Fully dynamic:** no static type system rejects programs. Runtime
>   checks, contracts and optional analyses are fine.
> - **Purely functional.**
>   - No observable mutation: no `set!`, no mutable pairs, no mutable cells,
>     no mutable environments.
>   - Internal writes the program cannot observe are allowed: caches,
>     thunk updates, initialization of recursive bindings.
>   - Every effect on the outside world goes through the explicit host
>     interface.
> - **Interpreted only.** No bytecode, no JIT, no closure compilation, no
>   ahead-of-time translation, no pre-pass that turns source into another
>   executable representation. Interpreter-internal caches are allowed if
>   their effect is unobservable.
> - **Embeddable**, as a library crate:
>   - `#![no_std]` + `alloc`, `#![forbid(unsafe_code)]`, zero dependencies
>     (a `std` feature and a REPL binary may add some). It builds for
>     `thumbv6m-none-eabi`.
>   - **Host API:** create an interpreter; evaluate source; register host
>     functions; convert between Rust values and language values with
>     traits; get errors as `Result` with source positions.
>   - **Resource limits:** a **fuel** (step) budget and a **memory** budget,
>     both settable per evaluation. Running out stops cleanly with a
>     distinguishable error, and leaves the interpreter usable (ideally
>     resumable).
>   - **Deterministic:** the same program, inputs and host responses always
>     give the same result.
>   - **Sandboxing:** the host decides exactly which bindings a script can
>     reach.
>   - **Cheap clones:** cloning or snapshotting a booted interpreter is much
>     cheaper than booting one.
>   - An `examples/embed.rs` showing a real embedding: a Rust program using
>     your language for configuration or scripting, with host functions and
>     limits.
> - **Ultra-minimal:** few Rust primitives (report the count and justify
>   each), with everything else written in your language's own library.
>   Report the size of the evaluator and core.
> - **Robust:** no Rust recursion proportional to program depth anywhere;
>   proper tail calls (or, in direction 07, the lazy equivalent); automatic
>   memory management with bounded heap under sustained allocation.
>
> **Priorities.** If time runs short, follow this order and record what was
> skipped:
> 1. a working implementation with the embedding API;
> 2. Lean 4 proofs;
> 3. the Haskell reference interpreter and differential tests;
> 4. Agda and Rocq proofs;
> 5. Bend 2 proofs;
> 6. interpreter performance work (except in direction 01, where it is part
>    of the direction).
>
> ### Step 1 — Research (`RESEARCH.md`)
> - Survey your region of the design space: `../../reading/`, Shutt's
>   thesis, and web search, with papers cited by title, authors and year.
> - Argue what the *most powerful pure, minimal, embeddable fexpr Lisp* is
>   from your direction: what it can express that others can't, and at what
>   cost in primitives, interpreter speed and predictability.
> - Address Wand's result (fexprs trivialize the equational theory) and how
>   your design relates to Shutt's answer to it.
> - Compare with grift `vau-core`, with Kernel, and with at least one
>   non-fexpr embeddable language (e.g. Lua, Rhai, Janet, Steel, Dhall).
> - State the theorems you will try to prove.
> - End with a decision and the main risks.
>
> ### Step 2 — Specification (`SPEC.md`)
> A complete, normative specification:
> - lexical syntax;
> - values;
> - **formal operational semantics** (small-step reduction or an abstract
>   machine, written out);
> - environments;
> - operatives and applicatives;
> - control;
> - errors;
> - the host interface and its semantics (host functions, effects, fuel,
>   memory limits);
> - the primitives, each with its reason for being primitive;
> - the library plan;
> - a table of deviations from R-1RK with rationale;
> - open questions.
>
> Use `../../docs/architecture/LANGUAGE_SPECIFICATION.md` as a model of the
> expected depth. Audit your own spec once before implementing, and record
> the findings at the end of `SPEC.md`.
>
> ### Step 3 — Implementation (Rust)
> - **Library crate** with the embedding API above, plus a REPL and file
>   runner behind a feature.
> - **Reader and printer**, with source positions in errors.
> - **Evaluator** implementing `SPEC.md` exactly.
> - **Memory management** with GC stress tests.
> - **Fuel and memory limits.**
> - **Standard library in the language itself**: lists, numbers, strings,
>   binding and control forms derived from `$vau`, plus whatever showcases
>   your direction.
> - **Examples** in `examples/`:
>   - `embed.rs`;
>   - user-defined control forms written as operatives (`$while`-style
>     iteration expressed purely, `$unless`, a custom `$let` variant);
>   - an interpreter for a small language, written in your language;
>   - one program that showcases what *only your design* does well.
>
> **"Fully implemented"** means a fresh clone can:
> - run `cargo build --release`;
> - run `cargo build --target thumbv6m-none-eabi` (library only);
> - run all examples;
> - pass `cargo test`.
>
> That test suite includes:
> - language-level test files with assertion helpers written in your
>   language (as grift's `tests/grift/` does);
> - 1,000,000-iteration loops through library control forms (or the
>   equivalent proof of constant space for direction 07; direction 10 shows
>   long-running loops still terminate);
> - GC stress tests with a bounded heap;
> - fuel and memory-limit tests;
> - embedding-API tests;
> - error-case tests.
>
> ### Step 4 — Proofs in five languages (`proofs/` and `PROOFS.md`)
> - **Statements first.** Before proving anything, formalize your core
>   semantics (or a clearly scoped core fragment) once per language: syntax,
>   values, environments, the operative/applicative distinction, and the
>   reduction relation or abstract machine.
> - **Coverage.** Prove as many targets as you can. Spread the important
>   theorems *across* languages, rather than proving the same thing five
>   times, unless repetition cross-validates something critical.
> - **Directories:** `proofs/lean/`, `proofs/rocq/`, `proofs/agda/`,
>   `proofs/bend/`, `proofs/haskell/`.
>
> Targets, adapted to your design with justified substitutions:
> - **F1 Purity and determinism.** Evaluation is a deterministic function of
>   the program, the environment and the host's responses. Nothing
>   observable is mutated. Big-step and small-step semantics agree.
> - **F2 Equational theory.** Confluence of your calculus (e.g. via parallel
>   reduction), and a set of *non-trivial* contextual equivalences that hold
>   despite fexprs (e.g. β for applicatives applied to values, or the
>   equations your interpreter shortcuts rely on).
> - **F3 Interpreter correctness.** The abstract machine your Rust code
>   implements simulates the reduction semantics.
> - **F4 Space safety.** The machine's continuation does not grow in tail
>   loops (or, in direction 07, the lazy equivalent).
> - **F5 GC safety.** Removing unreachable heap objects does not change
>   observable behaviour.
> - **F6 Your direction's headline property.** For example:
>   - (01) correctness of the lookup cache;
>   - (02) soundness of the equational shortcuts;
>   - (03) effect safety, and agreement between handlers and host;
>   - (04) capability safety, confinement and no ambient authority;
>   - (05) derivability of `$vau` from the factorisation core, and safety of
>     inspection;
>   - (06) persistent-structure invariants, and lossless Rust round-trips;
>   - (07) adequacy of the lazy machine with respect to the natural
>     semantics;
>   - (08) tower soundness;
>   - (09) soundness and completeness of interleaving search;
>   - (10) termination of every program.
> - **F7 Executable reference.** An interpreter written in a proof language,
>   proven to agree with the formal semantics, and usable as an oracle
>   against your Rust implementation.
> - **F8 Fuel soundness.** Running with fuel *n* either finishes with exactly
>   the result of the unbounded run, or stops with an out-of-fuel error;
>   never a different result. Resuming continues correctly, if supported.
>
> What each language must contain at minimum:
> - **Lean 4:** the core semantics plus mechanized proofs of F1 and F8, and
>   of at least one of F3/F4/F6. Ideally also the F7 executable reference
>   interpreter.
> - **Rocq:** a property *different* from Lean's: e.g. F2 confluence via
>   parallel reduction, or contextual equivalences via bisimulation or
>   logical relations, or F5. `Equations` may be used.
> - **Agda** (with `--safe`): e.g. a definitional interpreter in the
>   partiality or delay monad (Danielsson 2012) with determinism or
>   big-step/small-step agreement, or intrinsically scoped syntax for the
>   core.
> - **Haskell:** a reference interpreter (an executable specification) with
>   QuickCheck property tests and differential testing against the Rust
>   implementation on generated programs, including GC, fuel and memory-limit
>   stress. Liquid Haskell proofs are optional. Label all of it as testing
>   unless it is checked proof.
> - **Bend 2:** proofs (laws) about your standard library algorithms, and,
>   if Bend's type theory can express it, a fragment of your semantics with
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
> - the count of unfinished-proof markers per language.
>
> ### Step 5 — Tests and benchmarks
> - `cargo test` passes.
> - Haskell-versus-Rust differential testing passes on the test corpus plus
>   generated programs.
> - Port grift's `crates/grift/examples/fib_bench.rs` (34 programs) to your
>   language. Report each timing next to grift's in `../BASELINE.md`, with
>   the ratio, and say whether the machine was under other load.
> - Report three more numbers:
>   - the time to boot an interpreter;
>   - the time to clone a booted one;
>   - the memory footprint of a booted interpreter.
>
> ### Step 6 — Report (`REPORT.md`) and final message
> `REPORT.md` contains:
> - a one-paragraph summary;
> - what is implemented and what isn't;
> - what was reused from grift;
> - the primitive count and evaluator size;
> - the embedding API summary;
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
> 1. Abstraction power: what user code can define that other Lisps
>    can't.
> 2. Minimality: primitive count, evaluator size, trusted Rust code.
> 3. Purity: how complete the guarantee is, and what is proven about it.
> 4. Embeddability: API quality, `no_std`, limits, sandboxing, determinism,
>    boot and clone cost.
> 5. Semantic clarity and proof evidence: formal semantics, theorems
>    completed by language.
> 6. Implementation completeness: against the "fully implemented"
>    definition above.
> 7. Performance: benchmark ratio against grift's baseline, interpreter
>    only.
> 8. Usability and debuggability: error messages, REPL experience,
>    predictability of operatives.
> 9. Novelty: what this design offers that the other nine don't.

---

## 6. Phase 3: verification and comparison (orchestrator)

When all agents have finished:

1. **Re-run every claim yourself**, in each worktree:
   - `cargo build --release`;
   - the `thumbv6m-none-eabi` library build;
   - `cargo test`;
   - `proofs/check.sh`;
   - all examples, including `embed.rs`;
   - the benchmark (sequentially, one worktree at a time, so timings are
     comparable).

   Then check the hard constraints independently:
   - **No compilation:** inspect the evaluator for bytecode or
     closure-compilation passes.
   - **Purity:** look for observable mutation in the language.
   - **Embeddability:** dependencies in `Cargo.toml`, `unsafe`, and
     whether fuel and memory limits actually work.

   Count the unfinished-proof markers independently with `grep`, and
   correct any scorecard that doesn't match.
2. **Write `worktrees/COMPARISON.md`:**
   - the ten scorecards, with your corrections marked;
   - a constraint-compliance table (purity, interpretation only, `no_std`,
     zero dependencies, limits, determinism);
   - a theorem-by-language matrix (F1–F8 against Lean, Rocq, Agda, Bend,
     Haskell) for each design;
   - primitive counts and evaluator sizes;
   - one benchmark table with all ten designs and grift's baseline, plus
     boot and clone costs;
   - what each design does uniquely well, and its fatal flaws if any;
   - which ideas combine (e.g. one design's capability model with another's
     effect handlers and a third's data model);
   - a recommendation for grift's future, with reasoning.
3. Leave the ten branches and worktrees in place, and don't merge anything
   into `vau-core`. `COMPARISON.md`, `TOOLCHAINS.md` and `BASELINE.md` stay
   in `worktrees/`, which is excluded from git, unless the user asks to
   commit them.
4. **Final message to the user:**
   - one table, with one row per design and the scorecard totals;
   - the recommendation;
   - the path to `COMPARISON.md`;
   - anything that needs the user's decision (missing toolchains, failed
     agents, disputed claims, constraint violations).
