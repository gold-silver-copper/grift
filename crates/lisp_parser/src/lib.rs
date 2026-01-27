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
//! - **Lazy by default** - All evaluation is call-by-need (like Haskell)
//!
//! ## Value Representation
//!
//! - `Nil` - The empty list (NOT false!)
//! - `True` - Boolean true (#t)
//! - `False` - Boolean false (#f)
//! - `Number(i64)` - Integer numbers
//! - `Char(char)` - Single character
//! - `Cons { car, cdr }` - Pair/list cell
//! - `Symbol { chars, len }` - Symbol with contiguous string storage
//! - `Lambda { params, body, env }` - Closure
//! - `Thunk { expr, env, cached }` - Lazy computation (internal, auto-managed)
//! - `Builtin(Builtin)` - Optimized built-in function
//! - `StdLib(StdLib)` - Standard library function (static code, parsed on-demand)
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
//! ### Lazy Evaluation
//! - Side effects in lazy contexts may not happen when expected
//! - `cons` is lazy - car and cdr are wrapped in thunks
//! - Values are forced automatically in strict positions (arithmetic, predicates, etc.)
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
        /// - All evaluation is call-by-need (lazy by default)
        /// - Values are forced automatically in strict positions
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
    // List operations (non-strict - don't force arguments)
    /// car - Get first element of pair
    Car => "car",
    /// cdr - Get second element of pair
    Cdr => "cdr",
    /// cons - Create a pair
    Cons => "cons",
    /// list - Create a list from arguments
    List => "list",
    
    // Predicates (force their argument to check type)
    /// atom - Check if value is an atom
    Atom => "atom",
    /// eq - Check equality
    Eq => "eq",
    /// null? - Check if value is nil
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
    
    // Arithmetic (strict - force arguments)
    /// + - Addition
    Add => "+",
    /// - - Subtraction
    Sub => "-",
    /// * - Multiplication
    Mul => "*",
    /// / - Division
    Div => "/",
    /// mod - Modulo
    Mod => "mod",
    
    // Comparison (strict - force arguments)
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
    
    // I/O (strict - force arguments for printing)
    /// print - Print value with newline
    Print => "print",
    /// newline - Print a newline
    Newline => "newline",
    /// display - Print value without quotes
    Display => "display",
    
    // Error handling
    /// error - Raise an error
    Error => "error",
    
    // Memoization
    /// memoize - Wrap function with memoization
    Memoize => "memoize",
    
    // Symbol generation for hygiene
    /// gensym - Generate unique symbol
    Gensym => "gensym",
    
    // Mutation operations (strict - force pair argument)
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
// This macro reads the stdlib.lisp file and generates the StdLib enum.
// To add a new function, simply add a new entry in stdlib.lisp.
// Note: member/assoc use eq for comparison (like Scheme's memq/assq).
// This works for symbols and identical objects. For value comparison,
// define a custom function or use fold with a predicate.
lisp_macros::include_stdlib!("src/stdlib.lisp");

/// A Lisp value
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// The empty list (NOT false - use False for that)
    Nil,
    
    /// Boolean true (#t)
    True,
    
    /// Boolean false (#f) - the ONLY false value
    False,
    
    /// Integer number
    Number(i64),
    
    /// Single character (for building symbol char-lists)
    Char(char),
    
    /// Cons cell (pair)
    Cons {
        car: ArenaIndex,
        cdr: ArenaIndex,
    },
    
    /// Symbol (contains either a linked list of Char values or contiguous string)
    /// For contiguous strings: chars points to [Number(len), Char, Char, ...] and len > 0
    /// For char lists (legacy): chars points to list of Char values and len == 0
    Symbol {
        chars: ArenaIndex,
        len: usize,  // Length for contiguous strings; 0 for legacy char lists
    },
    
    /// Lambda / closure
    Lambda {
        params: ArenaIndex,  // List of symbols
        body: ArenaIndex,    // Expression
        env: ArenaIndex,     // Captured environment (alist)
    },
    
    /// Thunk - delayed computation for call-by-need
    /// Created by (delay expr), forced by (force thunk)
    Thunk {
        expr: ArenaIndex,    // Unevaluated expression
        env: ArenaIndex,     // Environment for evaluation
        cached: ArenaIndex,  // Cached result (NULL if not yet evaluated)
    },
    
    /// Memoized function - caches results keyed by argument values
    /// Created by (memoize fn), automatically caches return values
    Memo {
        func: ArenaIndex,    // The wrapped function (lambda or builtin)
        cache: ArenaIndex,   // Cache: alist of (args . result) pairs
    },
    
    /// Built-in function (optimized)
    Builtin(Builtin),
    
    /// Standard library function (stored in static memory with lazy caching)
    /// 
    /// Unlike Lambda which stores code in the arena, StdLib references static
    /// function definitions. The function body is parsed on first call and cached,
    /// providing good performance while minimizing initial arena cost.
    /// 
    /// # Memory Efficiency
    /// 
    /// - Function definitions are in static memory (const strings)
    /// - Parsed body and params are cached on first call (lazy initialization)
    /// - Subsequent calls reuse the cached parsed AST
    /// 
    /// # Cache Fields
    /// 
    /// - `cached_body`: NULL until first call, then points to parsed body AST
    /// - `cached_params`: NULL until first call, then points to param list
    StdLib {
        func: StdLib,
        cached_body: ArenaIndex,    // NULL = not yet parsed
        cached_params: ArenaIndex,  // NULL = not yet created
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
    
    /// Check if this value is a procedure (lambda, builtin, stdlib, or memoized function)
    #[inline]
    pub const fn is_procedure(&self) -> bool {
        matches!(self, Value::Lambda { .. } | Value::Builtin(_) | Value::StdLib { .. } | Value::Memo { .. })
    }
    
    /// Check if this value is a thunk (promise)
    #[inline]
    pub const fn is_thunk(&self) -> bool {
        matches!(self, Value::Thunk { .. })
    }
    
    /// Check if this value is a memoized function
    #[inline]
    pub const fn is_memo(&self) -> bool {
        matches!(self, Value::Memo { .. })
    }
    
    /// Check if this value is an array
    #[inline]
    pub const fn is_array(&self) -> bool {
        matches!(self, Value::Array { .. })
    }
    
    /// Get the number value if this is a number
    #[inline]
    pub const fn as_number(&self) -> Option<i64> {
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
            Value::Thunk { .. } => "promise",
            Value::Memo { .. } => "memoized",
            Value::Builtin(_) => "procedure",
            Value::StdLib { .. } => "procedure",
            Value::Array { .. } => "array",
        }
    }
}

