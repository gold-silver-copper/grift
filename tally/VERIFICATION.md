# tally — verification plan

*Status: draft, version 0.1, 2026-10-02. Companion to
[`DESIGN.md`](DESIGN.md) (version 0.2); section numbers written "D§n" refer
to it. Papers are in [`reading/`](reading/README.md).*

This document says how to gain justified confidence in tally's core: what
to prove, which tools to use for which part, in what order, and how much
each step costs. The principle is an **assurance ladder**. Cheap steps that
find most bugs come first; expensive mechanized proofs come later, and only
for the theorems that need them.

| Rung | Method | Catches | Cost |
|---|---|---|---|
| 1 | Executable reference kernel in Haskell, differential and mutation testing against the Rust kernel | bugs in the rules and the implementation | weeks |
| 2 | A translator from tally to Lean 4 (and/or Rocq) that re-checks the whole library in an independent kernel | unsound typing of real programs | weeks |
| 3 | A Lean 4 mechanization of a core fragment: runtime safety, erasure, E-total, uniqueness | design flaws in the quantity and erasure rules | months |
| 4 | A proven Lean reference checker run as an oracle against the Rust kernel in CI | divergence between implementation and theory | weeks, on top of rung 3 |
| 5 | A paper proof, later mechanized, that the translation preserves typing, giving consistency relative to Lean/Rocq | logical inconsistency | months to years |

---

## 1. What we want to know

| # | Theorem | Informal statement | Why it matters |
|---|---|---|---|
| **T1** | Subject reduction and progress | Reducing a well-typed term preserves its type; a closed well-typed term in running mode is a value or can step. | Well-typed programs don't go wrong. |
| **T2** | Consistency | There is no closed *total* term of type `Empty`. | Proofs mean something. |
| **T3** | Decidability of conversion | NbE with all-or-nothing case-tree unfolding (D§3.4) terminates on well-typed total terms. | Type checking terminates. |
| **T4** | Erasure soundness | For a closed, well-typed, total program `p : Nat` (or any first-order type), the erased intermediate code computes the same value as `p`. | The compiled program is correct. |
| **T5** | E-total safety | The erased evaluation of a well-typed closed program never reaches `unreachable` and never uses a value at the wrong type, even when the program is *partial*, provided rule E-total holds (D§3.3). | Partiality can't break erasure. |
| **T6** | Uniqueness of primitive linear types | In any reachable runtime state of a well-typed program, each `Array` and the `World` token is referenced at most once. | In-place update is safe (D§5.2). |
| **T7** | Implementation correctness | The Rust kernel accepts exactly the declarations the rules accept. | The implementation matches the theory. |

Out of scope for now: correctness of the elaborator (it is untrusted by
design), of the C backend, and of the reference-counting runtime beyond T6.
Iris (§5.3) could address the latter later.

---

## 2. Roles of the tools

