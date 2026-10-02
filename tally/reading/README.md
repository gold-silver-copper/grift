# Reading list for tally

Papers, theses and code behind the design in [`../DESIGN.md`](../DESIGN.md),
grouped by the part of the design they inform. Each entry notes what to
read it for. Section numbers (§) refer to `DESIGN.md`.

Everything here was downloaded from an author, university, arXiv or
publisher copy that is free to read. Papers available only from behind
publisher or anti-bot walls are listed at the end with links.

## Start here

| File | What to read it for |
|---|---|
| `brady-2021-idris2-qtt-in-practice.pdf` — Brady, *Idris 2: Quantitative Type Theory in Practice*, ECOOP 2021 | The closest existing language to tally: QTT in a real implementation, erasure, linearity, the core language, and lessons learned. |
| `loh-mcbride-swierstra-2010-lambda-pi-tutorial.pdf` — Löh, McBride & Swierstra, *A Tutorial Implementation of a Dependently Typed Lambda Calculus* | The smallest complete dependent type checker: the shape of the kernel (§3). |
| `elaboration-zoo/` — András Kovács, *elaboration-zoo* (Haskell code; `.git` removed) | Step-by-step reference implementations: closures and de Bruijn evaluation (01), type checking with normalization by evaluation (02), holes (03), implicit arguments (04), pruning (05), first-class polymorphism (06). The best model for the elaborator (§6). |
| `christiansen-nbe-tutorial.html` — Christiansen, *Checking Dependent Types with Normalization by Evaluation: A Tutorial* | Normalization by evaluation for conversion checking, step by step. |

## Foundations: dependent type theory

