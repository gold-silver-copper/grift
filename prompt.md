# Task: rewrite grift from scratch as a minimal, pure, type-strict, Kernel-inspired Lisp

You are working in /Users/kisaczka/Desktop/code/grift (Rust workspace, single crate `crates/grift`).
The current implementation on branch `pure` is a no_alloc, fixed-arena Kernel-inspired Lisp. Replace it
entirely with the language described below. Goals, in priority order:

1. A tiny primitive core in the spirit of Shutt's vau calculus: exactly one primitive operative.
2. No mutation of any kind: every value, binding, and environment is immutable once observable.
3. Strict runtime type discipline: no punning, no implicit conversion, typed errors everywhere,
   higher-order contracts with blame.
4. Everything derivable is written in grift itself, in the library.

## Git
- Create branch `vau-core` from `pure`. Do all work there. Do not push.
- Every commit must be authored exactly as `gold_silver_copper <stephen.korzen@gmail.com>`:
  `git -c user.name=gold_silver_copper -c user.email=stephen.korzen@gmail.com commit ...`
  No co-author trailers. Verify with `git log -1 --format='%an <%ae>'` after each commit.
- First commit: delete the old `crates/grift/src`, `prelude.grift`, old tests, and old example; exclude
  `grift-docs` from the workspace (leave the directory; it scrapes the old Rust source and is parked).
  Then commit once per phase below.

## References
- `05-07.pdf` (repo root): Revised⁻¹ Report on the Kernel Programming Language (R-1RK). Source of
  names and library derivations for every feature grift keeps. Where this prompt deviates, this prompt
  wins; many R-1RK derivations rely on mutable environments and must be re-derived.
- `jshutt.pdf` (repo root): Shutt's dissertation; the pure (mutation-free) vau calculus is the model.
- Dybvig, Peyton Jones, Sabry, "A Monadic Framework for Delimited Continuations" (JFP 2007): the
  control primitives below are its `newPrompt`/`pushPrompt`/`withSubCont`/`pushSubCont`.
- Kiselyov, Shan, Sabry, "Delimited Dynamic Binding" (ICFP 2006): dynamic variables are derived from
  delimited control in the library.
- Findler, Felleisen, "Contracts for Higher-Order Functions" (ICFP 2002): the contract library.
- Old `crates/grift/tests/lisp_tests.rs` on `pure`: only a source of test ideas; no compatibility goal.

## Purity invariant (normative)
No operation ever changes a value that has been observed. Pairs, strings, environments, combiners,
encapsulated values, and subcontinuations are immutable. There is no `set-car!`, no `$set!`, no
cells, no mutable environments, no memoizing promises. The single internal write is initialization of
a recursive binding (see `environment-bind-recursive`): reading a binding before it is initialized
raises `uninitialized-binding`, and attempting to initialize it a second time (e.g. by re-entering a
captured subcontinuation) raises `reinitialized-binding`, so no program can observe a binding changing
value. Side effects exist only in host-registered natives.

## Environments and definitions (pure)
- Environments are immutable values. Binding never modifies an environment; it returns a new child.
  - `(make-environment)` → a fresh empty environment with no parent.
  - `(environment-bind env ptree value)` → a new child of `env` with `ptree` matched against `value`.
  - `(environment-bind-recursive env ptree expr)` → creates a child `E` of `env` whose ptree symbols
    are uninitialized, evaluates `expr` in `E`, matches the result against `ptree` to initialize them,
    and returns `E` (letrec semantics; mutual recursion via a ptree like `(even? odd?)`).
  - `(eval expr env)`.
  - There is no way to enumerate an environment or obtain its parent.
- Declarations replace mutation. `(make-declaration f)` wraps an applicative `f : env → env`;
  `declaration?` recognizes them. A declaration is an opaque value; it only has effect when a *body*
  applies it.
- Bodies: the library's `$sequence` (and therefore every multi-form body: `$vau`, `$lambda`, `$let`, …)
  evaluates forms left to right; when a non-final form evaluates to a declaration, it applies it to
  the current environment and continues with the returned environment. The final form is evaluated in
  tail position and its value is the body's value.
- Top level: the host evaluates source as the same fold (each form in the current environment;
  declarations replace the current environment). `Grift::eval` and the REPL keep the resulting
  environment for the next call. Redefinition at the REPL shadows (new child); closures created earlier
  keep seeing the old binding (ML semantics). Library loading uses the same fold over the ground env.
- Library declaration forms (all user-extensible, since declarations are first-class):
  `$define!` (recursive: `(make-declaration (λ (env) (environment-bind-recursive env ptree expr)))`),
  `$import!`, `$define-record-type!`, `$define/contract!`. `$define!` also checks the sigil rule at
  definition time. Modules: `($module body ...)` evaluates a body and returns its final environment.