| Tool | Role | Theorems |
|---|---|---|
| **Haskell** | Executable reference kernel; property-based differential and mutation testing | T1 and T4 empirically; T7 empirically |
| **Lean 4** | Main mechanization (core fragment); proven reference checker run as an oracle; target of the library translator | T1, T4, T5, T6 proven for the fragment; T2 relative; T7 empirically |
| **Rocq** (MetaRocq, Iris) | Source of existing metatheory: PCUIC (Rocq's inductive types, `match`/`fix` and universes), a proven type checker, proven erasure; Iris for runtime resource reasoning; alternative translation target | T2 and T3 by reuse; templates for T4; T6 and reference counting via Iris |
| **Bend 2** (BendTT) | The closest existing proven design: running versus type-only code, quantities, a consistency proof in one Lean file. A proof template and a design contrast; not a target. | template for T1, T5 and the runtime half of T2 |
| **Agda** (not on the original list, but essential) | Where the closest existing proofs live: Abel, Danielsson & Eriksson's graded type theory with erasure; Danielsson's erased postulates and identity types | lemmas to port for T4 and T5 |

---

## 3. Rung 1 — Haskell reference kernel and differential testing

### 3.1 The reference kernel

A direct, unoptimized transcription of D§3 in Haskell, about 1,000–1,500
lines:
- de Bruijn indices;
- NbE with closures;
- the quantity/mode judgment;
- `case`, `case-K`, case-tree validation, lexicographic termination;
- E-total;
- builtin shape checks.

It follows the structure of Kovács's *elaboration-zoo*
(`reading/elaboration-zoo/02-typecheck-closures-debruijn`). It has no
elaborator: it reads the same textual core-term format the Rust kernel
prints (D§10 item 4).

Why Haskell for this rung: it is the fastest language to iterate on the
rules in, NbE is idiomatic in it, the reference material is in it, and
QuickCheck is mature. It is a *specification*, not a product, and may be
retired once the Lean oracle of rung 4 exists.

### 3.2 Properties

Run with QuickCheck on generated terms, against both kernels:

1. **Agreement.** Haskell and Rust accept and reject the same declarations,
   with the same error class.
2. **Subject reduction** (T1): if `⊢ t :σ A` and `t ⟶ t'`, then
   `⊢ t' :σ A`.
3. **NbE laws:** normalization is idempotent; conversion is reflexive,
   symmetric and transitive; reading back an evaluated normal form returns
   it unchanged.
4. **Erasure agreement** (T4, empirically): erase, run the erased code, and
   compare with NbE evaluation of the unerased term at first-order types.
5. **Unique arrays** (T6, empirically): an instrumented interpreter asserts
   at every step that no `Array` has two live references.

### 3.3 Generating terms

- **Well-typed terms** are generated *type-directed*. Pick a goal type, then
  an introduction form or a variable or constant whose type fits. Quantity
  budgets are tracked in the generator context, so linear variables are used
  exactly once by construction.
- **Mutation operators** turn well-typed terms into ill-typed ones. The
  kernel must reject every mutant that breaks a rule:
  - duplicate a linear variable, or drop one;
  - move a quantity-0 variable into running position;
  - insert a `partial` constant into an erased position (E-total);
  - make a recursive call non-decreasing;
  - add a negative or nested occurrence to a data declaration;
  - bump a universe level;
  - give an erased `case` a second constructor or a running field;
  - turn a `case-K` motive into one that abstracts the index.

A surviving mutant (one the kernel accepts) is a soundness bug candidate.
This is the most cost-effective soundness tool here: published kernel bugs
in other systems are typically of exactly this shape.

### 3.4 Exit criterion

Rung 1 runs in CI for M1–M3 (D§11). It passes when both kernels agree on
the full D§10 suite plus 10⁶ generated terms, and every mutation class is
rejected 100% of the time on 10⁵ mutants.

---

## 4. Rung 2 — Translating tally to Lean (and Rocq) to re-check the library

### 4.1 Idea

Translate checked tally declarations into Lean 4 declarations and re-check
them with Lean's kernel. If the translation preserves typing (rung 5 proves
this), any tally proof of `Empty` would become a Lean proof of `False`. Long
before that proof exists, re-checking the entire standard library and test
corpus in an independent kernel catches real unsoundness in tally's kernel.
This mirrors how Lean's own kernel is cross-checked by independent
re-implementations (lean4lean, written in Lean).

### 4.2 Mapping

| tally (D§3) | Lean 4 | Notes |
|---|---|---|
| `(Type ℓ)`, prenex level polymorphism, no cumulativity | `Type ℓ` (`Sort (ℓ+1)`), universe-polymorphic constants | Same design (D§3.5); levels map one-to-one. |
| `(Π (q x A) B)` | `(x : A) → B` | Quantities are forgotten (§4.4). |
| `λ`, application, `let` | the same | |
| `data` (inductive family) | `inductive` in `Type ℓ` | Strict positivity and universe checks are stronger in Lean (it accepts nesting), so every tally declaration is acceptable. |
| `case` | `casesOn` (or `match`) with an explicit motive | |
| `=` (D§3.7), a `Type`-valued family with K | `PLift (x = y)` over Lean's `Eq`, which lives in `Prop` | `case-K` becomes `Eq.rec`/`cast` with definitional proof irrelevance, so K holds. See §4.3. |
| total recursive definition (case tree, single-argument measure) | structural recursion (compiled to `brecOn`) | Unfolds definitionally, like tally's. |
| total recursive definition, lexicographic measure | nested structural auxiliaries, or `termination_by` | See §4.3. |
| `partial` definition | `opaque` constant with the same type (`axiom`-free) | Partial definitions never unfold in tally either. |
| `opaque` definition | `opaque` / `@[irreducible]` | |
| builtins (`Nat` with literals and acceleration) | Lean's `Nat` with GMP acceleration | Same technique (Allais 2023, D§3.9). |
| primitives (`Int64`, `String`, `Array`, `World`, …) | `opaque` types and constants with the declared types | Trusted in both systems; flagged in reports. |

