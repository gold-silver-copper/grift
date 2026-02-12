//! Hygienic macro expansion for syntax-case.
//!
//! This module implements the "Macros that Work" algorithm (Clinger & Rees, 1991)
//! for R7RS-compatible hygienic macros using syntax-case.

use grift_parser::{ArenaIndex, Value};

use crate::error::{ErrorKind, EvalError, EvalResult};
use crate::continuation::{TrampolineState, ContType, EnvRef, ExprRef};
use super::Evaluator;

// ============================================================================
// WriteCursor - Helper for no-alloc string formatting
// ============================================================================

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

// ============================================================================
// Gensym - Fresh Symbol Generation
// ============================================================================

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Generate a fresh symbol guaranteed to be unique
    ///
    /// Format: #:g{counter} or #:{base}{counter}
    ///
    /// # Example
    ///
    /// ```
    /// use grift_parser::Lisp;
    /// use grift_eval::Evaluator;
    /// 
    /// let lisp: Lisp<20000> = Lisp::new();
    /// let mut eval = Evaluator::new(&lisp).unwrap();
    /// 
    /// let sym1 = eval.gensym("tmp").unwrap();
    /// let sym2 = eval.gensym("tmp").unwrap();
    /// let sym3 = eval.gensym_simple().unwrap();
    /// 
    /// // Each symbol is unique
    /// assert_ne!(sym1, sym2);
    /// assert_ne!(sym2, sym3);
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

        // Gensym names are unique by construction (monotonic counter),
        // so skip the intern table lookup which would always miss.
        self.lisp.symbol_new_unique(name).map_err(Into::into)
    }

    /// Generate a simple gensym with default prefix
    pub fn gensym_simple(&mut self) -> EvalResult {
        self.gensym("g")
    }
}

// ============================================================================
// Mark Infrastructure for Hygiene
// ============================================================================

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Apply a fresh mark to a syntax object
    ///
    /// Marks track macro expansion scopes for hygiene. Each time a macro
    /// is expanded, a fresh mark is applied to all identifiers introduced
    /// by the macro. This allows the system to distinguish between
    /// identifiers with the same name but different binding scopes.
    ///
    /// # Arguments
    ///
    /// * `stx` - A syntax object to mark
    ///
    /// # Returns
    ///
    /// A new syntax object with the fresh mark added to its marks list
    ///
    /// # Memory
    ///
    /// Allocates 2 arena slots: one for the new mark, one for the cons cell
    pub fn mark_syntax(&mut self, stx: ArenaIndex) -> EvalResult {
        let (expr, marks, subst, lex_env) = self.lisp.syntax_parts_with_env(stx)?;

        // Generate fresh mark
        let new_mark = self.gensym_simple()?;

        // Add to marks list (prepend for efficiency)
        let new_marks = self.lisp.cons(new_mark, marks)?;

        // Create new syntax object with updated marks, preserving lexical environment
        self.lisp.syntax_with_env(expr, new_marks, subst, lex_env).map_err(Into::into)
    }

    /// Check if two marks lists are equal
    ///
    /// Two marks lists are equal if they contain the same marks in the same order.
    /// This is used by `bound_identifier_eq` to compare identifiers.
    ///
    /// # Arguments
    ///
    /// * `marks1` - First marks list
    /// * `marks2` - Second marks list
    ///
    /// # Returns
    ///
    /// `true` if the marks lists are identical, `false` otherwise
    fn marks_equal(&self, marks1: ArenaIndex, marks2: ArenaIndex) -> Result<bool, EvalError> {
        let mut m1 = marks1;
        let mut m2 = marks2;

        loop {
            match (self.lisp.get(m1)?, self.lisp.get(m2)?) {
                // Both nil - equal
                (Value::Nil, Value::Nil) => return Ok(true),
                // One nil, one not - not equal
                (Value::Nil, _) | (_, Value::Nil) => return Ok(false),
                // Both cons - compare cars and recurse on cdrs
                (Value::Cons { .. }, Value::Cons { .. }) => {
                    let car1 = self.lisp.car(m1)?;
                    let car2 = self.lisp.car(m2)?;
                    
                    // Compare marks using symbol equality
                    if !self.lisp.symbol_eq(car1, car2)? {
                        return Ok(false);
                    }
                    
                    m1 = self.lisp.cdr(m1)?;
                    m2 = self.lisp.cdr(m2)?;
                }
                // Any other combination - not equal
                _ => return Ok(false),
            }
        }
    }

    /// Check if two identifiers are bound-identifier=?
    ///
    /// Two identifiers are `bound-identifier=?` if they have the same name
    /// AND the same marks. This means they were introduced at the same
    /// point in the macro expansion process and would bind the same variable.
    ///
    /// # Arguments
    ///
    /// * `id1` - First identifier (as a syntax object)
    /// * `id2` - Second identifier (as a syntax object)
    ///
    /// # Returns
    ///
    /// `true` if the identifiers have the same name and marks
    ///
    /// # Example
    ///
    /// In the following macro, `x` introduced by the template is different
    /// from `x` provided by the user because they have different marks:
    ///
    /// ```scheme
    /// (define-syntax test
    ///   (lambda (stx)
    ///     (syntax-case stx ()
    ///       ((test x) (syntax (let ((x 1)) x))))))
    /// (let ((x 2)) (test x))  ; returns 1, not 2
    /// ```
    pub fn bound_identifier_eq(
        &self,
        id1: ArenaIndex,
        id2: ArenaIndex,
    ) -> Result<bool, EvalError> {
        // Check if both are syntax objects
        match (self.lisp.get(id1)?, self.lisp.get(id2)?) {
            (Value::Syntax { .. }, Value::Syntax { .. }) => {
                let (name1, marks1, _) = self.lisp.syntax_parts(id1)?;
                let (name2, marks2, _) = self.lisp.syntax_parts(id2)?;

                // Names must match
                if !self.lisp.eqv(name1, name2)? {
                    return Ok(false);
                }

                // Marks must match
                self.marks_equal(marks1, marks2)
            }
            // If both are symbols (not syntax objects), compare directly
            (Value::Symbol(_), Value::Symbol(_)) => {
                self.lisp.symbol_eq(id1, id2).map_err(Into::into)
            }
            // Mixed or non-identifier types - not equal
            _ => Ok(false),
        }
    }

    /// Resolve an identifier to its binding in the current environment
    ///
    /// This function looks up an identifier in the substitution environment
    /// associated with the syntax object, then in its captured lexical environment,
    /// and finally in the global environment.
    ///
    /// # Arguments
    ///
    /// * `id` - An identifier (as a syntax object or symbol)
    ///
    /// # Returns
    ///
    /// `Some(binding)` if the identifier is bound, `None` if unbound
    fn resolve_identifier(&self, id: ArenaIndex) -> Result<Option<ArenaIndex>, EvalError> {
        // If it's a syntax object, check its substitution environment first,
        // then its captured lexical environment
        if let Value::Syntax { .. } = self.lisp.get(id)? {
            let (name, _marks, subst, lex_env) = self.lisp.syntax_parts_with_env(id)?;
            
            // 1. Check substitution environment (explicit renames)
            if let Some(binding) = self.lookup_in_subst(name, subst)? {
                return Ok(Some(binding));
            }
            
            // 2. Check captured lexical environment (lexical scope preservation)
            if let Some(binding) = self.lookup_in_env(name, lex_env)? {
                return Ok(Some(binding));
            }
            
            // 3. Fall through to check global environment
            return self.lookup_in_env(name, self.global_env.0);
        }
        
        // For plain symbols, check the global environment
        self.lookup_in_env(id, self.global_env.0)
    }

    /// Look up a name in a substitution environment
    ///
    /// The substitution environment is an alist of (name . binding) pairs.
    /// This is identical in structure to a regular environment lookup.
    pub(super) fn lookup_in_subst(
        &self,
        name: ArenaIndex,
        subst: ArenaIndex,
    ) -> Result<Option<ArenaIndex>, EvalError> {
        self.lookup_in_env_optional(subst, name)
    }

    /// Look up a name in an environment
    fn lookup_in_env(
        &self,
        name: ArenaIndex,
        env: ArenaIndex,
    ) -> Result<Option<ArenaIndex>, EvalError> {
        self.lookup_in_env_optional(env, name)
    }

    /// Check if two identifiers are free-identifier=?
    ///
    /// Two identifiers are `free-identifier=?` if they resolve to the same
    /// binding in the current environment. This is used when checking if
    /// an identifier in a pattern matches a literal keyword.
    ///
    /// # Arguments
    ///
    /// * `id1` - First identifier
    /// * `id2` - Second identifier
    ///
    /// # Returns
    ///
    /// `true` if both identifiers resolve to the same binding (or both are unbound
    /// and have the same name)
    ///
    /// # Example
    ///
    /// ```scheme
    /// ;; Two references to the same variable are free-identifier=?
    /// (define x 1)
    /// ;; (free-identifier=? #'x #'x) => #t
    ///
    /// ;; In a macro, an identifier from the input may refer to a different
    /// ;; binding than an identifier introduced by the template:
    /// (let ((x 1))          ;; outer x
    ///   (let ((x 2))        ;; inner x (shadows outer)
    ///     ;; references to 'x' here refer to inner x (value 2)
    ///     ;; but in a macro that captured outer x, they would differ
    ///     x))               ;; => 2
    /// ```
    pub fn free_identifier_eq(
        &self,
        id1: ArenaIndex,
        id2: ArenaIndex,
    ) -> Result<bool, EvalError> {
        // Resolve both identifiers
        let binding1 = self.resolve_identifier(id1)?;
        let binding2 = self.resolve_identifier(id2)?;

        match (binding1, binding2) {
            // Both bound - compare bindings
            (Some(b1), Some(b2)) => self.lisp.eqv(b1, b2).map_err(Into::into),
            // Both unbound - compare names, then check call-site vs definition-site
            (None, None) => {
                let name1 = self.lisp.syntax_to_datum(id1)?;
                let name2 = self.lisp.syntax_to_datum(id2)?;
                if !self.lisp.eqv(name1, name2)? {
                    return Ok(false);
                }
                // Same name, both unbound in their resolved environments.
                // Additionally check whether a local binding at the macro call site
                // shadows this identifier. If the identifier is locally bound at the
                // call site but not at the macro definition site (global env), the
                // input identifier refers to a different binding than the template
                // identifier, so they are not free-identifier=?.
                // This handles: (let ((else #f)) (cond (else 42)))
                if !self.lisp.get(self.call_site_env.0)?.is_nil() {
                    let call_site_binding = self.lookup_in_env(name1, self.call_site_env.0)?;
                    let def_site_binding = self.lookup_in_env(name1, self.global_env.0)?;
                    match (call_site_binding, def_site_binding) {
                        (Some(_), None) | (None, Some(_)) => return Ok(false),
                        (Some(cb), Some(db)) => return self.lisp.eqv(cb, db).map_err(Into::into),
                        (None, None) => {}
                    }
                }
                Ok(true)
            }
            // One bound, one not - not equal
            _ => Ok(false),
        }
    }
}

