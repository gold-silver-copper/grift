//! Value types for the Lisp parser
//!
//! This module contains the core Value enum and related types like Builtin and StdLib.
//!
//! Note: The `define_builtins!` macro has been moved to `src/macros.rs`.

use grift_arena::{ArenaIndex, Trace};
use crate::fsize;
use crate::io::PortId;

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
    /// expt - Exponentiation
    Expt => "expt",
    
    // Numeric predicates
    /// integer? - Check if value is an integer
    Integerp => "integer?",
    /// exact? - Check if number is exact (always true for integers)
    Exactp => "exact?",
    /// inexact? - Check if number is inexact (always false for integers)
    Inexactp => "inexact?",
    /// exact->inexact - Convert exact number to inexact
    ExactToInexact => "exact->inexact",
    /// inexact->exact - Convert inexact number to exact
    InexactToExact => "inexact->exact",
    /// exact - R7RS exact conversion
    Exact => "exact",
    /// inexact - R7RS inexact conversion
    Inexact => "inexact",
    /// finite? - Check if number is finite
    Finitep => "finite?",
    /// infinite? - Check if number is infinite
    Infinitep => "infinite?",
    /// nan? - Check if number is NaN
    Nanp => "nan?",
    /// sqrt - Square root
    Sqrt => "sqrt",

    // Transcendental functions (R7RS §6.2.6) — powered by libm
    /// exp - Exponential function (e^x)
    Exp => "exp",
    /// log - Natural logarithm; optional base parameter
    Log => "log",
    /// sin - Sine (radians)
    Sin => "sin",
    /// cos - Cosine (radians)
    Cos => "cos",
    /// tan - Tangent (radians)
    Tan => "tan",
    /// asin - Arcsine (returns radians)
    Asin => "asin",
    /// acos - Arccosine (returns radians)
    Acos => "acos",
    /// atan - Arctangent; one or two argument form
    Atan => "atan",

    // Division procedures (R7RS §6.2.6)
    /// floor-quotient - ⌊n/d⌋
    FloorQuotient => "floor-quotient",
    /// floor-remainder - n - d·⌊n/d⌋
    FloorRemainder => "floor-remainder",
    /// floor/ - Returns quotient and remainder via values
    FloorDiv => "floor/",
    /// truncate-quotient - truncate(n/d)
    TruncateQuotient => "truncate-quotient",
    /// truncate-remainder - n - d·truncate(n/d)
    TruncateRemainder => "truncate-remainder",
    /// truncate/ - Returns quotient and remainder via values
    TruncateDiv => "truncate/",

    // Rational number operations (R7RS §6.2.6)
    /// numerator - Returns numerator of a number
    Numerator => "numerator",
    /// denominator - Returns denominator of a number
    Denominator => "denominator",
    /// rationalize - Simplest rational within tolerance
    Rationalize => "rationalize",

    // Exact integer square root (R7RS §6.2.6)
    /// exact-integer-sqrt - Returns s and r where n = s² + r
    ExactIntegerSqrt => "exact-integer-sqrt",

    // Complex number operations (R7RS §6.2.6)
    /// make-rectangular - Create complex from real and imaginary parts
    MakeRectangular => "make-rectangular",
    /// make-polar - Create complex from magnitude and angle
    MakePolar => "make-polar",
    /// real-part - Extract real part
    RealPart => "real-part",
    /// imag-part - Extract imaginary part
    ImagPart => "imag-part",
    /// magnitude - |z|
    Magnitude => "magnitude",
    /// angle - arg(z)
    Angle => "angle",

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
    
    // I/O
    /// newline - Print a newline
    Newline => "newline",
    /// display - Print value without quotes
    Display => "display",
    
    // Port operations (R7RS §6.13)
    /// port? - Check if value is a port
    Portp => "port?",
    /// input-port? - Check if value is an input port
    InputPortp => "input-port?",
    /// output-port? - Check if value is an output port
    OutputPortp => "output-port?",
    /// current-input-port - Get current input port
    CurrentInputPort => "current-input-port",
    /// current-output-port - Get current output port
    CurrentOutputPort => "current-output-port",
    /// current-error-port - Get current error port
    CurrentErrorPort => "current-error-port",
    /// close-port - Close a port
    ClosePort => "close-port",
    /// close-input-port - Close an input port
    CloseInputPort => "close-input-port",
    /// close-output-port - Close an output port
    CloseOutputPort => "close-output-port",
    /// read-char - Read a character from a port
    ReadChar => "read-char",
    /// write-char - Write a character to a port
    WriteChar => "write-char",
    /// peek-char - Peek at next character without consuming it
    PeekChar => "peek-char",
    /// char-ready? - Check if a character is available
    CharReadyp => "char-ready?",
    /// write - Write value with machine-readable representation
    Write => "write",
    /// read - Read an S-expression from a port
    Read => "read",
    /// eof-object - Return the EOF object
    EofObject => "eof-object",
    /// eof-object? - Check if value is the EOF object
    EofObjectp => "eof-object?",
    /// open-input-string - Create an input port from a string
    OpenInputString => "open-input-string",
    /// open-output-string - Create an output string port
    OpenOutputString => "open-output-string",
    /// get-output-string - Get accumulated string from an output string port
    GetOutputString => "get-output-string",
    /// read-line - Read a line of text from a port
    ReadLine => "read-line",
    /// read-string - Read up to k characters from a port
    ReadString => "read-string",
    /// write-shared - Write with shared structure notation
    WriteShared => "write-shared",
    /// write-simple - Write without shared structure handling
    WriteSimple => "write-simple",
    /// textual-port? - Check if port handles text
    TextualPortp => "textual-port?",
    /// binary-port? - Check if port handles binary data
    BinaryPortp => "binary-port?",
    /// input-port-open? - Check if input port is still open
    InputPortOpenp => "input-port-open?",
    /// output-port-open? - Check if output port is still open
    OutputPortOpenp => "output-port-open?",

    // File port operations (R7RS §6.13.2)
    /// open-input-file - Open a textual input port on a file
    OpenInputFile => "open-input-file",
    /// open-output-file - Open a textual output port on a file
    OpenOutputFile => "open-output-file",
    /// open-binary-input-file - Open a binary input port on a file
    OpenBinaryInputFile => "open-binary-input-file",
    /// open-binary-output-file - Open a binary output port on a file
    OpenBinaryOutputFile => "open-binary-output-file",
    /// call-with-port - Call proc with port, close port when proc returns
    CallWithPort => "call-with-port",
    /// call-with-input-file - Call proc with input port, then close it
    CallWithInputFile => "call-with-input-file",
    /// call-with-output-file - Call proc with output port, then close it
    CallWithOutputFile => "call-with-output-file",
    /// with-input-from-file - Redirect current-input-port to file
    WithInputFromFile => "with-input-from-file",
    /// with-output-to-file - Redirect current-output-port to file
    WithOutputToFile => "with-output-to-file",

    // Binary I/O operations (R7RS §6.13.2)
    /// read-u8 - Read a single byte from a binary input port
    ReadU8 => "read-u8",
    /// peek-u8 - Peek at next byte without consuming it
    PeekU8 => "peek-u8",
    /// u8-ready? - Check if a byte is ready to read
    U8Readyp => "u8-ready?",
    /// read-bytevector - Read up to k bytes into a new bytevector
    ReadBytevector => "read-bytevector",
    /// read-bytevector! - Read bytes into an existing bytevector
    ReadBytevectorBang => "read-bytevector!",
    /// write-u8 - Write a single byte to a binary output port
    WriteU8 => "write-u8",
    /// write-bytevector - Write bytevector bytes to a binary output port
    WriteBytevector => "write-bytevector",

    // Bytevector port operations (R7RS §6.13.2)
    /// open-input-bytevector - Create binary input port from bytevector
    OpenInputBytevector => "open-input-bytevector",
    /// open-output-bytevector - Create binary output port to bytevector
    OpenOutputBytevector => "open-output-bytevector",
    /// get-output-bytevector - Get bytevector from output bytevector port
    GetOutputBytevector => "get-output-bytevector",

    // Additional I/O operations (R7RS §6.13.2)
    /// write-string - Write string to textual output port
    WriteStringPort => "write-string",
    /// flush-output-port - Flush buffered output
    FlushOutputPort => "flush-output-port",

    // Error handling
    /// error - Raise an error
    Error => "error",
    /// error-object? - Check if value is an error object
    ErrorObjectP => "error-object?",
    /// error-object-message - Get message from error object
    ErrorObjectMessage => "error-object-message",
    /// error-object-irritants - Get irritants from error object
    ErrorObjectIrritants => "error-object-irritants",
    /// error-object-type - Get type from error object
    ErrorObjectType => "error-object-type",
    
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
    /// vector-copy! - Copy elements from one vector to another
    VectorCopyTo => "vector-copy!",
    /// vector-append - Concatenate vectors
    VectorAppend => "vector-append",
    /// vector-map - Apply procedure to elements of vectors
    VectorMap => "vector-map",
    /// vector-for-each - Apply procedure to elements for side effects
    VectorForEach => "vector-for-each",
    
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
    /// char-foldcase - Unicode simple case folding
    CharFoldcase => "char-foldcase",
    /// char-alphabetic? - Check if character is alphabetic (Unicode)
    CharAlphabetic => "char-alphabetic?",
    /// char-numeric? - Check if character is numeric (Unicode Nd)
    CharNumeric => "char-numeric?",
    /// char-whitespace? - Check if character is whitespace (Unicode)
    CharWhitespace => "char-whitespace?",
    /// char-upper-case? - Check if character is uppercase (Unicode)
    CharUpperCase => "char-upper-case?",
    /// char-lower-case? - Check if character is lowercase (Unicode)
    CharLowerCase => "char-lower-case?",
    /// digit-value - Return numeric value of a Unicode digit character
    DigitValue => "digit-value",
    /// char-ci=? - Case-insensitive character equality
    CharCiEq => "char-ci=?",
    /// char-ci<? - Case-insensitive character less than
    CharCiLt => "char-ci<?",
    /// char-ci>? - Case-insensitive character greater than
    CharCiGt => "char-ci>?",
    /// char-ci<=? - Case-insensitive character less than or equal
    CharCiLe => "char-ci<=?",
    /// char-ci>=? - Case-insensitive character greater than or equal
    CharCiGe => "char-ci>=?",
    /// string-upcase - Convert string to uppercase
    StringUpcase => "string-upcase",
    /// string-downcase - Convert string to lowercase
    StringDowncase => "string-downcase",
    /// string-foldcase - Convert string using case folding
    StringFoldcase => "string-foldcase",
    /// string-ci=? - Case-insensitive string equality
    StringCiEq => "string-ci=?",
    
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
    /// string-copy! - Copy characters from one string to another
    StringCopyTo => "string-copy!",
    /// string-fill! - Fill string with character
    StringFill => "string-fill!",
    /// string-ci<? - Case-insensitive string less than
    StringCiLt => "string-ci<?",
    /// string-ci>? - Case-insensitive string greater than
    StringCiGt => "string-ci>?",
    /// string-ci<=? - Case-insensitive string less than or equal
    StringCiLe => "string-ci<=?",
    /// string-ci>=? - Case-insensitive string greater than or equal
    StringCiGe => "string-ci>=?",
    
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
    /// syntax-e - Racket-style alias for syntax->datum, extract the datum from a syntax object
    SyntaxE => "syntax-e",
    /// datum->syntax - Wrap a datum with syntax context from a template identifier
    DatumToSyntax => "datum->syntax",
    /// datum->syntax-object - R6RS alias for datum->syntax
    DatumToSyntaxObject => "datum->syntax-object",
    /// syntax-object->datum - R6RS alias for syntax->datum
    SyntaxObjectToDatum => "syntax-object->datum",
    /// generate-temporaries - Generate a list of fresh identifiers
    GenerateTemporaries => "generate-temporaries",
    
    /// symbol->string - Convert symbol to string
    SymbolToString => "symbol->string",
    /// string->symbol - Convert string to symbol
    StringToSymbol => "string->symbol",
    /// number->string - Convert number to string
    NumberToString => "number->string",
    /// string->number - Convert string to number (or #f if invalid)
    StringToNumber => "string->number",

    // File system and process operations (R7RS §6.13, §6.14)
    /// load - Load and evaluate a Scheme source file
    Load => "load",
    /// file-exists? - Check if a file exists
    FileExistsP => "file-exists?",
    /// delete-file - Delete a file
    DeleteFile => "delete-file",
    /// command-line - Return command-line arguments as a list of strings
    CommandLine => "command-line",
    /// exit - Terminate the program normally
    Exit => "exit",
    /// emergency-exit - Terminate the program immediately without cleanup
    EmergencyExit => "emergency-exit",
    /// get-environment-variable - Get a single environment variable
    GetEnvironmentVariable => "get-environment-variable",
    /// get-environment-variables - Get all environment variables as an alist
    GetEnvironmentVariables => "get-environment-variables",

    // Environment procedures (R7RS §6.12)
    /// interaction-environment - Return the mutable REPL environment
    InteractionEnvironment => "interaction-environment",
    /// scheme-report-environment - Return environment for given R^n RS version
    SchemeReportEnvironment => "scheme-report-environment",
    /// null-environment - Return minimal environment with only syntax
    NullEnvironment => "null-environment",
    /// environment? - Check if value is an environment
    Environmentp => "environment?",

    // Bytevector operations (R7RS §6.9)
    /// bytevector? - Check if value is a bytevector
    Bytevectorp => "bytevector?",
    /// bytevector - Variadic constructor: (bytevector byte ...)
    Bytevector_ => "bytevector",
    /// make-bytevector - Create a bytevector with optional fill byte
    MakeBytevector => "make-bytevector",
    /// bytevector-length - Get length of bytevector
    BytevectorLength => "bytevector-length",
    /// bytevector-u8-ref - Get byte at index
    BytevectorU8Ref => "bytevector-u8-ref",
    /// bytevector-u8-set! - Set byte at index
    BytevectorU8Set => "bytevector-u8-set!",
    /// bytevector-copy - Copy a bytevector
    BytevectorCopy => "bytevector-copy",
    /// bytevector-copy! - Destructive copy between bytevectors
    BytevectorCopyBang => "bytevector-copy!",
    /// bytevector-append - Concatenate bytevectors
    BytevectorAppend => "bytevector-append",
    /// utf8->string - Decode bytevector as UTF-8 string
    Utf8ToString => "utf8->string",
    /// string->utf8 - Encode string as UTF-8 bytevector
    StringToUtf8 => "string->utf8",

    // Time procedures (R7RS §6.13.3)
    /// current-second - Current time as inexact seconds since epoch
    CurrentSecond => "current-second",
    /// current-jiffy - Current jiffy count (monotonic)
    CurrentJiffy => "current-jiffy",
    /// jiffies-per-second - Number of jiffies per SI second
    JiffiesPerSecond => "jiffies-per-second",

    // Error predicates (R7RS §6.11)
    /// read-error? - Check if error was raised by read
    ReadErrorP => "read-error?",
    /// file-error? - Check if error was raised by file operations
    FileErrorP => "file-error?",

    // Vector-String conversion (R7RS §6.8)
    /// vector->string - Create string from vector of characters
    VectorToString => "vector->string",
    /// string->vector - Create vector of characters from string
    StringToVector => "string->vector",

    // First-class procedures (R7RS requires these to be values, not special forms)
    /// values - Return multiple values
    Values => "values",
    /// call-with-values - Call producer, apply consumer to results
    CallWithValues => "call-with-values",
    /// apply - Apply procedure to list of arguments
    Apply => "apply",
    /// call/cc - Capture current continuation
    CallCc => "call/cc",
    /// call-with-current-continuation - Capture current continuation (long form)
    CallWithCurrentContinuation => "call-with-current-continuation",
    /// dynamic-wind - Install before/after thunks around body
    DynamicWind => "dynamic-wind",
    /// with-exception-handler - Install exception handler around thunk
    WithExceptionHandler => "with-exception-handler",
    /// raise - Raise a non-continuable exception
    RaiseBuiltin => "raise",
    /// raise-continuable - Raise a continuable exception
    RaiseContinuable => "raise-continuable",
    /// eval - Evaluate expression in environment
    EvalBuiltin => "eval",
    /// environment - Create environment from import sets
    EnvironmentBuiltin => "environment",
}

