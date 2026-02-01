# Hygienic Macros Implementation Plan

## Implementation Progress

**Status: Phase 1-6 Complete (Foundation through Cleanup)**

### Completed Work

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1 | Foundation (Data Structures, Gensym) | ✅ Complete |
| Phase 2 | Pattern Matching | ✅ Complete |
| Phase 3 | Template Transcription | ✅ Complete |
| Phase 4 | Expander Integration | ✅ Complete |
| Phase 5 | Standard Macros | ✅ Complete |
| Phase 6 | Cleanup | ✅ Complete |

### Files Modified/Created

- `crates/grift_parser/src/value.rs` - Added `SyntaxRules` variant to `Value` enum
- `crates/grift_parser/src/lisp.rs` - Added `syntax_rules()`, `syntax_rules_parts()`, and `eqv()` methods
- `crates/grift_eval/src/evaluator/mod.rs` - Added `macro_env` and `gensym_counter` fields
- `crates/grift_eval/src/evaluator/core.rs` - Modified `eval()` to expand macros, added macro loading, removed redundant special form handlers for macro-based forms
- `crates/grift_eval/src/evaluator/forms.rs` - Removed step_eval_* and continuation handlers for macro-based forms, added macro expansion to stdlib function parsing
- `crates/grift_eval/src/evaluator/expand.rs` - **NEW** - Complete macro expansion system
- `crates/grift_eval/src/continuation.rs` - Removed When, Unless, CondTest, And, Or continuation types
- `crates/grift_parser/src/macros.scm` - **NEW** - Standard macro definitions

### Working Features

- `define-syntax` for defining macros
- `syntax-rules` pattern language with:
  - Pattern variables
  - Literal keywords
  - Wildcard `_`
  - Ellipsis `...` for repetition
- `let-syntax` for local macro definitions
- Hygiene via gensym for lambda parameters
- Standard macros: `when`, `unless`, `and`, `or`, `cond`, `delay`
- Stdlib functions properly expand macros in their bodies

### Phase 6 Cleanup Details

The following changes were made as part of Phase 6:

1. **Removed special form handlers from core.rs:**
   - `when`, `unless` - now handled by macros
   - `and`, `or` - now handled by macros
   - `cond` - now handled by macro

2. **Removed continuation types from continuation.rs:**
   - `When`, `Unless`, `CondTest`, `And`, `Or`
   - Corresponding pack/unpack methods removed

3. **Removed step_eval_* functions from forms.rs:**
   - `step_eval_and`, `step_eval_or`, `step_eval_cond_cont`
   - Related continuation handlers

4. **Fixed stdlib function macro expansion:**
   - Stdlib functions now have their bodies expanded with macros at parse time
   - This enables stdlib functions to use `and`, `or`, `cond`, etc.

### Known Limitations

1. The `=>` clause in `cond` is not yet implemented (simplified for initial release)
2. **Nested ellipsis pattern bug**: Patterns like `((name val) ...)` don't correctly extract all elements. 
   Only the first element is correctly extracted; subsequent elements retain the full structure instead of 
   being deconstructed. **Workaround**: Use recursive macro patterns with dotted pairs instead.
   - See detailed analysis below in "Phase 7: Binding Form Macros"
3. `letrec-syntax` is not yet implemented
4. `case` and `do` remain as special forms (require complex ellipsis patterns - future work)
5. **Named let** is not supported by the `let` macro; the special form handles it
6. `letrec` and `letrec*` remain as special forms (not yet converted to macros)

### Testing

All existing tests pass with the macro system enabled. Standard macros are loaded at evaluator initialization.

### Recent Changes

1. **Fixed lambda parameter substitution bug**: Pattern variables in lambda parameter lists 
   are now correctly substituted (was returning the pattern variable symbol instead of the bound value).
2. **Added `let` and `let*` macros**: These use a recursive approach with dotted pair patterns
   to work around the nested ellipsis bug.
3. **Added `%let-binding` helper macro**: Internal macro for transforming single bindings.

---

## Overview

This document describes the implementation of hygienic macros for Grift, based on the
"Macros that Work" algorithm (Clinger & Rees, POPL 1991). The goal is to implement
`define-syntax` and `syntax-rules` to allow replacing derived special forms (like
`let`, `let*`, `cond`, etc.) with macro-based definitions.

### Design Goals

1. **Minimal memory overhead** - Arena-friendly, no per-expression marks
2. **Correct hygiene** - Macro-introduced bindings don't capture user variables
3. **R7RS compatibility** - Standard `syntax-rules` pattern language
4. **Performance** - Expansion happens once at load time, not during evaluation

### References

