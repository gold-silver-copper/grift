//! Value types for the Lisp parser
//!
//! This module contains the core Value enum and related types like Builtin and StdLib.

use pwn_arena::{ArenaIndex, Trace};

/// Macro for defining built-in functions.
/// 
/// This macro generates the `Builtin` enum, its `name()` method, and the `ALL` constant
/// from a single declarative definition. To add a new builtin, simply add a new entry
/// to the macro invocation (and implement its evaluation in grift_eval).
/// 
/// # Syntax
/// 
/// ```rust
/// use grift_parser::define_builtins;
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
/// use grift_parser::define_builtins;
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
        }
    };
}

// Define all built-in functions using the macro.
// To add a new builtin, add an entry here and implement its evaluation in grift_eval.
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
    
    // Vector operations (R7RS Section 6.8)
    /// vector? - Check if value is a vector
    Vectorp => "vector?",
    /// make-vector - Create a vector with optional fill value
    MakeVector => "make-vector",
    /// vector - Create vector from arguments
    Vector => "vector",
    /// vector-length - Get length of vector
    VectorLength => "vector-length",
    /// vector-ref - Get element at index
    VectorRef => "vector-ref",
    /// vector-set! - Set element at index
    VectorSet => "vector-set!",
    /// vector->list - Convert vector to list
    VectorToList => "vector->list",
    /// list->vector - Convert list to vector
    ListToVector => "list->vector",
    /// vector-fill! - Fill vector with value
    VectorFill => "vector-fill!",
    /// vector-copy - Copy a vector
    VectorCopy => "vector-copy",
    
    // Character operations (R7RS Section 6.6)
    /// char? - Check if value is a character
    Charp => "char?",
    /// char=? - Character equality
    CharEq => "char=?",
    /// char<? - Character less than
    CharLt => "char<?",
    /// char>? - Character greater than
    CharGt => "char>?",
    /// char<=? - Character less than or equal
    CharLe => "char<=?",
    /// char>=? - Character greater than or equal
    CharGe => "char>=?",
    /// char->integer - Convert character to its Unicode code point
    CharToInteger => "char->integer",
    /// integer->char - Convert Unicode code point to character
    IntegerToChar => "integer->char",
    /// char-upcase - Convert character to uppercase
    CharUpcase => "char-upcase",
    /// char-downcase - Convert character to lowercase
    CharDowncase => "char-downcase",
    
    // String operations (R7RS Section 6.7)
    /// string? - Check if value is a string
    Stringp => "string?",
    /// make-string - Create a string of given length
    MakeString => "make-string",
    /// string - Create string from characters
    String => "string",
    /// string-length - Get length of string
    StringLength => "string-length",
    /// string-ref - Get character at index
    StringRef => "string-ref",
    /// string-set! - Set character at index
    StringSet => "string-set!",
    /// string=? - String equality
    StringEq => "string=?",
    /// string<? - String less than
    StringLt => "string<?",
    /// string>? - String greater than
    StringGt => "string>?",
    /// string<=? - String less than or equal
    StringLe => "string<=?",
    /// string>=? - String greater than or equal
    StringGe => "string>=?",
    /// string-append - Concatenate strings
    StringAppend => "string-append",
    /// string->list - Convert string to list of characters
    StringToList => "string->list",
    /// list->string - Convert list of characters to string
    ListToString => "list->string",
    /// substring - Extract a substring
    Substring => "substring",
    /// string-copy - Copy a string
    StringCopy => "string-copy",
    
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
/// use grift_parser::define_stdlib;
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
/// use grift_parser::define_stdlib;
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
grift_macros::include_stdlib!("src/stdlib.scm");