/// Standard library function definition data.
///
/// Holds the static data for a single stdlib function:
/// - `name`: The Scheme function name (e.g. `"map"`)
/// - `params`: The parameter names (e.g. `&["f", "lst"]`)
/// - `body`: The body source code (e.g. `"(begin (if (null? lst) ...))"`)
#[derive(Debug)]
pub struct StdLibEntry {
    /// The function name.
    pub name: &'static str,
    /// The parameter names.
    pub params: &'static [&'static str],
    /// The body source code.
    pub body: &'static str,
}

/// Standard library function (stored in static memory, parsed on-demand)
///
/// A thin wrapper around a reference to static [`StdLibEntry`] data.
/// Downstream crates (like `grift_parser`) define the actual set of stdlib
/// functions via `STDLIB_ALL`.
#[derive(Clone, Copy, Debug)]
pub struct StdLib(pub &'static StdLibEntry);

impl PartialEq for StdLib {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.0, other.0)
    }
}

impl Eq for StdLib {}

impl StdLib {
    /// Create a new StdLib function reference.
    pub const fn new(entry: &'static StdLibEntry) -> Self {
        Self(entry)
    }

    /// Get the function name.
    pub const fn name(&self) -> &'static str {
        self.0.name
    }

    /// Get the parameter names for this function.
    pub const fn params(&self) -> &'static [&'static str] {
        self.0.params
    }

    /// Get the body source code (Lisp expression as static string).
    ///
    /// This string is parsed on each call to the function.
    /// The parsed AST is temporary and GC'd after evaluation.
    pub const fn body(&self) -> &'static str {
        self.0.body
    }
}

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
    
    /// Void/Unspecified value (R7RS)
    /// 
    /// Returned by side-effect-only forms like `define`, `set!`, `display`, etc.
    /// The REPL should not print anything when this is returned.
    Void,
    
    /// Boolean true (#t)
    True,
    
    /// Boolean false (#f) - the ONLY false value
    False,
    
    /// Integer number
    Number(isize),
    
    /// Inexact floating-point number (R7RS numeric tower)
    ///
    /// Uses `fsize` which is `f64` on 64-bit platforms and `f32` on 32-bit,
    /// matching the width of `isize`/`usize`.
    Float(fsize),
    
    /// Exact rational number (R7RS §6.2)
    ///
    /// Stored as numerator/denominator pair, always reduced to lowest terms.
    /// Denominator is always positive.
    Rational { num: isize, denom: isize },

    /// Complex number (R7RS §6.2)
    ///
    /// Stored as real and imaginary parts (both inexact).
    Complex { real: fsize, imag: fsize },

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
    
    /// Bytevector (R7RS §6.9) with inline length and data pointer
    /// 
    /// Bytevectors store exact integers in the range 0–255 as `Number` values
    /// contiguously in the arena, reusing the same layout as `Array`.
    /// 
    /// # Memory Layout
    /// 
    /// - `len`: Number of bytes (inline)
    /// - `data`: Points directly to first `Number` value in arena
    /// - Empty bytevectors have len=0 and data == NIL
    /// 
    /// # Example
    /// 
    /// ```scheme
    /// #u8(0 10 5)           ; Bytevector literal
    /// (bytevector-length #u8(1 2 3))  ; => 3
    /// ```
    Bytevector { len: usize, data: ArenaIndex },
    
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
    
    /// Native function with inline id
    ///
    /// Native functions are registered at runtime and identified by their ID.
    /// The actual function pointer is stored in the evaluator's NativeRegistry.
    Native { id: usize },
    
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
    /// for call/cc. Each frame stores a reference to the continuation type (stored
    /// separately in the arena as `Value::ContType`), associated data, and a
    /// reference to the parent continuation.
    ///
    /// # Memory Layout (2-index constraint, matching Lambda)
    ///
    /// - `cont_data`: ArenaIndex to cons cell `((cont_type_ref . data) . parent_cont)`
    ///   - car: cons cell `(cont_type_ref . data)` where cont_type_ref points to a `Value::ContType`
    ///   - cdr: ArenaIndex to parent ContFrame, or Nil for Done
    /// - `env`: ArenaIndex to the environment at this continuation point
    ///
    /// # References
    ///
    /// See docs/CALL_CC_IMPLEMENTATION_PLAN.md for the full implementation plan.
    ContFrame {
        cont_data: ArenaIndex,  // cons cell: ((cont_type_ref . data) . parent_cont)
        env: ArenaIndex,        // environment at this continuation point
    },
    
    /// Continuation type tag stored in the arena.
    ///
    /// This value is referenced by `ContFrame`'s `cont_data` field to identify
    /// what kind of continuation a frame represents. Stored in the arena to
    /// maintain the 2-index constraint on `ContFrame`.
    ContType(crate::ContType),
    
    /// R7RS error object (§6.11)
    ///
    /// Created by the `error` procedure. Stores the error message and
    /// associated irritant values for structured exception handling.
    ///
    /// # Memory Layout
    ///
    /// - `message`: ArenaIndex to a Value::String or Value::Symbol containing the error message
    /// - `irritants_and_type`: ArenaIndex to a cons cell `(irritants . error_type)`
    ///   - car: list of irritant values passed to `error`
    ///   - cdr: error type (Nil for standard `(error msg ...)` calls)
    ///
    /// # Example
    ///
    /// ```scheme
    /// (error "out of range" 42)       ; message="out of range", irritants=(42), type=()
    /// (guard (e ((error-object? e) (error-object-message e)))
    ///   (error "bad value" 1 2 3))    ; => "bad value"
    /// ```
    ErrorObject {
        message: ArenaIndex,
        irritants_and_type: ArenaIndex,
    },

    /// Captured continuation from call/cc - a first-class callable value
    ///
    /// When `call-with-current-continuation` (call/cc) is invoked, the current
    /// continuation is captured and wrapped as this value type. The continuation
    /// can later be invoked as a procedure, which abandons the current computation
    /// and returns to the point where the continuation was captured.
    ///
    /// # Memory Layout
    ///
    /// Following the 2-index constraint like Lambda and Cons:
    /// - `cont_chain`: ArenaIndex to ContFrame linked list (captured continuation stack)
    ///   - Points to the head of the continuation chain (most recent frame)
    ///   - Nil represents an empty continuation (e.g., at top-level REPL with no pending work)
    /// - `metadata`: ArenaIndex to cons cell `(capture_env . dynamic_wind_chain)`
    ///   - car: environment at capture point
    ///   - cdr: dynamic-wind chain for proper before/after thunk invocation
    ///
    /// # Semantics
    ///
    /// When a continuation is called with a value:
    /// 1. The current computation is abandoned
    /// 2. The captured continuation stack is restored
    /// 3. The value becomes the result at the call/cc point
    ///
    /// # Example
    ///
    /// ```scheme
    /// (+ 1 (call/cc (lambda (k) (+ 2 (k 3)))))
    /// ;; => 4 (not 6, because (k 3) never returns)
    /// ```
    ///
    /// # References
    ///
    /// See docs/CALL_CC_IMPLEMENTATION_PLAN.md for the full implementation plan.
    Continuation {
        cont_chain: ArenaIndex,  // Points to ContFrame linked list head (or Nil for empty)
        metadata: ArenaIndex,    // cons cell: (capture_env . dynamic_wind_chain)
    },

    /// I/O Port (R7RS §6.13)
    ///
    /// A first-class port value identified by a [`PortId`].
    /// Standard ports (stdin=0, stdout=1, stderr=2) are predefined;
    /// additional ports can be opened for string or file I/O.
    Port(PortId),

    /// End-of-file object (R7RS §6.13)
    ///
    /// A unique value returned by read operations when the end of input is reached.
    Eof,

    /// First-class environment object (R7RS §6.12)
    ///
    /// Created by `environment` or `interaction-environment`.
    /// - `env`: ArenaIndex to the environment bindings chain
    /// - `mutable`: whether new bindings can be added (`true` for
    ///   interaction-environment, `false` for `environment`)
    Environment { env: ArenaIndex, mutable: bool },
}