- [Macros that Work](https://www.researchgate.net/publication/220997237_Macros_That_Work) - Clinger & Rees, 1991
- [Hygienic Macro Expansion](https://www.semanticscholar.org/paper/Hygienic-macro-expansion-Kohlbecker-Friedman/d18e91ddfd00b2a04cdbbf800f25b3ce12e1c982) - Kohlbecker et al., 1986
- [R7RS-small](https://small.r7rs.org/attachment/r7rs.pdf) - Section 4.3 (Macros)

---

## Part 1: Data Structures

### 1.1 New Value Variant

Add to `crates/grift_parser/src/value.rs`:

```rust
/// Syntax-rules macro transformer
///
/// Stores a compiled macro with literals, rules, and definition environment.
/// Following the same 2-index constraint as Lambda, we pack fields into cons cells.
///
/// # Memory Layout
///
/// - `literals`: ArenaIndex to list of literal keyword symbols
/// - `rules_env`: ArenaIndex to cons cell (rules . definition_env)
///   - car: list of (pattern . template) pairs
///   - cdr: environment where macro was defined (for hygiene)
///
/// This matches Lambda's layout: 2 inline ArenaIndex fields = 1 arena slot.
///
/// # Example
///
/// ```scheme
/// (define-syntax when
///   (syntax-rules ()
///     ((when test body ...)
///      (if test (begin body ...)))))
/// ```
///
/// Stored as:
/// - literals: ()
/// - rules_env: (rules_list . global_env)
///   where rules_list = (((when test body ...) . (if test (begin body ...))))
SyntaxRules {
    literals: ArenaIndex,   // list of literal keyword symbols
    rules_env: ArenaIndex,  // cons cell: (rules . definition_env)
},
```

### 1.2 Lisp Constructor

Add to `crates/grift_parser/src/lisp.rs`:

```rust
impl<const N: usize> Lisp<N> {
    /// Create a syntax-rules macro transformer
    ///
    /// Packs rules and definition_env into a cons cell to maintain
    /// the 2-index constraint (matching Lambda's layout).
    ///
    /// # Arguments
    ///
    /// * `literals` - List of literal keywords that must match exactly
    /// * `rules` - List of (pattern . template) pairs
    /// * `definition_env` - Environment where macro was defined
    pub fn syntax_rules(
        &self,
        literals: ArenaIndex,
        rules: ArenaIndex,
        definition_env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        // Pack rules and env into cons cell: (rules . definition_env)
        let rules_env = self.cons(rules, definition_env)?;
        self.arena.alloc(Value::SyntaxRules {
            literals,
            rules_env,
        })
    }

    /// Extract parts from a SyntaxRules value
    ///
    /// Returns (literals, rules, definition_env) unpacked from the internal structure.
    pub fn syntax_rules_parts(
        &self,
        idx: ArenaIndex,
    ) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex)> {
        match self.get(idx)? {
            Value::SyntaxRules { literals, rules_env } => {
                // Unpack (rules . definition_env)
                let rules = self.car(rules_env)?;
                let definition_env = self.cdr(rules_env)?;
                Ok((literals, rules, definition_env))
            }
            _ => Err(ArenaError::TypeMismatch),
        }
    }
}
```

### 1.3 Evaluator Extensions

Add to `crates/grift_eval/src/evaluator/mod.rs`:

```rust
pub struct Evaluator<'a, const N: usize> {
    // ... existing fields ...

    /// Macro environment - stores (name . SyntaxRules) bindings
    /// Separate from value environment to allow shadowing
    macro_env: ArenaIndex,

    /// Counter for generating unique symbols (gensym)
    gensym_counter: usize,
}
```

Initialize in `Evaluator::new()`:

```rust
macro_env: lisp.nil()?,
gensym_counter: 0,
```

---

## Part 2: Hygiene Mechanism

### 2.1 Gensym (Fresh Symbol Generation)

The core hygiene mechanism generates unique symbols for macro-introduced bindings.

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Generate a fresh symbol guaranteed to be unique
    ///
    /// Format: #:g{counter} or #:{base}{counter}
    ///
    /// # Example
    ///
    /// ```rust
    /// let sym1 = eval.gensym("tmp")?;  // #:tmp0
    /// let sym2 = eval.gensym("tmp")?;  // #:tmp1
    /// let sym3 = eval.gensym_simple()?; // #:g2
    /// ```
    ///
    /// # Memory
    ///
    /// Each gensym allocates 1 arena slot for the symbol.
    /// Symbols are interned, so repeated calls create new unique symbols.
    pub fn gensym(&mut self, base: &str) -> EvalResult {
        use core::fmt::Write;

        // Build symbol name: #:{base}{counter}
        // Using a fixed buffer to avoid heap allocation
        let mut buf = [0u8; 48];
        let mut cursor = WriteCursor::new(&mut buf);

        let _ = write!(cursor, "#:{}{}", base, self.gensym_counter);
        self.gensym_counter += 1;

        let name = cursor.as_str()
            .ok_or_else(|| self.make_error(ErrorKind::Generic, self.lisp.nil().unwrap()))?;

        self.lisp.symbol(name).map_err(Into::into)
    }

    /// Generate a simple gensym with default prefix
    pub fn gensym_simple(&mut self) -> EvalResult {
        self.gensym("g")
    }
}

/// Helper for no-alloc string formatting
struct WriteCursor<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> WriteCursor<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.buf[..self.pos]).ok()
    }
}

impl core::fmt::Write for WriteCursor<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        if self.pos + bytes.len() <= self.buf.len() {
            self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
            self.pos += bytes.len();
            Ok(())
        } else {
            Err(core::fmt::Error)
        }
    }
}
```

### 2.2 Rename Environment

Track renamings during template transcription:

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Look up a symbol in the rename environment
    ///
    /// Returns the renamed symbol if found, otherwise the original.
    fn rename_lookup(&self, sym: ArenaIndex, renames: ArenaIndex) -> EvalResult {
        let mut current = renames;
        loop {
            if self.lisp.get(current)?.is_nil() {
                return Ok(sym); // Not renamed
            }
            let pair = self.lisp.car(current)?;
            let key = self.lisp.car(pair)?;
            if self.symbols_eq(key, sym)? {
                return self.lisp.cdr(pair);
            }
            current = self.lisp.cdr(current)?;
        }
    }

    /// Extend rename environment with a new mapping
    fn rename_extend(
        &self,
        renames: ArenaIndex,
        original: ArenaIndex,
        renamed: ArenaIndex,
    ) -> EvalResult {
        let pair = self.lisp.cons(original, renamed)?;
        self.lisp.cons(pair, renames).map_err(Into::into)
    }
}
```

---

## Part 3: Pattern Matching

### 3.1 Pattern Language

The `syntax-rules` pattern language supports:

| Pattern | Matches |
|---------|---------|
| `_` | Anything (wildcard, no binding) |
| `symbol` | Binds to any expression |
| `literal` | Must match exactly (from literals list) |
| `(pat1 pat2 ...)` | List with matching elements |
| `(pat1 ... patn . patm)` | Improper list |
| `(pat ...)` | Zero or more matches of pat |
| `(pat1 pat2 ... patn)` | pat2 repeats, pat1/patn fixed |

