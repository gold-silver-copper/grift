#![no_std]
#![forbid(unsafe_code)]

//! # Bidirectional Type Checker for Lisp
//!
//! This crate implements a bidirectional type checker based on the paper
//! "Complete and Easy Bidirectional Typechecking for Higher-Rank Polymorphism"
//! by Dunfield and Krishnaswami.
//!
//! ## Type System Overview
//!
//! The type system supports:
//! - Base types: `isize`, `bool`, `nil`, `char`
//! - Function types: `(fn t1 t2)` - function from t1 to t2
//! - Multi-param functions: `(fn (t1 t2 ...) result)` - curried automatically
//! - List types: `(list t)` - homogeneous list of type t
//! - Pair types: `(pair t1 t2)` - pair of t1 and t2
//! - Polymorphic types: `(forall (a b ...) type)` - universal quantification
//!
//! ## Bidirectional Typing
//!
//! The type checker uses two modes:
//! - **Synthesize** (⇒): Infer the type of an expression
//! - **Check** (⇐): Verify that an expression has an expected type
//!
//! The key insight is that some expressions have obvious types (literals, variables
//! with known types) while others need type annotations (lambdas without param types).
//!
//! ## Syntax (Pure S-expressions, lowercase)
//!
//! All type syntax uses pure Lisp S-expressions:
//!
//! ```text
//! ;; Base types
//! isize                    ;; integer type (like Rust's isize)
//! bool                     ;; boolean type
//! nil                      ;; unit/nil type
//! char                     ;; character type
//!
//! ;; Function types
//! (fn isize isize)         ;; function from isize to isize
//! (fn isize (fn isize isize))  ;; curried binary function
//! (fn (isize isize) isize)     ;; multi-param function (curried)
//!
//! ;; Collection types
//! (list isize)             ;; list of isize
//! (pair isize bool)        ;; pair of isize and bool
//!
//! ;; Polymorphic types
//! (forall (a) (fn a a))    ;; polymorphic identity
//! (forall (a b) (fn ((fn a b) (list a)) (list b)))  ;; map type
//!
//! ;; Type annotations using (the type expr)
//! (the isize 42)           ;; annotate 42 as isize
//! (the (fn isize isize) (lambda (x) x))  ;; annotate identity function
//!
//! ;; Type declarations with declare
//! (declare add (fn (isize isize) isize))
//! (define (add x y) (+ x y))
//!
//! ;; Typed lambdas
//! (lambda-typed ((x isize) (y isize)) (+ x y))
//! ```
//!
//! ## Implementation Notes
//!
//! This is a no_std, no_alloc implementation. All type representations use
//! the arena allocator from pwn_arena.

pub use lisp_parser::{
    Arena, ArenaIndex, ArenaError, ArenaResult, Trace, GcStats,
    Value, Builtin, StdLib, Lisp, ParseError,
};

// ============================================================================
// Type Representation
// ============================================================================

/// A type in the Lisp type system.
///
/// Types are represented as arena-allocated values to maintain the no_alloc
/// constraint. The type representation mirrors the S-expression syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    /// Integer type `isize`
    Isize,
    
    /// Boolean type `bool`
    Bool,
    
    /// Nil/unit type `nil`
    Nil,
    
    /// Character type `char`
    Char,
    
    /// Function type `(fn param_type return_type)`
    Arrow {
        param: ArenaIndex,   // Points to Type
        result: ArenaIndex,  // Points to Type
    },
    
    /// List type `(list element_type)`
    List {
        element: ArenaIndex, // Points to Type
    },
    
    /// Pair type `(pair first_type second_type)`
    Pair {
        first: ArenaIndex,   // Points to Type
        second: ArenaIndex,  // Points to Type
    },
    
    /// Type variable (for polymorphism support)
    /// Represented as a unique identifier
    Var(u32),
    
    /// Universal quantification `(forall (a b ...) type)`
    /// type_vars is a list of type variable names in the Lisp arena
    /// body is the quantified type
    ForAll {
        type_vars: ArenaIndex,  // List of type variable symbols in Lisp arena
        body: ArenaIndex,       // Points to Type
    },
}

impl Type {
    /// Check if this type is a base type (no nested type references)
    #[inline]
    pub const fn is_base(&self) -> bool {
        matches!(self, Type::Isize | Type::Bool | Type::Nil | Type::Char)
    }
    
    /// Check if this is a forall type
    #[inline]
    pub const fn is_forall(&self) -> bool {
        matches!(self, Type::ForAll { .. })
    }
    
    /// Check if this is a function type
    #[inline]
    pub const fn is_arrow(&self) -> bool {
        matches!(self, Type::Arrow { .. })
    }
    
    /// Check if this is a list type
    #[inline]
    pub const fn is_list(&self) -> bool {
        matches!(self, Type::List { .. })
    }
    
    /// Check if this is a pair type
    #[inline]
    pub const fn is_pair(&self) -> bool {
        matches!(self, Type::Pair { .. })
    }
    
    /// Get a human-readable name for this type kind
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Type::Isize => "isize",
            Type::Bool => "bool",
            Type::Nil => "nil",
            Type::Char => "char",
            Type::Arrow { .. } => "function",
            Type::List { .. } => "list",
            Type::Pair { .. } => "pair",
            Type::Var(_) => "type variable",
            Type::ForAll { .. } => "forall",
        }
    }
}

/// Implement Trace for GC support
impl<const N: usize> Trace<Type, N> for Type {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match self {
            Type::Isize | Type::Bool | Type::Nil | Type::Char | Type::Var(_) => {
                // No references
            }
            Type::Arrow { param, result } => {
                tracer(*param);
                tracer(*result);
            }
            Type::List { element } => {
                tracer(*element);
            }
            Type::Pair { first, second } => {
                tracer(*first);
                tracer(*second);
            }
            Type::ForAll { body, .. } => {
                // type_vars is in the Lisp arena, not type arena
                tracer(*body);
            }
        }
    }
}

// ============================================================================
// Type Context
// ============================================================================

/// Maximum number of type bindings in a context
const MAX_TYPE_BINDINGS: usize = 256;

/// A type binding: variable name to type
#[derive(Clone, Copy, Debug)]
struct TypeBinding {
    /// Symbol index in the Lisp arena
    name: ArenaIndex,
    /// Type index in the type arena
    ty: ArenaIndex,
}