/// Implement Trace for GC support
impl<const N: usize> Trace<Value, N> for Value {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match self {
            Value::Nil | Value::True | Value::False | 
            Value::Number(_) | Value::Char(_) | Value::Builtin(_) => {
                // No references
            }
            Value::StdLib { cached_body, cached_params, .. } => {
                // Trace cached parsed body and params if they exist
                if !cached_body.is_null() {
                    tracer(*cached_body);
                }
                if !cached_params.is_null() {
                    tracer(*cached_params);
                }
            }
            Value::Cons { car, cdr } => {
                tracer(*car);
                tracer(*cdr);
            }
            Value::Symbol { chars, len } => {
                tracer(*chars);
                // For contiguous strings (len > 0), trace all character slots
                // They are at chars+1, chars+2, ..., chars+len
                // The GC only uses the raw index from the ArenaIndex, not the generation,
                // so we can safely use chars.generation() for all slots in the contiguous block.
                if *len > 0 {
                    let base_idx = chars.raw();
                    for i in 1..=*len {
                        let char_idx = ArenaIndex::new(base_idx + i, chars.generation());
                        tracer(char_idx);
                    }
                }
            }
            Value::Lambda { params, body, env } => {
                tracer(*params);
                tracer(*body);
                tracer(*env);
            }
            Value::Thunk { expr, env, cached } => {
                tracer(*expr);
                tracer(*env);
                if !cached.is_null() {
                    tracer(*cached);
                }
            }
            Value::Memo { func, cache } => {
                tracer(*func);
                tracer(*cache);
            }
            Value::Array { data, len } => {
                // For non-empty arrays, trace all elements in the contiguous block
                // Empty arrays (len == 0) have data == NULL, so skip tracing
                if *len > 0 {
                    let base_idx = data.raw();
                    for i in 0..*len {
                        let elem_idx = ArenaIndex::new(base_idx + i, data.generation());
                        tracer(elem_idx);
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
    
    /// Allocate a number
    #[inline]
    pub fn number(&self, n: i64) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Number(n))
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
    #[inline]
    pub fn car(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(index)? {
            Value::Cons { car, .. } => Ok(car),
            Value::Nil => self.nil(), // car of nil is nil in classic Lisp
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get cdr of a cons cell
    #[inline]
    pub fn cdr(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.get(index)? {
            Value::Cons { cdr, .. } => Ok(cdr),
            Value::Nil => self.nil(), // cdr of nil is nil in classic Lisp
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
    /// The symbol's `chars` field points to a contiguous string block:
    /// [Number(length), Char(c1), Char(c2), ..., Char(cn)]
    /// 
    /// The symbol's `len` field stores the character count for GC tracing.
    /// 
    /// Symbol interning ensures the same symbol name always returns the same index.
    /// 
    /// This provides ~44% memory savings compared to linked list representation.
    pub fn symbol(&self, name: &str) -> ArenaResult<ArenaIndex> {
        // Create contiguous string for the symbol name
        let char_count = name.chars().count();
        let name_str = self.string(name)?;
        
        // Check intern table
        if let Some(existing_symbol) = self.intern_table_lookup(name_str)? {
            // Free the string we just created since we're using the interned one
            self.string_free(name_str)?;
            return Ok(existing_symbol);
        }
        
        // Not found - create new symbol
        let symbol = self.alloc(Value::Symbol { chars: name_str, len: char_count })?;
        
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
        // First, create contiguous string storage
        let char_count = bytes.len();
        let total_slots = 1 + char_count; // 1 for length + chars
        
        // Allocate contiguous block with default Value::Nil
        let start = self.arena.alloc_contiguous(total_slots, Value::Nil)?;
        
        // Set length in first slot
        self.arena.set(start, Value::Number(char_count as i64))?;
        
        // Set characters in following slots
        for (i, &b) in bytes.iter().enumerate() {
            let char_idx = self.arena.index_at_offset(start, i + 1)?;
            self.arena.set(char_idx, Value::Char(b as char))?;
        }
        
        // Check intern table
        if let Some(existing_symbol) = self.intern_table_lookup(start)? {
            // Free the string we just created since we're using the interned one
            self.string_free(start)?;
            return Ok(existing_symbol);
        }
        
        // Not found - create new symbol
        let symbol = self.alloc(Value::Symbol { chars: start, len: char_count })?;
        
        // Add to intern table: (start . symbol)
        let binding = self.cons(start, symbol)?;
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
    /// StdLib functions are stored in static memory with lazy caching.
    /// The function body is parsed on first call and cached for reuse.
    #[inline]
    pub fn stdlib(&self, s: StdLib) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::StdLib { 
            func: s, 
            cached_body: ArenaIndex::NULL, 
            cached_params: ArenaIndex::NULL 
        })
    }
    
    /// Allocate a lambda
    pub fn lambda(&self, params: ArenaIndex, body: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Lambda { params, body, env })
    }
    
    /// Allocate a thunk (delayed computation)
    pub fn thunk(&self, expr: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Thunk { expr, env, cached: ArenaIndex::NULL })
    }
    
    /// Create a memoized function (wraps a function with a cache)
    pub fn memo(&self, func: ArenaIndex, cache: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.alloc(Value::Memo { func, cache })
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
    
    /// Check if two symbols are equal (supports both contiguous and char list formats)
    #[inline]
    pub fn symbol_eq(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        // Fast path: same index means same symbol
        if a == b {
            return Ok(true);
        }
        
        let val_a = self.get(a)?;
        let val_b = self.get(b)?;
        
        match (val_a, val_b) {
            (Value::Symbol { chars: chars_a, len: len_a }, Value::Symbol { chars: chars_b, len: len_b }) => {
                // Use len to determine format: len > 0 means contiguous, len == 0 means char list
                match (len_a > 0, len_b > 0) {
                    (true, true) => self.string_eq_contiguous(chars_a, chars_b),
                    (false, false) => self.char_list_eq(chars_a, chars_b),
                    _ => Ok(false), // Different formats can't be equal
                }
            }
            _ => Ok(false),
        }
    }
    
    /// Compare two char lists for equality (legacy format)
    fn char_list_eq(&self, mut a: ArenaIndex, mut b: ArenaIndex) -> ArenaResult<bool> {
        // Fast path: same index means same char list
        if a == b {
            return Ok(true);
        }
        
        loop {
            let val_a = self.get(a)?;
            let val_b = self.get(b)?;
            
            match (val_a, val_b) {
                (Value::Nil, Value::Nil) => return Ok(true),
                (Value::Cons { car: car_a, cdr: cdr_a }, Value::Cons { car: car_b, cdr: cdr_b }) => {
                    let char_a = self.get(car_a)?;
                    let char_b = self.get(car_b)?;
                    
                    match (char_a, char_b) {
                        (Value::Char(c1), Value::Char(c2)) if c1 == c2 => {
                            a = cdr_a;
                            b = cdr_b;
                        }
                        _ => return Ok(false),
                    }
                }
                _ => return Ok(false),
            }
        }
    }
    
    /// Check if a symbol matches a string (supports both contiguous and char list formats)
    #[inline]
    pub fn symbol_matches(&self, sym: ArenaIndex, name: &str) -> ArenaResult<bool> {
        let val = self.get(sym)?;
        
        match val {
            Value::Symbol { chars, len } => {
                // Use len to determine format: len > 0 means contiguous
                if len > 0 {
                    return self.string_matches(chars, name);
                }
                
                // Legacy char list format
                let mut list = chars;
                let mut chars_iter = name.chars();
                
                loop {
                    let list_val = self.get(list)?;
                    let next_char = chars_iter.next();
                    
                    match (list_val, next_char) {
                        (Value::Nil, None) => return Ok(true),
                        (Value::Cons { car, cdr }, Some(expected)) => {
                            if let Value::Char(c) = self.get(car)? {
                                if c != expected {
                                    return Ok(false);
                                }
                                list = cdr;
                            } else {
                                return Ok(false);
                            }
                        }
                        _ => return Ok(false),
                    }
                }
            }
            _ => Ok(false),
        }
    }
    
    /// Extract symbol name to a fixed buffer (supports both contiguous and char list formats)
    pub fn symbol_to_bytes(&self, sym: ArenaIndex, buf: &mut [u8]) -> ArenaResult<usize> {
        let val = self.get(sym)?;
        
        match val {
            Value::Symbol { chars, len: sym_len } => {
                // Use len to determine format: len > 0 means contiguous
                if sym_len > 0 {
                    return self.string_to_bytes(chars, buf);
                }
                
                // Legacy char list format
                let mut list = chars;
                let mut written = 0;
                
                loop {
                    if written >= buf.len() {
                        break;
                    }
                    match self.get(list)? {
                        Value::Nil => break,
                        Value::Cons { car, cdr } => {
                            if let Value::Char(c) = self.get(car)? {
                                buf[written] = c as u8;
                                written += 1;
                            }
                            list = cdr;
                        }
                        _ => break,
                    }
                }
                Ok(written)
            }
            _ => Ok(0),
        }
    }
    
    /// Get the length of a symbol's name
    pub fn symbol_len(&self, sym: ArenaIndex) -> ArenaResult<usize> {
        match self.get(sym)? {
            Value::Symbol { len: sym_len, chars } => {
                if sym_len > 0 {
                    Ok(sym_len)
                } else {
                    // Legacy char list format - count the chars
                    let mut list = chars;
                    let mut count = 0;
                    loop {
                        match self.get(list)? {
                            Value::Nil => return Ok(count),
                            Value::Cons { cdr, .. } => {
                                count += 1;
                                list = cdr;
                            }
                            _ => return Ok(count),
                        }
                    }
                }
            }
            _ => Ok(0),
        }
    }
    
    /// Get a character at a specific index within a symbol's name
    /// Returns None if the index is out of bounds or if the value is not a symbol
    pub fn symbol_char_at(&self, sym: ArenaIndex, index: usize) -> ArenaResult<Option<char>> {
        match self.get(sym)? {
            Value::Symbol { chars, len: sym_len } => {
                if sym_len > 0 {
                    // Contiguous string format
                    if index >= sym_len {
                        return Ok(None);
                    }
                    Ok(Some(self.string_char_at(chars, index)?))
                } else {
                    // Legacy char list format
                    let mut list = chars;
                    let mut current_idx = 0;
                    loop {
                        match self.get(list)? {
                            Value::Nil => return Ok(None),
                            Value::Cons { car, cdr } => {
                                if current_idx == index {
                                    if let Value::Char(c) = self.get(car)? {
                                        return Ok(Some(c));
                                    }
                                    return Ok(None);
                                }
                                current_idx += 1;
                                list = cdr;
                            }
                            _ => return Ok(None),
                        }
                    }
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
        // Create a new roots array with intern table included
        // Using const-sized array to avoid alloc in no_std
        // 512 roots should be sufficient for most programs while keeping
        // stack usage reasonable (~8KB on 64-bit systems)
        const MAX_ROOTS: usize = 512;
        
        // Panic if too many roots - this indicates a programming error
        assert!(roots.len() < MAX_ROOTS, 
            "Too many GC roots: {} (max {})", roots.len(), MAX_ROOTS - 1);
        
        let mut all_roots = [ArenaIndex::NULL; MAX_ROOTS];
        
        // Add intern table reference cell as first root
        // This is a cons cell whose car is the intern table alist
        // Tracing from this cell will reach all interned symbols
        all_roots[0] = self.intern_table_slot;
        let mut root_count = 1;
        
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
    // Strings are stored as contiguous sequences of Value::Char in the arena:
    // [Number(length), Char(c1), Char(c2), ..., Char(cn)]
    // 
    // This provides:
    // - O(1) length lookup
    // - Cache-friendly sequential access
    // - ~40% memory savings vs linked list representation
    // ========================================================================
    
    /// Allocate a string as contiguous Char values.
    /// 
    /// Format: [Number(length), Char, Char, ..., Char]
    /// 
    /// The returned index points to the length slot. Character slots follow
    /// immediately after in contiguous memory.
    /// 
    /// # Memory Usage
    /// 
    /// Allocates `1 + s.chars().count()` slots:
    /// - 1 slot for the length (stored as Number)
    /// - 1 slot per character (stored as Char)
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::OutOfMemory` if:
    /// - No contiguous block is available
    /// - String length exceeds i64 (unlikely)
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
        let total_slots = 1 + char_count; // 1 for length + chars
        
        // Allocate contiguous block with default Value::Nil
        let start = self.arena.alloc_contiguous(total_slots, Value::Nil)?;
        
        // Set length in first slot
        self.arena.set(start, Value::Number(char_count as i64))?;
        
        // Set characters in following slots
        for (i, c) in s.chars().enumerate() {
            let char_idx = self.arena.index_at_offset(start, i + 1)?;
            self.arena.set(char_idx, Value::Char(c))?;
        }
        
        Ok(start)
    }
    
    /// Get the length of a contiguous string.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if the index doesn't point to
    /// a valid string (i.e., a Number value representing length).
    pub fn string_len(&self, str_idx: ArenaIndex) -> ArenaResult<usize> {
        match self.arena.get(str_idx)? {
            Value::Number(len) if len >= 0 => Ok(len as usize),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Get a character at the given index within a contiguous string.
    /// 
    /// Character indices are 0-based.
    /// 
    /// # Errors
    /// 
    /// Returns `ArenaError::InvalidIndex` if:
    /// - The string index is invalid
    /// - The character index is out of bounds
    /// - The slot doesn't contain a Char value
    pub fn string_char_at(&self, str_idx: ArenaIndex, char_index: usize) -> ArenaResult<char> {
        let len = self.string_len(str_idx)?;
        if char_index >= len {
            return Err(ArenaError::InvalidIndex);
        }
        
        let char_slot = self.arena.index_at_offset(str_idx, char_index + 1)?;
        match self.arena.get(char_slot)? {
            Value::Char(c) => Ok(c),
            _ => Err(ArenaError::InvalidIndex),
        }
    }
    
    /// Compare two contiguous strings for equality.
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
    
    /// Check if a contiguous string matches a Rust string slice.
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
    
    /// Copy a contiguous string's contents to a byte buffer.
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
    
    /// Free a contiguous string and all its character slots.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the string index is invalid.
    pub fn string_free(&self, str_idx: ArenaIndex) -> ArenaResult<()> {
        let len = self.string_len(str_idx)?;
        self.arena.free_contiguous(str_idx, 1 + len)
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
    pub line: u32,
    pub column: u32,
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
}

impl ParseError {
    pub fn new(kind: ParseErrorKind, line: u32, column: u32) -> Self {
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
    line: u32,
    column: u32,
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
            
            Some(b'-') => {
                // Could be negative number or symbol
                if self.peek_next().map_or(false, |c| c.is_ascii_digit()) {
                    self.parse_number(lisp)
                } else {
                    self.parse_symbol(lisp)
                }
            }
            
            Some(c) if Self::is_symbol_char(c) => self.parse_symbol(lisp),
            
            Some(c) => Err(self.error(ParseErrorKind::UnexpectedChar(c as char))),
        }
    }
    
    /// Parse hash literals (#t, #f, etc.)
    fn parse_hash_literal<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        self.advance(); // consume '#'
        
        match self.peek() {
            Some(b't') | Some(b'T') => {
                self.advance();
                lisp.true_val().map_err(Into::into)
            }
            Some(b'f') | Some(b'F') => {
                self.advance();
                lisp.false_val().map_err(Into::into)
            }
            Some(_) => Err(self.error(ParseErrorKind::InvalidHashLiteral)),
            None => Err(self.error(ParseErrorKind::UnexpectedEof)),
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
    
    /// Parse a number
    fn parse_number<const N: usize>(&mut self, lisp: &Lisp<N>) -> Result<ArenaIndex, ParseError> {
        let mut value: i64 = 0;
        let negative = if self.peek() == Some(b'-') {
            self.advance();
            true
        } else {
            false
        };
        
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
                value = value.checked_mul(10)
                    .and_then(|v| v.checked_add((c - b'0') as i64))
                    .ok_or_else(|| self.error(ParseErrorKind::NumberOverflow))?;
            } else {
                break;
            }
        }
        
        if negative {
            value = -value;
        }
        
        lisp.number(value).map_err(Into::into)
    }
    
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
        
        // Check for special symbols
        if name == b"nil" {
            return lisp.nil().map_err(Into::into);
        }
        // Note: #t and #f are now the canonical booleans
        // but we can still allow 'true' and 'false' as symbols that 
        // the evaluator can bind to #t and #f
        
        lisp.symbol_from_bytes(name).map_err(Into::into)
    }
    
    /// Check if there's more input (after whitespace)
    pub fn has_more(&mut self) -> bool {
        self.skip_whitespace();
        self.peek().is_some()
    }
    
    /// Get current position for error reporting
    pub fn position(&self) -> (u32, u32) {
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    // ========================================================================
    // Reserved Slots Tests
    // ========================================================================
    
    #[test]
    fn test_reserved_slots_are_singletons() {
        let lisp: Lisp<100> = Lisp::new();
        
        // Multiple calls to nil() should return the same index
        let nil1 = lisp.nil().unwrap();
        let nil2 = lisp.nil().unwrap();
        let nil3 = lisp.nil().unwrap();
        assert_eq!(nil1, nil2);
        assert_eq!(nil2, nil3);
        
        // Multiple calls to true_val() should return the same index
        let true1 = lisp.true_val().unwrap();
        let true2 = lisp.true_val().unwrap();
        let true3 = lisp.true_val().unwrap();
        assert_eq!(true1, true2);
        assert_eq!(true2, true3);
        
        // Multiple calls to false_val() should return the same index
        let false1 = lisp.false_val().unwrap();
        let false2 = lisp.false_val().unwrap();
        let false3 = lisp.false_val().unwrap();
        assert_eq!(false1, false2);
        assert_eq!(false2, false3);
        
        // Each singleton should be different
        assert_ne!(nil1, true1);
        assert_ne!(nil1, false1);
        assert_ne!(true1, false1);
    }
    
    #[test]
    fn test_reserved_slots_have_correct_values() {
        let lisp: Lisp<100> = Lisp::new();
        
        // Verify the values in reserved slots
        assert_eq!(lisp.get(lisp.nil().unwrap()).unwrap(), Value::Nil);
        assert_eq!(lisp.get(lisp.true_val().unwrap()).unwrap(), Value::True);
        assert_eq!(lisp.get(lisp.false_val().unwrap()).unwrap(), Value::False);
    }
    
    #[test]
    fn test_reserved_slots_occupy_first_three_slots() {
        let lisp: Lisp<100> = Lisp::new();
        
        // Reserved slots should be the first 4 slots (nil, true, false, intern_table_ref)
        assert_eq!(lisp.nil().unwrap().raw(), 0);
        assert_eq!(lisp.true_val().unwrap().raw(), 1);
        assert_eq!(lisp.false_val().unwrap().raw(), 2);
    }
    
    #[test]
    fn test_reserved_slots_not_reallocated() {
        let lisp: Lisp<100> = Lisp::new();
        
        // After creating the Lisp context, 4 slots should be used
        // (nil, true, false, intern_table_ref)
        assert_eq!(lisp.arena().len(), 4);
        
        // Calling nil/true_val/false_val should NOT increase allocation count
        let _ = lisp.nil();
        let _ = lisp.true_val();
        let _ = lisp.false_val();
        assert_eq!(lisp.arena().len(), 4);
        
        // Calling many times should not increase count
        for _ in 0..100 {
            let _ = lisp.nil();
            let _ = lisp.true_val();
            let _ = lisp.false_val();
        }
        assert_eq!(lisp.arena().len(), 4);
    }
    
    #[test]
    fn test_boolean_uses_reserved_slots() {
        let lisp: Lisp<100> = Lisp::new();
        
        // boolean() should use the reserved slots
        let b_true = lisp.boolean(true).unwrap();
        let b_false = lisp.boolean(false).unwrap();
        
        assert_eq!(b_true, lisp.true_val().unwrap());
        assert_eq!(b_false, lisp.false_val().unwrap());
    }
    
    #[test]
    fn test_reserved_slots_survive_gc() {
        let lisp: Lisp<100> = Lisp::new();
        
        let nil = lisp.nil().unwrap();
        let true_val = lisp.true_val().unwrap();
        let false_val = lisp.false_val().unwrap();
        
        // Allocate some garbage
        let _ = lisp.number(1);
        let _ = lisp.number(2);
        let _ = lisp.number(3);
        
        // Run GC with empty roots - reserved slots should NOT be collected
        // because they're implicitly roots
        let stats = lisp.gc(&[nil, true_val, false_val]);
        
        // The numbers should be collected
        assert_eq!(stats.collected, 3);
        
        // Reserved slots should still be valid
        assert_eq!(lisp.get(nil).unwrap(), Value::Nil);
        assert_eq!(lisp.get(true_val).unwrap(), Value::True);
        assert_eq!(lisp.get(false_val).unwrap(), Value::False);
    }
    
    #[test]
    fn test_regular_allocation_starts_after_reserved_slots() {
        let lisp: Lisp<100> = Lisp::new();
        
        // First regular allocation should be at slot 4 (after reserved 0, 1, 2, 3)
        // Slots: 0=nil, 1=true, 2=false, 3=intern_table_ref
        let num = lisp.number(42).unwrap();
        assert_eq!(num.raw(), 4);
        
        // Next allocations continue from there
        let num2 = lisp.number(43).unwrap();
        assert_eq!(num2.raw(), 5);
    }
    
    // ========================================================================
    // Parser Tests
    // ========================================================================
    
    #[test]
    fn test_parse_number() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "42").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::Number(42));
        
        let idx = parse(&lisp, "-123").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::Number(-123));
    }
    
    #[test]
    fn test_parse_symbol() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "hello").unwrap();
        assert!(lisp.symbol_matches(idx, "hello").unwrap());
    }
    
    #[test]
    fn test_parse_nil() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "nil").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::Nil);
        
        let idx = parse(&lisp, "()").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::Nil);
    }
    
    #[test]
    fn test_parse_booleans() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "#t").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::True);
        
        let idx = parse(&lisp, "#f").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::False);
        
        let idx = parse(&lisp, "#T").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::True);
        
        let idx = parse(&lisp, "#F").unwrap();
        assert_eq!(lisp.get(idx).unwrap(), Value::False);
    }
    
    #[test]
    fn test_parse_list() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "(1 2 3)").unwrap();
        
        // Check it's a cons
        let val = lisp.get(idx).unwrap();
        assert!(val.is_cons());
        
        // Check first element
        let car = lisp.car(idx).unwrap();
        assert_eq!(lisp.get(car).unwrap(), Value::Number(1));
    }
    
    #[test]
    fn test_parse_nested() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "(+ 1 (- 3 2))").unwrap();
        assert!(lisp.get(idx).unwrap().is_cons());
    }
    
    #[test]
    fn test_parse_quote() {
        let lisp: Lisp<100> = Lisp::new();
        
        let idx = parse(&lisp, "'x").unwrap();
        
        // Should be (quote x)
        let car = lisp.car(idx).unwrap();
        assert!(lisp.symbol_matches(car, "quote").unwrap());
    }
    
    #[test]
    fn test_symbol_equality() {
        let lisp: Lisp<100> = Lisp::new();
        
        let a = lisp.symbol("hello").unwrap();
        let b = lisp.symbol("hello").unwrap();
        let c = lisp.symbol("world").unwrap();
        
        assert!(lisp.symbol_eq(a, b).unwrap());
        assert!(!lisp.symbol_eq(a, c).unwrap());
    }
    
    // NOTE: test_set_car_cdr removed - this is a PURE Lisp!
    
    #[test]
    fn test_gc() {
        let lisp: Lisp<100> = Lisp::new();
        
        let root = parse(&lisp, "(1 2 3)").unwrap();
        
        // Allocate garbage
        for i in 0..20 {
            lisp.number(i * 1000).unwrap();
        }
        
        let stats = lisp.gc(&[root]);
        assert!(stats.collected > 0);
    }
    
    #[test]
    fn test_thunk_creation() {
        let lisp: Lisp<100> = Lisp::new();
        
        let expr = lisp.number(42).unwrap();
        let env = lisp.nil().unwrap();
        let thunk = lisp.thunk(expr, env).unwrap();
        
        assert!(lisp.get(thunk).unwrap().is_thunk());
    }
    
    #[test]
    fn test_thunk_structure() {
        let lisp: Lisp<100> = Lisp::new();
        
        let expr = lisp.number(42).unwrap();
        let env = lisp.nil().unwrap();
        let thunk = lisp.thunk(expr, env).unwrap();
        
        // Check thunk structure
        match lisp.get(thunk).unwrap() {
            Value::Thunk { expr: e, env: en, cached: c } => {
                assert_eq!(lisp.get(e).unwrap(), Value::Number(42));
                assert_eq!(lisp.get(en).unwrap(), Value::Nil);
                assert!(c.is_null()); // Not yet cached
            }
            _ => panic!("Expected Thunk"),
        }
    }
    
    #[test]
    fn test_thunk_with_complex_expr() {
        let lisp: Lisp<200> = Lisp::new();
        
        // Create a complex expression as the delayed expr
        let one = lisp.number(1).unwrap();
        let two = lisp.number(2).unwrap();
        let pair = lisp.cons(one, two).unwrap();
        let env = lisp.nil().unwrap();
        
        let thunk = lisp.thunk(pair, env).unwrap();
        
        assert!(lisp.get(thunk).unwrap().is_thunk());
        
        // Verify the expr is our pair
        match lisp.get(thunk).unwrap() {
            Value::Thunk { expr, .. } => {
                match lisp.get(expr).unwrap() {
                    Value::Cons { car, cdr } => {
                        assert_eq!(lisp.get(car).unwrap(), Value::Number(1));
                        assert_eq!(lisp.get(cdr).unwrap(), Value::Number(2));
                    }
                    _ => panic!("Expected Cons in thunk"),
                }
            }
            _ => panic!("Expected Thunk"),
        }
    }
    
    #[test]
    fn test_thunk_with_environment() {
        let lisp: Lisp<300> = Lisp::new();
        
        // Create a non-empty environment
        let name = lisp.symbol("x").unwrap();
        let value = lisp.number(100).unwrap();
        let binding = lisp.cons(name, value).unwrap();
        let env = lisp.cons(binding, lisp.nil().unwrap()).unwrap();
        
        let expr = lisp.number(42).unwrap();
        let thunk = lisp.thunk(expr, env).unwrap();
        
        // Verify environment is captured
        match lisp.get(thunk).unwrap() {
            Value::Thunk { env: e, .. } => {
                // First binding
                let first = lisp.car(e).unwrap();
                let bound_name = lisp.car(first).unwrap();
                let bound_val = lisp.cdr(first).unwrap();
                
                assert!(lisp.symbol_matches(bound_name, "x").unwrap());
                assert_eq!(lisp.get(bound_val).unwrap(), Value::Number(100));
            }
            _ => panic!("Expected Thunk"),
        }
    }
    
    #[test]
    fn test_thunk_gc_trace() {
        // Test that GC properly traces through thunks
        let lisp: Lisp<200> = Lisp::new();
        
        // Create data that will be referenced by thunk
        let inner_data = lisp.cons(lisp.number(1).unwrap(), lisp.number(2).unwrap()).unwrap();
        let env_val = lisp.number(999).unwrap();
        let env_name = lisp.symbol("y").unwrap();
        let binding = lisp.cons(env_name, env_val).unwrap();
        let env = lisp.cons(binding, lisp.nil().unwrap()).unwrap();
        
        let thunk = lisp.thunk(inner_data, env).unwrap();
        
        // Create garbage
        for i in 0..50 {
            lisp.number(i * 1000).unwrap();
        }
        
        // Run GC with thunk as root
        let stats = lisp.gc(&[thunk]);
        assert!(stats.collected > 0);
        
        // Thunk and its referenced data should still be accessible
        assert!(lisp.get(thunk).unwrap().is_thunk());
        match lisp.get(thunk).unwrap() {
            Value::Thunk { expr, env, .. } => {
                // inner_data should still be valid
                assert!(lisp.get(expr).unwrap().is_cons());
                // environment should still be valid
                assert!(lisp.get(env).unwrap().is_cons());
            }
            _ => panic!("Expected Thunk"),
        }
    }
    
    #[test]
    fn test_thunk_value_predicates() {
        let lisp: Lisp<100> = Lisp::new();
        
        let expr = lisp.number(42).unwrap();
        let env = lisp.nil().unwrap();
        let thunk = lisp.thunk(expr, env).unwrap();
        let thunk_val = lisp.get(thunk).unwrap();
        
        // Test all predicates on thunk
        assert!(thunk_val.is_thunk());
        assert!(!thunk_val.is_nil());
        assert!(!thunk_val.is_number());
        assert!(!thunk_val.is_symbol());
        assert!(!thunk_val.is_cons());
        assert!(!thunk_val.is_lambda());
        assert!(!thunk_val.is_builtin());
        assert!(!thunk_val.is_true());
        assert!(!thunk_val.is_false());
        assert!(!thunk_val.is_boolean());
        assert!(thunk_val.is_atom()); // Thunks are atoms
        assert!(!thunk_val.is_procedure()); // Thunks are not procedures
        
        // Type name
        assert_eq!(thunk_val.type_name(), "promise");
    }
    
    #[test]
    fn test_multiple_thunks() {
        let lisp: Lisp<500> = Lisp::new();
        
        // Create multiple thunks
        let env = lisp.nil().unwrap();
        let t1 = lisp.thunk(lisp.number(1).unwrap(), env).unwrap();
        let t2 = lisp.thunk(lisp.number(2).unwrap(), env).unwrap();
        let t3 = lisp.thunk(lisp.number(3).unwrap(), env).unwrap();
        
        // Put them in a list
        let list = lisp.cons(t3, lisp.nil().unwrap()).unwrap();
        let list = lisp.cons(t2, list).unwrap();
        let list = lisp.cons(t1, list).unwrap();
        
        // All should be thunks
        let first = lisp.car(list).unwrap();
        assert!(lisp.get(first).unwrap().is_thunk());
        
        let second = lisp.car(lisp.cdr(list).unwrap()).unwrap();
        assert!(lisp.get(second).unwrap().is_thunk());
        
        let third = lisp.car(lisp.cdr(lisp.cdr(list).unwrap()).unwrap()).unwrap();
        assert!(lisp.get(third).unwrap().is_thunk());
    }
    
    // ========================================================================
    // Contiguous String Tests
    // ========================================================================
    
    #[test]
    fn test_string_basic() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let hello = lisp.string("hello").unwrap();
        
        assert_eq!(lisp.string_len(hello).unwrap(), 5);
        assert_eq!(lisp.string_char_at(hello, 0).unwrap(), 'h');
        assert_eq!(lisp.string_char_at(hello, 1).unwrap(), 'e');
        assert_eq!(lisp.string_char_at(hello, 2).unwrap(), 'l');
        assert_eq!(lisp.string_char_at(hello, 3).unwrap(), 'l');
        assert_eq!(lisp.string_char_at(hello, 4).unwrap(), 'o');
    }
    
    #[test]
    fn test_string_empty() {
        let lisp: Lisp<100> = Lisp::new();
        
        let empty = lisp.string("").unwrap();
        
        assert_eq!(lisp.string_len(empty).unwrap(), 0);
        // Accessing index 0 on empty string should fail
        assert!(lisp.string_char_at(empty, 0).is_err());
    }
    
    #[test]
    fn test_string_single_char() {
        let lisp: Lisp<100> = Lisp::new();
        
        let single = lisp.string("x").unwrap();
        
        assert_eq!(lisp.string_len(single).unwrap(), 1);
        assert_eq!(lisp.string_char_at(single, 0).unwrap(), 'x');
        assert!(lisp.string_char_at(single, 1).is_err());
    }
    
    #[test]
    fn test_string_unicode() {
        let lisp: Lisp<100> = Lisp::new();
        
        // Unicode string: "héllo" (with accent)
        let unicode = lisp.string("héllo").unwrap();
        
        assert_eq!(lisp.string_len(unicode).unwrap(), 5);
        assert_eq!(lisp.string_char_at(unicode, 0).unwrap(), 'h');
        assert_eq!(lisp.string_char_at(unicode, 1).unwrap(), 'é');
        assert_eq!(lisp.string_char_at(unicode, 2).unwrap(), 'l');
    }
    
    #[test]
    fn test_string_matches() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let hello = lisp.string("hello").unwrap();
        
        assert!(lisp.string_matches(hello, "hello").unwrap());
        assert!(!lisp.string_matches(hello, "Hello").unwrap());
        assert!(!lisp.string_matches(hello, "hello!").unwrap());
        assert!(!lisp.string_matches(hello, "hell").unwrap());
        assert!(!lisp.string_matches(hello, "").unwrap());
    }
    
    #[test]
    fn test_string_eq_contiguous() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let hello1 = lisp.string("hello").unwrap();
        let hello2 = lisp.string("hello").unwrap();
        let world = lisp.string("world").unwrap();
        
        // Same content
        assert!(lisp.string_eq_contiguous(hello1, hello2).unwrap());
        
        // Same index
        assert!(lisp.string_eq_contiguous(hello1, hello1).unwrap());
        
        // Different content
        assert!(!lisp.string_eq_contiguous(hello1, world).unwrap());
    }
    
    #[test]
    fn test_string_to_bytes() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let hello = lisp.string("hello").unwrap();
        let mut buf = [0u8; 10];
        
        let len = lisp.string_to_bytes(hello, &mut buf).unwrap();
        
        assert_eq!(len, 5);
        assert_eq!(&buf[..5], b"hello");
    }
    
    #[test]
    fn test_string_to_bytes_truncated() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let hello = lisp.string("hello").unwrap();
        let mut buf = [0u8; 3]; // Too small
        
        let len = lisp.string_to_bytes(hello, &mut buf).unwrap();
        
        assert_eq!(len, 3);
        assert_eq!(&buf[..3], b"hel");
    }
    
    #[test]
    fn test_string_to_bytes_skips_non_ascii() {
        let lisp: Lisp<1000> = Lisp::new();
        
        // "hé" has 2 chars: 'h' (ASCII) and 'é' (non-ASCII)
        let mixed = lisp.string("héllo").unwrap();
        let mut buf = [0u8; 10];
        
        let len = lisp.string_to_bytes(mixed, &mut buf).unwrap();
        
        // Only ASCII chars are copied, 'é' is skipped
        assert_eq!(len, 4); // h, l, l, o
        assert_eq!(&buf[..4], b"hllo");
    }
    
    #[test]
    fn test_string_free() {
        let lisp: Lisp<100> = Lisp::new();
        
        let initial_allocated = lisp.stats().allocated;
        
        let hello = lisp.string("hello").unwrap();
        let after_alloc = lisp.stats().allocated;
        
        // Should have allocated 6 slots (1 length + 5 chars)
        assert_eq!(after_alloc - initial_allocated, 6);
        
        lisp.string_free(hello).unwrap();
        let after_free = lisp.stats().allocated;
        
        // Should be back to initial
        assert_eq!(after_free, initial_allocated);
    }
    
    #[test]
    fn test_string_multiple() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let s1 = lisp.string("foo").unwrap();
        let s2 = lisp.string("bar").unwrap();
        let s3 = lisp.string("baz").unwrap();
        
        // All strings should be independent
        assert!(lisp.string_matches(s1, "foo").unwrap());
        assert!(lisp.string_matches(s2, "bar").unwrap());
        assert!(lisp.string_matches(s3, "baz").unwrap());
        
        // Free one, others should still work
        lisp.string_free(s2).unwrap();
        
        assert!(lisp.string_matches(s1, "foo").unwrap());
        assert!(lisp.string_matches(s3, "baz").unwrap());
    }
    
    #[test]
    fn test_string_memory_layout() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let hello = lisp.string("hello").unwrap();
        
        // First slot should be Number(5)
        assert_eq!(lisp.get(hello).unwrap(), Value::Number(5));
        
        // Following slots should be Char values
        let idx1 = lisp.arena().index_at_offset(hello, 1).unwrap();
        let idx2 = lisp.arena().index_at_offset(hello, 2).unwrap();
        
        assert_eq!(lisp.get(idx1).unwrap(), Value::Char('h'));
        assert_eq!(lisp.get(idx2).unwrap(), Value::Char('e'));
    }
    
    #[test]
    fn test_string_char_out_of_bounds() {
        let lisp: Lisp<100> = Lisp::new();
        
        let hello = lisp.string("hi").unwrap();
        
        assert!(lisp.string_char_at(hello, 0).is_ok());
        assert!(lisp.string_char_at(hello, 1).is_ok());
        assert!(lisp.string_char_at(hello, 2).is_err());
        assert!(lisp.string_char_at(hello, 100).is_err());
    }
    
    // ========================================================================
    // Mutation Tests (set_car, set_cdr)
    // ========================================================================
    
    #[test]
    fn test_set_car() {
        let lisp: Lisp<100> = Lisp::new();
        
        let a = lisp.number(1).unwrap();
        let b = lisp.number(2).unwrap();
        let c = lisp.number(3).unwrap();
        
        let pair = lisp.cons(a, b).unwrap();
        
        // Initially car is a (1)
        assert_eq!(lisp.car(pair).unwrap(), a);
        
        // Mutate car to c (3)
        lisp.set_car(pair, c).unwrap();
        
        // Now car should be c
        assert_eq!(lisp.car(pair).unwrap(), c);
        
        // cdr should be unchanged
        assert_eq!(lisp.cdr(pair).unwrap(), b);
    }
    
    #[test]
    fn test_set_cdr() {
        let lisp: Lisp<100> = Lisp::new();
        
        let a = lisp.number(1).unwrap();
        let b = lisp.number(2).unwrap();
        let c = lisp.number(3).unwrap();
        
        let pair = lisp.cons(a, b).unwrap();
        
        // Initially cdr is b (2)
        assert_eq!(lisp.cdr(pair).unwrap(), b);
        
        // Mutate cdr to c (3)
        lisp.set_cdr(pair, c).unwrap();
        
        // Now cdr should be c
        assert_eq!(lisp.cdr(pair).unwrap(), c);
        
        // car should be unchanged
        assert_eq!(lisp.car(pair).unwrap(), a);
    }
    
    #[test]
    fn test_set_car_on_non_pair_fails() {
        let lisp: Lisp<100> = Lisp::new();
        
        let num = lisp.number(42).unwrap();
        let new_val = lisp.number(99).unwrap();
        
        assert!(lisp.set_car(num, new_val).is_err());
    }
    
    #[test]
    fn test_set_cdr_on_non_pair_fails() {
        let lisp: Lisp<100> = Lisp::new();
        
        let num = lisp.number(42).unwrap();
        let new_val = lisp.number(99).unwrap();
        
        assert!(lisp.set_cdr(num, new_val).is_err());
    }
    
    // ========================================================================
    // Symbol Interning Tests
    // ========================================================================
    
    #[test]
    fn test_symbol_interning_same_name_returns_same_index() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let sym1 = lisp.symbol("foo").unwrap();
        let sym2 = lisp.symbol("foo").unwrap();
        let sym3 = lisp.symbol("foo").unwrap();
        
        // Same symbol name should return the same index
        assert_eq!(sym1, sym2);
        assert_eq!(sym2, sym3);
    }
    
    #[test]
    fn test_symbol_interning_different_names_return_different_indices() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let foo = lisp.symbol("foo").unwrap();
        let bar = lisp.symbol("bar").unwrap();
        let baz = lisp.symbol("baz").unwrap();
        
        // Different symbol names should return different indices
        assert_ne!(foo, bar);
        assert_ne!(bar, baz);
        assert_ne!(foo, baz);
    }
    
    #[test]
    fn test_symbol_interning_from_bytes() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let sym1 = lisp.symbol("test").unwrap();
        let sym2 = lisp.symbol_from_bytes(b"test").unwrap();
        
        // Same content should return the same symbol
        assert_eq!(sym1, sym2);
    }
    
    #[test]
    fn test_symbol_interning_preserves_content() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let sym = lisp.symbol("hello").unwrap();
        
        // The symbol should still match its name
        assert!(lisp.symbol_matches(sym, "hello").unwrap());
        assert!(!lisp.symbol_matches(sym, "world").unwrap());
    }
    
    #[test]
    fn test_intern_table_is_gc_root() {
        let lisp: Lisp<1000> = Lisp::new();
        
        // Create some interned symbols
        let sym1 = lisp.symbol("a").unwrap();
        let sym2 = lisp.symbol("b").unwrap();
        let sym3 = lisp.symbol("c").unwrap();
        
        // Create some garbage
        for i in 0..50 {
            let _ = lisp.number(i);
        }
        
        // Run GC with no explicit roots
        let empty_roots: &[ArenaIndex] = &[];
        lisp.gc(empty_roots);
        
        // Interned symbols should still be accessible
        assert!(lisp.get(sym1).is_ok());
        assert!(lisp.get(sym2).is_ok());
        assert!(lisp.get(sym3).is_ok());
        
        // And should still match their names
        assert!(lisp.symbol_matches(sym1, "a").unwrap());
        assert!(lisp.symbol_matches(sym2, "b").unwrap());
        assert!(lisp.symbol_matches(sym3, "c").unwrap());
    }
    
    // ========================================================================
    // Symbol Helper Method Tests
    // ========================================================================
    
    #[test]
    fn test_symbol_len() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let empty = lisp.symbol("").unwrap();
        let short = lisp.symbol("hi").unwrap();
        let longer = lisp.symbol("hello world").unwrap();
        
        assert_eq!(lisp.symbol_len(empty).unwrap(), 0);
        assert_eq!(lisp.symbol_len(short).unwrap(), 2);
        assert_eq!(lisp.symbol_len(longer).unwrap(), 11);
    }
    
    #[test]
    fn test_symbol_char_at() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let hello = lisp.symbol("hello").unwrap();
        
        assert_eq!(lisp.symbol_char_at(hello, 0).unwrap(), Some('h'));
        assert_eq!(lisp.symbol_char_at(hello, 1).unwrap(), Some('e'));
        assert_eq!(lisp.symbol_char_at(hello, 2).unwrap(), Some('l'));
        assert_eq!(lisp.symbol_char_at(hello, 3).unwrap(), Some('l'));
        assert_eq!(lisp.symbol_char_at(hello, 4).unwrap(), Some('o'));
        assert_eq!(lisp.symbol_char_at(hello, 5).unwrap(), None);
        assert_eq!(lisp.symbol_char_at(hello, 100).unwrap(), None);
    }
    
    #[test]
    fn test_symbol_to_bytes() {
        let lisp: Lisp<1000> = Lisp::new();
        
        let hello = lisp.symbol("hello").unwrap();
        
        let mut buf = [0u8; 32];
        let len = lisp.symbol_to_bytes(hello, &mut buf).unwrap();
        
        assert_eq!(len, 5);
        assert_eq!(&buf[..len], b"hello");
    }
    
    #[test]
    fn test_contiguous_symbol_uses_less_memory() {
        let lisp: Lisp<10000> = Lisp::new();
        
        // Get initial allocation count (includes reserved slots)
        let initial = lisp.arena().len();
        
        // Create a symbol with contiguous strings
        // "factorial" (9 chars) = 1 (length) + 9 (chars) + 1 (Symbol) = 11 slots
        // Plus 2 slots for the intern table entry
        let _sym = lisp.symbol("factorial").unwrap();
        
        let after_symbol = lisp.arena().len();
        let slots_used = after_symbol - initial;
        
        // Old format would use: 9 Char + 9 Cons + 1 Symbol = 19 slots
        // New format uses: 1 Number + 9 Char + 1 Symbol + 2 Cons (intern table) = 13 slots
        // So we expect significantly fewer slots
        assert!(slots_used < 19, "Expected fewer than 19 slots, got {}", slots_used);
    }
    
    // ========================================================================
    // Array Tests
    // ========================================================================
    
    #[test]
    fn test_array_basic() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        // Create an array of 5 elements initialized to nil
        let arr = lisp.make_array(5, nil).unwrap();
        
        // Check length
        assert_eq!(lisp.array_len(arr).unwrap(), 5);
        
        // Check all elements are nil
        for i in 0..5 {
            let elem = lisp.array_get(arr, i).unwrap();
            assert_eq!(lisp.get(elem).unwrap(), Value::Nil);
        }
    }
    
    #[test]
    fn test_array_empty() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        // Create an empty array
        let arr = lisp.make_array(0, nil).unwrap();
        
        // Check length is 0
        assert_eq!(lisp.array_len(arr).unwrap(), 0);
        
        // Check it's actually an array
        assert!(lisp.get(arr).unwrap().is_array());
    }
    
    #[test]
    fn test_array_get_set() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        // Create array
        let arr = lisp.make_array(3, nil).unwrap();
        
        // Set values
        let val1 = lisp.number(42).unwrap();
        let val2 = lisp.number(100).unwrap();
        lisp.array_set(arr, 0, val1).unwrap();
        lisp.array_set(arr, 2, val2).unwrap();
        
        // Get values back
        let elem0 = lisp.array_get(arr, 0).unwrap();
        let elem1 = lisp.array_get(arr, 1).unwrap();
        let elem2 = lisp.array_get(arr, 2).unwrap();
        
        assert_eq!(lisp.get(elem0).unwrap(), Value::Number(42));
        assert_eq!(lisp.get(elem1).unwrap(), Value::Nil);  // Unchanged
        assert_eq!(lisp.get(elem2).unwrap(), Value::Number(100));
    }
    
    #[test]
    fn test_array_out_of_bounds() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        let arr = lisp.make_array(3, nil).unwrap();
        
        // Valid index
        assert!(lisp.array_get(arr, 2).is_ok());
        
        // Out of bounds
        assert!(lisp.array_get(arr, 3).is_err());
        assert!(lisp.array_get(arr, 100).is_err());
        
        // Out of bounds set
        assert!(lisp.array_set(arr, 3, nil).is_err());
    }
    
    #[test]
    fn test_array_with_different_value_types() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        let arr = lisp.make_array(4, nil).unwrap();
        
        // Set different value types
        let num = lisp.number(123).unwrap();
        let b = lisp.true_val().unwrap();
        let sym = lisp.symbol("foo").unwrap();
        let pair = lisp.cons(lisp.number(1).unwrap(), lisp.number(2).unwrap()).unwrap();
        
        lisp.array_set(arr, 0, num).unwrap();
        lisp.array_set(arr, 1, b).unwrap();
        lisp.array_set(arr, 2, sym).unwrap();
        lisp.array_set(arr, 3, pair).unwrap();
        
        // Verify
        let elem0 = lisp.array_get(arr, 0).unwrap();
        let elem1 = lisp.array_get(arr, 1).unwrap();
        let elem2 = lisp.array_get(arr, 2).unwrap();
        let elem3 = lisp.array_get(arr, 3).unwrap();
        
        assert!(lisp.get(elem0).unwrap().is_number());
        assert!(lisp.get(elem1).unwrap().is_true());
        assert!(lisp.get(elem2).unwrap().is_symbol());
        assert!(lisp.get(elem3).unwrap().is_cons());
    }
    
    #[test]
    fn test_array_is_array_predicate() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        let arr = lisp.make_array(3, nil).unwrap();
        let num = lisp.number(42).unwrap();
        let pair = lisp.cons(nil, nil).unwrap();
        
        assert!(lisp.get(arr).unwrap().is_array());
        assert!(!lisp.get(num).unwrap().is_array());
        assert!(!lisp.get(pair).unwrap().is_array());
        assert!(!lisp.get(nil).unwrap().is_array());
    }
    
    #[test]
    fn test_array_len_on_non_array() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        assert!(lisp.array_len(nil).is_err());
    }
    
    #[test]
    fn test_array_free() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        let initial = lisp.arena().len();
        
        let arr = lisp.make_array(5, nil).unwrap();
        let after_alloc = lisp.arena().len();
        
        // Array should have allocated: 5 data slots + 1 Array value = 6 slots
        assert_eq!(after_alloc - initial, 6);
        
        lisp.array_free(arr).unwrap();
        let after_free = lisp.arena().len();
        
        // All slots should be freed
        assert_eq!(after_free, initial);
    }
    
    #[test]
    fn test_array_memory_layout() {
        let lisp: Lisp<1000> = Lisp::new();
        let nil = lisp.nil().unwrap();
        
        let initial = lisp.arena().len();
        
        // Create array of 10 elements
        let arr = lisp.make_array(10, nil).unwrap();
        
        // Should use: 10 data slots + 1 Array value = 11 slots
        let after = lisp.arena().len();
        assert_eq!(after - initial, 11);
    }
    
    #[test]
    fn test_array_type_name() {
        let arr = Value::Array { 
            data: ArenaIndex::NULL, 
            len: 0 
        };
        assert_eq!(arr.type_name(), "array");
    }
}
