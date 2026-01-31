# Syntax-Rules Implementation Plan

## Executive Summary

This document provides a comprehensive plan for implementing R7RS-compliant `syntax-rules` hygienic macros in grift, followed by migration of special forms to macro-based implementations where appropriate.

The implementation uses **Kohlbecker hygiene with three paint contexts** — a minimal, correct approach that requires no gensyms, syntax objects, or phase separation.

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

### 2.1 Painted Symbol Representation

**Option A: New Value Variant (Recommended)**

Add a new `Value` variant for painted symbols:

```rust
pub enum Value {
    // ... existing variants ...

    /// Painted symbol for hygienic macros
    /// - symbol: ArenaIndex to the underlying Symbol
    /// - paint: usize identifier for the paint context
    PaintedSymbol { symbol: ArenaIndex, paint: usize },
}
```

**Rationale**:
- Preserves backward compatibility (unpainted symbols still work)
- Maintains the 24-byte Value invariant (2 × usize)
- Allows gradual migration
- Paint 0 symbols can remain as regular `Symbol` (optimization)

**Alternative Option B: Inline Paint in Symbol**

Modify the Symbol representation to always include paint:

```rust
// Change from:
Symbol(ArenaIndex),  // Points to String

// To:
Symbol { name: ArenaIndex, paint: usize },  // Points to String + paint
```

**Trade-offs**:
- Simpler conceptually
- Breaks symbol interning (same name, different paint = different symbol)
- All code paths must handle paint
- Still fits 24-byte invariant

**Recommendation**: Option A (PaintedSymbol variant) for cleaner separation and backward compatibility during development.

### 2.2 Paint Counter

Add a global paint counter to the evaluator:

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

### 2.3 Macro Value Type

Add a new Value variant for compiled macros:

```rust
pub enum Value {
    // ... existing variants ...

    /// Compiled syntax-rules macro transformer
    /// - def_paint: The paint assigned at definition time
    /// - literals: ArenaIndex to list of literal identifiers
    /// - rules: ArenaIndex to list of (pattern . template) pairs
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
pub fn parse_symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
    // For now, all parsed symbols have implicit paint 0
    // The evaluator treats Symbol as paint 0
    self.lisp.symbol(name)
}
```

No parser changes needed initially — unpainted `Symbol` is treated as paint 0 by convention.

### 3.2 Syntax-Rules Parsing

The `syntax-rules` form is recognized by the evaluator, not the parser. The parser treats it as a regular list structure:

```scheme
(syntax-rules (<literal>*)
  (<pattern> <template>)*)
```

Parsed as:
```
(syntax-rules
  (<literal-list>)
  (<rule-1>)
  (<rule-2>)
  ...)
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

    // Bind in environment
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
    // (except pattern variables, which are identified during pattern analysis)
    let painted_rules = self.repaint_rules(rules, def_paint)?;

    // Create the Macro value
    let data = self.lisp.cons(literals, painted_rules)?;
    self.lisp.alloc(Value::Macro { def_paint, data })
}
```

### 4.4 Repainting at Definition Time

```rust
fn repaint_rules(&self, rules: ArenaIndex, paint: usize)
    -> ArenaResult<ArenaIndex>
{
    // Recursively walk the rules structure and repaint all symbols
    self.repaint_tree(rules, paint)
}

fn repaint_tree(&self, expr: ArenaIndex, paint: usize)
    -> ArenaResult<ArenaIndex>
{
    match self.lisp.get(expr)? {
        Value::Symbol(_) => {
            // Repaint this symbol
            self.lisp.alloc(Value::PaintedSymbol { symbol: expr, paint })
        }
        Value::PaintedSymbol { symbol, .. } => {
            // Already painted - repaint with new paint
            self.lisp.alloc(Value::PaintedSymbol { symbol, paint })
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
    if let Value::Symbol(_) | Value::PaintedSymbol { .. } = self.lisp.get(car)? {
        if let Ok(macro_val) = self.env_lookup(env, car) {
            if let Value::Macro { def_paint, data } = self.lisp.get(macro_val)? {
                // Expand the macro and evaluate the result
                let expanded = self.expand_macro(expr, def_paint, data, env)?;
                return Ok(TrampolineState::Eval { expr: expanded, env });
            }
        }
    }

    // ... existing function application code ...
}
```

