# Syntax-Rules Implementation Plan

## Executive Summary

This document provides a comprehensive plan for implementing R7RS-compliant `syntax-rules` hygienic macros in grift, followed by migration of special forms to macro-based implementations where appropriate.

The implementation uses **Kohlbecker hygiene with three paint contexts** — a minimal, correct approach that requires no gensyms, syntax objects, or phase separation.

**Key architectural decision**: Symbols carry paint inline (`Symbol { name, paint }`) rather than using a separate wrapper type. This requires removing symbol interning as a prerequisite.

---

## Part 1: Theoretical Foundation

### The Three-Context Paint Model

Traditional approaches often use two contexts (user code vs expansion), which leads to hacks around primitives and library functions. The correct minimal approach uses **three contexts**:

| Context | Paint Source | When Assigned |
|---------|-------------|---------------|
| User code | `paint_user = 0` | At parse time |
| Macro definition | `paint_def = fresh` | At `define-syntax` |
| Macro expansion | `paint_use = fresh` | Per macro invocation |

### Why Three Contexts?

Consider this problematic case with only two contexts:

```scheme
(define-syntax my-if
  (syntax-rules ()
    ((my-if test then else)
     (if test then else))))  ; <-- This 'if' needs protection

(let ((if (lambda (x y z) z)))
  (my-if #t 'yes 'no))  ; Should be 'yes, not 'no
```

With two contexts:
- The `if` in the macro template has paint 0 (same as user code)
- User's `let` binding shadows it
- Macro breaks unexpectedly

With three contexts:
- The `if` in the macro template has `paint_def` (fresh at define-syntax time)
- User's `let` binding has paint 0
- Different paints → no collision → macro works correctly

### Core Hygiene Rules

1. **Pattern variables preserve input paint** — When a pattern matches user code, the matched portions retain their original paint

2. **Introduced identifiers get expansion paint** — New identifiers created during expansion get `paint_use`

3. **Definition-site identifiers keep definition paint** — Identifiers from the macro template that aren't pattern variables keep their `paint_def`

4. **Environment lookup uses (name, paint) pairs** — Two identifiers with the same name but different paint are distinct

---

## Part 2: Data Structure Changes

### 2.1 Symbol Representation with Inline Paint

Modify the Symbol variant to carry paint inline:

```rust
// Change from:
Symbol(ArenaIndex),  // Points to String

// To:
Symbol { name: ArenaIndex, paint: usize },  // Points to String + paint
```

**Properties**:
- Every symbol carries its paint context
- Same name with different paint = different identifiers
- Fits 24-byte Value invariant (1 ArenaIndex + 1 usize = 16 bytes payload)
- Conceptually simple: all symbols are painted, period

**Implications**:
- Symbol interning must be removed (same name, different paint = different symbols)
- All symbol-handling code must be updated
- `eq?` on symbols now compares both name AND paint
- `symbol_matches` helper needs updating for name-only comparisons

### 2.2 Why Remove Symbol Interning?

With inline paint:
- `'foo` (paint 0) and `'foo` (paint 1) are **different symbols**
- Interning assumes same name → same ArenaIndex
- This assumption breaks with painted symbols
- Attempting to keep interning would require a `(name, paint) → ArenaIndex` table
- Simpler to just remove interning entirely

**Performance impact**: Minimal for typical Scheme programs. Interning primarily helps with:
- `eq?` on symbols (now requires string comparison OR paint-aware lookup)
- Memory for repeated symbols (now each occurrence is separate)

For a macro-enabled interpreter, correctness matters more than micro-optimizations.

### 2.3 Paint Counter

Add a paint counter to the evaluator:

```rust
pub struct Evaluator<'a, const N: usize> {
    // ... existing fields ...

    /// Next paint value to allocate (starts at 1, paint 0 = user code)
    next_paint: usize,
}

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Allocate a fresh paint value
    fn alloc_paint(&mut self) -> usize {
        let paint = self.next_paint;
        self.next_paint += 1;
        paint
    }
}
```

### 2.4 Macro Value Type

Add a new Value variant for compiled macros:

```rust
pub enum Value {
    // ... existing variants ...

    /// Compiled syntax-rules macro transformer
    /// - def_paint: The paint assigned at definition time
    /// - data: ArenaIndex to cons cell (literals . rules)
    Macro { def_paint: usize, data: ArenaIndex },
}
```

Where `data` points to a cons cell `(literals . rules)`.

---

## Part 3: Parser Changes

### 3.1 Parse-Time Paint Assignment

All identifiers parsed from user input get paint 0:

```rust
// In parser.rs, when creating symbols:
fn parse_symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
    self.lisp.symbol_with_paint(name, 0)  // User code = paint 0
}
```

### 3.2 New Symbol Creation API