### 4.3 Known technical risks

1. **Equality is a `Type` in tally but K comes free only in `Prop` in
   Lean.** The `PLift (Eq …)` encoding restores K. It changes how equality
   proofs are wrapped, so translated terms need `PLift.up`/`.down` inserted
   around every use. This is mechanical but must be validated on the whole
   equality library first.
2. **Lexicographic recursion versus definitional unfolding.** Lean's
   well-founded definitions are irreducible by default, so a tally proof
   that relies on unfolding (e.g. `refl : (= (ack 0 n) (succ n))`) would
   fail to re-check. Mitigations, in order of preference:
   - translate lexicographic measures to **nested structural recursion**
     (outer function on the first measure component, an inner auxiliary on
     the next), which unfolds definitionally. This is always possible for
     tally's lexicographic case trees (Abel & Altenkirch 2002);
   - otherwise emit `termination_by` and turn the affected `refl` proofs
     into rewriting with the generated equation lemmas. The translator
     reports these as *not definitionally faithful*.
3. **Conversion must map into conversion.** tally's all-or-nothing unfolding
   (D§3.4) is *weaker* than Lean's structural-recursion unfolding: every
   tally reduction is also a Lean reduction. So tally's `≡` is contained in
   Lean's, which is the direction the typing-preservation proof needs.

### 4.4 What the translation does and does not cover

- **Covers T2** (relative consistency), **once rung 5 proves the
  translation preserves typing.** Quantities, E-total and erasure play no
  role in logical consistency.
- **Does not cover T4–T6.** Quantities are forgotten. Those need rung 3.

### 4.5 Rocq as a second target

The same mapping works for Rocq:
- `Type@{ℓ}` with universe polymorphism;
- `Inductive`;
- `match`/`fix`, where Rocq's `fix` unfolds when the decreasing argument is
  a constructor, again a superset of tally's unfolding;
- `eq` with the `Eqdep.UIP` axiom (known to be consistent with Rocq) for K;
- `Equations` for lexicographic definitions.

Translating to **both** Lean and Rocq gives two independent second
opinions. More importantly, Rocq is the system for which MetaRocq has
formal metatheory (§5.3).

### 4.6 Exit criterion

The whole standard library and test corpus translates and re-checks in
Lean, with every non-definitionally-faithful definition listed and the list
shrinking over time. It runs in CI from M5.

---

## 5. Rung 3 — Mechanizing a core fragment in Lean 4

### 5.1 The fragment

Small enough to finish, big enough to contain every new idea:

| Included | Excluded (justified by other rungs or the literature) |
|---|---|
| Π with quantities 0/1/ω and modes σ ∈ {0,1} | universe polymorphism (one level variable at most; Lean-style levels are well understood) |
| two universes, `Type 0 : Type 1` | literals and acceleration |
| `Nat`, `Empty`, `Unit`, `Bool`, the equality family with `case-K` | general inductive families (the fragment has fixed ones; Dybjer 1994 and PCUIC cover the general schema) |
| an `Acc`-like family with an erased field | lexicographic termination in full (only single-argument structural recursion in the fragment) |
| one abstract linear type with `with`-style creation (models `Array`) | the elaborator, pattern compilation, no-confusion |
| `case` and erased `case`, `let`, case-tree definitions with structural recursion | |
| `partial` definitions and rule E-total | |
| erasure to an untyped intermediate language with a small-step semantics | |

