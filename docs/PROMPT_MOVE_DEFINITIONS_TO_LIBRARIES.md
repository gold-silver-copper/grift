# Prompt: Move Scheme Definitions from Prelude into Proper R7RS Libraries

## Goal

Restructure grift so that pure Scheme function definitions live inside their proper R7RS library files (e.g., `caar` is defined inside `(scheme cxr)`, `make-promise` inside `(scheme lazy)`) instead of being defined in the monolithic `prelude.scm` and merely re-exported through libraries.

---

## Prompt

```
I need to restructure grift's standard library so that Scheme function definitions live inside their proper R7RS library files rather than all being defined in `prelude.scm` and merely re-exported.

## Current Architecture

1. `crates/grift_core/src/prelude.scm` (~1700 lines) contains ALL:
   - Macro definitions (`define-syntax` forms like `let`, `cond`, `and`, `or`, `guard`, `delay`, etc.)
   - Pure Scheme function definitions (`define` forms like `map`, `filter`, `caar`, `cadr`, `promise?`, etc.)

2. `crates/grift_core/src/lib/scheme/*.scm` (16 library files) contain ONLY `define-library` forms with `(export ...)` declarations and no `(begin ...)` bodies. Example:
   ```scheme
   (define-library (scheme cxr)
     (export caar cadr cdar cddr caaar ...))
   ```

3. `crates/grift_macros/src/lib.rs` has the `include_stdlib!` proc macro that parses `prelude.scm`, extracts all `(define (name params...) body)` forms, and generates a `StdLib` Rust enum with static metadata (name, params, body strings). These StdLib functions are registered into the global environment at evaluator startup.

4. At runtime, when a library is imported, `define-library` in `crates/grift_eval/src/evaluator/forms.rs:2495` starts with a copy of the global env (which already has all builtins + all StdLib functions), evaluates any `(begin ...)` bodies, then filters exports. This means libraries currently work by subsetting the global env, not by defining anything themselves.

## Desired Architecture

Move pure Scheme `define` forms out of `prelude.scm` and into `(begin ...)` blocks inside the appropriate library `.scm` files. For example, `lib/scheme/cxr.scm` should become:

```scheme
(define-library (scheme cxr)
  (export caar cadr cdar cddr caaar ...)
  (begin
    (define (caar lst) (car (car lst)))
    (define (cadr lst) (car (cdr lst)))
    (define (cdar lst) (cdr (car lst)))
    ...))
```

And the corresponding definitions should be REMOVED from `prelude.scm`.

## What Should Stay in prelude.scm

The prelude should retain ONLY things that are needed for bootstrapping the evaluator before any library can be loaded:

1. **Core macros** that the evaluator or library loading mechanism depends on (e.g., `syntax-rules` bootstrapping, `let`, `let*`, `letrec`, `begin`, `and`, `or`, `cond`, `case`, `do`, `quasiquote` helpers)
2. **Macros that other libraries need during their `(begin ...)` evaluation** — since `define-library` starts with a copy of global_env, any macro in prelude.scm will be available to library `(begin ...)` blocks

Anything that is a pure function definition (`define`) and is listed in a specific R7RS library's export list should move to that library.

## What Should Move — Library Mapping

Use the R7RS-small spec and the existing export lists in `lib/scheme/*.scm` to determine which definitions move where. Key mappings:

- **(scheme cxr)**: `caar`, `cadr`, `cdar`, `cddr`, `caaar`, `caadr`, `cadar`, `caddr`, `cdaar`, `cdadr`, `cddar`, `cdddr`, `caaaar`, `caaadr`, `caadar`, `caaddr`, `cadaar`, `cadadr`, `caddar`, `cadddr`, `cdaaar`, `cdaadr`, `cdadar`, `cddddr`
- **(scheme lazy)**: `delay` (syntax), `force` (syntax), `delay-force` (syntax), `make-promise`, `promise?`
- **(scheme char)**: `char-alphabetic?`, `char-numeric?`, `char-whitespace?`, `char-upper-case?`, `char-lower-case?`, `char-ci=?`, `char-ci<?`, `char-ci>?`, `char-ci<=?`, `char-ci>=?`, `char-upcase`, `char-downcase`, `char-foldcase`, `digit-value`, `string-ci=?`, `string-upcase`, `string-downcase`, `string-foldcase`
- **(scheme base)**: Everything else that's currently exported by `(scheme base)` — this is the largest set, including `map`, `filter`, `fold`, `for-each`, `length`, `reverse`, `list-tail`, `list-ref`, `member`, `assoc`, `not`, `zero?`, `positive?`, `negative?`, `even?`, `odd?`, `abs`, `min`, `max`, `gcd`, `lcm`, `square`, `exact->inexact`, `inexact->exact`, `number->string`, `string->number`, `make-list`, `list-copy`, `list-set!`, `iota`, `assq`, `assv`, `string-map`, `string-for-each`, `vector-map`, `vector-for-each`, etc. Also all macros that are part of `(scheme base)`: `let`, `let*`, `letrec`, `letrec*`, `and`, `or`, `when`, `unless`, `cond`, `case`, `do`, `let-values`, `let*-values`, `define-values`, `case-lambda`, `guard`, `parameterize`, `cond-expand`, `syntax-rules`
- **(scheme write)**: `display`, `write` (these are builtins, so just exported — no define needed)
- **(scheme read)**: `read` (builtin)
- **(scheme eval)**: `eval`, `environment` (builtins/special forms)
- **(scheme process-context)**, **(scheme time)**, **(scheme file)**, **(scheme inexact)**, **(scheme complex)**, **(scheme r5rs)**: Check each library's export list and move any pure Scheme definitions accordingly.

## Complications to Handle

### 1. The `include_stdlib!` / StdLib Enum Pipeline

Currently, `include_stdlib!` parses `prelude.scm` to generate a Rust `StdLib` enum. Functions that move to library `.scm` files will NO LONGER be parsed by this macro and won't become `StdLib` variants.

**Options (choose one):**
- **Option A (recommended):** Modify `include_stdlib!` to also parse the library `.scm` files — extract `define` forms from `(begin ...)` blocks inside `define-library`. This preserves the StdLib enum approach and static metadata.
- **Option B:** Keep definitions that need to be in StdLib in prelude.scm but ALSO duplicate them in the library files. This is messy — avoid if possible.
- **Option C:** Stop using the StdLib enum for functions that are now defined inside libraries. Instead, those functions get defined at library load time when the `(begin ...)` block is evaluated. This simplifies the Rust side but means those functions are only available after `(import ...)`, not globally. This is actually correct R7RS behavior.

### 2. Inter-Library Dependencies

Some functions depend on others. For example, functions in `(scheme base)` might use `caar` which is in `(scheme cxr)`. Handle this with `(import ...)` declarations inside library files:

```scheme
(define-library (scheme base)
  (import (scheme cxr))  ;; if base needs cxr functions
  (export ...)
  (begin ...))
```

But be careful — R7RS says `(scheme base)` should NOT require importing `(scheme cxr)` as a dependency. Check each case: if a `(scheme base)` function uses `caar`, either inline the `(car (car x))` call or accept the dependency.

### 3. Macro Bootstrapping

Macros defined with `define-syntax` in the prelude (like `let`, `cond`, `and`, `or`) are needed early — both by user code and by other library definitions. Two approaches:

- **Keep core macros in prelude.scm** so they're in the global env before any library loads. Library `(begin ...)` blocks can use them freely since `define-library` starts with a copy of `self.global_env`.
- **Move macros to `(scheme base)`** but ensure `(scheme base)` is loaded early enough. Since `define-library` starts with global_env (which has builtins), macros defined in `(begin ...)` would only be visible within that library and to importers.

The safest approach: keep fundamental macros (`let`, `let*`, `letrec`, `and`, `or`, `cond`, `case`, `do`, `when`, `unless`, `quasiquote` helpers, `syntax-rules`) in `prelude.scm` as bootstrap macros. Move `(scheme base)`-specific macros like `guard`, `parameterize`, `case-lambda`, `let-values`, `define-values`, `cond-expand`, `delay`, `delay-force`, `force` into their respective libraries.

### 4. `(scheme base)` Is Special

`(scheme base)` exports most of the language. It will have a large `(begin ...)` block. This is fine — it mirrors how other R7RS implementations work (Chibi, Gauche, etc.).

### 5. Tests

After restructuring, ALL existing tests must pass. Run `cargo test --workspace` after each library migration. The test suite is comprehensive (~16,000 LOC) and will catch regressions.

## Step-by-Step Plan

1. **Start with the smallest, most isolated library: `(scheme cxr)`**
   - Move all cxr `define` forms from prelude.scm into `lib/scheme/cxr.scm` inside a `(begin ...)` block
   - Remove those definitions from prelude.scm
   - Decide how to handle the StdLib enum (Option A or C above)
   - Run tests

2. **Next: `(scheme lazy)`**
   - Move `delay`, `force`, `delay-force` (macros), `make-promise`, `promise?` (functions)
   - Run tests

3. **Then: `(scheme char)`**
   - Move char/string case functions that are defined in Scheme (not builtins)
   - Run tests

4. **Then: smaller libraries** — `(scheme inexact)`, `(scheme complex)`, `(scheme time)`, `(scheme file)`, `(scheme process-context)`, `(scheme read)`, `(scheme write)`, `(scheme eval)`, `(scheme repl)`, `(scheme r5rs)`
   - Most of these export only builtins — they may not need `(begin ...)` blocks at all
   - Run tests after each

5. **Finally: `(scheme base)`** — the big one
   - Move all remaining pure Scheme definitions
   - Keep only bootstrap macros in prelude.scm
   - Run tests

6. **Clean up prelude.scm** — verify it contains only bootstrap macros
7. **Update `include_stdlib!`** if using Option A
8. **Update documentation** — R7RS-CONFORMANCE.md, README.md

## Important Constraints

- Maintain `no_std` / `no_alloc` compatibility throughout
- Do NOT change the evaluator's `define-library` implementation unless absolutely necessary — it already supports `(begin ...)` blocks correctly
- Do NOT change the library registry format
- Preserve all existing tests passing
- Each library should be self-contained: its `(begin ...)` block should define everything it exports (except builtins which are inherited from global_env)
```
