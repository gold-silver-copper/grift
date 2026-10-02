# tally

A minimal Lisp with full dependent types and quantitative types: quantitative
type theory over Martin-Löf type theory with inductive families, a small
trusted kernel, and erasure plus linear in-place updates at runtime.

Nothing is implemented yet. This folder holds the design and the reading
material behind it.

- [`DESIGN.md`](DESIGN.md): the design document (core calculus, typing rules,
  quantities, elaboration, runtime, metaprogramming, implementation plan,
  open questions).
- [`VERIFICATION.md`](VERIFICATION.md): how to gain confidence in the core,
  from differential testing through translation to Lean and Rocq to
  mechanized proofs.
- [`PROMPT.md`](PROMPT.md): the prompt for exploring the design space: ten
  subagents, each researching and fully implementing a different type
  system with proofs in Haskell, Lean 4, Rocq, Bend 2 and Agda, in worktrees
  under `worktrees/`.
- [`scripts/install-proof-toolchains.sh`](scripts/install-proof-toolchains.sh):
  installs Haskell, Lean 4, Rocq + Equations, Agda + stdlib and Bend 2 on
  macOS, and writes a `TOOLCHAINS.md` report (`--help` for options).
- [`reading/`](reading/README.md): papers, theses and reference code, with an
  annotated index.
