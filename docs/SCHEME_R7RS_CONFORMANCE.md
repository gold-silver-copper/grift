# Scheme R7RS Conformance Status

This document tracks the R7RS conformance status of the Grift Scheme implementation. Grift is a `no_std`, `no_alloc` Scheme built on a custom arena allocator, targeting embedded systems, WebAssembly, and other constrained environments.

> **Last updated**: February 2026

## Reference Specification

The authoritative R7RS specification is located at:
- **`r7rs-spec.html`** — The complete Revised⁷ Report on the Algorithmic Language Scheme

---

## Conformance Summary

| R7RS Area | Status | Notes |
|-----------|--------|-------|
| Lexical conventions (§2) | ✅ Mostly complete | Identifiers, booleans, numbers (integers only), characters, strings, vectors |
| Basic concepts (§3) | ✅ Complete | Lexical scoping, tail-call optimization, strict evaluation |
| Expressions (§4) | ✅ Mostly complete | All primitive and most derived expression types; includes `define-record-type`, `guard`, `parameterize` |
| Program structure (§5) | ⚠️ Partial | `define`, `define-syntax`, `define-record-type` work; no library/module system |
| Standard procedures (§6) | ⚠️ Partial | Strong coverage for lists, strings, chars, vectors; exception system complete; gaps in I/O, numeric tower |
| Formal syntax (§7) | ✅ Mostly complete | Parser handles R7RS syntax with minor gaps |
| Standard libraries (Appendix A) | ❌ Not implemented | No `define-library` / `import` / `export` |

---

## §2 — Lexical Conventions

| Feature | Status | Details |
|---------|--------|---------|
| Identifiers | ✅ | Standard identifiers, `+`, `-`, `...`, etc. |
| Boolean literals `#t` / `#f` | ✅ | Also `#true` / `#false` forms |
| Integer literals | ✅ | Decimal integers (`isize`); no radix prefixes `#b`, `#o`, `#x` for numbers |
| Floating-point literals | ❌ | Not supported — integers only |
| Character literals | ✅ | `#\a`, `#\space`, `#\newline`, `#\tab`, `#\return`, `#\null`, `#\alarm`, `#\backspace`, `#\delete`, `#\escape`, `#\x41` (hex) |
| String literals | ✅ | Escape sequences: `\n`, `\t`, `\r`, `\"`, `\\`, `\a`, `\b`, `\|`, `\x41;` (hex), line continuation |
| Vector literals `#(...)` | ✅ | Parsed at read time |
| Bytevector literals `#u8(...)` | ❌ | Not supported |
| Comments `;` | ✅ | Line comments |
| Datum comments `#;` | ✅ | Parser skips next datum |
| Block comments `#| ... |#` | ✅ | Nestable block comments |
| `#!fold-case` / `#!no-fold-case` | ❌ | Not supported |

---

## §3 — Basic Concepts

| Feature | Status | Details |
|---------|--------|---------|
| Variables and binding | ✅ | Lexical scoping with closures |
| Proper tail recursion | ✅ | Full TCO via trampolined evaluator |
| Strict evaluation | ✅ | Call-by-value semantics |
| Only `#f` is false | ✅ | `'()`, `0`, `nil` are all truthy |
| Unspecified values | ✅ | `#<void>` returned for side-effect forms |

---

## §4 — Expressions

### §4.1 — Primitive Expression Types

| Form | Status | Implementation |
|------|--------|---------------|
| `quote` | ✅ | Special form |
| `lambda` | ✅ | Special form; supports rest args `(lambda (a b . rest) ...)` |
| `if` | ✅ | Special form (also macro-expanded variant in macros.scm) |
| Assignment `set!` | ✅ | Special form |
| `include` / `include-ci` | ❌ | Not implemented |

### §4.2 — Derived Expression Types