### 4.6 Macro Expansion

```rust
fn expand_macro(&mut self, form: ArenaIndex, def_paint: usize, data: ArenaIndex, env: ArenaIndex)
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
    // Check if it's the ellipsis
    if self.lisp.symbol_matches(id, "...")? {
        return Ok(false);
    }

    // Check if it's the wildcard
    if self.lisp.symbol_matches(id, "_")? {
        return Ok(false);
    }

    // Check if it's in the literals list
    let mut current = literals;
    while !self.lisp.get(current)?.is_nil() {
        let lit = self.lisp.car(current)?;
        if self.symbol_name_eq(id, lit)? {
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
    /// Using a simple alist stored in arena
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
        // Pattern variable - bind to form
        (_, Value::Symbol(_) | Value::PaintedSymbol { .. })
            if self.is_pattern_variable(pattern, literals)? =>
        {
            self.add_binding(bindings, pattern, form)?;
            Ok(true)
        }

        // Wildcard - matches anything, no binding
        (_, Value::Symbol(_) | Value::PaintedSymbol { .. })
            if self.lisp.symbol_matches(pattern, "_")? =>
        {
            Ok(true)
        }

        // Literal - must match by name
        (Value::Symbol(_) | Value::PaintedSymbol { .. },
         Value::Symbol(_) | Value::PaintedSymbol { .. })
            if !self.is_pattern_variable(pattern, literals)? =>
        {
            // Match by name only (paint doesn't matter for literals)
            self.symbol_name_eq(form, pattern)
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
fn match_ellipsis(&self, form: ArenaIndex, pattern: ArenaIndex,
                  literals: ArenaIndex, bindings: &mut PatternBindings)
    -> Result<bool, EvalError>
{
    // Pattern is (pat-before... ellipsis-pattern ... pat-after...)
    // For simplicity, we handle the common case: (pat ... ) at end

    let pat_head = self.lisp.car(pattern)?;
    let pat_rest = self.lisp.cdr(pattern)?;

    // Get the ellipsis pattern and remaining pattern
    let ellipsis_pat = self.lisp.car(pat_rest)?;
    let after_ellipsis = self.lisp.cdr(pat_rest)?;  // Should have ... then maybe more

    // For now, require ellipsis is followed only by ... symbol
    // This handles (pat ...) but not (pat ... more-stuff)

    // Collect pattern variables in the ellipsis pattern
    let vars = self.collect_pattern_vars(ellipsis_pat, literals)?;

    // Match zero or more repetitions
    let mut current_form = form;
    let mut repetitions: Vec<ArenaIndex> = Vec::new();  // Temp storage for matches

    while !self.lisp.get(current_form)?.is_nil() {
        let form_elem = self.lisp.car(current_form)?;

        let mut temp_bindings = PatternBindings {
            bindings: self.lisp.nil()?,
            ellipsis_bindings: self.lisp.nil()?,
        };

        if !self.match_pattern_impl(form_elem, ellipsis_pat, literals, &mut temp_bindings)? {
            break;
        }

        repetitions.push(temp_bindings.bindings);
        current_form = self.lisp.cdr(current_form)?;
    }

    // Store the collected matches for each variable
    for var in vars {
        let values: Vec<ArenaIndex> = repetitions.iter()
            .filter_map(|b| self.lookup_binding(*b, var).ok())
            .collect();
        let value_list = self.list_from_vec(&values)?;
        self.add_ellipsis_binding(bindings, var, value_list)?;
    }

    Ok(true)
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
        // Symbol - check if pattern variable or introduced identifier
        Value::Symbol(_) => {
            // Unpainted symbol from template - this shouldn't happen after repainting
            // but handle it: treat as def_paint
            if let Some(val) = self.lookup_pattern_binding(bindings, template)? {
                Ok(val)  // Pattern variable - preserve input paint
            } else {
                // Introduced identifier - repaint with use_paint
                self.lisp.alloc(Value::PaintedSymbol { symbol: template, paint: use_paint })
            }
        }

        Value::PaintedSymbol { symbol, paint } => {
            if let Some(val) = self.lookup_pattern_binding(bindings, template)? {
                Ok(val)  // Pattern variable - preserve input paint
            } else if paint == def_paint {
                Ok(template)  // Definition-site identifier - keep def_paint
            } else {
                // Repaint with use_paint
                self.lisp.alloc(Value::PaintedSymbol { symbol, paint: use_paint })
            }
        }

        // List - expand recursively, handle ellipsis
        Value::Cons { car, cdr } => {
            // Check for ellipsis
            if self.is_ellipsis_template(cdr)? {
                return self.expand_ellipsis_template(car, bindings, def_paint, use_paint);
            }

            let expanded_car = self.expand_template(car, bindings, def_paint, use_paint)?;
            let expanded_cdr = self.expand_template(cdr, bindings, def_paint, use_paint)?;
            self.lisp.cons(expanded_car, expanded_cdr)
        }

        // Other values pass through unchanged
        _ => Ok(template)
    }
}
```