### 5.2 Structure

Following BendTT's precedent (`reading/bendtt-affine-dependent-type-theory.pdf`):
one Lean project, with theorems stated *about an executable checker*
defined in the same development, so the proven artifact is the one that
runs (§6).

```
Tally/Syntax.lean        terms (de Bruijn), quantities, the semiring and its order
Tally/Typing.lean        the mode judgment Γ ⊢ t :σ A (inductive relation)
Tally/Reduction.lean     reduction for unerased terms; case-tree δ
Tally/Erase.lean         erasure function; the untyped intermediate language and its semantics
Tally/Checker.lean       executable checker; soundness: check = ok → derivable
Tally/Metatheory.lean    T1: substitution lemmas, subject reduction, progress
Tally/Erasure.lean       T4: simulation between erased and unerased evaluation
Tally/ETotal.lean        T5: safety of erased case under E-total
Tally/Unique.lean        T6: at most one reference per linear abstract value
```

### 5.3 Proof strategy per theorem

**T1 (subject reduction, progress).** Standard. The new ingredients are:
- **substitution respecting quantities:** substituting a term typed in
  context Γ₂ for a variable used q times adds q·Γ₂ (Atkey 2018);
- **progress in mode 1 only:** mode-0 terms are never run.

Conversion is handled by **parallel reduction and confluence**, the
technique of the BendTT mechanization and of Abel, Danielsson & Eriksson,
rather than by normalization. This avoids needing T3 inside the
development.

**T5 (E-total safety), the BendTT-style runtime argument.**
1. Running code terminates and does not get stuck, for total programs.
   BendTT proves termination of running code by counting calls, without
   normalizing types. tally's running code may copy functions (at ω), so
   that counting argument does not transfer directly. In the fragment, use
   structural recursion plus a logical-relations termination proof for the
   running fragment.
2. **The key lemma (Danielsson 2026).** In a closed program, every erased
   scrutinee of an erased `case`/`case-K` is a closed total term, because
   E-total forbids partial constants there and closed substitution instances
   preserve that. A closed total term of the equality or `Acc` family
   reduces to its constructor (canonicity for the total fragment). So the
   unerased evaluation would also take the single branch, and the erased
   evaluation's skipping of the scrutinee is faithful.
3. **Empty types.** By the same canonicity, there are no closed total terms
   of `Empty`, so an erased `case` on `Empty` is unreachable.
4. Partial definitions appear only in running positions, where the erased
   semantics evaluates them normally (they may diverge, but not go wrong).

**T4 (erasure soundness).** A simulation relation between unerased terms
and erased intermediate code: erasing a term whose type is first-order
commutes with evaluation, step by step. The pattern to follow is MetaRocq's
erasure correctness proof (to λ-box) and Abel, Danielsson & Eriksson's
Agda proof for graded type theory. Port lemma statements, not proofs.

**T6 (uniqueness).** Annotate the erased intermediate code with quantities
on binders, and prove a *resource invariant* on runtime configurations: the
multiset of references to each linear abstract value has size ≤ 1. It is
preserved by every step, because typing forbids an ω binding of a linear
abstract type (D§5.2). This is a linearity proof in the style of Wadler
(1990) and Linear Haskell. If it gets heavy, move it to Iris in Rocq, where
resource invariants are native.

**T2 for the fragment.** The runtime half comes cheaply from T5: a closed
total running term of `Empty` would evaluate to a value of `Empty`, and
there is none. This is BendTT's consistency argument. Full logical
consistency, including terms that only appear in types, comes from rung 5.

### 5.4 Using machine help for proofs

BendTT's Lean development reports human-specified statements with
machine-generated proofs, all checked by Lean. The same workflow is
reasonable here:
- humans write and review every *definition* and *theorem statement*;
- proof scripts may be machine-produced, since Lean checks them;
- `sorry` and `axiom` are forbidden outside the declared primitive axioms,
  and CI checks with `#print axioms` on every theorem.