```rust
impl<const N: usize> Lisp<N> {
    /// Create a symbol with explicit paint
    pub fn symbol_with_paint(&self, name: &str, paint: usize) -> ArenaResult<ArenaIndex> {
        let name_str = self.string(name)?;
        self.alloc(Value::Symbol { name: name_str, paint })
    }

    /// Create a symbol with paint 0 (convenience for user code)
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        self.symbol_with_paint(name, 0)
    }

    /// Get the name string from a symbol (ignoring paint)
    pub fn symbol_name(&self, sym: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(sym)? {
            Value::Symbol { name, .. } => Ok(name),
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    /// Get name and paint from a symbol
    pub fn symbol_parts(&self, sym: ArenaIndex) -> ArenaResult<(ArenaIndex, usize)> {
        match self.get(sym)? {
            Value::Symbol { name, paint } => Ok((name, paint)),
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    /// Check if symbol name matches a string (ignores paint)
    pub fn symbol_matches(&self, sym: ArenaIndex, target: &str) -> ArenaResult<bool> {
        let name = self.symbol_name(sym)?;
        self.string_eq_str(name, target)
    }

    /// Check if two symbols have the same name (ignores paint)
    pub fn symbol_name_eq(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        let name_a = self.symbol_name(a)?;
        let name_b = self.symbol_name(b)?;
        self.string_eq(name_a, name_b)
    }

    /// Check if two symbols are identical (same name AND paint)
    pub fn symbol_eq(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        match (self.get(a)?, self.get(b)?) {
            (Value::Symbol { name: na, paint: pa },
             Value::Symbol { name: nb, paint: pb }) => {
                Ok(pa == pb && self.string_eq(na, nb)?)
            }
            _ => Ok(false),
        }
    }

    /// Create a copy of a symbol with different paint
    pub fn repaint_symbol(&self, sym: ArenaIndex, new_paint: usize) -> ArenaResult<ArenaIndex> {
        let name = self.symbol_name(sym)?;
        self.alloc(Value::Symbol { name, paint: new_paint })
    }
}
```

### 3.3 Syntax-Rules Parsing

The `syntax-rules` form is recognized by the evaluator, not the parser. The parser treats it as a regular list structure:

```scheme
(syntax-rules (<literal>*)
  (<pattern> <template>)*)
```

---

## Part 4: Evaluator Changes

### 4.1 New Special Forms

Add handling for these special forms in `step_eval_list`:

```rust
// define-syntax - Top-level macro definition
if self.lisp.symbol_matches(car, "define-syntax")? {
    return self.eval_define_syntax(cdr, env);
}

// let-syntax - Local macro bindings
if self.lisp.symbol_matches(car, "let-syntax")? {
    return self.eval_let_syntax(cdr, env);
}

// letrec-syntax - Recursive local macro bindings
if self.lisp.symbol_matches(car, "letrec-syntax")? {
    return self.eval_letrec_syntax(cdr, env);
}
```

### 4.2 define-syntax Implementation

```rust
fn eval_define_syntax(&mut self, args: ArenaIndex, env: ArenaIndex)
    -> Result<TrampolineState, EvalError>
{
    // (define-syntax name transformer)
    let name = self.lisp.car(args)?;
    let transformer_expr = self.lisp.car(self.lisp.cdr(args)?)?;

    // Allocate fresh paint for this macro definition
    let def_paint = self.alloc_paint();

    // Compile the transformer (must be syntax-rules)
    let macro_val = self.compile_syntax_rules(transformer_expr, def_paint)?;

    // Bind in environment (name keeps its original paint for lookup)
    self.define(name, macro_val, env)?;

    Ok(TrampolineState::Return { val: name })
}
```

### 4.3 syntax-rules Compilation

```rust
fn compile_syntax_rules(&mut self, expr: ArenaIndex, def_paint: usize)
    -> Result<ArenaIndex, EvalError>
{
    // (syntax-rules (literals...) (pattern template)...)

    // Verify head is 'syntax-rules'
    let head = self.lisp.car(expr)?;
    if !self.lisp.symbol_matches(head, "syntax-rules")? {
        return Err(self.make_error(ErrorKind::Generic, expr));
    }

    let rest = self.lisp.cdr(expr)?;
    let literals = self.lisp.car(rest)?;  // List of literal identifiers
    let rules = self.lisp.cdr(rest)?;     // List of (pattern template) pairs

    // Repaint all identifiers in the rules with def_paint
    let painted_rules = self.repaint_tree(rules, def_paint)?;

    // Create the Macro value
    let data = self.lisp.cons(literals, painted_rules)?;
    self.lisp.alloc(Value::Macro { def_paint, data })
}
```

### 4.4 Repainting at Definition Time

```rust
fn repaint_tree(&self, expr: ArenaIndex, paint: usize) -> ArenaResult<ArenaIndex> {
    match self.lisp.get(expr)? {
        Value::Symbol { name, .. } => {
            // Repaint this symbol with new paint
            self.lisp.alloc(Value::Symbol { name, paint })
        }
        Value::Cons { car, cdr } => {
            let new_car = self.repaint_tree(car, paint)?;
            let new_cdr = self.repaint_tree(cdr, paint)?;
            self.lisp.cons(new_car, new_cdr)
        }
        _ => Ok(expr)  // Numbers, booleans, etc. unchanged
    }
}
```

### 4.5 Macro Application

When applying a form, check if the operator is a macro:

```rust
fn step_eval_list(&mut self, car: ArenaIndex, cdr: ArenaIndex, expr: ArenaIndex, env: ArenaIndex)
    -> Result<TrampolineState, EvalError>
{
    // ... existing special form checks ...

    // Check if head resolves to a macro
    if let Value::Symbol { .. } = self.lisp.get(car)? {
        if let Ok(macro_val) = self.env_lookup(env, car) {
            if let Value::Macro { def_paint, data } = self.lisp.get(macro_val)? {
                // Expand the macro and evaluate the result
                let expanded = self.expand_macro(expr, def_paint, data)?;
                return Ok(TrampolineState::Eval { expr: expanded, env });
            }
        }
    }

    // ... existing function application code ...
}
```

### 4.6 Macro Expansion