### 6.2 Ellipsis Template Expansion

```rust
fn expand_ellipsis_template(&self, pattern: ArenaIndex, bindings: &PatternBindings,
                            def_paint: usize, use_paint: usize)
    -> Result<ArenaIndex, EvalError>
{
    // Find pattern variables in the ellipsis template
    let vars = self.collect_pattern_vars_in_template(pattern)?;

    // Get the number of repetitions from the first ellipsis-bound variable
    let first_var = self.lisp.car(vars)?;
    let repetition_values = self.lookup_ellipsis_binding(bindings, first_var)?;

    // Build output list
    let mut result = self.lisp.nil()?;
    let count = self.list_length(repetition_values)?;

    for i in (0..count).rev() {
        // Create bindings for this iteration
        let iter_bindings = self.make_iteration_bindings(bindings, vars, i)?;

        // Expand template with iteration bindings
        let expanded = self.expand_template(pattern, &iter_bindings, def_paint, use_paint)?;
        result = self.lisp.cons(expanded, result)?;
    }

    Ok(result)
}
```

---

## Part 7: Environment Changes

### 7.1 Painted Symbol Lookup

Modify `env_lookup` to handle painted symbols:

```rust
fn env_lookup(&self, env: ArenaIndex, name: ArenaIndex) -> EvalResult {
    let (name_str, name_paint) = self.get_symbol_name_and_paint(name)?;

    let mut current = env;
    loop {
        match self.lisp.get(current)? {
            Value::Nil => {
                // Try global with same paint check
                return self.env_lookup_global_painted(name_str, name_paint);
            }
            Value::Cons { car, cdr } => {
                if let Value::Cons { car: bound_name, cdr: bound_value } = self.lisp.get(car)? {
                    let (bound_str, bound_paint) = self.get_symbol_name_and_paint(bound_name)?;

                    // Match by BOTH name AND paint
                    if self.lisp.symbol_name_eq(name_str, bound_str)?
                        && name_paint == bound_paint
                    {
                        return Ok(bound_value);
                    }
                }
                current = cdr;
            }
            _ => return Err(self.make_error(ErrorKind::Generic, name)),
        }
    }
}

fn get_symbol_name_and_paint(&self, sym: ArenaIndex) -> ArenaResult<(ArenaIndex, usize)> {
    match self.lisp.get(sym)? {
        Value::Symbol(name) => Ok((sym, 0)),  // Unpainted = paint 0
        Value::PaintedSymbol { symbol, paint } => Ok((symbol, paint)),
        _ => Err(ArenaError::InvalidIndex),
    }
}
```

### 7.2 Global Environment with Paint

The global environment needs to store bindings with paint. For primitives and stdlib:

```rust
fn init_global_env(&mut self) -> Result<(), EvalError> {
    // Builtins get paint 0 (available to user code)
    for &builtin in Builtin::ALL {
        let name = self.lisp.symbol(builtin.name())?;  // paint 0 implicit
        let val = self.lisp.builtin(builtin)?;
        self.global_env = self.env_extend(self.global_env, name, val)?;
    }

    // ... same for stdlib ...
}
```

