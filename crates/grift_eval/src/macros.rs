//! Helper macros for the Lisp evaluator.
//!
//! This module contains macros for:
//! - Argument extraction from Lisp lists
//! - Builtin predicate and operation helpers
//! - Native function registration

// ============================================================================
// Helper Macros for Code Deduplication
// ============================================================================

/// Macro to extract arguments from a Lisp list using car/cdr.
///
/// This macro extracts multiple arguments from a list, shadowing the `args`
/// variable after each extraction.
///
/// # Example
/// 
/// `extract_args!(self, args, a, b, c)` extracts three arguments from `args`.
#[macro_export]
macro_rules! extract_args {
    ($self:expr, $args:ident, $var:ident) => {
        let $var = $self.lisp.car($args)?;
    };
    ($self:expr, $args:ident, $var:ident, $($rest:ident),+) => {
        let $var = $self.lisp.car($args)?;
        #[allow(unused_variables)]
        let $args = $self.lisp.cdr($args)?;
        $crate::extract_args!($self, $args, $($rest),+)
    };
}

/// Macro for unary predicate builtins.
///
/// Many builtins follow the pattern of extracting one argument and returning
/// a boolean based on some predicate on the value.
///
/// # Example
/// 
/// `builtin_unary_pred!(self, args, |v| v.is_nil())` extracts one argument
/// and returns a boolean result of the predicate.
#[macro_export]
macro_rules! builtin_unary_pred {
    ($self:expr, $args:expr, $check:expr) => {{
        let arg = $self.lisp.car($args)?;
        let val = $self.lisp.get(arg)?;
        $self.lisp.boolean($check(val)).map_err(Into::into)
    }};
}

/// Macro for numeric predicate builtins.
///
/// Extracts one numeric argument (integer or float) and returns a boolean based on the predicate.
/// For integer arguments, passes isize to int_check. For float arguments, passes fsize to float_check.
///
/// # Example
/// 
/// `builtin_numeric_pred!(self, args, call_expr, |n| n == 0, |f| f == 0.0)` 
#[macro_export]
macro_rules! builtin_numeric_pred {
    ($self:expr, $args:expr, $call_expr:expr, |$n:ident| $int_check:expr, |$f:ident| $float_check:expr) => {{
        let arg = $self.lisp.car($args)?;
        match $self.lisp.get(arg)? {
            Value::Number($n) => $self.lisp.boolean($int_check).map_err(Into::into),
            Value::Float($f) => $self.lisp.boolean($float_check).map_err(Into::into),
            Value::Rational { .. } => {
                // Rationals are exact; bind dummy value for the $n pattern variable
                // (used by callers like `|_n| true` which check integer predicates)
                #[allow(unused_variables)]
                let $n = 0isize;
                $self.lisp.boolean($int_check).map_err(Into::into)
            }
            Value::Complex { real: $f, .. } => $self.lisp.boolean($float_check).map_err(Into::into),
            v => Err($self.type_error($call_expr, "number", v.type_name())),
        }
    }};
    ($self:expr, $args:expr, $call_expr:expr, |$n:ident| $check:expr) => {{
        let $n = $self.get_int($self.lisp.car($args)?, $call_expr)?;
        $self.lisp.boolean($check).map_err(Into::into)
    }};
}

/// Macro for rounding operations.
///
/// For integers, these are identity operations. For floats, applies the float operation.
///
/// # Example
/// 
/// `builtin_rounding_op!(self, args, call_expr, |f| float_floor(f))` 
#[macro_export]
macro_rules! builtin_rounding_op {
    ($self:expr, $args:expr, $call_expr:expr, $float_op:expr) => {{
        let arg = $self.lisp.car($args)?;
        match $self.lisp.get(arg)? {
            Value::Number(n) => $self.lisp.number(n).map_err(Into::into),
            Value::Float(f) => {
                let result = $float_op(f);
                $self.lisp.float(result).map_err(Into::into)
            }
            Value::Rational { num, denom } => {
                let f = num as crate::fsize / denom as crate::fsize;
                let result = $float_op(f);
                $self.lisp.float(result).map_err(Into::into)
            }
            v => Err($self.type_error($call_expr, "number", v.type_name())),
        }
    }};
}

/// Macro for binary integer operations with division-by-zero check.
///
/// Extracts two integer arguments, checks for division by zero, and applies the operation.
/// Only works on exact integers (no float promotion).
///
/// # Example
/// 
/// `builtin_div_op!(self, args, call_expr, |a, b| a / b)` performs integer division.
#[macro_export]
macro_rules! builtin_div_op {
    ($self:expr, $args:expr, $call_expr:expr, $op:expr) => {{
        let a = $self.get_int($self.lisp.car($args)?, $call_expr)?;
        let b = $self.get_int($self.lisp.car($self.lisp.cdr($args)?)?, $call_expr)?;
        if b == 0 {
            return Err($self.make_error($crate::ErrorKind::DivisionByZero, $call_expr));
        }
        $self.lisp.number($op(a, b)).map_err(Into::into)
    }};
}

