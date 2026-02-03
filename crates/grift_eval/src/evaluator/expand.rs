//! Hygienic macro expansion for syntax-rules.
//!
//! This module implements the "Macros that Work" algorithm (Clinger & Rees, 1991)
//! for R7RS-compatible hygienic macros.

use grift_parser::{ArenaIndex, Value};

use crate::error::{ErrorKind, EvalError, EvalResult};
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
    /// ```ignore
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
        let (expr, marks, subst) = self.lisp.syntax_parts(stx)?;

        // Generate fresh mark
        let new_mark = self.gensym_simple()?;

        // Add to marks list (prepend for efficiency)
        let new_marks = self.lisp.cons(new_mark, marks)?;

        // Create new syntax object with updated marks
        self.lisp.syntax(expr, new_marks, subst).map_err(Into::into)
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
    ///   (syntax-rules ()
    ///     ((test x) (let ((x 1)) x))))
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
    /// associated with the syntax object, or in the global environment.
    ///
    /// # Arguments
    ///
    /// * `id` - An identifier (as a syntax object or symbol)
    ///
    /// # Returns
    ///
    /// `Some(binding)` if the identifier is bound, `None` if unbound
    fn resolve_identifier(&self, id: ArenaIndex) -> Result<Option<ArenaIndex>, EvalError> {
        // If it's a syntax object, check its substitution environment first
        if let Value::Syntax { .. } = self.lisp.get(id)? {
            let (name, _marks, subst) = self.lisp.syntax_parts(id)?;
            
            // Check substitution environment
            if let Some(binding) = self.lookup_in_subst(name, subst)? {
                return Ok(Some(binding));
            }
            
            // Fall through to check global environment using the unwrapped name
            return self.lookup_in_env(name, self.global_env);
        }
        
        // For plain symbols, check the global environment
        self.lookup_in_env(id, self.global_env)
    }

    /// Look up a name in a substitution environment
    ///
    /// The substitution environment is an alist of (name . binding) pairs.
    fn lookup_in_subst(
        &self,
        name: ArenaIndex,
        subst: ArenaIndex,
    ) -> Result<Option<ArenaIndex>, EvalError> {
        let mut current = subst;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let pair = self.lisp.car(current)?;
            let key = self.lisp.car(pair)?;
            
            if self.lisp.symbol_eq(key, name)? {
                return Ok(Some(self.lisp.cdr(pair)?));
            }
            
            current = self.lisp.cdr(current)?;
        }
        
        Ok(None)
    }

    /// Look up a name in an environment
    fn lookup_in_env(
        &self,
        name: ArenaIndex,
        env: ArenaIndex,
    ) -> Result<Option<ArenaIndex>, EvalError> {
        let mut current = env;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let binding = self.lisp.car(current)?;
            
            if let Value::Cons { .. } = self.lisp.get(binding)? {
                let bound_name = self.lisp.car(binding)?;
                
                if self.lisp.symbol_eq(bound_name, name)? {
                    return Ok(Some(self.lisp.cdr(binding)?));
                }
            }
            
            current = self.lisp.cdr(current)?;
        }
        
        Ok(None)
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
            // Both unbound - compare names
            (None, None) => {
                let name1 = self.lisp.syntax_to_datum(id1)?;
                let name2 = self.lisp.syntax_to_datum(id2)?;
                self.lisp.eqv(name1, name2).map_err(Into::into)
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
        let mut current = renames;
        loop {
            if self.lisp.get(current)?.is_nil() {
                return Ok(sym); // Not renamed
            }
            let pair = self.lisp.car(current)?;
            let key = self.lisp.car(pair)?;
            if self.lisp.symbol_eq(key, sym)? {
                return self.lisp.cdr(pair).map_err(Into::into);
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
                Value::Cons { .. } => {
                    let binding = self.lisp.car(current)?;
                    if let Value::Cons { .. } = self.lisp.get(binding)? {
                        let bound_name = self.lisp.car(binding)?;
                        if self.lisp.symbol_eq(bound_name, name)? {
                            return Ok(true);
                        }
                    }
                    current = self.lisp.cdr(current)?;
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

    /// Get minimum length a pattern requires
    fn pattern_min_length(&self, pattern: ArenaIndex, literals: ArenaIndex) -> Result<usize, EvalError> {
        match self.lisp.get(pattern)? {
            Value::Nil => Ok(0),
            Value::Cons { .. } => {
                let cdr = self.lisp.cdr(pattern)?;
                // Check for ellipsis which consumes variable number of elements
                if self.has_ellipsis(cdr)? {
                    // Pattern with ellipsis: element before ... is repeated
                    // Get rest pattern (nil if cdr is just the symbol ...)
                    let rest = match self.lisp.get(cdr)? {
                        Value::Symbol(_) => self.lisp.nil()?,  // ... as improper list cdr
                        Value::Cons { .. } => self.lisp.cdr(cdr)?,  // (... . rest)
                        _ => self.lisp.nil()?,
                    };
                    return self.pattern_min_length(rest, literals);
                }
                // Regular element: 1 + rest
                let rest_len = self.pattern_min_length(cdr, literals)?;
                Ok(1 + rest_len)
            }
            _ => Ok(0),
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
                
                // Handle ellipsis - two cases:
                // 1. cdr is the symbol ... (improper list like (a . ...))
                // 2. cdr is a list starting with ... (like (a ... rest))
                match self.lisp.get(cdr)? {
                    Value::Symbol(_) if self.lisp.symbol_matches(cdr, "...")? => {
                        // Case 1: cdr IS the ellipsis symbol - no more vars to collect
                        return Ok(());
                    }
                    Value::Cons { .. } => {
                        let first = self.lisp.car(cdr)?;
                        if self.lisp.symbol_matches(first, "...")? {
                            // Case 2: cdr starts with ... - skip it and process rest
                            let rest = self.lisp.cdr(cdr)?;
                            return self.collect_pattern_vars_into(rest, literals, vars);
                        }
                    }
                    _ => {}
                }
                self.collect_pattern_vars_into(cdr, literals, vars)
            }
            _ => Ok(()),
        }
    }

    /// Match an expression against a pattern
    ///
    /// Returns `Ok(Some(bindings))` on success, `Ok(None)` on failure.
    pub(crate) fn match_pattern(
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
        let pat_car = self.lisp.car(pattern)?;
        let pat_cdr = self.lisp.cdr(pattern)?;

        // Check for ellipsis FIRST - ellipsis can match empty lists
        // (subpat ... . rest) or (subpat . ...)
        if self.has_ellipsis(pat_cdr)? {
            return self.match_ellipsis_pattern(
                pat_car, pat_cdr, expr, literals, bindings
            );
        }
        
        // For non-ellipsis patterns, expression must be a non-empty list
        if !matches!(self.lisp.get(expr)?, Value::Cons { .. }) {
            return Ok(None);
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

    /// Match ellipsis pattern: (subpat ... . rest)
    fn match_ellipsis_pattern(
        &self,
        sub_pattern: ArenaIndex,
        pat_cdr: ArenaIndex,      // (... . rest) or just the symbol ...
        expr: ArenaIndex,
        literals: ArenaIndex,
        bindings: ArenaIndex,
    ) -> Result<Option<ArenaIndex>, EvalError> {
        // Get the rest pattern after ...
        // If pat_cdr is the symbol ... itself (improper list), there's no rest pattern
        // If pat_cdr is (... . rest), get the rest
        let rest_pattern = match self.lisp.get(pat_cdr)? {
            Value::Symbol(_) => self.lisp.nil()?,  // ... as improper list cdr - no rest
            Value::Cons { .. } => self.lisp.cdr(pat_cdr)?,  // (... . rest) - get rest
            _ => self.lisp.nil()?,
        };

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

            Value::Nil => Ok(self.lisp.nil()?),

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

    /// Find pattern variables that have list bindings (from ellipsis matching)
    fn find_ellipsis_vars(&self, template: ArenaIndex, bindings: ArenaIndex) -> EvalResult {
        let mut result = self.lisp.nil()?;
        self.find_ellipsis_vars_into(template, bindings, &mut result)?;
        Ok(result)
    }

    fn find_ellipsis_vars_into(
        &self,
        template: ArenaIndex,
        bindings: ArenaIndex,
        result: &mut ArenaIndex,
    ) -> Result<(), EvalError> {
        match self.lisp.get(template)? {
            Value::Symbol(_) => {
                // Check if this symbol is bound to a list (including empty list)
                // An ellipsis variable can be bound to:
                // - Cons (non-empty list) - from one or more matches
                // - Nil (empty list) - from zero matches
                if let Some(val) = self.bindings_lookup(bindings, template)? {
                    match self.lisp.get(val)? {
                        Value::Cons { .. } | Value::Nil => {
                            // Add to result if not already there
                            if self.bindings_lookup(*result, template)?.is_none() {
                                *result = self.lisp.cons(template, *result)?;
                            }
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
            Value::Cons { .. } => {
                let car = self.lisp.car(template)?;
                let cdr = self.lisp.cdr(template)?;
                self.find_ellipsis_vars_into(car, bindings, result)?;
                // Don't recurse into ellipsis
                if !self.has_ellipsis(cdr)? {
                    self.find_ellipsis_vars_into(cdr, bindings, result)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
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
        let rest = match self.lisp.get(template_cdr)? {
            Value::Symbol(_) => self.lisp.nil()?,  // ... as improper list cdr - no rest
            Value::Cons { .. } => self.lisp.cdr(template_cdr)?,  // (... . rest) - get rest
            _ => self.lisp.nil()?,
        };

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
        } else {
            // Other binding forms - transcribe normally for now
            // (let forms will be expanded as macros after bootstrap)
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
        let (new_params, new_renames) = self.identify_and_rename_introduced_params(
            transcribed_params, bindings, renames
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
    /// * `params` - Already transcribed parameter list (e.g., `(n acc)` after expanding `(vars ...)`)
    /// * `bindings` - Original pattern variable bindings from macro matching
    /// * `renames` - Current rename environment
    /// 
    /// After transcription, `params` is a list of actual symbols. We need to check each one
    /// to see if it was in the original pattern bindings (user-provided) or if it's 
    /// a macro-introduced symbol that needs gensym for hygiene.
    fn identify_and_rename_introduced_params(
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

            // Check if this param came from a pattern variable value
            // by checking if it appears as a value in any of the bindings
            let is_from_pattern = self.symbol_appears_in_binding_values(param, bindings)?;

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

        let new_params = self.reverse_list(new_params)?;
        Ok((new_params, new_renames))
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

    /// Check if a symbol appears in an expression (recursively)
    fn symbol_appears_in(&self, sym: ArenaIndex, expr: ArenaIndex) -> Result<bool, EvalError> {
        match self.lisp.get(expr)? {
            Value::Symbol(_) => self.symbols_eq(sym, expr),
            Value::Cons { .. } => {
                let car = self.lisp.car(expr)?;
                let cdr = self.lisp.cdr(expr)?;
                if self.symbol_appears_in(sym, car)? {
                    return Ok(true);
                }
                self.symbol_appears_in(sym, cdr)
            }
            _ => Ok(false),
        }
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

            Value::Nil => Ok(self.lisp.nil()?),

            Value::Cons { .. } => {
                let head = self.lisp.car(expr)?;
                let args = self.lisp.cdr(expr)?;

                // Check for special forms that affect expansion
                if let Value::Symbol(_) = self.lisp.get(head)? {
                    if self.lisp.symbol_matches(head, "quote")? {
                        // Don't expand inside quote
                        return Ok(expr);
                    }

                    if self.lisp.symbol_matches(head, "define-syntax")? {
                        return self.expand_define_syntax(args);
                    }

                    if self.lisp.symbol_matches(head, "let-syntax")? {
                        return self.expand_let_syntax(args, renames);
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
            return Ok(self.lisp.nil()?);
        }

        let car = self.lisp.car(expr)?;
        let cdr = self.lisp.cdr(expr)?;

        let new_car = self.expand_expr(car, renames)?;
        let new_cdr = self.expand_application(cdr, renames)?;

        self.lisp.cons(new_car, new_cdr).map_err(Into::into)
    }

    /// Look up macro in macro environment
    pub(super) fn lookup_macro(&self, name: ArenaIndex) -> Result<Option<ArenaIndex>, EvalError> {
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
    pub(super) fn apply_macro(
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
        Ok(self.lisp.nil()?)
    }

    /// Parse a transformer expression (syntax-rules ...)
    pub(super) fn parse_transformer(&self, expr: ArenaIndex) -> EvalResult {
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
            .map_err(Into::into)
    }

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
            self.lisp.car(expanded_body).map_err(Into::into)
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
            return Ok(self.lisp.nil()?);
        }

        let first = self.lisp.car(body)?;
        let rest = self.lisp.cdr(body)?;

        let expanded_first = self.expand_expr(first, renames)?;
        let expanded_rest = self.expand_body(rest, renames)?;

        self.lisp.cons(expanded_first, expanded_rest).map_err(Into::into)
    }
}