```rust
fn expand_macro(&mut self, form: ArenaIndex, def_paint: usize, data: ArenaIndex)
    -> Result<ArenaIndex, EvalError>
{
    // Allocate fresh paint for this expansion
    let use_paint = self.alloc_paint();

    // Extract literals and rules
    let (literals, rules) = self.lisp.car_cdr(data)?;

    // Try each rule in order
    let mut current_rule = rules;
    while !self.lisp.get(current_rule)?.is_nil() {
        let rule = self.lisp.car(current_rule)?;
        let pattern = self.lisp.car(rule)?;
        let template = self.lisp.car(self.lisp.cdr(rule)?)?;

        // Try to match pattern against form
        if let Some(bindings) = self.match_pattern(form, pattern, literals)? {
            // Pattern matched - expand template with bindings
            return self.expand_template(template, &bindings, def_paint, use_paint);
        }

        current_rule = self.lisp.cdr(current_rule)?;
    }

    // No pattern matched
    Err(self.make_error(ErrorKind::Generic, form))
}
```

---

## Part 5: Pattern Matching

### 5.1 Pattern Language

```
<pattern> ::= <pattern-identifier>     ; matches anything, binds value
           |  <literal-identifier>     ; matches literal exactly
           |  (<pattern>*)             ; matches list
           |  (<pattern>* . <pattern>) ; matches improper list
           |  (<pattern>* <pattern> <ellipsis>)  ; matches zero or more
           |  #(<pattern>*)            ; matches vector (future)
           |  <constant>               ; matches equal constant

<ellipsis> ::= ...
```

### 5.2 Pattern Identifier Detection

An identifier in a pattern is a **pattern variable** if:
1. It's not the macro name (first element of pattern)
2. It's not in the literals list
3. It's not `...` (ellipsis)
4. It's not `_` (wildcard)

```rust
fn is_pattern_variable(&self, id: ArenaIndex, literals: ArenaIndex) -> ArenaResult<bool> {
    // Must be a symbol
    if !matches!(self.lisp.get(id)?, Value::Symbol { .. }) {
        return Ok(false);
    }

    // Check if it's the ellipsis
    if self.lisp.symbol_matches(id, "...")? {
        return Ok(false);
    }

    // Check if it's the wildcard
    if self.lisp.symbol_matches(id, "_")? {
        return Ok(false);
    }

    // Check if it's in the literals list (by name)
    let mut current = literals;
    while !self.lisp.get(current)?.is_nil() {
        let lit = self.lisp.car(current)?;
        if self.lisp.symbol_name_eq(id, lit)? {
            return Ok(false);
        }
        current = self.lisp.cdr(current)?;
    }

    Ok(true)
}
```

### 5.3 Pattern Matching Algorithm

```rust
struct PatternBindings {
    /// Map from pattern variable name to matched value
    /// Using a simple alist stored in arena: ((name . value) ...)
    bindings: ArenaIndex,
    /// Map from pattern variable name to list of matched values (for ellipsis)
    ellipsis_bindings: ArenaIndex,
}

fn match_pattern(&self, form: ArenaIndex, pattern: ArenaIndex, literals: ArenaIndex)
    -> Result<Option<PatternBindings>, EvalError>
{
    let mut bindings = PatternBindings {
        bindings: self.lisp.nil()?,
        ellipsis_bindings: self.lisp.nil()?,
    };

    // Skip the macro name in both form and pattern
    let form_args = self.lisp.cdr(form)?;
    let pattern_args = self.lisp.cdr(pattern)?;

    if self.match_pattern_impl(form_args, pattern_args, literals, &mut bindings)? {
        Ok(Some(bindings))
    } else {
        Ok(None)
    }
}

fn match_pattern_impl(&self, form: ArenaIndex, pattern: ArenaIndex,
                      literals: ArenaIndex, bindings: &mut PatternBindings)
    -> Result<bool, EvalError>
{
    match (self.lisp.get(form)?, self.lisp.get(pattern)?) {
        // Pattern variable - bind to form (preserving form's paint)
        (_, Value::Symbol { .. }) if self.is_pattern_variable(pattern, literals)? => {
            self.add_binding(bindings, pattern, form)?;
            Ok(true)
        }

        // Wildcard - matches anything, no binding
        (_, Value::Symbol { .. }) if self.lisp.symbol_matches(pattern, "_")? => {
            Ok(true)
        }

        // Literal - must match by name (paint doesn't matter for literals)
        (Value::Symbol { .. }, Value::Symbol { .. })
            if !self.is_pattern_variable(pattern, literals)? =>
        {
            self.lisp.symbol_name_eq(form, pattern)
        }

        // List patterns
        (Value::Cons { car: form_car, cdr: form_cdr },
         Value::Cons { car: pat_car, cdr: pat_cdr }) =>
        {
            // Check for ellipsis in cdr
            if self.has_ellipsis(pat_cdr)? {
                return self.match_ellipsis(form, pattern, literals, bindings);
            }

            // Regular list matching
            if !self.match_pattern_impl(form_car, pat_car, literals, bindings)? {
                return Ok(false);
            }
            self.match_pattern_impl(form_cdr, pat_cdr, literals, bindings)
        }

        // Both nil - match
        (Value::Nil, Value::Nil) => Ok(true),

        // Constants - must be equal
        _ => self.lisp.equal(form, pattern),
    }
}
```

### 5.4 Ellipsis Matching

