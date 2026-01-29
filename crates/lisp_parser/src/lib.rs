#![no_std]
#![forbid(unsafe_code)]

//! # Lisp Parser
//!
//! A classic Lisp parser with arena-allocated values.
//!
//! ## Design
//!
//! - Symbols use contiguous string storage for memory efficiency
//! - Symbol interning ensures the same symbol name returns the same index
//! - All values are stored in a `pwn_arena` arena
//! - Supports garbage collection via the `Trace` trait
//! - Explicit boolean values (#t, #f) separate from nil/empty list
//! - **Strict evaluation** - All arguments are evaluated before function application (call-by-value)
//!
//! ## Value Representation
//!
//! - `Nil` - The empty list (NOT false!)
//! - `True` - Boolean true (#t)
//! - `False` - Boolean false (#f)
//! - `Number(Number)` - Full numerical tower: integer, rational, real, complex
//! - `Char(char)` - Single character
//! - `Cons { car, cdr }` - Pair/list cell
//! - `Symbol { chars }` - Symbol with contiguous string storage
//! - `Lambda { params, body, env }` - Closure
//! - `Builtin(Builtin)` - Optimized built-in function
//! - `StdLib(StdLib)` - Standard library function (static code, parsed on-demand)
//!
//! ## Numerical Tower
//!
//! The parser supports the full Scheme R7RS numerical tower:
//! - **Integer** - Exact integers (isize): `42`, `-17`
//! - **Rational** - Exact rationals: `3/4`, `-1/3`
//! - **Real** - Inexact floats (f64): `3.14`, `1.0e10`, `+inf.0`, `-inf.0`, `+nan.0`
//! - **Complex** - Complex numbers: `3+4i`, `1.0-2.5i`, `3@1.57`
//!
//! ## Reserved Slots
//!
//! The first 4 slots of the arena are reserved:
//! - Slot 0: `Value::Nil` - the empty list singleton
//! - Slot 1: `Value::True` - boolean true singleton
//! - Slot 2: `Value::False` - boolean false singleton
//! - Slot 3: `Value::Cons` - intern table reference cell
//!
//! ## Pitfalls and Gotchas
//!
//! ### Truthiness
//! - **Only `#f` is false!** Everything else is truthy, including:
//!   - `nil` / `'()` (the empty list)
//!   - `0` (the number zero)
//!   - Empty strings
//!
//! ### Garbage Collection
//! - The intern table is always a GC root - interned symbols are never collected
//! - Reserved slots (nil, true, false) are implicitly preserved
//! - Run `gc()` with appropriate roots to reclaim memory
//!
//! ### StdLib Functions
//! - Body is parsed on each call (minor overhead, but keeps code out of arena)
//! - Recursive stdlib functions work via the global environment
//! - Errors in static source strings are only caught at runtime

pub mod number;
pub use number::Number;

pub use pwn_arena::{Arena, ArenaIndex, ArenaError, ArenaResult, Trace, GcStats};

/// Macro for defining built-in functions.
/// 
/// This macro generates the `Builtin` enum, its `name()` method, and the `ALL` constant
/// from a single declarative definition. To add a new builtin, simply add a new entry
/// to the macro invocation (and implement its evaluation in lisp_eval).
/// 
/// # Syntax
/// 
/// ```rust
/// use lisp_parser::define_builtins;
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
/// use lisp_parser::define_builtins;
/// define_builtins! {
///     // ... existing builtins ...
///     /// (my-builtin x) - Does something with x
///     MyBuiltin => "my-builtin",
/// }
/// ```
/// 
/// Note: After adding a builtin here, you must also implement its evaluation
/// logic in the `lisp_eval` crate.
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
        /// 2. Implement its evaluation logic in `lisp_eval`
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
        }
    };
}

// Define all built-in functions using the macro.
// To add a new builtin, add an entry here and implement its evaluation in lisp_eval.
define_builtins! {
    // List operations
    /// car - Get first element of pair
    Car => "car",
    /// cdr - Get second element of pair
    Cdr => "cdr",
    /// cons - Create a pair
    Cons => "cons",
    /// list - Create a list from arguments
    List => "list",
    
    // Predicates (Scheme R7RS compliant)
    /// null? - Check if value is the empty list
    Null => "null?",
    /// pair? - Check if value is a pair
    Pairp => "pair?",
    /// number? - Check if value is a number
    Numberp => "number?",
    /// boolean? - Check if value is a boolean
    Booleanp => "boolean?",
    /// procedure? - Check if value is a procedure
    Procedurep => "procedure?",
    /// symbol? - Check if value is a symbol
    Symbolp => "symbol?",
    /// eq? - Scheme-compliant identity equality
    EqP => "eq?",
    /// eqv? - Scheme-compliant value equality
    EqvP => "eqv?",
    /// equal? - Scheme-compliant recursive structural equality
    EqualP => "equal?",
    
    // Arithmetic
    /// + - Addition
    Add => "+",
    /// - - Subtraction
    Sub => "-",
    /// * - Multiplication
    Mul => "*",
    /// / - Division
    Div => "/",
    /// modulo - Scheme modulo (result has sign of divisor)
    Modulo => "modulo",
    /// remainder - Scheme remainder (result has sign of dividend)
    Remainder => "remainder",
    /// quotient - Integer quotient (truncated towards zero)
    Quotient => "quotient",
    /// abs - Absolute value
    Abs => "abs",
    /// max - Maximum of numbers
    Max => "max",
    /// min - Minimum of numbers
    Min => "min",
    /// gcd - Greatest common divisor
    Gcd => "gcd",
    /// lcm - Least common multiple
    Lcm => "lcm",
    /// expt - Exponentiation
    Expt => "expt",
    /// square - Square of a number
    Square => "square",
    
    // Numeric predicates
    /// zero? - Check if number is zero
    Zerop => "zero?",
    /// positive? - Check if number is positive
    Positivep => "positive?",
    /// negative? - Check if number is negative
    Negativep => "negative?",
    /// odd? - Check if number is odd
    Oddp => "odd?",
    /// even? - Check if number is even
    Evenp => "even?",
    /// integer? - Check if value is an integer
    Integerp => "integer?",
    /// exact? - Check if number is exact (always true for integers)
    Exactp => "exact?",
    /// inexact? - Check if number is inexact (always false for integers)
    Inexactp => "inexact?",
    /// exact-integer? - Check if value is an exact integer
    ExactIntegerp => "exact-integer?",
    
    // Rounding operations (R7RS Section 6.2.6) - Identity for integers
    /// floor - Largest integer not greater than x (identity for integers)
    Floor => "floor",
    /// ceiling - Smallest integer not less than x (identity for integers)
    Ceiling => "ceiling",
    /// truncate - Integer closest to x whose absolute value is not larger (identity for integers)
    Truncate => "truncate",
    /// round - Closest integer to x, rounding to even when x is halfway (identity for integers)
    Round => "round",
    
    // Comparison
    /// < - Less than
    Lt => "<",
    /// > - Greater than
    Gt => ">",
    /// <= - Less than or equal
    Le => "<=",
    /// >= - Greater than or equal
    Ge => ">=",
    /// = - Numeric equality
    NumEq => "=",
    
    // Boolean operations
    /// not - Boolean negation
    Not => "not",
    
    // I/O
    /// newline - Print a newline
    Newline => "newline",
    /// display - Print value without quotes
    Display => "display",
    
    // Error handling
    /// error - Raise an error
    Error => "error",
    
    // Mutation operations
    /// set-car! - Mutate car of pair
    SetCar => "set-car!",
    /// set-cdr! - Mutate cdr of pair
    SetCdr => "set-cdr!",
    
    // Array operations (O(1) indexed access and mutation)
    /// make-array - Create an array with given length and initial value
    MakeArray => "make-array",
    /// array-ref - Get element at index (O(1))
    ArrayRef => "array-ref",
    /// array-set! - Set element at index (O(1))
    ArraySet => "array-set!",
    /// array-length - Get array length (O(1))
    ArrayLength => "array-length",
    /// array? - Check if value is an array
    Arrayp => "array?",
    
    // Garbage collection and arena control
    /// gc - Manually trigger garbage collection
    Gc => "gc",
    /// gc-enable - Enable automatic garbage collection
    GcEnable => "gc-enable",
    /// gc-disable - Disable automatic garbage collection
    GcDisable => "gc-disable",
    /// gc-enabled? - Check if GC is enabled
    GcEnabledP => "gc-enabled?",
    /// arena-stats - Get arena statistics as a list
    ArenaStats => "arena-stats",
}