## Primitives (Rust) — the complete target set
Every primitive except `$vau` is an applicative.
- Operative: `$vau` (exactly ONE body expression; formal tree, environment formal or `#ignore`).
- Combiners: `wrap` (requires a combiner), `unwrap` (requires an applicative).
- Environments: `make-environment`, `environment-bind`, `environment-bind-recursive`, `eval`,
  `make-declaration`, `declaration?`.
- Choice: `(select b x y)` → `x` if `b` is `#t`, `y` if `#f`, type error otherwise.
  `$if` is library: `($vau (c t e) env (eval (select (eval c env) t e) env))` (tail position).
- Data: `cons`, `eq?`, `make-encapsulation-type` (returns constructor, predicate, accessor).
  `eq?` is defined on immediates (by value), symbols, environments, combiners, prompts,
  subcontinuations, and declarations (by identity). On pairs, strings, and encapsulated values it
  raises `type-error`: immutable structured data has no identity; use `equal?`.
- Type predicates (unary in Rust from one shared table-driven helper; the library defines n-ary
  versions under the same names): `pair?`, `symbol?`, `integer?`, `char?`, `string?`,
  `environment?`, `operative?`, `applicative?`, `prompt?`, `sub-continuation?`.
  (`boolean?`, `null?`, `inert?`, `ignore?` are library via `eq?`.)
- Delimited control (Dybvig–Peyton Jones–Sabry):
  - `(make-prompt)` → a fresh prompt.
  - `(push-prompt p thunk)` → installs prompt `p` and calls `thunk` under it.
  - `(with-sub-cont p f)` → captures the subcontinuation up to (not including) the innermost `p`,
    removes it *and* that prompt, and calls `f` with it in the continuation of that `push-prompt`
    (control0 semantics). No matching prompt → `no-prompt` error.
  - `(push-sub-cont k thunk)` → reinstates subcontinuation `k` on top of the current continuation and
    calls `thunk` in it. Subcontinuations are multi-shot and are not callable.
  - The value `error-prompt` (a pre-made prompt bound in the primitive env).
- Integers (binary, checked i64): `+`, `-`, `*`, `div`, `<?`. Overflow → `overflow`; zero divisor →
  `division-by-zero`. (`=?` is library via `eq?`; `mod` via `div`.)
- Text: `string->list`, `list->string`, `string->symbol`, `symbol->string`, `char->integer`,
  `integer->char`. You may add at most a few justified performance primitives (e.g. `string=?`,
  `string-length`); justify each in a comment.
- Host: `read` (string → datum), `write`, `display` (datum → string).

Anything else is library code. If something proves genuinely underivable, add it and justify it in
the final report.

## Errors
- Errors are an effect delivered through `error-prompt`. When a primitive fails, the machine behaves
  exactly as `(with-sub-cont error-prompt (λ (k) (list k kind message irritants)))`: `kind` is a symbol
  (`type-error`, `arity-error`, `unbound-symbol`, `uninitialized-binding`, `reinitialized-binding`,
  `overflow`, `division-by-zero`, `sigil-violation`, `no-prompt`, `match-failure`,
  `contract-violation`, `parse-error`, …), `message` a string, `irritants` a list. `k` lets handlers
  resume.
- If no `error-prompt` is installed, the error reaches the host as `Err(...)` with kind, message, and
  irritants rendered.
- Library: an `error-object` record type; `raise` and `error` (kind `user`) use the same protocol;
  `$guard` / `with-error-handler` push `error-prompt` with a thunk whose normal result is wrapped in an
  `ok` record, so normal returns and errors are distinguished without ambiguity. Errors inside a
  handler run outside that prompt and propagate outward.

## Type discipline (normative — document in the spec)
1. No punning or implicit conversion: `()` is not false; `select`/`$if`/`$cond`/`$and?`/`$or?`/`not?`
   require booleans; strings are not lists; chars are not one-char strings; integers never change type.
2. Every primitive checks arity and argument types and raises a typed error on mismatch.
3. Purity: see the purity invariant.
4. User-defined types come only from `make-encapsulation-type`; built-in types behave like pre-made
   encapsulation types (known only by their predicates). `equal?` on encapsulated values raises
   `type-error` (no structural equality across an abstraction barrier); records may opt in.
5. Sigil rule: (a) at call sites, when a combination's head is a symbol, a `$`-prefixed symbol must
   evaluate to an operative and any other symbol to a non-operative; (b) `$define!` enforces the same
   at definition time. Formal parameters are not checked (operatives may be passed as data). Precompute
   the "starts with $" flag in the symbol table. The whole library must obey the rule.