/// Macro for unary numeric operations (supports both int and float).
///
/// Extracts one argument. If integer, applies int_op and returns Number.
/// If float, applies float_op and returns Float.
///
/// # Example
/// 
/// `builtin_unary_num!(self, args, call_expr, |n: isize| n.abs(), |f: fsize| f.abs())` 
#[macro_export]
macro_rules! builtin_unary_num {
    ($self:expr, $args:expr, $call_expr:expr, |$n:ident : isize| $int_op:expr, |$f:ident : fsize| $float_op:expr) => {{
        let arg = $self.lisp.car($args)?;
        match $self.lisp.get(arg)? {
            Value::Number($n) => $self.lisp.number($int_op).map_err(Into::into),
            Value::Float($f) => $self.lisp.float($float_op).map_err(Into::into),
            v => Err($self.type_error($call_expr, "number", v.type_name())),
        }
    }};
}

/// Macro for unary integer operations.
///
/// Extracts one integer argument, applies a transformation, and returns a number.
/// Also accepts floats, truncating to integer first.
///
/// # Example
/// 
/// `builtin_unary_int!(self, args, call_expr, |n| n.abs())` returns absolute value.
#[macro_export]
macro_rules! builtin_unary_int {
    ($self:expr, $args:expr, $call_expr:expr, $op:expr) => {{
        let n = $self.get_int($self.lisp.car($args)?, $call_expr)?;
        $self.lisp.number($op(n)).map_err(Into::into)
    }};
}

/// Macro for unary char-to-integer conversion.
///
/// Extracts one character argument and returns its Unicode code point as a number.
#[macro_export]
macro_rules! builtin_char_to_int {
    ($self:expr, $args:expr, $call_expr:expr) => {{
        let c = $self.get_char($self.lisp.car($args)?, $call_expr)?;
        $self.lisp.number(c as isize).map_err(Into::into)
    }};
}

/// Macro for binary numeric comparison (returns boolean).
///
/// Supports mixed int/float comparisons with automatic promotion.
/// Used in apply_binary_builtin for comparison operations.
#[macro_export]
macro_rules! binary_int_cmp {
    ($self:expr, $a:expr, $b:expr, $call_expr:expr, $cmp:expr) => {{
        let val_a = $self.lisp.get($a)?;
        let val_b = $self.lisp.get($b)?;
        let to_f = |v: Value| -> Result<$crate::fsize, _> {
            match v {
                Value::Number(x) => Ok(x as $crate::fsize),
                Value::Float(x) => Ok(x),
                Value::Rational { num, denom } => Ok(num as $crate::fsize / denom as $crate::fsize),
                _ => Err($self.type_error($call_expr, "number", v.type_name())),
            }
        };
        let fa = to_f(val_a)?;
        let fb = to_f(val_b)?;
        let result = $cmp(fa, fb);
        $self.lisp.boolean(result).map_err(Into::into)
    }};
}