| Form | Status | Implementation |
|------|--------|---------------|
| `cond` | ✅ | Macro (macros.scm) |
| `case` | ✅ | Macro (macros.scm) |
| `and` | ✅ | Macro (macros.scm); short-circuit |
| `or` | ✅ | Macro (macros.scm); short-circuit |
| `when` | ✅ | Macro (macros.scm) |
| `unless` | ✅ | Macro (macros.scm) |
| `cond-expand` | ✅ | Macro; features: `r7rs`, `grift`, `exact-closed`; compound: `and`, `or`, `not`; `(library ...)` → `#f` |
| `let` | ✅ | Macro (macros.scm); including named `let` |
| `let*` | ✅ | Macro (macros.scm) |
| `letrec` | ✅ | Macro (macros.scm) |
| `letrec*` | ✅ | Macro (macros.scm) |
| `let-values` | ✅ | Macro (macros.scm) |
| `let*-values` | ✅ | Macro (macros.scm) |
| `begin` | ✅ | Special form |
| `do` | ✅ | Macro (macros.scm); full R7RS syntax |
| `delay` | ✅ | Macro (macros.scm) |
| `delay-force` | ✅ | Macro (macros.scm) |
| `force` | ✅ | Macro (macros.scm) |
| `make-promise` | ✅ | Stdlib function |
| `promise?` | ✅ | Stdlib function (returns `#t` for procedures) |
| `case-lambda` | ✅ | Macro (macros.scm); multi-arity dispatch |
| `define` | ✅ | Special form; variable and function shorthand |
| `define-values` | ✅ | Macro (macros.scm); supports 0–4 variables explicitly |
| `define-syntax` | ✅ | Special form |
| `let-syntax` | ✅ | Special form |
| `letrec-syntax` | ✅ | Special form |
| `syntax-rules` | ✅ | Macro; pattern-based with ellipsis support |
| `syntax-error` | ✅ | Special form; raises compile-time/macro-expansion error |
| `define-record-type` | ✅ | Special form; records represented as tagged vectors (max 32 fields) |
| `guard` | ✅ | Macro (macros.scm); full implementation using `with-exception-handler` |
| `make-parameter` | ✅ | Stdlib function; creates parameter objects (R7RS §4.2.6) |
| `parameterize` | ✅ | Macro (macros.scm); uses `dynamic-wind` for safe restore |
| `quasiquote` / `unquote` / `unquote-splicing` | ✅ | Special form + macro variant |

---

## §5 — Program Structure

| Feature | Status | Details |
|---------|--------|---------|
| Top-level `define` | ✅ | Variables and functions |
| Top-level `define-syntax` | ✅ | Macro definitions |
| `define-library` | ❌ | No module/library system |
| `import` / `export` | ❌ | No module/library system |
| `include` / `include-ci` | ❌ | Not implemented |
| REPL interaction | ✅ | `grift_repl` crate with `std` support |

---

## §6 — Standard Procedures

### §6.1 — Equivalence Predicates

| Procedure | Status | Implementation |
|-----------|--------|---------------|
| `eqv?` | ✅ | Builtin |
| `eq?` | ✅ | Builtin |
| `equal?` | ✅ | Builtin |

### §6.2 — Numbers

**Numeric tower**: Grift supports **exact integers only** (`isize`). No floating-point, complex, or rational numbers. This is intentional for `no_std`/`no_alloc` constraints.

| Procedure | Status | Notes |
|-----------|--------|-------|
| `number?` | ✅ | Builtin |
| `complex?` / `real?` / `rational?` | ❌ | Not implemented |
| `integer?` | ✅ | Builtin |
| `exact?` | ✅ | Always returns `#t` |
| `inexact?` | ✅ | Always returns `#f` |
| `exact-integer?` | ✅ | Builtin |
| `=`, `<`, `>`, `<=`, `>=` | ✅ | Builtins; integer comparison |
| `zero?`, `positive?`, `negative?`, `odd?`, `even?` | ✅ | Builtins |
| `max`, `min` | ✅ | Builtins |
| `+`, `-`, `*`, `/` | ✅ | Builtins; `/` is integer division |
| `abs` | ✅ | Builtin |
| `floor`, `ceiling`, `truncate`, `round` | ✅ | Builtins (identity for integers) |
| `floor/`, `floor-quotient`, `floor-remainder` | ❌ | Not implemented |
| `truncate/`, `truncate-quotient`, `truncate-remainder` | ❌ | Not implemented |
| `quotient`, `remainder`, `modulo` | ✅ | Builtins (R5RS-style integer division) |
| `gcd`, `lcm` | ✅ | Builtins |
| `expt` | ✅ | Builtin (integer exponentiation) |
| `square` | ✅ | Builtin |
| `sqrt` | ✅ | Stdlib (integer square root via Newton-Raphson) |
| `exact->inexact` / `inexact->exact` | ❌ | Not applicable (integers only) |
| `number->string` | ✅ | Builtin (integer to decimal string) |
| `string->number` | ✅ | Builtin (decimal string to integer, returns `#f` if invalid) |
| Radix prefixes `#b`, `#o`, `#x`, `#d` | ❌ | Not supported |
| Exactness prefixes `#e`, `#i` | ❌ | Not supported |

