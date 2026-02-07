//! Macros for the Lisp parser.
//!
//! This module contains macros for:
//! - Defining builtin functions (`define_builtins!`)
//! - Defining standard library functions (`define_stdlib!`)
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
// Standard Library Definition Macro
// ============================================================================

/// Macro for defining standard library functions.
/// 
/// This macro generates the `StdLib` enum, its implementation, and the `ALL` constant
/// from a single declarative definition. To add a new stdlib function, simply add
/// a new entry to the macro invocation.
/// 
/// # Syntax
/// 
/// ```rust
/// use grift_core::define_stdlib;
/// define_stdlib! {
///     /// Documentation comment
///     VariantName("function-name", ["param1", "param2"], "lisp-body-code"),
///     // ... more functions
/// }
/// ```
/// 
/// # Example
/// 
/// To add a new function `(my-func x y)` that returns `(+ x y)`:
/// 
/// ```rust
/// use grift_core::define_stdlib;
/// define_stdlib! {
///     // ... existing functions ...
///     /// (my-func x y) - Add two numbers
///     MyFunc("my-func", ["x", "y"], "(+ x y)"),
/// }
/// ```
#[macro_export]
macro_rules! define_stdlib {
    (
        $(
            $(#[$attr:meta])*
            $variant:ident($name:literal, [$($param:literal),* $(,)?], $body:literal)
        ),* $(,)?
    ) => {
        /// Standard library functions (stored in static memory, not arena)
        /// 
        /// These functions are defined as Lisp code in static strings and are parsed
        /// on-demand when called. This provides:
        /// - Zero arena cost for function definitions (static strings)
        /// - Easy maintenance (just add entries to the `define_stdlib!` macro)
        /// - Simple implementation (no build scripts needed)
        /// 
        /// The parsing overhead is minimal since:
        /// 1. Standard library functions are typically called frequently (can optimize)
        /// 2. Parsing is fast (simple recursive descent)
        /// 3. Parsed AST is temporary and GC'd after evaluation
        /// 
        /// # Memory Layout
        /// 
        /// Each StdLib variant stores references to static data:
        /// - Function name (for lookup and debugging)
        /// - Parameter names (static slice)
        /// - Body source code (static string, parsed on each call)
        /// 
        /// # Adding New Functions
        /// 
        /// To add a new stdlib function, add an entry to the `define_stdlib!` macro
        /// invocation. No other code changes are needed.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum StdLib {
            $(
                $(#[$attr])*
                $variant,
            )*
        }

        impl StdLib {
            /// Get the function name
            pub const fn name(&self) -> &'static str {
                match self {
                    $(
                        StdLib::$variant => $name,
                    )*
                }
            }
            
            /// Get the parameter names for this function
            pub const fn params(&self) -> &'static [&'static str] {
                match self {
                    $(
                        StdLib::$variant => &[$($param),*],
                    )*
                }
            }
            
            /// Get the body source code (Lisp expression as static string)
            /// 
            /// This string is parsed on each call to the function.
            /// The parsed AST is temporary and GC'd after evaluation.
            pub const fn body(&self) -> &'static str {
                match self {
                    $(
                        StdLib::$variant => $body,
                    )*
                }
            }
            
            /// All standard library functions for initialization
            pub const ALL: &'static [StdLib] = &[
                $(
                    StdLib::$variant,
                )*
            ];
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