### 5.5 Exit criterion

T1, T4, T5 and T6 proven for the fragment with no `sorry` and no axioms
beyond those listed. The fragment's typing rules are reviewed line by line
against D§3.

---

## 6. Rung 4 — The proven checker as an oracle

Extract the fragment checker of §5.2 (later, a checker for all of D§3) as a
Lean executable, and run it on every declaration the Rust kernel checks in
CI. This mirrors Bend's `--verdict` mode, which runs the Lean-proven BendTT
kernel. Disagreement fails the build.

For the parts of D§3 not yet in the fragment, the oracle falls back to the
Haskell reference kernel (rung 1). It is "trusted but not proven" for those
parts, and the oracle reports which rules each check used.

**Optional, later: proving the Rust kernel.** Aeneas translates safe Rust
to Lean. Self-contained Rust kernel components with crisp specifications
(level normalization, the lexicographic termination check, the quantity
arithmetic) could be proven directly. The NbE core is a poor first
candidate (heavy sharing and closures).

---

## 7. Rung 5 — Typing preservation of the translation (relative consistency)

**Statement.** If `Γ ⊢ t :σ A` in tally (total fragment), then
`⟦Γ⟧ ⊢ ⟦t⟧ : ⟦A⟧` in the target theory. The target is Lean's kernel theory
or PCUIC with UIP. It follows that a closed total `t : Empty` gives a closed
`⟦t⟧ : False`, contradicting the target's consistency.

**Proof obligations** (on paper first):
1. ⟦–⟧ commutes with substitution.
2. tally's `≡` maps into the target's `≡` (§4.3 item 3).
3. Every tally inductive declaration maps to an accepted target declaration
   (positivity and universes are at least as permissive in the target).
4. Each case tree with a lexicographic measure maps to an accepted, nested
   structural `fix` whose unfolding simulates all-or-nothing unfolding.
5. `case-K` maps to UIP, which is consistent with the target.

**Mechanizing it** is a large project, comparable to a slice of MetaRocq.
Two routes:
- **In Rocq, on top of MetaRocq's PCUIC:** reuse its consistency,
  confluence and subject reduction, and prove only the translation lemmas.
  The most leverage per line.
- **In Lean:** extend the rung-3 fragment to a model of Lean's own kernel
  theory, as lean4lean has started for Lean itself. More consistent with the
  rest of the plan, less reusable infrastructure.

Recommendation: do the paper proof together with rung 2. Mechanize via
MetaRocq only if tally grows real users who depend on its logic.

---

## 8. What Bend 2 contributes, and what it doesn't

`reading/bendtt-affine-dependent-type-theory.pdf` describes BendTT, Bend 2's
core.

**Directly reusable ideas.**
- **Running versus type-only code.** BendTT's "live" and "dead" code are
  tally's modes 1 and 0 (D§3.3). The mode split is the right abstraction to
  mechanize, and BendTT shows it fits in one Lean development.
- **Runtime-first consistency.** "Running code terminates, evaluation
  preserves types, and no value of the empty type exists" proves the runtime
  half of T2 without normalizing types. tally uses the same shape in §5.3.
- **Theorems about the running checker.** BendTT states its theorems about
  the executable checker in the same file and runs that checker in
  production (`--verdict`). That is rung 4.
- **A conservative baseline for erased matches.** BendTT forbids matching
  on erased values entirely. tally allows one extension (erased `case` on
  single-constructor, all-erased families, guarded by E-total, D§3.6). The
  mechanization must justify exactly that extension (§5.3, T5 step 2), with
  Danielsson (2026) as the reference argument.

**What does not transfer.** BendTT is consistent despite `Type : Type` and
negative datatypes *because running code cannot copy functions*: copyable
types have a separate kind, `Data`, which contains no functions. tally
deliberately lets ω-quantity code copy functions, so it needs its universe
hierarchy and positivity check. BendTT's consistency proof does not apply to
tally as designed. DESIGN.md §12 item 14 records adopting the Data/Type
split as an alternative design.