### §6.3 — Booleans

| Procedure | Status | Notes |
|-----------|--------|-------|
| `not` | ✅ | Builtin |
| `boolean?` | ✅ | Builtin |
| `boolean=?` | ⚠️ | Available as `boolean-eq` in stdlib (non-standard name) |

### §6.4 — Pairs and Lists

| Procedure | Status | Implementation |
|-----------|--------|---------------|
| `pair?` | ✅ | Builtin |
| `cons` | ✅ | Builtin |
| `car` / `cdr` | ✅ | Builtins |
| `set-car!` / `set-cdr!` | ✅ | Builtins |
| `caar` through `cddddr` | ✅ | Stdlib (full set up to 4 levels) |
| `null?` | ✅ | Builtin |
| `list?` | ✅ | Stdlib |
| `make-list` | ✅ | Stdlib |
| `list` | ✅ | Builtin |
| `length` | ✅ | Stdlib |
| `append` | ✅ | Macro (variadic, uses `append-two` from stdlib) |
| `reverse` | ✅ | Stdlib |
| `list-tail` | ✅ | Stdlib |
| `list-ref` | ✅ | Stdlib |
| `list-set!` | ✅ | Stdlib |
| `list-copy` | ✅ | Stdlib |
| `memq` / `memv` / `member` | ✅ | Stdlib |
| `assq` / `assv` / `assoc` | ✅ | Stdlib |
| `map` | ✅ | Stdlib (single list only; R7RS multi-list variant not supported) |
| `for-each` | ✅ | Stdlib (single list only) |

### §6.5 — Symbols

| Procedure | Status | Notes |
|-----------|--------|-------|
| `symbol?` | ✅ | Builtin |
| `symbol=?` | ❌ | Not implemented (use `eq?` on symbols instead) |
| `symbol->string` | ✅ | Builtin |
| `string->symbol` | ✅ | Builtin |

### §6.6 — Characters

| Procedure | Status | Implementation |
|-----------|--------|---------------|
| `char?` | ✅ | Builtin |
| `char=?`, `char<?`, `char>?`, `char<=?`, `char>=?` | ✅ | Builtins |
| `char-ci=?`, `char-ci<?`, `char-ci>?`, `char-ci<=?`, `char-ci>=?` | ✅ | Stdlib |
| `char-alphabetic?`, `char-numeric?`, `char-whitespace?` | ✅ | Stdlib |
| `char-upper-case?`, `char-lower-case?` | ✅ | Stdlib |
| `digit-value` | ✅ | Stdlib |
| `char->integer` / `integer->char` | ✅ | Builtins |
| `char-upcase` / `char-downcase` / `char-foldcase` | ✅ | Builtins (`char-foldcase` in stdlib) |

### §6.7 — Strings

| Procedure | Status | Implementation |
|-----------|--------|---------------|
| `string?` | ✅ | Builtin |
| `make-string` | ✅ | Builtin |
| `string` (constructor) | ✅ | Builtin |
| `string-length` | ✅ | Builtin |
| `string-ref` / `string-set!` | ✅ | Builtins |
| `string=?`, `string<?`, `string>?`, `string<=?`, `string>=?` | ✅ | Builtins |
| `string-ci=?`, `string-ci<?`, `string-ci>?`, `string-ci<=?`, `string-ci>=?` | ⚠️ | Only `string-ci=?` in stdlib; others missing |
| `string-upcase` / `string-downcase` / `string-foldcase` | ✅ | Stdlib |
| `substring` | ✅ | Builtin |
| `string-append` | ✅ | Builtin |
| `string->list` / `list->string` | ✅ | Builtins |
| `string-copy` | ✅ | Builtin |
| `string-copy!` | ❌ | Not implemented |
| `string-fill!` | ❌ | Not implemented |
| `string-map` / `string-for-each` | ✅ | Stdlib |
| `number->string` / `string->number` | ✅ | Builtins |