```rust
fn has_ellipsis(&self, list: ArenaIndex) -> ArenaResult<bool> {
    if self.lisp.get(list)?.is_nil() {
        return Ok(false);
    }
    let first = self.lisp.car(list)?;
    if let Value::Symbol { .. } = self.lisp.get(first)? {
        return self.lisp.symbol_matches(first, "...");
    }
    Ok(false)
}

fn match_ellipsis(&self, form: ArenaIndex, pattern: ArenaIndex,
                  literals: ArenaIndex, bindings: &mut PatternBindings)
    -> Result<bool, EvalError>
{
    // Pattern is (stuff... ellipsis-pat ...)
    // For simplicity, handle common case: single element before ...

    let pat_list = self.lisp.cdr(pattern)?;  // Skip macro name in pattern
    let ellipsis_pat = self.lisp.car(pat_list)?;

    // Collect pattern variables in the ellipsis pattern
    let vars = self.collect_pattern_vars(ellipsis_pat, literals)?;

    // Match zero or more repetitions from form
    let form_list = self.lisp.cdr(form)?;  // Skip macro name in form
    let mut current_form = form_list;
    let mut all_matches: Vec<ArenaIndex> = Vec::new();

    while !self.lisp.get(current_form)?.is_nil() {
        let form_elem = self.lisp.car(current_form)?;

        let mut temp_bindings = PatternBindings {
            bindings: self.lisp.nil()?,
            ellipsis_bindings: self.lisp.nil()?,
        };

        if !self.match_pattern_impl(form_elem, ellipsis_pat, literals, &mut temp_bindings)? {
            break;
        }

        all_matches.push(temp_bindings.bindings);
        current_form = self.lisp.cdr(current_form)?;
    }

    // Store collected matches for each pattern variable
    let mut var_cursor = vars;
    while !self.lisp.get(var_cursor)?.is_nil() {
        let var = self.lisp.car(var_cursor)?;
        let var_name = self.lisp.symbol_name(var)?;

        // Collect all values for this variable across iterations
        let mut values = self.lisp.nil()?;
        for match_bindings in all_matches.iter().rev() {
            if let Some(val) = self.lookup_in_bindings(*match_bindings, var_name)? {
                values = self.lisp.cons(val, values)?;
            }
        }

        self.add_ellipsis_binding(bindings, var, values)?;
        var_cursor = self.lisp.cdr(var_cursor)?;
    }

    Ok(true)
}

fn collect_pattern_vars(&self, pattern: ArenaIndex, literals: ArenaIndex)
    -> Result<ArenaIndex, EvalError>
{
    let mut vars = self.lisp.nil()?;

    fn collect_impl(this: &Self, pat: ArenaIndex, lits: ArenaIndex, vars: &mut ArenaIndex)
        -> Result<(), EvalError>
    {
        match this.lisp.get(pat)? {
            Value::Symbol { .. } if this.is_pattern_variable(pat, lits)? => {
                *vars = this.lisp.cons(pat, *vars)?;
            }
            Value::Cons { car, cdr } => {
                collect_impl(this, car, lits, vars)?;
                collect_impl(this, cdr, lits, vars)?;
            }
            _ => {}
        }
        Ok(())
    }

    collect_impl(self, pattern, literals, &mut vars)?;
    Ok(vars)
}
```

---

## Part 6: Template Expansion

### 6.1 Template Expansion Algorithm

```rust
fn expand_template(&self, template: ArenaIndex, bindings: &PatternBindings,
                   def_paint: usize, use_paint: usize)
    -> Result<ArenaIndex, EvalError>
{
    match self.lisp.get(template)? {
        Value::Symbol { name, paint } => {
            // Check if this is a pattern variable (by name)
            if let Some(val) = self.lookup_pattern_binding(bindings, name)? {
                // Pattern variable - return matched value (preserves input paint)
                Ok(val)
            } else if paint == def_paint {
                // Definition-site identifier - keep def_paint
                Ok(template)
            } else {
                // Other identifier - repaint with use_paint
                self.lisp.alloc(Value::Symbol { name, paint: use_paint })
            }
        }

        // List - expand recursively, handle ellipsis
        Value::Cons { car, cdr } => {
            // Check for ellipsis
            if self.is_ellipsis_template(cdr)? {
                let expanded_list = self.expand_ellipsis_template(car, cdr, bindings, def_paint, use_paint)?;
                return Ok(expanded_list);
            }

            let expanded_car = self.expand_template(car, bindings, def_paint, use_paint)?;
            let expanded_cdr = self.expand_template(cdr, bindings, def_paint, use_paint)?;
            self.lisp.cons(expanded_car, expanded_cdr)
        }

        // Other values pass through unchanged
        _ => Ok(template)
    }
}

fn lookup_pattern_binding(&self, bindings: &PatternBindings, name: ArenaIndex)
    -> Result<Option<ArenaIndex>, EvalError>
{
    // First check regular bindings
    if let Some(val) = self.lookup_in_bindings(bindings.bindings, name)? {
        return Ok(Some(val));
    }
    Ok(None)
}

fn lookup_in_bindings(&self, alist: ArenaIndex, name: ArenaIndex)
    -> Result<Option<ArenaIndex>, EvalError>
{
    let mut current = alist;
    while !self.lisp.get(current)?.is_nil() {
        let pair = self.lisp.car(current)?;
        let (bound_sym, bound_val) = self.lisp.car_cdr(pair)?;
        let bound_name = self.lisp.symbol_name(bound_sym)?;

        if self.lisp.string_eq(name, bound_name)? {
            return Ok(Some(bound_val));
        }
        current = self.lisp.cdr(current)?;
    }
    Ok(None)
}
```

### 6.2 Ellipsis Template Expansion

