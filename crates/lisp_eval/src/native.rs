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
//! use lisp_eval::{NativeRegistry, define_native};
//!
//! // Define a simple native function
//! fn add_one(x: isize) -> isize {
//!     x + 1
//! }
//!
//! // Register it (in your evaluator setup)
//! // registry.register("add-one", |lisp, args| { ... });
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
/// use lisp_eval::{FromLisp, Lisp, ArenaIndex, ArenaResult, Value};
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
/// use lisp_eval::{ToLisp, Lisp, ArenaIndex, ArenaResult};
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
        // Accept nil as unit
        match lisp.get(idx)? {
            Value::Nil => Ok(()),
            _ => Ok(()), // Accept anything, just ignore
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
/// use lisp_eval::{NativeRegistry, Lisp, ArenaIndex, ArenaResult};
///
/// fn my_add<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
///     use lisp_eval::FromLisp;
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
            if let Some(e) = entry {
                if e.name == name {
                    return Some(e.func);
                }
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
/// use lisp_eval::{extract_arg, Lisp, ArenaIndex, ArenaResult};
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
            Value::Cons { cdr, .. } => {
                count += 1;
                args = cdr;
            }
            // Not a proper list
            _ => return Err(ArenaError::InvalidIndex),
        }
    }
}

// ============================================================================
// Macro for Easy Native Function Definition
// ============================================================================