/// Macro for binary numeric arithmetic (returns number).
///
/// Supports mixed int/float arithmetic with automatic promotion to float.
/// Used in apply_binary_builtin for arithmetic operations.
#[macro_export]
macro_rules! binary_int_op {
    ($self:expr, $a:expr, $b:expr, $call_expr:expr, $int_op:expr, $float_op:expr) => {{
        let val_a = $self.lisp.get($a)?;
        let val_b = $self.lisp.get($b)?;
        match (val_a, val_b) {
            (Value::Number(x), Value::Number(y)) => {
                match $int_op(x, y) {
                    Some(n) => $self.lisp.number(n).map_err(Into::into),
                    None => {
                        // Integer overflow: promote to float
                        $self.lisp.float($float_op(x as $crate::fsize, y as $crate::fsize)).map_err(Into::into)
                    }
                }
            }
            (Value::Number(x), Value::Float(y)) => {
                $self.lisp.float($float_op(x as $crate::fsize, y)).map_err(Into::into)
            }
            (Value::Float(x), Value::Number(y)) => {
                $self.lisp.float($float_op(x, y as $crate::fsize)).map_err(Into::into)
            }
            (Value::Float(x), Value::Float(y)) => {
                $self.lisp.float($float_op(x, y)).map_err(Into::into)
            }
            // Rational + Number: exact rational arithmetic
            (Value::Rational { num, denom }, Value::Number(y)) => {
                // (num/denom) op (y/1) => scale y to same denominator
                match y.checked_mul(denom).and_then(|yd| $int_op(num, yd)) {
                    Some(new_num) => $self.lisp.rational(new_num, denom).map_err(Into::into),
                    None => $self.lisp.float($float_op(num as $crate::fsize / denom as $crate::fsize, y as $crate::fsize)).map_err(Into::into),
                }
            }
            (Value::Number(x), Value::Rational { num, denom }) => {
                match x.checked_mul(denom).and_then(|xd| $int_op(xd, num)) {
                    Some(new_num) => $self.lisp.rational(new_num, denom).map_err(Into::into),
                    None => $self.lisp.float($float_op(x as $crate::fsize, num as $crate::fsize / denom as $crate::fsize)).map_err(Into::into),
                }
            }
            (Value::Rational { num: n1, denom: d1 }, Value::Rational { num: n2, denom: d2 }) => {
                // (n1/d1) op (n2/d2) => (n1*d2 op n2*d1) / (d1*d2)
                match (d1.checked_mul(d2), n1.checked_mul(d2), n2.checked_mul(d1)) {
                    (Some(new_d), Some(s1), Some(s2)) => {
                        match $int_op(s1, s2) {
                            Some(new_n) => $self.lisp.rational(new_n, new_d).map_err(Into::into),
                            None => $self.lisp.float($float_op(n1 as $crate::fsize / d1 as $crate::fsize, n2 as $crate::fsize / d2 as $crate::fsize)).map_err(Into::into),
                        }
                    }
                    _ => $self.lisp.float($float_op(n1 as $crate::fsize / d1 as $crate::fsize, n2 as $crate::fsize / d2 as $crate::fsize)).map_err(Into::into),
                }
            }
            (Value::Rational { num, denom }, Value::Float(y)) => {
                $self.lisp.float($float_op(num as $crate::fsize / denom as $crate::fsize, y)).map_err(Into::into)
            }
            (Value::Float(x), Value::Rational { num, denom }) => {
                $self.lisp.float($float_op(x, num as $crate::fsize / denom as $crate::fsize)).map_err(Into::into)
            }
            (v, _) if !v.is_number() => Err($self.type_error($call_expr, "number", v.type_name())),
            (_, v) => Err($self.type_error($call_expr, "number", v.type_name())),
        }
    }};
}

/// Macro for binary division operations with zero check.
///
/// Supports mixed int/float division with automatic promotion.
/// Used in apply_binary_builtin for div/mod/rem operations.
#[macro_export]
macro_rules! binary_div_op {
    ($self:expr, $a:expr, $b:expr, $call_expr:expr, $int_op:expr, $float_op:expr) => {{
        let val_a = $self.lisp.get($a)?;
        let val_b = $self.lisp.get($b)?;
        match (val_a, val_b) {
            (Value::Number(x), Value::Number(y)) => {
                if y == 0 {
                    return Err($self.make_error($crate::ErrorKind::DivisionByZero, $call_expr));
                }
                $self.lisp.number($int_op(x, y)).map_err(Into::into)
            }
            (Value::Number(x), Value::Float(y)) => {
                if y == 0.0 {
                    return Err($self.make_error($crate::ErrorKind::DivisionByZero, $call_expr));
                }
                $self.lisp.float($float_op(x as $crate::fsize, y)).map_err(Into::into)
            }
            (Value::Float(x), Value::Number(y)) => {
                if y == 0 {
                    return Err($self.make_error($crate::ErrorKind::DivisionByZero, $call_expr));
                }
                $self.lisp.float($float_op(x, y as $crate::fsize)).map_err(Into::into)
            }
            (Value::Float(x), Value::Float(y)) => {
                if y == 0.0 {
                    return Err($self.make_error($crate::ErrorKind::DivisionByZero, $call_expr));
                }
                $self.lisp.float($float_op(x, y)).map_err(Into::into)
            }
            (v, _) if !v.is_number() => Err($self.type_error($call_expr, "number", v.type_name())),
            (_, v) => Err($self.type_error($call_expr, "number", v.type_name())),
        }
    }};
}

// ============================================================================
// Macro for Native Function Definition
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
/// use grift_eval::register_native;
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
/// use grift_eval::register_native;
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
/// use grift_eval::{Lisp, ArenaIndex, ArenaResult, FromLisp};
/// use grift_arena::ArenaResult as PwnResult;
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

    // @with_lisp variant (lisp and args accessible in body)
    ($name:ident @with_lisp, ($($arg:ident : $ty:ty),*) -> $ret:ty, $body:block) => {
        #[allow(unused_variables)]
        pub fn $name<const N: usize>(
            lisp: &$crate::Lisp<N>,
            args: $crate::ArenaIndex,
        ) -> $crate::ArenaResult<$crate::ArenaIndex> {
            $crate::register_native!(@extract_args lisp, args, args, $($arg : $ty),*);
            let result: $ret = $body;
            $crate::ToLisp::to_lisp(&result, lisp)
        }
    };
}