```rust
fn is_ellipsis_template(&self, cdr: ArenaIndex) -> ArenaResult<bool> {
    if self.lisp.get(cdr)?.is_nil() {
        return Ok(false);
    }
    let first = self.lisp.car(cdr)?;
    if let Value::Symbol { .. } = self.lisp.get(first)? {
        return self.lisp.symbol_matches(first, "...");
    }
    Ok(false)
}

fn expand_ellipsis_template(&self, pattern: ArenaIndex, rest: ArenaIndex,
                            bindings: &PatternBindings,
                            def_paint: usize, use_paint: usize)
    -> Result<ArenaIndex, EvalError>
{
    // rest is (... maybe-more-stuff)
    // For now, assume ... is at the end

    // Find pattern variables in the ellipsis template
    let vars = self.collect_template_vars(pattern)?;

    // Get repetition count from first ellipsis-bound variable
    let first_var = self.lisp.car(vars)?;
    let first_name = self.lisp.symbol_name(first_var)?;
    let repetition_values = self.lookup_ellipsis_values(bindings, first_name)?;
    let count = self.list_length(repetition_values)?;

    // Build output list
    let mut result = self.lisp.nil()?;

    for i in (0..count).rev() {
        // Create iteration bindings: for each ellipsis var, get the i-th value
        let iter_bindings = self.make_iteration_bindings(bindings, vars, i)?;

        // Expand template with iteration bindings
        let expanded = self.expand_template(pattern, &iter_bindings, def_paint, use_paint)?;
        result = self.lisp.cons(expanded, result)?;
    }

    // Continue expanding the rest (after ...)
    let after_ellipsis = self.lisp.cdr(rest)?;
    if !self.lisp.get(after_ellipsis)?.is_nil() {
        let expanded_rest = self.expand_template(after_ellipsis, bindings, def_paint, use_paint)?;
        result = self.append_lists(result, expanded_rest)?;
    }

    Ok(result)
}

fn lookup_ellipsis_values(&self, bindings: &PatternBindings, name: ArenaIndex)
    -> Result<ArenaIndex, EvalError>
{
    self.lookup_in_bindings(bindings.ellipsis_bindings, name)?
        .ok_or_else(|| self.make_error(ErrorKind::UnboundVariable, name))
}

fn make_iteration_bindings(&self, bindings: &PatternBindings, vars: ArenaIndex, index: usize)
    -> Result<PatternBindings, EvalError>
{
    let mut new_bindings = self.lisp.nil()?;

    let mut var_cursor = vars;
    while !self.lisp.get(var_cursor)?.is_nil() {
        let var = self.lisp.car(var_cursor)?;
        let var_name = self.lisp.symbol_name(var)?;

        // Get the list of values for this variable
        let values = self.lookup_ellipsis_values(bindings, var_name)?;

        // Get the i-th value
        let value = self.list_ref(values, index)?;

        // Add to new bindings
        let pair = self.lisp.cons(var, value)?;
        new_bindings = self.lisp.cons(pair, new_bindings)?;

        var_cursor = self.lisp.cdr(var_cursor)?;
    }

    Ok(PatternBindings {
        bindings: new_bindings,
        ellipsis_bindings: self.lisp.nil()?,  // No nested ellipsis for now
    })
}
```

---

## Part 7: Environment Changes

### 7.1 Painted Symbol Lookup

Modify `env_lookup` to handle painted symbols:

```rust
fn env_lookup(&self, env: ArenaIndex, name_sym: ArenaIndex) -> EvalResult {
    let (name_str, name_paint) = self.lisp.symbol_parts(name_sym)?;

    let mut current = env;
    loop {
        match self.lisp.get(current)? {
            Value::Nil => {
                // Try global with fallback
                return self.env_lookup_global_with_fallback(name_str, name_paint);
            }
            Value::Cons { car, cdr } => {
                if let Value::Cons { car: bound_sym, cdr: bound_value } = self.lisp.get(car)? {
                    let (bound_name, bound_paint) = self.lisp.symbol_parts(bound_sym)?;

                    // Match by BOTH name AND paint
                    if self.lisp.string_eq(name_str, bound_name)? && name_paint == bound_paint {
                        return Ok(bound_value);
                    }
                }
                current = cdr;
            }
            _ => return Err(self.make_error(ErrorKind::Generic, name_sym)),
        }
    }
}
```

### 7.2 Global Environment with Paint Fallback

```rust
fn env_lookup_global_with_fallback(&self, name_str: ArenaIndex, paint: usize)
    -> EvalResult
{
    // First try exact match (name + paint) in global env
    if let Ok(val) = self.env_lookup_global_exact(name_str, paint) {
        return Ok(val);
    }

    // For non-zero paint, fall back to paint 0 in global env
    // This allows macro-introduced identifiers to find global bindings
    if paint != 0 {
        if let Ok(val) = self.env_lookup_global_exact(name_str, 0) {
            return Ok(val);
        }
    }

    Err(self.make_error(ErrorKind::UnboundVariable, /* reconstruct symbol */))
}

fn env_lookup_global_exact(&self, name_str: ArenaIndex, paint: usize) -> EvalResult {
    let mut current = self.global_env;
    while !self.lisp.get(current)?.is_nil() {
        let pair = self.lisp.car(current)?;
        let (bound_sym, bound_val) = self.lisp.car_cdr(pair)?;
        let (bound_name, bound_paint) = self.lisp.symbol_parts(bound_sym)?;

        if self.lisp.string_eq(name_str, bound_name)? && paint == bound_paint {
            return Ok(bound_val);
        }
        current = self.lisp.cdr(current)?;
    }
    Err(self.make_error(ErrorKind::UnboundVariable, name_str))
}
```

### 7.3 Global Environment Initialization

```rust
fn init_global_env(&mut self) -> Result<(), EvalError> {
    // Builtins get paint 0 (available to user code and macros via fallback)
    for &builtin in Builtin::ALL {
        let name = self.lisp.symbol_with_paint(builtin.name(), 0)?;
        let val = self.lisp.builtin(builtin)?;
        self.global_env = self.env_extend(self.global_env, name, val)?;
    }

    // Same for stdlib
    for &stdlib_fn in StdLib::ALL {
        let name = self.lisp.symbol_with_paint(stdlib_fn.name(), 0)?;
        let val = self.lisp.stdlib(stdlib_fn)?;
        self.global_env = self.env_extend(self.global_env, name, val)?;
    }

    Ok(())
}
```

