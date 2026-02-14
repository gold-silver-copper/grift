# Mark-Based Hygiene for Grift

This document describes the phased transition to proper mark-based hygiene in
grift's macro system, targeting four failing Chibi R7RS tests:

- **#103** `let-syntax` hygiene — macro should capture definition-time binding
- **#104** `letrec-syntax` my-or hygiene — shadowed identifiers
- **#121** Nested `define-syntax` — inner pattern variable capture
- **#125** `bound-identifier=?` vs `free-identifier=?` literal matching

## Background

Grift implements macros using **syntax-rules** (which desugars to
**syntax-case** wrapped in a lambda). The transformer lambda captures the
definition-site environment as its closure. However, the current template
transcription emits **raw symbols** for template-introduced identifiers instead
of preserving their definition-site meaning. This causes three classes of bugs:

1. **Environment capture failure** (#103): A template reference `x` should
   resolve at the macro's definition site, but the raw symbol `x` resolves at
   the use site instead.

2. **Introduced-identifier capture** (#104): Template-introduced binding forms
   (`let`, `if`, `temp`) should refer to the global definitions, not locally
   shadowed names at the use site.

3. **Nested macro pattern variable propagation** (#121, #125): When a macro
   generates a `define-syntax` containing `syntax-rules`, pattern variables
   from the outer macro should be substituted into the inner macro's patterns
   and templates.

## Reference: SRFI 72 Algorithm

SRFI 72 describes mark-based hygiene with these key operations:

- **mark(stx)** — Apply a fresh mark to all identifiers in a syntax object
- **expand(stx)** — Mark → transform → mark (double-mark cancellation)
- **bound-identifier=?** — Same name + same marks
- **free-identifier=?** — Both resolve to the same binding

Grift already has mark infrastructure (`mark_syntax`, `marks_equal`,
`bound_identifier_eq`, `free_identifier_eq`) but does not apply marks during
`syntax-rules` expansion.

## Phase 1: Definition-Environment Capture for `syntax-rules` Templates

**Fixes:** #103, #104

### Problem

When `syntax-rules` expands, template symbols that are NOT pattern variables
should resolve at the macro's definition site. Currently, `transcribe_symbol`
emits the raw symbol, which resolves at the use site.

### Solution

In `transcribe_symbol` (the non-lex-env variant used by `syntax-rules`), when
a symbol is not a pattern variable and not already renamed, check whether it is
bound in the macro's **definition environment** (`def_env`). If it is, rename
it to a gensym and install a binding in the use-site environment that maps the
gensym to the definition-site value. This is cheaper than wrapping every
identifier in a syntax object.

Concretely, in `transcribe_binding_form` for `let`, `lambda`, and `define`:
macro-introduced binding names are already gensym'd. The missing piece is that
references to those names (and to outer lexical bindings like `x` in test #103)
in the **body** also need to go through the rename table.

The actual fix is to make `syntax-rules` transformer lambdas carry their
definition-site **lexical** environment, and to use `transcribe_template_with_env`
(which wraps non-pattern symbols in syntax objects with captured env) instead of
`transcribe_template` (which emits raw symbols).

### Files Changed

- `crates/grift_eval/src/evaluator/expand.rs` — `transcribe_symbol`
- `crates/grift_eval/src/evaluator/forms.rs` — `step_eval_syntax`

## Phase 2: Hygienic Renaming of Macro-Introduced Core Forms

**Fixes:** #104

### Problem

Test #104's `my-or` macro introduces `let`, `if`, and `temp` in its template.
At the use site, the user has rebound `let` to `odd?`, `if` to `even?`, and
`temp` to `8`. Without renaming, the expanded code uses these shadowed names.

### Solution

The `transcribe_binding_form` already handles `let` binding variables. The
additional requirement is that the **keywords** `let` and `if` themselves need
to be preserved from the definition site. This is achieved by Phase 1's
environment capture — when the template references `let` or `if`, these are
looked up in `def_env` (where they are unbound, i.e., they refer to the global
special forms). The fix ensures they don't get shadowed by local bindings.

### Files Changed

Same as Phase 1 — the environment capture mechanism handles this automatically.

## Phase 3: Pattern Variable Propagation into Nested `define-syntax`

**Fixes:** #121

### Problem

```scheme
(define-syntax foo
  (syntax-rules ()
    ((foo bar y)
     (define-syntax bar
       (syntax-rules ()
         ((bar x) 'y))))))
(foo bar x)
(bar 1) ;; should return 'x, not 1
```

The outer macro `foo` has pattern variable `y` bound to `x`. The inner
`syntax-rules` template `'y` should substitute `y` → `x`, producing `'x`.

### Solution

The `transcribe_define_syntax_form` function already calls
`gensym_syntax_rules_pattern_vars` to rename inner pattern variables. The fix
is to ensure that **outer pattern variable bindings** are properly substituted
into the inner `syntax-rules` templates before the inner macro is installed.

Specifically, when transcribing the body of a `define-syntax` form, the outer
bindings must be applied to the inner syntax-rules templates (not just the
patterns).

### Files Changed

- `crates/grift_eval/src/evaluator/expand.rs` — `transcribe_define_syntax_form`

## Phase 4: Literal Matching with Substituted Pattern Variables

**Fixes:** #125

### Problem

```scheme
(let-syntax
    ((m (syntax-rules ()
          ((m x) (let-syntax
                     ((n (syntax-rules (k)
                           ((n x) 'bound-identifier=?)
                           ((n y) 'free-identifier=?))))
                   (n z))))))
  (m k))
```

When `m` is called with `k`, the pattern variable `x` is bound to `k`. The
inner `syntax-rules` for `n` has `k` in its literals list and `x` (substituted
to `k`) as a pattern. The invocation `(n z)` should match the second clause
(because `z` ≠ `k` as a literal), returning `'free-identifier=?`.

Wait — the expected result is `'bound-identifier=?`. This means the literal `k`
in `(syntax-rules (k) ...)` should be compared using `bound-identifier=?` with
the pattern `x` (which was substituted to `k` from the outer macro's bindings).
Since both the literal `k` and the substituted `k` come from the same expansion
context, they are `bound-identifier=?`, so the first clause matches.

### Solution

When constructing the inner syntax-rules literals list and patterns, outer
pattern variable substitutions must be applied. The literal `k` and the
substituted pattern variable `x` → `k` should be the same identifier (same
marks, same name), making `bound-identifier=?` return true.

This is achieved by Phase 3's pattern variable propagation — when the inner
`syntax-rules` is transcribed, the outer binding `x` → `k` is applied to both
the literals list and the patterns.

### Files Changed

Same as Phase 3.

## Testing Strategy

All four tests are already in `crates/grift/tests/scheme/r7rs-tests.scm` and
run as part of the Chibi R7RS test suite. The SRFI-64 runner reports them by
number.

Verification:
```bash
cargo test -p grift --test srfi64_runner chibi_r7rs_tests
```

Expected: All 4 tests (#103, #104, #121, #125) should pass, and the total
pass count should increase from 974 to 978.

## Risk Assessment

- **Regression risk**: High. Changing symbol transcription affects all macro
  expansions. Thorough testing with the full R7RS suite is essential.
- **Performance**: Minimal. The changes add symbol lookups during transcription
  but do not change algorithmic complexity.
- **Arena pressure**: Slightly increased due to additional gensym allocations
  and syntax object wrapping, but within normal bounds.