### 3.2 Pattern Matcher Implementation

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Match an expression against a pattern
    ///
    /// # Arguments
    ///
    /// * `pattern` - The syntax-rules pattern
    /// * `expr` - The expression to match
    /// * `literals` - List of literal keywords
    /// * `bindings` - Accumulated bindings (alist)
    ///
    /// # Returns
    ///
    /// * `Ok(Some(bindings))` - Match succeeded with bindings
    /// * `Ok(None)` - Match failed
    /// * `Err(_)` - Error during matching
    fn match_pattern(
        &self,
        pattern: ArenaIndex,
        expr: ArenaIndex,
        literals: ArenaIndex,
        bindings: ArenaIndex,
    ) -> Result<Option<ArenaIndex>, EvalError> {
        match self.lisp.get(pattern)? {
            // Wildcard: matches anything, binds nothing
            Value::Symbol(_) if self.lisp.symbol_matches(pattern, "_")? => {
                Ok(Some(bindings))
            }

            // Ellipsis symbol itself: error (shouldn't appear here)
            Value::Symbol(_) if self.lisp.symbol_matches(pattern, "...")? => {
                Err(self.make_error(ErrorKind::Generic, pattern)
                    .with_message("misplaced ellipsis in pattern"))
            }

            // Literal keyword: must match exactly
            Value::Symbol(_) if self.is_literal(pattern, literals)? => {
                match self.lisp.get(expr)? {
                    Value::Symbol(_) if self.symbols_eq(pattern, expr)? => {
                        Ok(Some(bindings))
                    }
                    _ => Ok(None),
                }
            }

            // Pattern variable: bind to expression
            Value::Symbol(_) => {
                let new_bindings = self.bindings_extend(bindings, pattern, expr)?;
                Ok(Some(new_bindings))
            }

            // Empty list: must match empty list
            Value::Nil => {
                if self.lisp.get(expr)?.is_nil() {
                    Ok(Some(bindings))
                } else {
                    Ok(None)
                }
            }

            // List pattern
            Value::Cons { .. } => {
                self.match_list_pattern(pattern, expr, literals, bindings)
            }

            // Other constants: must be eqv?
            _ => {
                if self.lisp.eqv(pattern, expr)? {
                    Ok(Some(bindings))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Match a list pattern, handling ellipsis
    fn match_list_pattern(
        &self,
        pattern: ArenaIndex,
        expr: ArenaIndex,
        literals: ArenaIndex,
        bindings: ArenaIndex,
    ) -> Result<Option<ArenaIndex>, EvalError> {
        // Check if expression is a list
        if !matches!(self.lisp.get(expr)?, Value::Cons { .. }) {
            return Ok(None);
        }

        let pat_car = self.lisp.car(pattern)?;
        let pat_cdr = self.lisp.cdr(pattern)?;

        // Check for ellipsis: (subpat ... . rest)
        if self.has_ellipsis(pat_cdr)? {
            return self.match_ellipsis_pattern(
                pat_car, pat_cdr, expr, literals, bindings
            );
        }

        // Regular list: match car and cdr
        let expr_car = self.lisp.car(expr)?;
        let expr_cdr = self.lisp.cdr(expr)?;

        match self.match_pattern(pat_car, expr_car, literals, bindings)? {
            Some(bindings1) => {
                self.match_pattern(pat_cdr, expr_cdr, literals, bindings1)
            }
            None => Ok(None),
        }
    }

    /// Check if pattern cdr starts with ellipsis
    fn has_ellipsis(&self, pat_cdr: ArenaIndex) -> Result<bool, EvalError> {
        match self.lisp.get(pat_cdr)? {
            Value::Cons { .. } => {
                let first = self.lisp.car(pat_cdr)?;
                self.lisp.symbol_matches(first, "...")
            }
            _ => Ok(false),
        }
    }

    /// Match ellipsis pattern: (subpat ... . rest)
    fn match_ellipsis_pattern(
        &self,
        sub_pattern: ArenaIndex,
        pat_cdr: ArenaIndex,      // (... . rest)
        expr: ArenaIndex,
        literals: ArenaIndex,
        bindings: ArenaIndex,
    ) -> Result<Option<ArenaIndex>, EvalError> {
        // Get the rest pattern after ...
        let rest_pattern = self.lisp.cdr(pat_cdr)?;

        // Count how many elements the rest pattern needs
        let rest_len = self.pattern_min_length(rest_pattern, literals)?;

        // Count expression length
        let expr_len = self.list_length(expr)?;

        if expr_len < rest_len {
            return Ok(None); // Not enough elements
        }

        // Elements for ellipsis = total - rest
        let ellipsis_count = expr_len - rest_len;

        // Collect pattern variables from sub_pattern
        let pattern_vars = self.collect_pattern_vars(sub_pattern, literals)?;

        // Initialize accumulators for each pattern variable
        let mut var_accums = self.lisp.nil()?;
        let mut pv_current = pattern_vars;
        while let Value::Cons { .. } = self.lisp.get(pv_current)? {
            let var = self.lisp.car(pv_current)?;
            let empty = self.lisp.nil()?;
            var_accums = self.bindings_extend(var_accums, var, empty)?;
            pv_current = self.lisp.cdr(pv_current)?;
        }

        // Match ellipsis elements
        let mut current = expr;
        for _ in 0..ellipsis_count {
            let elem = self.lisp.car(current)?;
            let empty = self.lisp.nil()?;

            match self.match_pattern(sub_pattern, elem, literals, empty)? {
                Some(elem_bindings) => {
                    // Append each matched value to its accumulator
                    var_accums = self.merge_ellipsis_bindings(
                        var_accums, elem_bindings
                    )?;
                }
                None => return Ok(None),
            }

            current = self.lisp.cdr(current)?;
        }

        // Reverse accumulated lists and merge with bindings
        let mut result = bindings;
        let mut va_current = var_accums;
        while let Value::Cons { .. } = self.lisp.get(va_current)? {
            let pair = self.lisp.car(va_current)?;
            let var = self.lisp.car(pair)?;
            let vals = self.lisp.cdr(pair)?;
            let reversed = self.reverse_list(vals)?;
            result = self.bindings_extend(result, var, reversed)?;
            va_current = self.lisp.cdr(va_current)?;
        }

        // Match rest pattern
        self.match_pattern(rest_pattern, current, literals, result)
    }

    /// Check if symbol is in literals list
    fn is_literal(&self, sym: ArenaIndex, literals: ArenaIndex) -> Result<bool, EvalError> {
        let mut current = literals;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let lit = self.lisp.car(current)?;
            if self.symbols_eq(sym, lit)? {
                return Ok(true);
            }
            current = self.lisp.cdr(current)?;
        }
        Ok(false)
    }

    /// Collect all pattern variables from a pattern
    fn collect_pattern_vars(
        &self,
        pattern: ArenaIndex,
        literals: ArenaIndex,
    ) -> EvalResult {
        let mut vars = self.lisp.nil()?;
        self.collect_pattern_vars_into(pattern, literals, &mut vars)?;
        Ok(vars)
    }

    fn collect_pattern_vars_into(
        &self,
        pattern: ArenaIndex,
        literals: ArenaIndex,
        vars: &mut ArenaIndex,
    ) -> Result<(), EvalError> {
        match self.lisp.get(pattern)? {
            Value::Symbol(_) => {
                // Skip _, ..., and literals
                if !self.lisp.symbol_matches(pattern, "_")?
                    && !self.lisp.symbol_matches(pattern, "...")?
                    && !self.is_literal(pattern, literals)?
                {
                    *vars = self.lisp.cons(pattern, *vars)?;
                }
                Ok(())
            }
            Value::Cons { .. } => {
                let car = self.lisp.car(pattern)?;
                let cdr = self.lisp.cdr(pattern)?;
                self.collect_pattern_vars_into(car, literals, vars)?;
                // Skip ellipsis symbol
                if let Value::Cons { .. } = self.lisp.get(cdr)? {
                    let first = self.lisp.car(cdr)?;
                    if self.lisp.symbol_matches(first, "...")? {
                        let rest = self.lisp.cdr(cdr)?;
                        return self.collect_pattern_vars_into(rest, literals, vars);
                    }
                }
                self.collect_pattern_vars_into(cdr, literals, vars)
            }
            _ => Ok(()),
        }
    }
}
```

---

## Part 4: Template Transcription

### 4.1 Template Substitution

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Transcribe a template with matched bindings
    ///
    /// # Arguments
    ///
    /// * `template` - The template to transcribe
    /// * `bindings` - Pattern variable bindings
    /// * `renames` - Current rename environment
    /// * `def_env` - Macro definition environment
    ///
    /// # Hygiene
    ///
    /// Variables introduced by the macro (not from pattern) are renamed
    /// using gensym to prevent capture.
    fn transcribe_template(
        &mut self,
        template: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> EvalResult {
        match self.lisp.get(template)? {
            Value::Symbol(_) => {
                self.transcribe_symbol(template, bindings, renames, def_env)
            }

            Value::Nil => self.lisp.nil(),

            Value::Cons { .. } => {
                self.transcribe_list(template, bindings, renames, def_env)
            }

            // Other atoms pass through unchanged
            _ => Ok(template),
        }
    }

    /// Transcribe a symbol in template
    fn transcribe_symbol(
        &mut self,
        sym: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> EvalResult {
        // 1. Check if it's a pattern variable
        if let Some(val) = self.bindings_lookup(bindings, sym)? {
            return Ok(val);
        }

        // 2. Check if already renamed
        let renamed = self.rename_lookup(sym, renames)?;
        if !self.symbols_eq(renamed, sym)? {
            return Ok(renamed);
        }

        // 3. Check if bound in macro's definition environment
        //    If so, keep original (it refers to macro's binding)
        if self.env_bound_anywhere(def_env, sym)? {
            return Ok(sym);
        }

        // 4. Otherwise, it's introduced by the macro - keep as-is
        //    (will be handled by binding form transcription)
        Ok(sym)
    }

    /// Transcribe a list in template
    fn transcribe_list(
        &mut self,
        template: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> EvalResult {
        let car = self.lisp.car(template)?;
        let cdr = self.lisp.cdr(template)?;

        // Check for ellipsis
        if self.has_ellipsis(cdr)? {
            return self.transcribe_ellipsis(car, cdr, bindings, renames, def_env);
        }

        // Check for binding forms that need special handling
        if self.is_binding_keyword(car)? {
            return self.transcribe_binding_form(
                template, bindings, renames, def_env
            );
        }

        // Regular list: transcribe each element
        let new_car = self.transcribe_template(car, bindings, renames, def_env)?;
        let new_cdr = self.transcribe_template(cdr, bindings, renames, def_env)?;
        self.lisp.cons(new_car, new_cdr).map_err(Into::into)
    }

    /// Transcribe ellipsis template: (subtempl ... . rest)
    fn transcribe_ellipsis(
        &mut self,
        sub_template: ArenaIndex,
        template_cdr: ArenaIndex,   // (... . rest)
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> EvalResult {
        // Find pattern variables with ellipsis bindings in sub_template
        let ellipsis_vars = self.find_ellipsis_vars(sub_template, bindings)?;

        if self.lisp.get(ellipsis_vars)?.is_nil() {
            // No ellipsis vars - transcribe once
            let transcribed = self.transcribe_template(
                sub_template, bindings, renames, def_env
            )?;
            let rest = self.lisp.cdr(template_cdr)?;
            let rest_transcribed = self.transcribe_template(
                rest, bindings, renames, def_env
            )?;
            return self.lisp.cons(transcribed, rest_transcribed).map_err(Into::into);
        }

        // Get iteration count from first ellipsis variable
        let first_var = self.lisp.car(ellipsis_vars)?;
        let first_vals = self.bindings_lookup(bindings, first_var)?
            .ok_or_else(|| self.make_error(ErrorKind::UnboundVariable, first_var))?;
        let count = self.list_length(first_vals)?;

        // Build result by iterating
        let mut result = self.lisp.nil()?;
        for i in (0..count).rev() {
            // Create bindings with i-th element of each ellipsis var
            let iter_bindings = self.make_iter_bindings(bindings, ellipsis_vars, i)?;
            let transcribed = self.transcribe_template(
                sub_template, iter_bindings, renames, def_env
            )?;
            result = self.lisp.cons(transcribed, result)?;
        }

        // Transcribe and append rest
        let rest = self.lisp.cdr(template_cdr)?;
        if !self.lisp.get(rest)?.is_nil() {
            let rest_transcribed = self.transcribe_template(
                rest, bindings, renames, def_env
            )?;
            result = self.append_lists(result, rest_transcribed)?;
        }

        Ok(result)
    }

    /// Handle binding forms (lambda, let, etc.)
    fn transcribe_binding_form(
        &mut self,
        form: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> EvalResult {
        let keyword = self.lisp.car(form)?;
        let args = self.lisp.cdr(form)?;

        if self.lisp.symbol_matches(keyword, "lambda")? {
            self.transcribe_lambda(args, bindings, renames, def_env)
        } else if self.lisp.symbol_matches(keyword, "let")?
               || self.lisp.symbol_matches(keyword, "let*")?
               || self.lisp.symbol_matches(keyword, "letrec")?
        {
            // Let forms are macros themselves after bootstrap
            // For now, transcribe normally
            let new_keyword = self.transcribe_template(
                keyword, bindings, renames, def_env
            )?;
            let new_args = self.transcribe_template(
                args, bindings, renames, def_env
            )?;
            self.lisp.cons(new_keyword, new_args).map_err(Into::into)
        } else {
            // Unknown binding form - transcribe normally
            let new_keyword = self.transcribe_template(
                keyword, bindings, renames, def_env
            )?;
            let new_args = self.transcribe_template(
                args, bindings, renames, def_env
            )?;
            self.lisp.cons(new_keyword, new_args).map_err(Into::into)
        }
    }

    /// Transcribe lambda, renaming parameters for hygiene
    fn transcribe_lambda(
        &mut self,
        args: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> EvalResult {
        let params = self.lisp.car(args)?;
        let body = self.lisp.cdr(args)?;

        // Rename each introduced parameter
        let (new_params, new_renames) = self.rename_introduced_params(
            params, bindings, renames
        )?;

        // Transcribe body with extended renames
        let new_body = self.transcribe_template(body, bindings, new_renames, def_env)?;

        let lambda_sym = self.lisp.symbol("lambda")?;
        let inner = self.lisp.cons(new_params, new_body)?;
        self.lisp.cons(lambda_sym, inner).map_err(Into::into)
    }

    /// Rename parameters that are introduced by the macro (not from pattern)
    fn rename_introduced_params(
        &mut self,
        params: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
    ) -> Result<(ArenaIndex, ArenaIndex), EvalError> {
        let mut new_params = self.lisp.nil()?;
        let mut new_renames = renames;
        let mut current = params;

        while let Value::Cons { .. } = self.lisp.get(current)? {
            let param = self.lisp.car(current)?;

            // Check if this param is from a pattern variable
            let new_param = if self.bindings_lookup(bindings, param)?.is_some() {
                // From pattern - use as-is (already substituted)
                param
            } else {
                // Introduced by macro - generate fresh name
                let fresh = self.gensym_simple()?;
                new_renames = self.rename_extend(new_renames, param, fresh)?;
                fresh
            };

            new_params = self.lisp.cons(new_param, new_params)?;
            current = self.lisp.cdr(current)?;
        }

        let new_params = self.reverse_list(new_params)?;
        Ok((new_params, new_renames))
    }

    /// Check if symbol is a binding keyword
    fn is_binding_keyword(&self, sym: ArenaIndex) -> Result<bool, EvalError> {
        if !matches!(self.lisp.get(sym)?, Value::Symbol(_)) {
            return Ok(false);
        }
        Ok(self.lisp.symbol_matches(sym, "lambda")?
           || self.lisp.symbol_matches(sym, "let")?
           || self.lisp.symbol_matches(sym, "let*")?
           || self.lisp.symbol_matches(sym, "letrec")?
           || self.lisp.symbol_matches(sym, "letrec*")?
           || self.lisp.symbol_matches(sym, "define")?)
    }
}
```

---

## Part 5: Macro Expander

### 5.1 Main Expansion Loop

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Expand all macros in an expression
    ///
    /// This is the main entry point for macro expansion.
    /// Called before evaluation.
    pub fn expand(&mut self, expr: ArenaIndex) -> EvalResult {
        let empty_renames = self.lisp.nil()?;
        self.expand_expr(expr, empty_renames)
    }

    /// Expand expression with rename environment
    fn expand_expr(
        &mut self,
        expr: ArenaIndex,
        renames: ArenaIndex,
    ) -> EvalResult {
        match self.lisp.get(expr)? {
            Value::Symbol(_) => {
                // Apply any pending renames
                self.rename_lookup(expr, renames)
            }

            Value::Nil => self.lisp.nil(),

            Value::Cons { .. } => {
                let head = self.lisp.car(expr)?;
                let args = self.lisp.cdr(expr)?;

                // Check for special forms that affect expansion
                if let Value::Symbol(_) = self.lisp.get(head)? {
                    if self.lisp.symbol_matches(head, "quote")? {
                        // Don't expand inside quote
                        return Ok(expr);
                    }

                    if self.lisp.symbol_matches(head, "quasiquote")? {
                        // Expand only unquoted parts
                        return self.expand_quasiquote(args, renames, 1);
                    }

                    if self.lisp.symbol_matches(head, "define-syntax")? {
                        return self.expand_define_syntax(args);
                    }

                    if self.lisp.symbol_matches(head, "let-syntax")? {
                        return self.expand_let_syntax(args, renames);
                    }

                    if self.lisp.symbol_matches(head, "letrec-syntax")? {
                        return self.expand_letrec_syntax(args, renames);
                    }

                    // Check for macro invocation
                    if let Some(transformer) = self.lookup_macro(head)? {
                        let expanded = self.apply_macro(transformer, expr)?;
                        // Re-expand the result
                        return self.expand_expr(expanded, renames);
                    }
                }

                // Not a macro - expand subexpressions
                self.expand_application(expr, renames)
            }

            // Atoms pass through unchanged
            _ => Ok(expr),
        }
    }

    /// Expand a function application (non-macro)
    fn expand_application(
        &mut self,
        expr: ArenaIndex,
        renames: ArenaIndex,
    ) -> EvalResult {
        if self.lisp.get(expr)?.is_nil() {
            return self.lisp.nil();
        }

        let car = self.lisp.car(expr)?;
        let cdr = self.lisp.cdr(expr)?;

        let new_car = self.expand_expr(car, renames)?;
        let new_cdr = self.expand_application(cdr, renames)?;

        self.lisp.cons(new_car, new_cdr).map_err(Into::into)
    }

    /// Look up macro in macro environment
    fn lookup_macro(&self, name: ArenaIndex) -> Result<Option<ArenaIndex>, EvalError> {
        if !matches!(self.lisp.get(name)?, Value::Symbol(_)) {
            return Ok(None);
        }

        let mut current = self.macro_env;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let binding = self.lisp.car(current)?;
            let key = self.lisp.car(binding)?;
            if self.symbols_eq(key, name)? {
                return Ok(Some(self.lisp.cdr(binding)?));
            }
            current = self.lisp.cdr(current)?;
        }

        Ok(None)
    }

    /// Apply a macro transformer to an expression
    fn apply_macro(
        &mut self,
        transformer: ArenaIndex,
        expr: ArenaIndex,
    ) -> EvalResult {
        let (literals, rules, def_env) = self.lisp.syntax_rules_parts(transformer)?;

        // Try each rule in order
        let mut current = rules;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let rule = self.lisp.car(current)?;
            let pattern = self.lisp.car(rule)?;
            let template = self.lisp.cdr(rule)?;

            // Match against pattern (skip the keyword in both)
            let pat_args = self.lisp.cdr(pattern)?;
            let expr_args = self.lisp.cdr(expr)?;

            let empty = self.lisp.nil()?;
            if let Some(bindings) = self.match_pattern(
                pat_args, expr_args, literals, empty
            )? {
                // Match succeeded - transcribe template
                let empty_renames = self.lisp.nil()?;
                return self.transcribe_template(
                    template, bindings, empty_renames, def_env
                );
            }

            current = self.lisp.cdr(current)?;
        }

        // No rule matched
        Err(self.make_error(ErrorKind::Generic, expr)
            .with_message("no matching syntax-rules clause"))
    }
}
```

### 5.2 `define-syntax` Handler

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Handle (define-syntax name transformer)
    fn expand_define_syntax(&mut self, args: ArenaIndex) -> EvalResult {
        let name = self.lisp.car(args)?;
        let transformer_expr = self.lisp.car(self.lisp.cdr(args)?)?;

        // Parse the transformer
        let transformer = self.parse_transformer(transformer_expr)?;

        // Add to macro environment
        let binding = self.lisp.cons(name, transformer)?;
        self.macro_env = self.lisp.cons(binding, self.macro_env)?;

        // Return unspecified value
        self.lisp.nil()
    }

    /// Parse a transformer expression (syntax-rules ...)
    fn parse_transformer(&self, expr: ArenaIndex) -> EvalResult {
        let head = self.lisp.car(expr)?;

        if !self.lisp.symbol_matches(head, "syntax-rules")? {
            return Err(self.make_error(ErrorKind::Generic, expr)
                .with_message("expected (syntax-rules ...)"));
        }

        let rest = self.lisp.cdr(expr)?;
        let literals = self.lisp.car(rest)?;
        let rules_raw = self.lisp.cdr(rest)?;

        // Parse rules into (pattern . template) pairs
        let mut rules = self.lisp.nil()?;
        let mut current = rules_raw;

        while let Value::Cons { .. } = self.lisp.get(current)? {
            let rule = self.lisp.car(current)?;
            let pattern = self.lisp.car(rule)?;
            let template = self.lisp.car(self.lisp.cdr(rule)?)?;

            let pair = self.lisp.cons(pattern, template)?;
            rules = self.lisp.cons(pair, rules)?;

            current = self.lisp.cdr(current)?;
        }

        // Reverse to maintain definition order
        let rules = self.reverse_list(rules)?;

        // Create SyntaxRules value
        self.lisp.syntax_rules(literals, rules, self.global_env)
    }
}
```

