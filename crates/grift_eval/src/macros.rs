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

/// Macro for unary character transformation.
///
/// Extracts one character argument, applies ASCII transformation, and returns a char.
///
/// # Example
/// 
/// `builtin_char_transform!(self, args, call_expr, 'a', 'z', b'a', b'A')` does uppercase.
#[macro_export]
macro_rules! builtin_char_transform {
    ($self:expr, $args:expr, $call_expr:expr, $from_low:expr, $from_high:expr, $from_base:expr, $to_base:expr) => {{
        let c = $self.get_char($self.lisp.car($args)?, $call_expr)?;
        let result = if c >= $from_low && c <= $from_high {
            ((c as u8) - $from_base + $to_base) as char
        } else {
            c
        };
        $self.lisp.char(result).map_err(Into::into)
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
        let result = match (val_a, val_b) {
            (Value::Number(x), Value::Number(y)) => $cmp(x as $crate::fsize, y as $crate::fsize),
            (Value::Number(x), Value::Float(y)) => $cmp(x as $crate::fsize, y),
            (Value::Float(x), Value::Number(y)) => $cmp(x, y as $crate::fsize),
            (Value::Float(x), Value::Float(y)) => $cmp(x, y),
            (v, _) if !v.is_number() => return Err($self.type_error($call_expr, "number", v.type_name())),
            (_, v) => return Err($self.type_error($call_expr, "number", v.type_name())),
        };
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
                    None => Err($self.make_error($crate::ErrorKind::DivisionByZero, $call_expr)),
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