---

## Part 8: Hygiene Guarantees

With this implementation:

| Scenario | Result | Why |
|----------|--------|-----|
| User shadows macro-introduced binding | Hygienic | Different paint values |
| Macro captures user binding | Preserved | Pattern vars keep user paint |
| Macro uses `if` | Works | Falls back to global paint 0 |
| User redefines `if` locally | Macro unaffected | Macro's `if` has def_paint |
| Nested macro expansion | Hygienic | Each expansion gets fresh paint |
| Library functions in macros | Work | Same fallback mechanism |

### Example Walkthrough

```scheme
(define-syntax swap!
  (syntax-rules ()
    ((swap! a b)
     (let ((temp a))
       (set! a b)
       (set! b temp)))))

(let ((temp 1) (x 2) (y 3))
  (swap! x y)
  (list temp x y))  ; => (1 3 2)
```

Expansion trace:
1. `define-syntax` assigns `paint_def = 1` to swap!'s template
2. In template: `let` (paint 1), `temp` (paint 1), `set!` (paint 1)
3. User code: `temp` (paint 0), `x` (paint 0), `y` (paint 0)
4. `(swap! x y)` triggers expansion with `paint_use = 2`
5. Pattern matching: `a` binds to `x` (paint 0), `b` binds to `y` (paint 0)
6. Template expansion:
   - `let` stays paint 1 (def_paint) → finds global `let` via fallback
   - `temp` stays paint 1 → creates new binding with paint 1
   - `a` expands to `x` (paint 0 preserved)
   - `set!` stays paint 1 → finds global `set!` via fallback
7. Result: macro's `temp` (paint 1) ≠ user's `temp` (paint 0)

---

## Part 9: Limitations (Intentional)

1. **No macro-defining macros** — Macros cannot define other macros
2. **No `syntax-case`** — Only pattern-based `syntax-rules`
3. **No procedural macros** — No arbitrary Scheme code in macro transformers
4. **No phase separation** — Single evaluation phase
5. **Limited ellipsis** — Only `...` at end of patterns, not nested
6. **No `syntax-error`** — Macro errors are generic
7. **No symbol interning** — Each symbol allocation is independent

These are acceptable for R5RS-level macro support.

---

## Part 10: Special Form Migration

After implementing `syntax-rules`, these special forms can become macros:

### 10.1 Immediate Migration Candidates

```scheme
;; when - trivial
(define-syntax when
  (syntax-rules ()
    ((when test body ...)
     (if test (begin body ...) #f))))

;; unless - trivial
(define-syntax unless
  (syntax-rules ()
    ((unless test body ...)
     (if (not test) (begin body ...) #f))))

;; and - short-circuit
(define-syntax and
  (syntax-rules ()
    ((and) #t)
    ((and test) test)
    ((and test rest ...)
     (if test (and rest ...) #f))))

;; or - short-circuit
(define-syntax or
  (syntax-rules ()
    ((or) #f)
    ((or test) test)
    ((or test rest ...)
     (let ((x test))
       (if x x (or rest ...))))))

;; cond - multi-way conditional
(define-syntax cond
  (syntax-rules (else =>)
    ((cond (else result ...))
     (begin result ...))
    ((cond (test => func) clause ...)
     (let ((temp test))
       (if temp (func temp) (cond clause ...))))
    ((cond (test) clause ...)
     (or test (cond clause ...)))
    ((cond (test result ...) clause ...)
     (if test (begin result ...) (cond clause ...)))))

;; let - using lambda
(define-syntax let
  (syntax-rules ()
    ((let ((name val) ...) body ...)
     ((lambda (name ...) body ...) val ...))
    ((let tag ((name val) ...) body ...)
     (letrec ((tag (lambda (name ...) body ...)))
       (tag val ...)))))

;; let* - sequential binding
(define-syntax let*
  (syntax-rules ()
    ((let* () body ...)
     (begin body ...))
    ((let* ((name val) rest ...) body ...)
     (let ((name val))
       (let* (rest ...) body ...)))))
```

### 10.2 Keep as Special Forms

These should remain as evaluator-handled special forms:

- **`quote`** — Fundamental, must prevent evaluation
- **`if`** — Fundamental conditional
- **`lambda`** — Creates closures with environment capture
- **`define`** — Modifies environment
- **`set!`** — Mutation
- **`begin`** — Can become macro but simpler as special form
- **`quasiquote`** — Complex nested evaluation
- **`letrec`** — Complex binding semantics (needed for bootstrapping)
- **`do`** — Complex iteration, keep for now

### 10.3 Migration Strategy

1. **Phase 0**: Remove symbol interning
2. **Phase 1**: Implement `syntax-rules` with all infrastructure
3. **Phase 2**: Add macro versions of `when`, `unless`, `and`, `or`
4. **Phase 3**: Test extensively, compare with special form versions
5. **Phase 4**: Remove special form versions, use macros exclusively
6. **Phase 5**: Add `cond` and `case` as macros
7. **Phase 6**: Add `let` and `let*` as macros (optional)

---

## Part 11: Implementation Phases

### Phase 0: Remove Symbol Interning

**Rationale**: With inline paint, symbol interning no longer makes sense. Same name with different paint must produce different symbols.

