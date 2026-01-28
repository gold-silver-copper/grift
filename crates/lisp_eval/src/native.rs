//! # Builtin Function Support
//!
//! This module provides traits and macros for defining Lisp builtin functions in Rust.
//!
//! ## Overview
//!
//! The builtin function system allows you to:
//! - Define Rust functions that can be called from Lisp as builtins
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
//! use lisp_eval::register_native;
//!
//! // Define a builtin function using the register_native! macro
//! register_native!(add_one, (x: isize) -> isize, { x + 1 });
//!
//! // The function can now be used as a builtin implementation
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
// Builtin Function Type
// ============================================================================

/// A builtin function that can be called from Lisp.
///
/// Builtin functions receive:
/// - A reference to the Lisp context
/// - The argument list as an ArenaIndex (a Lisp list)
///
/// They return an ArenaResult<ArenaIndex> containing the result.
pub type NativeFn<const N: usize> = fn(&Lisp<N>, ArenaIndex) -> ArenaResult<ArenaIndex>;

// ============================================================================
// Helper Functions for Builtin Functions
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
// Mock Hardware Storage (for embedded builtins)
// ============================================================================

use core::sync::atomic::{AtomicUsize, Ordering};

/// Number of 32-bit words in mock memory (256 words = 1KB)
pub const MOCK_MEMORY_WORDS: usize = 256;

/// Number of GPIO registers
pub const MOCK_GPIO_COUNT: usize = 16;

/// Simulated memory for testing (256 x word-size units = 1KB on 32-bit)
/// Using AtomicUsize for thread-safe access without unsafe code
pub static MOCK_MEMORY: [AtomicUsize; MOCK_MEMORY_WORDS] = {
    const INIT: AtomicUsize = AtomicUsize::new(0);
    [INIT; MOCK_MEMORY_WORDS]
};

/// Simulated GPIO registers (16 registers, word-size each)
pub static MOCK_GPIO: [AtomicUsize; MOCK_GPIO_COUNT] = {
    const INIT: AtomicUsize = AtomicUsize::new(0);
    [INIT; MOCK_GPIO_COUNT]
};

/// Reset mock memory and GPIO registers to zero.
///
/// This is useful for testing to ensure a clean state.
/// Uses SeqCst ordering to ensure all resets are visible across threads.
pub fn reset_mock_hardware() {
    for mem in MOCK_MEMORY.iter() {
        mem.store(0, Ordering::SeqCst);
    }
    for gpio in MOCK_GPIO.iter() {
        gpio.store(0, Ordering::SeqCst);
    }
}

// ============================================================================
// Macro for Builtin Function Definition
// ============================================================================

/// Register a native Rust function as a Lisp builtin with automatic argument extraction.
///
/// This macro generates wrapper code that extracts typed arguments from
/// the Lisp argument list and calls your function. It transforms any Rust
/// function into a Lisp builtin that can be called from Lisp code.
///
/// # Basic Syntax
///
/// ```rust
/// use lisp_eval::register_native;
///
/// // Define a function that adds two numbers
/// register_native!(add_two, (a: isize, b: isize) -> isize, {
///     a + b
/// });
///
/// // Define a function with no return value
/// register_native!(print_num, (n: isize) -> (), {
///     // In a real impl, you'd print n
///     ()
/// });
/// ```
///
/// # Stateful Functions
///
/// The macro can also be used for functions that access global static variables:
///
/// ```rust
/// use lisp_eval::register_native;
/// use core::sync::atomic::{AtomicUsize, Ordering};
///
/// static MY_COUNTER: AtomicUsize = AtomicUsize::new(0);
///
/// register_native!(
///     native_increment,
///     () -> isize,
///     {
///         MY_COUNTER.fetch_add(1, Ordering::Relaxed) as isize
///     }
/// );
/// ```
///
/// # Limitations: Lisp Context Access
///
/// Due to Rust macro hygiene, the `lisp` and `args` identifiers are NOT directly
/// accessible inside the macro body. The macro is designed for simple functions
/// that work with extracted typed arguments and return simple types.
///
/// For functions that need full access to the Lisp context (creating values,
/// processing variadic arguments, etc.), define a regular function instead:
///
/// ```rust
/// use lisp_eval::{Lisp, ArenaIndex, ArenaResult, FromLisp};
/// use pwn_arena::ArenaResult as PwnResult;
///
/// fn my_custom_fn<const N: usize>(
///     lisp: &Lisp<N>,
///     args: ArenaIndex,
/// ) -> ArenaResult<ArenaIndex> {
///     // Full access to lisp and args
///     let a = isize::from_lisp(lisp, lisp.car(args)?)?;
///     let rest = lisp.cdr(args)?;
///     let b = isize::from_lisp(lisp, lisp.car(rest)?)?;
///     lisp.cons(lisp.number(a)?, lisp.number(b)?)
/// }
/// ```
///
/// # @with_lisp Variant
///
/// The `@with_lisp` variant allows the extracted arguments to shadow the `args`
/// variable, leaving the remaining argument list available.
///
/// # Generated Code
///
/// The macro generates a function with signature:
/// `fn name<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex>`
#[macro_export]
macro_rules! register_native {
    // ========================================================================
    // Standard variants - access statics directly in the body
    // ========================================================================

    // No arguments
    ($name:ident, () -> $ret:ty, $body:block) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            _args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let _ = lisp;
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
    // @with_lisp variants - provide access to 'lisp' and 'args' in the 
    // function body for complex operations requiring the Lisp context.
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