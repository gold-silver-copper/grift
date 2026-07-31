//! Native function registration for user-defined Rust functions.
//!
//! This module provides the infrastructure for registering custom Rust
//! functions as callable Lisp applicatives. Native functions receive
//! already-evaluated arguments as a cons-list and return an arena-allocated
//! result.
//!
//! ## Overview
//!
//! - [`LispOps`] — trait exposing the capabilities native functions need.
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
//! let lisp: Lisp = Lisp::new();
//! lisp.register_native("add3", native_add3).unwrap();
//! assert_eq!(lisp.eval("(add3 1 2 3)"), Ok(Value::Number(6)));
//! ```

use crate::arena::{ArenaError, ArenaIndex, ArenaResult, ArenaStats, GcStats};

use crate::value::Value;

/// Type alias for native function pointers.
///
/// A native function receives a [`LispOps`] trait object and a cons-list of
/// already-evaluated arguments, and returns an arena-allocated result.
/// The trait-object boundary keeps native functions decoupled from the
/// concrete interpreter representation.
pub type NativeFn = fn(&dyn LispOps, ArenaIndex) -> ArenaResult<ArenaIndex>;

/// Trait that exposes the full public API of [`Lisp`](crate::Lisp) through
/// a trait object.
///
/// All user-registered native functions receive `&dyn LispOps` instead of
/// the concrete [`Lisp`](crate::Lisp) type, preserving a small capability
/// boundary for [`NativeFn`] values.
pub trait LispOps {
    /// Allocate a number value.
    fn number(&self, n: isize) -> ArenaResult<ArenaIndex>;
    /// Return the `ArenaIndex` for a boolean (`#t` or `#f`).
    ///
    /// Booleans are pre-allocated singletons, so this never allocates
    /// and cannot fail.
    fn boolean(&self, b: bool) -> ArenaIndex;
    /// Return the `ArenaIndex` for nil (the empty list).
    ///
    /// Nil is a pre-allocated singleton, so this never allocates
    /// and cannot fail.
    fn nil(&self) -> ArenaIndex;
    /// Allocate a cons cell.
    fn cons(&self, car: ArenaIndex, cdr: ArenaIndex) -> ArenaResult<ArenaIndex>;
    /// Allocate a character (one-element string).
    fn char_val(&self, c: char) -> ArenaResult<ArenaIndex>;
    /// Allocate a string from a `&str`.
    ///
    /// Strings are stored as a linked list of `CharPair` nodes in the arena.
    /// An empty string is represented as nil.
    fn alloc_string(&self, s: &str) -> ArenaResult<ArenaIndex>;
    /// Allocate (or retrieve an interned) symbol by name.
    fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex>;
    /// Get the value at an arena index.
    fn get(&self, idx: ArenaIndex) -> ArenaResult<Value>;
    /// Get car of a cons cell or CharPair (user-facing).
    fn car_char(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex>;
    /// Get cdr of a cons cell or CharPair (user-facing).
    fn cdr_char(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex>;
    /// Get car of cdr (second element of a list, user-facing).
    fn cadr_char(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex>;
    /// Allocate a validated lambda (applicative from an operative).
    fn lambda(
        &self,
        params: ArenaIndex,
        body: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex>;
    /// Wrap a combiner in an Applicative.
    fn wrap(&self, combiner: ArenaIndex) -> ArenaResult<ArenaIndex>;
    /// Unwrap an Applicative to get the inner combiner.
    fn unwrap_applicative(&self, idx: ArenaIndex) -> ArenaResult<ArenaIndex>;
    /// Allocate a validated operative (fexpr / vau closure).
    ///
    /// This is the checked public constructor; raw closure allocation stays
    /// internal to the interpreter.
    fn vau(
        &self,
        params: ArenaIndex,
        env_param: ArenaIndex,
        body: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex>;
    /// Extract operative parts: (params, env_param, body, env).
    fn vau_parts(
        &self,
        idx: ArenaIndex,
    ) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex)>;
    /// Parse and evaluate Lisp expression(s) in a string.
    fn eval(&self, input: &str) -> Result<Value, ArenaError>;
    /// Evaluate while protecting host-retained arena indices for the call.
    fn eval_with_roots(&self, input: &str, roots: &[ArenaIndex]) -> Result<Value, ArenaError>;
    /// Parse and evaluate Lisp expression(s), returning the arena index.
    fn eval_to_index(&self, input: &str) -> Result<ArenaIndex, ArenaError>;
    /// Evaluate to an index while protecting host-retained indices for the call.
    fn eval_to_index_with_roots(
        &self,
        input: &str,
        roots: &[ArenaIndex],
    ) -> Result<ArenaIndex, ArenaError>;
    /// Return arena allocation statistics.
    fn stats(&self) -> ArenaStats;
    /// Return the baseline allocation count.
    fn baseline_allocated(&self) -> ArenaResult<usize>;
    /// Run mark-and-sweep garbage collection with the given roots.
    fn collect_garbage(&self, roots: &[ArenaIndex]) -> ArenaResult<GcStats>;
    /// Write a machine-readable representation of the value at `idx`.
    ///
    /// This matches [`crate::Lisp::write_value`], including validation of
    /// reachable symbol and string structure before formatting.
    fn write_value(&self, idx: ArenaIndex, w: &mut dyn core::fmt::Write) -> core::fmt::Result;
    /// Human-readable output (like Scheme `display`).
    ///
    /// This matches [`crate::Lisp::display_value`], including validation of
    /// reachable symbol and string structure before formatting.
    fn display_value(&self, idx: ArenaIndex, w: &mut dyn core::fmt::Write) -> core::fmt::Result;
    /// Register a native Rust function as a Lisp applicative.
    fn register_native(&self, name: &str, f: NativeFn) -> ArenaResult<()>;
    /// Define a value in the global environment under the given symbol.
    ///
    /// `sym` must be a valid symbol `ArenaIndex` (e.g. from [`symbol`](Self::symbol)).
    fn define_global(&self, sym: ArenaIndex, value: ArenaIndex) -> ArenaResult<()>;
}

/// Trait for converting a Lisp value to a Rust type.
///
/// Implemented for common types (`isize`, `bool`, `char`, `ArenaIndex`).
/// Users can implement this trait for custom types.
pub trait FromLisp: Sized {
    /// Convert a Lisp value at the given arena index to this Rust type.
    fn from_lisp(lisp: &dyn LispOps, idx: ArenaIndex) -> ArenaResult<Self>;
}

/// Trait for converting a Rust value to a Lisp arena value.
///
/// Implemented for common types (`isize`, `bool`, `char`, `ArenaIndex`, `()`).
/// Users can implement this trait for custom types.
pub trait ToLisp {
    /// Allocate this value in the Lisp arena and return its index.
    fn to_lisp(&self, lisp: &dyn LispOps) -> ArenaResult<ArenaIndex>;
}

// ============================================================================
// FromLisp implementations
// ============================================================================

impl FromLisp for isize {
    #[inline]
    /// Extract an integer from a Lisp number value.
    fn from_lisp(lisp: &dyn LispOps, idx: ArenaIndex) -> ArenaResult<Self> {
        lisp.get(idx)?.as_number()
    }
}

impl FromLisp for bool {
    #[inline]
    /// Extract a Rust `bool` from a Lisp boolean.
    fn from_lisp(lisp: &dyn LispOps, idx: ArenaIndex) -> ArenaResult<Self> {
        lisp.get(idx)?.as_bool()
    }
}

impl FromLisp for ArenaIndex {
    #[inline]
    /// Return the raw arena index unchanged.
    fn from_lisp(_lisp: &dyn LispOps, idx: ArenaIndex) -> ArenaResult<Self> {
        Ok(idx)
    }
}

impl FromLisp for char {
    #[inline]
    /// Extract a Rust `char` from a one-character Lisp string node.
    fn from_lisp(lisp: &dyn LispOps, idx: ArenaIndex) -> ArenaResult<Self> {
        match lisp.get(idx)? {
            Value::CharPair { ch, cdr } if cdr.is_nil() => Ok(ch),
            _ => Err(ArenaError::TypeError),
        }
    }
}

// ============================================================================
// ToLisp implementations
// ============================================================================

impl ToLisp for isize {
    #[inline]
    /// Allocate the integer as a Lisp number.
    fn to_lisp(&self, lisp: &dyn LispOps) -> ArenaResult<ArenaIndex> {
        lisp.number(*self)
    }
}

impl ToLisp for bool {
    #[inline]
    /// Reuse the pre-allocated Lisp boolean singleton.
    fn to_lisp(&self, _lisp: &dyn LispOps) -> ArenaResult<ArenaIndex> {
        Ok(ArenaIndex::from_bool(*self))
    }
}

impl ToLisp for ArenaIndex {
    #[inline]
    /// Treat the arena index as an already-allocated Lisp value.
    fn to_lisp(&self, _lisp: &dyn LispOps) -> ArenaResult<ArenaIndex> {
        Ok(*self)
    }
}

impl ToLisp for () {
    #[inline]
    /// Map unit to the Lisp `#inert` singleton.
    fn to_lisp(&self, _lisp: &dyn LispOps) -> ArenaResult<ArenaIndex> {
        Ok(ArenaIndex::INERT)
    }
}

impl ToLisp for char {
    #[inline]
    /// Allocate the character as a one-element Lisp string.
    fn to_lisp(&self, lisp: &dyn LispOps) -> ArenaResult<ArenaIndex> {
        lisp.char_val(*self)
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
/// let lisp: Lisp = Lisp::new();
/// let idx = lisp.eval_to_index("(list 42 #t)").unwrap();
/// let (n, rest): (isize, ArenaIndex) = extract_arg(&lisp, idx).unwrap();
/// assert_eq!(n, 42);
/// let (b, rest): (bool, ArenaIndex) = extract_arg(&lisp, rest).unwrap();
/// assert_eq!(b, true);
/// ```
pub fn extract_arg<T: FromLisp>(
    lisp: &dyn LispOps,
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
        #[doc = "Native wrapper generated by `register_native!`."]
        #[doc = ""]
        #[doc = "Arguments are extracted from the Lisp argument list, converted"]
        #[doc = "via `FromLisp`, and the return value is converted back via"]
        #[doc = "`ToLisp`."]
        pub fn $name(
            lisp: &dyn $crate::LispOps,
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
        #[doc = "Native wrapper generated by `register_native!` with direct"]
        #[doc = "access to the interpreter handle and remaining argument list."]
        pub fn $name(
            $lisp: &dyn $crate::LispOps,
            $rest_args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            $crate::register_native!(@extract_args $lisp, $rest_args, $rest_args, $($arg : $ty),*);
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, $lisp)
        }
    };
}
