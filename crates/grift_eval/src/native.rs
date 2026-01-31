//! # Native Function Interop
//!
//! This module provides traits and macros for calling Rust functions from Lisp code.
//!
//! ## Overview
//!
//! The native function interop system allows you to:
//! - Register Rust functions that can be called from Lisp
//! - Automatically convert Lisp values to Rust types and back
//! - Handle errors gracefully
//!
//! ## Key Traits
//!
//! - [`FromLisp`] - Convert a Lisp value to a Rust type
//! - [`ToLisp`] - Convert a Rust type to a Lisp value
//!
//! ## Usage
//!
//! ```rust
//! use grift_eval::{NativeRegistry, register_native};
//!
//! // Define a native function using the register_native! macro
//! register_native!(add_one, (x: isize) -> isize, { x + 1 });
//!
//! // Register it with an evaluator
//! // eval.register_native("add-one", add_one).unwrap();
//! ```
//!
//! ## Design Notes
//!
//! This module is `no_std` compatible and uses no heap allocation.
//! All conversions work directly with arena-allocated values.

use crate::{ArenaIndex, ArenaResult, ArenaError, Lisp, Value};

// ============================================================================
// Conversion Traits
// ============================================================================

/// Trait for converting Lisp values to Rust types.
///
/// Implement this trait for any type you want to extract from Lisp arguments.
///
/// # Example
///
/// ```rust
/// use grift_eval::{FromLisp, Lisp, ArenaIndex, ArenaResult, Value};
///
/// // isize is already implemented
/// fn example<const N: usize>(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<isize> {
///     isize::from_lisp(lisp, idx)
/// }
/// ```
pub trait FromLisp<const N: usize>: Sized {
    /// Convert a Lisp value at the given index to this Rust type.
    ///
    /// Returns an error if the conversion fails (e.g., type mismatch).
    fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self>;
}

/// Trait for converting Rust types to Lisp values.
///
/// Implement this trait for any type you want to return to Lisp code.
///
/// # Example
///
/// ```rust
/// use grift_eval::{ToLisp, Lisp, ArenaIndex, ArenaResult};
///
/// // isize is already implemented
/// fn example<const N: usize>(lisp: &Lisp<N>, value: isize) -> ArenaResult<ArenaIndex> {
///     value.to_lisp(lisp)
/// }
/// ```
pub trait ToLisp<const N: usize> {
    /// Convert this Rust value to a Lisp value, allocating in the arena.
    ///
    /// Returns the ArenaIndex of the newly allocated value.
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex>;
}

// ============================================================================
// Implementations for Common Types
// ============================================================================

impl<const N: usize> FromLisp<N> for isize {
    fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self> {
        match lisp.get(idx)? {
            Value::Number(n) => Ok(n),
            // Note: Using InvalidIndex for type errors is semantically imprecise,
            // but ArenaError doesn't have a TypeError variant and adding one
            // would require changes to the core no_std crate.
            _ => Err(ArenaError::InvalidIndex),
        }
    }
}

impl<const N: usize> ToLisp<N> for isize {
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        lisp.number(*self)
    }
}

impl<const N: usize> FromLisp<N> for bool {
    fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self> {
        match lisp.get(idx)? {
            Value::True => Ok(true),
            Value::False => Ok(false),
            // In Lisp, only #f is false; everything else is truthy
            _ => Ok(true),
        }
    }
}

impl<const N: usize> ToLisp<N> for bool {
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        lisp.boolean(*self)
    }
}

impl<const N: usize> FromLisp<N> for () {
    fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self> {
        // Only accept nil as unit - other types are an error
        match lisp.get(idx)? {
            Value::Nil => Ok(()),
            // Note: Using InvalidIndex for type errors (see isize impl for rationale)
            _ => Err(ArenaError::InvalidIndex),
        }
    }
}

impl<const N: usize> ToLisp<N> for () {
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        lisp.nil()
    }
}

impl<const N: usize> FromLisp<N> for char {
    fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self> {
        match lisp.get(idx)? {
            Value::Char(c) => Ok(c),
            // Note: Using InvalidIndex for type errors (see isize impl for rationale)
            _ => Err(ArenaError::InvalidIndex),
        }
    }
}

impl<const N: usize> ToLisp<N> for char {
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        lisp.char(*self)
    }
}

