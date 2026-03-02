//! Native function registration for user-defined Rust functions.
//!
//! This module provides the infrastructure for registering custom Rust
//! functions as callable Lisp applicatives. Native functions receive
//! already-evaluated arguments as a cons-list and return an arena-allocated
//! result.
//!
//! ## Overview
//!
//! - [`NativeFn`] — type alias for the native function pointer signature.
//! - [`FromLisp`] / [`ToLisp`] — conversion traits between Rust and Lisp types.
//! - [`extract_arg`] — extract a typed argument from an argument list.
//! - [`register_native!`] — macro for ergonomic native function definition.
//!
//! ## Example
//!
//! ```rust
//! use grift::{Lisp, Value, register_native};
//!
//! // Define a native function using the macro
//! register_native!(native_add3, (a: isize, b: isize, c: isize) -> isize, {
//!     a + b + c
//! });
//!
//! let lisp: Lisp<20000> = Lisp::new();
//! lisp.register_native("add3", native_add3).unwrap();
//! assert_eq!(lisp.eval("(add3 1 2 3)"), Ok(Value::Number(6)));
//! ```

use grift_arena::{ArenaError, ArenaIndex, ArenaResult};

use crate::lisp::Lisp;
use crate::value::Value;

/// Maximum number of native functions that can be registered.
pub const MAX_NATIVE_FNS: usize = 64;

/// Type alias for native function pointers.
///
/// A native function receives the [`Lisp`] interpreter and a cons-list of
/// already-evaluated arguments, and returns an arena-allocated result.
pub type NativeFn<const N: usize> = fn(&Lisp<N>, ArenaIndex) -> ArenaResult<ArenaIndex>;

/// Trait for converting a Lisp value to a Rust type.
///
/// Implemented for common types (`isize`, `bool`, `ArenaIndex`).
/// Users can implement this trait for custom types.
pub trait FromLisp<const N: usize>: Sized {
    /// Convert a Lisp value at the given arena index to this Rust type.
    fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self>;
}

/// Trait for converting a Rust value to a Lisp arena value.
///
/// Implemented for common types (`isize`, `bool`, `ArenaIndex`, `()`).
/// Users can implement this trait for custom types.
pub trait ToLisp<const N: usize> {
    /// Allocate this value in the Lisp arena and return its index.
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex>;
}

// ============================================================================
// FromLisp implementations
// ============================================================================

impl<const N: usize> FromLisp<N> for isize {
    #[inline]
    fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self> {
        lisp.get(idx)?.as_number()
    }
}

impl<const N: usize> FromLisp<N> for bool {
    #[inline]
    fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self> {
        lisp.get(idx)?.as_bool()
    }
}

impl<const N: usize> FromLisp<N> for ArenaIndex {
    #[inline]
    fn from_lisp(_lisp: &Lisp<N>, idx: ArenaIndex) -> ArenaResult<Self> {
        Ok(idx)
    }
}

// ============================================================================
// ToLisp implementations
// ============================================================================

impl<const N: usize> ToLisp<N> for isize {
    #[inline]
    fn to_lisp(&self, lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        lisp.number(*self)
    }
}

impl<const N: usize> ToLisp<N> for bool {
    #[inline]
    fn to_lisp(&self, _lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        Ok(ArenaIndex::from_bool(*self))
    }
}

impl<const N: usize> ToLisp<N> for ArenaIndex {
    #[inline]
    fn to_lisp(&self, _lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        Ok(*self)
    }
}

impl<const N: usize> ToLisp<N> for () {
    #[inline]
    fn to_lisp(&self, _lisp: &Lisp<N>) -> ArenaResult<ArenaIndex> {
        Ok(ArenaIndex::INERT)
    }
}

// ============================================================================
// Argument extraction
// ============================================================================

/// Extract a typed argument from a Lisp argument cons-list.
///
/// Returns the extracted value and the remaining arguments (cdr of the list).
///
/// # Errors
///
/// Returns [`ArenaError::TypeError`] if the argument list is not a cons cell
/// or the value cannot be converted to type `T`.
///
/// # Example
///
/// ```rust
/// use grift::{Lisp, ArenaIndex, extract_arg};
///
/// let lisp: Lisp<20000> = Lisp::new();
/// let idx = lisp.eval_to_index("(list 42 #t)").unwrap();
/// let (n, rest): (isize, ArenaIndex) = extract_arg(&lisp, idx).unwrap();
/// assert_eq!(n, 42);
/// let (b, rest): (bool, ArenaIndex) = extract_arg(&lisp, rest).unwrap();
/// assert_eq!(b, true);
/// ```
pub fn extract_arg<const N: usize, T: FromLisp<N>>(
    lisp: &Lisp<N>,
    args: ArenaIndex,
) -> ArenaResult<(T, ArenaIndex)> {
    let Value::Cons { car, cdr } = lisp.get(args)? else {
        return Err(ArenaError::TypeError);
    };
    let val = T::from_lisp(lisp, car)?;
    Ok((val, cdr))
}

// ============================================================================
// register_native! macro
// ============================================================================

/// Define a native function for use with [`Lisp::register_native`](crate::Lisp::register_native).
///
/// Generates a function with the correct signature for registration.
/// Arguments are automatically extracted from the Lisp argument list and
/// converted to Rust types via [`FromLisp`]. The return value is converted
/// back via [`ToLisp`].
///
/// # Variants
///
/// **Standard** — body can only use extracted arguments:
/// ```rust
/// use grift::register_native;
///
/// register_native!(my_add, (a: isize, b: isize) -> isize, { a + b });
/// ```
///
/// **With lisp access** — use `|lisp, args|` to name the interpreter and
/// remaining argument variables, making them accessible in the body:
/// ```rust
/// use grift::register_native;
///
/// register_native!(my_fn, (n: isize) -> isize, |lisp, _args| {
///     // `lisp` and `_args` (remaining) are accessible here
///     let _ = lisp;
///     n * 2
/// });
/// ```
#[macro_export]
macro_rules! register_native {
    // Internal recursive argument extraction helper
    (@extract_args $lisp:ident, $args:ident, $rest_name:ident,) => {};
    (@extract_args $lisp:ident, $args:ident, $rest_name:ident, $arg:ident : $ty:ty $(, $rest_args:ident : $rest_tys:ty)*) => {
        let ($arg, $rest_name): ($ty, _) = $crate::extract_arg($lisp, $args)?;
        $crate::register_native!(@extract_args $lisp, $rest_name, $rest_name, $($rest_args : $rest_tys),*);
    };

    // Standard variant (no lisp access in body)
    ($name:ident, ($($arg:ident : $ty:ty),*) -> $ret:ty, $body:block) => {
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            let _ = lisp;
            let _ = args;
            $crate::register_native!(@extract_args lisp, args, _rest, $($arg : $ty),*);
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };

    // With-lisp variant (user names the lisp and remaining-args variables)
    ($name:ident, ($($arg:ident : $ty:ty),*) -> $ret:ty, |$lisp:ident, $rest_args:ident| $body:block) => {
        #[allow(unused_variables)]
        pub fn $name<const N: usize>(
            $lisp: &$crate::Lisp<N>,
            $rest_args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            $crate::register_native!(@extract_args $lisp, $rest_args, $rest_args, $($arg : $ty),*);
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, $lisp)
        }
    };
}
