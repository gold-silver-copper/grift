//! Value types for the Lisp parser
//!
//! This module contains the core Value enum and related types like Builtin and StdLib.
//!
//! Note: The `define_builtins!` and `define_stdlib!` macros have been moved to `src/macros.rs`.

use grift_arena::{ArenaIndex, Trace};

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
    
    // Syntax-case support (R6RS Chapter 11)
    /// identifier? - Check if value is an identifier (symbol or syntax-wrapped symbol)
    Identifierp => "identifier?",
    /// bound-identifier=? - Check if two identifiers have the same name and marks
    BoundIdentifierEq => "bound-identifier=?",
    /// free-identifier=? - Check if two identifiers resolve to the same binding
    FreeIdentifierEq => "free-identifier=?",
    /// syntax->datum - Strip syntax wrapper to get the underlying datum
    SyntaxToDatum => "syntax->datum",
    /// datum->syntax - Wrap a datum with syntax context from a template identifier
    DatumToSyntax => "datum->syntax",
    /// generate-temporaries - Generate a list of fresh identifiers
    GenerateTemporaries => "generate-temporaries",
}

// Define all standard library functions using the include_stdlib! macro.
// This macro reads the stdlib.scm file and generates the StdLib enum.
// To add a new function, simply add a new entry in stdlib.scm.
// Note: member/assoc use eq? for comparison (like Scheme's memq/assq).
// This works for symbols and identical objects. For value comparison,
// define a custom function or use fold with a predicate.
grift_macros::include_stdlib!("src/stdlib.scm");