This means user code (paint 0) can access builtins. Macro definitions can also access them because they look up with their def_paint first, then fall back.

### 7.3 Fallback Lookup for Macro-Introduced Identifiers

When a macro introduces an identifier like `if`, it has `def_paint`. But we want it to find the global `if` (paint 0). Solution:

```rust
fn env_lookup_painted(&self, env: ArenaIndex, name_str: ArenaIndex, paint: usize)
    -> EvalResult
{
    // First try exact match (name + paint)
    if let Ok(val) = self.env_lookup_exact(env, name_str, paint) {
        return Ok(val);
    }

    // For non-zero paint, fall back to paint 0 in global env only
    // This allows macro-introduced identifiers to find global bindings
    if paint != 0 {
        if let Ok(val) = self.env_lookup_global_exact(name_str, 0) {
            return Ok(val);
        }
    }

    Err(self.make_error(ErrorKind::UnboundVariable, name_str))
}
```

This is the key insight: definition-site identifiers (paint_def) can find global bindings (paint 0), but user bindings (paint 0) cannot accidentally shadow them because the macro's identifiers have different paint.

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
1. `define-syntax` assigns `paint_def = 1` to swap!
2. `let`, `temp`, `set!` in template get paint 1
3. User code has `temp`, `x`, `y` with paint 0
4. `(swap! x y)` triggers expansion with `paint_use = 2`
5. Pattern matching: `a` → `x` (paint 0), `b` → `y` (paint 0)
6. Template expansion:
   - `let` stays paint 1 (def_paint)
   - `temp` in `(let ((temp a))` stays paint 1
   - `a` expands to `x` (paint 0 preserved)
   - `set!` stays paint 1
   - etc.
7. Result: macro's `temp` (paint 1) ≠ user's `temp` (paint 0)

---

## Part 9: Limitations (Intentional)

1. **No macro-defining macros** — Macros cannot define other macros
2. **No `syntax-case`** — Only pattern-based `syntax-rules`
3. **No procedural macros** — No arbitrary Scheme code in macro transformers
4. **No phase separation** — Single evaluation phase
5. **Limited ellipsis** — Only `...` at end of patterns, not nested
6. **No `syntax-error`** — Macro errors are generic

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
- **`letrec`** — Complex binding semantics
- **`do`** — Complex iteration, keep for now

### 10.3 Migration Strategy

1. **Phase 1**: Implement `syntax-rules` with all infrastructure
2. **Phase 2**: Add macro versions of `when`, `unless`, `and`, `or`
3. **Phase 3**: Test extensively, compare with special form versions
4. **Phase 4**: Remove special form versions, use macros exclusively
5. **Phase 5**: Add `cond` and `case` as macros
6. **Phase 6**: Add `let` and `let*` as macros (optional, may keep special forms for performance)

---

## Part 11: Implementation Phases

### Phase 1: Foundation (Core Infrastructure)

**Files to modify:**
- `crates/grift_parser/src/value.rs` — Add `PaintedSymbol` and `Macro` variants
- `crates/grift_eval/src/evaluator.rs` — Add paint counter, symbol paint handling
- `crates/grift_parser/src/lisp.rs` — Add helpers for painted symbols

**Tasks:**
1. Add `Value::PaintedSymbol { symbol, paint }` variant
2. Add `Value::Macro { def_paint, data }` variant
3. Implement `Trace` for new variants
4. Add `next_paint` counter to Evaluator
5. Implement `get_symbol_name_and_paint()` helper
6. Implement `repaint_tree()` for repainting expressions

**Estimated LOC:** ~150

### Phase 2: Macro Definition

**Files to modify:**
- `crates/grift_eval/src/evaluator.rs` — Add define-syntax handling

**Tasks:**
1. Add `define-syntax` recognition in `step_eval_list`
2. Implement `eval_define_syntax()`
3. Implement `compile_syntax_rules()`
4. Implement `repaint_rules()` for definition-time repainting

**Estimated LOC:** ~100

### Phase 3: Pattern Matching

