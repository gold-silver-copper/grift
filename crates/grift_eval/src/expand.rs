//! Macro expansion module for hygienic macros.
//!
//! This module implements the expansion phase that runs before evaluation.
//! It uses the "set of scopes" model for macro hygiene, where each identifier
//! carries a set of scopes that track its lexical context.
//!
//! ## Expansion Pipeline
//!
//! ```text
//! parse(source) -> raw AST -> expand(ast, env) -> expanded AST -> eval(ast, env)
//! ```
//!
//! ## Key Concepts
//!
//! - **Syntax Object**: A datum wrapped with scope information (`Value::Syntax`)
//! - **Scope**: A unique identifier for a binding context
//! - **Scope Set**: The collection of scopes an identifier has passed through
//! - **Transformer**: A macro that transforms syntax objects
//!
//! ## Hygiene
//!
//! The set-of-scopes model ensures hygiene by:
//! 1. Adding a fresh scope to all syntax introduced by a macro
//! 2. Comparing scope sets during identifier resolution
//! 3. Using scope flipping for definition contexts

use grift_parser::{ArenaIndex, Lisp, Value, ArenaError};

/// Result type for expansion operations
pub type ExpandResult<T> = Result<T, ArenaError>;

// ============================================================================
// Scope Counter
// ============================================================================

/// A scope counter that generates unique scope IDs.
/// 
/// Each binding context (let, lambda, macro expansion, etc.) should
/// request a fresh scope ID to ensure hygiene.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScopeCounter {
    next_id: isize,
}

impl ScopeCounter {
    /// Create a new scope counter starting at 0.
    pub const fn new() -> Self {
        ScopeCounter { next_id: 0 }
    }
    
    /// Generate the next unique scope ID.
    /// 
    /// Each call returns a fresh ID that has never been used before.
    #[inline]
    pub fn fresh(&mut self) -> isize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    
    /// Get the current counter value (for debugging).
    #[inline]
    pub const fn current(&self) -> isize {
        self.next_id
    }
}

// ============================================================================
// Datum <-> Syntax Conversion
// ============================================================================

/// Convert a raw datum to a syntax object with the given scopes.
/// 
/// This recursively wraps compound structures (lists) so that each
/// identifier in the datum carries the scope information.
/// 
/// - Symbols are wrapped directly in syntax objects
/// - Lists are recursively processed (each element converted)
/// - Self-evaluating values (numbers, booleans, etc.) pass through unchanged
/// 
/// # Arguments
/// 
/// * `lisp` - The Lisp context
/// * `datum` - The raw datum to convert
/// * `scopes` - The scope set to attach
/// 
/// # Example
/// 
/// ```text
/// datum_to_syntax('(if x y), scopes) =>
///   (syntax:if (syntax:x scopes) (syntax:y scopes))
/// ```
pub fn datum_to_syntax<const N: usize>(
    lisp: &Lisp<N>,
    datum: ArenaIndex,
    scopes: ArenaIndex,
) -> ExpandResult<ArenaIndex> {
    match lisp.get(datum)? {
        // Symbols get wrapped in syntax objects
        Value::Symbol(_) => {
            lisp.syntax(datum, scopes)
        }
        
        // Lists are processed recursively
        Value::Cons { car, cdr } => {
            let new_car = datum_to_syntax(lisp, car, scopes)?;
            let new_cdr = datum_to_syntax(lisp, cdr, scopes)?;
            lisp.cons(new_car, new_cdr)
        }
        
        // Syntax objects - merge scopes
        Value::Syntax { datum: inner_datum, scopes: inner_scopes } => {
            // Merge the scope sets (prepend new scopes to existing)
            let merged_scopes = merge_scopes(lisp, scopes, inner_scopes)?;
            lisp.syntax(inner_datum, merged_scopes)
        }
        
        // Self-evaluating values pass through unchanged
        // (numbers, booleans, characters, strings, vectors, nil)
        Value::Nil | Value::True | Value::False | Value::Number(_) |
        Value::Char(_) | Value::String { .. } | Value::Array { .. } => {
            Ok(datum)
        }
        
        // Procedures and internal values pass through
        Value::Lambda { .. } | Value::Builtin(_) | Value::StdLib(_) |
        Value::Native { .. } | Value::Ref(_) | Value::Usize(_) |
        Value::Transformer { .. } => {
            Ok(datum)
        }
    }
}