// ============================================================================
// Binding and Rename Environments
// ============================================================================

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Look up a symbol in bindings (pattern variable -> value)
    /// Returns Some(value) if found, None otherwise
    pub(crate) fn bindings_lookup(&self, bindings: ArenaIndex, sym: ArenaIndex) -> Result<Option<ArenaIndex>, EvalError> {
        let mut current = bindings;
        loop {
            if self.lisp.get(current)?.is_nil() {
                return Ok(None);
            }
            let pair = self.lisp.car(current)?;
            let key = self.lisp.car(pair)?;
            if self.lisp.symbol_eq(key, sym)? {
                return Ok(Some(self.lisp.cdr(pair)?));
            }
            current = self.lisp.cdr(current)?;
        }
    }

    /// Extend bindings with a new mapping
    pub(crate) fn bindings_extend(
        &self,
        bindings: ArenaIndex,
        key: ArenaIndex,
        value: ArenaIndex,
    ) -> EvalResult {
        let pair = self.lisp.cons(key, value)?;
        self.lisp.cons(pair, bindings).map_err(Into::into)
    }

    /// Look up a symbol in the rename environment
    ///
    /// Returns the renamed symbol if found, otherwise the original.
    fn rename_lookup(&self, sym: ArenaIndex, renames: ArenaIndex) -> EvalResult {
        match self.bindings_lookup(renames, sym)? {
            Some(renamed) => Ok(renamed),
            None => Ok(sym),
        }
    }

    /// Extend rename environment with a new mapping
    fn rename_extend(
        &self,
        renames: ArenaIndex,
        original: ArenaIndex,
        renamed: ArenaIndex,
    ) -> EvalResult {
        self.bindings_extend(renames, original, renamed)
    }

    /// Check if two symbols are equal
    pub(crate) fn symbols_eq(&self, a: ArenaIndex, b: ArenaIndex) -> Result<bool, EvalError> {
        self.lisp.symbol_eq(a, b).map_err(Into::into)
    }

    /// Check if a variable is bound anywhere in an environment
    fn env_bound_anywhere(&self, env: ArenaIndex, name: ArenaIndex) -> Result<bool, EvalError> {
        let mut current = env;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return Ok(false),
                Value::Cons { car, cdr } => {
                    if let Value::Cons { car: bound_name, .. } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        return Ok(true);
                    }
                    current = cdr;
                }
                _ => return Ok(false),
            }
        }
    }
}

// ============================================================================
// List Helpers
// ============================================================================

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Get the length of a list
    pub(crate) fn list_length(&self, list: ArenaIndex) -> Result<usize, EvalError> {
        self.lisp.list_len(list).map_err(Into::into)
    }
}

