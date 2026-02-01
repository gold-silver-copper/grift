//! Helper macros for the Lisp evaluator.
//!
//! This module contains macros for:
//! - Argument extraction from Lisp lists
//! - Builtin predicate and operation helpers
//! - Native function registration
//! - Continuation data pack/unpack generation

// ============================================================================
// Continuation Data Pack/Unpack Macros
// ============================================================================

/// Generate pack/unpack function pairs for continuation data storage.
///
/// This macro generates functions for storing and retrieving `ArenaIndex` values
/// from the evaluator's data stack, eliminating boilerplate for each continuation type.
///
/// # Syntax
///
/// ```rust,ignore
/// define_cont_pack_unpack! {
///     pack_name / unpack_name => [field1, field2, ...];
///     // more definitions...
/// }
/// ```
///
/// # Example
///
/// ```rust,ignore
/// define_cont_pack_unpack! {
///     pack_if_branch / unpack_if_branch => [then_expr, else_expr, env];
///     pack_and / unpack_and => [remaining, env];
/// }
/// ```
///
/// Generates:
/// - `fn pack_if_branch(&mut self, then_expr: ArenaIndex, else_expr: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError>`
/// - `fn unpack_if_branch(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex)`
/// - etc.
#[macro_export]
macro_rules! define_cont_pack_unpack {
    // Entry point - process each definition
    ($($pack_name:ident / $unpack_name:ident => [$($field:ident),+ $(,)?]);+ $(;)?) => {
        $(
            $crate::define_cont_pack_unpack!(@impl $pack_name, $unpack_name, [$($field),+]);
        )+
    };

    // 1 field
    (@impl $pack_name:ident, $unpack_name:ident, [$f1:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data: [", stringify!($f1), "]")]
        #[inline]
        fn $pack_name(&mut self, $f1: ArenaIndex) -> Result<usize, EvalError> {
            self.push_data(&[$f1])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> ArenaIndex {
            self.read_data1(data_start)
        }
    };

    // 2 fields
    (@impl $pack_name:ident, $unpack_name:ident, [$f1:ident, $f2:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data: [", stringify!($f1), ", ", stringify!($f2), "]")]
        #[inline]
        fn $pack_name(&mut self, $f1: ArenaIndex, $f2: ArenaIndex) -> Result<usize, EvalError> {
            self.push_data(&[$f1, $f2])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> (ArenaIndex, ArenaIndex) {
            self.read_data2(data_start)
        }
    };

    // 3 fields
    (@impl $pack_name:ident, $unpack_name:ident, [$f1:ident, $f2:ident, $f3:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data: [", stringify!($f1), ", ", stringify!($f2), ", ", stringify!($f3), "]")]
        #[inline]
        fn $pack_name(&mut self, $f1: ArenaIndex, $f2: ArenaIndex, $f3: ArenaIndex) -> Result<usize, EvalError> {
            self.push_data(&[$f1, $f2, $f3])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex) {
            self.read_data3(data_start)
        }
    };

    // 4 fields
    (@impl $pack_name:ident, $unpack_name:ident, [$f1:ident, $f2:ident, $f3:ident, $f4:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data")]
        #[inline]
        fn $pack_name(&mut self, $f1: ArenaIndex, $f2: ArenaIndex, $f3: ArenaIndex, $f4: ArenaIndex) -> Result<usize, EvalError> {
            self.push_data(&[$f1, $f2, $f3, $f4])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
            self.read_data4(data_start)
        }
    };

    // 5 fields
    (@impl $pack_name:ident, $unpack_name:ident, [$f1:ident, $f2:ident, $f3:ident, $f4:ident, $f5:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data")]
        #[inline]
        fn $pack_name(&mut self, $f1: ArenaIndex, $f2: ArenaIndex, $f3: ArenaIndex, $f4: ArenaIndex, $f5: ArenaIndex) -> Result<usize, EvalError> {
            self.push_data(&[$f1, $f2, $f3, $f4, $f5])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
            self.read_data5(data_start)
        }
    };

    // 6 fields
    (@impl $pack_name:ident, $unpack_name:ident, [$f1:ident, $f2:ident, $f3:ident, $f4:ident, $f5:ident, $f6:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data")]
        #[inline]
        fn $pack_name(&mut self, $f1: ArenaIndex, $f2: ArenaIndex, $f3: ArenaIndex, $f4: ArenaIndex, $f5: ArenaIndex, $f6: ArenaIndex) -> Result<usize, EvalError> {
            self.push_data(&[$f1, $f2, $f3, $f4, $f5, $f6])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
            self.read_data6(data_start)
        }
    };

    // 7 fields
    (@impl $pack_name:ident, $unpack_name:ident, [$f1:ident, $f2:ident, $f3:ident, $f4:ident, $f5:ident, $f6:ident, $f7:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data")]
        #[inline]
        fn $pack_name(&mut self, $f1: ArenaIndex, $f2: ArenaIndex, $f3: ArenaIndex, $f4: ArenaIndex, $f5: ArenaIndex, $f6: ArenaIndex, $f7: ArenaIndex) -> Result<usize, EvalError> {
            self.push_data(&[$f1, $f2, $f3, $f4, $f5, $f6, $f7])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
            self.read_data7(data_start)
        }
    };
}

/// Generate pack/unpack pairs where the first field is a `Builtin`.
///
/// The Builtin is encoded via `Self::encode_builtin()` on pack and
/// decoded via `Self::decode_builtin()` on unpack.
///
/// # Syntax
///
/// ```rust,ignore
/// define_cont_pack_unpack_builtin_first! {
///     pack_name / unpack_name => builtin_field, [arena_fields...];
/// }
/// ```
///
/// # Example
///
/// ```rust,ignore
/// define_cont_pack_unpack_builtin_first! {
///     pack_builtin_force_arg / unpack_builtin_force_arg =>
///         builtin, [remaining_args, collected, call_expr, eval_env];
/// }
/// ```
#[macro_export]
macro_rules! define_cont_pack_unpack_builtin_first {
    // Entry point
    ($($pack_name:ident / $unpack_name:ident => $builtin:ident, [$($field:ident),* $(,)?]);+ $(;)?) => {
        $(
            $crate::define_cont_pack_unpack_builtin_first!(@impl $pack_name, $unpack_name, $builtin, [$($field),*]);
        )+
    };

    // Builtin + 2 ArenaIndex fields (total 3)
    (@impl $pack_name:ident, $unpack_name:ident, $builtin:ident, [$f1:ident, $f2:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data (Builtin first)")]
        #[inline]
        fn $pack_name(&mut self, $builtin: Builtin, $f1: ArenaIndex, $f2: ArenaIndex) -> Result<usize, EvalError> {
            let builtin_encoded = Self::encode_builtin($builtin);
            self.push_data(&[builtin_encoded, $f1, $f2])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> (Builtin, ArenaIndex, ArenaIndex) {
            let (b, f1, f2) = self.read_data3(data_start);
            (Self::decode_builtin(b), f1, f2)
        }
    };

    // Builtin + 3 ArenaIndex fields (total 4)
    (@impl $pack_name:ident, $unpack_name:ident, $builtin:ident, [$f1:ident, $f2:ident, $f3:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data (Builtin first)")]
        #[inline]
        fn $pack_name(&mut self, $builtin: Builtin, $f1: ArenaIndex, $f2: ArenaIndex, $f3: ArenaIndex) -> Result<usize, EvalError> {
            let builtin_encoded = Self::encode_builtin($builtin);
            self.push_data(&[builtin_encoded, $f1, $f2, $f3])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> (Builtin, ArenaIndex, ArenaIndex, ArenaIndex) {
            let (b, f1, f2, f3) = self.read_data4(data_start);
            (Self::decode_builtin(b), f1, f2, f3)
        }
    };

    // Builtin + 4 ArenaIndex fields (total 5)
    (@impl $pack_name:ident, $unpack_name:ident, $builtin:ident, [$f1:ident, $f2:ident, $f3:ident, $f4:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data (Builtin first)")]
        #[inline]
        fn $pack_name(&mut self, $builtin: Builtin, $f1: ArenaIndex, $f2: ArenaIndex, $f3: ArenaIndex, $f4: ArenaIndex) -> Result<usize, EvalError> {
            let builtin_encoded = Self::encode_builtin($builtin);
            self.push_data(&[builtin_encoded, $f1, $f2, $f3, $f4])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> (Builtin, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
            let (b, f1, f2, f3, f4) = self.read_data5(data_start);
            (Self::decode_builtin(b), f1, f2, f3, f4)
        }
    };
}

/// Generate pack/unpack pairs with a `usize` field at a specific position.
///
/// The usize is encoded via `ArenaIndex::new()` on pack and decoded via `.raw()` on unpack.
///
/// # Syntax
///
/// ```rust,ignore
/// define_cont_pack_unpack_with_usize! {
///     pack_name / unpack_name => [before_fields...], usize_field, [after_fields...];
/// }
/// ```
///
/// # Example
///
/// ```rust,ignore
/// define_cont_pack_unpack_with_usize! {
///     pack_native_args_collect / unpack_native_args_collect =>
///         [remaining, collected], id, [env];
///     pack_quasiquote_car / unpack_quasiquote_car =>
///         [cdr], depth, [env];
/// }
/// ```
#[macro_export]
macro_rules! define_cont_pack_unpack_with_usize {
    // Entry point
    ($($pack_name:ident / $unpack_name:ident => [$($before:ident),*], $usize_field:ident, [$($after:ident),*]);+ $(;)?) => {
        $(
            $crate::define_cont_pack_unpack_with_usize!(@impl $pack_name, $unpack_name, [$($before),*], $usize_field, [$($after),*]);
        )+
    };

    // [1 before], usize, [1 after] = 3 total
    (@impl $pack_name:ident, $unpack_name:ident, [$b1:ident], $usize_field:ident, [$a1:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data (with usize)")]
        #[inline]
        fn $pack_name(&mut self, $b1: ArenaIndex, $usize_field: usize, $a1: ArenaIndex) -> Result<usize, EvalError> {
            let usize_encoded = ArenaIndex::new($usize_field);
            self.push_data(&[$b1, usize_encoded, $a1])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> (ArenaIndex, usize, ArenaIndex) {
            let (b1, u, a1) = self.read_data3(data_start);
            (b1, u.raw(), a1)
        }
    };

    // [2 before], usize, [1 after] = 4 total
    (@impl $pack_name:ident, $unpack_name:ident, [$b1:ident, $b2:ident], $usize_field:ident, [$a1:ident]) => {
        #[doc = concat!("Pack ", stringify!($pack_name), " data (with usize)")]
        #[inline]
        fn $pack_name(&mut self, $b1: ArenaIndex, $b2: ArenaIndex, $usize_field: usize, $a1: ArenaIndex) -> Result<usize, EvalError> {
            let usize_encoded = ArenaIndex::new($usize_field);
            self.push_data(&[$b1, $b2, usize_encoded, $a1])
        }

        #[doc = concat!("Unpack ", stringify!($unpack_name), " data")]
        #[inline]
        fn $unpack_name(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, usize, ArenaIndex) {
            let (b1, b2, u, a1) = self.read_data4(data_start);
            (b1, b2, u.raw(), a1)
        }
    };
}

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
/// Extracts one integer argument and returns a boolean based on the predicate.
///
/// # Example
/// 
/// `builtin_numeric_pred!(self, args, call_expr, |n| n == 0)` extracts one integer
/// and returns whether it equals zero.
#[macro_export]
macro_rules! builtin_numeric_pred {
    ($self:expr, $args:expr, $call_expr:expr, $check:expr) => {{
        let n = $self.get_int($self.lisp.car($args)?, $call_expr)?;
        $self.lisp.boolean($check(n)).map_err(Into::into)
    }};
}

/// Macro for integer identity operations (rounding on integers).
///
/// For integers, floor/ceiling/truncate/round are all identity operations.
///
/// # Example
/// 
/// `builtin_int_identity!(self, args, call_expr)` extracts one integer and returns it.
#[macro_export]
macro_rules! builtin_int_identity {
    ($self:expr, $args:expr, $call_expr:expr) => {{
        let n = $self.get_int($self.lisp.car($args)?, $call_expr)?;
        $self.lisp.number(n).map_err(Into::into)
    }};
}

/// Macro for binary integer operations with division-by-zero check.
///
/// Extracts two integer arguments, checks for division by zero, and applies the operation.
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

/// Macro for binary character comparison operations.
///
/// Extracts two character arguments and compares them.
///
/// # Example
/// 
/// `builtin_char_cmp!(self, args, call_expr, |a, b| a == b)` compares two characters.
#[macro_export]
macro_rules! builtin_char_cmp {
    ($self:expr, $args:expr, $call_expr:expr, $cmp:expr) => {{
        let a = $self.get_char($self.lisp.car($args)?, $call_expr)?;
        let b = $self.get_char($self.lisp.car($self.lisp.cdr($args)?)?, $call_expr)?;
        $self.lisp.boolean($cmp(a, b)).map_err(Into::into)
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
