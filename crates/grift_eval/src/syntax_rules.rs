//! Pattern matching and template substitution for syntax-rules macros.
//!
//! This module implements R7RS syntax-rules (Section 4.3.2), which provides
//! pattern-based hygienic macros.
//!
//! ## Syntax
//!
//! ```scheme
//! (syntax-rules (<literal> ...)
//!   (<pattern> <template>)
//!   ...)
//! ```
//!
//! ## Patterns
//!
//! - `_` - Wildcard, matches anything, doesn't bind
//! - `<literal>` - Must match literally (from literals list)
//! - `<pattern-variable>` - Matches anything, binds for template use
//! - `(<pattern> ...)` - Matches a list
//! - `(<pattern> <ellipsis>)` - Matches zero or more
//! - `(<pattern> ... <pattern>)` - Fixed suffix after ellipsis
//!
//! ## Templates
//!
//! - `<pattern-variable>` - Replaced with bound value
//! - `(<template> ...)` - List construction
//! - `(<template> <ellipsis>)` - Replicate for each ellipsis match

use grift_parser::{ArenaIndex, Lisp, Value};
use crate::expand::{ExpandResult, is_identifier, add_scope_to_all};

// ============================================================================
// Pattern Variable Bindings
// ============================================================================

/// Maximum number of pattern variable bindings per macro invocation.
/// This limit exists for no_std compatibility (stack allocation).
pub const MAX_PATTERN_VARS: usize = 64;

/// Maximum ellipsis nesting depth (for nested `...` patterns).
pub const MAX_ELLIPSIS_DEPTH: usize = 8;

/// A binding from pattern matching.
/// 
/// Pattern variables can be bound to:
/// - A single syntax object (depth 0)
/// - A list of syntax objects (depth 1, from ellipsis)
/// - A list of lists (depth 2, from nested ellipsis)
/// - etc.
#[derive(Clone, Copy, Debug)]
pub struct PatternBinding {
    /// The pattern variable name (symbol)
    pub name: ArenaIndex,
    /// The bound value(s)
    pub value: ArenaIndex,
    /// Ellipsis depth (0 = single value, 1 = list, 2 = list of lists, etc.)
    pub depth: usize,
}

impl Default for PatternBinding {
    fn default() -> Self {
        PatternBinding {
            name: ArenaIndex::NIL,
            value: ArenaIndex::NIL,
            depth: 0,
        }
    }
}

/// Collection of pattern bindings from a successful match.
#[derive(Clone, Copy)]
pub struct PatternBindings {
    bindings: [PatternBinding; MAX_PATTERN_VARS],
    count: usize,
}

impl PatternBindings {
    /// Create an empty binding set.
    pub const fn new() -> Self {
        PatternBindings {
            bindings: [PatternBinding {
                name: ArenaIndex::NIL,
                value: ArenaIndex::NIL,
                depth: 0,
            }; MAX_PATTERN_VARS],
            count: 0,
        }
    }
    
    /// Add a binding.
    pub fn add(&mut self, name: ArenaIndex, value: ArenaIndex, depth: usize) -> bool {
        if self.count >= MAX_PATTERN_VARS {
            return false;
        }
        self.bindings[self.count] = PatternBinding { name, value, depth };
        self.count += 1;
        true
    }
    
    /// Look up a binding by name.
    pub fn lookup<const N: usize>(&self, lisp: &Lisp<N>, name: ArenaIndex) -> Option<&PatternBinding> {
        for i in 0..self.count {
            if lisp.symbol_eq(self.bindings[i].name, name).unwrap_or(false) {
                return Some(&self.bindings[i]);
            }
        }
        None
    }
    
    /// Get all bindings as a slice.
    pub fn as_slice(&self) -> &[PatternBinding] {
        &self.bindings[..self.count]
    }
    