### 5.3 `let-syntax` Handler

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Handle (let-syntax ((name transformer) ...) body ...)
    fn expand_let_syntax(
        &mut self,
        args: ArenaIndex,
        renames: ArenaIndex,
    ) -> EvalResult {
        let bindings = self.lisp.car(args)?;
        let body = self.lisp.cdr(args)?;

        // Save current macro environment
        let saved_macro_env = self.macro_env;

        // Add local macro bindings
        let mut current = bindings;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let binding = self.lisp.car(current)?;
            let name = self.lisp.car(binding)?;
            let transformer_expr = self.lisp.car(self.lisp.cdr(binding)?)?;

            let transformer = self.parse_transformer(transformer_expr)?;
            let macro_binding = self.lisp.cons(name, transformer)?;
            self.macro_env = self.lisp.cons(macro_binding, self.macro_env)?;

            current = self.lisp.cdr(current)?;
        }

        // Expand body
        let expanded_body = self.expand_body(body, renames)?;

        // Restore macro environment
        self.macro_env = saved_macro_env;

        // Wrap in begin if multiple expressions
        if self.list_length(expanded_body)? == 1 {
            self.lisp.car(expanded_body)
        } else {
            let begin_sym = self.lisp.symbol("begin")?;
            self.lisp.cons(begin_sym, expanded_body).map_err(Into::into)
        }
    }

    /// Expand a body (list of expressions)
    fn expand_body(
        &mut self,
        body: ArenaIndex,
        renames: ArenaIndex,
    ) -> EvalResult {
        if self.lisp.get(body)?.is_nil() {
            return self.lisp.nil();
        }

        let first = self.lisp.car(body)?;
        let rest = self.lisp.cdr(body)?;

        let expanded_first = self.expand_expr(first, renames)?;
        let expanded_rest = self.expand_body(rest, renames)?;

        self.lisp.cons(expanded_first, expanded_rest).map_err(Into::into)
    }
}
```

---

## Part 6: Integration

### 6.1 Modified Evaluation Entry Points

Update `crates/grift_eval/src/evaluator/core.rs`:

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Evaluate a string expression
    ///
    /// 1. Parse the string into an expression
    /// 2. Expand all macros
    /// 3. Evaluate the expanded expression
    pub fn eval_str(&mut self, code: &str) -> EvalResult {
        let expr = parse(self.lisp, code)?;
        self.eval(expr)
    }

    /// Evaluate an expression
    ///
    /// 1. Expand all macros
    /// 2. Evaluate the expanded expression
    pub fn eval(&mut self, expr: ArenaIndex) -> EvalResult {
        let expanded = self.expand(expr)?;
        self.eval_expanded(expanded)
    }

    /// Evaluate an already-expanded expression (internal)
    ///
    /// This is the raw evaluator without macro expansion.
    /// Use `eval()` for normal evaluation.
    pub(crate) fn eval_expanded(&mut self, expr: ArenaIndex) -> EvalResult {
        // ... existing trampoline code ...
    }
}
```

