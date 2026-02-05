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
    /// ```
    /// use grift_parser::Lisp;
    /// use grift_eval::Evaluator;
    /// 
    /// let lisp: Lisp<10000> = Lisp::new();
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

    /// Check if a symbol matches the current ellipsis
    fn is_ellipsis(&self, sym: ArenaIndex) -> Result<bool, EvalError> {
        self.lisp.symbol_eq(sym, self.current_ellipsis).map_err(Into::into)
    }

    /// Check if pattern cdr starts with ellipsis
    fn has_ellipsis(&self, pat_cdr: ArenaIndex) -> Result<bool, EvalError> {
        match self.lisp.get(pat_cdr)? {
            // Case 1: pat_cdr is a list starting with current ellipsis
            // Pattern like: (a ... rest) parsed as (a . (... . rest))
            Value::Cons { .. } => {
                let first = self.lisp.car(pat_cdr)?;
                self.is_ellipsis(first)
            }
            // Case 2: pat_cdr IS the ellipsis symbol itself
            // Pattern like: (a ...) parsed as improper list (a . ...)
            Value::Symbol(_) => {
                self.is_ellipsis(pat_cdr)
            }
            _ => Ok(false),
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
                        let rest = match self.lisp.get(cdr)? {
                            Value::Symbol(_) => self.lisp.nil()?,  // ... as improper list cdr
                            Value::Cons { .. } => self.lisp.cdr(cdr)?,  // (... . rest)
                            _ => self.lisp.nil()?,
                        };
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
    /// Iterative implementation using a work queue to avoid stack overflow.
    fn collect_pattern_vars_into(
        &self,
        pattern: ArenaIndex,
        literals: ArenaIndex,
        vars: &mut ArenaIndex,
    ) -> Result<(), EvalError> {
        // Use a work queue for depth-first traversal
        let mut queue = [ArenaIndex::new(0); 64];
        let mut queue_len = 1;
        queue[0] = pattern;
        
        while queue_len > 0 {
            // Pop from queue
            queue_len -= 1;
            let current = queue[queue_len];
            
            match self.lisp.get(current)? {
                Value::Symbol(_) => {
                    // Skip _, ellipsis, and literals
                    if !self.lisp.symbol_matches(current, "_")?
                        && !self.is_ellipsis(current)?
                        && !self.is_literal(current, literals)?
                    {
                        *vars = self.lisp.cons(current, *vars)?;
                    }
                }
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    
                    // Handle ellipsis - two cases:
                    // 1. cdr is the ellipsis symbol (improper list like (a . ...))
                    // 2. cdr is a list starting with ellipsis (like (a ... rest))
                    let should_process_cdr = match self.lisp.get(cdr)? {
                        Value::Symbol(_) if self.is_ellipsis(cdr)? => {
                            // Case 1: cdr IS the ellipsis symbol - no more vars to collect
                            false
                        }
                        Value::Cons { .. } => {
                            let first = self.lisp.car(cdr)?;
                            if self.is_ellipsis(first)? {
                                // Case 2: cdr starts with ellipsis - skip it and process rest
                                let rest = self.lisp.cdr(cdr)?;
                                if queue_len < queue.len() {
                                    queue[queue_len] = rest;
                                    queue_len += 1;
                                }
                                false
                            } else {
                                true
                            }
                        }
                        _ => true,
                    };
                    
                    // Add car and cdr to queue
                    if queue_len + 2 > queue.len() {
                        // Queue full - this is unlikely for normal patterns
                        // Just process car now and continue with cdr
                        self.collect_pattern_vars_into(car, literals, vars)?;
                        if should_process_cdr {
                            self.collect_pattern_vars_into(cdr, literals, vars)?;
                        }
                    } else {
                        if should_process_cdr {
                            queue[queue_len] = cdr;
                            queue_len += 1;
                        }
                        queue[queue_len] = car;
                        queue_len += 1;
                    }
                }
                _ => {}
            }
        }
        
        Ok(())
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
            Value::Symbol(_) if self.is_ellipsis(pattern)? => {
                Err(self.make_error(ErrorKind::SyntaxError, pattern)
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
    pub(crate) fn transcribe_template(
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
    /// 
    /// Iterative implementation to avoid stack overflow on deeply nested templates.
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

        // Regular list: transcribe each element iteratively
        // Collect all elements first
        let mut elements = [ArenaIndex::new(0); 128]; // Stack-allocated buffer
        let mut count = 0;
        let mut current = template;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            if count >= elements.len() {
                return Err(self.make_error(ErrorKind::Generic, template));
            }
            elements[count] = self.lisp.car(current)?;
            count += 1;
            
            current = self.lisp.cdr(current)?;
            
            // Check if we've hit a special form in the middle of the list
            if count > 0 {
                if let Value::Cons { .. } = self.lisp.get(current)? {
                    let next_car = self.lisp.car(current)?;
                    let next_cdr = self.lisp.cdr(current)?;
                    
                    // Stop if we hit ellipsis or binding keyword in middle
                    if self.has_ellipsis(next_cdr)? || self.is_binding_keyword(next_car)? {
                        // Process collected elements, then handle rest specially
                        // Process the remaining special part
                        let rest_transcribed = self.transcribe_template(current, bindings, renames, def_env)?;
                        let mut result = rest_transcribed;
                        
                        // Add the collected elements in reverse
                        for i in (0..count).rev() {
                            let transcribed = self.transcribe_template(elements[i], bindings, renames, def_env)?;
                            result = self.lisp.cons(transcribed, result)?;
                        }
                        
                        return Ok(result);
                    }
                }
            }
        }
        
        // Transcribe all collected elements
        for i in 0..count {
            elements[i] = self.transcribe_template(elements[i], bindings, renames, def_env)?;
        }
        
        // Rebuild the list from the end
        let mut result = if self.lisp.get(current)?.is_nil() {
            self.lisp.nil()?
        } else {
            // Improper list - transcribe the tail
            self.transcribe_template(current, bindings, renames, def_env)?
        };
        
        for i in (0..count).rev() {
            result = self.lisp.cons(elements[i], result)?;
        }
        
        Ok(result)
    }

    /// Find pattern variables that have list bindings (from ellipsis matching)
    fn find_ellipsis_vars(&self, template: ArenaIndex, bindings: ArenaIndex) -> EvalResult {
        let mut result = self.lisp.nil()?;
        self.find_ellipsis_vars_into(template, bindings, &mut result)?;
        Ok(result)
    }

    /// Find ellipsis vars in template (iterative)
    /// 
    /// Uses a work queue to avoid stack overflow.
    fn find_ellipsis_vars_into(
        &self,
        template: ArenaIndex,
        bindings: ArenaIndex,
        result: &mut ArenaIndex,
    ) -> Result<(), EvalError> {
        // Use a work queue
        let mut queue = [ArenaIndex::new(0); 64];
        let mut queue_len = 1;
        queue[0] = template;
        
        while queue_len > 0 {
            // Pop from queue
            queue_len -= 1;
            let current = queue[queue_len];
            
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
                                if self.bindings_lookup(*result, current)?.is_none() {
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
                    
                    // Add to queue
                    if queue_len + 2 > queue.len() {
                        // Queue full - fall back to recursive
                        self.find_ellipsis_vars_into(car, bindings, result)?;
                        if should_process_cdr {
                            self.find_ellipsis_vars_into(cdr, bindings, result)?;
                        }
                    } else {
                        if should_process_cdr {
                            queue[queue_len] = cdr;
                            queue_len += 1;
                        }
                        queue[queue_len] = car;
                        queue_len += 1;
                    }
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

        // Handle plain symbol formals (e.g., `(lambda args ...)`)
        // In this case, params is just a symbol, not a list at all
        if let Value::Symbol(_) = self.lisp.get(params)? {
            let is_from_pattern = self.symbol_appears_in_binding_values(params, bindings)?;
            let new_param = if is_from_pattern {
                params
            } else {
                let fresh = self.gensym_simple()?;
                new_renames = self.rename_extend(new_renames, params, fresh)?;
                fresh
            };
            return Ok((new_param, new_renames));
        }

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

        // Handle rest parameter for improper lists (e.g., `(a b . rest)`)
        // After the loop, `current` may be a symbol (the rest param) instead of nil
        let rest_param = if let Value::Symbol(_) = self.lisp.get(current)? {
            let is_from_pattern = self.symbol_appears_in_binding_values(current, bindings)?;
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
    /// Iterative implementation using a work queue to avoid stack overflow.
    fn symbol_appears_in(&self, sym: ArenaIndex, expr: ArenaIndex) -> Result<bool, EvalError> {
        // Use a fixed-size work queue for depth-first traversal
        let mut queue = [ArenaIndex::new(0); 64];
        let mut queue_len = 1;
        queue[0] = expr;
        
        while queue_len > 0 {
            // Pop from queue
            queue_len -= 1;
            let current = queue[queue_len];
            
            match self.lisp.get(current)? {
                Value::Symbol(_) => {
                    if self.symbols_eq(sym, current)? {
                        return Ok(true);
                    }
                }
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    
                    // Add both car and cdr to queue if there's space
                    if queue_len + 2 > queue.len() {
                        // Queue full - fall back to recursive check for this subtree
                        if self.symbol_appears_in_recursive(sym, car)? || self.symbol_appears_in_recursive(sym, cdr)? {
                            return Ok(true);
                        }
                    } else {
                        queue[queue_len] = cdr;
                        queue_len += 1;
                        queue[queue_len] = car;
                        queue_len += 1;
                    }
                }
                _ => {}
            }
        }
        
        Ok(false)
    }
    
    /// Recursive fallback for symbol_appears_in when queue is full
    /// This should rarely be called for normal code.
    fn symbol_appears_in_recursive(&self, sym: ArenaIndex, expr: ArenaIndex) -> Result<bool, EvalError> {
        match self.lisp.get(expr)? {
            Value::Symbol(_) => self.symbols_eq(sym, expr),
            Value::Cons { .. } => {
                let car = self.lisp.car(expr)?;
                let cdr = self.lisp.cdr(expr)?;
                if self.symbol_appears_in_recursive(sym, car)? {
                    return Ok(true);
                }
                self.symbol_appears_in_recursive(sym, cdr)
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

                        if self.lisp.symbol_matches(head, "define-syntax")? {
                            return self.expand_define_syntax(args);
                        }

                        if self.lisp.symbol_matches(head, "let-syntax")? {
                            return self.expand_let_syntax(args, renames);
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

        // Collect all elements first (iteratively)
        let mut elements = [ArenaIndex::new(0); 128]; // Stack-allocated buffer
        let mut count = 0;
        let mut current = expr;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            if count >= elements.len() {
                return Err(self.make_error(ErrorKind::Generic, expr));
            }
            elements[count] = self.lisp.car(current)?;
            count += 1;
            current = self.lisp.cdr(current)?;
        }
        
        // Expand each element (this may recurse through expand_expr, but not through expand_application)
        for i in 0..count {
            elements[i] = self.expand_expr(elements[i], renames)?;
        }
        
        // Rebuild the list from the end (iteratively)
        let mut result = if self.lisp.get(current)?.is_nil() {
            self.lisp.nil()?
        } else {
            // Improper list - keep the tail as-is
            current
        };
        
        for i in (0..count).rev() {
            result = self.lisp.cons(elements[i], result)?;
        }
        
        Ok(result)
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
    /// 
    /// Supports two types of transformers:
    /// 1. SyntaxRules - pattern-based declarative macros
    /// 2. Lambda - procedural macros (transformer receives the full expression)
    pub(super) fn apply_macro(
        &mut self,
        transformer: ArenaIndex,
        expr: ArenaIndex,
    ) -> EvalResult {
        match self.lisp.get(transformer)? {
            Value::SyntaxRules { .. } => {
                self.apply_syntax_rules_macro(transformer, expr)
            }
            Value::Lambda { .. } => {
                self.apply_procedural_macro(transformer, expr)
            }
            _ => Err(self.make_error(ErrorKind::SyntaxError, transformer)
                .with_message("expected syntax-rules or lambda transformer"))
        }
    }
    
    /// Apply a syntax-rules based macro transformer
    fn apply_syntax_rules_macro(
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
        Err(self.make_error(ErrorKind::SyntaxError, expr)
            .with_message("no matching syntax-rules clause"))
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
        self.eval_for_macro(body, call_env)
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
    //
    // Procedural macro expansion now uses the unified evaluator via eval_for_macro().

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

    /// Parse a transformer expression (syntax-rules ...) or (lambda (x) ...)
    /// 
    /// Supports two forms of macro transformers:
    /// 1. `(syntax-rules ...)` - declarative pattern-based macros (R7RS)
    /// 2. `(lambda (x) ...)` - procedural macros using syntax-case
    pub(super) fn parse_transformer(&mut self, expr: ArenaIndex) -> EvalResult {
        let head = self.lisp.car(expr)?;

        // Check for syntax-rules (declarative transformer)
        if self.lisp.symbol_matches(head, "syntax-rules")? {
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
            return self.lisp.syntax_rules(literals, rules, self.global_env)
                .map_err(Into::into);
        }

        // Check for lambda (procedural transformer)
        if self.lisp.symbol_matches(head, "lambda")? {
            // Evaluate the lambda to create a closure
            // The lambda will be called with the syntax object as its argument
            let lambda_result = self.eval_lambda_for_transformer(expr)?;
            return Ok(lambda_result);
        }

        Err(self.make_error(ErrorKind::SyntaxError, expr)
            .with_message("expected (syntax-rules ...) or (lambda ...)"))
    }

    /// Evaluate a lambda expression to create a transformer closure
    /// 
    /// This creates a Lambda value that can be invoked as a procedural macro.
    fn eval_lambda_for_transformer(&self, lambda_expr: ArenaIndex) -> EvalResult {
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
        
        // Create Lambda value using the proper API (params, body, env)
        self.lisp.lambda(params, body, self.global_env).map_err(Into::into)
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

        // Collect all elements first (iteratively)
        let mut elements = [ArenaIndex::new(0); 128]; // Stack-allocated buffer
        let mut count = 0;
        let mut current = body;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            if count >= elements.len() {
                return Err(self.make_error(ErrorKind::Generic, body));
            }
            elements[count] = self.lisp.car(current)?;
            count += 1;
            current = self.lisp.cdr(current)?;
        }
        
        // Expand each element
        for i in 0..count {
            elements[i] = self.expand_expr(elements[i], renames)?;
        }
        
        // Rebuild the list from the end (iteratively)
        let mut result = self.lisp.nil()?;
        for i in (0..count).rev() {
            result = self.lisp.cons(elements[i], result)?;
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
        
        // Process list iteratively
        let mut elements = [ArenaIndex::new(0); 64];
        let mut count = 0;
        let mut current = stx;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            if count >= elements.len() {
                // Buffer full - fall back to recursive for remainder
                let car = self.lisp.car(current)?;
                let cdr = self.lisp.cdr(current)?;
                let new_car = self.syntax_to_datum_recursive(car)?;
                let new_cdr = self.syntax_to_datum_recursive(cdr)?;
                let tail = self.lisp.cons(new_car, new_cdr)?;
                
                // Build result from collected elements
                let mut result = tail;
                for i in (0..count).rev() {
                    result = self.lisp.cons(elements[i], result)?;
                }
                return Ok(result);
            }
            
            elements[count] = self.lisp.car(current)?;
            count += 1;
            current = self.lisp.cdr(current)?;
        }
        
        // Process all collected elements
        for i in 0..count {
            elements[i] = self.syntax_to_datum_recursive(elements[i])?;
        }
        
        // Handle tail
        let tail = if self.lisp.get(current)?.is_nil() {
            self.lisp.nil()?
        } else {
            self.syntax_to_datum_recursive(current)?
        };
        
        // Rebuild list
        let mut result = tail;
        for i in (0..count).rev() {
            result = self.lisp.cons(elements[i], result)?;
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
        // Get the context from template_id
        let (marks, subst) = match self.lisp.get(template_id)? {
            Value::Syntax { context, .. } => {
                let marks = self.lisp.car(context)?;
                let subst = self.lisp.cdr(context)?;
                (marks, subst)
            }
            // If template_id is just a symbol, use empty context
            Value::Symbol(_) => {
                let nil = self.lisp.nil()?;
                (nil, nil)
            }
            _ => {
                // Non-identifier - use empty context
                let nil = self.lisp.nil()?;
                (nil, nil)
            }
        };

        // Recursively wrap the datum
        self.datum_to_syntax_with_context(datum, marks, subst)
    }

    /// Helper to wrap a datum with a given context (iterative)
    /// 
    /// Uses iteration for list traversal to avoid stack overflow.
    fn datum_to_syntax_with_context(
        &mut self,
        datum: ArenaIndex,
        marks: ArenaIndex,
        subst: ArenaIndex,
    ) -> EvalResult {
        // Handle simple cases directly
        match self.lisp.get(datum)? {
            // Symbols become syntax objects
            Value::Symbol(_) => {
                return self.lisp.syntax(datum, marks, subst).map_err(Into::into);
            }
            Value::Cons { .. } => {
                // Process list iteratively
            }
            // Other atoms pass through unchanged (numbers, strings, etc.)
            _ => return Ok(datum),
        }
        
        // Process list iteratively
        let mut elements = [ArenaIndex::new(0); 64];
        let mut count = 0;
        let mut current = datum;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            if count >= elements.len() {
                // Buffer full - fall back to recursive for remainder
                let car = self.lisp.car(current)?;
                let cdr = self.lisp.cdr(current)?;
                let wrapped_car = self.datum_to_syntax_with_context(car, marks, subst)?;
                let wrapped_cdr = self.datum_to_syntax_with_context(cdr, marks, subst)?;
                let tail = self.lisp.cons(wrapped_car, wrapped_cdr)?;
                
                // Build result from collected elements
                let mut result = tail;
                for i in (0..count).rev() {
                    result = self.lisp.cons(elements[i], result)?;
                }
                return Ok(result);
            }
            
            elements[count] = self.lisp.car(current)?;
            count += 1;
            current = self.lisp.cdr(current)?;
        }
        
        // Process all collected elements
        for i in 0..count {
            elements[i] = self.datum_to_syntax_with_context(elements[i], marks, subst)?;
        }
        
        // Handle tail
        let tail = if self.lisp.get(current)?.is_nil() {
            self.lisp.nil()?
        } else {
            self.datum_to_syntax_with_context(current, marks, subst)?
        };
        
        // Rebuild list
        let mut result = tail;
        for i in (0..count).rev() {
            result = self.lisp.cons(elements[i], result)?;
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

