# tally — design document

*Status: draft, version 0.2, 2026-10-02 (audited; §16 lists the changes from
0.1). "tally" is a working name: quantities tally how often each value is
used.*

tally is a minimal Lisp with full dependent types and quantitative types.

**What you can express**
- Types can mention values, e.g. `(Vec A n)` is a vector of exactly `n`
  elements.
- Programs are proofs: a total function of type `(= (+ n zero) n)` proves
  that equation.

**What runs**
- Every binder carries a quantity, `0`, `1` or `ω`. The quantities say:
  - which values exist only for type checking and are erased;
  - which are used exactly once, and so can thread resources and permit
    in-place update;
  - which are ordinary.
- The compiled program carries no proofs or type-level indices. It can
  mutate arrays in place and do I/O without monads stacked on monads.

**How it is built**
- A small trusted kernel checks a core calculus: quantitative type theory
  (QTT) over Martin-Löf type theory with inductive families.
- An untrusted elaborator turns s-expression surface syntax into kernel
  terms.
- Everything else (Bool, Nat, equality, lists, `if`, I/O helpers, `do`
  notation) is library code.

The reading list behind this design is in [`reading/`](reading/README.md);
§14 maps references to files there.

---

## 1. Goals and non-goals

### Goals

1. **Expressive.** Full dependent types: Π-types over any universe, inductive
   families, large elimination (computing types by recursion). Specifications
   and proofs are written in the language itself.
2. **Minimal trusted core.** Only the kernel and an enumerated list of
   primitives (§3.10) must be correct for soundness. The kernel should stay
   near 2,000 lines of Rust and be readable in an afternoon. Everything that
   can be defined in the library is.
3. **Efficient execution.**
   - Quantity-0 terms are erased, so proofs and indices cost nothing at
     runtime.
   - Uniquely held values (§5.2) are updated in place.
   - Memory is managed by precise reference counting.
4. **A Lisp.** S-expression syntax, a REPL, and metaprogramming written in the
   language (syntax macros first, elaborator reflection later).
5. **Sound by default.** Definitions are total unless explicitly marked
   `partial`.
   - Partial definitions never reduce during type checking.
   - They may not appear in erased positions of running code (§3.3), so
     erasure can never turn them into a crash.
   - Any definition that runs one is itself partial.

### Non-goals (for the first versions)

- Univalence or higher inductive types.
- Impredicativity (no `Prop` sort).
- Full type inference (top-level definitions need type annotations).
- Effect rows or algebraic effects; I/O goes through a linear world token
  (§5.4).
- Floating point, concurrency, a foreign function interface beyond declared
  primitives.
- Borrowing; QTT has none, and §12 lists the options.

---

## 2. Architecture at a glance

```
source (.tl)
  │  reader: s-expressions with spans
  ▼
surface syntax
  │  syntax macros (stage 1, §8.1), compiled and run natively
  ▼
elaborator (untrusted, §6)
  │  bidirectional checking, metavariables, implicit arguments,
  │  pattern-match compilation, coverage, termination measures,
  │  quantity inference
  ▼
core terms (definitions with case-tree bodies)
  │  KERNEL (trusted, §3): re-checks every declaration
  ▼
checked signature
  │  erasure (§7.1): drop quantity-0 terms and all types
  ▼
untyped IR, with reference-counting instructions inserted (§7.2)
  │
  ▼
interpreter (first)  /  C backend (later)
```

The elaborator may be large and clever; the kernel re-checks everything it
produces. A bug in the elaborator can make the system reject a good program
or report a bad error, but not accept an ill-typed one. This is the de Bruijn
criterion, which Lean, Rocq and Agda's design also follow.

---

## 3. The core calculus

### 3.1 Core syntax

Core terms use de Bruijn indices; names below are for readability.

```
ℓ    ::= n | u | (succ ℓ) | (max ℓ ℓ)        universe levels (u: level variables)
q    ::= 0 | 1 | ω                           quantities
t, A ::= x                                   variable
       | c.{ℓ ...}                           constant, instantiated at levels
       | (Type ℓ)                            universe
       | (Π (q x A) B)                       dependent function type
       | (λ (x) t)                           function
       | (t u)                               application
       | (let (q x A t) u)                   local definition
       | (case q t (ȳ z ⇒ P)                dependent case (§3.6)
               (c x̄ ⇒ t) ...)
       | (case-K q t (z ⇒ P) t)              axiom K for builtin equality (§3.7)
       | literal                             Nat, Int64, Char or String literal
```

A constant is a definition, inductive type, constructor or primitive.

**Declarations.** The kernel checks a sequence of declarations (the
*signature*):

- **Definitions:** `(def c.{ū} : A body)`, optionally `partial` or `opaque`,
  possibly in a `mutual` block. A recursive definition's body must be a
  **case tree** (§3.8): λs, then nested `case`s on variables, with ordinary
  terms at the leaves.
- **Inductive families** (§3.6), possibly mutual.
- **Primitives:** a name and a declared type, from the fixed list in §3.9.
- **Builtin registrations:** `(builtin Nat c)` and similar (§3.9), which tell
  the kernel and runtime which library declaration plays a special role,
  after a shape check.

**What is not in the core.**
- Σ, `=`, `Unit`, `Bool` and `Nat` are inductive families declared in the
  library.
- Local recursive functions are lifted to top-level definitions by the
  elaborator, with captured variables as extra parameters. The lexicographic
  termination check (§3.8) accepts the unchanged captured arguments.

### 3.2 Quantities

Quantities form the semiring R = {0, 1, ω}. ω means "any number of times".

| + | 0 | 1 | ω |   | · | 0 | 1 | ω |
|---|---|---|---|---|---|---|---|---|
| **0** | 0 | 1 | ω |   | **0** | 0 | 0 | 0 |
| **1** | 1 | ω | ω |   | **1** | 0 | 1 | ω |
| **ω** | ω | ω | ω |   | **ω** | 0 | ω | ω |

Actual usage is compared with declared quantities by the preorder
`0 ⊑ ω`, `1 ⊑ ω` (plus reflexivity). Addition and multiplication are
monotone for it, so R is an ordered semiring.

- A variable declared **ω** may be used any number of times, including zero
  or once.
- A variable declared **1** must be used exactly once: `0 ⋢ 1`, so dropping
  it is an error.
- A variable declared **0** may appear only where uses are not counted:
  types, and other quantity-0 positions.

This is Idris 2's discipline (Brady 2021). Strict linearity rather than
affinity is deliberate: it guarantees that a resource is consumed, so arrays
are frozen or freed and the world token is returned. The affine alternative
is discussed in §12.

**Quantities are part of a function type.** `(Π (1 x A) B)` and
`(Π (ω x A) B)` are different types and are not convertible. A linear
function may still be used where an unrestricted one is expected, because
the elaborator inserts an η-expansion (§6.4). No subtyping between quantities
enters the kernel.

### 3.3 Typing rules

Contexts carry a quantity per variable: `Γ ::= · | Γ, x :q A`.
- Contexts with the same underlying variables and types add pointwise
  (`Γ₁ + Γ₂`) and scale pointwise (`q·Γ`).
- `0Γ` is Γ with every quantity set to 0.
- `Γ ⊑ Γ'` is the pointwise preorder.

The judgment is `Γ ⊢ t :σ A` with **mode** σ ∈ {0, 1}, following Atkey
(2018).
- σ = 0 means "checked for types only": every quantity in Γ is 0, and no
  usage is counted.
- σ = 1 means "this term will run".
- Types are always checked at σ = 0, which is why a quantity-0 variable may
  appear in types.