6. Higher-order contracts with blame (library): contracts are first-class values; flat contracts are
   predicates; combinators `->` (including rest arguments), `list-of`, `or/c`, `and/c`, `any/c`,
   `none/c`. Applying a function contract returns a new checked applicative (built with `wrap` + an
   operative) that checks arguments and results on every call. Violations raise `contract-violation`
   whose irritants identify the blamed party (the definition vs. the caller), the contract, the
   offending value, and the binding name. `$define/contract!` is a declaration.
7. Exhaustive matching: library `$match` over lists, immediates, and records; no matching clause →
   `match-failure`.

## Removed (relative to R-1RK)
- All mutation: `set-car!`, `set-cdr!`, `copy-es-immutable`, `encycle!`, `append!`, `$set!`,
  `$provide!` (replaced by `$module`), mutable environments, cells.
- Cyclic structures and all cycle-handling machinery (`get-list-metrics`, etc.); printer needs no labels.
- Numeric tower: only checked i64 integers. (Exact rationals as an optional library module built on
  encapsulation, with separate names: `make-rational`, `rational?`, `rational+`, …)
- Multi-parent environments.
- `call/cc`, `guard-continuation`, `extend-continuation`, `root-continuation`, `error-continuation`,
  `$let/cc`: replaced by delimited control.
- Keyed static variables. Keyed dynamic variables are replaced by library parameters (below).
- Promises/`$lazy`/`memoize` (memoization is mutation); thunks remain.
- `dynamic-wind`: with no mutation in the language there is nothing to restore; host resources are
  the host's responsibility.
- Ports. `$quote` and the `'` reader shorthand (as in Kernel).

## Library (grift)
- Ordered files in `crates/grift/lib/`, embedded with `include_str!` and loaded eagerly into the ground
  env via the top-level fold:
  - `00-boot.grift`: bootstrap `$define!` from primitives (the first forms must build declarations
    directly from `make-declaration`/`environment-bind-recursive`, obtaining symbols with
    `(($vau (x) #ignore x) name)`), then `list`, `list*`, `car`, `cdr` (via ptrees), `$if`,
    `$sequence` (declaration-aware), multi-body `$vau`, `$lambda`.
  - `10-control.grift`: `$cond`, `$let`, `$let*`, `$letrec`, `$letrec*`, `$and?`, `$or?`, `and?`,
    `or?`, `not?`, `apply`, `$match`.
  - `20-env.grift`: `get-current-environment`, `make-kernel-standard-environment`, `$let-safe`,
    `$let-redirect`, `$remote-eval`, `$bindings->environment`, `$binds?` (via `$guard`), `$module`,
    `$import!`.
  - `30-list.grift`: `length`, `list-tail`, `list-ref`, `append`, `map`, `for-each`, `filter`, `reduce`,
    `fold-left`, `fold-right`, `assoc`, `assq`, `member?`, `memq?`, `equal?`, `combiner?`, n-ary predicates.
  - `40-integer.grift`: n-ary `+ - *`, `=?`, `<?`, `<=?`, `>?`, `>=?`, `mod`, `div-and-mod`, `zero?`,
    `positive?`, `negative?`, `odd?`, `even?`, `abs`, `min`, `max`, `gcd`, `lcm`, `expt`.
  - `50-control.grift`: `$reset`/`shift` style helpers derived from the four primitives, generators,
    error records, `raise`, `error`, `$guard`, `with-error-handler`, parameters (`make-parameter`,
    `$parameterize`) via Kiselyov–Shan–Sabry delimited dynamic binding.
  - `60-effects.grift`: algebraic effect handlers (`perform`, `$handle`, deep handlers) on top of
    multi-prompt control.
  - `70-contract.grift`: contracts, `$define/contract!`, `$define-record-type!` (with typed
    constructor/predicate/accessors and optional equality).
  - `80-string.grift`: string/char library (comparison, append, substring, etc.) over the text
    primitives.
  - `90-rational.grift`: optional rationals.
- Comment each definition; cite R-1RK or the paper it derives from.

