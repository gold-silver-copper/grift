# Extending Scheme Macros in Grift

This document describes the design and implementation approach for extending Grift's macro system beyond the current `syntax-rules` implementation, including:

1. **Recursive Pattern Extensions**: Enhancing `syntax-rules` with recursive helpers for deeper pattern matching
2. **`syntax-case` (psyntax)**: Implementing procedural macros for the `no_std`, `no_alloc` arena-based runtime

---

## Implementation Status

| Phase | Feature | Status |
|-------|---------|--------|
| 1 | Recursive pattern documentation | ✅ Complete |
| 1 | Helper macro examples | ✅ Complete |
| 1 | Tests for complex patterns | ✅ Complete |
| 1 | Improved nested ellipsis handling | ✅ Complete |
| 2 | `Value::Syntax` variant | ✅ Complete |
| 2 | `syntax`, `syntax_parts`, `syntax_to_datum` methods | ✅ Complete |
| 2 | Mark infrastructure | ✅ Complete |
| 3-6 | `syntax-case` and related features | ⏳ Pending |

**Last updated**: February 2025

---

## Table of Contents

1. [Overview](#overview)
2. [Part 1: Extending syntax-rules](#part-1-extending-syntax-rules)
   - [1.1 Recursive Helper Macros](#11-recursive-helper-macros)
   - [1.2 Utilities for Nested Patterns](#12-utilities-for-nested-patterns)
   - [1.3 Focus Area: Quasiquote](#13-focus-area-quasiquote)
   - [1.4 Focus Area: do Loops](#14-focus-area-do-loops)
   - [1.5 Testing Recursive Patterns](#15-testing-recursive-patterns)
   - [1.6 Comparison with syntax-case](#16-comparison-with-syntax-case)
3. [Part 2: Implementing syntax-case (psyntax)](#part-2-implementing-syntax-case-psyntax)
   - [2.1 Syntax Object Design](#21-syntax-object-design)
   - [2.2 Procedural Matching](#22-procedural-matching)
   - [2.3 Hygiene Support](#23-hygiene-support)
   - [2.4 Evaluator Integration](#24-evaluator-integration)
   - [2.5 Focus Area: Quasiquote as Procedural Macro](#25-focus-area-quasiquote-as-procedural-macro)
   - [2.6 Testing and Optimization](#26-testing-and-optimization)
   - [2.7 syntax-rules vs syntax-case Comparison](#27-syntax-rules-vs-syntax-case-comparison)
4. [Implementation Roadmap](#implementation-roadmap)
5. [Appendix: Arena Memory Considerations](#appendix-arena-memory-considerations)

---

## Overview

### Current State

Grift implements `syntax-rules` following R7RS Section 4.3.2, with:

- Pattern matching with literals, wildcards (`_`), and ellipsis (`...`)
- Template transcription with hygiene via gensym
- Evaluation-time macro expansion
- Standard macros: `let`, `let*`, `letrec`, `cond`, `case`, `do`, `when`, `unless`, `and`, `or`, etc.

### Limitations

The current implementation has these limitations:

1. **Nested ellipsis patterns**: The pattern `((a ...) ...)` has limited support
2. **Quasiquote as special form**: Cannot be implemented as a macro due to depth tracking requirements
3. **No procedural macros**: Complex transformations require multiple helper macros
4. **Static matching only**: No runtime decisions during expansion

### Goals

This document addresses these limitations through two complementary approaches:

1. **Extend `syntax-rules`** with recursive helper patterns and utilities
2. **Implement `syntax-case`** for procedural macros when static patterns are insufficient

---

## Part 1: Extending syntax-rules

### 1.1 Recursive Helper Macros

The key insight for handling complex patterns with `syntax-rules` is to use **mutually recursive helper macros** that incrementally process nested structures.

#### Design Pattern: Accumulator-Based Recursion

The general pattern is:

```scheme
;; Main macro: initialize accumulators and delegate
(define-syntax main-macro
  (syntax-rules ()
    ((main-macro input)
     (%helper input () ()))))  ; Two empty accumulators

;; Helper: process one element at a time
(define-syntax %helper
  (syntax-rules ()
    ;; Base case: input exhausted, use accumulators
    ((%helper () (acc1 ...) (acc2 ...))
     (result-using acc1 ... acc2 ...))
    ;; Recursive case: process first element
    ((%helper (first . rest) (acc1 ...) (acc2 ...))
     (%helper rest (acc1 ... processed-first) (acc2 ... other-processed)))))
```

#### Example: Processing Variable-Length Bindings

The `do` macro in `macros.scm` demonstrates this pattern:

```scheme
;; do uses two helpers to extract:
;; 1. (var init) pairs for named let bindings
;; 2. step expressions for the recursive call

(define-syntax %do-vars
  (syntax-rules ()
    ;; Base case: no more bindings
    ((%do-vars () (pairs ...) (steps ...) test result body ...)
     (%do-run (pairs ...) (steps ...) test result body ...))
    ;; Binding with explicit step
    ((%do-vars ((var init step) . rest) (pairs ...) (steps ...) test result body ...)
     (%do-vars rest (pairs ... (var init)) (steps ... step) test result body ...))
    ;; Binding without step (step defaults to var)
    ((%do-vars ((var init) . rest) (pairs ...) (steps ...) test result body ...)
     (%do-vars rest (pairs ... (var init)) (steps ... var) test result body ...))))

(define-syntax %do-run
  (syntax-rules ()
    ((%do-run (bindings ...) (steps ...) test (result ...) body ...)
     (let %do-loop (bindings ...)
       (if test
           (begin (if #f #f) result ...)
           (begin
             body ...
             (%do-loop steps ...)))))))
```

#### Advanced Pattern: Multi-Pass Processing

For complex transformations, use multiple passes:

```scheme
;; Pass 1: Extract variable names
(define-syntax %extract-names
  (syntax-rules ()
    ((%extract-names () (names ...))
     (names ...))
    ((%extract-names ((name val) . rest) (names ...))
     (%extract-names rest (names ... name)))))

;; Pass 2: Extract initial values
(define-syntax %extract-vals
  (syntax-rules ()
    ((%extract-vals () (vals ...))
     (vals ...))
    ((%extract-vals ((name val) . rest) (vals ...))
     (%extract-vals rest (vals ... val)))))

;; Combine in main macro
(define-syntax multi-bind
  (syntax-rules ()
    ((multi-bind bindings body ...)
     (%multi-bind-expand bindings bindings body ...))))

(define-syntax %multi-bind-expand
  (syntax-rules ()
    ((%multi-bind-expand bindings bindings-copy body ...)
     ;; Create lambda with all names, apply to all vals
     ((lambda (%extract-names bindings ()) body ...)
      . (%extract-vals bindings-copy ())))))
```

### 1.2 Utilities for Nested Patterns

#### The `%sub-pattern` Approach

For deeply nested patterns, introduce helper macros that break down template processing:

```scheme
;; Handle patterns like ((a b c) ...)
(define-syntax %process-nested
  (syntax-rules ()
    ;; Base case
    ((%process-nested () result)
     result)
    ;; Process one nested group
    ((%process-nested ((a b c) . rest) result)
     (%process-nested rest (result (process a b c))))))
```

#### Ellipsis Utilities

The current implementation supports single-level ellipsis. For nested ellipsis:

```scheme
;; Pattern: (((a ...) ...) ...)
;; This requires three levels of iteration

;; Helper for innermost level
(define-syntax %inner-ellipsis
  (syntax-rules ()
    ((%inner-ellipsis () acc)
     (reverse acc))
    ((%inner-ellipsis (a . rest) acc)
     (%inner-ellipsis rest (cons a acc)))))

;; Helper for middle level
(define-syntax %middle-ellipsis
  (syntax-rules ()
    ((%middle-ellipsis () acc)
     (reverse acc))
    ((%middle-ellipsis ((items ...) . rest) acc)
     (%middle-ellipsis rest (cons (%inner-ellipsis (items ...) ()) acc)))))
```

#### Pattern Decomposition

When a pattern is too complex for direct matching:

```scheme
;; Instead of: (syntax-rules () ((mac (a (b c) ...) body) ...))
;; Use decomposition:

(define-syntax mac
  (syntax-rules ()
    ((mac nested-list body ...)
     (%mac-decompose nested-list () body ...))))

(define-syntax %mac-decompose
  (syntax-rules ()
    ((%mac-decompose () processed body ...)
     (%mac-final (processed ...) body ...))
    ((%mac-decompose ((a inner) . rest) (processed ...) body ...)
     (%mac-process-inner a inner rest (processed ...) body ...))))

(define-syntax %mac-process-inner
  (syntax-rules ()
    ((%mac-process-inner a (b c) rest (processed ...) body ...)
     (%mac-decompose rest (processed ... (a b c)) body ...))))
```

### 1.3 Focus Area: Quasiquote

`quasiquote` is challenging for `syntax-rules` because it requires:

1. **Depth tracking**: Nested `quasiquote`/`unquote` pairs
2. **Runtime evaluation**: `unquote` expressions must be evaluated

#### Recursive Template Approach

A `syntax-rules` implementation must track nesting depth:

```scheme
;; quasiquote with depth tracking via helper macros
;; 
;; IMPORTANT: This is a CONCEPTUAL sketch showing what would be needed.
;; It does NOT work in pure syntax-rules - see Limitation below.

(define-syntax quasiquote
  (syntax-rules ()
    ((quasiquote x)
     (%qq x 0))))  ; Start at depth 0

(define-syntax %qq
  (syntax-rules (quasiquote unquote unquote-splicing)
    ;; At depth 0, unquote evaluates
    ((%qq (unquote x) 0)
     x)
    ;; At depth > 0, unquote decrements depth
    ;; NOTE: (- n 1) CANNOT be computed at expansion time in syntax-rules!
    ((%qq (unquote x) n)
     (list 'unquote (%qq x (- n 1))))  ; <-- This doesn't work!
    ;; Nested quasiquote increments depth
    ;; NOTE: (+ n 1) CANNOT be computed at expansion time in syntax-rules!
    ((%qq (quasiquote x) n)
     (list 'quasiquote (%qq x (+ n 1))))  ; <-- This doesn't work!
    ;; Pairs: recurse into both parts
    ((%qq (a . b) n)
     (cons (%qq a n) (%qq b n)))
    ;; Atoms pass through
    ((%qq x n)
     'x)))
```

**Limitation**: The above sketch does NOT work in pure `syntax-rules` because:
- Arithmetic (`+`, `-`) can't be done at expansion time
- Need to evaluate `unquote` expressions at runtime

**Solution**: Use `syntax-case` (see Part 2) or keep as special form.

### 1.4 Focus Area: do Loops

The `do` macro demonstrates successful recursive pattern handling:

```scheme
;; Full do implementation from macros.scm
(define-syntax do
  (syntax-rules ()
    ((do bindings (test result ...) body ...)
     (%do-vars bindings () () test (result ...) body ...))))

;; Variable-length bindings handled recursively
(define-syntax %do-vars
  (syntax-rules ()
    ;; Base case
    ((%do-vars () (pairs ...) (steps ...) test result body ...)
     (%do-run (pairs ...) (steps ...) test result body ...))
    ;; Three-element binding (var init step)
    ((%do-vars ((var init step) . rest) (pairs ...) (steps ...) test result body ...)
     (%do-vars rest (pairs ... (var init)) (steps ... step) test result body ...))
    ;; Two-element binding (var init) - step defaults to var
    ((%do-vars ((var init) . rest) (pairs ...) (steps ...) test result body ...)
     (%do-vars rest (pairs ... (var init)) (steps ... var) test result body ...))))

;; Final expansion using named let
(define-syntax %do-run
  (syntax-rules ()
    ((%do-run (bindings ...) (steps ...) test (result ...) body ...)
     (let %do-loop (bindings ...)
       (if test
           (begin (if #f #f) result ...)
           (begin
             body ...
             (%do-loop steps ...)))))))
```

**Key Techniques**:

1. **Dual accumulators**: `(pairs ...)` and `(steps ...)` collect different transformations
2. **Pattern alternatives**: Different patterns for 2-element vs 3-element bindings
3. **Named let for iteration**: The loop is implemented via named let

### 1.5 Testing Recursive Patterns

#### Comprehensive Test Cases

```scheme
;; Test 1: Basic nested pattern
(define-syntax test-nested
  (syntax-rules ()
    ((test-nested ((a b) ...) body ...)
     (list '(a ...) '(b ...) body ...))))

(test-nested ((1 2) (3 4) (5 6)) 'done)
;; Expected: ((1 3 5) (2 4 6) done)

;; Test 2: Multiple ellipsis levels (requires helpers)
(define-syntax %flatten-inner
  (syntax-rules ()
    ((%flatten-inner () acc) (reverse acc))
    ((%flatten-inner (a . rest) acc) (%flatten-inner rest (cons a acc)))))

(define-syntax test-multi-level
  (syntax-rules ()
    ((test-multi-level ((items ...) ...))
     (%flatten-multi ((items ...) ...) ()))))

;; Test 3: Variable-length with predicates
(define-syntax test-var-length
  (syntax-rules ()
    ((test-var-length () result)
     result)
    ((test-var-length (item . rest) result)
     (test-var-length rest (cons (process item) result)))))
```

#### Hygiene Verification

Test that macro-introduced bindings don't capture user variables:

```scheme
;; Test hygiene in recursive macros
(define temp 42)

(define-syntax test-hygiene
  (syntax-rules ()
    ((test-hygiene items ...)
     (let ((temp 'macro-temp))
       (list items ... temp)))))

(test-hygiene 1 2 3)
;; Expected: (1 2 3 macro-temp)
;; User's 'temp' binding (42) is NOT captured

;; Verify user binding is intact
temp
;; Expected: 42
```

### 1.6 Comparison with syntax-case

| Feature | syntax-rules | syntax-case |
|---------|--------------|-------------|
| **Pattern matching** | Static, declarative | Dynamic, procedural |
| **Recursion depth** | Fixed at definition | Runtime-determined |
| **Arithmetic in patterns** | Not possible | Via Scheme code |
| **Error messages** | Generic | Custom `syntax-error` |
| **Hygiene** | Automatic | Manual control available |
| **Runtime overhead** | None (expansion-time only) | Minimal (same expansion model) |
| **Code complexity** | Multiple helpers for complex cases | Single procedural macro |
| **Debugging** | Pattern matching traces | Standard Scheme debugging |

**When to use syntax-rules**:
- Simple template-based transformations
- Standard derived forms (`let`, `cond`, `case`, etc.)
- No runtime decisions during expansion

**When to use syntax-case**:
- Complex depth tracking (quasiquote)
- Custom error reporting
- Pattern guards and predicates
- Generating patterns dynamically

---

## Part 2: Implementing syntax-case (psyntax)

### 2.1 Syntax Object Design

#### Core Data Structure

Add to `crates/grift_parser/src/value.rs`:

```rust
/// Syntax object wrapping an expression with metadata.
///
/// Supports pattern matching for context-sensitive transformations.
/// Uses arena indices for memory constraints.
///
/// # Memory Layout
///
/// - `expr`: ArenaIndex to the wrapped expression
/// - `context`: ArenaIndex to cons cell (marks . subst)
///   - car: list of marks for hygiene scopes
///   - cdr: substitution environment
///
/// This maintains the 2-index constraint per arena slot.
///
/// # Example
///
/// ```scheme
/// (syntax-case stx ()
///   ((keyword arg ...)
///    (with-syntax ((name (generate-name)))
///      #'(define name (lambda () arg ...)))))
/// ```
Syntax {
    expr: ArenaIndex,      // The wrapped datum
    context: ArenaIndex,   // (marks . substitutions)
},
```

#### Lisp Constructor Methods

Add to `crates/grift_parser/src/lisp.rs`:

```rust
impl<const N: usize> Lisp<N> {
    /// Create a syntax object from an expression
    ///
    /// # Arguments
    ///
    /// * `expr` - The expression to wrap
    /// * `marks` - List of hygiene marks
    /// * `subst` - Substitution environment
    pub fn syntax(
        &self,
        expr: ArenaIndex,
        marks: ArenaIndex,
        subst: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let context = self.cons(marks, subst)?;
        self.arena.alloc(Value::Syntax { expr, context })
    }

    /// Extract components from a syntax object
    pub fn syntax_parts(
        &self,
        idx: ArenaIndex,
    ) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex)> {
        match self.get(idx)? {
            Value::Syntax { expr, context } => {
                let marks = self.car(context)?;
                let subst = self.cdr(context)?;
                Ok((expr, marks, subst))
            }
            _ => Err(ArenaError::TypeMismatch),
        }
    }

    /// Unwrap a syntax object to get the raw datum
    pub fn syntax_to_datum(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(idx)? {
            Value::Syntax { expr, .. } => Ok(expr),
            _ => Ok(idx), // Non-syntax passes through
        }
    }
}
```

### 2.2 Procedural Matching

#### syntax-case Form

The `syntax-case` form provides procedural pattern matching:

```scheme
(syntax-case <stx-expr> (<literal> ...)
  (<pattern> <fender> <output>)
  (<pattern> <output>)
  ...)
```

#### Implementation in Evaluator

Add to `crates/grift_eval/src/evaluator/forms.rs`:

```rust
/// Handle syntax-case form
///
/// (syntax-case stx-expr (literal ...)
///   (pattern [fender] expr)
///   ...)
pub fn step_eval_syntax_case(
    &mut self,
    stx_expr: ArenaIndex,
    literals: ArenaIndex,
    clauses: ArenaIndex,
) -> EvalResult {
    // 1. Evaluate stx-expr to get syntax object
    let stx = self.eval(stx_expr)?;

    // 2. Try each clause in order
    let mut current = clauses;
    while let Value::Cons { .. } = self.lisp.get(current)? {
        let clause = self.lisp.car(current)?;
        let pattern = self.lisp.car(clause)?;
        let clause_cdr = self.lisp.cdr(clause)?;

        // Try to match pattern against stx
        if let Some(bindings) = self.match_syntax_pattern(pattern, stx, literals)? {
            // Check for fender (optional guard)
            let (fender, expr) = self.extract_fender_and_expr(clause_cdr)?;

            // Evaluate fender if present
            if let Some(fender_expr) = fender {
                let fender_result = self.eval_with_bindings(fender_expr, bindings)?;
                if !self.is_truthy(fender_result)? {
                    current = self.lisp.cdr(current)?;
                    continue;
                }
            }

            // Expand bindings and evaluate expr
            return self.eval_with_bindings(expr, bindings);
        }

        current = self.lisp.cdr(current)?;
    }

    // No pattern matched - error
    Err(self.make_error(ErrorKind::SyntaxError, stx_expr)
        .with_message("syntax-case: no pattern matched"))
}

/// Match a pattern against a syntax object
fn match_syntax_pattern(
    &self,
    pattern: ArenaIndex,
    stx: ArenaIndex,
    literals: ArenaIndex,
) -> Result<Option<ArenaIndex>, EvalError> {
    // Unwrap syntax to get datum
    let datum = self.lisp.syntax_to_datum(stx)?;

    // Use existing pattern matching with syntax-aware extensions
    self.match_pattern_syntax_aware(pattern, datum, stx, literals)
}
```

#### Syntax Templates (#'template)

The `#'` reader syntax creates syntax templates:

```rust
/// Handle syntax template: #'template or (syntax template)
pub fn step_eval_syntax_template(
    &mut self,
    template: ArenaIndex,
    pattern_bindings: ArenaIndex,
) -> EvalResult {
    // Transcribe template using pattern bindings
    // Each pattern variable maps to a syntax object
    self.transcribe_syntax_template(template, pattern_bindings)
}
```

### 2.3 Hygiene Support

#### Gensym for Arena Model

The existing gensym implementation works for `syntax-case`:

```rust
/// Generate unique symbol using arena-friendly approach
///
/// Format: #:g{counter} or #:{base}{counter}
pub fn gensym(&mut self, base: &str) -> EvalResult {
    use core::fmt::Write;

    let mut buf = [0u8; 48];
    let mut cursor = WriteCursor::new(&mut buf);

    let _ = write!(cursor, "#:{}{}", base, self.gensym_counter);
    self.gensym_counter += 1;

    let name = cursor.as_str()
        .ok_or_else(|| self.make_error(ErrorKind::Generic, self.lisp.nil().unwrap()))?;

    self.lisp.symbol(name).map_err(Into::into)
}
```

#### Mark-Based Hygiene

For full psyntax compatibility, implement marks:

```rust
/// Apply a fresh mark to a syntax object
///
/// Marks track macro expansion scopes for hygiene.
pub fn mark_syntax(&mut self, stx: ArenaIndex) -> EvalResult {
    let (expr, marks, subst) = self.lisp.syntax_parts(stx)?;

    // Generate fresh mark
    let new_mark = self.gensym_simple()?;

    // Add to marks list
    let new_marks = self.lisp.cons(new_mark, marks)?;

    // Create new syntax object with updated marks
    self.lisp.syntax(expr, new_marks, subst).map_err(Into::into)
}

/// Check if two identifiers are bound-identifier=?
///
/// Two identifiers are bound-identifier=? if they have the same
/// name and marks (same binding).
pub fn bound_identifier_eq(
    &self,
    id1: ArenaIndex,
    id2: ArenaIndex,
) -> Result<bool, EvalError> {
    let (name1, marks1, _) = self.lisp.syntax_parts(id1)?;
    let (name2, marks2, _) = self.lisp.syntax_parts(id2)?;

    // Names must match
    if !self.lisp.eqv(name1, name2)? {
        return Ok(false);
    }

    // Marks must match
    self.marks_equal(marks1, marks2)
}

/// Check if two identifiers are free-identifier=?
///
/// Two identifiers are free-identifier=? if they resolve to the
/// same binding in the current environment.
pub fn free_identifier_eq(
    &self,
    id1: ArenaIndex,
    id2: ArenaIndex,
) -> Result<bool, EvalError> {
    // Resolve both identifiers and compare bindings
    let binding1 = self.resolve_identifier(id1)?;
    let binding2 = self.resolve_identifier(id2)?;

    match (binding1, binding2) {
        (Some(b1), Some(b2)) => self.lisp.eqv(b1, b2),
        (None, None) => {
            // Both unbound - compare names
            let name1 = self.lisp.syntax_to_datum(id1)?;
            let name2 = self.lisp.syntax_to_datum(id2)?;
            self.lisp.eqv(name1, name2)
        }
        _ => Ok(false),
    }
}
```

#### syntax-error

Provide custom error signaling:

```rust
/// Handle syntax-error form
///
/// (syntax-error message detail ...)
pub fn step_eval_syntax_error(
    &mut self,
    message: ArenaIndex,
    details: ArenaIndex,
) -> EvalResult {
    // Evaluate message to string
    let msg_value = self.eval(message)?;

    // Build error with optional details
    let mut error = self.make_error(ErrorKind::SyntaxError, msg_value);

    // Add details if provided
    let mut current = details;
    while let Value::Cons { .. } = self.lisp.get(current)? {
        let detail = self.lisp.car(current)?;
        // Append detail to error message
        error = error.with_detail(detail);
        current = self.lisp.cdr(current)?;
    }

    Err(error)
}
```

### 2.4 Evaluator Integration

#### Registration in Evaluator

Add to `crates/grift_eval/src/evaluator/mod.rs`:

```rust
impl<'a, const N: usize> Evaluator<'a, N> {
    /// Check if form is syntax-case special form
    fn is_syntax_case(&self, head: ArenaIndex) -> Result<bool, EvalError> {
        self.lisp.symbol_matches(head, "syntax-case")
    }

    /// Check if form is syntax template
    fn is_syntax_template(&self, head: ArenaIndex) -> Result<bool, EvalError> {
        self.lisp.symbol_matches(head, "syntax")
    }
}
```

Add to special form handling in `step_eval_list`:

```rust
// In step_eval_list, after checking for macros
if self.is_syntax_case(head)? {
    return self.step_eval_syntax_case(/* ... */);
}

if self.is_syntax_template(head)? {
    return self.step_eval_syntax_template(/* ... */);
}
```

#### Differentiating from syntax-rules

```rust
/// Determine if a transformer is syntax-rules or syntax-case based
fn transformer_type(&self, transformer: ArenaIndex) -> Result<TransformerType, EvalError> {
    match self.lisp.get(transformer)? {
        Value::SyntaxRules { .. } => Ok(TransformerType::SyntaxRules),
        Value::Lambda { .. } => {
            // Could be a syntax-case transformer (procedure)
            Ok(TransformerType::Procedural)
        }
        _ => Err(self.make_error(ErrorKind::TypeMismatch, transformer))
    }
}

enum TransformerType {
    SyntaxRules,   // Static pattern transformer
    Procedural,    // syntax-case based (lambda)
}
```

### 2.5 Focus Area: Quasiquote as Procedural Macro

With `syntax-case`, quasiquote can be properly implemented:

```scheme
;; Quasiquote implemented with syntax-case
;; Handles arbitrary nesting depth

(define-syntax quasiquote
  (lambda (x)
    (syntax-case x ()
      ((_ template)
       (qq-expand #'template 0)))))

;; Helper procedure for expansion (called at expansion time)
(define (qq-expand stx depth)
  (syntax-case stx (quasiquote unquote unquote-splicing)
    ;; Nested quasiquote - increase depth
    ((quasiquote inner)
     (if (= depth 0)
         #`(list 'quasiquote #,(qq-expand #'inner (+ depth 1)))
         #`(list 'quasiquote #,(qq-expand #'inner (+ depth 1)))))

    ;; Unquote at depth 0 - evaluate
    ((unquote expr)
     (if (= depth 0)
         #'expr
         #`(list 'unquote #,(qq-expand #'expr (- depth 1)))))

    ;; Unquote-splicing at depth 0 - splice
    ((unquote-splicing expr)
     (if (= depth 0)
         (syntax-error "unquote-splicing in non-list context")
         #`(list 'unquote-splicing #,(qq-expand #'expr (- depth 1)))))

    ;; Pair - recurse into car and cdr
    ((car . cdr)
     (syntax-case #'car (unquote-splicing)
       ;; Handle splicing in car position
       ((unquote-splicing expr)
        (if (= depth 0)
            #`(append expr #,(qq-expand #'cdr depth))
            #`(cons (list 'unquote-splicing #,(qq-expand #'expr (- depth 1)))
                    #,(qq-expand #'cdr depth))))
       ;; Normal car
       (else
        #`(cons #,(qq-expand #'car depth)
                #,(qq-expand #'cdr depth)))))

    ;; Vector
    (#(elements ...)
     #`(list->vector #,(qq-expand #'(elements ...) depth)))

    ;; Atom - quote it
    (atom
     #''atom)))
```

**Key advantages over syntax-rules**:
- Depth tracking via procedure parameter
- Arithmetic operations during expansion
- Conditional logic based on depth value

### 2.6 Testing and Optimization

#### Comprehensive Tests

```scheme
;; Test 1: Basic syntax-case
(define-syntax swap
  (lambda (x)
    (syntax-case x ()
      ((_ a b)
       #'(let ((temp a))
           (set! a b)
           (set! b temp))))))

(let ((x 1) (y 2))
  (swap x y)
  (list x y))
;; Expected: (2 1)

;; Test 2: Hygiene verification
(define temp 'global)

(let ((x 1) (y 2))
  (swap x y)
  temp)
;; Expected: global (not captured by macro's temp)

;; Test 3: Recursive macro
(define-syntax my-or
  (lambda (x)
    (syntax-case x ()
      ((_) #'#f)
      ((_ e) #'e)
      ((_ e1 e2 ...)
       (with-syntax ((temp (generate-id 'temp)))
         #'(let ((temp e1))
             (if temp temp (my-or e2 ...))))))))

;; Test 4: Quasiquote depth handling
`(a `(b ,(+ 1 2) ,,(+ 3 4)))
;; Expected: (a (quasiquote (b (unquote (+ 1 2)) 7)))

;; Test 5: Custom error message
(define-syntax require-symbol
  (lambda (x)
    (syntax-case x ()
      ((_ sym)
       (if (identifier? #'sym)
           #'sym
           (syntax-error "expected symbol" #'sym))))))
```

#### Benchmarks

Compare expansion time between `syntax-rules` and `syntax-case`:

```scheme
;; Benchmark: Expand 1000 let forms

;; syntax-rules version (current)
(time
  (let loop ((n 1000))
    (if (= n 0)
        'done
        (begin
          (expand '(let ((x 1) (y 2)) (+ x y)))
          (loop (- n 1))))))

;; syntax-case version
(time
  (let loop ((n 1000))
    (if (= n 0)
        'done
        (begin
          (expand-syntax-case '(let ((x 1) (y 2)) (+ x y)))
          (loop (- n 1))))))
```

Expected results:
- `syntax-rules`: Faster for simple patterns (no procedure call overhead)
- `syntax-case`: Comparable for simple cases, more flexible for complex ones

### 2.7 syntax-rules vs syntax-case Comparison

| Aspect | syntax-rules | syntax-case |
|--------|--------------|-------------|
| **Paradigm** | Declarative pattern/template | Procedural with patterns |
| **R7RS Status** | Required (Section 4.3.2) | Not required (R6RS) |
| **Pattern Language** | Patterns + templates | Patterns + Scheme code |
| **Hygiene** | Automatic, always | Default automatic, can override |
| **Recursion** | Via helper macros | Via Scheme procedures |
| **Error Messages** | Generic | Custom via `syntax-error` |
| **Guard Predicates** | Not supported | Fenders (guards) |
| **Generated Output** | Templates only | Any Scheme expression |
| **Use Case** | 90% of macros | Complex transformations |
| **Memory (arena)** | ~1 slot per rule | Similar + procedure overhead |
| **Expansion Time** | Fast (pattern matching) | Slightly slower (eval) |

#### When to Use Each

**Use `syntax-rules` when**:
- The transformation is a simple pattern-to-template mapping
- All pattern variables map directly to template positions
- No arithmetic or conditionals needed during expansion
- R7RS compatibility is required

**Use `syntax-case` when**:
- Depth tracking is required (quasiquote)
- Custom error messages are needed
- Guard predicates filter matches
- Output depends on input analysis
- Hygiene bending is required (e.g., `datum->syntax`)

---

## Implementation Roadmap

### Phase 1: Enhanced syntax-rules ✅ Complete

1. **Document recursive patterns** ✅ (this document)
2. **Add more helper macro examples** ✅
3. **Improve nested ellipsis handling in expand.rs** ✅
4. **Add tests for complex patterns** ✅

### Phase 2: Syntax Objects ✅ Complete

1. Add `Value::Syntax` variant to parser ✅
2. Implement `syntax`, `syntax->datum` primitives ✅ (Lisp methods added)
3. Add mark infrastructure ✅ (`mark_syntax`, `bound_identifier_eq`, `free_identifier_eq`, etc.)

### Phase 3: syntax-case Core (Current Focus)

1. Implement `syntax-case` special form handler
2. Add `syntax` template transcription
3. Implement `with-syntax`

### Phase 4: Hygiene Utilities

1. Implement `bound-identifier=?`, `free-identifier=?` (Scheme-level wrappers for Rust functions) 
2. Add `datum->syntax`, `syntax->datum` (Scheme-level wrappers)
3. Implement `generate-temporaries`

### Phase 5: Replace Special Forms

1. Convert `quasiquote` to procedural macro
2. Optimize expansion performance
3. Add comprehensive test suite

### Phase 6: Polish

1. Error message improvements
2. Documentation
3. Benchmarking and optimization

---

## Appendix: Arena Memory Considerations

### Memory Budget

| Component | Slots per Instance | Expected Count | Total Slots |
|-----------|-------------------|----------------|-------------|
| Syntax object | 1 | Transient | ~50 during expansion |
| Mark | 1 | Per expansion level | ~10 max |
| Substitution | 1 per binding | Variable | ~20 per macro |
| Pattern bindings | 1 per pattern var | Variable | ~10 per expansion |
| **Total per expansion** | - | - | ~100 slots |
| **Concurrent expansions** | - | 1 (trampolined) | ~100 slots |

### Optimization Strategies

1. **Reuse syntax contexts**: Share marks/subst when possible
2. **Limit expansion depth**: Cap nested `syntax-case` calls
3. **Eager cleanup**: Dereference temporary syntax objects
4. **Pattern caching**: Cache compiled patterns for repeated use

### Comparison with Current Implementation

| Metric | syntax-rules Only | With syntax-case |
|--------|-------------------|------------------|
| Memory per macro | ~10 slots | ~15 slots |
| Expansion overhead | ~20 slots | ~100 slots |
| Maximum depth | Unlimited (tail-recursive) | ~50 (stack) |
| Concurrent expansions | 1 | 1 |

---

## References

1. [R7RS-small](https://small.r7rs.org/attachment/r7rs.pdf) - Sections 4.3.2 (syntax-rules), 4.3.3 (signaling errors)
2. [R6RS](http://www.r6rs.org/) - Chapter 11 (syntax-case)
3. [Macros that Work](https://www.researchgate.net/publication/220997237_Macros_That_Work) - Clinger & Rees, 1991
4. [psyntax](https://www.cs.indiana.edu/~dyb/pubs/tr356.pdf) - Dybvig, Hieb, Bruggeman
5. [Hygienic Macro Expansion](https://www.semanticscholar.org/paper/Hygienic-macro-expansion-Kohlbecker-Friedman/d18e91ddfd00b2a04cdbbf800f25b3ce12e1c982) - Kohlbecker et al., 1986

---

## Conclusion

Extending Grift's macro system requires a two-pronged approach:

1. **For most macros**: Use `syntax-rules` with recursive helper patterns. The existing implementation handles 90% of use cases with the techniques described in Part 1.

2. **For complex transformations**: Implement `syntax-case` to handle cases like `quasiquote` that require arithmetic, conditionals, or custom error handling during expansion.

The arena-based, `no_std` constraints are compatible with both approaches. The key insight is that:

- `syntax-rules` transformations are purely structural (no allocation beyond pattern bindings)
- `syntax-case` adds procedure calls but still operates within the arena model
- Both share the same hygiene infrastructure (gensym, marks)

This document provides the design foundation for implementing these extensions while maintaining Grift's core principles of minimal memory usage and `no_std` compatibility.