/// Convert a syntax object back to a raw datum, stripping all scope information.
/// 
/// This recursively unwraps compound structures.
/// 
/// # Arguments
/// 
/// * `lisp` - The Lisp context
/// * `stx` - The syntax object to convert
/// 
/// # Example
/// 
/// ```text
/// syntax_to_datum((syntax:if (syntax:x scopes) (syntax:y scopes))) =>
///   '(if x y)
/// ```
pub fn syntax_to_datum<const N: usize>(
    lisp: &Lisp<N>,
    stx: ArenaIndex,
) -> ExpandResult<ArenaIndex> {
    match lisp.get(stx)? {
        // Unwrap syntax objects
        Value::Syntax { datum, .. } => {
            syntax_to_datum(lisp, datum)
        }
        
        // Recursively process lists
        Value::Cons { car, cdr } => {
            let new_car = syntax_to_datum(lisp, car)?;
            let new_cdr = syntax_to_datum(lisp, cdr)?;
            lisp.cons(new_car, new_cdr)
        }
        
        // Everything else passes through unchanged
        _ => Ok(stx)
    }
}

/// Merge two scope sets by prepending the first to the second.
/// 
/// This creates a new scope list containing all scopes from both sets.
fn merge_scopes<const N: usize>(
    lisp: &Lisp<N>,
    scopes_a: ArenaIndex,
    scopes_b: ArenaIndex,
) -> ExpandResult<ArenaIndex> {
    match lisp.get(scopes_a)? {
        Value::Nil => Ok(scopes_b),
        Value::Cons { car, cdr } => {
            let rest = merge_scopes(lisp, cdr, scopes_b)?;
            lisp.cons(car, rest)
        }
        _ => Ok(scopes_b),
    }
}

// ============================================================================
// Syntax Object Traversal
// ============================================================================

/// Add a scope to all identifiers in a syntax object tree.
/// 
/// This recursively traverses the structure, adding the scope to any
/// syntax objects encountered.
pub fn add_scope_to_all<const N: usize>(
    lisp: &Lisp<N>,
    stx: ArenaIndex,
    scope_id: isize,
) -> ExpandResult<ArenaIndex> {
    match lisp.get(stx)? {
        // Add scope to syntax objects
        Value::Syntax { datum, scopes } => {
            let new_datum = add_scope_to_all(lisp, datum, scope_id)?;
            let scope_val = lisp.number(scope_id)?;
            let new_scopes = lisp.cons(scope_val, scopes)?;
            lisp.syntax(new_datum, new_scopes)
        }
        
        // Recursively process lists
        Value::Cons { car, cdr } => {
            let new_car = add_scope_to_all(lisp, car, scope_id)?;
            let new_cdr = add_scope_to_all(lisp, cdr, scope_id)?;
            lisp.cons(new_car, new_cdr)
        }
        
        // Wrap bare symbols in syntax with the scope
        Value::Symbol(_) => {
            let scope_val = lisp.number(scope_id)?;
            let scopes = lisp.cons(scope_val, lisp.nil()?)?;
            lisp.syntax(stx, scopes)
        }
        
        // Everything else passes through
        _ => Ok(stx)
    }
}

/// Remove a scope from all identifiers in a syntax object tree.
/// 
/// Used for scope flipping in definition contexts.
pub fn remove_scope_from_all<const N: usize>(
    lisp: &Lisp<N>,
    stx: ArenaIndex,
    scope_id: isize,
) -> ExpandResult<ArenaIndex> {
    match lisp.get(stx)? {
        // Remove scope from syntax objects
        Value::Syntax { datum, scopes } => {
            let new_datum = remove_scope_from_all(lisp, datum, scope_id)?;
            let new_scopes = remove_scope_from_list(lisp, scopes, scope_id)?;
            lisp.syntax(new_datum, new_scopes)
        }
        
        // Recursively process lists
        Value::Cons { car, cdr } => {
            let new_car = remove_scope_from_all(lisp, car, scope_id)?;
            let new_cdr = remove_scope_from_all(lisp, cdr, scope_id)?;
            lisp.cons(new_car, new_cdr)
        }
        
        // Everything else passes through
        _ => Ok(stx)
    }
}