### §6.8 — Vectors

| Procedure | Status | Implementation |
|-----------|--------|---------------|
| `vector?` | ✅ | Builtin |
| `make-vector` | ✅ | Builtin |
| `vector` (constructor) | ✅ | Builtin |
| `vector-length` | ✅ | Builtin |
| `vector-ref` / `vector-set!` | ✅ | Builtins |
| `vector->list` / `list->vector` | ✅ | Builtins |
| `vector-fill!` | ✅ | Builtin |
| `vector-copy` | ✅ | Builtin |
| `vector-copy!` | ❌ | Not implemented |
| `vector-append` | ❌ | Not implemented |
| `vector-map` / `vector-for-each` | ❌ | Not implemented |
| Vector literal `#(...)` | ✅ | Parser support |

### §6.9 — Bytevectors

Not implemented. No bytevector types, literals, or operations.

### §6.10 — Control Features

| Procedure | Status | Implementation |
|-----------|--------|---------------|
| `procedure?` | ✅ | Builtin |
| `apply` | ✅ | Special form |
| `map` | ✅ | Stdlib (single list only) |
| `for-each` | ✅ | Stdlib (single list only) |
| `string-map` / `string-for-each` | ✅ | Stdlib |
| `vector-map` / `vector-for-each` | ❌ | Not implemented |
| `call-with-current-continuation` / `call/cc` | ✅ | Special form |
| `values` | ✅ | Special form |
| `call-with-values` | ✅ | Special form |
| `dynamic-wind` | ✅ | Special form |
| `eval` | ✅ | Special form |

### §6.11 — Exceptions

| Procedure | Status | Notes |
|-----------|--------|-------|
| `with-exception-handler` | ✅ | Special form; installs exception handler during thunk evaluation |
| `raise` | ✅ | Special form; raises a non-continuable exception |
| `raise-continuable` | ✅ | Special form; raises a continuable exception |
| `error` | ✅ | Builtin; supports message and irritant arguments, creates error objects |
| `error-object?` | ✅ | Builtin |
| `error-object-message` | ✅ | Builtin |
| `error-object-irritants` | ✅ | Builtin |
| `error-object-type` | ✅ | Builtin |
| `guard` | ✅ | Macro (macros.scm); full implementation using `with-exception-handler` |

### §6.12 — Environments and Evaluation

| Procedure | Status | Notes |
|-----------|--------|-------|
| `eval` | ✅ | Special form |
| `environment` | ❌ | Not implemented |
| `scheme-report-environment` | ❌ | Not implemented |
| `null-environment` | ❌ | Not implemented |
| `interaction-environment` | ❌ | Not implemented |

### §6.13 — Input and Output

| Procedure | Status | Notes |
|-----------|--------|-------|
| `display` | ✅ | Builtin (via output callback) |
| `newline` | ✅ | Builtin (via output callback) |
| `write` | ✅ | Builtin; machine-readable output with quotes |
| `write-shared` / `write-simple` | ❌ | Not implemented |
| `read` | ❌ | Not implemented (parser exists but not exposed as Scheme procedure) |
| Port types and predicates | ❌ | Not implemented |
| `current-input-port` / `current-output-port` / `current-error-port` | ❌ | Not implemented |
| `open-input-string` / `open-output-string` / `get-output-string` | ❌ | Not implemented |
| `write-char` | ✅ | Builtin; writes a character to a port |
| `read-char` / `peek-char` | ❌ | Not implemented |
| `read-line` / `read-string` | ❌ | Not implemented |
| File I/O (`open-input-file`, etc.) | ❌ | Not implemented |

### §6.14 — System Interface