impl Default for TypeBinding {
    fn default() -> Self {
        TypeBinding {
            name: ArenaIndex::NULL,
            ty: ArenaIndex::NULL,
        }
    }
}

/// Type context - maps variable names to their types
#[derive(Clone, Copy)]
pub struct TypeContext {
    bindings: [TypeBinding; MAX_TYPE_BINDINGS],
    len: usize,
}

impl TypeContext {
    /// Create an empty type context
    pub const fn new() -> Self {
        TypeContext {
            bindings: [TypeBinding { name: ArenaIndex::NULL, ty: ArenaIndex::NULL }; MAX_TYPE_BINDINGS],
            len: 0,
        }
    }
    
    /// Extend context with a new binding
    pub fn extend(&self, name: ArenaIndex, ty: ArenaIndex) -> Option<Self> {
        if self.len >= MAX_TYPE_BINDINGS {
            return None;
        }
        
        let mut new_ctx = *self;
        new_ctx.bindings[new_ctx.len] = TypeBinding { name, ty };
        new_ctx.len += 1;
        Some(new_ctx)
    }
    
    /// Look up a variable's type in the context
    pub fn lookup(&self, name: ArenaIndex) -> Option<ArenaIndex> {
        // Search from end to beginning for shadowing
        for i in (0..self.len).rev() {
            if self.bindings[i].name == name {
                return Some(self.bindings[i].ty);
            }
        }
        None
    }
}

impl Default for TypeContext {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Type Error
// ============================================================================

/// Type checking error kinds
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeErrorKind {
    /// Type mismatch between expected and actual
    Mismatch,
    
    /// Unknown variable (not in context)
    UnboundVariable,
    
    /// Cannot synthesize type (needs annotation)
    CannotSynthesize,
    
    /// Invalid type syntax
    InvalidTypeSyntax,
    
    /// Expected function type for application
    NotAFunction,
    
    /// Arena allocation error
    ArenaError,
    
    /// Context overflow
    ContextOverflow,
}

impl TypeErrorKind {
    /// Get a human-readable description
    pub const fn as_str(&self) -> &'static str {
        match self {
            TypeErrorKind::Mismatch => "type mismatch",
            TypeErrorKind::UnboundVariable => "unbound variable",
            TypeErrorKind::CannotSynthesize => "cannot infer type (add annotation)",
            TypeErrorKind::InvalidTypeSyntax => "invalid type syntax",
            TypeErrorKind::NotAFunction => "expected function type",
            TypeErrorKind::ArenaError => "arena allocation error",
            TypeErrorKind::ContextOverflow => "type context overflow",
        }
    }
}

/// A type error with context
#[derive(Clone, Copy, Debug)]
pub struct TypeError {
    /// The kind of error
    pub kind: TypeErrorKind,
    /// The expression that caused the error
    pub expr: ArenaIndex,
    /// Expected type (if applicable)
    pub expected: ArenaIndex,
    /// Actual type (if applicable)
    pub actual: ArenaIndex,
}

impl TypeError {
    /// Create a new type error
    pub const fn new(kind: TypeErrorKind, expr: ArenaIndex) -> Self {
        TypeError {
            kind,
            expr,
            expected: ArenaIndex::NULL,
            actual: ArenaIndex::NULL,
        }
    }
    
    /// Set expected and actual types for mismatch errors
    pub const fn with_types(mut self, expected: ArenaIndex, actual: ArenaIndex) -> Self {
        self.expected = expected;
        self.actual = actual;
        self
    }
}

/// Result type for type checking
pub type TypeResult<T> = Result<T, TypeError>;

// ============================================================================
// Type Checker
// ============================================================================

/// The bidirectional type checker
///
/// This struct holds references to both the Lisp arena (for expressions)
/// and a type arena (for type representations).
pub struct TypeChecker<'a, const N: usize, const M: usize> {
    /// Lisp context for expressions
    lisp: &'a Lisp<N>,
    /// Type arena for type representations
    types: &'a Arena<Type, M>,
}

impl<'a, const N: usize, const M: usize> TypeChecker<'a, N, M> {
    /// Create a new type checker
    pub fn new(lisp: &'a Lisp<N>, types: &'a Arena<Type, M>) -> Self {
        TypeChecker {
            lisp,
            types,
        }
    }
    
    /// Get the Lisp context
    pub fn lisp(&self) -> &Lisp<N> {
        self.lisp
    }
    
    /// Get the type arena
    pub fn type_arena(&self) -> &Arena<Type, M> {
        self.types
    }
    
    // ========================================================================
    // Type Allocation Helpers
    // ========================================================================
    