## Implementation constraints
- `#![no_std]` + `extern crate alloc` (Vec/Box/BTreeMap allowed; std only in tests/REPL binary).
  `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, edition 2024, MSRV 1.85. Must build for
  `thumbv6m-none-eabi`. No dependencies in the core crate. `cargo clippy --workspace -- -D warnings`
  and `cargo fmt --check` must pass.
- Representation:
  - `Value`: small `Copy` enum (target 16 bytes). Immediates: `Nil`, `Bool`, `Inert`, `Ignore`,
    `Int(i64)`, `Char(char)`, `Sym(SymId)`, `Prim(PrimId)`. Everything else is a `u32` handle.
  - Heap: `Vec<Slot>` with free list and mark bits. Objects: Pair, String(Box<str>), Environment
    { frame, parent: Option } (frames immutable after construction except the internal
    uninitialized→initialized transition), Operative { ptree, eparam, body, static_env },
    Applicative(Value), Encapsulation { type_id, value }, Declaration(Value), Prompt, continuation
    frames, SubContinuation.
  - Symbols interned in a separate table (with the `$` flag), never collected.
  - Because environments are immutable, symbol lookups may be cached; do so if it measurably helps.
- Evaluator: explicit machine loop; no Rust recursion proportional to program depth (remove the
  `RUST_MIN_STACK` env and `opt-level=1` workarounds). Continuation frames are heap objects linked by
  parent pointers; prompts are marker frames. `with-sub-cont` captures the frame segment up to the
  prompt; `push-sub-cont` reinstates it (copying the segment is acceptable; it must be multi-shot
  safe). Proper tail calls: a tail call pushes no frame, including `eval` in tail position of an
  operative body, so library `$if`, `$sequence`, `$cond`, `$let`, `$and?`, `$match` etc. are properly
  tail-recursive. Test with ≥1,000,000-iteration loops through library forms.
- Formal parameter trees validated when an operative is created and on `environment-bind*`
  (only symbols, `#ignore`, `()`, pairs; no duplicates). Applicative operands evaluated left to right.
- GC: mark-and-sweep, only at safe points in the machine loop, where everything live is reachable from
  (control, env, current continuation, primitive/ground envs, host-pinned roots). Rust primitives never
  observe a collection and need no rooting. Trigger on heap growth (e.g. k× since last collection).

## Reader / printer
- Reader: integers (decimal, optional sign), strings with escapes, chars (`#\a`, `#\space`,
  `#\newline`, `#\x41`), symbols, `#t #f #inert #ignore`, lists and dotted pairs, comments `;`, `#;`,
  `#| |#`. No `'`. Parse errors carry line/column.
- Printer: `write` and `display` modes; opaque objects as `#[operative]`, `#[applicative]`,
  `#[environment]`, `#[prompt]`, `#[sub-continuation]`, `#[declaration]`, `#[encapsulation]`.

## Embedding API
- `Grift::new()` (boots library), `Clone` for cheap copies of a booted interpreter, `eval(&mut self,
  src: &str) -> Result<Value, Error>` (top-level fold; the resulting environment persists across
  calls), write/display a value to String, `register_native(name, fn(&mut Ctx, &[Value]) ->
  Result<Value, Error>)` exposing natives as applicatives (unsigiled names). Native errors are
  delivered through the error protocol. Values given to the host are valid until the next eval unless
  pinned (`pin`/`unpin`).
- REPL binary behind the `repl` feature (rustyline), file-runner mode, prompt `Λ> `.

## Tests
- Grift-level tests in `crates/grift/tests/grift/*.grift` run by a Rust harness; `assert-equal` and
  `assert-error` (by kind, via `$guard`) written in grift. Cover: ptrees; immutable environments,
  declarations in bodies and at top level, shadowing semantics, recursive and mutual recursive
  `$define!`, `uninitialized-binding` and `reinitialized-binding`; wrap/unwrap/eval; every library
  combiner; every type-discipline rule (each violation raises the right kind, including the sigil
  rule at call and definition sites, `eq?` on structured data, `equal?` across abstraction barriers);
  tail calls through library forms; delimited control (nested and same-tag prompts, multi-shot
  resumption, `no-prompt`), shift/reset derivations, generators, effect handlers, parameters;
  errors (nested guards, errors in handlers, resumption via `k`); higher-order contracts with correct
  blame on both sides; records and `$match` exhaustiveness; integer overflow/division errors;
  rationals; strings/chars; GC under sustained allocation with bounded heap.
- A few Rust tests for the embedding API, natives, pinning, and the top-level fold.
- Rewrite `examples/fib_bench.rs` for the new API; report its numbers vs the old `pure` branch.

## Phases (commit after each, all tests green)
1. Delete old code; skeleton; Value, heap, symbols, reader, printer.
2. Machine evaluator, `$vau`, combiners, immutable environments, declarations, top-level fold,
   proper tail calls, sigil check.
3. Library boot, control, env, list, integer modules.
4. GC.
5. Delimited control, error protocol, control/effects library, contracts, records, strings, rationals.
6. Tests expanded, benchmark, REPL.
7. Docs: rewrite README.md, docs/architecture/ARCHITECTURE.md, docs/architecture/LANGUAGE_SPECIFICATION.md
   (primitive-vs-library table, purity invariant, type-discipline rules, control and error protocol,
   every deviation from R-1RK with rationale). Update `.github/workflows/ci.yml` (no-std check:
   `-p grift` on thumbv6m-none-eabi) and `.github/copilot-instructions.md` (alloc allowed; arena rule
   gone; purity and type-discipline rules).

When finished, report: the final Rust primitive list with justification for any addition, every
deviation from R-1RK, benchmark numbers, and open questions.