    /// Number of bindings.
    pub fn len(&self) -> usize {
        self.count
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl Default for PatternBindings {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Literals Set
// ============================================================================

/// Maximum number of literals in a syntax-rules form.
pub const MAX_LITERALS: usize = 32;

/// Set of literal identifiers for a syntax-rules form.
#[derive(Clone, Copy)]
pub struct LiteralsSet {
    literals: [ArenaIndex; MAX_LITERALS],
    count: usize,
}

impl LiteralsSet {
    /// Create an empty literals set.
    pub const fn new() -> Self {
        LiteralsSet {
            literals: [ArenaIndex::NIL; MAX_LITERALS],
            count: 0,
        }
    }
    
    /// Add a literal.
    pub fn add(&mut self, lit: ArenaIndex) -> bool {
        if self.count >= MAX_LITERALS {
            return false;
        }
        self.literals[self.count] = lit;
        self.count += 1;
        true
    }
    
    /// Check if an identifier is a literal.
    pub fn contains<const N: usize>(&self, lisp: &Lisp<N>, id: ArenaIndex) -> ExpandResult<bool> {
        let sym = lisp.syntax_datum(id)?;
        for i in 0..self.count {
            if lisp.symbol_eq(sym, self.literals[i])? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    
    /// Parse literals list from syntax.
    pub fn from_syntax<const N: usize>(lisp: &Lisp<N>, stx: ArenaIndex) -> ExpandResult<Self> {
        let mut set = LiteralsSet::new();
        let datum = lisp.syntax_datum(stx)?;
        
        let mut current = datum;
        loop {
            match lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { car, cdr } => {
                    let sym = lisp.syntax_datum(car)?;
                    if matches!(lisp.get(sym)?, Value::Symbol(_)) {
                        set.add(sym);
                    }
                    current = cdr;
                }
                _ => break,
            }
        }
        
        Ok(set)
    }
}

impl Default for LiteralsSet {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Pattern Matching
// ============================================================================

/// Check if an identifier is the underscore wildcard.
fn is_underscore<const N: usize>(lisp: &Lisp<N>, id: ArenaIndex) -> ExpandResult<bool> {
    let sym = lisp.syntax_datum(id)?;
    lisp.symbol_matches(sym, "_")
}

/// Check if an identifier is the ellipsis.
fn is_ellipsis<const N: usize>(lisp: &Lisp<N>, id: ArenaIndex) -> ExpandResult<bool> {
    let sym = lisp.syntax_datum(id)?;
    lisp.symbol_matches(sym, "...")
}

/// Match a pattern against input syntax.
/// 
/// Returns Some(bindings) if the match succeeds, None if it fails.
/// 
/// # Arguments
/// 
/// * `lisp` - The Lisp context
/// * `pattern` - The pattern to match against
/// * `input` - The input syntax to match
/// * `literals` - Set of literal identifiers
/// * `ellipsis_depth` - Current ellipsis nesting depth
pub fn match_pattern<const N: usize>(
    lisp: &Lisp<N>,
    pattern: ArenaIndex,
    input: ArenaIndex,
    literals: &LiteralsSet,
    ellipsis_depth: usize,
) -> ExpandResult<Option<PatternBindings>> {
    let mut bindings = PatternBindings::new();
    
    if match_pattern_impl(lisp, pattern, input, literals, ellipsis_depth, &mut bindings)? {
        Ok(Some(bindings))
    } else {
        Ok(None)
    }
}

/// Internal pattern matching implementation.
fn match_pattern_impl<const N: usize>(
    lisp: &Lisp<N>,
    pattern: ArenaIndex,
    input: ArenaIndex,
    literals: &LiteralsSet,
    ellipsis_depth: usize,
    bindings: &mut PatternBindings,
) -> ExpandResult<bool> {
    let pattern_datum = lisp.syntax_datum(pattern)?;
    let input_datum = lisp.syntax_datum(input)?;
    
    match lisp.get(pattern_datum)? {
        // Pattern is a symbol - could be literal, wildcard, or pattern variable
        Value::Symbol(_) => {
            // Check for underscore wildcard
            if is_underscore(lisp, pattern)? {
                return Ok(true); // Matches anything, binds nothing
            }
            
            // Check for literal
            if literals.contains(lisp, pattern)? {
                // Must match literally
                if !is_identifier(lisp, input)? {
                    return Ok(false);
                }
                let input_sym = lisp.syntax_datum(input)?;
                return lisp.symbol_eq(pattern_datum, input_sym);
            }
            
            // Pattern variable - matches anything and binds
            bindings.add(pattern_datum, input, ellipsis_depth);
            Ok(true)
        }
        
        // Pattern is a list
        Value::Cons { .. } => {
            match_list_pattern(lisp, pattern, input, literals, ellipsis_depth, bindings)
        }
        
        // Pattern is nil (empty list)
        Value::Nil => {
            // Input must also be empty list
            Ok(matches!(lisp.get(input_datum)?, Value::Nil))
        }
        
        // Pattern is a constant (number, boolean, etc.)
        Value::Number(n) => {
            match lisp.get(input_datum)? {
                Value::Number(m) => Ok(n == m),
                _ => Ok(false),
            }
        }
        
        Value::True => Ok(matches!(lisp.get(input_datum)?, Value::True)),
        Value::False => Ok(matches!(lisp.get(input_datum)?, Value::False)),
        
        Value::Char(c) => {
            match lisp.get(input_datum)? {
                Value::Char(d) => Ok(c == d),
                _ => Ok(false),
            }
        }
        
        // Other patterns don't match
        _ => Ok(false),
    }
}

/// Match a list pattern against input.
fn match_list_pattern<const N: usize>(
    lisp: &Lisp<N>,
    pattern: ArenaIndex,
    input: ArenaIndex,
    literals: &LiteralsSet,
    ellipsis_depth: usize,
    bindings: &mut PatternBindings,
) -> ExpandResult<bool> {
    let pattern_datum = lisp.syntax_datum(pattern)?;
    let input_datum = lisp.syntax_datum(input)?;
    
    // Input must be a list
    if !matches!(lisp.get(input_datum)?, Value::Cons { .. }) {
        // Unless pattern is also empty and so is input
        if matches!(lisp.get(pattern_datum)?, Value::Nil) && matches!(lisp.get(input_datum)?, Value::Nil) {
            return Ok(true);
        }
        return Ok(false);
    }
    
    // Check for ellipsis in pattern
    // Pattern like (a b ... c) has ellipsis after b
    let (has_ellipsis, ellipsis_pos, pattern_len) = find_ellipsis_in_list(lisp, pattern_datum)?;
    
    if has_ellipsis {
        match_list_with_ellipsis(
            lisp, pattern_datum, input_datum, literals,
            ellipsis_depth, ellipsis_pos, pattern_len, bindings
        )
    } else {
        // No ellipsis - match element by element
        match_list_no_ellipsis(lisp, pattern_datum, input_datum, literals, ellipsis_depth, bindings)
    }
}

/// Find ellipsis position in a list pattern.
/// Returns (has_ellipsis, position, total_length).
fn find_ellipsis_in_list<const N: usize>(
    lisp: &Lisp<N>,
    pattern: ArenaIndex,
) -> ExpandResult<(bool, usize, usize)> {
    let mut pos = 0;
    let mut ellipsis_pos = 0;
    let mut has_ellipsis = false;
    let mut current = pattern;
    
    loop {
        match lisp.get(current)? {
            Value::Nil => break,
            Value::Cons { car, cdr } => {
                if is_ellipsis(lisp, car)? {
                    has_ellipsis = true;
                    ellipsis_pos = pos;
                }
                pos += 1;
                current = cdr;
            }
            _ => break,
        }
    }
    
    Ok((has_ellipsis, ellipsis_pos, pos))
}

/// Match a list pattern without ellipsis.
fn match_list_no_ellipsis<const N: usize>(
    lisp: &Lisp<N>,
    pattern: ArenaIndex,
    input: ArenaIndex,
    literals: &LiteralsSet,
    ellipsis_depth: usize,
    bindings: &mut PatternBindings,
) -> ExpandResult<bool> {
    let mut pat_cur = pattern;
    let mut inp_cur = input;
    
    loop {
        match (lisp.get(pat_cur)?, lisp.get(inp_cur)?) {
            (Value::Nil, Value::Nil) => return Ok(true),
            (Value::Nil, _) | (_, Value::Nil) => return Ok(false),
            (Value::Cons { car: pat_car, cdr: pat_cdr }, Value::Cons { car: inp_car, cdr: inp_cdr }) => {
                if !match_pattern_impl(lisp, pat_car, inp_car, literals, ellipsis_depth, bindings)? {
                    return Ok(false);
                }
                pat_cur = pat_cdr;
                inp_cur = inp_cdr;
            }
            // Improper list pattern (a . b)
            (Value::Cons { car: pat_car, cdr: pat_cdr }, _) => {
                // Match car
                if !match_pattern_impl(lisp, pat_car, inp_cur, literals, ellipsis_depth, bindings)? {
                    return Ok(false);
                }
                // pat_cdr should match remaining (which might be non-list)
                return match_pattern_impl(lisp, pat_cdr, inp_cur, literals, ellipsis_depth, bindings);
            }
            _ => return Ok(false),
        }
    }
}

/// Match a list pattern with ellipsis.
/// 
/// Pattern: (pre... <pattern> ... post...)
/// The pattern before `...` matches zero or more times.
fn match_list_with_ellipsis<const N: usize>(
    lisp: &Lisp<N>,
    pattern: ArenaIndex,
    input: ArenaIndex,
    literals: &LiteralsSet,
    ellipsis_depth: usize,
    ellipsis_pos: usize,
    pattern_len: usize,
    bindings: &mut PatternBindings,
) -> ExpandResult<bool> {
    // Pattern structure: (p1 p2 ... pN ... q1 q2 ... qM)
    // ellipsis_pos points to the `...` itself
    // The pattern before `...` (at ellipsis_pos - 1) is the repeated pattern
    // Everything after `...` is the suffix
    
    if ellipsis_pos == 0 {
        // Malformed: ellipsis at start
        return Ok(false);
    }
    
    let pre_count = ellipsis_pos - 1; // Patterns before the repeated one
    let post_count = pattern_len - ellipsis_pos - 1; // Patterns after ellipsis
    
    // Get input length
    let input_len = lisp.list_len(input)?;
    
    // Minimum input length: pre_count + post_count (zero ellipsis matches)
    if input_len < pre_count + post_count {
        return Ok(false);
    }
    
    // Number of ellipsis matches
    let ellipsis_matches = input_len - pre_count - post_count;
    
    // Collect pattern elements
    let mut patterns = [ArenaIndex::NIL; 64];
    let mut pat_count = 0;
    let mut current = pattern;
    while let Value::Cons { car, cdr } = lisp.get(current)? {
        if pat_count < 64 {
            patterns[pat_count] = car;
            pat_count += 1;
        }
        current = cdr;
    }
    
    // Collect input elements
    let mut inputs = [ArenaIndex::NIL; 128];
    let mut inp_count = 0;
    let mut current = input;
    while let Value::Cons { car, cdr } = lisp.get(current)? {
        if inp_count < 128 {
            inputs[inp_count] = car;
            inp_count += 1;
        }
        current = cdr;
    }
    
    // Match pre-ellipsis patterns
    for i in 0..pre_count {
        if !match_pattern_impl(lisp, patterns[i], inputs[i], literals, ellipsis_depth, bindings)? {
            return Ok(false);
        }
    }
    
    // The repeated pattern (just before ellipsis)
    let repeated_pattern = patterns[ellipsis_pos - 1];
    
    // Collect pattern variables from repeated pattern for ellipsis binding
    let mut ellipsis_vars = [ArenaIndex::NIL; MAX_PATTERN_VARS];
    let mut ellipsis_var_count = 0;
    collect_pattern_variables(lisp, repeated_pattern, literals, &mut ellipsis_vars, &mut ellipsis_var_count)?;
    
    // Initialize ellipsis match collectors (one list per variable)
    let mut ellipsis_matches_arr: [[ArenaIndex; 128]; MAX_PATTERN_VARS] = [[ArenaIndex::NIL; 128]; MAX_PATTERN_VARS];
    
    // Match ellipsis portion
    for i in 0..ellipsis_matches {
        let input_idx = pre_count + i;
        let mut temp_bindings = PatternBindings::new();
        
        if !match_pattern_impl(lisp, repeated_pattern, inputs[input_idx], literals, ellipsis_depth + 1, &mut temp_bindings)? {
            return Ok(false);
        }
        
        // Collect the bindings for this iteration
        for j in 0..ellipsis_var_count {
            if let Some(binding) = temp_bindings.lookup(lisp, ellipsis_vars[j]) {
                ellipsis_matches_arr[j][i] = binding.value;
            }
        }
    }
    
    // Create list bindings for ellipsis variables
    for j in 0..ellipsis_var_count {
        // Build list from collected matches
        let mut list = lisp.nil()?;
        for i in (0..ellipsis_matches).rev() {
            list = lisp.cons(ellipsis_matches_arr[j][i], list)?;
        }
        bindings.add(ellipsis_vars[j], list, ellipsis_depth + 1);
    }
    
    // Match post-ellipsis patterns
    for i in 0..post_count {
        let pattern_idx = ellipsis_pos + 1 + i;
        let input_idx = pre_count + ellipsis_matches + i;
        if !match_pattern_impl(lisp, patterns[pattern_idx], inputs[input_idx], literals, ellipsis_depth, bindings)? {
            return Ok(false);
        }
    }
    
    Ok(true)
}

/// Collect pattern variable names from a pattern.
fn collect_pattern_variables<const N: usize>(
    lisp: &Lisp<N>,
    pattern: ArenaIndex,
    literals: &LiteralsSet,
    vars: &mut [ArenaIndex; MAX_PATTERN_VARS],
    count: &mut usize,
) -> ExpandResult<()> {
    let pattern_datum = lisp.syntax_datum(pattern)?;
    
    match lisp.get(pattern_datum)? {
        Value::Symbol(_) => {
            // Skip underscore and literals
            if !is_underscore(lisp, pattern)? && !literals.contains(lisp, pattern)? {
                if *count < MAX_PATTERN_VARS {
                    vars[*count] = pattern_datum;
                    *count += 1;
                }
            }
        }
        Value::Cons { car, cdr } => {
            // Skip ellipsis
            if !is_ellipsis(lisp, car)? {
                collect_pattern_variables(lisp, car, literals, vars, count)?;
            }
            collect_pattern_variables(lisp, cdr, literals, vars, count)?;
        }
        _ => {}
    }
    
    Ok(())
}

// ============================================================================
// Template Substitution
// ============================================================================

/// Substitute pattern variable bindings into a template.
/// 
/// # Arguments
/// 
/// * `lisp` - The Lisp context
/// * `template` - The template syntax
/// * `bindings` - Pattern variable bindings from matching
/// * `intro_scope` - Scope to add to introduced identifiers
pub fn substitute_template<const N: usize>(
    lisp: &Lisp<N>,
    template: ArenaIndex,
    bindings: &PatternBindings,
    intro_scope: isize,
) -> ExpandResult<ArenaIndex> {
    substitute_template_impl(lisp, template, bindings, intro_scope, 0)
}

/// Internal template substitution.
fn substitute_template_impl<const N: usize>(
    lisp: &Lisp<N>,
    template: ArenaIndex,
    bindings: &PatternBindings,
    intro_scope: isize,
    ellipsis_depth: usize,
) -> ExpandResult<ArenaIndex> {
    let template_datum = lisp.syntax_datum(template)?;
    
    match lisp.get(template_datum)? {
        // Template is a symbol - could be pattern variable or introduced identifier
        Value::Symbol(_) => {
            // Check if it's a bound pattern variable
            if let Some(binding) = bindings.lookup(lisp, template_datum) {
                // If we're at the right depth, return the value directly
                // Otherwise, we need to index into the ellipsis list
                if binding.depth == ellipsis_depth {
                    return Ok(binding.value);
                } else if binding.depth > ellipsis_depth {
                    // We're expanding an ellipsis; the value should be a list
                    // Return the value as-is; the ellipsis handler will iterate
                    return Ok(binding.value);
                }
            }
            
            // Not a pattern variable - it's an introduced identifier
            // Add the introduction scope for hygiene.
            // With scope-aware lookup (Phase 5.4), this identifier will still
            // resolve to the correct binding because scopes(ref) ⊆ scopes(bind)
            // when the binding has no scopes (global bindings like 'if', 'let').
            add_scope_to_all(lisp, template, intro_scope)
        }
        
        // Template is a list
        Value::Cons { .. } => {
            substitute_list_template(lisp, template, bindings, intro_scope, ellipsis_depth)
        }
        
        // Other values pass through (with scope for identifiers)
        _ => Ok(template),
    }
}

/// Substitute a list template.
fn substitute_list_template<const N: usize>(
    lisp: &Lisp<N>,
    template: ArenaIndex,
    bindings: &PatternBindings,
    intro_scope: isize,
    ellipsis_depth: usize,
) -> ExpandResult<ArenaIndex> {
    let template_datum = lisp.syntax_datum(template)?;
    
    // Check for ellipsis in template
    let (has_ellipsis, ellipsis_pos, _) = find_ellipsis_in_list(lisp, template_datum)?;
    
    if has_ellipsis && ellipsis_pos > 0 {
        substitute_list_with_ellipsis(lisp, template_datum, bindings, intro_scope, ellipsis_depth, ellipsis_pos)
    } else {
        // No ellipsis - substitute element by element
        substitute_list_no_ellipsis(lisp, template_datum, bindings, intro_scope, ellipsis_depth)
    }
}

/// Substitute a list template without ellipsis.
fn substitute_list_no_ellipsis<const N: usize>(
    lisp: &Lisp<N>,
    template: ArenaIndex,
    bindings: &PatternBindings,
    intro_scope: isize,
    ellipsis_depth: usize,
) -> ExpandResult<ArenaIndex> {
    match lisp.get(template)? {
        Value::Nil => lisp.nil(),
        Value::Cons { car, cdr } => {
            let new_car = substitute_template_impl(lisp, car, bindings, intro_scope, ellipsis_depth)?;
            let new_cdr = substitute_list_no_ellipsis(lisp, cdr, bindings, intro_scope, ellipsis_depth)?;
            lisp.cons(new_car, new_cdr)
        }
        _ => Ok(template),
    }
}

/// Substitute a list template with ellipsis.
fn substitute_list_with_ellipsis<const N: usize>(
    lisp: &Lisp<N>,
    template: ArenaIndex,
    bindings: &PatternBindings,
    intro_scope: isize,
    ellipsis_depth: usize,
    ellipsis_pos: usize,
) -> ExpandResult<ArenaIndex> {
    // Collect template elements
    let mut templates = [ArenaIndex::NIL; 64];
    let mut count = 0;
    let mut current = template;
    while let Value::Cons { car, cdr } = lisp.get(current)? {
        if count < 64 {
            templates[count] = car;
            count += 1;
        }
        current = cdr;
    }
    
    // The repeated template is just before the ellipsis
    let repeated_template = templates[ellipsis_pos - 1];
    
    // Find pattern variables in the repeated template to determine repetition count
    let mut vars = [ArenaIndex::NIL; MAX_PATTERN_VARS];
    let mut var_count = 0;
    collect_template_variables(lisp, repeated_template, bindings, &mut vars, &mut var_count)?;
    
    // Get repetition count from the first ellipsis-depth variable
    let rep_count = if var_count > 0 {
        if let Some(binding) = bindings.lookup(lisp, vars[0]) {
            if binding.depth > ellipsis_depth {
                lisp.list_len(binding.value)?
            } else {
                1
            }
        } else {
            0
        }
    } else {
        0
    };
    
    // Build result list
    let mut result_elements = [ArenaIndex::NIL; 128];
    let mut result_count = 0;
    
    // Add pre-ellipsis elements
    for i in 0..(ellipsis_pos - 1) {
        let elem = substitute_template_impl(lisp, templates[i], bindings, intro_scope, ellipsis_depth)?;
        if result_count < 128 {
            result_elements[result_count] = elem;
            result_count += 1;
        }
    }
    
    // Add repeated elements
    for rep_idx in 0..rep_count {
        // Create bindings for this iteration
        let iter_bindings = create_iteration_bindings(lisp, bindings, ellipsis_depth, rep_idx)?;
        let elem = substitute_template_impl(lisp, repeated_template, &iter_bindings, intro_scope, ellipsis_depth + 1)?;
        if result_count < 128 {
            result_elements[result_count] = elem;
            result_count += 1;
        }
    }
    
    // Add post-ellipsis elements
    for i in (ellipsis_pos + 1)..count {
        let elem = substitute_template_impl(lisp, templates[i], bindings, intro_scope, ellipsis_depth)?;
        if result_count < 128 {
            result_elements[result_count] = elem;
            result_count += 1;
        }
    }
    
    // Build the result list
    let mut result = lisp.nil()?;
    for i in (0..result_count).rev() {
        result = lisp.cons(result_elements[i], result)?;
    }
    
    Ok(result)
}

/// Collect pattern variable names from a template.
fn collect_template_variables<const N: usize>(
    lisp: &Lisp<N>,
    template: ArenaIndex,
    bindings: &PatternBindings,
    vars: &mut [ArenaIndex; MAX_PATTERN_VARS],
    count: &mut usize,
) -> ExpandResult<()> {
    let template_datum = lisp.syntax_datum(template)?;
    
    match lisp.get(template_datum)? {
        Value::Symbol(_) => {
            if bindings.lookup(lisp, template_datum).is_some() {
                if *count < MAX_PATTERN_VARS {
                    vars[*count] = template_datum;
                    *count += 1;
                }
            }
        }
        Value::Cons { car, cdr } => {
            if !is_ellipsis(lisp, car)? {
                collect_template_variables(lisp, car, bindings, vars, count)?;
            }
            collect_template_variables(lisp, cdr, bindings, vars, count)?;
        }
        _ => {}
    }
    
    Ok(())
}

/// Create bindings for one iteration of ellipsis expansion.
fn create_iteration_bindings<const N: usize>(
    lisp: &Lisp<N>,
    bindings: &PatternBindings,
    ellipsis_depth: usize,
    iteration: usize,
) -> ExpandResult<PatternBindings> {
    let mut new_bindings = PatternBindings::new();
    
    for binding in bindings.as_slice() {
        if binding.depth > ellipsis_depth {
            // This is an ellipsis variable - extract the iteration's value
            let mut current = binding.value;
            for _ in 0..iteration {
                if let Value::Cons { cdr, .. } = lisp.get(current)? {
                    current = cdr;
                } else {
                    break;
                }
            }
            if let Value::Cons { car, .. } = lisp.get(current)? {
                new_bindings.add(binding.name, car, binding.depth - 1);
            }
        } else {
            // Copy non-ellipsis binding as-is
            new_bindings.add(binding.name, binding.value, binding.depth);
        }
    }
    
    Ok(new_bindings)
}

// ============================================================================
// Syntax-Rules Transformer
// ============================================================================

/// A compiled syntax-rules transformer.
/// 
/// This represents a parsed syntax-rules form ready for use as a macro.
#[derive(Clone, Copy)]
pub struct SyntaxRulesTransformer {
    /// The literals set
    pub literals: LiteralsSet,
    /// Pattern-template pairs (as a list in the arena)
    pub rules: ArenaIndex,
    /// Number of rules
    pub rule_count: usize,
}

impl SyntaxRulesTransformer {
    /// Create a new transformer from a syntax-rules form.
    /// 
    /// # Arguments
    /// 
    /// * `lisp` - The Lisp context
    /// * `form` - The syntax-rules form: `(syntax-rules (literals...) (pattern template)...)`
    pub fn new<const N: usize>(
        lisp: &Lisp<N>,
        form: ArenaIndex,
    ) -> ExpandResult<Self> {
        // form should be: (syntax-rules (literals...) rules...)
        let form_datum = lisp.syntax_datum(form)?;
        
        // Skip 'syntax-rules' keyword
        let rest = lisp.cdr(form_datum)?;
        
        // Get literals list
        let literals_syntax = lisp.car(rest)?;
        let literals = LiteralsSet::from_syntax(lisp, literals_syntax)?;
        
        // Get rules
        let rules = lisp.cdr(rest)?;
        let rule_count = lisp.list_len(rules)?;
        
        Ok(SyntaxRulesTransformer {
            literals,
            rules,
            rule_count,
        })
    }
    
    /// Apply the transformer to input syntax.
    /// 
    /// Tries each rule in order until one matches, then substitutes the template.
    /// Returns None if no rule matches.
    pub fn apply<const N: usize>(
        &self,
        lisp: &Lisp<N>,
        input: ArenaIndex,
        intro_scope: isize,
    ) -> ExpandResult<Option<ArenaIndex>> {
        let mut current = self.rules;
        
        loop {
            match lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { car: rule, cdr } => {
                    // Each rule is (pattern template)
                    let rule_datum = lisp.syntax_datum(rule)?;
                    let pattern = lisp.car(rule_datum)?;
                    let template = lisp.car(lisp.cdr(rule_datum)?)?;
                    
                    // Try to match
                    if let Some(bindings) = match_pattern(lisp, pattern, input, &self.literals, 0)? {
                        // Match succeeded - substitute template
                        let result = substitute_template(lisp, template, &bindings, intro_scope)?;
                        return Ok(Some(result));
                    }
                    
                    current = cdr;
                }
                _ => break,
            }
        }
        
        Ok(None) // No rule matched
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expand::datum_to_syntax;
    
    #[test]
    fn test_pattern_bindings() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut bindings = PatternBindings::new();
        
        let foo = lisp.symbol("foo").unwrap();
        let bar = lisp.symbol("bar").unwrap();
        let val1 = lisp.number(42).unwrap();
        let val2 = lisp.number(99).unwrap();
        
        bindings.add(foo, val1, 0);
        bindings.add(bar, val2, 0);
        
        assert_eq!(bindings.len(), 2);
        assert!(bindings.lookup(&lisp, foo).is_some());
        assert!(bindings.lookup(&lisp, bar).is_some());
    }
    
    #[test]
    fn test_match_pattern_variable() {
        let lisp: Lisp<1000> = Lisp::new();
        let literals = LiteralsSet::new();
        
        // Pattern: x (a pattern variable)
        let x = lisp.symbol("x").unwrap();
        let pattern = datum_to_syntax(&lisp, x, lisp.nil().unwrap()).unwrap();
        
        // Input: 42
        let input = lisp.number(42).unwrap();
        
        let result = match_pattern(&lisp, pattern, input, &literals, 0).unwrap();
        assert!(result.is_some());
        
        let bindings = result.unwrap();
        let binding = bindings.lookup(&lisp, x).unwrap();
        assert_eq!(lisp.get(binding.value).unwrap().as_number(), Some(42));
    }
    
    #[test]
    fn test_match_wildcard() {
        let lisp: Lisp<1000> = Lisp::new();
        let literals = LiteralsSet::new();
        
        // Pattern: _
        let underscore = lisp.symbol("_").unwrap();
        let pattern = datum_to_syntax(&lisp, underscore, lisp.nil().unwrap()).unwrap();
        
        // Input: 42
        let input = lisp.number(42).unwrap();
        
        let result = match_pattern(&lisp, pattern, input, &literals, 0).unwrap();
        assert!(result.is_some());
        
        // Wildcard doesn't bind
        let bindings = result.unwrap();
        assert!(bindings.is_empty());
    }
    
    #[test]
    fn test_match_literal() {
        let lisp: Lisp<1000> = Lisp::new();
        
        // Literals: (if)
        let mut literals = LiteralsSet::new();
        let if_sym = lisp.symbol("if").unwrap();
        literals.add(if_sym);
        
        // Pattern: if (literal)
        let pattern = datum_to_syntax(&lisp, if_sym, lisp.nil().unwrap()).unwrap();
        
        // Input: if (should match)
        let input = datum_to_syntax(&lisp, if_sym, lisp.nil().unwrap()).unwrap();
        let result = match_pattern(&lisp, pattern, input, &literals, 0).unwrap();
        assert!(result.is_some());
        
        // Input: else (should not match)
        let else_sym = lisp.symbol("else").unwrap();
        let input2 = datum_to_syntax(&lisp, else_sym, lisp.nil().unwrap()).unwrap();
        let result2 = match_pattern(&lisp, pattern, input2, &literals, 0).unwrap();
        assert!(result2.is_none());
    }
    
    #[test]
    fn test_match_simple_list() {
        let lisp: Lisp<1000> = Lisp::new();
        let literals = LiteralsSet::new();
        let nil = lisp.nil().unwrap();
        
        // Pattern: (a b)
        let a = lisp.symbol("a").unwrap();
        let b = lisp.symbol("b").unwrap();
        let pattern_list = lisp.list([a, b]).unwrap();
        let pattern = datum_to_syntax(&lisp, pattern_list, nil).unwrap();
        
        // Input: (1 2)
        let one = lisp.number(1).unwrap();
        let two = lisp.number(2).unwrap();
        let input = lisp.list([one, two]).unwrap();
        
        let result = match_pattern(&lisp, pattern, input, &literals, 0).unwrap();
        assert!(result.is_some());
        
        let bindings = result.unwrap();
        let a_binding = bindings.lookup(&lisp, a).unwrap();
        let b_binding = bindings.lookup(&lisp, b).unwrap();
        assert_eq!(lisp.get(a_binding.value).unwrap().as_number(), Some(1));
        assert_eq!(lisp.get(b_binding.value).unwrap().as_number(), Some(2));
    }
    
    #[test]
    fn test_substitute_simple() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        // Bindings: x -> 42
        let mut bindings = PatternBindings::new();
        let x = lisp.symbol("x").unwrap();
        let val = lisp.number(42).unwrap();
        bindings.add(x, val, 0);
        
        // Template: x
        let template = datum_to_syntax(&lisp, x, nil).unwrap();
        
        let result = substitute_template(&lisp, template, &bindings, 999).unwrap();
        assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
    }
    
    #[test]
    fn test_substitute_list() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        // Bindings: a -> 1, b -> 2
        let mut bindings = PatternBindings::new();
        let a = lisp.symbol("a").unwrap();
        let b = lisp.symbol("b").unwrap();
        bindings.add(a, lisp.number(1).unwrap(), 0);
        bindings.add(b, lisp.number(2).unwrap(), 0);
        
        // Template: (b a) - reversed
        let template_list = lisp.list([b, a]).unwrap();
        let template = datum_to_syntax(&lisp, template_list, nil).unwrap();
        
        let result = substitute_template(&lisp, template, &bindings, 999).unwrap();
        
        // Result should be (2 1)
        let car = lisp.car(result).unwrap();
        let cadr = lisp.car(lisp.cdr(result).unwrap()).unwrap();
        assert_eq!(lisp.get(car).unwrap().as_number(), Some(2));
        assert_eq!(lisp.get(cadr).unwrap().as_number(), Some(1));
    }
    
    #[test]
    fn test_match_list_with_ellipsis() {
        let lisp: Lisp<1000> = Lisp::new();
        let literals = LiteralsSet::new();
        let nil = lisp.nil().unwrap();
        
        // Pattern: (a x ...)
        let a = lisp.symbol("a").unwrap();
        let x = lisp.symbol("x").unwrap();
        let ellipsis = lisp.symbol("...").unwrap();
        let pattern_list = lisp.list([a, x, ellipsis]).unwrap();
        let pattern = datum_to_syntax(&lisp, pattern_list, nil).unwrap();
        
        // Input: (test 1 2 3)
        let test = lisp.symbol("test").unwrap();
        let one = lisp.number(1).unwrap();
        let two = lisp.number(2).unwrap();
        let three = lisp.number(3).unwrap();
        let input = lisp.list([test, one, two, three]).unwrap();
        
        let result = match_pattern(&lisp, pattern, input, &literals, 0).unwrap();
        assert!(result.is_some());
        
        let bindings = result.unwrap();
        
        // 'a' should be bound to 'test'
        let a_binding = bindings.lookup(&lisp, a).unwrap();
        assert!(lisp.symbol_matches(a_binding.value, "test").unwrap());
        
        // 'x' should be bound to a list (1 2 3)
        let x_binding = bindings.lookup(&lisp, x).unwrap();
        assert_eq!(x_binding.depth, 1); // Ellipsis depth
        let x_list_len = lisp.list_len(x_binding.value).unwrap();
        assert_eq!(x_list_len, 3);
    }
    
    #[test]
    fn test_match_ellipsis_empty() {
        let lisp: Lisp<1000> = Lisp::new();
        let literals = LiteralsSet::new();
        let nil = lisp.nil().unwrap();
        
        // Pattern: (a x ...)
        let a = lisp.symbol("a").unwrap();
        let x = lisp.symbol("x").unwrap();
        let ellipsis = lisp.symbol("...").unwrap();
        let pattern_list = lisp.list([a, x, ellipsis]).unwrap();
        let pattern = datum_to_syntax(&lisp, pattern_list, nil).unwrap();
        
        // Input: (test) - no ellipsis elements
        let test = lisp.symbol("test").unwrap();
        let input = lisp.list([test]).unwrap();
        
        let result = match_pattern(&lisp, pattern, input, &literals, 0).unwrap();
        assert!(result.is_some());
        
        let bindings = result.unwrap();
        
        // 'x' should be bound to empty list
        let x_binding = bindings.lookup(&lisp, x).unwrap();
        assert!(matches!(lisp.get(x_binding.value).unwrap(), Value::Nil));
    }
    
    #[test]
    fn test_substitute_with_ellipsis() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        // Bindings: x -> (1 2 3) at depth 1
        let mut bindings = PatternBindings::new();
        let x = lisp.symbol("x").unwrap();
        let one = lisp.number(1).unwrap();
        let two = lisp.number(2).unwrap();
        let three = lisp.number(3).unwrap();
        let values = lisp.list([one, two, three]).unwrap();
        bindings.add(x, values, 1);
        
        // Template: (list x ...)
        let list_sym = lisp.symbol("list").unwrap();
        let ellipsis = lisp.symbol("...").unwrap();
        let template_list = lisp.list([list_sym, x, ellipsis]).unwrap();
        let template = datum_to_syntax(&lisp, template_list, nil).unwrap();
        
        let result = substitute_template(&lisp, template, &bindings, 999).unwrap();
        
        // Result should be (list 1 2 3)
        let result_len = lisp.list_len(result).unwrap();
        assert_eq!(result_len, 4); // list + 3 elements
    }
    
    #[test]
    fn test_nested_list_pattern() {
        let lisp: Lisp<1000> = Lisp::new();
        let literals = LiteralsSet::new();
        let nil = lisp.nil().unwrap();
        
        // Pattern: ((a b) c)
        let a = lisp.symbol("a").unwrap();
        let b = lisp.symbol("b").unwrap();
        let c = lisp.symbol("c").unwrap();
        let inner = lisp.list([a, b]).unwrap();
        let pattern_list = lisp.list([inner, c]).unwrap();
        let pattern = datum_to_syntax(&lisp, pattern_list, nil).unwrap();
        
        // Input: ((1 2) 3)
        let one = lisp.number(1).unwrap();
        let two = lisp.number(2).unwrap();
        let three = lisp.number(3).unwrap();
        let inner_input = lisp.list([one, two]).unwrap();
        let input = lisp.list([inner_input, three]).unwrap();
        
        let result = match_pattern(&lisp, pattern, input, &literals, 0).unwrap();
        assert!(result.is_some());
        
        let bindings = result.unwrap();
        assert_eq!(lisp.get(bindings.lookup(&lisp, a).unwrap().value).unwrap().as_number(), Some(1));
        assert_eq!(lisp.get(bindings.lookup(&lisp, b).unwrap().value).unwrap().as_number(), Some(2));
        assert_eq!(lisp.get(bindings.lookup(&lisp, c).unwrap().value).unwrap().as_number(), Some(3));
    }
    
    #[test]
    fn test_is_ellipsis() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let ellipsis = lisp.symbol("...").unwrap();
        assert!(is_ellipsis(&lisp, ellipsis).unwrap());
        
        let not_ellipsis = lisp.symbol("foo").unwrap();
        assert!(!is_ellipsis(&lisp, not_ellipsis).unwrap());
    }
    
    #[test]
    fn test_match_constant() {
        let lisp: Lisp<1000> = Lisp::new();
        let literals = LiteralsSet::new();
        let nil = lisp.nil().unwrap();
        
        // Pattern: (42 x)
        let forty_two = lisp.number(42).unwrap();
        let x = lisp.symbol("x").unwrap();
        let pattern_list = lisp.list([forty_two, x]).unwrap();
        let pattern = datum_to_syntax(&lisp, pattern_list, nil).unwrap();
        
        // Input: (42 hello) - should match
        let hello = lisp.symbol("hello").unwrap();
        let input = lisp.list([forty_two, hello]).unwrap();
        let result = match_pattern(&lisp, pattern, input, &literals, 0).unwrap();
        assert!(result.is_some());
        
        // Input: (99 hello) - should not match
        let ninety_nine = lisp.number(99).unwrap();
        let input2 = lisp.list([ninety_nine, hello]).unwrap();
        let result2 = match_pattern(&lisp, pattern, input2, &literals, 0).unwrap();
        assert!(result2.is_none());
    }
}