| Procedure | Status | Notes |
|-----------|--------|-------|
| `load` | ❌ | Not implemented |
| `file-exists?` | ❌ | Not implemented |
| `delete-file` | ❌ | Not implemented |
| `command-line` | ❌ | Not implemented |
| `exit` / `emergency-exit` | ❌ | Not implemented |
| `get-environment-variable` / `get-environment-variables` | ❌ | Not implemented |
| `current-second` / `current-jiffy` / `jiffies-per-second` | ❌ | Not implemented |
| `features` | ❌ | Not implemented (but `cond-expand` recognizes features) |

---

## Hygienic Macro System

Grift implements a comprehensive hygienic macro system using mark-based hygiene (Clinger & Rees 1991).

### R7RS Standard (§4.3)

| Feature | Status | Notes |
|---------|--------|-------|
| `define-syntax` | ✅ | Top-level syntax definitions |
| `let-syntax` | ✅ | Local syntax bindings |
| `letrec-syntax` | ✅ | Recursive local syntax bindings |
| `syntax-rules` | ✅ | Pattern-based macros with ellipsis support |
| `syntax-error` | ✅ | Special form; raises compile-time/macro-expansion error |

### Beyond R7RS (Procedural Macros)

These extensions from R6RS / Chez Scheme are also supported:

| Feature | Status | Notes |
|---------|--------|-------|
| `syntax-case` | ✅ | Pattern matching with fenders |
| Lambda transformers | ✅ | `(lambda (stx) ...)` procedural macros |
| `syntax` | ✅ | Template construction |
| `with-syntax` | ✅ | Pattern variable binding |
| `identifier?` | ✅ | Builtin predicate |
| `bound-identifier=?` / `free-identifier=?` | ✅ | Builtins |
| `syntax->datum` / `datum->syntax` | ✅ | Builtins |
| `generate-temporaries` | ✅ | Builtin |

### Standard Forms Implemented as Macros

The following R7RS forms are implemented as hygienic macros in `macros.scm`:

- **Binding**: `let`, `let*`, `letrec`, `letrec*`, `let-values`, `let*-values`, `define-values`
- **Conditionals**: `and`, `or`, `when`, `unless`, `cond`, `case`
- **Iteration**: `do`
- **Lazy evaluation**: `delay`, `delay-force`, `force`
- **Multiple arity**: `case-lambda`
- **Feature detection**: `cond-expand`
- **Quoting**: `append` (variadic macro), `quasiquote` (educational variant)

### Known Limitations

- Nested ellipsis patterns (e.g., `((a ...) ...)`) in template transcription have edge cases where ellipsis depth tracking may fail. Workaround: use recursive helper macros instead.

---

## Continuations and Dynamic Wind

| Feature | Status | Notes |
|---------|--------|-------|
| `call-with-current-continuation` / `call/cc` | ✅ | Full first-class continuations |
| Continuation capture and reinvocation | ✅ | Can be captured, stored, and invoked multiple times |
| `dynamic-wind` | ✅ | Before/after thunks called during continuation transitions |

**Implementation**: The evaluator uses array-based stacks internally, but continuations are serialized to arena-based `ContFrame` chains when captured.

---

## Standard Library Extensions (Non-R7RS)

These features are intentionally non-R7RS, designed for embedded systems and runtime control:

### GC Control
| Procedure | Description |
|-----------|-------------|
| `gc` | Manually trigger garbage collection |
| `gc-enable` / `gc-disable` | Enable/disable automatic GC |
| `gc-enabled?` | Check if GC is enabled |
| `arena-stats` | Get arena statistics as a list: `(capacity allocated free usage%)` |

### Native Function FFI
- Native Rust functions can be registered and called from Scheme
- Used for hardware access in embedded contexts

### Additional Stdlib Functions (SRFI-inspired)

Beyond R7RS, the stdlib provides many convenience functions:

- **List utilities**: `nth`, `take`, `drop`, `zip`, `take-right`, `drop-right`, `split-at`, `concatenate`, `flatten`, `count`, `last`, `last-pair`, `range`, `iota1`/`iota2`/`iota3`, `list-tabulate`
- **List accessors**: `first` through `tenth`
- **Higher-order**: `filter-map`, `partition`, `remove`, `delete`, `any`, `every`, `find`, `fold`, `fold-right`, `reduce`
- **Composition**: `compose`, `identity`, `constantly`, `flip`, `curry`
- **Math**: `cube`, `sum`, `product`, `average`, `sign`
- **Strings**: `string-null?`, `string-reverse`, `string-contains`, `string-join`, `string-split`, `string-trim`
- **Boolean**: `boolean-eq`

