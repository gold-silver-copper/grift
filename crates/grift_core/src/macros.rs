//! Macros for the Lisp parser.
//!
//! This module contains macros for:
//! - Defining builtin functions (`define_builtins!`)
//! - Pack/unpack refs operations for Lisp context (`impl_pack_unpack_refs!`)

// ============================================================================
// Builtin Definition Macro
// ============================================================================

/// Macro for defining built-in functions.
/// 
/// This macro generates the `Builtin` enum, its `name()` method, and the `ALL` constant
/// from a single declarative definition. To add a new builtin, simply add a new entry
/// to the macro invocation (and implement its evaluation in grift_eval).
/// 
/// # Syntax
/// 
/// ```rust
/// use grift_core::define_builtins;
/// define_builtins! {
///     /// Documentation comment
///     VariantName => "lisp-name",
///     // ... more builtins
/// }
/// ```
/// 
/// # Example
/// 
/// To add a new builtin `my-builtin`:
/// 
/// ```rust
/// use grift_core::define_builtins;
/// define_builtins! {
///     // ... existing builtins ...
///     /// (my-builtin x) - Does something with x
///     MyBuiltin => "my-builtin",
/// }
/// ```
/// 
/// Note: After adding a builtin here, you must also implement its evaluation
/// logic in the `grift_eval` crate.
#[macro_export]
macro_rules! define_builtins {
    (
        $(
            $(#[$attr:meta])*
            $variant:ident => $name:literal
        ),* $(,)?
    ) => {
        /// Built-in functions (optimization to avoid symbol lookup)
        /// 
        /// NOTE: This Lisp supports mutation via set!, set-car!, and set-cdr!
        /// - Mutation operations break referential transparency
        /// - All evaluation is call-by-value (strict)
        /// 
        /// # Adding New Builtins
        /// 
        /// To add a new builtin:
        /// 1. Add an entry to the `define_builtins!` macro invocation
        /// 2. Implement its evaluation logic in `grift_eval`
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Builtin {
            $(
                $(#[$attr])*
                $variant,
            )*
        }

        impl Builtin {
            /// Get the symbol name for this builtin
            pub const fn name(&self) -> &'static str {
                match self {
                    $(
                        Builtin::$variant => $name,
                    )*
                }
            }
            
            /// All builtins for initialization
            pub const ALL: &'static [Builtin] = &[
                $(
                    Builtin::$variant,
                )*
            ];
            
            /// Convert from usize discriminant (for continuation data stack encoding)
            /// Returns the first builtin if out of range.
            #[inline]
            pub fn from_usize(n: usize) -> Self {
                Self::ALL.get(n).copied().unwrap_or(Self::ALL[0])
            }
        }
    };
}

// ============================================================================
// Pack/Unpack Refs Macro (Internal)
// ============================================================================

/// Internal macro to generate pack_refsN and unpack_refsN methods.
/// Reduces ~120 lines of repetitive code to ~15 lines of macro invocations.
macro_rules! impl_pack_unpack_refs {
    // Special case for 1 (no contiguous allocation needed)
    (1, $pack_name:ident, $unpack_name:ident) => {
        #[inline]
        pub fn $pack_name(&self, a: ArenaIndex) -> ArenaResult<ArenaIndex> {
            self.arena.alloc(Value::Ref(a))
        }
        
        #[inline]
        pub fn $unpack_name(&self, data: ArenaIndex) -> ArenaResult<ArenaIndex> {
            self.arena.get(data)?.as_ref().ok_or(ArenaError::InvalidIndex)
        }
    };
    // General case for N >= 2
    ($n:expr, $pack_name:ident, $unpack_name:ident, $set_fn:ident, $get_fn:ident, [$($var:ident),+ $(,)?]) => {
        #[inline]
        #[allow(clippy::too_many_arguments)]
        pub fn $pack_name(&self, $($var: ArenaIndex),+) -> ArenaResult<ArenaIndex> {
            let data = self.arena.alloc_contiguous($n, Value::Nil)?;
            self.arena.$set_fn(data, $(Value::Ref($var)),+)?;
            Ok(data)
        }
        
        #[inline]
        pub fn $unpack_name(&self, data: ArenaIndex) -> ArenaResult<( $( impl_pack_unpack_refs!(@T $var) ),+ )> {
            let ($($var),+) = self.arena.$get_fn(data)?;
            match ($($var.as_ref()),+) {
                ($(Some($var)),+) => Ok(($($var),+)),
                _ => Err(ArenaError::InvalidIndex),
            }
        }
    };
    // Helper to generate ArenaIndex for tuple type
    (@T $var:ident) => { ArenaIndex };
}

// Note: impl_pack_unpack_refs! is available crate-wide via #[macro_use] in lib.rs