### 6.2 Loading Standard Macros

Add initialization in `Evaluator::new()`:

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    pub fn new(lisp: &'a Lisp<N>) -> Result<Self, EvalError> {
        let mut eval = Evaluator {
            lisp,
            global_env: lisp.nil()?,
            macro_env: lisp.nil()?,
            gensym_counter: 0,
            // ... other fields ...
        };

        // ... existing builtin/stdlib initialization ...

        // Load standard macros
        eval.load_standard_macros()?;

        Ok(eval)
    }

    fn load_standard_macros(&mut self) -> Result<(), EvalError> {
        // Standard macros defined in Scheme
        const STANDARD_MACROS: &str = include_str!("../../grift_parser/src/macros.scm");

        // Parse and expand each definition
        // (They define-syntax themselves into macro_env)
        for line in STANDARD_MACROS.lines() {
            // Skip comments and empty lines
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            // Note: This is simplified - real implementation needs proper
            // multi-line form handling
        }

        // Actually, parse the whole file as a sequence
        let forms = parse_all(self.lisp, STANDARD_MACROS)?;
        let mut current = forms;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let form = self.lisp.car(current)?;
            self.expand(form)?; // This handles define-syntax
            current = self.lisp.cdr(current)?;
        }

        Ok(())
    }
}
```

---

## Part 7: Standard Macros

Create `crates/grift_parser/src/macros.scm`:

```scheme
;;; Standard Scheme Macros for Grift
;;;
;;; These macros are loaded at startup and replace the built-in
;;; special forms with macro-based implementations.