/// Remove a scope ID from a scope list.
fn remove_scope_from_list<const N: usize>(
    lisp: &Lisp<N>,
    scopes: ArenaIndex,
    scope_id: isize,
) -> ExpandResult<ArenaIndex> {
    match lisp.get(scopes)? {
        Value::Nil => lisp.nil(),
        Value::Cons { car, cdr } => {
            let rest = remove_scope_from_list(lisp, cdr, scope_id)?;
            if let Value::Number(n) = lisp.get(car)? {
                if n == scope_id {
                    return Ok(rest); // Skip this scope
                }
            }
            lisp.cons(car, rest)
        }
        _ => lisp.nil(),
    }
}

/// Flip a scope on all identifiers in a syntax object tree.
/// 
/// Adds the scope if absent, removes it if present. Used for definition contexts.
pub fn flip_scope_on_all<const N: usize>(
    lisp: &Lisp<N>,
    stx: ArenaIndex,
    scope_id: isize,
) -> ExpandResult<ArenaIndex> {
    match lisp.get(stx)? {
        // Flip scope on syntax objects
        Value::Syntax { datum, scopes } => {
            let new_datum = flip_scope_on_all(lisp, datum, scope_id)?;
            let new_scopes = if scope_list_contains(lisp, scopes, scope_id)? {
                remove_scope_from_list(lisp, scopes, scope_id)?
            } else {
                let scope_val = lisp.number(scope_id)?;
                lisp.cons(scope_val, scopes)?
            };
            lisp.syntax(new_datum, new_scopes)
        }
        
        // Recursively process lists
        Value::Cons { car, cdr } => {
            let new_car = flip_scope_on_all(lisp, car, scope_id)?;
            let new_cdr = flip_scope_on_all(lisp, cdr, scope_id)?;
            lisp.cons(new_car, new_cdr)
        }
        
        // Wrap bare symbols and add the scope
        Value::Symbol(_) => {
            let scope_val = lisp.number(scope_id)?;
            let scopes = lisp.cons(scope_val, lisp.nil()?)?;
            lisp.syntax(stx, scopes)
        }
        
        // Everything else passes through
        _ => Ok(stx)
    }
}

/// Check if a scope list contains a given scope ID.
fn scope_list_contains<const N: usize>(
    lisp: &Lisp<N>,
    scopes: ArenaIndex,
    scope_id: isize,
) -> ExpandResult<bool> {
    let mut current = scopes;
    loop {
        match lisp.get(current)? {
            Value::Nil => return Ok(false),
            Value::Cons { car, cdr } => {
                if let Value::Number(n) = lisp.get(car)? {
                    if n == scope_id {
                        return Ok(true);
                    }
                }
                current = cdr;
            }
            _ => return Ok(false),
        }
    }
}

// ============================================================================
// Identifier Operations
// ============================================================================

/// Check if a syntax object is an identifier (wraps a symbol).
pub fn is_identifier<const N: usize>(
    lisp: &Lisp<N>,
    stx: ArenaIndex,
) -> ExpandResult<bool> {
    let datum = lisp.syntax_datum(stx)?;
    Ok(matches!(lisp.get(datum)?, Value::Symbol(_)))
}

/// Get the symbol name from an identifier (syntax object wrapping a symbol).
/// 
/// Returns the underlying symbol, or the value itself if not a syntax object.
pub fn identifier_symbol<const N: usize>(
    lisp: &Lisp<N>,
    stx: ArenaIndex,
) -> ExpandResult<ArenaIndex> {
    lisp.syntax_datum(stx)
}

/// Check if two identifiers are "free-identifier=?".
/// 
/// Two identifiers are free-identifier=? if they would resolve to the same
/// binding. This compares both the symbol name and the scope sets.
/// 
/// For now, this is a simplified version that just checks symbol equality
/// and scope subset relationship. Full implementation requires environment lookup.
pub fn free_identifier_eq<const N: usize>(
    lisp: &Lisp<N>,
    id1: ArenaIndex,
    id2: ArenaIndex,
) -> ExpandResult<bool> {
    let sym1 = lisp.syntax_datum(id1)?;
    let sym2 = lisp.syntax_datum(id2)?;
    
    // Check symbol equality
    if !lisp.symbol_eq(sym1, sym2)? {
        return Ok(false);
    }
    
    // Check scope equality (for full hygiene)
    let scopes1 = lisp.syntax_scopes(id1)?;
    let scopes2 = lisp.syntax_scopes(id2)?;
    
    // For now, check bidirectional subset (i.e., equality)
    Ok(lisp.scopes_subset(scopes1, scopes2)? && lisp.scopes_subset(scopes2, scopes1)?)
}