/// Macro for defining standard library functions.
/// 
/// This macro generates the `StdLib` enum, its implementation, and the `ALL` constant
/// from a single declarative definition. To add a new stdlib function, simply add
/// a new entry to the macro invocation.
/// 
/// # Syntax
/// 
/// ```rust
/// use lisp_parser::define_stdlib;
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
/// use lisp_parser::define_stdlib;
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

// Define all standard library functions using the include_stdlib! macro.
// This macro reads the stdlib.scm file and generates the StdLib enum.
// To add a new function, simply add a new entry in stdlib.scm.
// Note: member/assoc use eq? for comparison (like Scheme's memq/assq).
// This works for symbols and identical objects. For value comparison,
// define a custom function or use fold with a predicate.
lisp_macros::include_stdlib!("src/stdlib.scm");

/// A Lisp value
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// The empty list (NOT false - use False for that)
    Nil,
    
    /// Boolean true (#t)
    True,
    
    /// Boolean false (#f) - the ONLY false value
    False,
    
    /// Number (full numerical tower: integer, rational, real, complex)
    Number(Number),
    
    /// Single character (used in strings and symbol storage)
    Char(char),
    
    /// Cons cell (pair)
    Cons {
        car: ArenaIndex,
        cdr: ArenaIndex,
    },
    
    /// Symbol (contains a contiguous string)
    /// The `chars` field points to a Value::String which contains the symbol name.
    /// The length is obtained from the String value, providing a single source of truth.
    Symbol {
        chars: ArenaIndex,
    },
    
    /// Lambda / closure (memory-optimized)
    /// 
    /// To minimize enum size, lambda data is stored as a linked list in the arena:
    /// `data` points to a cons cell `(params . (body . env))` where:
    /// - `params`: List of parameter symbols
    /// - `body`: Expression to evaluate
    /// - `env`: Captured environment (alist)
    /// 
    /// Use `Lisp::lambda()` to create and `Lisp::lambda_parts()` to extract.
    Lambda {
        data: ArenaIndex,  // Points to (params . (body . env))
    },
    
    /// Built-in function (optimized)
    Builtin(Builtin),
    
    /// Standard library function (stored in static memory with caching)
    /// 
    /// Unlike Lambda which stores code in the arena, StdLib references static
    /// function definitions. The function body is parsed on first call and cached,
    /// providing good performance while minimizing initial arena cost.
    /// 
    /// # Memory Efficiency
    /// 
    /// - Function definitions are in static memory (const strings)
    /// - Parsed body and params are cached on first call
    /// - Subsequent calls reuse the cached parsed AST
    /// 
    /// # Cache Field
    /// 
    /// - `cache`: NULL until first call, then points to `(body . params)` cons cell
    /// 
    /// Use `Lisp::stdlib()` to create and `Lisp::stdlib_cache()` to access cache.
    StdLib {
        func: StdLib,
        cache: ArenaIndex,  // NULL = not yet parsed, else (body . params)
    },
    
    /// Array (contiguous storage of values in the arena)
    /// 
    /// Arrays store values contiguously in the arena, similar to how symbols
    /// store characters. This provides O(1) indexed access and mutation.
    /// 
    /// # Memory Layout
    /// 
    /// - `data` points to the first element in contiguous storage
    /// - Elements are stored at data+0, data+1, ..., data+(len-1)
    /// 
    /// # Example
    /// 
    /// ```lisp
    /// (define arr (make-array 3 0))  ; Create array of 3 zeros
    /// (array-set! arr 1 42)          ; Set index 1 to 42
    /// (array-ref arr 1)              ; => 42
    /// (array-length arr)             ; => 3
    /// ```
    Array {
        data: ArenaIndex,  // Points to first element in contiguous block
        len: usize,        // Number of elements
    },
    
    /// String (contiguous storage of Char values in the arena)
    /// 
    /// Strings store characters contiguously in the arena, similar to arrays.
    /// This provides O(1) indexed access and O(1) length lookup.
    /// 
    /// # Memory Layout
    /// 
    /// - `data` points to the first character in contiguous storage
    /// - Characters are stored at data+0, data+1, ..., data+(len-1)
    /// 
    /// # Example
    /// 
    /// ```lisp
    /// (string-length "hello")   ; => 5
    /// (string-ref "hello" 0)    ; => #\h
    /// ```
    String {
        data: ArenaIndex,  // Points to first Char in contiguous block
        len: usize,        // Number of characters
    },
    
    /// Native function (Rust function callable from Lisp)
    ///
    /// Native functions are registered at runtime and identified by their ID.
    /// The actual function pointer is stored in the evaluator's NativeRegistry.
    ///
    /// # Fields
    ///
    /// - `id`: Index into the NativeRegistry's entries array
    /// - `name_hash`: Hash of the function name for quick comparison
    Native {
        id: usize,          // Index in the NativeRegistry
        name_hash: usize,   // Hash for debugging/lookup verification
    },
}

impl Value {
    /// Check if this value is nil (empty list)
    #[inline]
    pub const fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }
    
    /// Check if this value is false (#f)
    /// This is the ONLY way to be false in this Lisp
    #[inline]
    pub const fn is_false(&self) -> bool {
        matches!(self, Value::False)
    }
    
    /// Check if this value is true (#t)
    #[inline]
    pub const fn is_true(&self) -> bool {
        matches!(self, Value::True)
    }
    
    /// Check if this value is a boolean (#t or #f)
    #[inline]
    pub const fn is_boolean(&self) -> bool {
        matches!(self, Value::True | Value::False)
    }
    
    /// Check if this value is an atom (not a cons cell)
    #[inline]
    pub const fn is_atom(&self) -> bool {
        !matches!(self, Value::Cons { .. })
    }
    
    /// Check if this value is a number
    #[inline]
    pub const fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }
    
    /// Check if this value is a symbol
    #[inline]
    pub const fn is_symbol(&self) -> bool {
        matches!(self, Value::Symbol { .. })
    }
    
    /// Check if this value is a cons cell (pair)
    #[inline]
    pub const fn is_cons(&self) -> bool {
        matches!(self, Value::Cons { .. })
    }
    
    /// Check if this value is a lambda
    #[inline]
    pub const fn is_lambda(&self) -> bool {
        matches!(self, Value::Lambda { .. })
    }
    
    /// Check if this value is a builtin
    #[inline]
    pub const fn is_builtin(&self) -> bool {
        matches!(self, Value::Builtin(_))
    }
    
    /// Check if this value is a stdlib function
    #[inline]
    pub const fn is_stdlib(&self) -> bool {
        matches!(self, Value::StdLib { .. })
    }
    
    /// Check if this value is a native (Rust) function
    #[inline]
    pub const fn is_native(&self) -> bool {
        matches!(self, Value::Native { .. })
    }
    
    /// Check if this value is a procedure (lambda, builtin, stdlib, or native function)
    #[inline]
    pub const fn is_procedure(&self) -> bool {
        matches!(self, Value::Lambda { .. } | Value::Builtin(_) | Value::StdLib { .. } | Value::Native { .. })
    }
    
    /// Check if this value is an array
    #[inline]
    pub const fn is_array(&self) -> bool {
        matches!(self, Value::Array { .. })
    }
    
    /// Check if this value is a string
    #[inline]
    pub const fn is_string(&self) -> bool {
        matches!(self, Value::String { .. })
    }
    
    /// Get the number value if this is a number
    #[inline]
    pub const fn as_number(&self) -> Option<Number> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
    
    /// Get the number as an isize if this is an exact integer
    #[inline]
    pub fn as_integer(&self) -> Option<isize> {
        match self {
            Value::Number(n) => n.to_isize(),
            _ => None,
        }
    }
    
    /// Get the char value if this is a char
    #[inline]
    pub const fn as_char(&self) -> Option<char> {
        match self {
            Value::Char(c) => Some(*c),
            _ => None,
        }
    }
    
    /// Get a human-readable type name
    pub const fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::True | Value::False => "boolean",
            Value::Number(_) => "number",
            Value::Char(_) => "char",
            Value::Cons { .. } => "pair",
            Value::Symbol { .. } => "symbol",
            Value::Lambda { .. } => "procedure",
            Value::Builtin(_) => "procedure",
            Value::StdLib { .. } => "procedure",
            Value::Native { .. } => "native",
            Value::Array { .. } => "array",
            Value::String { .. } => "string",
        }
    }
}