/// Generate simple `is_*` predicate methods on `Value`.
macro_rules! value_predicates {
    ($( $(#[doc = $doc:literal])* $name:ident => $pat:pat ),+ $(,)?) => {
        $(
            $(#[doc = $doc])*
            #[inline]
            pub const fn $name(&self) -> bool {
                matches!(self, $pat)
            }
        )+
    };
}

impl Value {
    value_predicates! {
        /// Check if this value is nil (empty list)
        is_nil => Value::Nil,
        /// Check if this value is void (unspecified value)
        ///
        /// Void is returned by side-effect-only forms like `define`, `set!`, `display`.
        /// The REPL should not print anything when this value is returned.
        is_void => Value::Void,
        /// Check if this value is false (#f) — the ONLY way to be false in this Lisp
        is_false => Value::False,
        /// Check if this value is true (#t)
        is_true => Value::True,
        /// Check if this value is a boolean (#t or #f)
        is_boolean => Value::True | Value::False,
        /// Check if this value is a float
        is_float => Value::Float(_),
        /// Check if this value is a symbol
        is_symbol => Value::Symbol(_),
        /// Check if this value is a cons cell (pair)
        is_cons => Value::Cons { .. },
        /// Check if this value is a lambda
        is_lambda => Value::Lambda { .. },
        /// Check if this value is a builtin
        is_builtin => Value::Builtin(_),
        /// Check if this value is a stdlib function
        is_stdlib => Value::StdLib(_),
        /// Check if this value is a native (Rust) function
        is_native => Value::Native { .. },
        /// Check if this value is a procedure (lambda, builtin, stdlib, or native function)
        is_procedure => Value::Lambda { .. } | Value::Builtin(_) | Value::StdLib(_) | Value::Native { .. },
        /// Check if this value is an array
        is_array => Value::Array { .. },
        /// Check if this value is a bytevector
        is_bytevector => Value::Bytevector { .. },
        /// Check if this value is a string
        is_string => Value::String { .. },
        /// Check if this value is a ref (internal arena index reference)
        is_ref => Value::Ref(_),
        /// Check if this value is a usize (internal unsigned integer)
        is_usize => Value::Usize(_),
        /// Check if this value is a syntax object
        is_syntax => Value::Syntax { .. },
        /// Check if this value is a continuation frame
        is_cont_frame => Value::ContFrame { .. },
        /// Check if this value is a captured continuation (from call/cc)
        is_continuation => Value::Continuation { .. },
    }
    
    /// Check if this value is an atom (not a cons cell)
    #[inline]
    pub const fn is_atom(&self) -> bool {
        !matches!(self, Value::Cons { .. })
    }
    
    /// Check if this value is a number (integer, float, rational, or complex)
    #[inline]
    pub const fn is_number(&self) -> bool {
        matches!(self, Value::Number(_) | Value::Float(_) | Value::Rational { .. } | Value::Complex { .. })
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

    /// Get the number value if this is an integer
    #[inline]
    pub const fn as_number(&self) -> Option<isize> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
    
    /// Get the float value if this is a Float
    #[inline]
    pub fn as_float(&self) -> Option<fsize> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }
    
    /// Get the numeric value as an fsize (works for both Number and Float)
    #[inline]
    pub fn as_fsize(&self) -> Option<fsize> {
        match self {
            Value::Number(n) => Some(*n as fsize),
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
            Value::Void => "void",
            Value::True | Value::False => "boolean",
            Value::Number(_) | Value::Float(_) | Value::Rational { .. } | Value::Complex { .. } => "number",
            Value::Char(_) => "char",
            Value::Cons { .. } => "pair",
            Value::Symbol(_) => "symbol",
            Value::Lambda { .. } | Value::Builtin(_) | Value::StdLib(_) => "procedure",
            Value::Native { .. } => "native",
            Value::Array { .. } => "array",
            Value::Bytevector { .. } => "bytevector",
            Value::String { .. } => "string",
            Value::Ref(_) => "ref",
            Value::Usize(_) => "usize",
            Value::Syntax { .. } => "syntax",
            Value::ContFrame { .. } => "cont-frame",
            Value::ContType(_) => "cont-type",
            Value::Continuation { .. } => "continuation",
            Value::ErrorObject { .. } => "error-object",
            Value::Port(_) => "port",
            Value::Eof => "eof-object",
            Value::Environment { .. } => "environment",
        }
    }
    
}

/// Implement Trace for GC support
/// 
/// With inline fields, tracing is simpler - we just trace the ArenaIndex fields directly.
/// No arena access needed to determine structure, which improves GC performance.
impl<const N: usize> Trace<Value, N> for Value {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match self {
            Value::Nil | Value::Void | Value::True | Value::False | 
            Value::Number(_) | Value::Float(_) | Value::Rational { .. } |
            Value::Complex { .. } | Value::Char(_) | Value::Builtin(_) |
            Value::StdLib(_) | Value::Usize(_) | Value::Port(_) | Value::Eof |
            Value::ContType(_) => {
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
                // id is an inline usize value, no arena references to trace
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
            Value::Syntax { expr, context } => {
                // expr and context are inline ArenaIndex - trace both
                // context points to a cons cell (marks . substitutions)
                tracer(*expr);
                tracer(*context);
            }
            Value::ContFrame { cont_data, env } => {
                // cont_data and env are inline ArenaIndex - trace both
                // cont_data points to ((cont_type_ref . data) . parent_cont)
                tracer(*cont_data);
                tracer(*env);
            }
            Value::ErrorObject { message, irritants_and_type } => {
                // message and irritants_and_type are inline ArenaIndex - trace both
                tracer(*message);
                tracer(*irritants_and_type);
            }
            Value::Continuation { cont_chain, metadata } => {
                // cont_chain and metadata are inline ArenaIndex - trace both
                // cont_chain points to ContFrame linked list
                // metadata points to a cons cell (capture_env . dynamic_wind_chain)
                tracer(*cont_chain);
                tracer(*metadata);
            }
            Value::Array { len, data }
            | Value::Bytevector { len, data }
            | Value::String { len, data } => {
                // For non-empty contiguous data, trace all element slots
                if *len > 0 {
                    let base_idx = data.raw();
                    for i in 0..*len {
                        tracer(ArenaIndex::new(base_idx + i));
                    }
                }
            }
            Value::Environment { env, .. } => {
                tracer(*env);
            }
        }
    }
    
    fn trace_with_arena<F: FnMut(ArenaIndex)>(&self, _arena: &grift_arena::Arena<Value, N>, tracer: F) {
        // With inline length fields, we no longer need arena access for tracing.
        // Simply delegate to the standard trace method.
        <Value as Trace<Value, N>>::trace(self, tracer)
    }
}