/// Check if two identifiers are "bound-identifier=?".
/// 
/// Two identifiers are bound-identifier=? if they have the same symbol name
/// AND the same scope set. This is stricter than free-identifier=?.
pub fn bound_identifier_eq<const N: usize>(
    lisp: &Lisp<N>,
    id1: ArenaIndex,
    id2: ArenaIndex,
) -> ExpandResult<bool> {
    let sym1 = lisp.syntax_datum(id1)?;
    let sym2 = lisp.syntax_datum(id2)?;
    
    // Check symbol equality
    if !lisp.symbol_eq(sym1, sym2)? {
        return Ok(false);
    }
    
    // Check scope set equality
    let scopes1 = lisp.syntax_scopes(id1)?;
    let scopes2 = lisp.syntax_scopes(id2)?;
    
    // Must have exactly the same scopes
    Ok(lisp.scopes_subset(scopes1, scopes2)? && lisp.scopes_subset(scopes2, scopes1)?)
}

// ============================================================================
// Syntax Utilities
// ============================================================================

/// Check if a syntax object represents a list.
pub fn syntax_is_list<const N: usize>(
    lisp: &Lisp<N>,
    stx: ArenaIndex,
) -> ExpandResult<bool> {
    let datum = lisp.syntax_datum(stx)?;
    Ok(matches!(lisp.get(datum)?, Value::Cons { .. } | Value::Nil))
}

/// Get the length of a syntax list.
pub fn syntax_list_length<const N: usize>(
    lisp: &Lisp<N>,
    stx: ArenaIndex,
) -> ExpandResult<usize> {
    let datum = lisp.syntax_datum(stx)?;
    lisp.list_len(datum)
}

/// Get the car of a syntax list, preserving syntax wrapping.
pub fn syntax_car<const N: usize>(
    lisp: &Lisp<N>,
    stx: ArenaIndex,
) -> ExpandResult<ArenaIndex> {
    let datum = lisp.syntax_datum(stx)?;
    lisp.car(datum)
}

/// Get the cdr of a syntax list, preserving syntax wrapping.
pub fn syntax_cdr<const N: usize>(
    lisp: &Lisp<N>,
    stx: ArenaIndex,
) -> ExpandResult<ArenaIndex> {
    let datum = lisp.syntax_datum(stx)?;
    lisp.cdr(datum)
}

/// Convert a syntax list to a Rust vector of syntax elements.
/// 
/// Note: Limited to MAX_SYNTAX_LIST_LEN elements for no_std compatibility.
pub const MAX_SYNTAX_LIST_LEN: usize = 128;