---

## What's Missing for Full R7RS Conformance

### High Priority

1. **I/O port system** (§6.13) — Ports, `read`, `read-char`, string ports (note: `write`, `write-char`, and `display` are implemented)
2. **Library system** (§5.6) — `define-library`, `import`, `export`

### Medium Priority

3. **Multi-list `map`/`for-each`** — R7RS requires these to accept multiple list arguments
4. **Bytevectors** (§6.9) — Types, literals, and operations
5. **Missing vector operations** — `vector-map`, `vector-for-each`, `vector-copy!`, `vector-append`
6. **Missing string operations** — `string-copy!`, `string-fill!`, remaining `string-ci` comparisons

### Low Priority / Intentionally Deferred

7. **Full numeric tower** — Floating-point, rationals, complex numbers (conflicts with `no_std`/`no_alloc` design)
8. **Environments** — `environment`, `scheme-report-environment`, `null-environment`
9. **System interface** (§6.14) — `load`, `file-exists?`, `exit`, `command-line`, timing
10. **Tail context tracking** — Full R7RS tail-position specification compliance

---

## Architecture Notes

### Design Constraints

- **`no_std`, `no_alloc`** — Core crates (`grift_arena`, `grift_parser`, `grift_eval`) use no heap allocation
- **Arena-based** — All values allocated in a fixed-size arena with mark-and-sweep GC
- **`Copy` types** — All arena-stored values implement `Copy`
- **Integers only** — Numeric tower limited to exact integers (`isize`) by design

### Crate Structure

| Crate | Role | `no_std` |
|-------|------|----------|
| `grift` | Unified re-export crate | ✅ |
| `grift_arena` | Arena allocator with mark-and-sweep GC | ✅ |
| `grift_core` | Core types: `Value`, `Builtin`, `StdLib`, `Lisp`, `DisplayValue` | ✅ |
| `grift_parser` | Parser with symbol interning | ✅ |
| `grift_eval` | Trampolined evaluator; includes `macros.scm` and `expand.rs` | ✅ |
| `grift_macros` | Proc macros for stdlib generation | N/A |
| `grift_repl` | Interactive REPL | ❌ (uses `std`) |
| `grift_util` | Scheme-to-Rust name conversion utilities | ✅ |
| `grift_arena_embedded` | Hardware access for embedded targets | ✅ |

### Key Source Files

| File | Purpose |
|------|---------|
| `crates/grift_eval/src/evaluator/core.rs` | Special form evaluation (`quote`, `lambda`, `define`, `set!`, `if`, `begin`, `call/cc`, `dynamic-wind`, `values`, `call-with-values`, `eval`, `apply`, `define-syntax`, `let-syntax`, `letrec-syntax`, `syntax-case`, `syntax`, `quasiquote`) |
| `crates/grift_eval/src/evaluator/builtins.rs` | ~86 builtin function implementations |
| `crates/grift_eval/src/evaluator/macros.scm` | ~35 macro definitions (standard R7RS forms) |
| `crates/grift_eval/src/evaluator/expand.rs` | Macro expansion engine |
| `crates/grift_core/src/stdlib.scm` | ~60 stdlib function definitions |
| `crates/grift_core/src/value.rs` | `Value` enum, `Builtin` enum, `StdLib` enum |
| `crates/grift_core/src/display.rs` | Value formatting/display |
| `crates/grift_parser/src/lib.rs` | Parser (lexer + reader) |

---

## Resources

- **R7RS Spec**: `scheme-spec-r7rs/spec.html`
- **Arena Architecture**: `docs/ARENA_ARCHITECTURE.md`
- **Macro Implementation**: `docs/HYGIENIC_MACROS_IMPLEMENTATION.md`
- **Continuations Plan**: `docs/CALL_CC_IMPLEMENTATION_PLAN.md`
- **Syntax-case Details**: `docs/EXTENDING_SCHEME_MACROS.md`