/// A Lisp value
/// 
/// # Memory Optimization
/// 
/// This enum inlines fixed-size data directly into variants to save arena slots
/// and improve cache locality. Variable-length data (strings, arrays) still uses
/// arena storage but with inline length for O(1) access.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// The empty list (NOT false - use False for that)
    Nil,
    
    /// Boolean true (#t)
    True,
    
    /// Boolean false (#f) - the ONLY false value
    False,
    
    /// Integer number
    Number(isize),
    
    /// Single character (used in strings and symbol storage)
    Char(char),
    
    /// Cons cell (pair) with inline car/cdr indices
    /// 
    /// Both car and cdr are stored inline, saving 2 arena slots per cons.
    /// 
    /// # Memory Savings
    /// 
    /// Previously: 1 slot for Cons + 2 slots for [Ref(car), Ref(cdr)] = 3 slots
    /// Now: 1 slot for Cons with inline car/cdr = 1 slot (saves 2 slots)
    /// 
    /// Use `Lisp::cons()`, `Lisp::car()`, `Lisp::cdr()` to create and access.
    Cons { car: ArenaIndex, cdr: ArenaIndex },
    
    /// Symbol (contains a contiguous string)
    /// Points to a Value::String which contains the symbol name.
    /// The length is obtained from the String value, providing a single source of truth.
    Symbol(ArenaIndex),
    
    /// Lambda / closure with inline params and body_env indices
    /// 
    /// - `params`: ArenaIndex to list of parameter symbols
    /// - `body_env`: ArenaIndex to a cons cell containing (body . env)
    /// 
    /// # Memory Savings
    /// 
    /// Previously: 1 slot for Lambda + 3 slots for [Ref(params), Ref(body), Ref(env)] = 4 slots
    /// Now: 1 slot for Lambda with inline params/body_env + 1 cons for body.env = 2 slots (saves 2 slots)
    /// 
    /// Use `Lisp::lambda()` to create and `Lisp::lambda_parts()` to extract.
    Lambda { params: ArenaIndex, body_env: ArenaIndex },
    
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
    
    /// Vector/Array with inline length and data pointer
    /// 
    /// Vectors store values contiguously in the arena. The length is inlined
    /// for O(1) access, saving 1 arena slot per array.
    /// 
    /// # Memory Layout
    /// 
    /// - `len`: Number of elements (inline)
    /// - `data`: Points directly to first element (no length header in arena)
    /// - Empty arrays have len=0 and data == NIL
    /// 
    /// # Memory Savings
    /// 
    /// Previously: 1 slot for Array + Number(len) header + elements
    /// Now: 1 slot for Array with inline len + elements only (saves 1 slot)
    /// 
    /// # Example
    /// 
    /// ```lisp
    /// (define vec (make-vector 3 0))  ; Create vector of 3 zeros
    /// (vector-set! vec 1 42)          ; Set index 1 to 42
    /// (vector-ref vec 1)              ; => 42
    /// (vector-length vec)             ; => 3 (O(1) - inline!)
    /// #(1 2 3)                        ; Vector literal syntax
    /// ```
    Array { len: usize, data: ArenaIndex },
    
    /// String with inline length and data pointer
    /// 
    /// Strings store characters contiguously in the arena. The length is inlined
    /// for O(1) access, saving 1 arena slot per string.
    /// 
    /// # Memory Layout
    /// 
    /// - `len`: Number of characters (inline)
    /// - `data`: Points directly to first Char value (no length header in arena)
    /// - Empty strings have len=0 and data == NIL
    /// 
    /// # Memory Savings
    /// 
    /// Previously: 1 slot for String + Number(len) header + chars
    /// Now: 1 slot for String with inline len + chars only (saves 1 slot)
    /// 
    /// # Example
    /// 
    /// ```lisp
    /// (string-length "hello")   ; => 5 (O(1) - inline!)
    /// (string-ref "hello" 0)    ; => #\h
    /// ```
    String { len: usize, data: ArenaIndex },
    
    /// Native function with inline id and name_hash
    ///
    /// Native functions are registered at runtime and identified by their ID.
    /// The actual function pointer is stored in the evaluator's NativeRegistry.
    ///
    /// # Memory Savings
    /// 
    /// Previously: 1 slot for Native + 2 slots for [Usize(id), Usize(name_hash)] = 3 slots
    /// Now: 1 slot for Native with inline id/name_hash = 1 slot (saves 2 slots)
    Native { id: usize, name_hash: usize },
    
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
    
    /// Syntax-rules macro transformer
    ///
    /// Stores a compiled macro with literals, rules, and definition environment.
    /// Following the same 2-index constraint as Lambda, we pack fields into cons cells.
    ///
    /// # Memory Layout
    ///
    /// - `literals`: ArenaIndex to list of literal keyword symbols
    /// - `rules_env`: ArenaIndex to cons cell (rules . definition_env)
    ///   - car: list of (pattern . template) pairs
    ///   - cdr: environment where macro was defined (for hygiene)
    ///
    /// This matches Lambda's layout: 2 inline ArenaIndex fields = 1 arena slot.
    ///
    /// # Example
    ///
    /// ```scheme
    /// (define-syntax when
    ///   (syntax-rules ()
    ///     ((when test body ...)
    ///      (if test (begin body ...)))))
    /// ```
    ///
    /// Stored as:
    /// - literals: ()
    /// - rules_env: (rules_list . global_env)
    ///   where rules_list = (((when test body ...) . (if test (begin body ...))))
    SyntaxRules {
        literals: ArenaIndex,   // list of literal keyword symbols
        rules_env: ArenaIndex,  // cons cell: (rules . definition_env)
    },
    
    /// Syntax object for procedural macros (syntax-case)
    ///
    /// A syntax object wraps an expression with lexical context information
    /// for hygienic macro expansion. This is the foundation for implementing
    /// R6RS-style `syntax-case` macros.
    ///
    /// # Memory Layout
    ///
    /// - `expr`: ArenaIndex to the wrapped datum (the actual S-expression)
    /// - `context`: ArenaIndex to cons cell (marks . substitutions)
    ///   - car: list of marks for tracking hygiene scopes
    ///   - cdr: substitution environment for identifier resolution
    ///
    /// This maintains the 2-index constraint per arena slot, matching Lambda's layout.
    ///
    /// # Example
    ///
    /// ```scheme
    /// (syntax-case stx ()
    ///   ((keyword arg ...)
    ///    (with-syntax ((name (generate-name)))
    ///      #'(define name (lambda () arg ...)))))
    /// ```
    ///
    /// # References
    ///
    /// - R6RS Chapter 11 (syntax-case)
    /// - "Macros that Work" (Clinger & Rees, 1991)
    /// - psyntax (Dybvig, Hieb, Bruggeman)
    Syntax {
        expr: ArenaIndex,       // The wrapped datum
        context: ArenaIndex,    // cons cell: (marks . substitutions)
    },
    
    /// Continuation frame for arena-based continuation stack (call/cc support)
    ///
    /// Continuation frames form a linked list in the arena, enabling O(1) capture
    /// for call/cc. Each frame stores the continuation type, associated data, and
    /// a reference to the parent continuation.
    ///
    /// # Memory Layout
    ///
    /// - `cont_data`: ArenaIndex to cons cell (type_and_data . parent_cont)
    ///   - car: (Usize(cont_type) . data) where data encodes continuation-specific values
    ///   - cdr: ArenaIndex to parent ContFrame, or Nil for Done
    /// - `env`: ArenaIndex to the environment at this continuation point
    ///
    /// This maintains the 2-index constraint per arena slot, matching Lambda's layout.
    ///
    /// # Example Continuation Types (encoded as Usize)
    ///
    /// - 0: Done - computation complete
    /// - 1: ApplyForced - after evaluating function
    /// - 2: IfBranch - after evaluating condition
    /// - etc.
    ///
    /// # References
    ///
    /// See docs/CALL_CC_IMPLEMENTATION_PLAN.md for the full implementation plan.
    ContFrame {
        cont_data: ArenaIndex,  // cons cell: (type_and_data . parent_cont)
        env: ArenaIndex,        // environment at this continuation point
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
    
    /// Check if this value is an integer
    #[inline]
    pub const fn is_integer(&self) -> bool {
        matches!(self, Value::Number(_))
    }
    
    /// Extract ArenaIndex from a Ref value.
    /// Returns None if not a Ref.
    #[inline]
    pub const fn as_ref(&self) -> Option<ArenaIndex> {
        match self {
            Value::Ref(idx) => Some(*idx),
            _ => None,
        }
    }
    
    /// Extract ArenaIndex from a Ref value, panicking if not a Ref.
    /// Use only when you are certain the value is a Ref (e.g., after alloc_contiguous for Refs).
    #[inline]
    pub fn unwrap_ref(self) -> ArenaIndex {
        match self {
            Value::Ref(idx) => idx,
            _ => panic!("expected Ref"),
        }
    }
    
    /// Check if this value is a symbol
    #[inline]
    pub const fn is_symbol(&self) -> bool {
        matches!(self, Value::Symbol(_))
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
    
    /// Get the char value if this is a char
    #[inline]
    pub const fn as_char(&self) -> Option<char> {
        match self {
            Value::Char(c) => Some(*c),
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
            Value::Char(_) => "char",
            Value::Cons { .. } => "pair",
            Value::Symbol(_) => "symbol",
            Value::Lambda { .. } => "procedure",
            Value::Builtin(_) => "procedure",
            Value::StdLib(_) => "procedure",
            Value::Native { .. } => "native",
            Value::Array { .. } => "array",
            Value::String { .. } => "string",
            Value::Ref(_) => "ref",
            Value::Usize(_) => "usize",
            Value::SyntaxRules { .. } => "syntax-rules",
            Value::Syntax { .. } => "syntax",
            Value::ContFrame { .. } => "cont-frame",
        }
    }
    
    /// Check if this value is a syntax-rules transformer
    #[inline]
    pub const fn is_syntax_rules(&self) -> bool {
        matches!(self, Value::SyntaxRules { .. })
    }
    
    /// Check if this value is a syntax object
    #[inline]
    pub const fn is_syntax(&self) -> bool {
        matches!(self, Value::Syntax { .. })
    }
    
    /// Check if this value is a continuation frame
    #[inline]
    pub const fn is_cont_frame(&self) -> bool {
        matches!(self, Value::ContFrame { .. })
    }
}

/// Implement Trace for GC support
/// 
/// With inline fields, tracing is simpler - we just trace the ArenaIndex fields directly.
/// No arena access needed to determine structure, which improves GC performance.
impl<const N: usize> Trace<Value, N> for Value {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match self {
            Value::Nil | Value::True | Value::False | 
            Value::Number(_) | Value::Char(_) | Value::Builtin(_) |
            Value::StdLib(_) | Value::Usize(_) => {
                // No references
            }
            Value::Ref(idx) => {
                // Trace the referenced value
                tracer(*idx);
            }
            Value::Cons { car, cdr } => {
                // Inline car and cdr - trace both directly
                tracer(*car);
                tracer(*cdr);
            }
            Value::Native { .. } => {
                // id and name_hash are inline usize values, no arena references to trace
            }
            Value::Symbol(chars) => {
                // chars points to a Value::String, which handles its own tracing
                tracer(*chars);
            }
            Value::Lambda { params, body_env } => {
                // params and body_env are inline ArenaIndex - trace both
                // body_env points to a cons cell (body . env)
                tracer(*params);
                tracer(*body_env);
            }
            Value::SyntaxRules { literals, rules_env } => {
                // literals and rules_env are inline ArenaIndex - trace both
                // rules_env points to a cons cell (rules . definition_env)
                tracer(*literals);
                tracer(*rules_env);
            }
            Value::Syntax { expr, context } => {
                // expr and context are inline ArenaIndex - trace both
                // context points to a cons cell (marks . substitutions)
                tracer(*expr);
                tracer(*context);
            }
            Value::ContFrame { cont_data, env } => {
                // cont_data and env are inline ArenaIndex - trace both
                // cont_data points to a cons cell (type_and_data . parent_cont)
                tracer(*cont_data);
                tracer(*env);
            }
            Value::Array { len, data } => {
                // For non-empty arrays, trace all elements
                // Empty arrays have len=0 and data == NIL
                if *len > 0 {
                    let base_idx = data.raw();
                    for i in 0..*len {
                        // Elements are at data, data+1, ..., data+len-1
                        let elem_idx = ArenaIndex::new(base_idx + i);
                        tracer(elem_idx);
                    }
                }
            }
            Value::String { len, data } => {
                // For non-empty strings, trace all Char slots
                // Empty strings have len=0 and data == NIL
                if *len > 0 {
                    let base_idx = data.raw();
                    for i in 0..*len {
                        // Characters are at data, data+1, ..., data+len-1
                        let char_idx = ArenaIndex::new(base_idx + i);
                        tracer(char_idx);
                    }
                }
            }
        }
    }
    
    fn trace_with_arena<F: FnMut(ArenaIndex)>(&self, _arena: &grift_arena::Arena<Value, N>, tracer: F) {
        // With inline length fields, we no longer need arena access for tracing.
        // Simply delegate to the standard trace method.
        <Value as Trace<Value, N>>::trace(self, tracer)
    }
}