;; ============================================================
;; Binding Forms
;; ============================================================

(define-syntax let
  (syntax-rules ()
    ;; Named let: (let name ((var val) ...) body ...)
    ((let name ((var val) ...) body ...)
     (letrec ((name (lambda (var ...) body ...)))
       (name val ...)))
    ;; Regular let: (let ((var val) ...) body ...)
    ((let ((var val) ...) body ...)
     ((lambda (var ...) body ...) val ...))))

(define-syntax let*
  (syntax-rules ()
    ((let* () body ...)
     (begin body ...))
    ((let* ((var val) rest ...) body ...)
     (let ((var val))
       (let* (rest ...) body ...)))))

(define-syntax letrec
  (syntax-rules ()
    ((letrec ((var val) ...) body ...)
     (let ((var #f) ...)
       (set! var val) ...
       (let () body ...)))))

(define-syntax letrec*
  (syntax-rules ()
    ((letrec* () body ...)
     (begin body ...))
    ((letrec* ((var val) rest ...) body ...)
     (let ((var #f))
       (set! var val)
       (letrec* (rest ...) body ...)))))

;; ============================================================
;; Conditionals
;; ============================================================

(define-syntax and
  (syntax-rules ()
    ((and) #t)
    ((and test) test)
    ((and test rest ...)
     (if test (and rest ...) #f))))

(define-syntax or
  (syntax-rules ()
    ((or) #f)
    ((or test) test)
    ((or test rest ...)
     (let ((temp test))
       (if temp temp (or rest ...))))))

(define-syntax when
  (syntax-rules ()
    ((when test body ...)
     (if test (begin body ...)))))

(define-syntax unless
  (syntax-rules ()
    ((unless test body ...)
     (if (not test) (begin body ...)))))

(define-syntax cond
  (syntax-rules (else =>)
    ((cond (else result ...))
     (begin result ...))
    ((cond (test => func) rest ...)
     (let ((temp test))
       (if temp (func temp) (cond rest ...))))
    ((cond (test) rest ...)
     (or test (cond rest ...)))
    ((cond (test result ...) rest ...)
     (if test
         (begin result ...)
         (cond rest ...)))
    ((cond)
     #f)))

(define-syntax case
  (syntax-rules (else =>)
    ((case key (else => func))
     (func key))
    ((case key (else result ...))
     (begin result ...))
    ((case key ((atoms ...) => func) rest ...)
     (if (memv key '(atoms ...))
         (func key)
         (case key rest ...)))
    ((case key ((atoms ...) result ...) rest ...)
     (if (memv key '(atoms ...))
         (begin result ...)
         (case key rest ...)))
    ((case key)
     #f)))

;; ============================================================
;; Iteration
;; ============================================================

(define-syntax do
  (syntax-rules ()
    ((do ((var init step ...) ...)
         (test result ...)
         body ...)
     (let loop ((var init) ...)
       (if test
           (begin (if #f #f) result ...)
           (begin
             body ...
             (loop (do-step var step ...) ...)))))))

(define-syntax do-step
  (syntax-rules ()
    ((do-step var) var)
    ((do-step var step) step)))

;; ============================================================
;; Delayed Evaluation
;; ============================================================

(define-syntax delay
  (syntax-rules ()
    ((delay expr)
     (let ((forced #f)
           (value #f))
       (lambda ()
         (if forced
             value
             (begin
               (set! value expr)
               (set! forced #t)
               value)))))))

;; force is a regular function, not a macro:
;; (define (force promise) (promise))

;; ============================================================
;; Parameter Objects (simplified)
;; ============================================================

(define-syntax parameterize
  (syntax-rules ()
    ((parameterize () body ...)
     (begin body ...))
    ((parameterize ((param val) rest ...) body ...)
     (let ((old (param)))
       (dynamic-wind
         (lambda () (param val))
         (lambda () (parameterize (rest ...) body ...))
         (lambda () (param old)))))))

;; ============================================================
;; Guard (exception handling)
;; ============================================================

(define-syntax guard
  (syntax-rules (else)
    ((guard (var clause ...) body ...)
     (call-with-current-continuation
       (lambda (guard-k)
         (with-exception-handler
           (lambda (var)
             (guard-k
               (cond clause ...)))
           (lambda () body ...)))))))
```

---

## Part 8: File Organization

```
crates/
├── grift_parser/src/
│   ├── value.rs              # Add SyntaxRules variant
│   ├── lisp.rs               # Add syntax_rules() constructor
│   ├── macros.scm            # Standard macro definitions (NEW)
│   └── lib.rs                # Export new types
│
├── grift_eval/src/
│   ├── evaluator/
│   │   ├── mod.rs            # Add macro_env, gensym_counter fields
│   │   ├── core.rs           # Modify eval entry points
│   │   ├── forms.rs          # Remove derived forms (Phase 2)
│   │   ├── expand.rs         # Macro expander (NEW)
│   │   ├── pattern.rs        # Pattern matching (NEW)
│   │   └── transcribe.rs     # Template transcription (NEW)
│   ├── continuation.rs       # (unchanged initially)
│   └── lib.rs                # Re-export expander
```

---

## Part 9: Implementation Phases

### Phase 1: Foundation (1-2 days)

1. Add `SyntaxRules` variant to `Value`
2. Add `syntax_rules()` and `syntax_rules_parts()` to `Lisp`
3. Add `macro_env` and `gensym_counter` to `Evaluator`
4. Implement `gensym()` and `gensym_simple()`
5. Add helper functions: `bindings_extend`, `bindings_lookup`, `rename_extend`, `rename_lookup`

### Phase 2: Pattern Matching (2-3 days)

1. Implement `match_pattern()` for basic patterns (no ellipsis)
2. Implement `is_literal()` and `collect_pattern_vars()`
3. Test with simple patterns
4. Add ellipsis support to `match_pattern()`
5. Implement `match_ellipsis_pattern()`

### Phase 3: Template Transcription (2-3 days)

1. Implement `transcribe_template()` for basic templates
2. Implement `transcribe_symbol()` and `transcribe_list()`
3. Add ellipsis support to transcription
4. Implement `transcribe_binding_form()` for hygiene
5. Test with simple macros (`when`, `unless`)

### Phase 4: Expander Integration (1-2 days)

1. Implement `expand()` entry point
2. Implement `expand_define_syntax()`
3. Implement `parse_transformer()`
4. Implement `apply_macro()`
5. Modify `eval()` to call expander first

### Phase 5: Standard Macros (1-2 days)

1. Create `macros.scm` with standard definitions
2. Implement macro loading in `Evaluator::new()`
3. Test all standard macros
4. Handle edge cases (empty bodies, nested macros)

### Phase 6: Cleanup (1-2 days)

1. Remove derived forms from `forms.rs`
2. Remove corresponding continuation types
3. Update tests
4. Performance testing
5. Documentation

### Phase 7: Binding Form Macros (Partial - workaround applied)

**Status: Partially Complete - `let` and `let*` implemented using workaround**

The goal of this phase is to replace `let`, `let*`, `letrec`, and `letrec*` special forms with macro-based implementations. This would complete the R7RS macro-based derived forms.

#### Implemented Workaround

Due to the nested ellipsis pattern bug, we use a recursive approach with dotted pair patterns instead of the standard R7RS patterns:

```scheme
;; Helper macro for single binding
(define-syntax %let-binding
  (syntax-rules ()
    ((%let-binding (name val) body ...)
     ((lambda (name) body ...) val))))

;; Recursive let using dotted pair pattern
(define-syntax let
  (syntax-rules ()
    ((let () body ...)
     (begin body ...))
    ((let (first-binding . rest-bindings) body ...)
     (%let-binding first-binding 
       (let rest-bindings body ...)))))

;; let* - same approach
(define-syntax let*
  (syntax-rules ()
    ((let* () body ...)
     (begin body ...))
    ((let* (first-binding . rest-bindings) body ...)
     (%let-binding first-binding
       (let* rest-bindings body ...)))))
```

This works because:
1. Dotted pair patterns `(first . rest)` correctly match the first element and remaining list
2. Each binding is processed individually through `%let-binding`
3. Recursive expansion handles multiple bindings

#### Bug Fix: Lambda Parameter Substitution

During implementation, a bug was discovered in `rename_introduced_params` where pattern variables in lambda parameter lists were not being substituted. The fix:

```rust
// Before (buggy):
let new_param = if self.bindings_lookup(bindings, param)?.is_some() {
    param  // Wrong: returns the pattern variable symbol
} else { ... }

// After (fixed):
let new_param = if let Some(bound_val) = self.bindings_lookup(bindings, param)? {
    bound_val  // Correct: returns the bound value
} else { ... }
```

This fix was essential for `%let-binding` to work correctly.

#### Remaining Work

1. **Named let**: Not supported by macro, still uses special form
2. **letrec/letrec***: Still use special forms (need set! in macro expansion)

#### Original Blocker: Nested Ellipsis Pattern Bug

When attempting to implement using standard R7RS patterns, a bug was discovered in the ellipsis pattern matching for nested structures.

**Bug Description:**

For a pattern like `((name val) ...)` matching against `((x 5) (y 6) (z 7))`:

- **Expected**: `name → (x y z)`, `val → (5 6 7)`
- **Actual**: `name → (x (y 6) (z 7))`, `val → (5 (y 6) (z 7))`

Only the first element is correctly extracted. Subsequent elements retain their full structure instead of being deconstructed.

**Symptoms:**

1. `(let ((x 5) (y 6)) (+ x y))` fails with "no matching syntax-rules clause"
2. Custom test macros show incorrect binding values:
   ```scheme
   (define-syntax test-mac 
     (syntax-rules () 
       ((test-mac ((a) ...) body) 
        (quote (a ...)))))
   (test-mac ((1) (2) (3)) 42)
   ;; Expected: (1 2 3)
   ;; Actual: (1 (2) (3))
   ```

**Root Cause Analysis:**

The bug appears to be in `match_ellipsis_pattern` or `merge_ellipsis_bindings` in `expand.rs`. Despite extensive debugging, the exact cause was not identified. The pattern matching loop appears to:

1. Correctly extract the first element on the first iteration
2. On subsequent iterations, either:
   - Pass the wrong element to `match_pattern`, OR
   - Merge the bindings incorrectly

**Future Work:**

1. Add debug tracing to `match_ellipsis_pattern` to trace the exact values at each step
2. Create unit tests for the pattern matching functions in isolation
3. Compare against a reference implementation (e.g., Chibi Scheme or Guile)

---

## Part 10: Testing Strategy

### Unit Tests

```rust
#[test]
fn test_simple_pattern_match() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Pattern: (x y)
    // Expression: (1 2)
    // Expected: ((x . 1) (y . 2))
}

#[test]
fn test_ellipsis_pattern() {
    // Pattern: (x ...)
    // Expression: (1 2 3)
    // Expected: ((x . (1 2 3)))
}

#[test]
fn test_hygiene_or_macro() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // This should NOT capture user's 'temp' variable
    eval.eval_str("(define temp 42)").unwrap();
    let result = eval.eval_str("(or #f temp)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
}

#[test]
fn test_let_macro() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    let result = eval.eval_str("(let ((x 1) (y 2)) (+ x y))").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(3));
}

#[test]
fn test_named_let() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    let result = eval.eval_str(
        "(let loop ((n 5) (acc 1))
           (if (= n 0) acc (loop (- n 1) (* n acc))))"
    ).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(120));
}
```

### Integration Tests

```rust
#[test]
fn test_standard_library_with_macros() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Test that stdlib functions work with macro-based forms
    let result = eval.eval_str(
        "(let ((xs '(1 2 3 4 5)))
           (filter (lambda (x) (> x 2)) xs))"
    ).unwrap();
    // Should return (3 4 5)
}
```

---

## Appendix A: Memory Analysis

| Component | Arena Slots | Notes |
|-----------|-------------|-------|
| `SyntaxRules` value | 1 per macro | 2 inline ArenaIndex (same as Lambda) |
| `rules_env` cons cell | 1 per macro | (rules . definition_env) |
| Compiled rules | ~2-4 per rule | (pattern . template) pairs |
| Gensym symbols | 1 per expansion | Interned, reused |
| Expanded code | Same as input | No overhead |
| **Total for 20 macros** | ~120 slots | ~1% of 10K arena |

### Comparison: Before vs After

| Metric | Before (forms.rs) | After (macros) |
|--------|-------------------|----------------|
| Rust code lines | ~500 | ~200 (expander) |
| Continuation types | 15+ derived | 0 derived |
| Arena overhead | 0 | ~120 slots |
| Extensibility | Requires Rust | Pure Scheme |

---

## Appendix B: Known Limitations

1. **No `syntax-case`**: Only pattern-based macros, no procedural macros
2. **No identifier macros**: Can't make a symbol expand to something else
3. **No `syntax-parameterize`**: No hygiene-bending utilities
4. **Single expansion phase**: No separate visit/expand phases

These limitations are acceptable for R7RS-small compatibility and can be addressed in future versions if needed.

---

## Appendix C: Future Enhancements

1. **Source locations**: Track macro expansion origin for error messages
2. **`syntax-case`**: Add procedural macro support
3. **Syntax objects**: Wrap expressions with metadata
4. **Module system**: Per-module macro environments
5. **Macro debugging**: Expansion tracing/stepping