/// ArenaIndex can be passed through directly (for when you want raw Lisp values)
impl<const N: usize> FromLisp<N> for ArenaIndex {
    fn from_lisp(_lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self> {
        Ok(idx)
    }
}

impl<const N: usize> ToLisp<N> for ArenaIndex {
    fn to_lisp(&self, _lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        Ok(*self)
    }
}

// ============================================================================
// Native Function Registry
// ============================================================================

/// Maximum number of native functions that can be registered.
pub const MAX_NATIVE_FUNCTIONS: usize = 64;

/// A native function that can be called from Lisp.
///
/// Native functions receive:
/// - A reference to the Lisp context
/// - The argument list as an ArenaIndex (a Lisp list)
///
/// They return an ArenaResult<ArenaIndex> containing the result.
pub type NativeFn<const N: usize> = fn(&Lisp<N>, ArenaIndex) -> ArenaResult<ArenaIndex>;

/// A registered native function with its name.
#[derive(Clone, Copy)]
pub struct NativeEntry<const N: usize> {
    /// The name used to call this function from Lisp
    pub name: &'static str,
    /// The Rust function to call
    pub func: NativeFn<N>,
}

/// Registry for native functions.
///
/// This struct holds the registered native functions and provides
/// lookup functionality for the evaluator.
///
/// # Example
///
/// ```rust
/// use grift_eval::{NativeRegistry, Lisp, ArenaIndex, ArenaResult};
///
/// fn my_add<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
///     use grift_eval::FromLisp;
///     let a = isize::from_lisp(lisp, lisp.car(args)?)?;
///     let b = isize::from_lisp(lisp, lisp.car(lisp.cdr(args)?)?)?;
///     lisp.number(a + b)
/// }
///
/// let mut registry: NativeRegistry<1000> = NativeRegistry::new();
/// registry.register("my-add", my_add);
/// ```
pub struct NativeRegistry<const N: usize> {
    entries: [Option<NativeEntry<N>>; MAX_NATIVE_FUNCTIONS],
    count: usize,
}

impl<const N: usize> NativeRegistry<N> {
    /// Create a new empty native function registry.
    pub const fn new() -> Self {
        NativeRegistry {
            entries: [None; MAX_NATIVE_FUNCTIONS],
            count: 0,
        }
    }

    /// Register a native function.
    ///
    /// # Panics
    ///
    /// Panics if the registry is full (MAX_NATIVE_FUNCTIONS exceeded).
    pub fn register(&mut self, name: &'static str, func: NativeFn<N>) {
        assert!(self.count < MAX_NATIVE_FUNCTIONS, "Native function registry is full");
        self.entries[self.count] = Some(NativeEntry { name, func });
        self.count += 1;
    }

    /// Look up a native function by name.
    ///
    /// Returns None if the function is not registered.
    pub fn lookup(&self, name: &str) -> Option<NativeFn<N>> {
        for entry in &self.entries[..self.count] {
            if let Some(e) = entry
                && e.name == name
            {
                return Some(e.func);
            }
        }
        None
    }

    /// Get all registered function names.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.entries[..self.count]
            .iter()
            .filter_map(|e| e.as_ref().map(|e| e.name))
    }

    /// Get the number of registered functions.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    
    /// Look up a native function by its ID (index).
    ///
    /// Returns None if the ID is out of range.
    pub fn lookup_by_id(&self, id: usize) -> Option<NativeFn<N>> {
        if id < self.count {
            self.entries[id].map(|e| e.func)
        } else {
            None
        }
    }
    
    /// Get the name of a native function by its ID.
    pub fn name_by_id(&self, id: usize) -> Option<&'static str> {
        if id < self.count {
            self.entries[id].map(|e| e.name)
        } else {
            None
        }
    }
}

impl<const N: usize> Default for NativeRegistry<N> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Hash Function
// ============================================================================

/// Simple hash function for native function names.
///
/// Uses a simple djb2-like hash that's fast and produces good distribution.
/// This is used for verification during native function calls.
pub const fn simple_hash(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut hash: usize = 5381;
    let mut i = 0;
    while i < bytes.len() {
        hash = hash.wrapping_mul(33).wrapping_add(bytes[i] as usize);
        i += 1;
    }
    hash
}

// ============================================================================
// Helper Functions for Native Functions
// ============================================================================

/// Extract a single argument from a Lisp argument list.
///
/// # Example
///
/// ```rust
/// use grift_eval::{extract_arg, Lisp, ArenaIndex, ArenaResult};
///
/// fn example<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<(isize, ArenaIndex)> {
///     extract_arg::<N, isize>(lisp, args)
/// }
/// ```
pub fn extract_arg<const N: usize, T: FromLisp<N>>(
    lisp: &Lisp<N>,
    args: ArenaIndex,
) -> ArenaResult<(T, ArenaIndex)> {
    let head = lisp.car(args)?;
    let tail = lisp.cdr(args)?;
    let value = T::from_lisp(lisp, head)?;
    Ok((value, tail))
}

/// Check if the argument list is empty (nil).
pub fn args_empty<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<bool> {
    Ok(lisp.get(args)?.is_nil())
}

/// Count the number of arguments in a list.
///
/// Returns an error if the argument is not a proper list.
/// Note: Uses InvalidIndex for type errors (see FromLisp impls for rationale).
pub fn count_args<const N: usize>(lisp: &Lisp<N>, mut args: ArenaIndex) -> ArenaResult<usize> {
    let mut count = 0;
    loop {
        match lisp.get(args)? {
            Value::Nil => return Ok(count),
            Value::Cons { .. } => {
                count += 1;
                args = lisp.cdr(args)?;
            }
            // Not a proper list
            _ => return Err(ArenaError::InvalidIndex),
        }
    }
}

// Note: The register_native! macro has been moved to src/macros.rs