/// Implement Trace for GC support
impl<const N: usize> Trace<Value, N> for Value {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match self {
            Value::Nil | Value::True | Value::False | 
            Value::Number(_) | Value::Char(_) | Value::Builtin(_) |
            Value::Native { .. } => {
                // No references
            }
            Value::StdLib { cache, .. } => {
                // Trace cached cons cell (body . params) if it exists
                if !cache.is_null() {
                    tracer(*cache);
                }
            }
            Value::Cons { car, cdr } => {
                tracer(*car);
                tracer(*cdr);
            }
            Value::Symbol { chars } => {
                // chars points to a Value::String, which handles its own tracing
                tracer(*chars);
            }
            Value::Lambda { data } => {
                // data points to (params . (body . env)), trace the whole structure
                tracer(*data);
            }
            Value::Array { data, len } => {
                // For non-empty arrays, trace all elements in the contiguous block
                // Empty arrays (len == 0) have data == NULL, so skip tracing
                if *len > 0 {
                    let base_idx = data.raw();
                    for i in 0..*len {
                        let elem_idx = ArenaIndex::new(base_idx + i);
                        tracer(elem_idx);
                    }
                }
            }
            Value::String { data, len } => {
                // For non-empty strings, trace all Char slots in the contiguous block
                // Empty strings (len == 0) have data == NULL, so skip tracing
                if *len > 0 {
                    let base_idx = data.raw();
                    for i in 0..*len {
                        let char_idx = ArenaIndex::new(base_idx + i);
                        tracer(char_idx);
                    }
                }
            }
        }
    }
}

// ============================================================================
// Lisp Context - Arena wrapper with helper methods
// ============================================================================

/// A Lisp execution context wrapping an arena
/// 
/// ## Reserved Slots
/// 
/// The first 4 slots of the arena are reserved for singleton values:
/// - Slot 0: `Value::Nil` - the empty list
/// - Slot 1: `Value::True` - boolean true (#t)
/// - Slot 2: `Value::False` - boolean false (#f)
/// - Slot 3: `Value::Cons` - intern table reference cell (car = intern table root)
/// 
/// These slots are pre-allocated during `Lisp::new()` and returned as
/// constants from `nil()`, `true_val()`, and `false_val()`. This optimization
/// avoids allocating new slots for these frequently-used values.
/// 
/// ## Symbol Interning
/// 
/// All symbols are interned in an association list stored in the arena.
/// The `intern_table_slot` field points to a cons cell whose car is the alist 
/// of `(string_index . symbol_index)` pairs. The cons cell is used as a 
/// "reference cell" to allow updating the intern table without RefCell.
/// When creating a symbol, we first check if it already exists in the table.
/// This ensures that the same symbol name always returns the same index.
pub struct Lisp<const N: usize> {
    arena: Arena<Value, N>,
    /// Pre-allocated Nil slot (always slot 0)
    nil_slot: ArenaIndex,
    /// Pre-allocated True slot (always slot 1)
    true_slot: ArenaIndex,
    /// Pre-allocated False slot (always slot 2)
    false_slot: ArenaIndex,
    /// Intern table reference cell (always slot 3)
    /// This is a cons cell where car = intern table root (alist)
    /// Using a cons cell avoids needing RefCell for interior mutability
    intern_table_slot: ArenaIndex,
}

/// Number of reserved slots in the arena (nil, true, false, intern_table_ref)
pub const RESERVED_SLOTS: usize = 4;

impl<const N: usize> Lisp<N> {
    /// Create a new Lisp context
    /// 
    /// Pre-allocates reserved slots for Nil, True, False, and intern table ref cell.
    /// These slots (0, 1, 2, 3) are never freed and are returned as constants
    /// from `nil()`, `true_val()`, and `false_val()`.
    /// 
    /// The intern table is initialized to nil (empty alist).
    /// 
    /// # Panics
    /// 
    /// Panics if the arena capacity N < RESERVED_SLOTS, as we need at least 4 slots
    /// for the reserved singleton values and intern table reference cell.
    pub fn new() -> Self {
        const { assert!(N >= RESERVED_SLOTS, "Lisp arena must have capacity >= RESERVED_SLOTS for reserved slots") };
        
        let arena = Arena::new(Value::Nil);
        
        // Pre-allocate reserved slots in order: Nil, True, False, InternTableRef
        // These will be slots 0, 1, 2, 3 respectively
        let nil_slot = arena.alloc(Value::Nil)
            .expect("Failed to pre-allocate reserved Nil slot during Lisp initialization");
        let true_slot = arena.alloc(Value::True)
            .expect("Failed to pre-allocate reserved True slot during Lisp initialization");
        let false_slot = arena.alloc(Value::False)
            .expect("Failed to pre-allocate reserved False slot during Lisp initialization");
        
        // Pre-allocate intern table reference cell (slot 3)
        // This is a cons cell where car = intern table root (initially nil)
        // Using a cons cell as a "reference cell" allows updating via set()
        // instead of requiring RefCell for interior mutability
        let intern_table_slot = arena.alloc(Value::Cons { car: nil_slot, cdr: nil_slot })
            .expect("Failed to pre-allocate intern table reference cell during Lisp initialization");
        
        Lisp {
            arena,
            nil_slot,
            true_slot,
            false_slot,
            intern_table_slot,
        }
    }
    
    /// Get reference to the underlying arena
    pub fn arena(&self) -> &Arena<Value, N> {
        &self.arena
    }
    
    /// Allocate a value
    #[inline]
    pub fn alloc(&self, value: Value) -> ArenaResult<ArenaIndex> {
        self.arena.alloc(value)
    }
    
    /// Get a value
    #[inline]
    pub fn get(&self, index: ArenaIndex) -> ArenaResult<Value> {
        self.arena.get(index)
    }
    
    /// Set a value
    #[inline]
    pub fn set(&self, index: ArenaIndex, value: Value) -> ArenaResult<()> {
        self.arena.set(index, value)
    }
    
    /// Get the pre-allocated Nil singleton (empty list)
    /// 
    /// This returns the reserved slot 0 which always contains `Value::Nil`.
    /// No allocation is performed.
    #[inline]
    pub fn nil(&self) -> ArenaResult<ArenaIndex> {
        Ok(self.nil_slot)
    }
    
    /// Get the pre-allocated True singleton (#t)
    /// 
    /// This returns the reserved slot 1 which always contains `Value::True`.
    /// No allocation is performed.
    #[inline]
    pub fn true_val(&self) -> ArenaResult<ArenaIndex> {
        Ok(self.true_slot)
    }
    
    /// Get the pre-allocated False singleton (#f)
    /// 
    /// This returns the reserved slot 2 which always contains `Value::False`.
    /// No allocation is performed.
    #[inline]
    pub fn false_val(&self) -> ArenaResult<ArenaIndex> {
        Ok(self.false_slot)
    }
    
    /// Allocate a boolean based on a Rust bool
    #[inline]
    pub fn boolean(&self, b: bool) -> ArenaResult<ArenaIndex> {
        if b { self.true_val() } else { self.false_val() }
    }
    