| File | What to read it for |
|---|---|
| `martin-lof-1984-intuitionistic-type-theory.pdf` — Martin-Löf, *Intuitionistic Type Theory* (Sambin's notes, Bibliopolis) | The original predicative theory: Π, Σ, identity types, universes, judgments. |
| `coquand-huet-1988-calculus-of-constructions.pdf` — Coquand & Huet, *The Calculus of Constructions*, Information and Computation 1988 | The impredicative alternative, and the top of the lambda cube. |
| `barendregt-1992-lambda-calculi-with-types.pdf` — Barendregt, *Lambda Calculi with Types* (Handbook of Logic in CS) | The lambda cube and pure type systems: where tally sits. |
| `hurkens-1995-simplification-of-girards-paradox.pdf` — Hurkens, *A Simplification of Girard's Paradox* | Why `Type : Type` is inconsistent. A required rejection test for the kernel (§3.5, §10). |
| `coquand-1996-algorithm-type-checking-dependent-types.pdf` — Coquand, *An Algorithm for Type-Checking Dependent Types* | The classic checking-with-evaluation algorithm. |
| `abel-2013-habilitation-nbe-dependent-types.pdf` — Abel, *Normalization by Evaluation: Dependent Types and Impredicativity* (habilitation) | Theory of normalization by evaluation for conversion, including η. |
| `hott-book-2013.pdf` — The Univalent Foundations Program, *Homotopy Type Theory* | Background on identity types, and why tally does *not* aim for univalence (§3.7). |

## Quantities, linearity and erasure

| File | What to read it for |
|---|---|
| `mcbride-2016-plenty-o-nuttin.pdf` — McBride, *I Got Plenty o' Nuttin'* | The origin of quantitative type theory: types checked "at zero". |
| `atkey-2018-quantitative-type-theory.pdf` — Atkey, *Syntax and Semantics of Quantitative Type Theory*, LICS 2018 | The precise QTT rules (the σ ∈ {0,1} mode, context scaling) on which §3.3 is based, and their semantics. |
| `abel-danielsson-eriksson-2023-graded-type-theory-erasure-formalized.pdf` — Abel, Danielsson & Eriksson, *A Graded Modal Dependent Type Theory with a Universe and Erasure, Formalized*, ICFP 2023 | A machine-checked account of erasure soundness, and the conditions on erased matches (§3.6, §3.11, §5.1). |
| `graded-modal-dtt-with-erasure-formalized-2026.pdf` — Abel, Danielsson & Eriksson, *A Graded Modal Dependent Type Theory with Erasure, Formalized* (arXiv 2603.29716, 2026) | The extended journal version: unit and graded Σ types, substitution for grade assignment (§3.11). |
| `erased-postulates-identity-types-quotients-2026.pdf` — Danielsson, *Erased Postulates, Identity Types and Quotients* (arXiv 2609.08578, 2026) | When transport along erased identity proofs is safe: no inconsistent erased postulates. The basis of rule E-total (§3.3); also quotients via erased postulates (§12). |
| `bendtt-affine-dependent-type-theory.pdf` — Taelin et al., *BendTT: An Affine Dependent Type Theory* (Higher Order Company, 2026; from github.com/bendlang/bend) | Bend 2's core. Its "live" and "dead" code correspond to tally's modes 1 and 0. It keeps `Type : Type` and negative datatypes consistent by forbidding copies of functions in running code, and forbids matching on erased scrutinees. Confluence, subject reduction, progress, termination of live code and consistency are claimed in one Lean 4 file (the paper says the Lean proofs were AI-produced and machine-checked). A template for mechanizing tally's runtime theorems, and the main alternative design to compare against (§3.5, §3.6, §12). |
| `moon-eades-orchard-2021-graded-modal-dependent-type-theory.pdf` — Moon, Eades & Orchard, *Graded Modal Dependent Type Theory*, ESOP 2021 | Grades separating "used in types" from "used in terms": an alternative to QTT. |
| `choudhury-eades-eisenberg-weirich-2021-graded-dependent-usage-aware.pdf` — Choudhury, Eades, Eisenberg & Weirich, *A Graded Dependent Type System with a Usage-Aware Semantics*, POPL 2021 | Usage-aware semantics and the subusaging questions discussed in §3.2. |
| `orchard-liepelt-eades-2019-granule-graded-modal-types.pdf` — Orchard, Liepelt & Eades, *Quantitative Program Reasoning with Graded Modal Types* (Granule), ICFP 2019 | Graded modalities in practice, and a route towards borrowing-like patterns (§12). |
| `bernardy-et-al-2018-linear-haskell.pdf` — Bernardy et al., *Linear Haskell* | Linearity on function arrows, linear APIs for arrays and I/O (§5.3, §5.4). |
| `wadler-1990-linear-types-can-change-the-world.pdf` — Wadler, *Linear Types Can Change the World!* | The classic argument for in-place update through linearity. |

## Inductive families and pattern matching

| File | What to read it for |
|---|---|
| `dybjer-1994-inductive-families.pdf` — Dybjer, *Inductive Families* | The schema for inductive families with parameters and indices (§3.6). |
| `chapman-dagand-mcbride-morris-2010-gentle-art-of-levitation.pdf` — Chapman, Dagand, McBride & Morris, *The Gentle Art of Levitation* | The descriptions alternative: one universe of data-type descriptions. |
| `goguen-mcbride-mckinna-2006-eliminating-dependent-pattern-matching.pdf` — Goguen, McBride & McKinna, *Eliminating Dependent Pattern Matching* | How dependent pattern matching reduces to eliminators plus K. |
| `cockx-devriese-piessens-2014-pattern-matching-without-k.pdf` — Cockx, Devriese & Piessens, *Pattern Matching Without K*, ICFP 2014 | The alternative, if tally ever drops K (§3.7). |
| `cockx-abel-2018-elaborating-dependent-copattern-matching.pdf` — Cockx & Abel, *Elaborating Dependent (Co)pattern Matching*, ICFP 2018 | The algorithm for compiling clauses to case trees, with coverage checking (§6.3). |
| `cockx-2017-thesis-dependent-pattern-matching.pdf` — Cockx, *Dependent Pattern Matching and Proof-Relevant Unification* (PhD thesis) | Unification for pattern matching in depth. |
| `mcbride-2002-elimination-with-a-motive.pdf` — McBride, *Elimination with a Motive*, TYPES 2000 | Motive generalization with equations: the first step of translating a split to a kernel `case` (§6.3). |

## Termination

| File | What to read it for |
|---|---|
| `abel-1998-foetus-termination-checker.pdf` — Abel, *foetus — Termination Checker for Simple Functional Programs* | The lexicographic call-relation criterion that the kernel verifies (§3.8). |
| `abel-altenkirch-2002-predicative-analysis-structural-recursion.pdf` — Abel & Altenkirch, *A Predicative Analysis of Structural Recursion*, JFP 2002 | The soundness argument for lexicographic structural recursion (§3.8). |
| `norell-2007-thesis-practical-dependent-types.pdf` — Norell, *Towards a Practical Programming Language Based on Dependent Type Theory* (PhD thesis) | Agda's design: metavariables, implicit arguments, pattern matching, the overall elaborator architecture. |
| `allais-2023-builtin-types-as-inductive-families.pdf` — Allais, *Builtin Types viewed as Inductive Families* | Making `Nat` an inductive type that still compiles to machine integers (§7.3). |

## Elaboration and unification

| File | What to read it for |
|---|---|
| `dunfield-krishnaswami-2021-bidirectional-typing.pdf` — Dunfield & Krishnaswami, *Bidirectional Typing* (survey) | How to organize checking and inference (§6.1). |
| `abel-pientka-2011-higher-order-pattern-unification.pdf` — Abel & Pientka, *Higher-Order Dynamic Pattern Unification for Dependent Types and Records* | The unification algorithm behind implicit arguments (§6.2). |
| `gundry-2013-thesis-type-inference-haskell-dependent-types.pdf` — Gundry, *Type Inference, Haskell and Dependent Types* (PhD thesis) | Unification in context, and type inference meeting dependent types. |
| `brady-2013-idris-design-and-implementation.pdf` — Brady, *Idris, a General-Purpose Dependently Typed Programming Language: Design and Implementation*, JFP 2013 | The elaborator-as-tactics architecture of Idris 1. |
| `christiansen-brady-2016-elaborator-reflection.pdf` — Christiansen & Brady, *Elaborator Reflection: Extending Idris in Idris*, ICFP 2016 | The design for tally's metaprogramming stage 2 (§8.2). |
| `de-moura-ullrich-2021-lean4.pdf` — de Moura & Ullrich, *The Lean 4 Theorem Prover and Programming Language* | A production design with a small kernel, a large elaborator and metaprogramming in the language itself. |

## Equality

| File | What to read it for |
|---|---|
| `altenkirch-mcbride-swierstra-2007-observational-equality-now.pdf` — Altenkirch, McBride & Swierstra, *Observational Equality, Now!*, PLPV 2007 | Function extensionality with decidable checking: the main alternative to intensional equality (§3.7, §12). |
| `pujet-tabareau-2022-observational-equality-now-for-good.pdf` — Pujet & Tabareau, *Observational Equality: Now for Good*, POPL 2022 | The modern, proven-normalizing version of observational type theory. |
| `pujet-tabareau-2025-observational-equality-meets-cic.pdf` — Pujet & Tabareau, *Observational Equality Meets CIC* | Observational equality together with inductive families: what adopting it later would require. |

## Overloading

| File | What to read it for |
|---|---|
| `selsam-ullrich-de-moura-2020-tabled-typeclass-resolution.pdf` — Selsam, Ullrich & de Moura, *Tabled Typeclass Resolution* | Lean 4's type-class resolution: one candidate for overloading (§12 item 4). |

## Runtime and effects

| File | What to read it for |
|---|---|
| `reinking-et-al-2021-perceus.pdf` — Reinking, Xie, de Moura & Leijen, *Perceus: Garbage Free Reference Counting with Reuse* (MSR technical report) | Precise reference counting with in-place reuse: the runtime design (§7.2). |
| `ullrich-de-moura-2019-counting-immutable-beans.pdf` — Ullrich & de Moura, *Counting Immutable Beans* | Reference counting optimized for pure functional languages, as used in Lean 4. |
| `plotkin-pretnar-2013-handling-algebraic-effects.pdf` — Plotkin & Pretnar, *Handling Algebraic Effects*, LMCS 2013 | Foundations of effect handlers: the future effects layer (§5.4, §12). |
| `leijen-2017-type-directed-compilation-row-typed-effects.pdf` — Leijen, *Type Directed Compilation of Row-Typed Algebraic Effects*, POPL 2017 | Koka's row-typed effects and how to compile them. |
| `leijen-2016-algebraic-effects-for-functional-programming-tr.pdf` — Leijen, *Algebraic Effects for Functional Programming* (MSR technical report) | The longer, more tutorial version of the above. |

## Not downloaded (no freely downloadable copy found)

| Paper | Where | Relevance |
|---|---|---|
| Lee, Jones & Ben-Amram, *The Size-Change Principle for Program Termination*, POPL 2001 | <https://dl.acm.org/doi/10.1145/360204.360210> | A stronger termination checker than tally's lexicographic one (§3.8, §12 item 8). |
| Paulin-Mohring, *Inductive Definitions in the System Coq: Rules and Properties*, TLCA 1993 | <https://inria.hal.science/hal-01095078> (behind an anti-bot challenge) | Inductive types with `match` and `fix` in a kernel: background for §3.6 and §3.8. |
| Tejiščák, *A Dependently Typed Calculus with Pattern Matching and Erasure Inference*, ICFP 2020 | <https://dl.acm.org/doi/10.1145/3408973> | Inferring quantity-0 annotations instead of writing them (§12). |
| McBride, Goguen & McKinna, *A Few Constructions on Constructors*, TYPES 2004 | <https://doi.org/10.1007/11617990_12> | The original no-confusion and acyclicity constructions used by the pattern compiler (§6.3). |