    /// Allocate the isize type
    pub fn isize_type(&self) -> TypeResult<ArenaIndex> {
        self.types.alloc(Type::Isize)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, ArenaIndex::NULL))
    }
    
    /// Allocate the bool type
    pub fn bool_type(&self) -> TypeResult<ArenaIndex> {
        self.types.alloc(Type::Bool)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, ArenaIndex::NULL))
    }
    
    /// Allocate the nil type
    pub fn nil_type(&self) -> TypeResult<ArenaIndex> {
        self.types.alloc(Type::Nil)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, ArenaIndex::NULL))
    }
    
    /// Allocate the char type
    pub fn char_type(&self) -> TypeResult<ArenaIndex> {
        self.types.alloc(Type::Char)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, ArenaIndex::NULL))
    }
    
    /// Allocate a function type
    pub fn arrow_type(&self, param: ArenaIndex, result: ArenaIndex) -> TypeResult<ArenaIndex> {
        self.types.alloc(Type::Arrow { param, result })
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, ArenaIndex::NULL))
    }
    
    /// Allocate a list type
    pub fn list_type(&self, element: ArenaIndex) -> TypeResult<ArenaIndex> {
        self.types.alloc(Type::List { element })
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, ArenaIndex::NULL))
    }
    
    /// Allocate a pair type
    pub fn pair_type(&self, first: ArenaIndex, second: ArenaIndex) -> TypeResult<ArenaIndex> {
        self.types.alloc(Type::Pair { first, second })
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, ArenaIndex::NULL))
    }
    
    /// Allocate a forall type
    pub fn forall_type(&self, type_vars: ArenaIndex, body: ArenaIndex) -> TypeResult<ArenaIndex> {
        self.types.alloc(Type::ForAll { type_vars, body })
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, ArenaIndex::NULL))
    }
    
    /// Allocate a type variable
    pub fn var_type(&self, id: u32) -> TypeResult<ArenaIndex> {
        self.types.alloc(Type::Var(id))
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, ArenaIndex::NULL))
    }
    
    /// Get a type from the type arena
    pub fn get_type(&self, idx: ArenaIndex) -> TypeResult<Type> {
        self.types.get(idx)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, ArenaIndex::NULL))
    }
    
    // ========================================================================
    // Type Parsing from S-expressions
    // ========================================================================
    
    /// Parse a type from a Lisp S-expression
    ///
    /// Type syntax (pure S-expressions, lowercase):
    /// - `isize` - integer type
    /// - `bool` - boolean type  
    /// - `nil` - nil/unit type
    /// - `char` - character type
    /// - `(fn param_type result_type)` - single-param function type
    /// - `(fn (t1 t2 ...) result)` - multi-param function type (curried)
    /// - `(list t)` - list type
    /// - `(pair t1 t2)` - pair type
    /// - `(forall (a b ...) type)` - polymorphic type
    /// - `a` - type variable (lowercase single letter or any symbol in forall scope)
    pub fn parse_type(&self, expr: ArenaIndex) -> TypeResult<ArenaIndex> {
        self.parse_type_with_vars(expr, ArenaIndex::NULL)
    }
    
    /// Parse a type with known type variables in scope
    fn parse_type_with_vars(&self, expr: ArenaIndex, type_vars: ArenaIndex) -> TypeResult<ArenaIndex> {
        let val = self.lisp.get(expr)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
        
        match val {
            // Base type symbols or type variables
            Value::Symbol { .. } => {
                if self.lisp.symbol_matches(expr, "isize")
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))? 
                {
                    return self.isize_type();
                }
                if self.lisp.symbol_matches(expr, "bool")
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))? 
                {
                    return self.bool_type();
                }
                if self.lisp.symbol_matches(expr, "nil")
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))? 
                {
                    return self.nil_type();
                }
                if self.lisp.symbol_matches(expr, "char")
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))? 
                {
                    return self.char_type();
                }
                
                // Check if it's a type variable in scope
                if !type_vars.is_null() {
                    if let Some(idx) = self.find_type_var_index(expr, type_vars)? {
                        return self.var_type(idx);
                    }
                }
                
                // Unknown type symbol
                Err(TypeError::new(TypeErrorKind::InvalidTypeSyntax, expr))
            }
            
            // Compound types
            Value::Cons { car, cdr: _ } => {
                let head = self.lisp.get(car)
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
                
                if let Value::Symbol { .. } = head {
                    // Function type: (fn param result) or (fn (t1 t2 ...) result)
                    if self.lisp.symbol_matches(car, "fn")
                        .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))? 
                    {
                        return self.parse_fn_type(expr, type_vars);
                    }
                    
                    // List type: (list element_type)
                    if self.lisp.symbol_matches(car, "list")
                        .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))? 
                    {
                        let args = self.lisp.cdr(expr)
                            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
                        let elem_expr = self.lisp.car(args)
                            .map_err(|_| TypeError::new(TypeErrorKind::InvalidTypeSyntax, expr))?;
                        
                        let element = self.parse_type_with_vars(elem_expr, type_vars)?;
                        
                        return self.list_type(element);
                    }
                    
                    // Pair type: (pair first_type second_type)
                    if self.lisp.symbol_matches(car, "pair")
                        .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))? 
                    {
                        let args = self.lisp.cdr(expr)
                            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
                        let first_expr = self.lisp.car(args)
                            .map_err(|_| TypeError::new(TypeErrorKind::InvalidTypeSyntax, expr))?;
                        let rest = self.lisp.cdr(args)
                            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
                        let second_expr = self.lisp.car(rest)
                            .map_err(|_| TypeError::new(TypeErrorKind::InvalidTypeSyntax, expr))?;
                        
                        let first = self.parse_type_with_vars(first_expr, type_vars)?;
                        let second = self.parse_type_with_vars(second_expr, type_vars)?;
                        
                        return self.pair_type(first, second);
                    }
                    
                    // Forall type: (forall (a b ...) type)
                    if self.lisp.symbol_matches(car, "forall")
                        .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?
                    {
                        return self.parse_forall_type(expr);
                    }
                }
                
                Err(TypeError::new(TypeErrorKind::InvalidTypeSyntax, expr))
            }
            
            _ => Err(TypeError::new(TypeErrorKind::InvalidTypeSyntax, expr))
        }
    }
    
    /// Parse a function type: (fn param result) or (fn (t1 t2 ...) result)
    fn parse_fn_type(&self, expr: ArenaIndex, type_vars: ArenaIndex) -> TypeResult<ArenaIndex> {
        let args = self.lisp.cdr(expr)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
        let param_expr = self.lisp.car(args)
            .map_err(|_| TypeError::new(TypeErrorKind::InvalidTypeSyntax, expr))?;
        let rest = self.lisp.cdr(args)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
        let result_expr = self.lisp.car(rest)
            .map_err(|_| TypeError::new(TypeErrorKind::InvalidTypeSyntax, expr))?;
        
        // Check if param_expr is a list (multi-param function)
        let param_val = self.lisp.get(param_expr)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
        
        match param_val {
            Value::Cons { .. } => {
                // Multi-param: (fn (t1 t2 ...) result) -> curried function
                self.parse_multi_param_fn(param_expr, result_expr, type_vars)
            }
            _ => {
                // Single-param: (fn t1 t2)
                let param = self.parse_type_with_vars(param_expr, type_vars)?;
                let result = self.parse_type_with_vars(result_expr, type_vars)?;
                self.arrow_type(param, result)
            }
        }
    }
    
    /// Parse a multi-param function type: (fn (t1 t2 ...) result)
    /// Converts to curried form: t1 -> t2 -> ... -> result
    fn parse_multi_param_fn(&self, params: ArenaIndex, result_expr: ArenaIndex, type_vars: ArenaIndex) -> TypeResult<ArenaIndex> {
        let result_type = self.parse_type_with_vars(result_expr, type_vars)?;
        self.parse_param_list_to_arrow(params, result_type, type_vars)
    }
    
    /// Convert a list of param types to a curried arrow type
    fn parse_param_list_to_arrow(&self, params: ArenaIndex, result: ArenaIndex, type_vars: ArenaIndex) -> TypeResult<ArenaIndex> {
        let val = self.lisp.get(params)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, params))?;
        
        match val {
            Value::Nil => Ok(result),
            Value::Cons { car: first, cdr: rest } => {
                // Parse remaining params first (right-to-left construction)
                let rest_type = self.parse_param_list_to_arrow(rest, result, type_vars)?;
                let first_type = self.parse_type_with_vars(first, type_vars)?;
                self.arrow_type(first_type, rest_type)
            }
            _ => Err(TypeError::new(TypeErrorKind::InvalidTypeSyntax, params))
        }
    }
    
    /// Parse a forall type: (forall (a b ...) type)
    fn parse_forall_type(&self, expr: ArenaIndex) -> TypeResult<ArenaIndex> {
        let args = self.lisp.cdr(expr)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
        let vars_expr = self.lisp.car(args)
            .map_err(|_| TypeError::new(TypeErrorKind::InvalidTypeSyntax, expr))?;
        let rest = self.lisp.cdr(args)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
        let body_expr = self.lisp.car(rest)
            .map_err(|_| TypeError::new(TypeErrorKind::InvalidTypeSyntax, expr))?;
        
        // Parse body with vars in scope
        let body = self.parse_type_with_vars(body_expr, vars_expr)?;
        
        self.forall_type(vars_expr, body)
    }
    
    /// Find index of a type variable in the type variables list
    fn find_type_var_index(&self, var: ArenaIndex, type_vars: ArenaIndex) -> TypeResult<Option<u32>> {
        let mut current = type_vars;
        let mut idx = 0u32;
        
        loop {
            let val = self.lisp.get(current)
                .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, current))?;
            
            match val {
                Value::Nil => return Ok(None),
                Value::Cons { car, cdr } => {
                    // Symbols are interned, so ArenaIndex comparison is sufficient
                    if car == var {
                        return Ok(Some(idx));
                    }
                    current = cdr;
                    idx += 1;
                }
                _ => return Err(TypeError::new(TypeErrorKind::InvalidTypeSyntax, type_vars))
            }
        }
    }
    
    // ========================================================================
    // Type Equality
    // ========================================================================
    
    /// Check if two types are equal
    pub fn types_equal(&self, t1: ArenaIndex, t2: ArenaIndex) -> TypeResult<bool> {
        // Same index means same type
        if t1 == t2 {
            return Ok(true);
        }
        
        let ty1 = self.get_type(t1)?;
        let ty2 = self.get_type(t2)?;
        
        match (ty1, ty2) {
            (Type::Isize, Type::Isize) => Ok(true),
            (Type::Bool, Type::Bool) => Ok(true),
            (Type::Nil, Type::Nil) => Ok(true),
            (Type::Char, Type::Char) => Ok(true),
            (Type::Var(v1), Type::Var(v2)) => Ok(v1 == v2),
            
            (Type::Arrow { param: p1, result: r1 }, Type::Arrow { param: p2, result: r2 }) => {
                Ok(self.types_equal(p1, p2)? && self.types_equal(r1, r2)?)
            }
            
            (Type::List { element: e1 }, Type::List { element: e2 }) => {
                self.types_equal(e1, e2)
            }
            
            (Type::Pair { first: f1, second: s1 }, Type::Pair { first: f2, second: s2 }) => {
                Ok(self.types_equal(f1, f2)? && self.types_equal(s1, s2)?)
            }
            
            (Type::ForAll { type_vars: _, body: b1 }, Type::ForAll { type_vars: _, body: b2 }) => {
                // For now, just check body equality (assumes same variable ordering)
                self.types_equal(b1, b2)
            }
            
            _ => Ok(false),
        }
    }
    
    // ========================================================================
    // Subtyping
    // ========================================================================
    
    /// Check if t1 is a subtype of t2 (t1 <: t2)
    ///
    /// Currently uses simple equality - can be extended for:
    /// - Polymorphism
    /// - Width subtyping for records
    /// - etc.
    pub fn is_subtype(&self, t1: ArenaIndex, t2: ArenaIndex) -> TypeResult<bool> {
        // For now, subtyping is just equality
        // Can be extended later for polymorphism
        self.types_equal(t1, t2)
    }
    
    // ========================================================================
    // Bidirectional Type Checking - Core
    // ========================================================================
    
    /// Synthesize a type for an expression (⇒)
    ///
    /// Given an expression, infer its type without any expected type information.
    /// Returns the synthesized type index.
    pub fn synthesize(&mut self, ctx: &TypeContext, expr: ArenaIndex) -> TypeResult<ArenaIndex> {
        let val = self.lisp.get(expr)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
        
        match val {
            // Literals synthesize their natural type
            Value::Number(_) => self.isize_type(),
            Value::True | Value::False => self.bool_type(),
            Value::Nil => self.nil_type(),
            Value::Char(_) => self.char_type(),
            
            // Variables look up their type in context
            Value::Symbol { .. } => {
                ctx.lookup(expr)
                    .ok_or(TypeError::new(TypeErrorKind::UnboundVariable, expr))
            }
            
            // Application: synthesize function type, check argument
            Value::Cons { car, cdr } => {
                self.synthesize_application(ctx, car, cdr, expr)
            }
            
            // Lambda without annotation cannot synthesize
            Value::Lambda { .. } => {
                Err(TypeError::new(TypeErrorKind::CannotSynthesize, expr))
            }
            
            // Thunks: synthesize the underlying expression
            Value::Thunk { expr: inner_expr, .. } => {
                self.synthesize(ctx, inner_expr)
            }
            
            // Builtins have known types
            Value::Builtin(builtin) => {
                self.builtin_type(builtin)
            }
            
            // StdLib functions would need type signatures
            Value::StdLib { .. } => {
                Err(TypeError::new(TypeErrorKind::CannotSynthesize, expr))
            }
            
            // Memoized functions inherit the wrapped function's type
            Value::Memo { func, .. } => {
                self.synthesize(ctx, func)
            }
        }
    }
    
    /// Synthesize type for function application
    fn synthesize_application(
        &mut self, 
        ctx: &TypeContext, 
        func_expr: ArenaIndex, 
        args_expr: ArenaIndex,
        full_expr: ArenaIndex,
    ) -> TypeResult<ArenaIndex> {
        // Check for type annotation (: expr type)
        let func_val = self.lisp.get(func_expr)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        
        if let Value::Symbol { .. } = func_val {
            // Check for (: expr type) annotation
            if self.lisp.symbol_matches(func_expr, ":")
                .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))? 
            {
                let expr_to_check = self.lisp.car(args_expr)
                    .map_err(|_| TypeError::new(TypeErrorKind::InvalidTypeSyntax, full_expr))?;
                let rest = self.lisp.cdr(args_expr)
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
                let type_expr = self.lisp.car(rest)
                    .map_err(|_| TypeError::new(TypeErrorKind::InvalidTypeSyntax, full_expr))?;
                
                let expected_type = self.parse_type(type_expr)?;
                self.check(ctx, expr_to_check, expected_type)?;
                
                return Ok(expected_type);
            }
            
            // Check for special forms that need type synthesis
            if self.lisp.symbol_matches(func_expr, "if")
                .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))? 
            {
                return self.synthesize_if(ctx, args_expr, full_expr);
            }
            
            if self.lisp.symbol_matches(func_expr, "let")
                .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))? 
            {
                return self.synthesize_let(ctx, args_expr, full_expr);
            }
            
            if self.lisp.symbol_matches(func_expr, "begin")
                .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))? 
            {
                return self.synthesize_begin(ctx, args_expr, full_expr);
            }
            
            if self.lisp.symbol_matches(func_expr, "quote")
                .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?
            {
                // Quoted expressions have unknown type without further analysis
                // For simplicity, we could return a "any" type or require annotation
                return Err(TypeError::new(TypeErrorKind::CannotSynthesize, full_expr));
            }
            
            if self.lisp.symbol_matches(func_expr, "lambda")
                .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?
            {
                // Lambda without annotation cannot synthesize
                return Err(TypeError::new(TypeErrorKind::CannotSynthesize, full_expr));
            }
        }
        
        // Regular function application
        let func_type = self.synthesize(ctx, func_expr)?;
        let func_ty = self.get_type(func_type)?;
        
        match func_ty {
            Type::Arrow { param, result } => {
                // Get the argument expression
                let arg_expr = self.lisp.car(args_expr)
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
                
                // Check argument against parameter type
                self.check(ctx, arg_expr, param)?;
                
                // Return result type
                Ok(result)
            }
            _ => {
                Err(TypeError::new(TypeErrorKind::NotAFunction, func_expr)
                    .with_types(ArenaIndex::NULL, func_type))
            }
        }
    }
    
    /// Synthesize type for if expression
    fn synthesize_if(
        &mut self,
        ctx: &TypeContext,
        args: ArenaIndex,
        full_expr: ArenaIndex,
    ) -> TypeResult<ArenaIndex> {
        // (if cond then else)
        let cond = self.lisp.car(args)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        let rest = self.lisp.cdr(args)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        let then_expr = self.lisp.car(rest)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        let rest2 = self.lisp.cdr(rest)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        
        // Check condition is bool
        let bool_type = self.bool_type()?;
        self.check(ctx, cond, bool_type)?;
        
        // Synthesize then branch type
        let then_type = self.synthesize(ctx, then_expr)?;
        
        // Check if there's an else branch
        let else_val = self.lisp.get(rest2)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        
        if else_val.is_nil() {
            // No else branch - result is the then branch type
            // (could also require nil type for then in this case)
            Ok(then_type)
        } else {
            let else_expr = self.lisp.car(rest2)
                .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
            
            // Check else branch against then branch type (they must match)
            self.check(ctx, else_expr, then_type)?;
            
            Ok(then_type)
        }
    }
    
    /// Synthesize type for let expression
    fn synthesize_let(
        &mut self,
        ctx: &TypeContext,
        args: ArenaIndex,
        full_expr: ArenaIndex,
    ) -> TypeResult<ArenaIndex> {
        // (let ((x e) ...) body)
        let bindings = self.lisp.car(args)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        let body_rest = self.lisp.cdr(args)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        let body = self.lisp.car(body_rest)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        
        // Process bindings and extend context
        let new_ctx = self.process_let_bindings(ctx, bindings, full_expr)?;
        
        // Synthesize body type in extended context
        self.synthesize(&new_ctx, body)
    }
    
    /// Process let bindings and return extended context
    fn process_let_bindings(
        &mut self,
        ctx: &TypeContext,
        bindings: ArenaIndex,
        full_expr: ArenaIndex,
    ) -> TypeResult<TypeContext> {
        let mut new_ctx = *ctx;
        let mut current = bindings;
        
        loop {
            let val = self.lisp.get(current)
                .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
            
            match val {
                Value::Nil => break,
                Value::Cons { car: binding, cdr: rest } => {
                    // Each binding is (name expr) or (name : type expr)
                    let name = self.lisp.car(binding)
                        .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
                    let expr_rest = self.lisp.cdr(binding)
                        .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
                    let init_expr = self.lisp.car(expr_rest)
                        .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
                    
                    // Synthesize type of init expression
                    let ty = self.synthesize(&new_ctx, init_expr)?;
                    
                    // Extend context
                    new_ctx = new_ctx.extend(name, ty)
                        .ok_or(TypeError::new(TypeErrorKind::ContextOverflow, full_expr))?;
                    
                    current = rest;
                }
                _ => return Err(TypeError::new(TypeErrorKind::InvalidTypeSyntax, full_expr)),
            }
        }
        
        Ok(new_ctx)
    }
    
    /// Synthesize type for begin expression
    fn synthesize_begin(
        &mut self,
        ctx: &TypeContext,
        args: ArenaIndex,
        full_expr: ArenaIndex,
    ) -> TypeResult<ArenaIndex> {
        // (begin e1 e2 ... en) - type is type of last expression
        let mut current = args;
        let mut last_type = self.nil_type()?;
        
        loop {
            let val = self.lisp.get(current)
                .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
            
            match val {
                Value::Nil => break,
                Value::Cons { car: expr, cdr: rest } => {
                    last_type = self.synthesize(ctx, expr)?;
                    current = rest;
                }
                _ => return Err(TypeError::new(TypeErrorKind::InvalidTypeSyntax, full_expr)),
            }
        }
        
        Ok(last_type)
    }
    
    /// Check an expression against an expected type (⇐)
    ///
    /// Verifies that the expression has the expected type.
    pub fn check(&mut self, ctx: &TypeContext, expr: ArenaIndex, expected: ArenaIndex) -> TypeResult<()> {
        let val = self.lisp.get(expr)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
        
        match val {
            // Lambda: check against function type
            Value::Lambda { params, body, env: _ } => {
                let expected_ty = self.get_type(expected)?;
                
                match expected_ty {
                    Type::Arrow { param: param_type, result: result_type } => {
                        // Get the lambda parameter
                        let param = self.lisp.car(params)
                            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
                        
                        // Extend context with parameter type
                        let new_ctx = ctx.extend(param, param_type)
                            .ok_or(TypeError::new(TypeErrorKind::ContextOverflow, expr))?;
                        
                        // Check body against result type
                        self.check(&new_ctx, body, result_type)
                    }
                    _ => {
                        Err(TypeError::new(TypeErrorKind::Mismatch, expr)
                            .with_types(expected, ArenaIndex::NULL))
                    }
                }
            }
            
            // If expression: check both branches against expected type
            Value::Cons { car, cdr } => {
                let head = self.lisp.get(car)
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))?;
                
                if let Value::Symbol { .. } = head {
                    if self.lisp.symbol_matches(car, "if")
                        .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))? 
                    {
                        return self.check_if(ctx, cdr, expected, expr);
                    }
                    
                    if self.lisp.symbol_matches(car, "lambda")
                        .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, expr))? 
                    {
                        return self.check_lambda(ctx, cdr, expected, expr);
                    }
                }
                
                // Subsumption: synthesize and check subtyping
                self.subsumption(ctx, expr, expected)
            }
            
            // Default: use subsumption
            _ => self.subsumption(ctx, expr, expected)
        }
    }
    
    /// Check lambda expression against function type
    fn check_lambda(
        &mut self,
        ctx: &TypeContext,
        args: ArenaIndex,
        expected: ArenaIndex,
        full_expr: ArenaIndex,
    ) -> TypeResult<()> {
        let expected_ty = self.get_type(expected)?;
        
        match expected_ty {
            Type::Arrow { param: param_type, result: result_type } => {
                // (lambda (x) body)
                let params = self.lisp.car(args)
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
                let body_rest = self.lisp.cdr(args)
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
                let body = self.lisp.car(body_rest)
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
                
                // Get parameter name
                let param = self.lisp.car(params)
                    .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
                
                // Extend context with parameter type
                let new_ctx = ctx.extend(param, param_type)
                    .ok_or(TypeError::new(TypeErrorKind::ContextOverflow, full_expr))?;
                
                // Check body against result type
                self.check(&new_ctx, body, result_type)
            }
            _ => {
                Err(TypeError::new(TypeErrorKind::Mismatch, full_expr)
                    .with_types(expected, ArenaIndex::NULL))
            }
        }
    }
    
    /// Check if expression against expected type
    fn check_if(
        &mut self,
        ctx: &TypeContext,
        args: ArenaIndex,
        expected: ArenaIndex,
        full_expr: ArenaIndex,
    ) -> TypeResult<()> {
        // (if cond then else)
        let cond = self.lisp.car(args)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        let rest = self.lisp.cdr(args)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        let then_expr = self.lisp.car(rest)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        let rest2 = self.lisp.cdr(rest)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        
        // Check condition is bool
        let bool_type = self.bool_type()?;
        self.check(ctx, cond, bool_type)?;
        
        // Check both branches against expected type
        self.check(ctx, then_expr, expected)?;
        
        let else_val = self.lisp.get(rest2)
            .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
        
        if !else_val.is_nil() {
            let else_expr = self.lisp.car(rest2)
                .map_err(|_| TypeError::new(TypeErrorKind::ArenaError, full_expr))?;
            self.check(ctx, else_expr, expected)?;
        }
        
        Ok(())
    }
    
    /// Subsumption: synthesize type and check subtyping
    ///
    /// This is the key rule that connects synthesis and checking:
    /// If we can synthesize type A for expression e, and A <: B,
    /// then e checks against B.
    fn subsumption(&mut self, ctx: &TypeContext, expr: ArenaIndex, expected: ArenaIndex) -> TypeResult<()> {
        let actual = self.synthesize(ctx, expr)?;
        
        if self.is_subtype(actual, expected)? {
            Ok(())
        } else {
            Err(TypeError::new(TypeErrorKind::Mismatch, expr)
                .with_types(expected, actual))
        }
    }
    
    // ========================================================================
    // Builtin Types
    // ========================================================================
    
    /// Get the type of a builtin function
    fn builtin_type(&self, builtin: Builtin) -> TypeResult<ArenaIndex> {
        match builtin {
            // Arithmetic: (fn isize (fn isize isize))
            Builtin::Add | Builtin::Sub | Builtin::Mul | Builtin::Div | Builtin::Mod => {
                let isize_t = self.isize_type()?;
                let binary = self.arrow_type(isize_t, isize_t)?;
                self.arrow_type(isize_t, binary)
            }
            
            // Comparison: (fn isize (fn isize bool))
            Builtin::Lt | Builtin::Gt | Builtin::Le | Builtin::Ge | Builtin::NumEq => {
                let isize_t = self.isize_type()?;
                let bool_t = self.bool_type()?;
                let result = self.arrow_type(isize_t, bool_t)?;
                self.arrow_type(isize_t, result)
            }
            
            // Boolean: (fn bool bool)
            Builtin::Not => {
                let bool_t = self.bool_type()?;
                self.arrow_type(bool_t, bool_t)
            }
            
            // Predicates return bool but take any type
            // For now, we can't type these precisely without polymorphism
            Builtin::Null | Builtin::Pairp | Builtin::Numberp | 
            Builtin::Booleanp | Builtin::Procedurep | Builtin::Symbolp | Builtin::Atom => {
                // These need polymorphism to type precisely
                // For now, we'll say they can't be synthesized
                Err(TypeError::new(TypeErrorKind::CannotSynthesize, ArenaIndex::NULL))
            }
            
            // List operations need polymorphism
            Builtin::Car | Builtin::Cdr | Builtin::Cons | Builtin::List => {
                Err(TypeError::new(TypeErrorKind::CannotSynthesize, ArenaIndex::NULL))
            }
            
            // Eq needs polymorphism
            Builtin::Eq => {
                Err(TypeError::new(TypeErrorKind::CannotSynthesize, ArenaIndex::NULL))
            }
            
            // IO operations
            Builtin::Print | Builtin::Display | Builtin::Newline => {
                // print : (fn any nil) (roughly)
                Err(TypeError::new(TypeErrorKind::CannotSynthesize, ArenaIndex::NULL))
            }
            
            // Error
            Builtin::Error => {
                Err(TypeError::new(TypeErrorKind::CannotSynthesize, ArenaIndex::NULL))
            }
            
            // Other builtins that need polymorphism or special handling
            _ => {
                Err(TypeError::new(TypeErrorKind::CannotSynthesize, ArenaIndex::NULL))
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    // Helper to create test infrastructure
    fn setup() -> (Lisp<1024>, Arena<Type, 256>) {
        let lisp = Lisp::new();
        let types = Arena::new(Type::Nil);
        (lisp, types)
    }
    
    #[test]
    fn test_parse_base_types() {
        let (lisp, types) = setup();
        let checker = TypeChecker::new(&lisp, &types);
        
        // Parse isize
        let isize_expr = lisp.symbol("isize").unwrap();
        let isize_type = checker.parse_type(isize_expr).unwrap();
        assert_eq!(types.get(isize_type).unwrap(), Type::Isize);
        
        // Parse bool
        let bool_expr = lisp.symbol("bool").unwrap();
        let bool_type = checker.parse_type(bool_expr).unwrap();
        assert_eq!(types.get(bool_type).unwrap(), Type::Bool);
        
        // Parse nil
        let nil_expr = lisp.symbol("nil").unwrap();
        let nil_type = checker.parse_type(nil_expr).unwrap();
        assert_eq!(types.get(nil_type).unwrap(), Type::Nil);
        
        // Parse char
        let char_expr = lisp.symbol("char").unwrap();
        let char_type = checker.parse_type(char_expr).unwrap();
        assert_eq!(types.get(char_type).unwrap(), Type::Char);
    }
    
    #[test]
    fn test_parse_arrow_type() {
        let (lisp, types) = setup();
        let checker = TypeChecker::new(&lisp, &types);
        
        // Parse (fn isize isize)
        let arrow_sym = lisp.symbol("fn").unwrap();
        let isize_sym = lisp.symbol("isize").unwrap();
        let isize_sym2 = lisp.symbol("isize").unwrap();
        let list = lisp.list([arrow_sym, isize_sym, isize_sym2]).unwrap();
        
        let arrow_type = checker.parse_type(list).unwrap();
        let ty = types.get(arrow_type).unwrap();
        
        match ty {
            Type::Arrow { param, result } => {
                assert_eq!(types.get(param).unwrap(), Type::Isize);
                assert_eq!(types.get(result).unwrap(), Type::Isize);
            }
            _ => panic!("Expected Arrow type"),
        }
    }
    
    #[test]
    fn test_parse_list_type() {
        let (lisp, types) = setup();
        let checker = TypeChecker::new(&lisp, &types);
        
        // Parse (list isize)
        let list_sym = lisp.symbol("list").unwrap();
        let isize_sym = lisp.symbol("isize").unwrap();
        let list_expr = lisp.list([list_sym, isize_sym]).unwrap();
        
        let list_type = checker.parse_type(list_expr).unwrap();
        let ty = types.get(list_type).unwrap();
        
        match ty {
            Type::List { element } => {
                assert_eq!(types.get(element).unwrap(), Type::Isize);
            }
            _ => panic!("Expected List type"),
        }
    }
    
    #[test]
    fn test_parse_pair_type() {
        let (lisp, types) = setup();
        let checker = TypeChecker::new(&lisp, &types);
        
        // Parse (pair isize bool)
        let pair_sym = lisp.symbol("pair").unwrap();
        let isize_sym = lisp.symbol("isize").unwrap();
        let bool_sym = lisp.symbol("bool").unwrap();
        let pair_expr = lisp.list([pair_sym, isize_sym, bool_sym]).unwrap();
        
        let pair_type = checker.parse_type(pair_expr).unwrap();
        let ty = types.get(pair_type).unwrap();
        
        match ty {
            Type::Pair { first, second } => {
                assert_eq!(types.get(first).unwrap(), Type::Isize);
                assert_eq!(types.get(second).unwrap(), Type::Bool);
            }
            _ => panic!("Expected Pair type"),
        }
    }
    
    #[test]
    fn test_synthesize_literals() {
        let (lisp, types) = setup();
        let mut checker = TypeChecker::new(&lisp, &types);
        let ctx = TypeContext::new();
        
        // Numbers synthesize to isize
        let num = lisp.number(42).unwrap();
        let num_type = checker.synthesize(&ctx, num).unwrap();
        assert_eq!(types.get(num_type).unwrap(), Type::Isize);
        
        // Booleans synthesize to bool
        let t = lisp.true_val().unwrap();
        let t_type = checker.synthesize(&ctx, t).unwrap();
        assert_eq!(types.get(t_type).unwrap(), Type::Bool);
        
        let f = lisp.false_val().unwrap();
        let f_type = checker.synthesize(&ctx, f).unwrap();
        assert_eq!(types.get(f_type).unwrap(), Type::Bool);
        
        // Nil synthesizes to nil
        let n = lisp.nil().unwrap();
        let n_type = checker.synthesize(&ctx, n).unwrap();
        assert_eq!(types.get(n_type).unwrap(), Type::Nil);
    }
    
    #[test]
    fn test_synthesize_variable() {
        let (lisp, types) = setup();
        let mut checker = TypeChecker::new(&lisp, &types);
        
        // Create context with x : isize
        let x = lisp.symbol("x").unwrap();
        let isize_type = checker.isize_type().unwrap();
        let ctx = TypeContext::new().extend(x, isize_type).unwrap();
        
        // Synthesize x
        let x_type = checker.synthesize(&ctx, x).unwrap();
        assert_eq!(types.get(x_type).unwrap(), Type::Isize);
    }
    
    #[test]
    fn test_check_lambda() {
        let (lisp, types) = setup();
        let mut checker = TypeChecker::new(&lisp, &types);
        let ctx = TypeContext::new();
        
        // Create (lambda (x) x) - identity function
        let x = lisp.symbol("x").unwrap();
        let params = lisp.list([x]).unwrap();
        let nil = lisp.nil().unwrap();
        let lambda = lisp.lambda(params, x, nil).unwrap();
        
        // Create type (fn isize isize)
        let isize_type = checker.isize_type().unwrap();
        let arrow_type = checker.arrow_type(isize_type, isize_type).unwrap();
        
        // Check lambda against function type
        assert!(checker.check(&ctx, lambda, arrow_type).is_ok());
    }
    
    #[test]
    fn test_type_annotation() {
        let (lisp, types) = setup();
        let mut checker = TypeChecker::new(&lisp, &types);
        let ctx = TypeContext::new();
        
        // Create (: 42 isize)
        let colon = lisp.symbol(":").unwrap();
        let num = lisp.number(42).unwrap();
        let isize_sym = lisp.symbol("isize").unwrap();
        let annotated = lisp.list([colon, num, isize_sym]).unwrap();
        
        // Synthesize type
        let result_type = checker.synthesize(&ctx, annotated).unwrap();
        assert_eq!(types.get(result_type).unwrap(), Type::Isize);
    }
    
    #[test]
    fn test_type_mismatch() {
        let (lisp, types) = setup();
        let mut checker = TypeChecker::new(&lisp, &types);
        let ctx = TypeContext::new();
        
        // Try to check 42 against bool
        let num = lisp.number(42).unwrap();
        let bool_type = checker.bool_type().unwrap();
        
        let result = checker.check(&ctx, num, bool_type);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, TypeErrorKind::Mismatch);
    }
    
    #[test]
    fn test_types_equal() {
        let (_lisp, types) = setup();
        let checker = TypeChecker::new(&_lisp, &types);
        
        let isize1 = checker.isize_type().unwrap();
        let isize2 = checker.isize_type().unwrap();
        let bool_t = checker.bool_type().unwrap();
        
        assert!(checker.types_equal(isize1, isize2).unwrap());
        assert!(!checker.types_equal(isize1, bool_t).unwrap());
        
        // Arrow types
        let arrow1 = checker.arrow_type(isize1, bool_t).unwrap();
        let arrow2 = checker.arrow_type(isize2, bool_t).unwrap();
        assert!(checker.types_equal(arrow1, arrow2).unwrap());
    }
    
    #[test]
    fn test_parse_multi_param_fn_type() {
        let (lisp, types) = setup();
        let checker = TypeChecker::new(&lisp, &types);
        
        // Parse (fn (isize isize) isize) - binary function type
        let fn_sym = lisp.symbol("fn").unwrap();
        let isize_sym1 = lisp.symbol("isize").unwrap();
        let isize_sym2 = lisp.symbol("isize").unwrap();
        let isize_sym3 = lisp.symbol("isize").unwrap();
        let params = lisp.list([isize_sym1, isize_sym2]).unwrap();
        let fn_type_expr = lisp.list([fn_sym, params, isize_sym3]).unwrap();
        
        let fn_type = checker.parse_type(fn_type_expr).unwrap();
        let ty = types.get(fn_type).unwrap();
        
        // Should be isize -> (isize -> isize)
        match ty {
            Type::Arrow { param, result } => {
                assert_eq!(types.get(param).unwrap(), Type::Isize);
                // result should be another arrow
                let result_ty = types.get(result).unwrap();
                match result_ty {
                    Type::Arrow { param: p2, result: r2 } => {
                        assert_eq!(types.get(p2).unwrap(), Type::Isize);
                        assert_eq!(types.get(r2).unwrap(), Type::Isize);
                    }
                    _ => panic!("Expected nested Arrow type"),
                }
            }
            _ => panic!("Expected Arrow type"),
        }
    }
    
    #[test]
    fn test_parse_forall_type() {
        let (lisp, types) = setup();
        let checker = TypeChecker::new(&lisp, &types);
        
        // Parse (forall (a) (fn a a)) - polymorphic identity type
        let forall_sym = lisp.symbol("forall").unwrap();
        let a_sym = lisp.symbol("a").unwrap();
        let fn_sym = lisp.symbol("fn").unwrap();
        let a_sym2 = lisp.symbol("a").unwrap();
        let a_sym3 = lisp.symbol("a").unwrap();
        
        let vars = lisp.list([a_sym]).unwrap();
        let body = lisp.list([fn_sym, a_sym2, a_sym3]).unwrap();
        let forall_expr = lisp.list([forall_sym, vars, body]).unwrap();
        
        let forall_type = checker.parse_type(forall_expr).unwrap();
        let ty = types.get(forall_type).unwrap();
        
        match ty {
            Type::ForAll { type_vars, body } => {
                // type_vars should be (a)
                assert!(!type_vars.is_null());
                // body should be Arrow { Var(0), Var(0) }
                let body_ty = types.get(body).unwrap();
                match body_ty {
                    Type::Arrow { param, result } => {
                        assert_eq!(types.get(param).unwrap(), Type::Var(0));
                        assert_eq!(types.get(result).unwrap(), Type::Var(0));
                    }
                    _ => panic!("Expected Arrow type in forall body"),
                }
            }
            _ => panic!("Expected ForAll type"),
        }
    }
    
    #[test]
    fn test_parse_forall_multi_vars() {
        let (lisp, types) = setup();
        let checker = TypeChecker::new(&lisp, &types);
        
        // Parse (forall (a b) (fn a b)) - function from a to b
        let forall_sym = lisp.symbol("forall").unwrap();
        let a_sym = lisp.symbol("a").unwrap();
        let b_sym = lisp.symbol("b").unwrap();
        let fn_sym = lisp.symbol("fn").unwrap();
        let a_sym2 = lisp.symbol("a").unwrap();
        let b_sym2 = lisp.symbol("b").unwrap();
        
        let vars = lisp.list([a_sym, b_sym]).unwrap();
        let body = lisp.list([fn_sym, a_sym2, b_sym2]).unwrap();
        let forall_expr = lisp.list([forall_sym, vars, body]).unwrap();
        
        let forall_type = checker.parse_type(forall_expr).unwrap();
        let ty = types.get(forall_type).unwrap();
        
        match ty {
            Type::ForAll { body, .. } => {
                let body_ty = types.get(body).unwrap();
                match body_ty {
                    Type::Arrow { param, result } => {
                        // a is first (Var(0)), b is second (Var(1))
                        assert_eq!(types.get(param).unwrap(), Type::Var(0));
                        assert_eq!(types.get(result).unwrap(), Type::Var(1));
                    }
                    _ => panic!("Expected Arrow type in forall body"),
                }
            }
            _ => panic!("Expected ForAll type"),
        }
    }
}