/// A Lisp value
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// The empty list (NOT false - use False for that)
    Nil,
    
    /// Boolean true (#t)
    True,
    
    /// Boolean false (#f) - the ONLY false value
    False,
    
    /// Integer number (exact)
    Number(isize),
    
    /// Floating-point number (inexact)
    Float(f64),
    
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
    
    /// Standard library function (stored in static memory)
    /// 
    /// Unlike Lambda which stores code in the arena, StdLib references static
    /// function definitions. The function body is parsed on each call.
    /// 
    /// # Memory Efficiency
    /// 
    /// - Function definitions are in static memory (const strings)
    /// - No arena allocation for the function definition itself
    /// - Parsed AST is temporary and GC'd after evaluation
    StdLib(StdLib),
    
    /// Vector/Array (contiguous storage of values in the arena)
    /// 
    /// Vectors store values contiguously in the arena, similar to how symbols
    /// store characters. This provides O(1) indexed access and mutation.
    /// Used internally for R7RS vector operations (Section 6.8).
    /// 
    /// # Memory Layout
    /// 
    /// - `data` points to a `Value::Number(len)` header followed by elements
    /// - Elements are stored at data+1, data+2, ..., data+len
    /// - Empty arrays have data == NULL (len=0)
    /// 
    /// # Example
    /// 
    /// ```lisp
    /// (define vec (make-vector 3 0))  ; Create vector of 3 zeros
    /// (vector-set! vec 1 42)          ; Set index 1 to 42
    /// (vector-ref vec 1)              ; => 42
    /// (vector-length vec)             ; => 3
    /// #(1 2 3)                        ; Vector literal syntax
    /// ```
    Array {
        data: ArenaIndex,  // Points to Number(len) header, elements follow at data+1
    },
    
    /// String (contiguous storage of Char values in the arena)
    /// 
    /// Strings store characters contiguously in the arena, similar to vectors.
    /// This provides O(1) indexed access and O(1) length lookup.
    /// 
    /// # Memory Layout
    /// 
    /// - `data` points to a `Value::Number(len)` header followed by characters
    /// - Characters are stored at data+1, data+2, ..., data+len
    /// - Empty strings have data == NULL (len=0)
    /// 
    /// # Example
    /// 
    /// ```lisp
    /// (string-length "hello")   ; => 5
    /// (string-ref "hello" 0)    ; => #\h
    /// ```
    String {
        data: ArenaIndex,  // Points to Number(len) header, chars follow at data+1
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
    
    /// Raw arena index reference
    /// 
    /// Used internally for storing arena indices in contiguous blocks.
    /// This allows other variants (like Cons) to store their references
    /// in the arena rather than inline, enabling memory optimizations.
    /// 
    /// # Note
    /// 
    /// This is an internal implementation detail and should not be
    /// exposed to Lisp code directly. It's traced by the GC like any
    /// other reference.
    Ref(ArenaIndex),
    
    /// Unsigned integer (internal use)
    /// 
    /// Used for storing unsigned values like array lengths, indices,
    /// or other internal counters that need the full positive range
    /// of a machine word.
    /// 
    /// # Note
    /// 
    /// This is primarily an internal implementation detail. For user-facing
    /// integers, prefer `Number(isize)` which supports negative values.
    Usize(usize),
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
    
    /// Check if this value is a number (integer or float)
    #[inline]
    pub const fn is_number(&self) -> bool {
        matches!(self, Value::Number(_) | Value::Float(_))
    }
    
    /// Check if this value is an integer
    #[inline]
    pub const fn is_integer(&self) -> bool {
        matches!(self, Value::Number(_))
    }
    
    /// Check if this value is a float
    #[inline]
    pub const fn is_float(&self) -> bool {
        matches!(self, Value::Float(_))
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
        matches!(self, Value::StdLib(_))
    }
    
    /// Check if this value is a native (Rust) function
    #[inline]
    pub const fn is_native(&self) -> bool {
        matches!(self, Value::Native { .. })
    }
    
    /// Check if this value is a procedure (lambda, builtin, stdlib, or native function)
    #[inline]
    pub const fn is_procedure(&self) -> bool {
        matches!(self, Value::Lambda { .. } | Value::Builtin(_) | Value::StdLib(_) | Value::Native { .. })
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
    
    /// Check if this value is a ref (internal arena index reference)
    #[inline]
    pub const fn is_ref(&self) -> bool {
        matches!(self, Value::Ref(_))
    }
    
    /// Get the number value if this is an integer
    #[inline]
    pub const fn as_number(&self) -> Option<isize> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
    
    /// Get the float value if this is a float
    #[inline]
    pub const fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }
    
    /// Get any numeric value as f64
    #[inline]
    pub const fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n as f64),
            Value::Float(f) => Some(*f),
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
    
    /// Get the arena index if this is a ref
    #[inline]
    pub const fn as_ref(&self) -> Option<ArenaIndex> {
        match self {
            Value::Ref(idx) => Some(*idx),
            _ => None,
        }
    }
    
    /// Check if this value is a usize (internal unsigned integer)
    #[inline]
    pub const fn is_usize(&self) -> bool {
        matches!(self, Value::Usize(_))
    }
    
    /// Get the usize value if this is a Usize
    #[inline]
    pub const fn as_usize(&self) -> Option<usize> {
        match self {
            Value::Usize(n) => Some(*n),
            _ => None,
        }
    }
    
    /// Get a human-readable type name
    pub const fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::True | Value::False => "boolean",
            Value::Number(_) => "number",
            Value::Float(_) => "number",
            Value::Char(_) => "char",
            Value::Cons { .. } => "pair",
            Value::Symbol { .. } => "symbol",
            Value::Lambda { .. } => "procedure",
            Value::Builtin(_) => "procedure",
            Value::StdLib(_) => "procedure",
            Value::Native { .. } => "native",
            Value::Array { .. } => "array",
            Value::String { .. } => "string",
            Value::Ref(_) => "ref",
            Value::Usize(_) => "usize",
        }
    }
}