// ============================================================================
// Pattern Matching
// ============================================================================

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Check if symbol is in literals list
    fn is_literal(&self, sym: ArenaIndex, literals: ArenaIndex) -> Result<bool, EvalError> {
        self.symbol_in_list(sym, literals)
    }

    /// Check if a symbol is a member of a list of symbols
    fn symbol_in_list(&self, sym: ArenaIndex, list: ArenaIndex) -> Result<bool, EvalError> {
        let mut current = list;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let item = self.lisp.car(current)?;
            if self.symbols_eq(sym, item)? {
                return Ok(true);
            }
            current = self.lisp.cdr(current)?;
        }
        Ok(false)
    }

    /// Check if pattern cdr starts with ellipsis
    fn has_ellipsis(&self, pat_cdr: ArenaIndex) -> Result<bool, EvalError> {
        match self.lisp.get(pat_cdr)? {
            // Case 1: pat_cdr is a list starting with ...
            // Pattern like: (a ... rest) parsed as (a . (... . rest))
            Value::Cons { .. } => {
                let first = self.lisp.car(pat_cdr)?;
                self.lisp.symbol_matches(first, "...").map_err(Into::into)
            }
            // Case 2: pat_cdr IS the ellipsis symbol itself
            // Pattern like: (a ...) parsed as improper list (a . ...)
            Value::Symbol(_) => {
                self.lisp.symbol_matches(pat_cdr, "...").map_err(Into::into)
            }
            _ => Ok(false),
        }
    }

    /// Extract the rest pattern/template after an ellipsis.
    /// Given the cdr that contains the ellipsis, returns the remaining elements after it.
    fn rest_after_ellipsis(&self, ellipsis_cdr: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        match self.lisp.get(ellipsis_cdr)? {
            Value::Cons { .. } => self.lisp.cdr(ellipsis_cdr).map_err(Into::into),
            _ => self.lisp.nil().map_err(Into::into),
        }
    }

    /// Get minimum length a pattern requires
    /// Calculate minimum number of elements matched by a pattern
    /// 
    /// Iterative implementation to avoid stack overflow on deeply nested patterns.
    fn pattern_min_length(&self, mut pattern: ArenaIndex, _literals: ArenaIndex) -> Result<usize, EvalError> {
        let mut length = 0;
        
        loop {
            match self.lisp.get(pattern)? {
                Value::Nil => return Ok(length),
                Value::Cons { .. } => {
                    let cdr = self.lisp.cdr(pattern)?;
                    // Check for ellipsis which consumes variable number of elements
                    if self.has_ellipsis(cdr)? {
                        // Pattern with ellipsis: element before ... is repeated
                        // Get rest pattern (nil if cdr is just the symbol ...)
                        let rest = self.rest_after_ellipsis(cdr)?;
                        // Continue with rest pattern (tail recursion)
                        pattern = rest;
                        continue;
                    }
                    // Regular element: 1 + rest
                    length += 1;
                    pattern = cdr;
                }
                _ => return Ok(length),
            }
        }
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

    /// Collect pattern variables into a list
    /// 
    /// Iterative implementation using an arena-allocated work stack to avoid
    /// both stack overflow and fixed-size limits.
    fn collect_pattern_vars_into(
        &self,
        pattern: ArenaIndex,
        literals: ArenaIndex,
        vars: &mut ArenaIndex,
    ) -> Result<(), EvalError> {
        // Use an arena-allocated cons list as a work stack
        let nil = self.lisp.nil()?;
        let mut stack = self.lisp.cons(pattern, nil)?;
        
        while !self.lisp.get(stack)?.is_nil() {
            // Pop from stack
            let current = self.lisp.car(stack)?;
            stack = self.lisp.cdr(stack)?;
            
            match self.lisp.get(current)? {
                Value::Symbol(_) => {
                    // Skip _, ..., and literals
                    if !self.lisp.symbol_matches(current, "_")?
                        && !self.lisp.symbol_matches(current, "...")?
                        && !self.is_literal(current, literals)?
                    {
                        *vars = self.lisp.cons(current, *vars)?;
                    }
                }
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    
                    // Handle ellipsis - two cases:
                    // 1. cdr is the symbol ... (improper list like (a . ...))
                    // 2. cdr is a list starting with ... (like (a ... rest))
                    let should_process_cdr = match self.lisp.get(cdr)? {
                        Value::Symbol(_) if self.lisp.symbol_matches(cdr, "...")? => {
                            // Case 1: cdr IS the ellipsis symbol - no more vars to collect
                            false
                        }
                        Value::Cons { .. } => {
                            let first = self.lisp.car(cdr)?;
                            if self.lisp.symbol_matches(first, "...")? {
                                // Case 2: cdr starts with ... - skip it and process rest
                                let rest = self.lisp.cdr(cdr)?;
                                stack = self.lisp.cons(rest, stack)?;
                                false
                            } else {
                                true
                            }
                        }
                        _ => true,
                    };
                    
                    // Push car and cdr to stack
                    if should_process_cdr {
                        stack = self.lisp.cons(cdr, stack)?;
                    }
                    stack = self.lisp.cons(car, stack)?;
                }
                _ => {}
            }
        }
        
        Ok(())
    }

    /// Syntax-aware pattern matching for syntax-case
    ///
    /// This is like `match_pattern`, but it preserves syntax objects when binding
    /// pattern variables. This ensures that pattern variables bound to identifiers
    /// preserve their lexical context (from the macro call site).
    ///
    /// The key difference from `match_pattern`:
    /// - Pattern variables bind to the original syntax object, not the unwrapped datum
    /// - Structure matching (car/cdr) looks through syntax wrappers
    pub(crate) fn match_pattern_syntax(
        &self,
        pattern: ArenaIndex,
        stx: ArenaIndex,
        literals: ArenaIndex,
        bindings: ArenaIndex,
    ) -> Result<Option<ArenaIndex>, EvalError> {
        // Get the underlying datum for structural matching
        let expr = self.lisp.syntax_to_datum(stx)?;
        
        match self.lisp.get(pattern)? {
            // Wildcard: matches anything, binds nothing
            Value::Symbol(_) if self.lisp.symbol_matches(pattern, "_")? => {
                Ok(Some(bindings))
            }

            // Ellipsis symbol itself: error (shouldn't appear here)
            Value::Symbol(_) if self.lisp.symbol_matches(pattern, "...")? => {
                Err(self.make_error(ErrorKind::SyntaxError, pattern)
                    .with_message("misplaced ellipsis in pattern"))
            }

            // Literal keyword: must match via free-identifier=?
            // Per R7RS, a literal in a syntax-rules pattern matches only if
            // the input identifier and the literal are free-identifier=?.
            // Both identifiers are resolved: the input in the call-site env,
            // the literal in the global env (where the macro was defined).
            // They match if both resolve to the same binding, or both are unbound.
            Value::Symbol(_) if self.is_literal(pattern, literals)? => {
                match self.lisp.get(expr)? {
                    Value::Symbol(_) if self.symbols_eq(pattern, expr)? => {
                        // Names match - check if they resolve to the same binding
                        let input_binding = self.lookup_in_env(expr, self.call_site_env.0)?;
                        let literal_binding = self.lookup_in_env(pattern, self.global_env.0)?;
                        match (input_binding, literal_binding) {
                            // Both bound - match only if they resolve to the same value
                            (Some(ib), Some(lb)) => {
                                if self.lisp.eqv(ib, lb)? {
                                    Ok(Some(bindings))
                                } else {
                                    Ok(None)
                                }
                            }
                            // Both unbound - match (same name, same free reference)
                            (None, None) => Ok(Some(bindings)),
                            // One bound, one not - different bindings, no match
                            _ => Ok(None),
                        }
                    }
                    _ => Ok(None),
                }
            }

            // Pattern variable: bind to the ORIGINAL syntax object (not unwrapped datum)
            // This preserves lexical context for identifiers
            Value::Symbol(_) => {
                let new_bindings = self.bindings_extend(bindings, pattern, stx)?;
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
                self.match_list_pattern_syntax(pattern, stx, literals, bindings)
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

    /// Syntax-aware list pattern matching
    fn match_list_pattern_syntax(
        &self,
        pattern: ArenaIndex,
        stx: ArenaIndex,
        literals: ArenaIndex,
        bindings: ArenaIndex,
    ) -> Result<Option<ArenaIndex>, EvalError> {
        // Get the underlying datum for structural checks
        let expr = self.lisp.syntax_to_datum(stx)?;
        
        let pat_car = self.lisp.car(pattern)?;
        let pat_cdr = self.lisp.cdr(pattern)?;

        // Check for escaping ellipsis: (... <pattern>) matches <pattern> literally
        // (... ...) in a pattern matches the literal symbol ...
        if self.lisp.symbol_matches(pat_car, "...")?
            && let Value::Cons { .. } = self.lisp.get(pat_cdr)? {
                let inner_pat = self.lisp.car(pat_cdr)?;
                // Match the inner pattern literally (without ellipsis processing)
                return self.match_pattern_syntax(inner_pat, stx, literals, bindings);
            }

        // Check for ellipsis FIRST - ellipsis can match empty lists
        if self.has_ellipsis(pat_cdr)? {
            return self.match_ellipsis_pattern_syntax(
                pat_car, pat_cdr, stx, literals, bindings
            );
        }
        
        // For non-ellipsis patterns, expression must be a non-empty list
        if !matches!(self.lisp.get(expr)?, Value::Cons { .. }) {
            return Ok(None);
        }

        // Get sub-expressions from the original stx to preserve syntax context
        // If stx is a syntax object wrapping a list, get its parts
        let (expr_car, expr_cdr) = match self.lisp.get(stx)? {
            Value::Syntax { .. } => {
                // Get car/cdr of the wrapped datum
                let datum = self.lisp.syntax_to_datum(stx)?;
                self.lisp.car_cdr(datum)?
            }
            Value::Cons { .. } => {
                self.lisp.car_cdr(stx)?
            }
            _ => return Ok(None),
        };

        match self.match_pattern_syntax(pat_car, expr_car, literals, bindings)? {
            Some(bindings1) => {
                self.match_pattern_syntax(pat_cdr, expr_cdr, literals, bindings1)
            }
            None => Ok(None),
        }
    }

    /// Syntax-aware ellipsis pattern matching
    fn match_ellipsis_pattern_syntax(
        &self,
        sub_pattern: ArenaIndex,
        pat_cdr: ArenaIndex,
        stx: ArenaIndex,
        literals: ArenaIndex,
        bindings: ArenaIndex,
    ) -> Result<Option<ArenaIndex>, EvalError> {
        // Get the underlying datum for structural checks
        let expr = self.lisp.syntax_to_datum(stx)?;
        
        // Get the rest pattern after ...
        let rest_pattern = self.rest_after_ellipsis(pat_cdr)?;

        // Count how many elements the rest pattern needs
        let rest_len = self.pattern_min_length(rest_pattern, literals)?;

        // Count expression length. If the expression is not a proper list
        // (e.g., a symbol or atom), the ellipsis pattern can't match.
        let expr_len = match self.list_length(expr) {
            Ok(len) => len,
            Err(_) => return Ok(None),
        };

        if expr_len < rest_len {
            return Ok(None);
        }

        let ellipsis_count = expr_len - rest_len;

        // Collect pattern variables
        let pattern_vars = self.collect_pattern_vars(sub_pattern, literals)?;

        // Initialize accumulators
        let mut var_accums = self.lisp.nil()?;
        let mut pv_current = pattern_vars;
        while let Value::Cons { .. } = self.lisp.get(pv_current)? {
            let var = self.lisp.car(pv_current)?;
            let empty = self.lisp.nil()?;
            var_accums = self.bindings_extend(var_accums, var, empty)?;
            pv_current = self.lisp.cdr(pv_current)?;
        }

        // Match ellipsis elements - get elements from the original stx to preserve syntax
        let mut current = match self.lisp.get(stx)? {
            Value::Syntax { .. } => expr,
            _ => stx,
        };
        
        for _ in 0..ellipsis_count {
            let elem = self.lisp.car(current)?;
            let empty = self.lisp.nil()?;

            match self.match_pattern_syntax(sub_pattern, elem, literals, empty)? {
                Some(elem_bindings) => {
                    var_accums = self.merge_ellipsis_bindings(var_accums, elem_bindings)?;
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
        self.match_pattern_syntax(rest_pattern, current, literals, result)
    }

    /// Merge element bindings into accumulator
    fn merge_ellipsis_bindings(
        &self,
        accums: ArenaIndex,
        elem_bindings: ArenaIndex,
    ) -> EvalResult {
        let mut result = self.lisp.nil()?;
        let mut acc_current = accums;
        
        while let Value::Cons { .. } = self.lisp.get(acc_current)? {
            let acc_pair = self.lisp.car(acc_current)?;
            let var = self.lisp.car(acc_pair)?;
            let acc_list = self.lisp.cdr(acc_pair)?;
            
            // Find this var's value in elem_bindings
            let val = match self.bindings_lookup(elem_bindings, var)? {
                Some(v) => v,
                None => self.lisp.nil()?,
            };
            
            // Prepend to accumulator (we'll reverse at the end)
            let new_acc_list = self.lisp.cons(val, acc_list)?;
            let new_pair = self.lisp.cons(var, new_acc_list)?;
            result = self.lisp.cons(new_pair, result)?;
            
            acc_current = self.lisp.cdr(acc_current)?;
        }
        
        self.reverse_list(result)
    }
}

// ============================================================================
// Template Transcription
// ============================================================================

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Transcribe a template with matched bindings
    ///
    /// Hygiene: Variables introduced by the macro (not from pattern) are renamed
    /// using gensym to prevent capture.
    pub(crate) fn transcribe_template(
        &mut self,
        template: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> EvalResult {
        self.transcribe_template_impl(template, bindings, renames, def_env, None)
    }
    
    /// Transcribe a template with matched bindings, capturing lexical environment
    /// for free variables.
    ///
    /// This version creates syntax objects with captured lexical environment
    /// for identifiers that are:
    /// 1. Not pattern variables
    /// 2. Bound in the current lexical environment
    ///
    /// This enables lexically-scoped syntax objects as required by R6RS.
    pub(super) fn transcribe_template_with_env(
        &mut self,
        template: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
        lex_env: ArenaIndex,
    ) -> EvalResult {
        self.transcribe_template_impl(template, bindings, renames, def_env, Some(lex_env))
    }

    /// Unified template transcription with optional lexical environment.
    fn transcribe_template_impl(
        &mut self,
        template: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
        lex_env: Option<ArenaIndex>,
    ) -> EvalResult {
        match self.lisp.get(template)? {
            Value::Symbol(_) => {
                if let Some(le) = lex_env {
                    self.transcribe_symbol_with_env(template, bindings, renames, def_env, le)
                } else {
                    self.transcribe_symbol(template, bindings, renames, def_env)
                }
            }

            Value::Nil => Ok(self.lisp.nil()?),

            Value::Cons { .. } => {
                self.transcribe_list_impl(template, bindings, renames, def_env, lex_env)
            }

            // Other values (including Syntax objects) pass through unchanged
            _ => Ok(template),
        }
    }
    
    /// Transcribe a symbol, potentially wrapping it in a syntax object with
    /// captured lexical environment.
    ///
    /// The order of checks is crucial for proper scope handling:
    /// 1. Check if the symbol is bound locally BUT is NOT a pattern variable.
    ///    Local bindings from `let`, `lambda`, etc. should shadow pattern variables.
    ///    This enables test 6.2 where `(let ((x 999)) (syntax x))` should create
    ///    a syntax object referring to the local `x`, not the pattern variable `x`.
    /// 2. Check if it's a pattern variable (from syntax-case bindings)
    /// 3. Check if already renamed (for hygiene)
    /// 4. Check if bound in macro's definition environment
    /// 5. Otherwise, it's introduced by the macro
    fn transcribe_symbol_with_env(
        &mut self,
        sym: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
        lex_env: ArenaIndex,
    ) -> EvalResult {
        // 1. Check if bound locally (in lex_env but NOT in global_env) AND
        //    if this local binding shadows a pattern variable.
        //    Local bindings from `let`, `lambda`, etc. should shadow pattern variables.
        //    This is crucial for test 6.2: when a local `let` shadows a pattern
        //    variable, `(syntax x)` should refer to the local binding.
        let pattern_binding = self.bindings_lookup(bindings, sym)?;
        let bound_locally = self.env_bound_anywhere(lex_env, sym)? && 
                           !self.env_bound_anywhere(self.global_env.0, sym)?;
        
        // Handle the case where both local and pattern bindings exist
        if bound_locally {
            if let Some(pattern_val) = pattern_binding {
                // Both local and pattern bindings exist. Check if the local binding
                // shadows the pattern binding by comparing the actual values.
                // If they're different, the local binding takes precedence.
                if let Some(env_binding) = self.lookup_in_env(sym, lex_env)?
                    && !self.lisp.eqv(env_binding, pattern_val)? {
                        let nil = self.lisp.nil()?;
                        return self.lisp.syntax_with_env(sym, nil, nil, lex_env).map_err(Into::into);
                    }
            } else {
                // Locally bound but NOT a pattern variable - wrap with lexical env
                let nil = self.lisp.nil()?;
                return self.lisp.syntax_with_env(sym, nil, nil, lex_env).map_err(Into::into);
            }
        }

        // 2. Check if it's a pattern variable (reuse the earlier lookup result)
        if let Some(val) = pattern_binding {
            return Ok(val);
        }

        // 3. Check if already renamed
        let renamed = self.rename_lookup(sym, renames)?;
        if !self.symbols_eq(renamed, sym)? {
            return Ok(renamed);
        }

        // 4. Check if bound in macro's definition environment
        // If so, keep original (it refers to macro's binding)
        if self.env_bound_anywhere(def_env, sym)? {
            return Ok(sym);
        }

        // 5. Otherwise, it's introduced by the macro - keep as-is
        Ok(sym)
    }
    
    /// Transcribe a list with lexical environment capture.
    /// Transcribe a list with optional lexical environment capture.
    fn transcribe_list_impl(
        &mut self,
        template: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
        lex_env: Option<ArenaIndex>,
    ) -> EvalResult {
        let (car, cdr) = self.lisp.car_cdr(template)?;

        // Check for escaping ellipsis: (... <template>) means treat <template> literally
        // (... ...) produces the literal symbol ...
        if self.lisp.symbol_matches(car, "...")? {
            // The cdr should be a single element - return it without ellipsis processing
            if let Value::Cons { .. } = self.lisp.get(cdr)? {
                let inner = self.lisp.car(cdr)?;
                return Ok(inner);
            }
            // (... . atom) - return the atom literally
            return Ok(cdr);
        }

        // Check for ellipsis
        if self.has_ellipsis(cdr)? {
            return self.transcribe_ellipsis(car, cdr, bindings, renames, def_env);
        }

        // Check for binding forms that need special handling
        if self.is_binding_keyword(car)? {
            return self.transcribe_binding_form(template, bindings, renames, def_env);
        }

        // Regular list: transcribe each element iteratively
        // Collect elements into arena cons list (reversed by prepending)
        let nil = self.lisp.nil()?;
        let mut collected = nil;
        let mut current = template;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let elem = self.lisp.car(current)?;
            collected = self.lisp.cons(elem, collected)?;
            
            current = self.lisp.cdr(current)?;
            
            // Check if we've hit a special form in the middle of the list
            if let Value::Cons { .. } = self.lisp.get(current)? {
                    let next_car = self.lisp.car(current)?;
                    let next_cdr = self.lisp.cdr(current)?;
                    
                    // Stop if we hit ellipsis or binding keyword in middle
                    if self.has_ellipsis(next_cdr)? || self.is_binding_keyword(next_car)? {
                        // Process collected elements, then handle rest specially
                        let rest_transcribed = self.transcribe_template_impl(current, bindings, renames, def_env, lex_env)?;
                        let mut result = rest_transcribed;
                        
                        // Walk reversed collected list, transcribe and cons (restores original order)
                        let mut cursor = collected;
                        while let Value::Cons { .. } = self.lisp.get(cursor)? {
                            let e = self.lisp.car(cursor)?;
                            let transcribed = self.transcribe_template_impl(e, bindings, renames, def_env, lex_env)?;
                            result = self.lisp.cons(transcribed, result)?;
                            cursor = self.lisp.cdr(cursor)?;
                        }
                        
                        return Ok(result);
                    }
                }
        }
        
        // Transcribe all collected elements and rebuild list
        // Walk reversed collected list: transcribing and consing restores original order
        let mut result = if self.lisp.get(current)?.is_nil() {
            nil
        } else {
            // Improper list - transcribe the tail
            self.transcribe_template_impl(current, bindings, renames, def_env, lex_env)?
        };
        
        let mut cursor = collected;
        while let Value::Cons { .. } = self.lisp.get(cursor)? {
            let elem = self.lisp.car(cursor)?;
            let transcribed = self.transcribe_template_impl(elem, bindings, renames, def_env, lex_env)?;
            result = self.lisp.cons(transcribed, result)?;
            cursor = self.lisp.cdr(cursor)?;
        }
        
        Ok(result)
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


    /// Find pattern variables that have list bindings (from ellipsis matching)
    fn find_ellipsis_vars(&self, template: ArenaIndex, bindings: ArenaIndex) -> EvalResult {
        let mut result = self.lisp.nil()?;
        self.find_ellipsis_vars_into(template, bindings, &mut result)?;
        Ok(result)
    }

    /// Find ellipsis vars in template (iterative)
    /// 
    /// Uses an arena-allocated work stack to avoid both stack overflow and fixed-size limits.
    fn find_ellipsis_vars_into(
        &self,
        template: ArenaIndex,
        bindings: ArenaIndex,
        result: &mut ArenaIndex,
    ) -> Result<(), EvalError> {
        // Use an arena-allocated cons list as a work stack
        let nil = self.lisp.nil()?;
        let mut stack = self.lisp.cons(template, nil)?;
        
        while !self.lisp.get(stack)?.is_nil() {
            // Pop from stack
            let current = self.lisp.car(stack)?;
            stack = self.lisp.cdr(stack)?;
            
            match self.lisp.get(current)? {
                Value::Symbol(_) => {
                    // Check if this symbol is bound to a list (including empty list)
                    // An ellipsis variable can be bound to:
                    // - Cons (non-empty list) - from one or more matches
                    // - Nil (empty list) - from zero matches
                    if let Some(val) = self.bindings_lookup(bindings, current)? {
                        match self.lisp.get(val)? {
                            Value::Cons { .. } | Value::Nil => {
                                // Add to result if not already there
                                // Note: result is a simple list of symbols, not an alist
                                if !self.symbol_in_list(current, *result)? {
                                    *result = self.lisp.cons(current, *result)?;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    
                    // Don't recurse into ellipsis
                    let should_process_cdr = !self.has_ellipsis(cdr)?;
                    
                    // Push to stack
                    if should_process_cdr {
                        stack = self.lisp.cons(cdr, stack)?;
                    }
                    stack = self.lisp.cons(car, stack)?;
                }
                _ => {}
            }
        }
        
        Ok(())
    }

    /// Create bindings for i-th iteration of ellipsis expansion
    fn make_iter_bindings(
        &self,
        bindings: ArenaIndex,
        ellipsis_vars: ArenaIndex,
        idx: usize,
    ) -> EvalResult {
        let mut result = bindings;
        let mut current = ellipsis_vars;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let var = self.lisp.car(current)?;
            if let Some(vals) = self.bindings_lookup(bindings, var)? {
                // Get the idx-th element
                let val = self.list_nth(vals, idx)?;
                result = self.bindings_extend(result, var, val)?;
            }
            current = self.lisp.cdr(current)?;
        }
        
        Ok(result)
    }

    /// Get the nth element of a list
    fn list_nth(&self, list: ArenaIndex, mut n: usize) -> EvalResult {
        let mut current = list;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return Ok(self.lisp.nil()?),
                Value::Cons { .. } => {
                    if n == 0 {
                        return self.lisp.car(current).map_err(Into::into);
                    }
                    n -= 1;
                    current = self.lisp.cdr(current)?;
                }
                _ => return Ok(self.lisp.nil()?),
            }
        }
    }

    /// Transcribe ellipsis template: (subtempl ... . rest) or (subtempl . ...)
    fn transcribe_ellipsis(
        &mut self,
        sub_template: ArenaIndex,
        template_cdr: ArenaIndex,   // (... . rest) or just the symbol ...
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> EvalResult {
        // Find pattern variables with ellipsis bindings in sub_template
        let ellipsis_vars = self.find_ellipsis_vars(sub_template, bindings)?;

        // Get rest template after ellipsis (nil if template_cdr is just ...)
        let rest = self.rest_after_ellipsis(template_cdr)?;

        if self.lisp.get(ellipsis_vars)?.is_nil() {
            // No ellipsis vars - transcribe once
            let transcribed = self.transcribe_template(
                sub_template, bindings, renames, def_env
            )?;
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

        // Transcribe and append rest (if any)
        if !self.lisp.get(rest)?.is_nil() {
            let rest_transcribed = self.transcribe_template(
                rest, bindings, renames, def_env
            )?;
            result = self.append_lists(result, rest_transcribed)?;
        }

        Ok(result)
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

    /// Handle binding forms (lambda, let, etc.) with hygienic renaming
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
               || self.lisp.symbol_matches(keyword, "letrec*")? {
            self.transcribe_let_form(keyword, args, bindings, renames, def_env)
        } else if self.lisp.symbol_matches(keyword, "define")? {
            self.transcribe_define_form(args, bindings, renames, def_env)
        } else {
            let new_keyword = self.transcribe_template(
                keyword, bindings, renames, def_env
            )?;
            let new_args = self.transcribe_template(
                args, bindings, renames, def_env
            )?;
            self.lisp.cons(new_keyword, new_args).map_err(Into::into)
        }
    }

    /// Transcribe a let/let*/letrec/letrec* form with hygienic renaming of binding variables
    ///
    /// For `(let ((a 3) (b 4)) (+ a b))`, the binding variables `a` and `b` need
    /// to be renamed if they are macro-introduced (not from pattern variables).
    /// This prevents macro-introduced binding names from capturing user variables.
    fn transcribe_let_form(
        &mut self,
        keyword: ArenaIndex,
        args: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> EvalResult {
        // args is ((var val) ... body ...)
        // First element should be the bindings list
        let let_bindings_template = self.lisp.car(args)?;
        let body_template = self.lisp.cdr(args)?;

        // Check for named let: (let name ((var val) ...) body ...)
        // where first element is a literal symbol (NOT a pattern variable)
        // that represents the loop name.
        // If the first element is a pattern variable (which may expand to a list),
        // we must NOT treat it as a named let.
        if let Value::Symbol(_) = self.lisp.get(let_bindings_template)? {
            let is_pattern_var = self.bindings_lookup(bindings, let_bindings_template)?.is_some();
            
            if !is_pattern_var {
                // Named let: first element is a literal symbol (loop name)
                let name_template = let_bindings_template;
                let actual_bindings_template = self.lisp.car(body_template)?;
                let actual_body_template = self.lisp.cdr(body_template)?;

                let transcribed_name = self.transcribe_template(name_template, bindings, renames, def_env)?;
                let mut new_renames = renames;
                // Named let loop name is always macro-introduced (it's a literal in the template)
                if let Value::Symbol(_) = self.lisp.get(transcribed_name)? {
                    let fresh = self.gensym_simple()?;
                    new_renames = self.rename_extend(new_renames, transcribed_name, fresh)?;
                    let (new_let_bindings, body_renames) = self.transcribe_let_bindings_hygiene(
                        actual_bindings_template, bindings, new_renames, def_env
                    )?;
                    let new_body = self.transcribe_template(actual_body_template, bindings, body_renames, def_env)?;

                    let new_keyword = self.transcribe_template(keyword, bindings, renames, def_env)?;
                    let rest = self.lisp.cons(new_let_bindings, new_body)?;
                    let rest2 = self.lisp.cons(fresh, rest)?;
                    return self.lisp.cons(new_keyword, rest2).map_err(Into::into);
                }
            }
            // Pattern variable or renamed to non-symbol - fall through to normal transcription
        }

        // Regular let: transcribe bindings with hygienic renaming
        let (new_let_bindings, body_renames) = self.transcribe_let_bindings_hygiene(
            let_bindings_template, bindings, renames, def_env
        )?;

        // Transcribe body with extended renames (so renamed vars are used in body)
        let new_body = self.transcribe_template(body_template, bindings, body_renames, def_env)?;

        let new_keyword = self.transcribe_template(keyword, bindings, renames, def_env)?;
        let rest = self.lisp.cons(new_let_bindings, new_body)?;
        self.lisp.cons(new_keyword, rest).map_err(Into::into)
    }

    /// Transcribe let-bindings with hygienic renaming of binding variables
    ///
    /// Only renames macro-introduced binding variables that would clash with
    /// user-provided binding variables (from pattern variables) in the same
    /// binding list. This prevents inadvertent variable capture while allowing
    /// intentional references to macro-introduced names from the body.
    fn transcribe_let_bindings_hygiene(
        &mut self,
        bindings_template: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> Result<(ArenaIndex, ArenaIndex), EvalError> {
        // If the bindings template is a symbol (pattern variable), just transcribe normally
        if let Value::Symbol(_) = self.lisp.get(bindings_template)? {
            let transcribed = self.transcribe_template(bindings_template, bindings, renames, def_env)?;
            return Ok((transcribed, renames));
        }

        // If it's nil (empty bindings), return as-is
        if self.lisp.get(bindings_template)?.is_nil() {
            return Ok((self.lisp.nil()?, renames));
        }

        // If the bindings template contains ellipsis or pattern variable tail,
        // the binding variables come from pattern variables - transcribe normally
        if let Value::Cons { .. } = self.lisp.get(bindings_template)? {
            let cdr = self.lisp.cdr(bindings_template)?;
            if self.has_ellipsis(cdr)? {
                let transcribed = self.transcribe_template(bindings_template, bindings, renames, def_env)?;
                return Ok((transcribed, renames));
            }
            if let Value::Symbol(_) = self.lisp.get(cdr)?
                && self.bindings_lookup(bindings, cdr)?.is_some() {
                    let transcribed = self.transcribe_template(bindings_template, bindings, renames, def_env)?;
                    return Ok((transcribed, renames));
                }
        }

        // Phase 1: Collect the transcribed names of user-provided binding variables
        // (pattern variables in binding positions).
        // Uses arena-allocated cons list instead of fixed-size array.
        let mut user_var_names = self.lisp.nil()?;
        let mut current = bindings_template;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let pair_template = self.lisp.car(current)?;
            if let Value::Cons { .. } = self.lisp.get(pair_template)? {
                let var_template = self.lisp.car(pair_template)?;
                if let Value::Symbol(_) = self.lisp.get(var_template)?
                    && self.bindings_lookup(bindings, var_template)?.is_some() {
                        // This is a pattern variable - transcribe to get the user's actual name
                        let transcribed = self.transcribe_template(var_template, bindings, renames, def_env)?;
                        user_var_names = self.lisp.cons(transcribed, user_var_names)?;
                    }
            }
            current = self.lisp.cdr(current)?;
        }

        // Phase 2: Transcribe each binding, renaming macro-introduced vars that
        // would clash with user-provided vars
        let mut new_renames = renames;
        let mut new_binding_pairs = self.lisp.nil()?;
        current = bindings_template;

        while let Value::Cons { .. } = self.lisp.get(current)? {
            let pair_template = self.lisp.car(current)?;

            if let Value::Cons { .. } = self.lisp.get(pair_template)? {
                let var_template = self.lisp.car(pair_template)?;
                let val_template = self.lisp.cdr(pair_template)?;

                let is_pattern_var = if let Value::Symbol(_) = self.lisp.get(var_template)? {
                    self.bindings_lookup(bindings, var_template)?.is_some()
                } else {
                    false
                };

                let transcribed_var = self.transcribe_template(var_template, bindings, new_renames, def_env)?;
                let transcribed_val = self.transcribe_template(val_template, bindings, new_renames, def_env)?;

                let final_var = if is_pattern_var {
                    // User-provided via pattern variable - keep as-is
                    transcribed_var
                } else if let Value::Symbol(_) = self.lisp.get(transcribed_var)? {
                    // Macro-introduced - check if it clashes with any user-provided var
                    let mut clashes = false;
                    let mut name_cursor = user_var_names;
                    while let Value::Cons { .. } = self.lisp.get(name_cursor)? {
                        let name = self.lisp.car(name_cursor)?;
                        if self.symbols_eq(transcribed_var, name)? {
                            clashes = true;
                            break;
                        }
                        name_cursor = self.lisp.cdr(name_cursor)?;
                    }
                    if clashes {
                        let fresh = self.gensym_simple()?;
                        new_renames = self.rename_extend(new_renames, transcribed_var, fresh)?;
                        fresh
                    } else {
                        transcribed_var
                    }
                } else {
                    transcribed_var
                };

                let new_pair = self.lisp.cons(final_var, transcribed_val)?;
                new_binding_pairs = self.lisp.cons(new_pair, new_binding_pairs)?;
            } else {
                let transcribed = self.transcribe_template(pair_template, bindings, new_renames, def_env)?;
                new_binding_pairs = self.lisp.cons(transcribed, new_binding_pairs)?;
            }

            current = self.lisp.cdr(current)?;
        }

        let final_bindings = self.reverse_list(new_binding_pairs)?;
        Ok((final_bindings, new_renames))
    }

    /// Transcribe a define form with hygienic renaming
    fn transcribe_define_form(
        &mut self,
        args: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> EvalResult {
        let name_template = self.lisp.car(args)?;
        let val_template = self.lisp.cdr(args)?;

        // Check if the name in the TEMPLATE is a pattern variable
        let is_pattern_var = if let Value::Symbol(_) = self.lisp.get(name_template)? {
            self.bindings_lookup(bindings, name_template)?.is_some()
        } else {
            false
        };

        let transcribed_name = self.transcribe_template(name_template, bindings, renames, def_env)?;

        let mut new_renames = renames;
        let final_name = if is_pattern_var {
            // User-provided via pattern variable
            transcribed_name
        } else if let Value::Symbol(_) = self.lisp.get(transcribed_name)? {
            let fresh = self.gensym_simple()?;
            new_renames = self.rename_extend(new_renames, transcribed_name, fresh)?;
            fresh
        } else {
            // Function shorthand: (define (f x) body)
            transcribed_name
        };

        let new_val = self.transcribe_template(val_template, bindings, new_renames, def_env)?;

        let define_sym = self.lisp.symbol("define")?;
        let rest = self.lisp.cons(final_name, new_val)?;
        self.lisp.cons(define_sym, rest).map_err(Into::into)
    }

    /// Transcribe lambda, renaming parameters for hygiene
    /// 
    /// This function needs to:
    /// 1. First transcribe the params list using normal transcription (handles ellipsis properly)
    /// 2. Then identify any macro-introduced symbols in the transcribed params and gensym them
    fn transcribe_lambda(
        &mut self,
        args: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
        def_env: ArenaIndex,
    ) -> EvalResult {
        let params_template = self.lisp.car(args)?;
        let body = self.lisp.cdr(args)?;

        // First, transcribe the params list using normal template transcription
        // This properly handles ellipsis patterns like (vars ...) -> (n acc)
        let transcribed_params = self.transcribe_template(params_template, bindings, renames, def_env)?;
        
        // Now walk through transcribed params and identify which are macro-introduced
        // (symbols that weren't in the original bindings and need gensym for hygiene)
        // Pass the original params_template so we can check which template params
        // were pattern variable keys vs template-literal symbols.
        let (new_params, new_renames) = self.identify_and_rename_introduced_params(
            params_template, transcribed_params, bindings, renames
        )?;

        // Transcribe body with extended renames
        let new_body = self.transcribe_template(body, bindings, new_renames, def_env)?;

        let lambda_sym = self.lisp.symbol("lambda")?;
        let inner = self.lisp.cons(new_params, new_body)?;
        self.lisp.cons(lambda_sym, inner).map_err(Into::into)
    }

    /// Identify and rename symbols in transcribed params that were introduced by the macro
    /// 
    /// # Arguments
    /// 
    /// * `original_template` - The original (pre-transcription) parameter template
    /// * `params` - Already transcribed parameter list (e.g., `(n acc)` after expanding `(vars ...)`)
    /// * `bindings` - Original pattern variable bindings from macro matching
    /// * `renames` - Current rename environment
    /// 
    /// To correctly identify whether a transcribed param is user-provided (from a pattern
    /// variable) or macro-introduced (template literal), we check the ORIGINAL template:
    /// - If the original template symbol was a pattern variable KEY → user-provided → keep
    /// - If the original template symbol was NOT a pattern variable KEY → macro-introduced → rename
    /// 
    /// This avoids false positives from `symbol_appears_in_binding_values` where a
    /// template-literal symbol happens to have the same name as a binding value.
    fn identify_and_rename_introduced_params(
        &mut self,
        original_template: ArenaIndex,
        params: ArenaIndex,
        bindings: ArenaIndex,
        renames: ArenaIndex,
    ) -> Result<(ArenaIndex, ArenaIndex), EvalError> {
        let mut new_params = self.lisp.nil()?;
        let mut new_renames = renames;
        let mut current = params;

        // Handle plain symbol formals (e.g., `(lambda args ...)`)
        // In this case, params is just a symbol, not a list at all
        if let Value::Symbol(_) = self.lisp.get(params)? {
            let is_from_pattern = self.is_param_from_pattern(original_template, params, bindings)?;
            let new_param = if is_from_pattern {
                params
            } else {
                let fresh = self.gensym_simple()?;
                new_renames = self.rename_extend(new_renames, params, fresh)?;
                fresh
            };
            return Ok((new_param, new_renames));
        }

        // Walk the original template and transcribed params together
        let mut orig_current = original_template;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let param = self.lisp.car(current)?;
            
            // Determine if this param was from a pattern variable by checking
            // the original template at this position
            let is_from_pattern = if let Value::Cons { .. } = self.lisp.get(orig_current)? {
                let orig_param = self.lisp.car(orig_current)?;
                let orig_cdr = self.lisp.cdr(orig_current)?;
                
                // Check if the original template has an ellipsis here
                if self.has_ellipsis(orig_cdr)? {
                    // Ellipsis: all remaining params are expanded from the pattern variable
                    // If the sub-pattern is a pattern variable, all expanded values are user-provided
                    self.bindings_lookup(bindings, orig_param)?.is_some()
                    // Don't advance orig_current - the ellipsis covers all remaining
                } else {
                    // Normal (non-ellipsis): check if original param is a pattern variable key
                    let result = self.bindings_lookup(bindings, orig_param)?.is_some();
                    orig_current = orig_cdr;
                    result
                }
            } else {
                // Original template exhausted but transcribed has more
                // (can happen with ellipsis expansion) - treat as from pattern
                self.symbol_appears_in_binding_values(param, bindings)?
            };

            let new_param = if is_from_pattern {
                // User-provided name - keep as is
                param
            } else {
                // Macro-introduced - generate fresh name for hygiene
                let fresh = self.gensym_simple()?;
                new_renames = self.rename_extend(new_renames, param, fresh)?;
                fresh
            };

            new_params = self.lisp.cons(new_param, new_params)?;
            current = self.lisp.cdr(current)?;
        }

        // Handle rest parameter for improper lists (e.g., `(a b . rest)`)
        // After the loop, `current` may be a symbol (the rest param) instead of nil
        let rest_param = if let Value::Symbol(_) = self.lisp.get(current)? {
            let is_from_pattern = self.is_param_from_pattern(original_template, current, bindings)?;
            if is_from_pattern {
                Some(current)
            } else {
                let fresh = self.gensym_simple()?;
                new_renames = self.rename_extend(new_renames, current, fresh)?;
                Some(fresh)
            }
        } else {
            None
        };

        // Reverse the proper list part and optionally append rest parameter
        let new_params = self.reverse_list_with_tail(new_params, rest_param)?;
        Ok((new_params, new_renames))
    }

    /// Check if a transcribed param originally came from a pattern variable.
    /// 
    /// For plain symbol templates, checks if the original template is a pattern variable key.
    /// For list templates, walks the original to find the rest/tail position.
    /// Falls back to `symbol_appears_in_binding_values` when the original template
    /// structure doesn't provide enough information.
    fn is_param_from_pattern(
        &self,
        original_template: ArenaIndex,
        param: ArenaIndex,
        bindings: ArenaIndex,
    ) -> Result<bool, EvalError> {
        // If the original template is a plain symbol, check if it's a pattern variable key
        if let Value::Symbol(_) = self.lisp.get(original_template)? {
            return Ok(self.bindings_lookup(bindings, original_template)?.is_some());
        }
        // For improper list rest params, check the tail of the original template
        let mut current = original_template;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            current = self.lisp.cdr(current)?;
        }
        // current is now the tail (nil for proper lists, symbol for improper)
        if let Value::Symbol(_) = self.lisp.get(current)?
            && self.bindings_lookup(bindings, current)?.is_some() {
                return Ok(true);
            }
        // Fall back to checking binding values
        self.symbol_appears_in_binding_values(param, bindings)
    }

    /// Reverse a list, optionally ending with an improper tail
    /// 
    /// This function is used for constructing improper list formals like `(a b . rest)`
    /// when transcribing lambda expressions in macros.
    /// 
    /// # Arguments
    /// 
    /// * `list` - The proper list to reverse (e.g., `(c b a)`)
    /// * `tail` - Optional tail element for improper list construction
    /// 
    /// # Returns
    /// 
    /// * With `tail=None`: Returns a proper list (e.g., `(c b a)` → `(a b c)`)
    /// * With `tail=Some(rest)`: Returns an improper list (e.g., `(c b a)` → `(a b c . rest)`)
    fn reverse_list_with_tail(&self, mut list: ArenaIndex, tail: Option<ArenaIndex>) -> EvalResult {
        let mut result = tail.unwrap_or(self.lisp.nil()?);
        loop {
            match self.lisp.get(list)? {
                Value::Nil => return Ok(result),
                Value::Cons { .. } => {
                    let car = self.lisp.car(list)?;
                    let cdr = self.lisp.cdr(list)?;
                    result = self.lisp.cons(car, result)?;
                    list = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, list)),
            }
        }
    }

    /// Check if a symbol appears in any binding value (including inside lists)
    fn symbol_appears_in_binding_values(
        &self,
        sym: ArenaIndex,
        bindings: ArenaIndex,
    ) -> Result<bool, EvalError> {
        let mut current = bindings;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let pair = self.lisp.car(current)?;
            let val = self.lisp.cdr(pair)?;
            if self.symbol_appears_in(sym, val)? {
                return Ok(true);
            }
            current = self.lisp.cdr(current)?;
        }
        Ok(false)
    }

    /// Check if a symbol appears in an expression
    /// 
    /// Iterative implementation using an arena-allocated work stack to avoid
    /// both stack overflow and fixed-size limits.
    fn symbol_appears_in(&self, sym: ArenaIndex, expr: ArenaIndex) -> Result<bool, EvalError> {
        // Use an arena-allocated cons list as a work stack
        let nil = self.lisp.nil()?;
        let mut stack = self.lisp.cons(expr, nil)?;
        
        while !self.lisp.get(stack)?.is_nil() {
            // Pop from stack
            let current = self.lisp.car(stack)?;
            stack = self.lisp.cdr(stack)?;
            
            match self.lisp.get(current)? {
                Value::Symbol(_) => {
                    if self.symbols_eq(sym, current)? {
                        return Ok(true);
                    }
                }
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    
                    // Push both car and cdr to stack
                    stack = self.lisp.cons(cdr, stack)?;
                    stack = self.lisp.cons(car, stack)?;
                }
                _ => {}
            }
        }
        
        Ok(false)
    }
}

// ============================================================================
// Macro Expander
// ============================================================================

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
    /// Expand an expression (iterative to avoid stack overflow)
    /// 
    /// This function uses a loop to handle tail recursion when macro expansion
    /// produces another expression that needs expansion.
    fn expand_expr(
        &mut self,
        mut expr: ArenaIndex,
        renames: ArenaIndex,
    ) -> EvalResult {
        // Loop to handle tail recursion from macro expansion
        loop {
            match self.lisp.get(expr)? {
                Value::Symbol(_) => {
                    // Apply any pending renames
                    return self.rename_lookup(expr, renames);
                }

                Value::Nil => return Ok(self.lisp.nil()?),

                Value::Cons { .. } => {
                    let head = self.lisp.car(expr)?;
                    let args = self.lisp.cdr(expr)?;

                    // Check for special forms that affect expansion
                    if let Value::Symbol(_) = self.lisp.get(head)? {
                        if self.lisp.symbol_matches(head, "quote")? {
                            // Don't expand inside quote
                            return Ok(expr);
                        }

                        // define-syntax is NOT processed during expansion - see step_eval_define_syntax
                        // in forms.rs for the evaluation-time implementation that captures lexical scope.

                        if self.lisp.symbol_matches(head, "let-syntax")? {
                            return self.expand_let_syntax(args, renames);
                        }

                        if self.lisp.symbol_matches(head, "letrec-syntax")? {
                            return self.expand_letrec_syntax(args, renames);
                        }

                        // Check for macro invocation
                        if let Some(transformer) = self.lookup_macro(head)? {
                            let expanded = self.apply_macro(transformer, expr)?;
                            // Re-expand the result (tail recursion - loop back)
                            expr = expanded;
                            continue;
                        }
                    }

                    // Not a macro - expand subexpressions
                    return self.expand_application(expr, renames);
                }

                // Atoms pass through unchanged
                _ => return Ok(expr),
            }
        }
    }

    /// Expand a function application (non-macro)
    /// 
    /// Iterative implementation to avoid stack overflow on deeply nested lists.
    fn expand_application(
        &mut self,
        expr: ArenaIndex,
        renames: ArenaIndex,
    ) -> EvalResult {
        if self.lisp.get(expr)?.is_nil() {
            return Ok(self.lisp.nil()?);
        }

        // Collect all elements into arena cons list (reversed by prepending)
        let nil = self.lisp.nil()?;
        let mut collected = nil;
        let mut current = expr;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let elem = self.lisp.car(current)?;
            collected = self.lisp.cons(elem, collected)?;
            current = self.lisp.cdr(current)?;
        }
        
        // Walk reversed list, expand each and cons onto result (restores original order)
        let mut result = if self.lisp.get(current)?.is_nil() {
            nil
        } else {
            // Improper list - keep the tail as-is
            current
        };
        
        let mut cursor = collected;
        while let Value::Cons { .. } = self.lisp.get(cursor)? {
            let elem = self.lisp.car(cursor)?;
            let expanded = self.expand_expr(elem, renames)?;
            result = self.lisp.cons(expanded, result)?;
            cursor = self.lisp.cdr(cursor)?;
        }
        
        Ok(result)
    }

    /// Look up macro in macro environment
    pub(super) fn lookup_macro(&self, name: ArenaIndex) -> Result<Option<ArenaIndex>, EvalError> {
        if !matches!(self.lisp.get(name)?, Value::Symbol(_)) {
            return Ok(None);
        }

        let mut current = self.macro_env.0;
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
    /// 
    /// All transformers are Lambda-based (procedural macros via syntax-case).
    /// The transformer lambda is called with the full expression as its argument.
    pub(super) fn apply_macro(
        &mut self,
        transformer: ArenaIndex,
        expr: ArenaIndex,
    ) -> EvalResult {
        match self.lisp.get(transformer)? {
            Value::Lambda { .. } => {
                self.apply_procedural_macro(transformer, expr)
            }
            _ => Err(self.make_error(ErrorKind::SyntaxError, transformer)
                .with_message("expected lambda transformer"))
        }
    }
    
    /// Apply a procedural (lambda-based) macro transformer
    /// 
    /// The transformer lambda is called with the full expression as its argument.
    /// It should return the expanded form.
    fn apply_procedural_macro(
        &mut self,
        transformer: ArenaIndex,
        expr: ArenaIndex,
    ) -> EvalResult {
        // Get lambda components
        let (params, body_env) = match self.lisp.get(transformer)? {
            Value::Lambda { params, body_env } => (params, body_env),
            _ => return Err(self.make_error(ErrorKind::SyntaxError, transformer)
                .with_message("expected lambda transformer")),
        };
        
        let body = self.lisp.car(body_env)?;
        let def_env = self.lisp.cdr(body_env)?;
        
        // The parameter should be a single symbol (e.g., (lambda (x) ...))
        // Bind it to the expression being expanded
        let param = self.lisp.car(params)?;
        let binding = self.lisp.cons(param, expr)?;
        let call_env = self.lisp.cons(binding, def_env)?;
        
        // Evaluate the transformer body in the extended environment
        // Use eval_for_macro which preserves outer continuation state
        // This enables full runtime capabilities during expansion while maintaining
        // proper nesting of evaluations
        self.eval_for_macro(ExprRef(body), EnvRef(call_env))
    }
    
    /// Apply a macro transformer using continuation-based evaluation.
    /// 
    /// This is the trampolined version of macro application that avoids Rust stack
    /// recursion for deeply nested/recursive macros. Instead of calling `eval_for_macro`
    /// (which creates a new nested trampoline), this pushes a continuation and returns
    /// a TrampolineState to evaluate the transformer body within the current trampoline.
    /// 
    /// When the transformer body completes, the ContType::MacroResult continuation will
    /// re-evaluate the result in the original environment. If the result is another
    /// macro invocation, it will be expanded the same way - using continuations instead
    /// of recursive function calls.
    /// 
    /// # Arguments
    /// 
    /// * `transformer` - The macro transformer (must be a Lambda value)
    /// * `expr` - The full macro invocation expression
    /// * `eval_env` - The environment where the expanded code should be evaluated
    /// 
    /// # Returns
    /// 
    /// A TrampolineState::Eval to evaluate the transformer body, with a ContType::MacroResult
    /// continuation pushed to handle the expansion result.
    pub(super) fn apply_macro_trampolined(
        &mut self,
        transformer: ArenaIndex,
        expr: ArenaIndex,
        eval_env: EnvRef,
    ) -> Result<TrampolineState, EvalError> {
        // Get lambda components
        let (params, body_env) = match self.lisp.get(transformer)? {
            Value::Lambda { params, body_env } => (params, body_env),
            _ => return Err(self.make_error(ErrorKind::SyntaxError, transformer)
                .with_message("expected lambda transformer")),
        };
        
        let body = self.lisp.car(body_env)?;
        let def_env = self.lisp.cdr(body_env)?;
        
        // The parameter should be a single symbol (e.g., (lambda (x) ...))
        // Bind it to the expression being expanded
        let param = self.lisp.car(params)?;
        let binding = self.lisp.cons(param, expr)?;
        let call_env = self.lisp.cons(binding, def_env)?;
        
        // Save the call-site environment for free-identifier=? literal matching.
        // When syntax-rules patterns contain literals like `else`, the pattern
        // matcher needs to check if the input identifier is locally bound at the
        // call site to distinguish it from the unbound literal keyword.
        let saved_call_site_env = self.call_site_env;
        self.call_site_env = eval_env;
        
        // Push continuation to re-evaluate the macro result in the original environment
        // Data: (eval_env . saved_call_site_env)
        // Note: cont_env is set to eval_env (not call_env) because if an error occurs
        // during re-evaluation, the relevant context is the expansion site, not the
        // transformer body's scope.
        self.cont(ContType::MacroResult, eval_env).data2(eval_env.0, saved_call_site_env.0)?;
        
        // Evaluate the transformer body in the extended environment
        // When this completes, ContType::MacroResult will re-evaluate the result
        Ok(TrampolineState::Eval { expr: ExprRef(body), env: EnvRef(call_env) })
    }
    
    // NOTE: The following functions have been removed as part of the unified
    // evaluator transition (see docs/DYNAMIC_RUNTIME_SYNTAX_CASE.md):
    // - eval_for_macro_expansion()
    // - eval_args_for_expansion()
    // - apply_for_expansion()
    // - bind_params_for_expansion()
    // - apply_builtin_for_expansion()
    // - builtin_add_for_expansion()
    // - builtin_sub_for_expansion()
    // - eval_syntax_case_for_expansion()
    // - extract_fender_and_output_expansion()
    // - extend_env_with_bindings_expansion()
    // - is_truthy_expansion()
    // - eval_syntax_for_expansion()
    // - get_pattern_bindings_from_env_expansion()
    // - eval_if_for_expansion()
    // - eval_begin_for_expansion()
    // - eval_let_for_expansion()
    // - eval_with_syntax_for_expansion()
    // - expand_define_syntax() - now handled at evaluation time by step_eval_define_syntax
    //
    // Procedural macro expansion now uses the unified evaluator via eval_for_macro().

    /// Expand a transformer expression if it's not already a lambda.
    /// 
    /// This allows macros like `syntax-rules` to be used to define other macros.
    /// If the expression is already a lambda, it's returned as-is to avoid
    /// expanding pattern variables in the lambda body.
    pub(super) fn expand_transformer_if_needed(&mut self, transformer_expr: ArenaIndex) -> EvalResult {
        if let Value::Cons { .. } = self.lisp.get(transformer_expr)? {
            let head = self.lisp.car(transformer_expr)?;
            if self.lisp.symbol_matches(head, "lambda")? {
                // Already a lambda - use as-is (don't expand body)
                Ok(transformer_expr)
            } else {
                // Not a lambda - expand it (e.g., syntax-rules call)
                self.expand(transformer_expr)
            }
        } else {
            Ok(transformer_expr)
        }
    }

    /// Parse a transformer expression (lambda (x) ...)
    /// 
    /// All macro transformers are procedural (lambda-based) using syntax-case.
    pub(super) fn parse_transformer(&mut self, expr: ArenaIndex) -> EvalResult {
        // Use global environment for backward compatibility
        self.parse_transformer_with_env(expr, self.global_env.0)
    }
    
    /// Parse a transformer expression with a specific lexical environment.
    /// 
    /// This variant allows macro transformers to capture lexical variables
    /// from their definition site, enabling tests like:
    /// ```scheme
    /// (let ((val 123))
    ///   (define-syntax test-macro
    ///     (lambda (_) val))  ; val should be resolved from let binding
    ///   (test-macro))
    /// ```
    pub(super) fn parse_transformer_with_env(&mut self, expr: ArenaIndex, env: ArenaIndex) -> EvalResult {
        let head = self.lisp.car(expr)?;

        // Check for lambda (procedural transformer)
        if self.lisp.symbol_matches(head, "lambda")? {
            // Evaluate the lambda to create a closure with the given environment
            let lambda_result = self.eval_lambda_for_transformer_with_env(expr, env)?;
            return Ok(lambda_result);
        }

        Err(self.make_error(ErrorKind::SyntaxError, expr)
            .with_message("expected (lambda ...)"))
    }

    /// Evaluate a lambda expression to create a transformer closure with a specific environment.
    fn eval_lambda_for_transformer_with_env(&self, lambda_expr: ArenaIndex, env: ArenaIndex) -> EvalResult {
        let rest = self.lisp.cdr(lambda_expr)?;
        let params = self.lisp.car(rest)?;
        let body_list = self.lisp.cdr(rest)?;
        
        // Check if body is a single expression (cdr is nil) or multiple
        // This is more efficient than calling list_length which traverses the whole list
        let first_body = self.lisp.car(body_list)?;
        let rest_body = self.lisp.cdr(body_list)?;
        
        let body = if self.lisp.get(rest_body)?.is_nil() {
            // Single expression - use directly
            first_body
        } else {
            // Multiple expressions - wrap in begin
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body_list)?
        };
        
        // Create Lambda value with the provided lexical environment
        self.lisp.lambda(params, body, env).map_err(Into::into)
    }

    /// Handle (let-syntax ((name transformer) ...) body ...)
    /// All transformers are parsed in the outer macro environment (parallel semantics).
    fn expand_let_syntax(
        &mut self,
        args: ArenaIndex,
        renames: ArenaIndex,
    ) -> EvalResult {
        self.expand_with_syntax_bindings(args, renames, true)
    }

    /// Handle (letrec-syntax ((name transformer) ...) body ...)
    /// Transformers are installed sequentially so each can see previous bindings.
    fn expand_letrec_syntax(
        &mut self,
        args: ArenaIndex,
        renames: ArenaIndex,
    ) -> EvalResult {
        self.expand_with_syntax_bindings(args, renames, false)
    }

    /// Shared implementation for let-syntax and letrec-syntax.
    /// When `parallel` is true, all transformers are parsed before installation (let-syntax).
    /// When false, each transformer is parsed and installed sequentially (letrec-syntax).
    fn expand_with_syntax_bindings(
        &mut self,
        args: ArenaIndex,
        renames: ArenaIndex,
        parallel: bool,
    ) -> EvalResult {
        let bindings = self.lisp.car(args)?;
        let body = self.lisp.cdr(args)?;
        let saved_macro_env = self.macro_env;

        if parallel {
            // Phase 1: Parse ALL transformers in the outer macro environment.
            let mut parsed_bindings = self.lisp.nil()?;
            let mut current = bindings;
            while let Value::Cons { .. } = self.lisp.get(current)? {
                let binding = self.lisp.car(current)?;
                let name = self.lisp.car(binding)?;
                let transformer_expr = self.lisp.car(self.lisp.cdr(binding)?)?;
                let transformer = self.parse_transformer(transformer_expr)?;
                let pair = self.lisp.cons(name, transformer)?;
                parsed_bindings = self.lisp.cons(pair, parsed_bindings)?;
                current = self.lisp.cdr(current)?;
            }
            // Phase 2: Install all bindings at once
            let mut cursor = parsed_bindings;
            while let Value::Cons { .. } = self.lisp.get(cursor)? {
                let pair = self.lisp.car(cursor)?;
                let name = self.lisp.car(pair)?;
                let transformer = self.lisp.cdr(pair)?;
                let macro_binding = self.lisp.cons(name, transformer)?;
                self.macro_env = EnvRef(self.lisp.cons(macro_binding, self.macro_env.0)?);
                cursor = self.lisp.cdr(cursor)?;
            }
        } else {
            // Install bindings sequentially - each transformer can see previous bindings
            let mut current = bindings;
            while let Value::Cons { .. } = self.lisp.get(current)? {
                let binding = self.lisp.car(current)?;
                let name = self.lisp.car(binding)?;
                let transformer_expr = self.lisp.car(self.lisp.cdr(binding)?)?;
                let transformer = self.parse_transformer(transformer_expr)?;
                let macro_binding = self.lisp.cons(name, transformer)?;
                self.macro_env = EnvRef(self.lisp.cons(macro_binding, self.macro_env.0)?);
                current = self.lisp.cdr(current)?;
            }
        }

        let expanded_body = self.expand_body(body, renames)?;
        self.macro_env = saved_macro_env;

        if self.list_length(expanded_body)? == 1 {
            self.lisp.car(expanded_body).map_err(Into::into)
        } else {
            let begin_sym = self.lisp.symbol("begin")?;
            self.lisp.cons(begin_sym, expanded_body).map_err(Into::into)
        }
    }

    /// Expand a body (list of expressions)
    /// 
    /// Iterative implementation to avoid stack overflow on deeply nested lists.
    fn expand_body(
        &mut self,
        body: ArenaIndex,
        renames: ArenaIndex,
    ) -> EvalResult {
        if self.lisp.get(body)?.is_nil() {
            return Ok(self.lisp.nil()?);
        }

        // Collect all elements into arena cons list (reversed by prepending)
        let nil = self.lisp.nil()?;
        let mut collected = nil;
        let mut current = body;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let elem = self.lisp.car(current)?;
            collected = self.lisp.cons(elem, collected)?;
            current = self.lisp.cdr(current)?;
        }
        
        // Walk reversed list, expand each and cons onto result (restores original order)
        let mut result = nil;
        let mut cursor = collected;
        while let Value::Cons { .. } = self.lisp.get(cursor)? {
            let elem = self.lisp.car(cursor)?;
            let expanded = self.expand_expr(elem, renames)?;
            result = self.lisp.cons(expanded, result)?;
            cursor = self.lisp.cdr(cursor)?;
        }
        
        Ok(result)
    }
}

// ============================================================================
// Syntax-case Support Functions
// ============================================================================

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Recursively strip syntax wrappers to get the underlying datum.
    ///
    /// This walks through the expression, converting:
    /// - Syntax objects to their wrapped datum
    /// - Pairs to pairs with recursively unwrapped car and cdr
    /// - Atoms pass through unchanged
    ///
    /// # Example
    ///
    /// ```scheme
    /// (syntax->datum #'(a b c))  ; => (a b c)
    /// (syntax->datum #'42)       ; => 42
    /// ```
    /// Unwrap syntax objects to get datums (iterative)
    /// 
    /// Uses iteration for list traversal to avoid stack overflow.
    pub fn syntax_to_datum_recursive(&mut self, stx: ArenaIndex) -> EvalResult {
        // Handle simple cases directly
        match self.lisp.get(stx)? {
            Value::Syntax { expr, .. } => {
                // Unwrap one level and continue
                return self.syntax_to_datum_recursive(expr);
            }
            Value::Cons { .. } => {
                // Process list iteratively
            }
            // Atoms pass through unchanged
            _ => return Ok(stx),
        }
        
        // Process list iteratively using arena cons list
        let nil = self.lisp.nil()?;
        let mut collected = nil;
        let mut current = stx;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let elem = self.lisp.car(current)?;
            collected = self.lisp.cons(elem, collected)?;
            current = self.lisp.cdr(current)?;
        }
        
        // Walk reversed list, process each and cons onto tail (restores original order)
        let mut result = if self.lisp.get(current)?.is_nil() {
            nil
        } else {
            self.syntax_to_datum_recursive(current)?
        };
        
        let mut cursor = collected;
        while let Value::Cons { .. } = self.lisp.get(cursor)? {
            let elem = self.lisp.car(cursor)?;
            let processed = self.syntax_to_datum_recursive(elem)?;
            result = self.lisp.cons(processed, result)?;
            cursor = self.lisp.cdr(cursor)?;
        }
        
        Ok(result)
    }

    /// Wrap a datum with syntax context from a template identifier.
    ///
    /// The template-id provides the lexical context (marks and substitutions)
    /// for the resulting syntax object. This is essential for creating
    /// hygienically correct identifiers in procedural macros.
    ///
    /// # Arguments
    ///
    /// * `template_id` - An identifier whose lexical context is used
    /// * `datum` - The datum to wrap
    ///
    /// # Example
    ///
    /// ```scheme
    /// (datum->syntax #'here 'new-name)  ; Creates syntax object with 'here's context
    /// ```
    pub fn datum_to_syntax(
        &mut self,
        template_id: ArenaIndex,
        datum: ArenaIndex,
    ) -> EvalResult {
        // Get the context from template_id (including lexical environment)
        let (marks, subst, lex_env) = match self.lisp.get(template_id)? {
            Value::Syntax { .. } => {
                let (_, marks, subst, lex_env) = self.lisp.syntax_parts_with_env(template_id)?;
                (marks, subst, lex_env)
            }
            // If template_id is just a symbol, use empty context
            Value::Symbol(_) => {
                let nil = self.lisp.nil()?;
                (nil, nil, nil)
            }
            _ => {
                // Non-identifier - use empty context
                let nil = self.lisp.nil()?;
                (nil, nil, nil)
            }
        };

        // Recursively wrap the datum with full context including lexical environment
        self.datum_to_syntax_with_context(datum, marks, subst, lex_env)
    }

    /// Helper to wrap a datum with a given context (iterative)
    /// 
    /// Uses iteration for list traversal to avoid stack overflow.
    fn datum_to_syntax_with_context(
        &mut self,
        datum: ArenaIndex,
        marks: ArenaIndex,
        subst: ArenaIndex,
        lex_env: ArenaIndex,
    ) -> EvalResult {
        // Handle simple cases directly
        match self.lisp.get(datum)? {
            // Symbols become syntax objects with captured lexical environment
            Value::Symbol(_) => {
                return self.lisp.syntax_with_env(datum, marks, subst, lex_env).map_err(Into::into);
            }
            Value::Cons { .. } => {
                // Process list iteratively
            }
            // Other atoms pass through unchanged (numbers, strings, etc.)
            _ => return Ok(datum),
        }
        
        // Process list iteratively using arena cons list
        let nil = self.lisp.nil()?;
        let mut collected = nil;
        let mut current = datum;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let elem = self.lisp.car(current)?;
            collected = self.lisp.cons(elem, collected)?;
            current = self.lisp.cdr(current)?;
        }
        
        // Walk reversed list, process each and cons onto tail (restores original order)
        let mut result = if self.lisp.get(current)?.is_nil() {
            nil
        } else {
            self.datum_to_syntax_with_context(current, marks, subst, lex_env)?
        };
        
        let mut cursor = collected;
        while let Value::Cons { .. } = self.lisp.get(cursor)? {
            let elem = self.lisp.car(cursor)?;
            let processed = self.datum_to_syntax_with_context(elem, marks, subst, lex_env)?;
            result = self.lisp.cons(processed, result)?;
            cursor = self.lisp.cdr(cursor)?;
        }
        
        Ok(result)
    }

    /// Generate a list of fresh temporary identifiers.
    ///
    /// For each element in the input list, generates a unique temporary
    /// identifier using gensym. The temporary identifiers are syntax objects
    /// with the same lexical context.
    ///
    /// # Arguments
    ///
    /// * `input` - A list (length determines number of temporaries generated)
    ///
    /// # Example
    ///
    /// ```scheme
    /// (generate-temporaries '(a b c))  ; => (#:tmp0 #:tmp1 #:tmp2)
    /// ```
    pub fn generate_temporaries(&mut self, input: ArenaIndex) -> EvalResult {
        let mut result = self.lisp.nil()?;
        let mut current = input;

        // Count elements and generate that many temporaries
        while let Value::Cons { .. } = self.lisp.get(current)? {
            // Generate a fresh temporary symbol
            let temp = self.gensym("tmp")?;
            
            // Wrap it as a syntax object with empty context
            let nil = self.lisp.nil()?;
            let stx = self.lisp.syntax(temp, nil, nil)?;
            
            result = self.lisp.cons(stx, result)?;
            current = self.lisp.cdr(current)?;
        }

        // Reverse the list to maintain order
        self.reverse_list(result)
    }
}