/// Define a native function with automatic argument extraction.
///
/// This macro generates wrapper code that extracts typed arguments from
/// the Lisp argument list and calls your function.
///
/// # Basic Syntax
///
/// ```rust
/// use lisp_eval::define_native;
///
/// // Define a function that adds two numbers
/// define_native!(add_two, (a: isize, b: isize) -> isize, {
///     a + b
/// });
///
/// // Define a function with no return value
/// define_native!(print_num, (n: isize) -> (), {
///     // In a real impl, you'd print n
///     ()
/// });
/// ```
///
/// # With Lisp Context Access
///
/// For complex functions that need access to the Lisp context (for allocation,
/// creating values, etc.) or remaining arguments, use the `@with_lisp` variant.
/// The `lisp` and `args` variables are available in the body:
///
/// - `lisp`: Reference to the Lisp context for allocating values, calling methods
/// - `args`: The remaining argument list (ArenaIndex) after extracting typed args
///
/// Note: The `@with_lisp` variant is mainly useful when you need to process
/// variadic arguments or access the remaining argument list after typed extraction.
/// For most simple functions, the standard variant is sufficient.
///
/// # Generated Code
///
/// The macro generates a function with signature:
/// `fn name<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex>`
#[macro_export]
macro_rules! define_native {
    // ========================================================================
    // Standard variants (body does not access lisp directly)
    // ========================================================================

    // No arguments, with return type
    ($name:ident, () -> $ret:ty, $body:block) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            _args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let _ = lisp; // suppress unused warning when lisp not used in body
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Single argument
    ($name:ident, ($arg1:ident : $ty1:ty) -> $ret:ty, $body:block) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let ($arg1, _rest): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Two arguments
    ($name:ident, ($arg1:ident : $ty1:ty, $arg2:ident : $ty2:ty) -> $ret:ty, $body:block) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let ($arg1, rest): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let ($arg2, _rest): ($ty2, _) = $crate::extract_arg(lisp, rest)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Three arguments
    ($name:ident, ($arg1:ident : $ty1:ty, $arg2:ident : $ty2:ty, $arg3:ident : $ty3:ty) -> $ret:ty, $body:block) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let ($arg1, rest): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let ($arg2, rest): ($ty2, _) = $crate::extract_arg(lisp, rest)?;
            let ($arg3, _rest): ($ty3, _) = $crate::extract_arg(lisp, rest)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Four arguments
    ($name:ident, ($arg1:ident : $ty1:ty, $arg2:ident : $ty2:ty, $arg3:ident : $ty3:ty, $arg4:ident : $ty4:ty) -> $ret:ty, $body:block) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let ($arg1, rest): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let ($arg2, rest): ($ty2, _) = $crate::extract_arg(lisp, rest)?;
            let ($arg3, rest): ($ty3, _) = $crate::extract_arg(lisp, rest)?;
            let ($arg4, _rest): ($ty4, _) = $crate::extract_arg(lisp, rest)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // ========================================================================
    // @with_lisp variants (body can access lisp context and args directly)
    // These provide access to 'lisp' and 'args' for complex operations
    // ========================================================================

    // No arguments, with lisp access
    ($name:ident @with_lisp, () -> $ret:ty, $body:block) => {
        #[allow(unused_variables)]
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Single argument, with lisp access
    ($name:ident @with_lisp, ($arg1:ident : $ty1:ty) -> $ret:ty, $body:block) => {
        #[allow(unused_variables)]
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let ($arg1, args): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Two arguments, with lisp access
    ($name:ident @with_lisp, ($arg1:ident : $ty1:ty, $arg2:ident : $ty2:ty) -> $ret:ty, $body:block) => {
        #[allow(unused_variables)]
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let ($arg1, args): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let ($arg2, args): ($ty2, _) = $crate::extract_arg(lisp, args)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Three arguments, with lisp access
    ($name:ident @with_lisp, ($arg1:ident : $ty1:ty, $arg2:ident : $ty2:ty, $arg3:ident : $ty3:ty) -> $ret:ty, $body:block) => {
        #[allow(unused_variables)]
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let ($arg1, args): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let ($arg2, args): ($ty2, _) = $crate::extract_arg(lisp, args)?;
            let ($arg3, args): ($ty3, _) = $crate::extract_arg(lisp, args)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Four arguments, with lisp access
    ($name:ident @with_lisp, ($arg1:ident : $ty1:ty, $arg2:ident : $ty2:ty, $arg3:ident : $ty3:ty, $arg4:ident : $ty4:ty) -> $ret:ty, $body:block) => {
        #[allow(unused_variables)]
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let ($arg1, args): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let ($arg2, args): ($ty2, _) = $crate::extract_arg(lisp, args)?;
            let ($arg3, args): ($ty3, _) = $crate::extract_arg(lisp, args)?;
            let ($arg4, args): ($ty4, _) = $crate::extract_arg(lisp, args)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };
}

/// Define a native function with automatic argument extraction and static variable access.
///
/// This macro extends `define_native!` to indicate that the function accesses
/// a static variable. The static variable name is passed to the macro for documentation
/// and compile-time verification, but the body should reference the static directly.
///
/// # Syntax
///
/// ```rust
/// use lisp_eval::define_native_stateful;
/// use core::sync::atomic::{AtomicUsize, Ordering};
///
/// // Define a static variable
/// static MY_COUNTER: AtomicUsize = AtomicUsize::new(0);
///
/// // Define a function that accesses the static
/// define_native_stateful!(
///     native_increment,
///     static: MY_COUNTER,
///     () -> isize,
///     {
///         MY_COUNTER.fetch_add(1, Ordering::Relaxed) as isize
///     }
/// );
/// ```
///
/// # Generated Code
///
/// The macro generates a function with signature:
/// `fn name<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex>`
///
/// The body has direct access to the named static variable.
#[macro_export]
macro_rules! define_native_stateful {
    // ========================================================================
    // Stateful variants with static variable access
    // ========================================================================

    // No arguments
    ($name:ident, static: $static_name:ident, () -> $ret:ty, $body:tt) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            _args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let _ = &$static_name; // verify static exists at compile time
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Single argument
    ($name:ident, static: $static_name:ident, ($arg1:ident : $ty1:ty) -> $ret:ty, $body:tt) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let _ = &$static_name; // verify static exists at compile time
            let ($arg1, _rest): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Two arguments
    ($name:ident, static: $static_name:ident, ($arg1:ident : $ty1:ty, $arg2:ident : $ty2:ty) -> $ret:ty, $body:tt) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let _ = &$static_name; // verify static exists at compile time
            let ($arg1, rest): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let ($arg2, _rest): ($ty2, _) = $crate::extract_arg(lisp, rest)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Three arguments
    ($name:ident, static: $static_name:ident, ($arg1:ident : $ty1:ty, $arg2:ident : $ty2:ty, $arg3:ident : $ty3:ty) -> $ret:ty, $body:tt) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let _ = &$static_name; // verify static exists at compile time
            let ($arg1, rest): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let ($arg2, rest): ($ty2, _) = $crate::extract_arg(lisp, rest)?;
            let ($arg3, _rest): ($ty3, _) = $crate::extract_arg(lisp, rest)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // Four arguments
    ($name:ident, static: $static_name:ident, ($arg1:ident : $ty1:ty, $arg2:ident : $ty2:ty, $arg3:ident : $ty3:ty, $arg4:ident : $ty4:ty) -> $ret:ty, $body:tt) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let _ = &$static_name; // verify static exists at compile time
            let ($arg1, rest): ($ty1, _) = $crate::extract_arg(lisp, args)?;
            let ($arg2, rest): ($ty2, _) = $crate::extract_arg(lisp, rest)?;
            let ($arg3, rest): ($ty3, _) = $crate::extract_arg(lisp, rest)?;
            let ($arg4, _rest): ($ty4, _) = $crate::extract_arg(lisp, rest)?;
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };
}