**Caveats.**
- Bend 2 describes itself as young.
- Its Lean proofs are machine-produced (machine-checked).
- Consistency with `Type : Type` is a new claim, not yet peer-reviewed.

Read the Lean file and re-check it locally before porting any lemma or
statement.

**Not a target.** tally programs cannot be checked or proven by Bend:
- BendTT forbids the function copying that tally permits;
- BendTT accepts types that tally rejects (and vice versa for erased
  matches).

Using Bend as a compilation backend would also break on copied closures.

---

## 9. Schedule

Dates are relative to the implementation milestones in D§11. Estimates are
for one person and rough.

| Phase | With | Work | Estimate | Rung |
|---|---|---|---|---|
| V1 | M1–M3 | Haskell reference kernel; term generator; mutation operators; differential CI | 3–5 weeks | 1 |
| V2 | after M3 | Lean fragment: syntax, typing, reduction, erasure; T1 | 6–10 weeks | 3 |
| V3 | after M5 | Lean translator (and later Rocq); library re-check in CI; paper proof of typing preservation | 4–8 weeks | 2, 5 (paper) |
| V4 | after V2 | T5 and T4 for the fragment; executable checker plus soundness; oracle in CI | 8–14 weeks | 3, 4 |
| V5 | after M6 | T6 (resource invariant), in Lean or in Iris | 4–8 weeks | 3 |
| V6 | optional | Mechanized typing preservation on MetaRocq; Aeneas proofs of selected Rust kernel parts | months to years | 5, 4 |

**What to do first:** V1, then V3's library re-check. Together they deliver
most of the assurance for a small fraction of the cost.

---

## 10. Success criteria

- **Before announcing tally as a proof assistant:** rungs 1, 2 and 4 in CI
  and T2 proven on paper.
- **Before claiming erasure is safe in the presence of partiality:** T5
  mechanized (rung 3).
- **Before claiming in-place updates are safe:** T6 mechanized, or
  validated by the instrumented interpreter on the whole test corpus with a
  written argument.

---

## 11. Risks and open questions

1. **Termination of running code with function copying (T5 step 1).**
   BendTT's call-counting argument relies on affinity. tally needs a
   logical-relations proof, the most technically demanding item in rung 3.
   Fallback: prove T5 only for running code whose ω-quantity values are
   first-order. That is weaker, but covers the erased-case safety question.
2. **Definitional faithfulness of the Lean translation** (§4.3 item 2) may
   force changes in how tally elaborates lexicographic recursion, towards
   nested structural recursion. That is acceptable if the translation shows
   it is needed.
3. **The equality encoding** (§4.3 item 1) must be validated on the whole
   equality library early.
4. **Drift between DESIGN.md, the Rust kernel, the Haskell spec and the
   Lean fragment.** Mitigation: the textual core-term format is the single
   interchange format. Every rule change updates all of them in one change
   set, with the D§16 revision history as the log.
5. **Machine-produced proofs** hide no soundness risk (Lean checks them)
   but can hide *statement* errors. Mitigation: human review of every
   definition and theorem statement (§5.4).

---

## 12. References

All in `reading/` unless noted:
- BendTT (Taelin et al., 2026) — §5, §6, §8.
- Atkey 2018; McBride 2016 — quantities and substitution (§5.3).
- Abel, Danielsson & Eriksson 2023 and 2026; Danielsson 2026 — erasure, and
  E-total (§5.3).
- Abel & Altenkirch 2002; Abel 1998 — lexicographic structural recursion
  (§4.3).
- Allais 2023 — builtin `Nat` (§4.2).
- Wadler 1990; Bernardy et al. 2018 — linearity (§5.3).
- Kovács, *elaboration-zoo*; Christiansen, NbE tutorial; Löh, McBride &
  Swierstra — the Haskell reference kernel (§3).
- Not in `reading/`; look up as needed:
  - MetaRocq (formerly MetaCoq), including PCUIC, the verified checker and
    verified erasure — §4.5, §7;
  - Iris — §5.3;
  - lean4lean — §4.1, §7;
  - Aeneas — §6;
  - Rocq's `Equations` — §4.5.