**Files to modify:**
- `crates/grift_parser/src/value.rs` — Change Symbol variant
- `crates/grift_parser/src/lisp.rs` — Remove intern table, update symbol creation
- `crates/grift_eval/src/evaluator.rs` — Update symbol comparisons

**Tasks:**

1. **Change Symbol variant**:
   ```rust
   // From:
   Symbol(ArenaIndex),

   // To:
   Symbol { name: ArenaIndex, paint: usize },
   ```

2. **Remove intern table infrastructure**:
   - Remove reserved slot 3 usage for intern table
   - Remove `intern_table_lookup()` and related methods
   - Remove `intern_table_lookup_bytes()`
   - Remove `get_intern_table_root()` / `set_intern_table_root()`

3. **Update symbol creation**:
   ```rust
   // Remove interning logic from:
   pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex>
   pub fn symbol_from_bytes(&self, bytes: &[u8]) -> ArenaResult<ArenaIndex>

   // Replace with direct allocation:
   pub fn symbol_with_paint(&self, name: &str, paint: usize) -> ArenaResult<ArenaIndex> {
       let name_str = self.string(name)?;
       self.alloc(Value::Symbol { name: name_str, paint })
   }

   pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
       self.symbol_with_paint(name, 0)
   }
   ```

4. **Add symbol helper methods**:
   ```rust
   pub fn symbol_name(&self, sym: ArenaIndex) -> ArenaResult<ArenaIndex>
   pub fn symbol_paint(&self, sym: ArenaIndex) -> ArenaResult<usize>
   pub fn symbol_parts(&self, sym: ArenaIndex) -> ArenaResult<(ArenaIndex, usize)>
   pub fn symbol_name_eq(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool>
   pub fn repaint_symbol(&self, sym: ArenaIndex, paint: usize) -> ArenaResult<ArenaIndex>
   ```

5. **Update eq? semantics**:
   - `eq?` on symbols now compares both name AND paint
   - For backward compatibility, may need a `symbol-name=?` procedure

6. **Update Trace implementation** for GC:
   ```rust
   Value::Symbol { name, .. } => {
       // Trace the name string
       mark(name);
   }
   ```

7. **Update parser** to use paint 0:
   ```rust
   // In parse_atom or wherever symbols are created:
   self.lisp.symbol_with_paint(name, 0)
   ```

8. **Update all symbol_matches calls** — these should still work since they compare by name only

**Estimated LOC changes:**
- Removals: ~80 lines (intern table code)
- Additions: ~50 lines (new helpers)
- Modifications: ~30 lines (scattered updates)
- **Net: -30 to 0 lines**

**Testing:**
- All existing tests should pass (symbols with paint 0 behave like before)
- `(eq? 'foo 'foo)` may now return `#f` if symbols aren't the same allocation
- Add test for `symbol_name_eq` helper

---

### Phase 1: Foundation (Macro Infrastructure)

**Files to modify:**
- `crates/grift_parser/src/value.rs` — Add `Macro` variant
- `crates/grift_eval/src/evaluator.rs` — Add paint counter, repaint_tree

**Tasks:**
1. Add `Value::Macro { def_paint: usize, data: ArenaIndex }` variant
2. Implement `Trace` for Macro variant
3. Add `next_paint: usize` field to Evaluator (initialize to 1)
4. Implement `alloc_paint()` method
5. Implement `repaint_tree()` for repainting expressions

**Estimated LOC:** ~80

---

### Phase 2: Macro Definition

**Files to modify:**
- `crates/grift_eval/src/evaluator.rs` — Add define-syntax handling

**Tasks:**
1. Add `define-syntax` recognition in `step_eval_list`
2. Implement `eval_define_syntax()`
3. Implement `compile_syntax_rules()`

**Estimated LOC:** ~80

---

### Phase 3: Pattern Matching

**Files to modify:**
- `crates/grift_eval/src/evaluator.rs` or new `crates/grift_eval/src/macros.rs`

**Tasks:**
1. Define `PatternBindings` structure
2. Implement `is_pattern_variable()`
3. Implement `match_pattern()` — main entry point
4. Implement `match_pattern_impl()` — recursive matcher
5. Implement `has_ellipsis()` and `match_ellipsis()`
6. Implement `collect_pattern_vars()`
7. Add binding management helpers

**Estimated LOC:** ~200

---

### Phase 4: Template Expansion

**Files to modify:**
- `crates/grift_eval/src/evaluator.rs` or `crates/grift_eval/src/macros.rs`

**Tasks:**
1. Implement `expand_template()` — main expansion
2. Implement `is_ellipsis_template()`
3. Implement `expand_ellipsis_template()`
4. Implement binding lookup helpers
5. Implement `make_iteration_bindings()`

**Estimated LOC:** ~150

---

### Phase 5: Environment Integration

**Files to modify:**
- `crates/grift_eval/src/evaluator.rs` — Modify env_lookup

**Tasks:**
1. Modify `env_lookup()` to use `symbol_parts()` and check paint
2. Implement `env_lookup_global_with_fallback()`
3. Implement `env_lookup_global_exact()`
4. Add macro application in `step_eval_list`

**Estimated LOC:** ~60

---

### Phase 6: Testing

**Files to create/modify:**
- `crates/grift_eval/tests/macro_tests.rs` (new)