**Files to modify:**
- `crates/grift_eval/src/evaluator.rs` or new module `crates/grift_eval/src/macros.rs`

**Tasks:**
1. Define `PatternBindings` structure
2. Implement `is_pattern_variable()`
3. Implement `match_pattern()` — main entry point
4. Implement `match_pattern_impl()` — recursive matcher
5. Implement `match_ellipsis()` — ellipsis handling
6. Add binding management helpers

**Estimated LOC:** ~200

### Phase 4: Template Expansion

**Files to modify:**
- `crates/grift_eval/src/evaluator.rs` or `crates/grift_eval/src/macros.rs`

**Tasks:**
1. Implement `expand_template()` — main expansion
2. Implement ellipsis template expansion
3. Implement binding lookup during expansion
4. Implement use_paint assignment

**Estimated LOC:** ~150

### Phase 5: Environment Integration

**Files to modify:**
- `crates/grift_eval/src/evaluator.rs` — Modify env_lookup

**Tasks:**
1. Modify `env_lookup()` to check paint
2. Implement fallback to global paint 0
3. Update `env_extend()` if needed
4. Ensure macro application calls expand first

**Estimated LOC:** ~50

### Phase 6: Testing

**Files to modify:**
- `crates/grift_eval/tests/macro_tests.rs` (new)

**Tasks:**
1. Basic macro definition and expansion tests
2. Hygiene tests (user shadowing doesn't break macros)
3. Pattern matching tests
4. Ellipsis tests
5. Nested macro tests
6. Error case tests

**Estimated LOC:** ~300 (tests)

### Phase 7: Special Form Migration

**Files to modify:**
- `crates/grift_parser/src/stdlib.scm` or new `crates/grift_parser/src/macros.scm`
- `crates/grift_eval/src/evaluator.rs` — Remove migrated special forms

**Tasks:**
1. Define `when`, `unless` as macros
2. Define `and`, `or` as macros
3. Test against previous special form behavior
4. Remove special form handlers
5. Optionally migrate `cond`, `let`, `let*`

**Estimated LOC:** ~50 (macros) + removal of ~100 lines

---

## Part 12: Total Effort Estimate

| Phase | Description | LOC |
|-------|-------------|-----|
| 1 | Foundation | ~150 |
| 2 | Macro Definition | ~100 |
| 3 | Pattern Matching | ~200 |
| 4 | Template Expansion | ~150 |
| 5 | Environment Integration | ~50 |
| 6 | Testing | ~300 |
| 7 | Special Form Migration | ~50 new, -100 removed |
| **Total** | | **~800-900 new lines** |

Special form migration will remove more code than it adds, resulting in a net simplification.

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

---

## Appendix A: R7RS Compliance Notes

From R7RS Section 4.3 (Macros):

> "Pattern variables match arbitrary input elements and are used to refer to elements of the input in the template."

> "Identifiers that appear in the template but are not pattern variables or the identifier ... are inserted into the output as literal identifiers."

> "The syntactic keyword of a macro may shadow variable bindings, and local variable bindings may shadow syntactic bindings."

Our implementation satisfies these requirements through the three-context paint model.

---

## Appendix B: Alternative Approaches Considered

### Gensym-Based Approach
- Generate unique symbols for introduced bindings
- Simpler conceptually but breaks lexical scoping intuition
- Doesn't properly handle definition-site identifiers
- Rejected: less principled than paint

### Syntax Objects (Racket-style)
- Wrap every expression with metadata
- Full phase separation
- Maximum flexibility
- Rejected: overkill for R5RS-level macros, much more complex

### Sets of Scopes (Racket 2.0)
- Most sophisticated approach
- Handles macro-generating macros correctly
- Rejected: far too complex for our needs

The three-context paint model is the minimal correct solution for our requirements.

---

## Appendix C: References

- [Kohlbecker et al., "Hygienic Macro Expansion"](https://dl.acm.org/doi/10.1145/319838.319859) (1986)
- [R7RS Specification, Section 4.3](https://small.r7rs.org/)
- [Binding as Sets of Scopes](https://www.cs.utah.edu/plt/scope-sets/) (for future reference)