```
Var     0Γ ⊢ A :0 (Type ℓ)
        ─────────────────────────────
        0Γ, x :σ A ⊢ x :σ A

Weak    Γ ⊢ t :σ B        0Γ ⊢ A :0 (Type ℓ)
        ─────────────────────────────
        Γ, x :0 A ⊢ t :σ B

Sub     Γ ⊢ t :σ A        Γ ⊑ Γ'
        ─────────────────────────────
        Γ' ⊢ t :σ A

Const   c.{ū} : A ∈ Σ          ℓ̄ well-formed
        ─────────────────────────────
        0Γ ⊢ c.{ℓ̄} :σ A[ℓ̄/ū]

Type    0Γ ⊢ (Type ℓ) :0 (Type (succ ℓ))

Pi      0Γ ⊢ A :0 (Type ℓ₁)        0Γ, x :0 A ⊢ B :0 (Type ℓ₂)
        ─────────────────────────────
        0Γ ⊢ (Π (q x A) B) :0 (Type (max ℓ₁ ℓ₂))

Lam     Γ, x :(σ·q) A ⊢ t :σ B
        ─────────────────────────────
        Γ ⊢ (λ (x) t) :σ (Π (q x A) B)

App     Γ₁ ⊢ f :σ (Π (q x A) B)      Γ₂ ⊢ a :σ' A      σ' = 0 if σ·q = 0, else 1
        ─────────────────────────────
        Γ₁ + q·Γ₂ ⊢ (f a) :σ B[a/x]

Let     Γ₁ ⊢ a :σ' A      Γ₂, x :(σ·q) A ⊢ u :σ B      σ' as in App
        ─────────────────────────────
        q·Γ₁ + Γ₂ ⊢ (let (q x A a) u) :σ B[a/x]

Conv    Γ ⊢ t :σ A        0Γ ⊢ A ≡ B
        ─────────────────────────────
        Γ ⊢ t :σ B
```

**Global definitions.** The kernel checks a definition `(def c : A body)`
as follows:
- check `· ⊢ A :0 (Type ℓ)`;
- check `· ⊢ body :1 A` in the empty context, so a definition captures no
  resources and may be used any number of times (rule `Const`);
- for a mutual block, add the declared types to the signature first, and
  check the termination measure (§3.8) instead of allowing arbitrary
  self-reference;
- a partial definition is checked the same way, minus termination.

**Erased positions must be total (rule E-total).**
- **Definition.** A term is in an *erased position* when it is checked at
  mode 0 *from* mode 1. That covers three cases, when the enclosing term is
  checked at σ = 1:
  - the argument in `App` with q = 0;
  - the bound term in `Let` with q = 0;
  - the scrutinee of `case`/`case-K` with q = 0.
- **Rule.** No partial constant may occur in an erased position.
- **Why.** Erased terms are never evaluated, so nothing forces them to be
  canonical. A partial definition can inhabit any type, including
  `(= Nat Bool)` or `Empty`. Combined with erased `case` (§3.6), such an
  argument would let running code transport a `Nat` as a `Bool`, or reach
  `unreachable`.
- **What it does not restrict.** Types themselves (Π-domains, annotations,
  motives) may still mention partial constants. They are stuck there,
  because partial constants never unfold, and harmless.
- **Literature.** This is the "no inconsistent erased postulates" condition
  under which Danielsson (2026) proves that transport along erased identity
  proofs is safe, and under which erased matches are sound in Abel,
  Danielsson & Eriksson (2023, 2026).

**Notes.**
- **Erased arguments use nothing.** An argument at a quantity-0 parameter is
  checked at mode 0, may mention erased variables, and its context is scaled
  by 0.
- **Partiality is recorded per constant.** A definition is partial if it is
  declared so, or if a partial constant occurs in a *runtime* position (an
  instance of `Const` at σ = 1) of its body. Occurrences in types do not make
  it partial.

### 3.4 Definitional equality

`A ≡ B` is decided by **normalization by evaluation** (NbE): evaluate both
sides into a semantic domain with closures, read them back to normal form,
and compare.

It includes:
- **β:** function application;
- **ζ:** `let`;
- **ι:** `case` on a constructor, and `case-K` on `refl`;
- **η for functions:** `(λ (x) (f x)) ≡ f`;
- **δ:** unfolding definitions, with the restrictions below.

**Unfolding.**
- **Non-recursive definitions** unfold freely.
- **Recursive definitions unfold all-or-nothing, as case trees.** Applying
  `f` to arguments evaluates its case tree. If every split meets a
  constructor (or a literal) and a leaf is reached, the application reduces
  to that leaf. If a split meets a neutral term, the application stays the
  neutral `(f args…)`, with no partial unfolding.
  - This is Agda's semantics.
  - It keeps read-back terminating: a recursive call on a pattern variable
    blocks immediately instead of unfolding forever under binders.
  - It also handles lexicographic recursion, which Rocq's "unfold when the
    decreasing argument is a constructor" rule does not.
- **Partial and `opaque` definitions** never unfold.

**Literals.**
- `Nat` literals are values that convert to and from `zero`/`succ` when a
  split needs it.
- Registered accelerated operations (§3.9) compute natively on literals.

**Implementation.** Closures over de Bruijn levels, with "glued" evaluation:
values keep both the unfolded and the folded form, so error messages and
elaboration show `(+ n m)` rather than its unfolding. See Kovács's
elaboration-zoo (`02-typecheck-closures-debruijn`, `GluedEval.hs`),
Christiansen's tutorial and Abel's habilitation.

### 3.5 Universes

A predicative hierarchy, `(Type 0) : (Type 1) : …`, with **prenex universe
polymorphism** and **no cumulativity**, as in Lean.

- Top-level constants may abstract over level variables:
  `(data (List {A (Type u)}) …)` gives `List.{u}`.