    /// Allocate an integer number
    #[inline]
    pub fn number(&self, n: isize) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Number(Number::integer(n)))
    }
    
    /// Allocate a number from the full Number type
    #[inline]
    pub fn number_value(&self, n: Number) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Number(n))
    }
    
    /// Allocate a float number
    #[inline]
    pub fn float(&self, f: f64) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Number(Number::float(f)))
    }
    
    /// Allocate a rational number
    #[inline]
    pub fn rational(&self, num: isize, denom: isize) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Number(Number::rational(num, denom)))
    }
    
    /// Allocate a complex number
    #[inline]
    pub fn complex(&self, real: f64, imag: f64) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Number(Number::complex(real, imag).normalize()))
    }
    
    /// Allocate a character
    #[inline]
    pub fn char(&self, c: char) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Char(c))
    }
    
    /// Allocate a cons cell
    #[inline]
    pub fn cons(&self, car: ArenaIndex, cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Cons { car, cdr })
    }
    
    /// Get car of a cons cell
    /// 
    /// In Scheme R7RS, car of an empty list is an error.
    #[inline]
    pub fn car(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(index)? {
            Value::Cons { car, .. } => Ok(car),
            // Scheme R7RS: car of empty list is an error
            Value::Nil => Err(ArenaError::InvalidIndex),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get cdr of a cons cell
    /// 
    /// In Scheme R7RS, cdr of an empty list is an error.
    #[inline]
    pub fn cdr(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(index)? {
            Value::Cons { cdr, .. } => Ok(cdr),
            // Scheme R7RS: cdr of empty list is an error
            Value::Nil => Err(ArenaError::InvalidIndex),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Set car of a cons cell (mutation operation)
    /// Returns the new value on success
    #[inline]
    pub fn set_car(&self, index: ArenaIndex, new_car: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(index)? {
            Value::Cons { cdr, .. } => {
                self.set(index, Value::Cons { car: new_car, cdr })?;
                Ok(new_car)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Set cdr of a cons cell (mutation operation)
    /// Returns the new value on success
    #[inline]
    pub fn set_cdr(&self, index: ArenaIndex, new_cdr: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(index)? {
            Value::Cons { car, .. } => {
                self.set(index, Value::Cons { car, cdr: new_cdr })?;
                Ok(new_cdr)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    // ========================================================================
    // Symbol Interning
    // ========================================================================
    
    /// Get the intern table root (for GC roots)
    /// 
    /// The intern table is stored in the car of the intern_table_slot cons cell.
    pub fn intern_table(&self) -> ArenaIndex {
        // intern_table_slot always exists and is slot 3
        // Its car contains the actual intern table root
        self.intern_table_slot
    }
    
    /// Get the current intern table root (the actual alist)
    fn get_intern_table_root(&self) -> ArenaResult<ArenaIndex> {
        match self.get(self.intern_table_slot)? {
            Value::Cons { car, .. } => Ok(car),
            _ => unreachable!("intern_table_slot should always be a Cons cell"),
        }
    }
    
    /// Set the intern table root (update the car of the reference cell)
    fn set_intern_table_root(&self, new_root: ArenaIndex) -> ArenaResult<()> {
        self.set(self.intern_table_slot, Value::Cons { car: new_root, cdr: self.nil_slot })
    }
    
    /// Look up a string in the intern table
    /// Returns Some(symbol_index) if found, None otherwise
    fn intern_table_lookup(&self, string_idx: ArenaIndex) -> ArenaResult<Option<ArenaIndex>> {
        let mut current = self.get_intern_table_root()?;
        
        loop {
            match self.get(current)? {
                Value::Nil => return Ok(None),
                Value::Cons { car, cdr } => {
                    // car is (string_index . symbol_index)
                    if let Value::Cons { car: entry_string, cdr: entry_symbol } = self.get(car)? {
                        if self.string_eq_contiguous(string_idx, entry_string)? {
                            return Ok(Some(entry_symbol));
                        }
                    }
                    current = cdr;
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
        }
    }
    
    /// Create or retrieve an interned symbol from a string slice
    /// 
    /// The symbol's `chars` field points to a Value::String with the symbol name.
    /// 
    /// Symbol interning ensures the same symbol name always returns the same index.
    /// 
    /// This provides ~44% memory savings compared to linked list representation.
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        // Create string for the symbol name
        let name_str = self.string(name)?;
        
        // Check intern table
        if let Some(existing_symbol) = self.intern_table_lookup(name_str)? {
            // Free the string we just created since we're using the interned one
            self.string_free(name_str)?;
            return Ok(existing_symbol);
        }
        
        // Not found - create new symbol
        let symbol = self.alloc(Value::Symbol { chars: name_str })?;
        
        // Add to intern table: (name_str . symbol)
        let binding = self.cons(name_str, symbol)?;
        let current_table = self.get_intern_table_root()?;
        let new_table = self.cons(binding, current_table)?;
        
        // Update intern table root
        self.set_intern_table_root(new_table)?;
        
        Ok(symbol)
    }
    
    /// Create or retrieve an interned symbol from bytes (for parsing)
    pub fn symbol_from_bytes(&self, bytes: &[u8]) -> ArenaResult<ArenaIndex> {
        let char_count = bytes.len();
        
        // Create a Value::String for the symbol name
        let name_str = if char_count == 0 {
            // Empty string
            self.alloc(Value::String { 
                data: ArenaIndex::NULL, 
                len: 0 
            })?
        } else {
            // Allocate contiguous block for characters
            let data = self.arena.alloc_contiguous(char_count, Value::Nil)?;
            
            // Set characters in slots
            for (i, &b) in bytes.iter().enumerate() {
                let char_idx = self.arena.index_at_offset(data, i)?;
                self.arena.set(char_idx, Value::Char(b as char))?;
            }
            
            // Create the String value
            self.alloc(Value::String { data, len: char_count })?
        };
        
        // Check intern table
        if let Some(existing_symbol) = self.intern_table_lookup(name_str)? {
            // Free the string we just created since we're using the interned one
            self.string_free(name_str)?;
            return Ok(existing_symbol);
        }
        
        // Not found - create new symbol
        let symbol = self.alloc(Value::Symbol { chars: name_str })?;
        
        // Add to intern table: (name_str . symbol)
        let binding = self.cons(name_str, symbol)?;
        let current_table = self.get_intern_table_root()?;
        let new_table = self.cons(binding, current_table)?;
        
        // Update intern table root
        self.set_intern_table_root(new_table)?;
        
        Ok(symbol)
    }
    
    /// Allocate a builtin function
    #[inline]
    pub fn builtin(&self, b: Builtin) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Builtin(b))
    }
    
    /// Allocate a stdlib function
    /// 
    /// StdLib functions are stored in static memory with on-demand parsing.
    /// The function body is parsed on first call and cached for reuse.
    #[inline]
    pub fn stdlib(&self, s: StdLib) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::StdLib { 
            func: s, 
            cache: ArenaIndex::NULL,
        })
    }
    
    /// Get cached body and params from a StdLib value.
    /// 
    /// Returns `Some((body, params))` if cached, `None` if not yet parsed.
    #[inline]
    pub fn stdlib_cache(&self, index: ArenaIndex) -> ArenaResult<Option<(ArenaIndex, ArenaIndex)>> {
        let val = self.get(index)?;
        match val {
            Value::StdLib { cache, .. } => {
                if cache.is_null() {
                    Ok(None)
                } else {
                    // cache points to (body . params)
                    let body = self.car(cache)?;
                    let params = self.cdr(cache)?;
                    Ok(Some((body, params)))
                }
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Set the cached body and params for a StdLib value.
    #[inline]
    pub fn set_stdlib_cache(&self, index: ArenaIndex, body: ArenaIndex, params: ArenaIndex) -> ArenaResult<()> {
        let val = self.get(index)?;
        match val {
            Value::StdLib { func, .. } => {
                // Create cons cell (body . params)
                let cache = self.cons(body, params)?;
                self.set(index, Value::StdLib { func, cache })
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Allocate a native function reference.
    ///
    /// Native functions are Rust functions registered with the evaluator.
    /// The `id` is the index in the NativeRegistry, and `name_hash` is
    /// a simple hash for verification.
    #[inline]
    pub fn native(&self, id: usize, name_hash: usize) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Native { id, name_hash })
    }
    
    /// Allocate a lambda
    /// 
    /// Stores lambda data as a linked structure in the arena: `(params . (body . env))`
    pub fn lambda(&self, params: ArenaIndex, body: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        // Build (body . env)
        let body_env = self.cons(body, env)?;
        // Build (params . (body . env))
        let data = self.cons(params, body_env)?;
        self.alloc(Value::Lambda { data })
    }
    
    /// Extract parts from a lambda: (params, body, env)
    /// 
    /// Lambda data is stored as `(params . (body . env))`.
    #[inline]
    pub fn lambda_parts(&self, index: ArenaIndex) -> ArenaResult<(ArenaIndex, ArenaIndex, ArenaIndex)> {
        let val = self.get(index)?;
        match val {
            Value::Lambda { data } => {
                // data = (params . (body . env))
                let params = self.car(data)?;
                let body_env = self.cdr(data)?;
                let body = self.car(body_env)?;
                let env = self.cdr(body_env)?;
                Ok((params, body, env))
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Build a list from an iterator of indices
    pub fn list<I: IntoIterator<Item = ArenaIndex>>(&self, items: I) -> ArenaResult<ArenaIndex>
    where
        I::IntoIter: DoubleEndedIterator,
    {
        let mut result = self.nil()?;
        for item in items.into_iter().rev() {
            result = self.cons(item, result)?;
        }
        Ok(result)
    }
    
    /// Get the length of a list
    pub fn list_len(&self, mut list: ArenaIndex) -> ArenaResult<usize> {
        let mut len = 0;
        loop {
            match self.get(list)? {
                Value::Nil => return Ok(len),
                Value::Cons { cdr, .. } => {
                    len += 1;
                    list = cdr;
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
        }
    }
    
    /// Check if two symbols are equal.
    /// 
    /// Symbols are compared by their underlying string content.
    #[inline]
    pub fn symbol_eq(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        // Fast path: same index means same symbol
        if a == b {
            return Ok(true);
        }
        
        let val_a = self.get(a)?;
        let val_b = self.get(b)?;
        
        match (val_a, val_b) {
            (Value::Symbol { chars: chars_a }, Value::Symbol { chars: chars_b }) => {
                self.string_eq_contiguous(chars_a, chars_b)
            }
            _ => Ok(false),
        }
    }
    
    /// Check if a symbol matches a string.
    #[inline]
    pub fn symbol_matches(&self, sym: ArenaIndex, name: &str) -> ArenaResult<bool> {
        let val = self.get(sym)?;
        
        match val {
            Value::Symbol { chars } => self.string_matches(chars, name),
            _ => Ok(false),
        }
    }
    
    /// Extract symbol name to a fixed buffer.
    pub fn symbol_to_bytes(&self, sym: ArenaIndex, buf: &mut [u8]) -> ArenaResult<usize> {
        let val = self.get(sym)?;
        
        match val {
            Value::Symbol { chars } => self.string_to_bytes(chars, buf),
            _ => Ok(0),
        }
    }
    
    /// Get the length of a symbol's name.
    pub fn symbol_len(&self, sym: ArenaIndex) -> ArenaResult<usize> {
        match self.get(sym)? {
            Value::Symbol { chars } => self.string_len(chars),
            _ => Ok(0),
        }
    }
    
    /// Get a character at a specific index within a symbol's name
    /// Returns None if the index is out of bounds or if the value is not a symbol
    pub fn symbol_char_at(&self, sym: ArenaIndex, index: usize) -> ArenaResult<Option<char>> {
        match self.get(sym)? {
            Value::Symbol { chars } => {
                let len = self.string_len(chars)?;
                if index >= len {
                    Ok(None)
                } else {
                    Ok(Some(self.string_char_at(chars, index)?))
                }
            }
            _ => Ok(None),
        }
    }
    
    /// Run garbage collection with intern table as an additional root
    /// 
    /// The intern table reference cell (slot 3) is always included as a GC root
    /// to prevent interned symbols from being collected. The intern table is
    /// stored as a cons cell whose car points to the alist of interned symbols.
    /// 
    /// # Panics
    /// 
    /// Panics if the number of roots exceeds the internal limit (512 roots).
    /// This limit is chosen to balance stack usage in no_std environments
    /// with typical program needs. Most Lisp programs use far fewer roots.
    pub fn gc(&self, roots: &[ArenaIndex]) -> GcStats {
        // Create a new roots array with reserved slots and intern table included
        // Using const-sized array to avoid alloc in no_std
        // 512 roots should be sufficient for most programs while keeping
        // stack usage reasonable (~8KB on 64-bit systems)
        const MAX_ROOTS: usize = 512;
        
        // Panic if too many roots - this indicates a programming error
        // Account for 4 reserved roots (nil, true, false, intern_table)
        assert!(roots.len() < MAX_ROOTS - 4, 
            "Too many GC roots: {} (max {})", roots.len(), MAX_ROOTS - 4 - 1);
        
        let mut all_roots = [ArenaIndex::NULL; MAX_ROOTS];
        let mut root_count = 0;
        
        // Add reserved slots as roots to prevent them from being collected
        // These slots (nil, true, false) must always be preserved
        all_roots[root_count] = self.nil_slot;
        root_count += 1;
        all_roots[root_count] = self.true_slot;
        root_count += 1;
        all_roots[root_count] = self.false_slot;
        root_count += 1;
        
        // Add intern table reference cell as root
        // This is a cons cell whose car is the intern table alist
        // Tracing from this cell will reach all interned symbols
        all_roots[root_count] = self.intern_table_slot;
        root_count += 1;
        
        // Copy provided roots
        for &root in roots {
            all_roots[root_count] = root;
            root_count += 1;
        }
        
        self.arena.collect_garbage(&all_roots[..root_count])
    }
    
    /// Allocate with GC on failure
    pub fn alloc_or_gc(&self, value: Value, roots: &[ArenaIndex]) -> ArenaResult<ArenaIndex> {
        self.arena.alloc_or_gc(value, roots)
    }
    
    /// Get arena stats
    pub fn stats(&self) -> pwn_arena::ArenaStats {
        self.arena.stats()
    }
    
    // ========================================================================
    // Contiguous String Storage
    // ========================================================================
    // 
    // Strings are stored as Value::String { data, len } pointing to
    // contiguous sequences of Value::Char in the arena:
    // [Char(c1), Char(c2), ..., Char(cn)]
    // 
    // This provides:
    // - O(1) length lookup (stored in the Value::String variant)
    // - Cache-friendly sequential access
    // - Consistent design with arrays (single source of truth for length)
    // ========================================================================
    
    /// Allocate a string as contiguous Char values.
    /// 
    /// Returns a Value::String that stores the length, with data pointing to
    /// the first character slot. Empty strings have data == NULL and len == 0.
    /// 
    /// # Memory Usage
    /// 
    /// Allocates `s.chars().count()` slots for characters, plus 1 slot for
    /// the String value itself.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::OutOfMemory` if:
    /// - No contiguous block is available
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use lisp_parser::Lisp;
    /// let lisp = Lisp::<1000>::new();
    /// let hello = lisp.string("hello").unwrap();
    /// 
    /// assert_eq!(lisp.string_len(hello).unwrap(), 5);
    /// assert_eq!(lisp.string_char_at(hello, 0).unwrap(), 'h');
    /// ```
    pub fn string(&self, s: &str) -> ArenaResult<ArenaIndex> {
        let char_count = s.chars().count();
        
        if char_count == 0 {
            // Empty string - no data slots needed
            return self.alloc(Value::String { 
                data: ArenaIndex::NULL, 
                len: 0 
            });
        }
        
        // Allocate contiguous block for characters
        let data = self.arena.alloc_contiguous(char_count, Value::Nil)?;
        
        // Set characters in slots
        for (i, c) in s.chars().enumerate() {
            let char_idx = self.arena.index_at_offset(data, i)?;
            self.arena.set(char_idx, Value::Char(c))?;
        }
        
        // Create the String value pointing to the data
        self.alloc(Value::String { data, len: char_count })
    }
    
    /// Get the length of a string.
    /// 
    /// Returns O(1) since length is stored in the String value.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if the index doesn't point to
    /// a valid string.
    pub fn string_len(&self, str_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.arena.get(str_idx)? {
            Value::String { len, .. } => Ok(len),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get a character at the given index within a string.
    /// 
    /// Character indices are 0-based. Returns O(1) access.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if:
    /// - The string index is invalid
    /// - The character index is out of bounds
    /// - The slot doesn't contain a Char value
    pub fn string_char_at(&self, str_idx: ArenaIndex, char_index: usize) -> ArenaResult<char> {
        match self.arena.get(str_idx)? {
            Value::String { data, len } => {
                if char_index >= len {
                    return Err(ArenaError::InvalidIndex);
                }
                let char_slot = self.arena.index_at_offset(data, char_index)?;
                match self.arena.get(char_slot)? {
                    Value::Char(c) => Ok(c),
                    _ => Err(ArenaError::InvalidIndex),
                }
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Compare two strings for equality.
    /// 
    /// # Errors
    /// 
    /// Returns an error if either string index is invalid.
    pub fn string_eq_contiguous(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        // Fast path: same index
        if a == b {
            return Ok(true);
        }
        
        let len_a = self.string_len(a)?;
        let len_b = self.string_len(b)?;
        
        if len_a != len_b {
            return Ok(false);
        }
        
        for i in 0..len_a {
            let char_a = self.string_char_at(a, i)?;
            let char_b = self.string_char_at(b, i)?;
            if char_a != char_b {
                return Ok(false);
            }
        }
        
        Ok(true)
    }
    
    /// Check if a string matches a Rust string slice.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the string index is invalid.
    pub fn string_matches(&self, str_idx: ArenaIndex, s: &str) -> ArenaResult<bool> {
        let len = self.string_len(str_idx)?;
        let s_len = s.chars().count();
        
        if len != s_len {
            return Ok(false);
        }
        
        for (i, expected) in s.chars().enumerate() {
            let actual = self.string_char_at(str_idx, i)?;
            if actual != expected {
                return Ok(false);
            }
        }
        
        Ok(true)
    }
    
    /// Copy a string's contents to a byte buffer.
    /// 
    /// Returns the number of bytes written. Only ASCII characters (0-127)
    /// are copied; non-ASCII characters are skipped.
    /// 
    /// # Warning
    /// 
    /// This method is designed for ASCII strings. For strings containing
    /// non-ASCII Unicode characters, some characters will be skipped and
    /// the byte count may not match the character count.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the string index is invalid.
    pub fn string_to_bytes(&self, str_idx: ArenaIndex, buf: &mut [u8]) -> ArenaResult<usize> {
        let len = self.string_len(str_idx)?;
        let mut buf_idx = 0;
        
        for i in 0..len {
            if buf_idx >= buf.len() {
                break;
            }
            let c = self.string_char_at(str_idx, i)?;
            // Only copy ASCII characters (0-127)
            if c.is_ascii() {
                buf[buf_idx] = c as u8;
                buf_idx += 1;
            }
            // Non-ASCII characters are skipped
        }
        
        Ok(buf_idx)
    }
    
    /// Free a string and all its character slots.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the string index is invalid.
    pub fn string_free(&self, str_idx: ArenaIndex) -> ArenaResult<()> {
        match self.arena.get(str_idx)? {
            Value::String { data, len } => {
                // Free the data slots
                if len > 0 {
                    self.arena.free_contiguous(data, len)?;
                }
                // Free the String value itself
                self.arena.free(str_idx)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    // ========================================================================
    // Contiguous Array Storage
    // ========================================================================
    // 
    // Arrays are stored as contiguous sequences of Value in the arena.
    // Unlike strings which have a length slot, arrays store the length in
    // the Value::Array variant itself.
    // 
    // This provides:
    // - O(1) indexed access and mutation
    // - Cache-friendly sequential access
    // - Efficient memory layout
    // ========================================================================
    
    /// Create an array with the given length, initialized with a default value.
    /// 
    /// The array stores `len` values contiguously in the arena.
    /// 
    /// # Memory Usage
    /// 
    /// Allocates `len` slots for element storage, plus 1 slot for the Array value itself.
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use lisp_parser::Lisp;
    /// let lisp = Lisp::<1000>::new();
    /// let arr = lisp.make_array(3, lisp.nil().unwrap()).unwrap();
    /// 
    /// assert_eq!(lisp.array_len(arr).unwrap(), 3);
    /// ```
    pub fn make_array(&self, len: usize, default: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if len == 0 {
            // Empty array - no data slots needed
            return self.alloc(Value::Array { 
                data: ArenaIndex::NULL, 
                len: 0 
            });
        }
        
        // Allocate contiguous block for elements
        let default_val = self.arena.get(default)?;
        let data = self.arena.alloc_contiguous(len, default_val)?;
        
        // Create the Array value pointing to the data
        self.alloc(Value::Array { data, len })
    }
    
    /// Get the length of an array.
    /// 
    /// Returns O(1) since length is stored in the Array value.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if the index doesn't point to an array.
    pub fn array_len(&self, arr_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.arena.get(arr_idx)? {
            Value::Array { len, .. } => Ok(len),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get the element at the given index within an array.
    /// 
    /// Returns O(1) access via direct index calculation.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if:
    /// - The array index is invalid
    /// - The element index is out of bounds
    pub fn array_get(&self, arr_idx: ArenaIndex, index: usize) -> ArenaResult<ArenaIndex> {
        match self.arena.get(arr_idx)? {
            Value::Array { data, len } => {
                if index >= len {
                    return Err(ArenaError::InvalidIndex);
                }
                self.arena.index_at_offset(data, index)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Set the element at the given index within an array.
    /// 
    /// Returns O(1) mutation via direct index calculation.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if:
    /// - The array index is invalid
    /// - The element index is out of bounds
    pub fn array_set(&self, arr_idx: ArenaIndex, index: usize, value: ArenaIndex) -> ArenaResult<()> {
        match self.arena.get(arr_idx)? {
            Value::Array { data, len } => {
                if index >= len {
                    return Err(ArenaError::InvalidIndex);
                }
                let elem_slot = self.arena.index_at_offset(data, index)?;
                let val = self.arena.get(value)?;
                self.arena.set(elem_slot, val)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Free an array and all its element slots.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the array index is invalid.
    pub fn array_free(&self, arr_idx: ArenaIndex) -> ArenaResult<()> {
        match self.arena.get(arr_idx)? {
            Value::Array { data, len } => {
                // Free the data slots
                if len > 0 {
                    self.arena.free_contiguous(data, len)?;
                }
                // Free the Array value itself
                self.arena.free(arr_idx)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }
}

impl<const N: usize> Default for Lisp<N> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Parser
// ============================================================================

/// Source location for error reporting
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceLoc {
    pub line: usize,
    pub column: usize,
}

/// Parser error with location
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub loc: SourceLoc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// Unexpected end of input
    UnexpectedEof,
    /// Unexpected character
    UnexpectedChar(char),
    /// Unmatched parenthesis
    UnmatchedParen,
    /// Number too large
    NumberOverflow,
    /// Arena is full
    OutOfMemory,
    /// Invalid hash literal
    InvalidHashLiteral,
    /// Invalid syntax in number
    InvalidSyntax,
}

impl ParseError {
    pub fn new(kind: ParseErrorKind, line: usize, column: usize) -> Self {
        ParseError { kind, loc: SourceLoc { line, column } }
    }
}

impl From<ArenaError> for ParseError {
    fn from(e: ArenaError) -> Self {
        ParseError {
            kind: match e {
                ArenaError::OutOfMemory => ParseErrorKind::OutOfMemory,
                _ => ParseErrorKind::OutOfMemory,
            },
            loc: SourceLoc::default(),
        }
    }
}

/// Parser state
pub struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    line: usize,
    column: usize,
}

impl<'a> Parser<'a> {
    /// Create a new parser
    pub fn new(input: &'a str) -> Self {
        Parser {
            input: input.as_bytes(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }
    
    /// Create from bytes
    pub fn from_bytes(input: &'a [u8]) -> Self {
        Parser { input, pos: 0, line: 1, column: 1 }
    }
    
    /// Get current source location
    fn loc(&self) -> SourceLoc {
        SourceLoc { line: self.line, column: self.column }
    }
    
    /// Create an error at current location
    fn error(&self, kind: ParseErrorKind) -> ParseError {
        ParseError { kind, loc: self.loc() }
    }
    
    /// Peek at the current character
    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }
    
    /// Peek at the next character (lookahead)
    fn peek_next(&self) -> Option<u8> {
        self.input.get(self.pos + 1).copied()
    }
    
    /// Advance and return the current character
    fn advance(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }
    
    /// Skip whitespace and comments
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.advance();
            } else if c == b';' {
                // Comment - skip to end of line
                while let Some(c) = self.advance() {
                    if c == b'\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }
    
    /// Check if a character can be part of a symbol
    fn is_symbol_char(c: u8) -> bool {
        matches!(c, 
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' |
            b'+' | b'-' | b'*' | b'/' | b'<' | b'>' | b'=' |
            b'?' | b'!' | b'_' | b'&' | b'%' | b'^' | b'~'
        )
    }
    
    /// Parse a single expression
    pub fn parse<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        self.skip_whitespace();
        
        match self.peek() {
            None => Err(self.error(ParseErrorKind::UnexpectedEof)),
            
            Some(b'(') => self.parse_list(lisp),
            
            Some(b')') => Err(self.error(ParseErrorKind::UnmatchedParen)),
            
            Some(b'\'') => {
                // Quote: 'x -> (quote x)
                self.advance();
                let expr = self.parse(lisp)?;
                let quote_sym = lisp.symbol("quote")?;
                let nil = lisp.nil()?;
                let quoted = lisp.cons(expr, nil)?;
                lisp.cons(quote_sym, quoted).map_err(Into::into)
            }
            
            Some(b'#') => self.parse_hash_literal(lisp),
            
            Some(c) if c.is_ascii_digit() => self.parse_number(lisp),
            
            Some(b'.') => {
                // Could be .5 (number) or a symbol
                if self.peek_next().map_or(false, |c| c.is_ascii_digit()) {
                    self.parse_number(lisp)
                } else {
                    self.parse_symbol(lisp)
                }
            }
            
            Some(b'-') => {
                // Could be negative number, -inf.0, -nan.0, -i, or symbol
                match self.peek_next() {
                    Some(c) if c.is_ascii_digit() => self.parse_number(lisp),
                    Some(b'.') => self.parse_number(lisp),  // -.5
                    Some(b'i') => self.parse_number(lisp),  // -i, -3i
                    Some(b'n') | Some(b'N') => self.parse_number(lisp),  // -nan.0
                    Some(b'I') => self.parse_number(lisp),  // -inf.0
                    _ => self.parse_symbol(lisp)
                }
            }
            
            Some(b'+') => {
                // Could be positive number, +inf.0, +nan.0, +i, or symbol
                match self.peek_next() {
                    Some(c) if c.is_ascii_digit() => self.parse_number(lisp),
                    Some(b'.') => self.parse_number(lisp),  // +.5
                    Some(b'i') => self.parse_number(lisp),  // +i, +3i
                    Some(b'n') | Some(b'N') => self.parse_number(lisp),  // +nan.0
                    Some(b'I') => self.parse_number(lisp),  // +inf.0
                    _ => self.parse_symbol(lisp)
                }
            }
            
            Some(c) if Self::is_symbol_char(c) => self.parse_symbol(lisp),
            
            Some(c) => Err(self.error(ParseErrorKind::UnexpectedChar(c as char))),
        }
    }
    
    /// Parse hash literals (#t, #f, #e, #i, #b, #o, #d, #x, #\char, etc.)
    fn parse_hash_literal<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        // Don't consume # yet - parse_number handles prefixes
        
        match self.peek_at(1) {
            // Boolean literals
            Some(b't') | Some(b'T') => {
                self.advance(); // #
                self.advance(); // t
                // Check for #true
                if self.peek() == Some(b'r') {
                    if self.matches_bytes(b"rue") {
                        return lisp.true_val().map_err(Into::into);
                    }
                }
                lisp.true_val().map_err(Into::into)
            }
            Some(b'f') | Some(b'F') => {
                self.advance(); // #
                self.advance(); // f
                // Check for #false
                if self.peek() == Some(b'a') {
                    if self.matches_bytes(b"alse") {
                        return lisp.false_val().map_err(Into::into);
                    }
                }
                lisp.false_val().map_err(Into::into)
            }
            // Number prefixes - delegate to parse_number
            Some(b'e') | Some(b'E') | Some(b'i') | Some(b'I') |
            Some(b'b') | Some(b'B') | Some(b'o') | Some(b'O') |
            Some(b'd') | Some(b'D') | Some(b'x') | Some(b'X') => {
                self.parse_number(lisp)
            }
            // Character literal
            Some(b'\\') => {
                self.advance(); // #
                self.advance(); // \
                self.parse_character_literal(lisp)
            }
            Some(_) => Err(self.error(ParseErrorKind::InvalidHashLiteral)),
            None => Err(self.error(ParseErrorKind::UnexpectedEof)),
        }
    }
    
    /// Parse a character literal after #\
    fn parse_character_literal<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        // Try named characters first
        if let Some(c) = self.try_parse_named_char()? {
            return lisp.char(c).map_err(Into::into);
        }
        
        // Single character
        match self.peek() {
            Some(c) => {
                self.advance();
                lisp.char(c as char).map_err(Into::into)
            }
            None => Err(self.error(ParseErrorKind::UnexpectedEof)),
        }
    }
    
    /// Try to parse named character constants
    fn try_parse_named_char(&mut self) -> Result<Option<char>, ParseError> {
        let start = self.pos;
        
        // Read identifier
        let mut buf = [0u8; 16];
        let mut len = 0;
        while let Some(c) = self.peek() {
            if Self::is_symbol_char(c) && len < 16 {
                buf[len] = c.to_ascii_lowercase();
                len += 1;
                self.advance();
            } else {
                break;
            }
        }
        
        if len == 0 {
            return Ok(None);
        }
        
        let name = &buf[..len];
        match name {
            b"space" => Ok(Some(' ')),
            b"newline" => Ok(Some('\n')),
            b"tab" => Ok(Some('\t')),
            b"return" => Ok(Some('\r')),
            b"null" => Ok(Some('\0')),
            b"alarm" => Ok(Some('\x07')),
            b"backspace" => Ok(Some('\x08')),
            b"delete" => Ok(Some('\x7f')),
            b"escape" => Ok(Some('\x1b')),
            _ => {
                // Not a named char - if single character, return that
                if len == 1 {
                    self.pos = start + 1;
                    Ok(Some(buf[0] as char))
                } else {
                    self.pos = start;
                    Ok(None)
                }
            }
        }
    }
    
    /// Parse a list (including nil)
    fn parse_list<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        self.advance(); // consume '('
        self.skip_whitespace();
        
        if self.peek() == Some(b')') {
            self.advance();
            return lisp.nil().map_err(Into::into);
        }
        
        // Parse elements and build list
        const MAX_LIST_DEPTH: usize = 128;
        let mut elements: [ArenaIndex; MAX_LIST_DEPTH] = [ArenaIndex::NULL; MAX_LIST_DEPTH];
        let mut count = 0;
        
        loop {
            self.skip_whitespace();
            
            match self.peek() {
                None => return Err(self.error(ParseErrorKind::UnexpectedEof)),
                Some(b')') => {
                    self.advance();
                    break;
                }
                Some(b'.') => {
                    // Dotted pair: (a . b)
                    self.advance();
                    self.skip_whitespace();
                    
                    if count == 0 {
                        return Err(self.error(ParseErrorKind::UnexpectedChar('.')));
                    }
                    
                    let cdr = self.parse(lisp)?;
                    self.skip_whitespace();
                    
                    if self.advance() != Some(b')') {
                        return Err(self.error(ParseErrorKind::UnmatchedParen));
                    }
                    
                    // Build the dotted list
                    let mut result = cdr;
                    for i in (0..count).rev() {
                        result = lisp.cons(elements[i], result)?;
                    }
                    return Ok(result);
                }
                Some(_) => {
                    if count >= MAX_LIST_DEPTH {
                        return Err(self.error(ParseErrorKind::OutOfMemory));
                    }
                    elements[count] = self.parse(lisp)?;
                    count += 1;
                }
            }
        }
        
        // Build proper list
        let mut result = lisp.nil()?;
        for i in (0..count).rev() {
            result = lisp.cons(elements[i], result)?;
        }
        Ok(result)
    }
    
    /// Parse a number (full numerical tower support)
    /// 
    /// Supports:
    /// - Integers: 42, -17, #b1010, #o755, #xFF
    /// - Rationals: 3/4, -1/3
    /// - Floats: 3.14, 1.0e10, .5, 5.
    /// - Special floats: +inf.0, -inf.0, +nan.0, -nan.0
    /// - Complex rectangular: 3+4i, 1-2i, +3i, -4i, 3+i, 3-i
    /// - Complex polar: 1@0.5
    /// - Exactness prefixes: #e (exact), #i (inexact)
    fn parse_number<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        // Parse optional prefixes (#e, #i, #b, #o, #d, #x)
        let mut force_exact = false;
        let mut force_inexact = false;
        let mut radix = 10u8;
        
        // Check for prefixes (can have exactness and radix in any order)
        while self.peek() == Some(b'#') {
            let saved_pos = self.pos;
            self.advance();
            match self.peek().map(|c| c.to_ascii_lowercase()) {
                Some(b'e') => { self.advance(); force_exact = true; }
                Some(b'i') => { self.advance(); force_inexact = true; }
                Some(b'b') => { self.advance(); radix = 2; }
                Some(b'o') => { self.advance(); radix = 8; }
                Some(b'd') => { self.advance(); radix = 10; }
                Some(b'x') => { self.advance(); radix = 16; }
                _ => {
                    self.pos = saved_pos;
                    break;
                }
            }
        }
        
        // Check for special values: +inf.0, -inf.0, +nan.0, -nan.0
        if let Some(special) = self.try_parse_special_float()? {
            return lisp.float(special).map_err(Into::into);
        }
        
        // Parse sign
        let negative = match self.peek() {
            Some(b'-') => { self.advance(); true }
            Some(b'+') => { self.advance(); false }
            _ => false
        };
        
        // Check for pure imaginary: +i, -i
        if self.peek() == Some(b'i') && (self.peek_at(1).is_none() || self.is_delimiter(self.peek_at(1).unwrap_or(b' '))) {
            self.advance();
            let imag = if negative { -1.0 } else { 1.0 };
            return lisp.complex(0.0, imag).map_err(Into::into);
        }
        
        // Parse integer part
        let (int_part, _has_int) = self.parse_integer_part(radix)?;
        
        // Check for decimal point (only valid for radix 10)
        let (frac_part, has_frac) = if radix == 10 && self.peek() == Some(b'.') {
            self.advance();
            let (frac, frac_digits) = self.parse_fractional_part()?;
            (frac / pow10(frac_digits as i32), true)
        } else {
            (0.0, false)
        };
        
        // Check for exponent (only valid for radix 10)
        let exp_part = if radix == 10 && matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.advance();
            self.parse_exponent()?
        } else {
            0
        };
        
        // Check for rational denominator
        if self.peek() == Some(b'/') {
            self.advance();
            let (denom, _) = self.parse_integer_part(radix)?;
            if denom == 0 {
                return lisp.float(f64::NAN).map_err(Into::into);
            }
            let num = if negative { -(int_part as isize) } else { int_part as isize };
            
            if force_inexact {
                return lisp.float(num as f64 / denom as f64).map_err(Into::into);
            }
            return lisp.rational(num, denom as isize).map_err(Into::into);
        }
        
        // Check for complex (rectangular: +/- imaginary part, or polar: @angle)
        if self.peek() == Some(b'@') {
            // Polar form: r@theta
            self.advance();
            let angle_negative = match self.peek() {
                Some(b'-') => { self.advance(); true }
                Some(b'+') => { self.advance(); false }
                _ => false
            };
            let angle = self.parse_real_magnitude(radix)?;
            let angle = if angle_negative { -angle } else { angle };
            let r = self.build_real(int_part, frac_part, exp_part, negative);
            let real = r * libm::cos(angle);
            let imag = r * libm::sin(angle);
            return lisp.complex(real, imag).map_err(Into::into);
        }
        
        // Check for complex rectangular imaginary part
        if matches!(self.peek(), Some(b'+') | Some(b'-')) {
            let imag_negative = self.peek() == Some(b'-');
            self.advance();
            
            // Check for +i or -i (imaginary unit)
            if self.peek() == Some(b'i') && (self.peek_at(1).is_none() || self.is_delimiter(self.peek_at(1).unwrap_or(b' '))) {
                self.advance();
                let real = self.build_real(int_part, frac_part, exp_part, negative);
                let imag = if imag_negative { -1.0 } else { 1.0 };
                return lisp.complex(real, imag).map_err(Into::into);
            }
            
            let imag = self.parse_real_magnitude(radix)?;
            
            // Must end with 'i'
            if self.peek() != Some(b'i') {
                return Err(self.error(ParseErrorKind::InvalidSyntax));
            }
            self.advance();
            
            let real = self.build_real(int_part, frac_part, exp_part, negative);
            let imag = if imag_negative { -imag } else { imag };
            return lisp.complex(real, imag).map_err(Into::into);
        }
        
        // Check for pure imaginary with magnitude: 3i
        if self.peek() == Some(b'i') {
            self.advance();
            let imag = self.build_real(int_part, frac_part, exp_part, negative);
            return lisp.complex(0.0, imag).map_err(Into::into);
        }
        
        // Build the final number
        if has_frac || exp_part != 0 || force_inexact {
            // Inexact (float)
            let value = self.build_real(int_part, frac_part, exp_part, negative);
            if force_exact {
                // Convert to exact rational
                let n = Number::float(value).to_exact();
                lisp.number_value(n).map_err(Into::into)
            } else {
                lisp.float(value).map_err(Into::into)
            }
        } else {
            // Exact integer
            let value = if negative { -(int_part as isize) } else { int_part as isize };
            if force_inexact {
                lisp.float(value as f64).map_err(Into::into)
            } else {
                lisp.number(value).map_err(Into::into)
            }
        }
    }
    
    /// Try to parse special float constants: +inf.0, -inf.0, +nan.0, -nan.0
    fn try_parse_special_float(&mut self) -> Result<Option<f64>, ParseError> {
        let start = self.pos;
        
        // Check for sign
        let sign = match self.peek() {
            Some(b'+') => { self.advance(); 1.0 }
            Some(b'-') => { self.advance(); -1.0 }
            _ => return Ok(None)
        };
        
        // Try to match "inf.0" or "nan.0"
        if self.matches_bytes(b"inf.0") {
            return Ok(Some(sign * f64::INFINITY));
        }
        if self.matches_bytes(b"nan.0") {
            return Ok(Some(f64::NAN));
        }
        
        // Not a special float, reset position
        self.pos = start;
        Ok(None)
    }
    
    /// Check if the current position matches the given bytes (case-insensitive)
    fn matches_bytes(&mut self, bytes: &[u8]) -> bool {
        for (i, &b) in bytes.iter().enumerate() {
            match self.peek_at(i) {
                Some(c) if c.to_ascii_lowercase() == b.to_ascii_lowercase() => {}
                _ => return false
            }
        }
        // All matched, advance
        for _ in 0..bytes.len() {
            self.advance();
        }
        true
    }
    
    /// Peek at a specific offset from current position
    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.input.get(self.pos + offset).copied()
    }
    
    /// Parse an integer in the given radix
    fn parse_integer_part(&mut self, radix: u8) -> Result<(usize, bool), ParseError> {
        let mut value: usize = 0;
        let mut has_digits = false;
        
        while let Some(c) = self.peek() {
            let digit = match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => break,
            };
            
            if digit >= radix {
                break;
            }
            
            self.advance();
            has_digits = true;
            value = value.checked_mul(radix as usize)
                .and_then(|v| v.checked_add(digit as usize))
                .ok_or_else(|| self.error(ParseErrorKind::NumberOverflow))?;
        }
        
        Ok((value, has_digits))
    }
    
    /// Parse fractional part after decimal point
    fn parse_fractional_part(&mut self) -> Result<(f64, usize), ParseError> {
        let mut value = 0.0;
        let mut digits = 0usize;
        
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
                value = value * 10.0 + (c - b'0') as f64;
                digits += 1;
            } else {
                break;
            }
        }
        
        Ok((value, digits))
    }
    
    /// Parse exponent part
    fn parse_exponent(&mut self) -> Result<i32, ParseError> {
        let negative = match self.peek() {
            Some(b'-') => { self.advance(); true }
            Some(b'+') => { self.advance(); false }
            _ => false
        };
        
        let mut exp: i32 = 0;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
                exp = exp.saturating_mul(10).saturating_add((c - b'0') as i32);
            } else {
                break;
            }
        }
        
        Ok(if negative { -exp } else { exp })
    }
    
    /// Parse a real number magnitude (for complex parts)
    fn parse_real_magnitude(&mut self, radix: u8) -> Result<f64, ParseError> {
        let (int_part, _) = self.parse_integer_part(radix)?;
        
        let frac_part = if radix == 10 && self.peek() == Some(b'.') {
            self.advance();
            let (frac, frac_digits) = self.parse_fractional_part()?;
            frac / pow10(frac_digits as i32)
        } else {
            0.0
        };
        
        let exp_part = if radix == 10 && matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.advance();
            self.parse_exponent()?
        } else {
            0
        };
        
        // Check for rational
        if self.peek() == Some(b'/') {
            self.advance();
            let (denom, _) = self.parse_integer_part(radix)?;
            if denom == 0 {
                return Ok(f64::NAN);
            }
            return Ok(int_part as f64 / denom as f64);
        }
        
        Ok(self.build_real(int_part, frac_part, exp_part, false))
    }
    
    /// Build a real number from components
    fn build_real(&self, int_part: usize, frac_part: f64, exp_part: i32, negative: bool) -> f64 {
        let value = (int_part as f64 + frac_part) * pow10(exp_part);
        if negative { -value } else { value }
    }
    
    /// Check if character is a delimiter
    fn is_delimiter(&self, c: u8) -> bool {
        matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'(' | b')' | b'"' | b';' | b'[' | b']' | b'{' | b'}')
    }
}

/// Power of 10 helper (no_std compatible)
fn pow10(exp: i32) -> f64 {
    libm::pow(10.0, exp as f64)
}

impl<'a> Parser<'a> {
    
    /// Parse a symbol
    fn parse_symbol<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        const MAX_SYMBOL_LEN: usize = 64;
        let mut buffer: [u8; MAX_SYMBOL_LEN] = [0; MAX_SYMBOL_LEN];
        let mut len = 0;
        
        while let Some(c) = self.peek() {
            if Self::is_symbol_char(c) && len < MAX_SYMBOL_LEN {
                buffer[len] = c.to_ascii_lowercase();
                len += 1;
                self.advance();
            } else {
                break;
            }
        }
        
        let name = &buffer[..len];
        
        // Note: In Scheme, nil is just a regular symbol.
        // The empty list is written as () or '() only.
        // No special handling for 'nil' - it's parsed as a regular symbol.
        
        lisp.symbol_from_bytes(name).map_err(Into::into)
    }
    
    /// Check if there's more input (after whitespace)
    pub fn has_more(&mut self) -> bool {
        self.skip_whitespace();
        self.peek().is_some()
    }
    
    /// Get current position for error reporting
    pub fn position(&self) -> (usize, usize) {
        (self.line, self.column)
    }
}

/// Parse a string into a Lisp expression
pub fn parse<const N: usize>(lisp: &Lisp<N>, input: &str) -> Result<ArenaIndex, ParseError> {
    let mut parser = Parser::new(input);
    parser.parse(lisp)
}

/// Parse multiple expressions
pub fn parse_all<const N: usize>(lisp: &Lisp<N>, input: &str) -> Result<ArenaIndex, ParseError> {
    let mut parser = Parser::new(input);
    let mut results = [ArenaIndex::NULL; 64];
    let mut count = 0;
    
    while parser.has_more() && count < 64 {
        results[count] = parser.parse(lisp)?;
        count += 1;
    }
    
    // Build list of results
    let mut result = lisp.nil()?;
    for i in (0..count).rev() {
        result = lisp.cons(results[i], result)?;
    }
    Ok(result)
}