**Tasks:**
1. Basic macro definition and expansion tests
2. Hygiene tests (user shadowing doesn't break macros)
3. Pattern variable binding tests
4. Ellipsis matching and expansion tests
5. Nested macro tests
6. Literal identifier tests
7. Error case tests

**Estimated LOC:** ~300 (tests)

---

### Phase 7: Special Form Migration

**Files to modify:**
- `crates/grift_parser/src/stdlib.scm` or new `crates/grift_parser/src/macros.scm`
- `crates/grift_eval/src/evaluator.rs` — Remove migrated special forms

**Tasks:**
1. Define `when`, `unless` as macros
2. Define `and`, `or` as macros
3. Test against previous special form behavior
4. Remove special form handlers from evaluator
5. Optionally migrate `cond`, `let`, `let*`

**Estimated LOC:** ~50 (macros) + removal of ~100 lines

---

## Part 12: Total Effort Estimate

| Phase | Description | LOC Change |
|-------|-------------|------------|
| 0 | Remove Symbol Interning | ~-30 to 0 |
| 1 | Foundation | ~+80 |
| 2 | Macro Definition | ~+80 |
| 3 | Pattern Matching | ~+200 |
| 4 | Template Expansion | ~+150 |
| 5 | Environment Integration | ~+60 |
| 6 | Testing | ~+300 |
| 7 | Special Form Migration | ~+50, -100 |
| **Total** | | **~700-800 new lines** |

The removal of interning and special form migration results in a net code reduction in core logic, offset by new macro infrastructure.

---

## Part 13: Success Criteria

1. **Basic hygiene test passes:**
```scheme
(define-syntax swap!
  (syntax-rules ()
    ((swap! a b)
     (let ((temp a))
       (set! a b)
       (set! b temp)))))

(let ((temp 1) (x 2) (y 3))
  (swap! x y)
  (list temp x y))  ; Must return (1 3 2)
```

2. **Macro using library functions works:**
```scheme
(define-syntax double-list
  (syntax-rules ()
    ((double-list x)
     (list x x))))

(let ((list +))  ; Shadow 'list'
  (double-list 5))  ; Must return (5 5), not 10
```

3. **Nested macros work:**
```scheme
(define-syntax my-when
  (syntax-rules ()
    ((my-when test body ...)
     (if test (begin body ...) #f))))

(my-when #t
  (my-when #t 'nested))  ; Must return 'nested
```

4. **Ellipsis works:**
```scheme
(define-syntax my-list
  (syntax-rules ()
    ((my-list x ...)
     (list x ...))))

(my-list 1 2 3)  ; Must return (1 2 3)
```

5. **Literals work:**
```scheme
(define-syntax my-cond
  (syntax-rules (else)
    ((my-cond (else result)) result)
    ((my-cond (test result)) (if test result #f))))

(my-cond (else 'default))  ; Must return 'default
```

---

## Appendix A: R7RS Compliance Notes

From R7RS Section 4.3 (Macros):

> "Pattern variables match arbitrary input elements and are used to refer to elements of the input in the template."

> "Identifiers that appear in the template but are not pattern variables or the identifier ... are inserted into the output as literal identifiers."

> "The syntactic keyword of a macro may shadow variable bindings, and local variable bindings may shadow syntactic bindings."

Our implementation satisfies these requirements through the three-context paint model.

---

## Appendix B: Symbol Interning Removal Details

### Current Interning Implementation

The current implementation uses an association list stored in reserved arena slot 3:

```rust
// Reserved slots:
// 0: Nil
// 1: True
// 2: False
// 3: Intern table root (cons cell)

// Intern table structure: ((name1 . symbol1) (name2 . symbol2) ...)
```

### Why Interning is Incompatible with Painted Symbols

1. **Identity assumption**: Interning assumes `(eq? 'foo 'foo) => #t` because both resolve to the same ArenaIndex

2. **With paint**: `'foo` with paint 0 and `'foo` with paint 1 must be different symbols for hygiene

3. **Interning + paint** would require a `(name, paint) → ArenaIndex` mapping, which:
   - Grows unboundedly as new paints are allocated
   - Provides no benefit (we don't need fast identity comparison across paints)
   - Complicates the implementation

4. **Clean solution**: Remove interning entirely. Symbol comparison becomes structural (string comparison + paint comparison).

### Impact on eq?

After removing interning:

```scheme
(eq? 'foo 'foo)  ; May return #f (different allocations)
(eqv? 'foo 'foo) ; Returns #t (same name, same paint)
(equal? 'foo 'foo) ; Returns #t
```

This is actually more correct — `eq?` tests identity (same object), not equality. Two separately allocated symbols with the same name are equal but not identical.

For backward compatibility, ensure `eqv?` and `equal?` compare symbols by name+paint.

---

## Appendix C: Alternative Approaches Considered

### PaintedSymbol Variant (Option A)
- Add a separate `PaintedSymbol { symbol, paint }` variant
- Keep `Symbol(ArenaIndex)` for unpainted (paint 0) symbols
- Allows gradual migration
- **Rejected**: Two representations for symbols is confusing; all code must handle both cases

### Gensym-Based Approach
- Generate unique symbols for introduced bindings
- Simpler conceptually but breaks lexical scoping intuition
- Doesn't properly handle definition-site identifiers
- **Rejected**: Less principled than paint

### Syntax Objects (Racket-style)
- Wrap every expression with metadata
- Full phase separation
- Maximum flexibility
- **Rejected**: Overkill for R5RS-level macros, much more complex

### Sets of Scopes (Racket 2.0)
- Most sophisticated approach
- Handles macro-generating macros correctly
- **Rejected**: Far too complex for our needs

The inline paint model with three contexts is the minimal correct solution for our requirements.

---

## Appendix D: References

- [Kohlbecker et al., "Hygienic Macro Expansion"](https://dl.acm.org/doi/10.1145/319838.319859) (1986)
- [R7RS Specification, Section 4.3](https://small.r7rs.org/)
- [Binding as Sets of Scopes](https://www.cs.utah.edu/plt/scope-sets/) (for future reference)