pub fn syntax_to_vec<const N: usize>(
    lisp: &Lisp<N>,
    stx: ArenaIndex,
) -> ExpandResult<([ArenaIndex; MAX_SYNTAX_LIST_LEN], usize)> {
    let mut result = [ArenaIndex::NIL; MAX_SYNTAX_LIST_LEN];
    let mut count = 0;
    
    let mut current = lisp.syntax_datum(stx)?;
    loop {
        match lisp.get(current)? {
            Value::Nil => break,
            Value::Cons { car, cdr } => {
                if count >= MAX_SYNTAX_LIST_LEN {
                    break;
                }
                result[count] = car;
                count += 1;
                current = cdr;
            }
            _ => break,
        }
    }
    
    Ok((result, count))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scope_counter() {
        let mut counter = ScopeCounter::new();
        assert_eq!(counter.fresh(), 0);
        assert_eq!(counter.fresh(), 1);
        assert_eq!(counter.fresh(), 2);
        assert_eq!(counter.current(), 3);
    }
    
    #[test]
    fn test_datum_to_syntax_symbol() {
        let lisp: Lisp<1000> = Lisp::new();
        let sym = lisp.symbol("foo").unwrap();
        let scopes = lisp.nil().unwrap();
        
        let stx = datum_to_syntax(&lisp, sym, scopes).unwrap();
        
        // Should be wrapped in syntax
        assert!(matches!(lisp.get(stx).unwrap(), Value::Syntax { .. }));
        
        // Datum should be the original symbol
        let datum = lisp.syntax_datum(stx).unwrap();
        assert!(lisp.symbol_eq(datum, sym).unwrap());
    }
    
    #[test]
    fn test_datum_to_syntax_list() {
        let lisp: Lisp<1000> = Lisp::new();
        let a = lisp.symbol("a").unwrap();
        let b = lisp.symbol("b").unwrap();
        let nil = lisp.nil().unwrap();
        let list = lisp.cons(a, lisp.cons(b, nil).unwrap()).unwrap();
        
        let scopes = lisp.nil().unwrap();
        let stx = datum_to_syntax(&lisp, list, scopes).unwrap();
        
        // Should still be a cons
        assert!(matches!(lisp.get(stx).unwrap(), Value::Cons { .. }));
        
        // Car should be a syntax-wrapped symbol
        let car = lisp.car(stx).unwrap();
        assert!(matches!(lisp.get(car).unwrap(), Value::Syntax { .. }));
    }
    
    #[test]
    fn test_syntax_to_datum() {
        let lisp: Lisp<1000> = Lisp::new();
        let sym = lisp.symbol("foo").unwrap();
        let scopes = lisp.nil().unwrap();
        
        let stx = lisp.syntax(sym, scopes).unwrap();
        let datum = syntax_to_datum(&lisp, stx).unwrap();
        
        // Should unwrap to the original symbol
        assert!(lisp.symbol_eq(datum, sym).unwrap());
    }
    
    #[test]
    fn test_add_scope_to_all() {
        let lisp: Lisp<1000> = Lisp::new();
        let sym = lisp.symbol("x").unwrap();
        let empty_scopes = lisp.nil().unwrap();
        let stx = lisp.syntax(sym, empty_scopes).unwrap();
        
        let result = add_scope_to_all(&lisp, stx, 42).unwrap();
        
        // Should have the new scope
        let scopes = lisp.syntax_scopes(result).unwrap();
        assert!(scope_list_contains(&lisp, scopes, 42).unwrap());
    }
    
    #[test]
    fn test_remove_scope_from_all() {
        let lisp: Lisp<1000> = Lisp::new();
        let sym = lisp.symbol("x").unwrap();
        
        // Create syntax with scope 42
        let scope_val = lisp.number(42).unwrap();
        let scopes = lisp.cons(scope_val, lisp.nil().unwrap()).unwrap();
        let stx = lisp.syntax(sym, scopes).unwrap();
        
        // Remove scope 42
        let result = remove_scope_from_all(&lisp, stx, 42).unwrap();
        
        // Should not have scope 42 anymore
        let new_scopes = lisp.syntax_scopes(result).unwrap();
        assert!(!scope_list_contains(&lisp, new_scopes, 42).unwrap());
    }
    
    #[test]
    fn test_flip_scope_on_all_add() {
        let lisp: Lisp<1000> = Lisp::new();
        let sym = lisp.symbol("x").unwrap();
        let empty_scopes = lisp.nil().unwrap();
        let stx = lisp.syntax(sym, empty_scopes).unwrap();
        
        // Flip should add scope 99 (not present)
        let result = flip_scope_on_all(&lisp, stx, 99).unwrap();
        
        let scopes = lisp.syntax_scopes(result).unwrap();
        assert!(scope_list_contains(&lisp, scopes, 99).unwrap());
    }
    
    #[test]
    fn test_flip_scope_on_all_remove() {
        let lisp: Lisp<1000> = Lisp::new();
        let sym = lisp.symbol("x").unwrap();
        
        // Create syntax with scope 99
        let scope_val = lisp.number(99).unwrap();
        let scopes = lisp.cons(scope_val, lisp.nil().unwrap()).unwrap();
        let stx = lisp.syntax(sym, scopes).unwrap();
        
        // Flip should remove scope 99 (present)
        let result = flip_scope_on_all(&lisp, stx, 99).unwrap();
        
        let new_scopes = lisp.syntax_scopes(result).unwrap();
        assert!(!scope_list_contains(&lisp, new_scopes, 99).unwrap());
    }
    
    #[test]
    fn test_is_identifier() {
        let lisp: Lisp<1000> = Lisp::new();
        
        // Symbol wrapped in syntax is an identifier
        let sym = lisp.symbol("foo").unwrap();
        let stx = lisp.syntax(sym, lisp.nil().unwrap()).unwrap();
        assert!(is_identifier(&lisp, stx).unwrap());
        
        // Number is not an identifier
        let num = lisp.number(42).unwrap();
        assert!(!is_identifier(&lisp, num).unwrap());
    }
    
    #[test]
    fn test_bound_identifier_eq() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let foo = lisp.symbol("foo").unwrap();
        let scope1 = lisp.number(1).unwrap();
        let scopes1 = lisp.cons(scope1, lisp.nil().unwrap()).unwrap();
        let id1 = lisp.syntax(foo, scopes1).unwrap();
        
        // Same symbol, same scopes
        let scopes2 = lisp.cons(scope1, lisp.nil().unwrap()).unwrap();
        let id2 = lisp.syntax(foo, scopes2).unwrap();
        assert!(bound_identifier_eq(&lisp, id1, id2).unwrap());
        
        // Same symbol, different scopes
        let scope2 = lisp.number(2).unwrap();
        let scopes3 = lisp.cons(scope2, lisp.nil().unwrap()).unwrap();
        let id3 = lisp.syntax(foo, scopes3).unwrap();
        assert!(!bound_identifier_eq(&lisp, id1, id3).unwrap());
    }
    
    #[test]
    fn test_syntax_car_cdr() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let a = lisp.symbol("a").unwrap();
        let b = lisp.symbol("b").unwrap();
        let nil = lisp.nil().unwrap();
        let list = lisp.cons(a, lisp.cons(b, nil).unwrap()).unwrap();
        
        // Wrap in syntax
        let stx = datum_to_syntax(&lisp, list, nil).unwrap();
        
        // syntax_car should give us the first element
        let car = syntax_car(&lisp, stx).unwrap();
        let car_datum = lisp.syntax_datum(car).unwrap();
        assert!(lisp.symbol_eq(car_datum, a).unwrap());
        
        // syntax_cdr should give us the rest
        let cdr = syntax_cdr(&lisp, stx).unwrap();
        assert!(matches!(lisp.get(cdr).unwrap(), Value::Cons { .. }));
    }
    
    #[test]
    fn test_datum_to_syntax_preserves_numbers() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let num = lisp.number(42).unwrap();
        let scopes = lisp.nil().unwrap();
        
        // Numbers should pass through unchanged
        let result = datum_to_syntax(&lisp, num, scopes).unwrap();
        assert!(matches!(lisp.get(result).unwrap(), Value::Number(42)));
    }
    
    #[test]
    fn test_nested_datum_to_syntax() {
        let lisp: Lisp<1000> = Lisp::new();
        
        // Build (if (> x 0) x 0)
        let if_sym = lisp.symbol("if").unwrap();
        let gt_sym = lisp.symbol(">").unwrap();
        let x_sym = lisp.symbol("x").unwrap();
        let zero = lisp.number(0).unwrap();
        let nil = lisp.nil().unwrap();
        
        // (> x 0)
        let condition = lisp.list([gt_sym, x_sym, zero]).unwrap();
        // (if (> x 0) x 0)
        let expr = lisp.list([if_sym, condition, x_sym, zero]).unwrap();
        
        // Convert to syntax
        let scope_val = lisp.number(1).unwrap();
        let scopes = lisp.cons(scope_val, nil).unwrap();
        let stx = datum_to_syntax(&lisp, expr, scopes).unwrap();
        
        // The if symbol should be wrapped with scopes
        let if_stx = lisp.car(stx).unwrap();
        assert!(matches!(lisp.get(if_stx).unwrap(), Value::Syntax { .. }));
        
        let if_scopes = lisp.syntax_scopes(if_stx).unwrap();
        assert!(scope_list_contains(&lisp, if_scopes, 1).unwrap());
    }
}