/// Implement Trace for GC support
impl<const N: usize> Trace<Value, N> for Value {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match self {
            Value::Nil | Value::True | Value::False | 
            Value::Number(_) | Value::Float(_) | Value::Char(_) | Value::Builtin(_) |
            Value::Native { .. } | Value::StdLib(_) | Value::Usize(_) => {
                // No references
            }
            Value::Ref(idx) => {
                // Trace the referenced value
                tracer(*idx);
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
            Value::Array { data } | Value::String { data } => {
                // For non-empty arrays/strings, we need arena access to read the length.
                // The basic trace just marks the data pointer; trace_with_arena handles
                // the full tracing with element traversal.
                if !data.is_null() {
                    tracer(*data);
                }
            }
        }
    }
    
    fn trace_with_arena<F: FnMut(ArenaIndex)>(&self, arena: &pwn_arena::Arena<Value, N>, mut tracer: F) {
        match self {
            Value::Array { data } => {
                // For non-empty arrays, trace the length header and all elements
                // Empty arrays have data == NULL, so skip tracing
                if !data.is_null() {
                    // Trace the length header at data
                    tracer(*data);
                    
                    // Read the length from the arena and trace all elements
                    if let Ok(Value::Number(len)) = arena.get(*data) {
                        let base_idx = data.raw();
                        for i in 0..(len as usize) {
                            // Elements are at data+1, data+2, ..., data+len
                            let elem_idx = ArenaIndex::new(base_idx + 1 + i);
                            tracer(elem_idx);
                        }
                    }
                }
            }
            Value::String { data } => {
                // For non-empty strings, trace the length header and all Char slots
                // Empty strings have data == NULL, so skip tracing
                if !data.is_null() {
                    // Trace the length header at data
                    tracer(*data);
                    
                    // Read the length from the arena and trace all character slots
                    if let Ok(Value::Number(len)) = arena.get(*data) {
                        let base_idx = data.raw();
                        for i in 0..(len as usize) {
                            // Characters are at data+1, data+2, ..., data+len
                            let char_idx = ArenaIndex::new(base_idx + 1 + i);
                            tracer(char_idx);
                        }
                    }
                }
            }
            // For all other types, delegate to the standard trace
            _ => <Value as Trace<Value, N>>::trace(self, tracer),
        }
    }
}