- Level expressions are `n | u | (succ ℓ) | (max ℓ ℓ)`. Two levels are
  compared by normalizing max-expressions (Lean's algorithm). The elaborator
  solves level metavariables; the kernel only checks the instantiated
  levels.
- `Type` without a level is `(Type 0)`. A `Π` lives at the max of its parts'
  levels (rule `Pi`).

Why this choice:
- **`Type : Type`** (Idris 2's current behaviour) is inconsistent: Hurkens's
  paradox (`reading/hurkens-1995-…`). That conflicts with goal 5.
- **Cumulativity without polymorphism** forces duplicated definitions for
  every level.
- **Cumulativity plus polymorphism** (Rocq) complicates conversion with
  subtyping. Lean shows the prenex, non-cumulative design is workable at
  scale.

### 3.6 Inductive families and `case`

**Declarations.** An inductive family is declared as

```
(data (D.{ū} params…) : (Π (indices…) (Type ℓ))
  (c₁ : (Π (fields…) (D params idx₁…)))
  …)
```

Each constructor field carries a quantity. Fields default to ω; a field
declared 0 is not stored at runtime.

The kernel checks:
- **Uniform parameters.** Every constructor's result is `(D params …)` with
  the same parameters. Indices may vary.
- **Strict positivity.** `D` (and every type of a mutual block) occurs in a
  field type only as the final result of a Π, never to the left of an arrow
  and never as an argument of another type constructor. Nested occurrences
  such as `(List (Tree A))` are therefore rejected in v1 (§12). Indices of
  the occurrence may not mention `D`.
- **Universe levels.** Each field type lies in a universe ≤ `ℓ`. Parameter
  and index types may live in any universe; they do not count towards `ℓ`.
  There is no impredicativity, hence no restriction on large elimination.
- **Mutual blocks** are checked together.

**Dependent `case`.** `(case q s (ȳ z ⇒ P) (cₖ x̄ ⇒ mₖ)…)` eliminates one
layer of an inductive family:

```
Case    Γ₀ ⊢ s :σ' (D p̄ ī)                       σ' = 0 if σ·q = 0, else 1
        0Γ, ȳ :0 Ī, z :0 (D p̄ ȳ) ⊢ P :0 (Type ℓ)
        for every constructor cₖ with fields x̄ :ρ̄ F̄ and result indices īₖ:
          Γ₁, x̄ :(σ·q·ρ̄) F̄ ⊢ mₖ :σ P[īₖ/ȳ, (cₖ p̄ x̄)/z]
        ─────────────────────────────
        q·Γ₀ + Γ₁ ⊢ (case q s (ȳ z ⇒ P) (cₖ x̄ ⇒ mₖ)…) :σ P[ī/ȳ, s/z]
```

- **Branches share one context.** All branches use the same `Γ₁` (only one
  of them runs), so a linear resource must be consumed in every branch. A
  `case` with no branches (empty type) consumes everything vacuously.
- **Field quantities.** Each field is bound at the scrutinee's quantity times
  the field's declared quantity. A field declared 0 is unusable at runtime
  whatever the scrutinee. Casing on a linear scrutinee makes its fields
  linear (if declared 1) or unrestricted (if declared ω).
- **Erased case** (σ = 1, q = 0) is allowed only when `D` has at most one
  constructor and every field of that constructor is quantity 0. Examples:
  `=`, `Acc` with an erased field (§3.8), and empty types.
  - The scrutinee is never inspected: the case compiles to its single
    branch, or to `unreachable` for an empty type.
  - It is safe only together with rule E-total (§3.3), which guarantees that
    erased scrutinees come from total code.
  - With both rules the design stays inside the conditions studied in Abel,
    Danielsson & Eriksson (2023, 2026) and Danielsson (2026).
- **Index unification is the elaborator's job.** The kernel `case` only
  substitutes each constructor's indices into the motive (the Rocq-style
  `match … in … return` design). Matching a clause like `((cons x xs) ys)`
  against `(Vec A (succ n))` requires motive generalization with equations
  and a constructor-disjointness ("no-confusion") argument. Both are
  untrusted elaborator work (§6.3).

**Why `case` and case trees instead of eliminators.** An earlier sketch kept
only eliminators (recursors) in the kernel, as Lean does. That sits badly
with linearity:
- A recursor's step function may run many times, so its context is scaled by
  ω and cannot capture linear resources.
- An inductive hypothesis computed from a linear field consumes that field.

`case` with exact per-branch usage, plus checked recursion over case trees
(§3.8), keeps linearity precise. The cost is a small termination check in
the kernel. Eliminators remain definable in the library.

### 3.7 Equality and axiom K

Propositional equality is a library inductive family, registered as builtin
so the kernel can attach K to it:

```lisp
(data (= {A (Type u)} (x A)) : (Π ((y A)) (Type u))
  (refl : (= x x)))
(builtin Equality =)
```

`J` (path induction) is defined in the library with ordinary `case`.

**K** (uniqueness of identity proofs) is the kernel construct `case-K`:

```
K       Γ₀ ⊢ s :σ' (= {A} a a)                   σ' = 0 if σ·q = 0, else 1
        0Γ, z :0 (= {A} a a) ⊢ P :0 (Type ℓ)
        Γ₁ ⊢ m :σ P[refl/z]
        ─────────────────────────────
        q·Γ₀ + Γ₁ ⊢ (case-K q s (z ⇒ P) m) :σ P[s/z]

ι       (case-K q refl (z ⇒ P) m)  ≡  m
```

- **Difference from `case`.** Unlike `case`, the motive does not abstract the
  index, so a proof of `(= a a)` may be assumed to be `refl`.
- **Erasure.** `refl` has no fields, so `case-K` at q = 0 is an erased case:
  free at runtime, subject to E-total.
- **What K buys.** Dependent pattern matching elaborates by the
  straightforward algorithm of Goguen, McBride & McKinna (2006), and
  matching on equality proofs behaves as programmers expect.

Trade-offs:
- K is incompatible with univalence, which is a non-goal.
- **Without K:** keeps the univalence door open, at the price of a much
  harder pattern compiler (Cockx et al. 2014).
- **Observational equality:** gives function extensionality with decidable
  conversion (Altenkirch et al. 2007, Pujet & Tabareau 2022, 2025), at the
  cost of a larger kernel. The most likely future upgrade (§12).

### 3.8 Recursion and totality

**Case-tree bodies.** A recursive definition's body is a case tree:
- λs binding the parameters;
- then nested `case` (or `case-K`) splits whose scrutinees are parameters or
  pattern variables bound by earlier splits;
- ordinary terms at the leaves.

Recursive calls may occur only in leaves.

**Termination by lexicographic structural descent** (the *foetus* criterion,
Abel 1998; Abel & Altenkirch 2002). Each definition `f` in a mutual block
carries a **measure**: a sequence of its parameter positions. The elaborator
finds one by search, or the user gives it with `:measure`; the kernel only
verifies it. All measures in a block have the same length.

For every recursive call `(g ā)` in a leaf of `f`, the kernel builds the
relation between `g`'s measured arguments and `f`'s measured parameters:
- **`<`:** the argument is a variable bound as a recursive field by a split
  on that parameter or, transitively, on one of its strict subterms;
- **`=`:** the argument is that parameter itself, or the constructor pattern
  it was split into, e.g. `(succ m)` in the branch where the parameter was
  split by `succ` with field `m`;
- **`?`:** anything else.

The call is accepted if the relations read lexicographically are
`(=, …, =, <, …)`: some prefix equal and then one strict decrease. Arguments
after the decrease are unconstrained. The block terminates because every
call decreases one fixed lexicographic order.

What this accepts:
- **Ackermann directly:** measure `(m n)`, with calls `(< ?)`, `(< ?)` and
  `(= <)`.
- **Lifted local recursion:** captured parameters are passed unchanged, so
  they relate by `=`.
- **Mutual even/odd.**

The checker is purely syntactic: no size types and no reasoning through
nested inductives. It is the riskiest part of the trusted base (Rocq's guard
condition has had soundness bugs), so it stays this small.

**Well-founded recursion at zero runtime cost.**

```lisp
(data (Acc {A Type} (R (-> A A Type))) : (-> A Type)
  (acc : (Pi ({x A} (0 h (Pi ((y A) (0 _ (R y x))) (Acc R y)))) (Acc R x))))
```

- `Acc`'s single field is quantity 0, so a definition can recurse
  structurally on an *erased* accessibility proof, splitting it with an
  erased case.
- At runtime the proof and every split on it vanish, and only the real
  recursion remains.
- E-total guarantees that the erased proof is total, so the runtime
  recursion really terminates.

This replaces "a stronger termination checker in the kernel" for most
non-structural algorithms.

**Partial definitions.** Marked `partial`, these may recurse arbitrarily.
- They never unfold during conversion.
- They make any definition that runs them partial (§3.3).
- They may not appear in erased positions (E-total).

A type inhabited only via partial definitions proves nothing; `:total name`
in the REPL reports a definition's status. `main` and I/O loops are
typically partial.

**Default.** Total. A file may say `(default partial)` for scripting.

### 3.9 Primitive and builtin types

**Builtin registrations** connect library declarations to native support.
The kernel checks each registration against an expected shape before
accepting it:

| Registration | Expected shape | Gives |
|---|---|---|
| `(builtin Nat N)` | two constructors: nullary, then one recursive field | literals; kernel and runtime acceleration of `+ - * div mod = <` on literals (after Allais 2023 and Lean) |
| `(builtin Equality E)` | the family of §3.7 | `case-K` |
| `(builtin Unrestricted U)` | one constructor, one ω field | used in primitive types (§5.2) |
| `(builtin LinearPair P)` | one constructor: a quantity-1 field, then an ω field | used in primitive types (§5.3, §5.4) |
| `(builtin Unit T)`, `(builtin Bool B)` | one / two nullary constructors | results of primitive operations |

**Primitive types and operations** are opaque to the kernel and implemented
natively. Each has a declared type:

| Type | Representation | Notes |
|---|---|---|
| `Int64` | machine integer | wrapping arithmetic; comparisons return `Bool`; opaque to proofs except via declared lemmas |
| `Char`, `String` | Unicode scalar; immutable UTF-8 | conversions to and from `(List Char)` and `Nat` |
| `Array` | `(Array A n)`, heap array | unique-reference API only (§5.2–5.3) |
| `IArray` | `(IArray A n)`, immutable heap array | ordinary ω value |
| `World` | token, no constructors | created only by the runtime for `main` (§5.4) |

The kernel trusts:
- the declared types of primitives;
- the shape checks of builtins;
- the literal computation rules of accelerated operations.

This list is the deliberate, enumerated extension of the trusted base, and
there is no `believe_me`.

### 3.10 The trusted base

Soundness (no closed total proof of `Empty`, and no stuck or unsafe runtime
for total programs) depends on exactly:

1. the kernel: §3.3–3.8 as implemented;
2. the builtin shape checks and literal acceleration (§3.9);
3. the declared types of primitives (§3.9, §5);
4. the runtime's implementation of primitives and erasure (§7.1).

The elaborator, macros, pattern compiler, termination-measure search and
the standard library are outside it.

### 3.11 Metatheory status

No single published result covers exactly this combination: QTT with
σ ∈ {0, 1}, prenex universe polymorphism, inductive families, K, erased case
and lexicographic structural recursion. The pieces are established:

| Piece | Evidence |
|---|---|
| QTT core (Π, modes, erasure via realizability) | Atkey 2018; McBride 2016 |
| Graded dependent type theory with a universe, Π, Σ, ℕ, Empty, Unit; erasure soundness; conditions on erased matches | Abel, Danielsson & Eriksson 2023 and 2026 (formalized in Agda) |
| Transport along erased identity proofs, given no inconsistent erased postulates | Danielsson 2026 (formalized) |
| Inductive families with `match`/`fix`-style recursion and K | Dybjer 1994; Goguen, McBride & McKinna 2006; Rocq and Agda in practice |
| Lexicographic structural recursion | Abel 1998; Abel & Altenkirch 2002 |
| Universe-polymorphic, non-cumulative predicative hierarchy | Lean 4 in practice |
| QTT with inductive families, in practice | Idris 2 (with `Type : Type`) |

Plan:
- keep each kernel feature close to its established form;
- treat the combination as a risk, covered by the negative tests in §10;
- follow the verification plan in [`VERIFICATION.md`](VERIFICATION.md):
  differential testing against a Haskell reference kernel, re-checking the
  library in Lean through a translator, and a Lean mechanization of a core
  fragment (runtime safety, erasure, E-total, uniqueness).

### 3.12 Kernel budget

| Component | Estimated Rust lines |
|---|---|
| Core terms, levels, signature, builtin shape checks | 350 |
| NbE: evaluation, read-back, conversion, η, glued and case-tree unfolding | 450 |
| Type and quantity checking, E-total (§3.3) | 400 |
| Inductive declarations: positivity, universes, constructor types | 300 |
| `case`, `case-K`, case-tree validation, lexicographic termination | 350 |
| Primitives and literal acceleration | 200 |
| **Total** | **~2,050** |

---

## 4. A tour of the surface language

### Reader conventions

- **Files** are sequences of top-level forms; `;` comments; strings, chars
  (`#\a`) and natural-number literals as in grift.
- **Quantities** are written `0`, `1` and `ω` (ASCII alternative `*`).
- **Binders** appear only in binder lists, so they are unambiguous:
  - `(x A)`: explicit, quantity ω;
  - `(q x A)`: explicit with a quantity, e.g. `(1 w World)`;
  - `{x A}`: implicit, quantity 0;
  - `{q x A}`: implicit with a quantity, e.g. `{ω n Nat}` when the value is
    needed at runtime.
- **Types:**
  - `(Pi (binders…) B)`: dependent function type;
  - `(-> A B C)`: non-dependent, every argument ω;
  - `(-o A B)`: non-dependent with a linear first argument (`⊸`).

  Linear arrows have their own form because `(-> (1 A) B)` would be
  ambiguous with applying the literal `1`.
- **Terms:**
  - `(fn (x y) body)`: function;
  - `(f a b)`: application; `(f {A} a)` passes an implicit argument
    explicitly;
  - `(the T e)`: type ascription;
  - `(let ((x e) (1 y T e2)) body)`: local definitions;
  - `(match e (pattern body) …)`: pattern matching;
  - `?name` or `_`: a hole or metavariable.
- **Declarations:**
  - `(def name : T body)`: a value or a function without pattern matching;
  - `(defn name : T clause…)`: a definition by pattern matching. A clause is
    `((pattern…) body)`, one pattern per explicit argument; implicit
    arguments are bound automatically.
  - `(data …)`, `(mutual …)`, `(partial (defn …))`, `(opaque (def …))`,
    `(builtin …)`.
  - `:measure (m n)` after the type gives a termination measure explicitly.
- **Modules:** `(import path)`, `(module name …)`.

### Example

```lisp
;; Natural numbers: an ordinary inductive type, compiled to big integers.
(data Nat : Type
  (zero : Nat)
  (succ : (-> Nat Nat)))
(builtin Nat Nat)

(defn + : (-> Nat Nat Nat)
  ((zero n) n)
  (((succ m) n) (succ (+ m n))))

;; Length-indexed vectors. A is a parameter, the length an index; the
;; length field of cons has quantity 0 and is not stored at runtime.
(data (Vec {A Type}) : (-> Nat Type)
  (nil  : (Vec A zero))
  (cons : (Pi ({n Nat} (x A) (xs (Vec A n))) (Vec A (succ n)))))

;; The type states the length of the result; at runtime this is list append.
(defn append : (Pi ({A Type} {m Nat} {n Nat} (xs (Vec A m)) (ys (Vec A n)))
                   (Vec A (+ m n)))
  ((nil ys) ys)
  (((cons x xs) ys) (cons x (append xs ys))))

;; No empty case: the type rules it out, and coverage checking knows it.
(defn head : (Pi ({A Type} {n Nat} (v (Vec A (succ n)))) A)
  (((cons x _)) x))

;; Accepted by the lexicographic check with measure (m n).
(defn ack : (-> Nat Nat Nat)
  ((zero n) (succ n))
  (((succ m) zero) (ack m 1))
  (((succ m) (succ n)) (ack m (ack (succ m) n))))

;; A proof, by structural recursion.
(defn plus-zero : (Pi ((n Nat)) (= (+ n zero) n))
  ((zero) refl)
  (((succ m)) (cong succ (plus-zero m))))

;; An erased precondition: the proof costs nothing at runtime.
(defn safe-div : (Pi ((a Nat) (b Nat) (0 ok (Not (= b zero)))) Nat)
  ((a b _) (nat-div a b)))

;; `if` is a macro over match: a function would evaluate both branches
;; under strict evaluation.
(def abs-diff : (-> Nat Nat Nat)
  (fn (a b) (if (< a b) (- b a) (- a b))))
```

---

## 5. Quantities in practice

### 5.1 Erasure

Everything at quantity 0 disappears at runtime:
- type arguments and indices;
- proofs of preconditions;
- constructor fields declared 0, e.g. `cons`'s length or `Acc`'s field;
- erased `case` and `case-K` (§3.6, §3.7).

In addition, *every value whose type is a universe* is compiled to a
placeholder, whatever its quantity. Nothing can inspect a type at runtime
(there is no case on `Type`), so even an ω type argument carries no data.

**Erasure soundness**, meaning that running the erased program gives the
same result as the unerased one:
- **What is proven.** Atkey (2018) for the QTT core; Abel, Danielsson &
  Eriksson (2023, 2026) for a graded theory with Π, Σ, ℕ, Empty, Unit and a
  universe, with erased matches under conditions. Rule E-total enforces
  those conditions.
- **What is not.** No proof covers tally's full combination (§3.11).
- **Testing.** It is checked differentially on every test program (§10).

### 5.2 Linearity is not uniqueness

QTT linearity says how a function *uses* an argument; it does not by itself
say a value is *unique*. A quantity-ω variable may be passed to two different
linear functions (usage 1 + 1 = ω ⊑ ω). In-place update needs uniqueness, so
tally gets it from **abstract primitive types whose values are only ever
handed out through linear binders**, in the continuation style of Linear
Haskell and Idris 2:

```lisp
;; Unrestricted values: the field is ω, so a linear value cannot be put
;; inside. This is what stops a linear array escaping a with-array scope.
(data (Ur {A Type})
  (ur : (Pi ((x A)) (Ur A))))
(builtin Unrestricted Ur)

(primitive with-array
  : (Pi ({A Type} {R Type} (n Nat) (init A)
         (1 k (Pi ((1 arr (Array A n))) (Ur R))))
        R))
```

An `Array` can exist only as the linear argument of `k` or of the array
operations, each of which returns it again or consumes it. The typing rules
make it impossible to move it into an ω binding:
- `(ur arr)` would use `arr` at ω;
- passing it to an ω parameter would too.

So every `Array` value is unique by construction, and the runtime updates it
in place without checking. The same argument applies to `World`.

For ordinary data used linearly, uniqueness is *dynamic*: the reference
counter (§7.2) reuses a cell in place when its count is 1.

### 5.3 Linear arrays

```lisp
(data (LPair {A Type} {B Type})
  (lpair : (Pi ((1 a A) (b B)) (LPair A B))))
(builtin LinearPair LPair)

(primitive get    : (Pi ({A Type} {n Nat} (1 arr (Array A n)) (i (Fin n)))
                        (LPair (Array A n) A)))   ; the array comes back
(primitive set    : (Pi ({A Type} {n Nat} (1 arr (Array A n)) (i (Fin n)) (x A))
                        (Array A n)))
(primitive freeze : (Pi ({A Type} {n Nat} (1 arr (Array A n)))
                        (Ur (IArray A n))))       ; immutable, shareable
(primitive free   : (Pi ({A Type} {n Nat} (1 arr (Array A n))) Unit))
```

- Matching an `LPair` at quantity 1 gives a linear array and an unrestricted
  element.
- Indices are `(Fin n)`, so bounds checks are proved, not executed.
- There is no borrowing: reading returns the array. A library `with-get`
  helper hides the threading.

### 5.4 I/O

```lisp
(primitive World : Type)              ; no constructors
(def IO : (-> Type Type)
  (fn (A) (Pi ((1 w World)) (LPair World A))))
```

- `main : (IO Unit)` is the only place a `World` is created (by the runtime).
  `main` may be partial.
- `pure`, `bind` and the primitive operations (`print-line`, `read-line`,
  file access) are library and primitive code over this type.
- `do` notation is a syntax macro (§8.1):

```lisp
(def main : (IO Unit)
  (do (print-line "What is your name?")
      (<- name read-line)
      (print-line (string-append "Hello, " name))))
```

Algebraic effects with rows are a possible later layer (§12).

---

## 6. Elaboration

The elaborator turns surface syntax into kernel terms. It is untrusted, and
it contains most of the implementation effort.

### 6.1 Bidirectional checking

Two operations (Dunfield & Krishnaswami):
- **`check(Γ, e, A)`** handles `fn`, constructors, `match`, holes and
  literals against a known type;
- **`infer(Γ, e)`** handles variables, applications and annotations.

Top-level definitions require a type annotation, so checking always starts
from a known type.

### 6.2 Metavariables, implicit arguments, unification

Implicit arguments become fresh metavariables:
- when a constant with leading implicit binders is used, they are inserted;
- when checking against an implicit Π, a binder is inserted.

Unification works on NbE values:
- **Pattern unification** (Miller patterns, Abel & Pientka 2011), with
  pruning as in elaboration-zoo `05-pruning`.
- **Postponement:** constraints outside the pattern fragment wait until more
  is known.

Level metavariables are solved alongside. When elaboration of a definition
finishes, every metavariable must be solved; otherwise the definition fails
with the unsolved goals shown.

### 6.3 Pattern matching

Clauses compile to **case trees** by the algorithm of Cockx & Abel (2018):
- splitting on a constructor runs unification of indices (with K, §3.7) to
  refine the other patterns and the goal;
- absurd branches come from unification failure;
- **coverage checking** falls out of the algorithm: missing cases are
  reported with the uncovered patterns.

**Translation to the kernel.** The kernel `case` only substitutes indices,
so each split is translated in the style of Goguen, McBride & McKinna (2006):
1. **Generalize the motive** over the scrutinee's indices, with equations
   back to the actual indices ("elimination with a motive", McBride 2002).
2. **Split** with `case`.
3. **Discharge the equations in each branch:**
   - by `case`/`case-K` on the equation proofs;
   - by *no-confusion* lemmas for constructor injectivity and disjointness,
     which are generated per inductive type by the elaborator and are
     themselves ordinary kernel-checked definitions.

The equation proofs are quantity 0, so all of this erases. The resulting
case tree is the body the kernel checks (§3.8). The elaborator also searches
for a termination measure and records it.

### 6.4 Quantity defaults and checking

**Defaults.**
- implicit binders: 0;
- explicit binders: ω;
- **binders whose type is a universe: 0**, explicit or not, since types carry
  no runtime data (§5.1).

**Checking.** The elaborator computes actual usage and reports mismatches,
with source locations, before the kernel re-checks. Two common situations
get targeted help:
- If an implicit is used at runtime, the error suggests `{ω n Nat}`.
- If a linear function `(Π (1 x A) B)` is used where `(Π (ω x A) B)` is
  expected, the elaborator η-expands it to `(fn (x) (f x))`, which has the ω
  type. This replaces quantity subtyping (§3.2).

**Constructor fields** of indexed families: the elaborator suggests
quantity 0 where a field is determined by the indices (like `cons`'s `n`);
the declaration states it explicitly.

**Erasure inference** (Tejiščák 2020) is future work (§12).

### 6.5 Holes and errors

- **Holes.** `?name` or `_` in a term stops elaboration of that definition
  and reports the goal type and context, with quantities. The REPL can then
  ask `:goal`, `:type`, `:normalize`, `:total`.
- **Error messages** show glued (folded) terms, the expected and actual
  types, and the unification constraint that failed.
  - Termination errors show the offending call and its relation row.
  - E-total errors name the partial constant and the erased position.
  - Their quality is a first-class requirement, not polish.

### 6.6 Definitions and modules

- **Checking order.** A file is checked top to bottom; mutual blocks are
  checked together. Forward references outside a `mutual` block are errors:
  with dependent types, the type of a definition must be known before it is
  used.
- **Modules.** Modules are namespaces over files; `import` brings their
  public names into scope; names can be qualified (`Vec.append`).
- **No mutable top level.** Redefining a name in the REPL shadows it, as in
  grift. Earlier definitions keep referring to the old constant.

---

## 7. Compilation and runtime

### 7.1 Erasure and IR

After kernel checking, each definition is translated to an **untyped IR**:
- quantity-0 binders, arguments and fields are removed;
- values whose type is a universe become a placeholder;
- `case` becomes a switch on the constructor tag;
- erased `case`/`case-K` becomes its single branch, or `unreachable`.

The IR is in A-normal form (every intermediate result named) with explicit
closures. Local recursion was already lifted to top-level definitions, so no
closure refers to itself.

### 7.2 Memory management

**Precise reference counting with reuse**, as in Perceus (Reinking et al.
2021) and Lean 4 (Ullrich & de Moura 2019).

- **No cycles, so no cycle collector.** Three facts rule cycles out:
  - values are immutable once built;
  - closures never refer to themselves (recursion goes through top-level
    names);
  - `Array` values are unique and cannot be stored inside themselves
    (§5.2).

  Reference counting alone is therefore complete.
- **Reuse.** A cell whose count drops to zero can directly supply the memory
  for a new cell of the same size ("functional but in place").
- **Static uniqueness.** `Array` and `World` operations update in place with
  no count check, by the argument of §5.2.
- **Dynamic uniqueness.** For everything else, in-place reuse happens when
  the count is 1 at runtime.

The alternative is a tracing collector like grift's. It is simpler and
already available, but gives up deterministic freeing and reuse.

### 7.3 Data representation

- **Constructors** are a tag plus their runtime fields (after erasure).
- **Enumerations** (only nullary constructors after erasure) are machine
  integers, e.g. `Bool` as 0/1.
- **Newtypes** (one constructor with exactly one runtime field) are
  represented by that field, e.g. `Ur`.
- **Nat-like types.** After erasure these have one nullary constructor and
  one constructor with a single recursive runtime field. `Nat` and `Fin n`
  (whose bound is erased) both qualify, and they get the `Nat`
  representation: a small integer or a big integer, with `zero`/`succ` and
  matching compiled to integer operations (Allais 2023).
- **Unboxed values:** `Int64`, `Char`, `World` (no runtime content), type
  placeholders.
- **`String`, `Array` and `IArray`** are heap objects with reference counts.

### 7.4 Execution

- **Evaluation** is strict, call-by-value, with guaranteed tail calls.
- **First backend:** an IR interpreter, an explicit-loop machine in the
  style of grift's, so no Rust recursion is proportional to program depth.
- **Second backend:** C, with reference counting emitted inline (Koka and
  Lean style).
- **Type checking** uses the kernel's NbE evaluator.
- **Macros and stage-2 metaprograms** (§8) are compiled to IR and run on the
  interpreter. Running them through NbE would be orders of magnitude slower.

---

## 8. Metaprogramming

### 8.1 Stage 1: syntax macros

```lisp
(defsyntax do : (-> Sexp Sexp)
  ...)
```

- **What a macro is:** a tally function from s-expressions to
  s-expressions. It is elaborated, kernel-checked, compiled and run before
  elaboration of the code that uses it, so it must be defined earlier in the
  file or in an imported module.
- **Totality:** macros may be partial; one that diverges is stopped by a fuel
  limit and reported as a compile-time error.
- **Hygiene:** fresh names via a gensym primitive.
- **Uses:** `do`, `if`, `cond`-style sugar, record syntax, `deriving`-style
  boilerplate.

### 8.2 Stage 2: elaborator reflection

Following Christiansen & Brady (2016) and Lean 4:
- **The `Elab` monad.** Library code runs inside an `Elab` monad with access
  to the elaborator: read the current goal, create metavariables, unify,
  check, normalize, and emit definitions.
- **Typed terms.** Terms are exposed as a quoted, typed syntax tree.
- **Safety.** Results go through the kernel, so a metaprogram can fail but
  never produce an unsound definition.

This is tally's counterpart of grift's `$vau`: user code decides how syntax
becomes programs, but at elaboration time and under the kernel's control.
It enables tactics, derived instances, and proof automation written in tally.

---

## 9. Standard library

The prelude is written in tally, most of it total.

| Module | Contents |
|---|---|
| `Core` | `Unit`, `Empty`, `Bool` with `if` and `cond` (macros over `match`), `Not`, `Σ` / `Pair` / `LPair`, `Ur`, `Either`, `Maybe` |
| `Equality` | `=`, `refl`, `J`, `sym`, `trans`, `cong`, `subst`, `Dec`, decidable equality for builtins, no-confusion support |
| `Nat` | arithmetic, order, induction principles, `Fin`, `Acc` and well-founded recursion |
| `Data` | `List`, `Vec`, `All`, `Any`, `Elem`, sorting with a proof of sortedness |
| `Text` | `Char`, `String`, conversions |
| `Array` | `with-array`, `get`, `set`, `freeze`, `free`, `IArray`, in-place sorting |
| `IO` | `World`, `IO`, `pure`, `bind`, console and file primitives, `do` |
| `Meta` | `Sexp`, `defsyntax` support; later `Elab` |

---

## 10. Testing and validation

This section lists the test suites. [`VERIFICATION.md`](VERIFICATION.md)
places them in the wider assurance plan: reference kernels, translation to
Lean and Rocq, and mechanized proofs.

1. **Kernel acceptance tests.** Must typecheck:
   - polymorphic identity, Church numerals, Nat, Vec, `=` with `J` and K;
   - mutual even/odd, Ackermann, a lifted local recursion with captured
     parameters;
   - universe-polymorphic `List`;
   - well-founded recursion on an erased `Acc`.
2. **Kernel rejection tests.** Must be rejected:
   - **Paradoxes and positivity:**
     - Hurkens's paradox (requires `Type : Type`, which the kernel lacks);
     - a negative inductive such as
       `(data Bad (mk : (-> (-> Bad Empty) Bad)))`;
     - a nested occurrence such as `(mk : (List Bad))` (rejected in v1);
     - universe-level violations.
   - **Termination:**
     - non-lexicographic recursion outside `partial`;
     - a recursive call outside a case-tree leaf.
   - **Partiality:**
     - a total definition that runs a partial one;
     - **a partial constant in an erased position** (E-total), e.g. passing
       a `partial` "proof" of `(= Nat Bool)` to a function that transports
       along it.
   - **Quantities:**
     - linear variables used twice or dropped;
     - erased variables used at runtime;
     - erased `case` on a type with two constructors, or with a runtime
       field;
     - `(Π (1 x A) B)` given where `(Π (ω x A) B)` is expected, in the kernel
       without the elaborator's η-expansion;
     - builtin registrations with the wrong shape.
3. **Normalization tests.**
   - Normalizing open applications of recursive definitions terminates, e.g.
     `(ack m n)` with neutral `m`.
   - `(+ zero n)` reduces and `(+ n zero)` does not, as all-or-nothing
     unfolding implies.
   - `case-K` computes.
4. **The de Bruijn check.** Every definition the elaborator produces is
   re-checked by the kernel in tests and debug builds. A test mode also
   pretty-prints core terms and re-checks them from text.
5. **Properties of NbE.** Normalization is idempotent, conversion is
   symmetric and transitive, and reading back an evaluated normal form gives
   it back. Tested with property-based tests on generated well-typed terms.
6. **Erasure soundness, differentially.** Run each test program erased (IR
   interpreter) and unerased (NbE) and compare results.
7. **Language-level tests.** `.tl` files with expected outputs, plus
   expected-error files whose diagnostics are compared textually.
8. **Benchmarks.** Ported grift and Idris 2 benchmarks (fib, ackermann, list
   operations, in-place array sort) against the interpreter, and later the C
   backend.

---

## 11. Implementation plan

A Rust workspace; the kernel crate is `no_std` + `alloc`, since it needs no
I/O:

```
crates/tally-syntax    reader, s-expressions with spans, pretty-printer
crates/tally-kernel    core terms, levels, NbE, conversion, checking (§3)
crates/tally-elab      surface AST, elaborator, unification, pattern matching,
                       no-confusion generation, measure search, macros
crates/tally-compile   erasure, IR, reference-counting insertion
crates/tally-runtime   IR interpreter, heap, primitives, big integers
crates/tally-cli       REPL, file runner, :goal / :type / :normalize / :total
lib/                   the standard library in tally
tests/                 kernel tests, .tl tests, expected-error tests
```

Reuse from grift:
- the reader (s-expressions, no quote shorthand);
- the explicit-loop machine, as the model for the interpreter;
- a lesson: a library-first design needs early attention to performance, so
  §7's reference counting and data representation are planned from the
  start.

| Milestone | Contents | Exit criterion |
|---|---|---|
| **M1 Kernel core** | Core terms, levels, Π/λ/let, NbE, conversion, η | Polymorphic and Church-encoded examples check; Hurkens rejected |
| **M2 Inductives** | Declarations, positivity, `case`, `case-K`, builtins, case-tree definitions, lexicographic termination, partial/opaque | Nat, Vec, `=`, Fin, Acc, Ackermann; all §10 inductive and termination rejection tests pass |
| **M3 Quantities** | Usage checking, erased case, E-total | All quantity and E-total rejection tests pass; `append` checks with `n` erased |
| **M4 Elaborator** | Bidirectional checking, metavariables, implicit arguments, levels, holes, error messages, quantity defaults and η-expansion | The §4 tour elaborates, writing only the binders shown |
| **M5 Pattern matching** | Case-tree compilation, coverage, absurd clauses, motive generalization, no-confusion, measure search | `head` without an empty case; `ack` without `:measure`; coverage errors name the missing patterns |
| **M6 Runtime** | Erasure, IR, interpreter, reference counting with reuse, primitives, big-integer Nat-like types | Differential erasure tests pass; in-place array sort runs without copying |
| **M7 Library and REPL** | §9 prelude, I/O, `do`/`if` as built-in macros | Interactive programs run; library is total except marked I/O |
| **M8 Syntax macros** | `defsyntax`, gensym, compiled macro execution | `do`, `if` and record sugar move to the library |
| **M9 Elaborator reflection** | `Elab` monad | A tactic (e.g. `decide`) and a derived `DecEq` written in tally |
| **M10 C backend** | Optional | Benchmarks within 3× of Idris 2's Chez backend |

**Rough size.**

| Part | Estimated lines |
|---|---|
| Kernel | ~2.1k |
| Elaborator | 5–7k (pattern matching with motive generalization and no-confusion is a large share) |
| Compiler and runtime | 2–3k |
| Library (tally) | 2–3k |

The elaborator, its unification, the pattern compiler and error messages are
the schedule risk.

---

## 12. Open questions and future work

1. **Linear or affine.** Strict linearity guarantees consumption but makes
   error paths verbose. An affine variant (`0 ⊑ 1`), with dropped resources
   freed by reference counting, is easier to program with. Decide after M6
   with real code.
2. **Borrowing.** Reading a linear array returns it. Graded modalities
   (Granule) or a dedicated borrow quantity could fix this; both are
   research-level extensions of QTT.
3. **Quantity polymorphism,** e.g. `map` that is linear in its list when its
   function is. Needs quantity variables in the kernel; Idris 2 lacks it.
4. **Overloading.** Without type classes, `+` on `Nat`, `Int64` and
   rationals needs separate names. Candidates:
   - Agda-style instance arguments (an implicit filled by searching
     in-scope instances);
   - Lean-style tabled type-class resolution (Selsam, Ullrich & de Moura
     2020).

   Both are elaborator-only, so the kernel is unaffected. This is the most
   important usability question after M7.
5. **Erasure inference** (Tejiščák 2020).
6. **Observational equality** (Pujet & Tabareau 2022, 2025), for function
   extensionality and quotients. The most valuable kernel upgrade after v1;
   perhaps 500 kernel lines. Danielsson (2026) shows quotients with erased
   postulates also interact well with erasure.
7. **Without K,** if univalent reasoning becomes a goal. Requires the
   Cockx-style unifier.
8. **Termination beyond lexicographic structural recursion:**
   - size-change termination (Lee, Jones & Ben-Amram 2001) in the kernel;
   - or keep the kernel as it is and rely on erased-`Acc` well-founded
     recursion (current plan).
9. **Nested inductive types,** e.g. rose trees `(List (Tree A))` as a field.
   They need either positivity through parameters or translation to mutual
   inductives (Lean's approach).
10. **Effects:** algebraic effects with rows (Leijen 2017) on top of the
    world token, as typed counterparts of grift's prompts.
11. **Records with η,** and coinductive types / copatterns (Cockx & Abel
    2018).
12. **Mechanized metatheory** for the kernel fragment (§3.11), building on
    the Agda formalization of Abel, Danielsson & Eriksson.
13. **Self-hosting the elaborator** via stage-2 metaprogramming.
14. **The Data/Type split from BendTT** (Bend 2's core, see
    `reading/bendtt-affine-dependent-type-theory.pdf`).
    - **The idea:** give every type a kind, and allow only first-order
      `Data` types (no functions inside) to be copied by running code.
      Functions become at most affine at runtime.
    - **What it buys:** BendTT shows that with this restriction `Type : Type`
      and negative datatypes stay consistent, so tally could drop universe
      levels and the positivity check from the kernel. That would make it
      smaller.
    - **What it costs:** ω-quantity closures (higher-order programs that
      reuse a function) would need explicit copying or re-creation, and the
      consistency argument becomes BendTT's, which is new and not yet
      peer-reviewed.
    - **Decision:** keep the current design. Revisit after the mechanized
      fragment of VERIFICATION.md §5 exists, since the same Lean development
      could model both variants.

---

## 13. Comparison

| | tally | Idris 2 | Lean 4 | Agda | Rocq | grift |
|---|---|---|---|---|---|---|
| Core | MLTT + QTT | QTT | CIC variant, recursors | MLTT, case trees | CIC, `match`/`fix` | vau calculus (untyped) |
| Universes | predicative, prenex polymorphic | `Type : Type` | predicative + impredicative `Prop`, polymorphic | predicative, polymorphic | cumulative + `Prop`/`SProp` | none |
| Quantities / erasure | 0/1/ω in the kernel, E-total | 0/1/ω | erasure of `Prop` and types | `@0` annotations, erased matches | extraction | none |
| Recursion in the kernel | case trees, lexicographic structural | case trees; totality checked outside the kernel | recursors only | case trees + termination checker | guard condition | n/a |
| K / UIP | yes (`case-K`) | yes | definitional proof irrelevance | optional | no (`SProp` aside) | n/a |
| Metaprogramming | syntax macros → elaborator reflection | elaborator reflection | macros + `MetaM` | reflection | Ltac / Ltac2 / plugins | fexprs (`$vau`) |
| Syntax | s-expressions | Haskell-like | own | mixfix | own | s-expressions |
| Runtime | RC + reuse; static uniqueness for arrays | Chez Scheme (tracing GC) | RC + reuse | via Haskell/JS | extraction | tracing GC |

tally's niche: Idris 2's quantities, with a consistent universe hierarchy, an
explicit and small trusted base, and erasure made safe in the presence of
partiality, in a Lisp with a path to elaborator reflection.

---

## 14. References

Files are in `reading/`; its index says what each is for.

- **Quantities:** Atkey, *Syntax and Semantics of Quantitative Type Theory*
  (LICS 2018); McBride, *I Got Plenty o' Nuttin'* (2016); Brady, *Idris 2:
  Quantitative Type Theory in Practice* (ECOOP 2021). §3.2–3.3, §5.
- **Erasure:** Abel, Danielsson & Eriksson, *A Graded Modal Dependent Type
  Theory with a Universe and Erasure, Formalized* (ICFP 2023) and the
  extended *… with Erasure, Formalized* (arXiv 2603.29716, 2026);
  Danielsson, *Erased Postulates, Identity Types and Quotients* (arXiv
  2609.08578, 2026). §3.3 (E-total), §3.6, §3.11, §5.1.
- **Foundations:** Martin-Löf, *Intuitionistic Type Theory* (1984); Coquand &
  Huet, *The Calculus of Constructions* (1988); Barendregt, *Lambda Calculi
  with Types* (1992); Hurkens, *A Simplification of Girard's Paradox* (1995).
  §3, §3.5, §10.
- **Inductive families and pattern matching:** Dybjer, *Inductive Families*
  (1994); McBride, *Elimination with a Motive* (2002); Goguen, McBride &
  McKinna, *Eliminating Dependent Pattern Matching* (2006); Cockx, Devriese &
  Piessens, *Pattern Matching Without K* (2014); Cockx & Abel, *Elaborating
  Dependent (Co)pattern Matching* (2018); Cockx, PhD thesis (2017).
  §3.6–3.7, §6.3.
- **Termination:** Abel, *foetus — Termination Checker for Simple Functional
  Programs* (1998); Abel & Altenkirch, *A Predicative Analysis of Structural
  Recursion* (JFP 2002). §3.8.
- **Checking and elaboration:** Coquand, *An Algorithm for Type-Checking
  Dependent Types* (1996); Abel, *Normalization by Evaluation* (habilitation,
  2013); Christiansen, *Checking Dependent Types with NbE* (tutorial); Kovács,
  *elaboration-zoo*; Löh, McBride & Swierstra, *A Tutorial Implementation of
  a Dependently Typed Lambda Calculus* (2010); Dunfield & Krishnaswami,
  *Bidirectional Typing* (2021); Abel & Pientka, *Higher-Order Dynamic
  Pattern Unification* (2011); Gundry, PhD thesis (2013); Norell, PhD thesis
  (2007). §3.4, §6.
- **Metaprogramming:** Christiansen & Brady, *Elaborator Reflection* (ICFP
  2016); Brady, *Idris: Design and Implementation* (JFP 2013); de Moura &
  Ullrich, *The Lean 4 Theorem Prover and Programming Language* (2021). §8.
- **Equality:** Altenkirch, McBride & Swierstra, *Observational Equality,
  Now!* (2007); Pujet & Tabareau, *Observational Equality: Now for Good*
  (POPL 2022) and *Observational Equality Meets CIC* (2025). §3.7, §12.
- **Runtime:** Allais, *Builtin Types viewed as Inductive Families* (2023);
  Reinking, Xie, de Moura & Leijen, *Perceus* (2021); Ullrich & de Moura,
  *Counting Immutable Beans* (2019). §3.9, §7.
- **Linearity:** Wadler, *Linear Types Can Change the World!* (1990); Bernardy
  et al., *Linear Haskell* (2018); Orchard, Liepelt & Eades, *Granule* (ICFP
  2019); Moon, Eades & Orchard (ESOP 2021); Choudhury et al. (POPL 2021).
  §5, §12.
- **Effects:** Plotkin & Pretnar, *Handling Algebraic Effects* (2013); Leijen,
  *Type Directed Compilation of Row-Typed Algebraic Effects* (POPL 2017) and
  *Algebraic Effects for Functional Programming* (MSR TR 2016). §5.4, §12.
- **Overloading:** Selsam, Ullrich & de Moura, *Tabled Typeclass Resolution*
  (2020). §12.
- **Alternatives and background:** Chapman, Dagand, McBride & Morris, *The
  Gentle Art of Levitation* (2010), §3.6; The Univalent Foundations Program,
  *Homotopy Type Theory* (2013), §1 non-goals.
- **Not downloaded:** Lee, Jones & Ben-Amram, *The Size-Change Principle for
  Program Termination* (POPL 2001); Paulin-Mohring, *Inductive Definitions in
  the System Coq* (TLCA 1993); Tejiščák, *A Dependently Typed Calculus with
  Pattern Matching and Erasure Inference* (ICFP 2020); McBride, Goguen &
  McKinna, *A Few Constructions on Constructors* (TYPES 2004), the source of
  no-confusion. Links are in `reading/README.md`.

---

## 15. Glossary

| Term | Meaning |
|---|---|
| **Case tree** | λs, then nested `case` splits on variables, then ordinary terms at the leaves. |
| **E-total** | The rule that partial constants may not occur in erased positions of running code (§3.3). |
| **Erased position** | A term checked at mode 0 from inside a mode-1 term: an argument for a quantity-0 parameter, a quantity-0 `let`, or a quantity-0 scrutinee. |
| **Erased case** | A `case` or `case-K` on a quantity-0 scrutinee inside running code. It compiles to its single branch or to `unreachable`. |
| **Mode (σ)** | 0 when checking for types only, 1 when the term will run. |
| **Measure** | The sequence of parameter positions a recursive definition decreases lexicographically. |
| **No-confusion** | Lemmas stating that constructors are injective and distinct, used by the pattern compiler. |

---

## 16. Revision history

### 0.2 (2026-10-02): audit of 0.1

**Soundness.**
1. *Erased arguments built from partial definitions.* Erased case or
   transport could then crash, or run a value at the wrong type. **Fix:**
   rule E-total (§3.3); taint now flows through runtime positions only.
   Grounded in Danielsson 2026 and Abel et al. 2023/2026.
2. *Unfolding of recursive definitions.* The 0.1 rule ("unfold when the
   decreasing argument is a constructor") does not keep read-back
   terminating for lexicographic recursion. **Fix:** recursive definitions
   have case-tree bodies and unfold all-or-nothing (§3.4, §3.8).
3. *K was stated informally.* **Fix:** the `case-K` construct with its typing
   and ι rules (§3.7).
4. *Erasure soundness was overstated* as "a theorem for QTT". **Fix:** an
   accurate account of what is proven (§3.11, §5.1).
5. *"Linear values skip reference counting" was overstated,* since linearity
   is not uniqueness. **Fix:** static uniqueness only for abstract primitive
   types handed out through linear binders (§5.2, §7.2).

**Expressiveness.**
6. *Single-argument structural recursion plus lifting rejected Ackermann and
   lifted local recursion with captured parameters.* **Fix:** lexicographic
   measures verified by the kernel (§3.8).
7. *No well-founded recursion story beyond "in the library".* **Fix:**
   recursion on an erased `Acc` at zero runtime cost (§3.8).

**Specification gaps.**
8. Builtin registrations (Nat acceleration, equality for K, `Ur` and `LPair`
   in primitive types) were missing. **Fix:** §3.9, with shape checks.
9. How global definitions are checked (mode, context) was missing. **Fix:**
   §3.3.
10. Quantities in conversion were unspecified. **Fix:** compared exactly;
    the elaborator η-expands (§3.2, §6.4).
11. Universe levels of parameters and indices, and positivity through
    indices and nesting, were unspecified. **Fix:** §3.6.
12. The pattern compiler's need for motive generalization and no-confusion
    was unstated. **Fix:** §6.3, plus a larger elaborator estimate.
13. The trusted base was never listed. **Fix:** §3.10.

**Correctness and usability.**
14. `if` "as a function" evaluated both branches under strict evaluation.
    **Fix:** a macro over `match` (§4, §9).
15. `(-> (1 A) B)` was ambiguous with applying the literal `1`. **Fix:**
    `(-o A B)` for linear arrows, and quantities appear only in binders
    (§4).
16. Explicit binders of universe type defaulted to ω, keeping types at
    runtime. **Fix:** they default to 0, and every type value compiles to a
    placeholder (§5.1, §6.4, §7.1).
17. Runtime-needed implicits had no guidance. **Fix:** `{ω n Nat}` and a
    targeted error (§4, §6.4).
18. Macros ran on the NbE evaluator, which is far too slow. **Fix:** compiled
    to IR (§7.4, §8.1).
19. `Fin n` "as a machine integer" ignored large bounds. **Fix:** the general
    Nat-like representation rule, and newtype unboxing (§7.3).
20. Overloading was missing from the open questions. **Fix:** instance
    arguments or type classes, §12 item 4.
