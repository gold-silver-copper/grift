#![no_std]
#![forbid(unsafe_code)]

//! # Lisp Evaluator
//!
//! A Lisp evaluator with **fully trampolined evaluation** - no Rust stack
//! recursion, enabling unlimited recursion depth (bounded only by heap/arena size).
//!
//! ## Key Features
//!
//! - **Full Trampolining**: All evaluation uses continuation-passing style with
//!   an explicit continuation stack. No Rust recursion means no stack overflow.
//! - **Strict Evaluation**: Call-by-value semantics - all arguments are evaluated before function application
//! - **Proper TCO**: Tail calls reuse the same continuation frame
//! - **Lexically Scoped Closures**: First-class functions with captured environments
//! - **Rich Error Handling**: Error messages with stack traces
//! - **Pattern Matching**: `case` for value matching
//! - **Iteration**: `do` loops for imperative-style iteration
//! - **Meta-programming**: `eval` for runtime code evaluation, `quasiquote`/`unquote`
//! - **Mutation**: `set!`, `set-car!`, `set-cdr!` for imperative programming
//!
//! ## Evaluation Strategy
//!
//! This evaluator uses strict (call-by-value) evaluation:
//! - All function arguments are fully evaluated before the function is called
//! - This provides predictable evaluation order and side-effect timing
//! - Tail call optimization is supported for constant-space recursion
//!
//! ## Mutation
//!
//! This Lisp supports mutation operations for imperative programming:
//! - `set!` - Mutate a variable binding
//! - `set-car!` - Mutate the car of a cons cell
//! - `set-cdr!` - Mutate the cdr of a cons cell
//!
//! Note: Mutation breaks referential transparency but enables imperative patterns.
//!
//! ## Truthiness
//!
//! Only `#f` is false. Everything else (including the empty list `'()`) is truthy.
//!
//! ## Special Forms
//!
//! - `quote` - Return expression unevaluated
//! - `if` - Conditional
//! - `cond` - Multi-way conditional
//! - `case` - Pattern matching on values
//! - `lambda` - Create closure
//! - `define` - Define variable/function
//! - `set!` - Mutate variable binding
//! - `let` - Local binding
//! - `let*` - Sequential local binding
//! - `begin` - Sequence of expressions
//! - `and` / `or` - Short-circuit boolean operations
//! - `do` - Iteration with initialization and step expressions
//! - `quasiquote` - Template with `unquote` and `unquote-splicing`
//! - `eval` - Evaluate expression at runtime
//! - `apply` - Apply function to argument list
//! - `values` - Return multiple values as a list

pub use lisp_parser::{
    Arena, ArenaIndex, ArenaError, ArenaResult, Trace, GcStats,
    Value, Builtin, StdLib, Lisp, ParseError, ParseErrorKind, SourceLoc, parse,
};

// Native function interop
pub mod native;
pub use native::{
    FromLisp, ToLisp, NativeRegistry, NativeEntry, NativeFn,
    extract_arg, args_empty, count_args, simple_hash, MAX_NATIVE_FUNCTIONS,
};

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
/// The macro `extract_args!(self, args, a, b, c)` expands to:
/// ```rust
/// # fn example() -> lisp_eval::ArenaResult<()> {
/// #     use lisp_eval::*;
/// #     let lisp = Lisp::<1000>::new();
/// #     let args = lisp.nil()?;
/// #     let a = lisp.car(args)?;
/// #     let args = lisp.cdr(args)?;
/// #     let b = lisp.car(args)?;
/// #     let args = lisp.cdr(args)?;
/// #     let c = lisp.car(args)?;
/// #     Ok(())
/// # }
/// ```
macro_rules! extract_args {
    ($self:expr, $args:ident, $var:ident) => {
        let $var = $self.lisp.car($args)?;
    };
    ($self:expr, $args:ident, $var:ident, $($rest:ident),+) => {
        let $var = $self.lisp.car($args)?;
        #[allow(unused_variables)]
        let $args = $self.lisp.cdr($args)?;
        extract_args!($self, $args, $($rest),+)
    };
}

/// Macro for unary predicate builtins.
///
/// Many builtins follow the pattern of extracting one argument and returning
/// a boolean based on some predicate on the value.
///
/// # Example
/// 
/// The macro `builtin_unary_pred!(self, args, |v| v.is_nil())` expands to:
/// ```rust
/// # fn example() -> lisp_eval::ArenaResult<()> {
/// #     use lisp_eval::*;
/// #     let lisp = Lisp::<1000>::new();
/// #     let args = lisp.nil()?;
/// #     let arg = lisp.car(args)?;
/// #     let val = lisp.get(arg)?;
/// #     let check = |v: Value| v.is_nil();
/// #     lisp.boolean(check(val))?;
/// #     Ok(())
/// # }
/// ```
macro_rules! builtin_unary_pred {
    ($self:expr, $args:expr, $check:expr) => {{
        let arg = $self.lisp.car($args)?;
        let val = $self.lisp.get(arg)?;
        $self.lisp.boolean($check(val)).map_err(Into::into)
    }};
}



// ============================================================================
// Error Handling
// ============================================================================

/// Maximum call stack depth for traces
const MAX_STACK_DEPTH: usize = 64;
/// Maximum frames to include in error backtrace
const MAX_BACKTRACE: usize = 16;

/// Error kind enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Arena is full
    OutOfMemory,
    /// Unbound variable
    UnboundVariable,
    /// Not a function
    NotAFunction,
    /// Wrong number of arguments
    WrongArgCount,
    /// Type error
    TypeError,
    /// Division by zero
    DivisionByZero,
    /// Parse error
    Parse,
    /// User-raised error
    UserError,
    /// Stack overflow (recursion too deep)
    StackOverflow,
    /// Cannot mutate non-pair
    NotAPair,
    /// Generic error
    Generic,
}

impl ErrorKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            ErrorKind::OutOfMemory => "out of memory",
            ErrorKind::UnboundVariable => "unbound variable",
            ErrorKind::NotAFunction => "not a function",
            ErrorKind::WrongArgCount => "wrong number of arguments",
            ErrorKind::TypeError => "type error",
            ErrorKind::DivisionByZero => "division by zero",
            ErrorKind::Parse => "parse error",
            ErrorKind::UserError => "error",
            ErrorKind::StackOverflow => "stack overflow",
            ErrorKind::NotAPair => "not a pair",
            ErrorKind::Generic => "error",
        }
    }
}

/// A stack frame for error reporting
#[derive(Clone, Copy, Debug)]
pub struct StackFrame {
    /// The expression being evaluated (for display)
    pub expr: ArenaIndex,
    /// Function being called (if applicable)
    pub func: ArenaIndex,
}

impl Default for StackFrame {
    fn default() -> Self {
        StackFrame {
            expr: ArenaIndex::NULL,
            func: ArenaIndex::NULL,
        }
    }
}

/// Evaluation error with context
#[derive(Debug)]
pub struct EvalError {
    /// What kind of error
    pub kind: ErrorKind,
    /// Human-readable message context
    pub message: ErrorMessage,
    /// The expression that caused the error
    pub expr: ArenaIndex,
    /// Expected type (for type errors)
    pub expected: Option<&'static str>,
    /// Got type (for type errors)
    pub got: Option<&'static str>,
    /// Expected argument count
    pub expected_args: Option<usize>,
    /// Got argument count
    pub got_args: Option<usize>,
    /// Call stack backtrace
    pub backtrace: [StackFrame; MAX_BACKTRACE],
    pub backtrace_len: usize,
    /// Original parse error (if applicable)
    pub parse_error: Option<ParseError>,
}

/// Fixed-size message buffer for no_std
#[derive(Debug, Clone, Copy)]
pub struct ErrorMessage {
    buf: [u8; 64],
    len: usize,
}

impl ErrorMessage {
    pub const fn empty() -> Self {
        ErrorMessage { buf: [0; 64], len: 0 }
    }
    
    pub fn from_str(s: &str) -> Self {
        let mut msg = ErrorMessage::empty();
        let bytes = s.as_bytes();
        let len = bytes.len().min(64);
        msg.buf[..len].copy_from_slice(&bytes[..len]);
        msg.len = len;
        msg
    }
    
    pub fn as_str(&self) -> &str {
        // We only store UTF-8 bytes; fall back to empty on error
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl Default for ErrorMessage {
    fn default() -> Self {
        Self::empty()
    }
}

impl EvalError {
    pub fn new(kind: ErrorKind) -> Self {
        EvalError {
            kind,
            message: ErrorMessage::empty(),
            expr: ArenaIndex::NULL,
            expected: None,
            got: None,
            expected_args: None,
            got_args: None,
            backtrace: [StackFrame::default(); MAX_BACKTRACE],
            backtrace_len: 0,
            parse_error: None,
        }
    }
    
    pub fn with_expr(mut self, expr: ArenaIndex) -> Self {
        self.expr = expr;
        self
    }
    
    pub fn with_message(mut self, msg: &str) -> Self {
        self.message = ErrorMessage::from_str(msg);
        self
    }
    
    pub fn with_types(mut self, expected: &'static str, got: &'static str) -> Self {
        self.expected = Some(expected);
        self.got = Some(got);
        self
    }
    
    pub fn with_args(mut self, expected: usize, got: usize) -> Self {
        self.expected_args = Some(expected);
        self.got_args = Some(got);
        self
    }
    
    pub fn with_backtrace(mut self, stack: &[StackFrame], len: usize) -> Self {
        let copy_len = len.min(MAX_BACKTRACE);
        self.backtrace[..copy_len].copy_from_slice(&stack[..copy_len]);
        self.backtrace_len = copy_len;
        self
    }
}

impl From<ArenaError> for EvalError {
    fn from(e: ArenaError) -> Self {
        match e {
            ArenaError::OutOfMemory => EvalError::new(ErrorKind::OutOfMemory),
            _ => EvalError::new(ErrorKind::Generic),
        }
    }
}

impl From<ParseError> for EvalError {
    fn from(e: ParseError) -> Self {
        let mut err = EvalError::new(ErrorKind::Parse);
        err.parse_error = Some(e);
        err
    }
}

impl EvalError {
    /// Create an EvalError from a ParseError with expression context
    pub fn from_parse_error(e: ParseError, expr: ArenaIndex) -> Self {
        let mut err = EvalError::new(ErrorKind::Parse);
        err.parse_error = Some(e);
        err.expr = expr;
        err
    }
}

/// Result type for evaluation
pub type EvalResult = Result<ArenaIndex, EvalError>;

/// Result for TCO helper functions (internal)
enum TcoResult {
    /// Return this value immediately
    Return(ArenaIndex),
    /// Continue evaluation with new expression and environment (tail call)
    TailCall { new_expr: ArenaIndex, new_env: ArenaIndex },
}

// ============================================================================
// Continuation-Based Trampoline (Full TCO)
// ============================================================================

/// Maximum continuation stack depth
const MAX_CONT_DEPTH: usize = 1024;

/// Continuation - what to do after a computation completes
#[derive(Clone, Copy, Debug)]
enum Cont {
    /// We're done - return the value
    Done,
    
    /// After evaluating function, decide builtin vs lambda
    ApplyForced { args_expr: ArenaIndex, env: ArenaIndex, call_expr: ArenaIndex },
    
    /// After evaluating condition, choose branch
    IfBranch { then_expr: ArenaIndex, else_expr: ArenaIndex, env: ArenaIndex },
    
    /// After evaluating argument for builtin (variadic ops like +)
    BuiltinForceArg { builtin: Builtin, remaining_args: ArenaIndex, 
                      collected: ArenaIndex, call_expr: ArenaIndex, eval_env: ArenaIndex },
    
    /// OPTIMIZED: After evaluating first arg of binary builtin, evaluate second arg
    BinaryBuiltinFirst { builtin: Builtin, second_arg: ArenaIndex, call_expr: ArenaIndex, eval_env: ArenaIndex },
    
    /// OPTIMIZED: After evaluating both args of binary builtin, apply
    BinaryBuiltinSecond { builtin: Builtin, first_val: ArenaIndex, call_expr: ArenaIndex },
    
    /// After evaluating first lambda arg, bind it to param
    LambdaFirstBind { param: ArenaIndex },
    
    /// After binding a lambda arg, continue with remaining args
    LambdaBindArg { remaining_exprs: ArenaIndex, eval_env: ArenaIndex,
                    remaining_params: ArenaIndex, body: ArenaIndex,
                    new_env: ArenaIndex, call_expr: ArenaIndex },
}

/// Trampoline state - what we're currently doing
#[derive(Clone, Copy, Debug)]
enum TrampolineState {
    /// Evaluate expression in environment
    Eval { expr: ArenaIndex, env: ArenaIndex },
    /// Return a value to the continuation
    Return { val: ArenaIndex },
}

/// Check if a builtin is a binary operation (exactly 2 args, optimized path)
fn is_binary_builtin(builtin: Builtin) -> bool {
    matches!(builtin, 
        Builtin::Add | Builtin::Sub | Builtin::Mul | Builtin::Div | Builtin::Modulo | Builtin::Remainder |
        Builtin::Lt | Builtin::Gt | Builtin::Le | Builtin::Ge | Builtin::NumEq |
        Builtin::EqP | Builtin::EqvP | Builtin::Cons
    )
}

// ============================================================================
// Evaluator
// ============================================================================

/// Maximum number of macros that can be defined
/// The Lisp evaluator with full trampolined TCO
pub struct Evaluator<'a, const N: usize> {
    lisp: &'a Lisp<N>,
    /// Global environment
    global_env: ArenaIndex,
    /// Call stack for error reporting
    call_stack: [StackFrame; MAX_STACK_DEPTH],
    call_stack_depth: usize,
    /// Continuation stack for full trampolining
    cont_stack: [Cont; MAX_CONT_DEPTH],
    cont_depth: usize,
    /// Native function registry
    native_registry: NativeRegistry<N>,
}

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Create a new evaluator with standard environment
    pub fn new(lisp: &'a Lisp<N>) -> Result<Self, EvalError> {
        let mut eval = Evaluator {
            lisp,
            global_env: ArenaIndex::NULL,
            call_stack: [StackFrame::default(); MAX_STACK_DEPTH],
            call_stack_depth: 0,
            cont_stack: [Cont::Done; MAX_CONT_DEPTH],
            cont_depth: 0,
            native_registry: NativeRegistry::new(),
        };
        
        // Initialize global environment with builtins
        eval.global_env = lisp.nil()?;
        
        for &builtin in Builtin::ALL {
            let name = lisp.symbol(builtin.name())?;
            let val = lisp.builtin(builtin)?;
            eval.global_env = eval.env_extend(eval.global_env, name, val)?;
        }
        
        // Register standard library functions
        // These are stored in static memory and parsed on-demand
        for &stdlib in StdLib::ALL {
            let name = lisp.symbol(stdlib.name())?;
            let val = lisp.stdlib(stdlib)?;
            eval.global_env = eval.env_extend(eval.global_env, name, val)?;
        }
        
        // Note: In Scheme, only #t and #f are the booleans. 
        // 'true' and 'false' are NOT predefined aliases.
        
        Ok(eval)
    }
    
    /// Get the Lisp context
    pub fn lisp(&self) -> &Lisp<N> {
        self.lisp
    }
    
    /// Get the global environment
    pub fn global_env(&self) -> ArenaIndex {
        self.global_env
    }
    
    /// Register a native Rust function that can be called from Lisp.
    ///
    /// The function will be bound to the given name in the global environment.
    ///
    /// # Example
    ///
    /// ```rust
    /// use lisp_eval::{Lisp, Evaluator, ArenaIndex, ArenaResult, FromLisp, ToLisp};
    ///
    /// fn my_double<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    ///     let n = isize::from_lisp(lisp, lisp.car(args)?)?;
    ///     (n * 2).to_lisp(lisp)
    /// }
    ///
    /// let lisp: Lisp<10000> = Lisp::new();
    /// let mut eval = Evaluator::new(&lisp).unwrap();
    /// eval.register_native("my-double", my_double).unwrap();
    ///
    /// // Now you can call (my-double 5) from Lisp to get 10
    /// let result = eval.eval_str("(my-double 5)").unwrap();
    /// assert_eq!(lisp.get(result).unwrap().as_number(), Some(10));
    /// ```
    pub fn register_native(&mut self, name: &'static str, func: NativeFn<N>) -> Result<(), EvalError> {
        // Get the ID before registering (it's the current count)
        let id = self.native_registry.len();
        
        // Compute a simple hash of the name for verification
        let name_hash = simple_hash(name);
        
        // Register in the native registry
        self.native_registry.register(name, func);
        
        // Create a symbol and a Native value, then bind in global env
        let name_sym = self.lisp.symbol(name)?;
        let native_val = self.lisp.native(id, name_hash)?;
        self.global_env = self.env_extend(self.global_env, name_sym, native_val)?;
        
        Ok(())
    }
    
    /// Get a reference to the native function registry.
    pub fn native_registry(&self) -> &NativeRegistry<N> {
        &self.native_registry
    }
    
    /// Run GC with current roots (global env only)
    pub fn gc(&self) -> GcStats {
        self.lisp.gc(&[self.global_env])
    }
    
    /// Run GC during evaluation - marks continuation stack AND current state as roots
    fn gc_with_state(&self, state: &TrampolineState) -> GcStats {
        // Collect all roots: global env + current state + all ArenaIndex values in continuations
        const MAX_ROOTS: usize = 512;
        let mut roots = [ArenaIndex::NULL; MAX_ROOTS];
        let mut root_count = 0;
        
        // Always include global env
        roots[root_count] = self.global_env;
        root_count += 1;
        
        // Include current state
        match state {
            TrampolineState::Eval { expr, env } => {
                roots[root_count] = *expr; root_count += 1;
                roots[root_count] = *env; root_count += 1;
            }
            TrampolineState::Return { val } => {
                roots[root_count] = *val; root_count += 1;
            }
        }
        
        // Collect roots from all continuations
        for i in 0..self.cont_depth {
            if root_count >= MAX_ROOTS - 20 {
                break; // Leave some room
            }
            
            match self.cont_stack[i] {
                Cont::Done => {}
                Cont::ApplyForced { args_expr, env, call_expr } => {
                    roots[root_count] = args_expr; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                    roots[root_count] = call_expr; root_count += 1;
                }
                Cont::IfBranch { then_expr, else_expr, env } => {
                    roots[root_count] = then_expr; root_count += 1;
                    roots[root_count] = else_expr; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::BuiltinForceArg { remaining_args, collected, call_expr, eval_env, .. } => {
                    roots[root_count] = remaining_args; root_count += 1;
                    roots[root_count] = collected; root_count += 1;
                    roots[root_count] = call_expr; root_count += 1;
                    roots[root_count] = eval_env; root_count += 1;
                }
                Cont::BinaryBuiltinFirst { second_arg, call_expr, eval_env, .. } => {
                    roots[root_count] = second_arg; root_count += 1;
                    roots[root_count] = call_expr; root_count += 1;
                    roots[root_count] = eval_env; root_count += 1;
                }
                Cont::BinaryBuiltinSecond { first_val, call_expr, .. } => {
                    roots[root_count] = first_val; root_count += 1;
                    roots[root_count] = call_expr; root_count += 1;
                }
                Cont::LambdaFirstBind { param } => {
                    roots[root_count] = param; root_count += 1;
                }
                Cont::LambdaBindArg { remaining_exprs, eval_env, remaining_params, body, new_env, call_expr } => {
                    roots[root_count] = remaining_exprs; root_count += 1;
                    roots[root_count] = eval_env; root_count += 1;
                    roots[root_count] = remaining_params; root_count += 1;
                    roots[root_count] = body; root_count += 1;
                    roots[root_count] = new_env; root_count += 1;
                    roots[root_count] = call_expr; root_count += 1;
                }
            }
        }
        
        self.lisp.gc(&roots[..root_count])
    }
    
    // ========================================================================
    // Stack Management
    // ========================================================================
    
    fn push_frame(&mut self, expr: ArenaIndex, func: ArenaIndex) -> Result<(), EvalError> {
        if self.call_stack_depth >= MAX_STACK_DEPTH {
            return Err(self.make_error(ErrorKind::StackOverflow, expr));
        }
        self.call_stack[self.call_stack_depth] = StackFrame { expr, func };
        self.call_stack_depth += 1;
        Ok(())
    }
    
    fn pop_frame(&mut self) {
        if self.call_stack_depth > 0 {
            self.call_stack_depth -= 1;
        }
    }
    
    fn make_error(&self, kind: ErrorKind, expr: ArenaIndex) -> EvalError {
        EvalError::new(kind)
            .with_expr(expr)
            .with_backtrace(&self.call_stack, self.call_stack_depth)
    }
    
    fn type_error(&self, expr: ArenaIndex, expected: &'static str, got: &'static str) -> EvalError {
        self.make_error(ErrorKind::TypeError, expr)
            .with_types(expected, got)
    }
    
    fn arg_error(&self, expr: ArenaIndex, expected: usize, got: usize) -> EvalError {
        self.make_error(ErrorKind::WrongArgCount, expr)
            .with_args(expected, got)
    }
    
    // ========================================================================
    // Environment Management
    // ========================================================================
    
    /// Extend an environment with a binding
    #[inline]
    fn env_extend(&self, env: ArenaIndex, name: ArenaIndex, value: ArenaIndex) -> EvalResult {
        let binding = self.lisp.cons(name, value)?;
        self.lisp.cons(binding, env).map_err(Into::into)
    }
    
    /// Look up a variable in an environment
    fn env_lookup(&self, env: ArenaIndex, name: ArenaIndex) -> EvalResult {
        let mut current = env;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    // Try global
                    return self.env_lookup_global(name);
                }
                Value::Cons { car, cdr } => {
                    let binding = self.lisp.get(car)?;
                    if let Value::Cons { car: bound_name, cdr: bound_value } = binding {
                        if self.lisp.symbol_eq(bound_name, name)? {
                            return Ok(bound_value);
                        }
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::Generic, name)),
            }
        }
    }
    
    /// Look up in global environment only
    fn env_lookup_global(&self, name: ArenaIndex) -> EvalResult {
        let mut current = self.global_env;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    return Err(self.make_error(ErrorKind::UnboundVariable, name));
                }
                Value::Cons { car, cdr } => {
                    let binding = self.lisp.get(car)?;
                    if let Value::Cons { car: bound_name, cdr: bound_value } = binding {
                        if self.lisp.symbol_eq(bound_name, name)? {
                            return Ok(bound_value);
                        }
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::Generic, name)),
            }
        }
    }
    
    /// Set a variable in an environment (mutation operation)
    /// Searches both local and global environments
    /// Returns the new value on success
    fn env_set(&self, env: ArenaIndex, name: ArenaIndex, value: ArenaIndex) -> EvalResult {
        // First search local environment
        let mut current = env;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    // Not found in local env, try global
                    return self.env_set_global(name, value);
                }
                Value::Cons { car, cdr } => {
                    let binding = self.lisp.get(car)?;
                    if let Value::Cons { car: bound_name, cdr: _ } = binding {
                        if self.lisp.symbol_eq(bound_name, name)? {
                            // Found it - mutate the binding
                            self.lisp.set(car, Value::Cons { car: bound_name, cdr: value })?;
                            return Ok(value);
                        }
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::Generic, name)),
            }
        }
    }
    
    /// Set a variable in global environment only
    fn env_set_global(&self, name: ArenaIndex, value: ArenaIndex) -> EvalResult {
        let mut current = self.global_env;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    // Not found anywhere - error
                    return Err(self.make_error(ErrorKind::UnboundVariable, name));
                }
                Value::Cons { car, cdr } => {
                    let binding = self.lisp.get(car)?;
                    if let Value::Cons { car: bound_name, cdr: _ } = binding {
                        if self.lisp.symbol_eq(bound_name, name)? {
                            // Found it - mutate the binding
                            self.lisp.set(car, Value::Cons { car: bound_name, cdr: value })?;
                            return Ok(value);
                        }
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::Generic, name)),
            }
        }
    }
    
    /// Define in global environment (NOTE: only allowed at top-level)
    pub fn define(&mut self, name: ArenaIndex, value: ArenaIndex) -> EvalResult {
        // Check if already defined and update
        let mut current = self.global_env;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    // Not found, add new binding
                    self.global_env = self.env_extend(self.global_env, name, value)?;
                    return Ok(value);
                }
                Value::Cons { car, cdr } => {
                    let binding = self.lisp.get(car)?;
                    if let Value::Cons { car: bound_name, cdr: _ } = binding {
                        if self.lisp.symbol_eq(bound_name, name)? {
                            // Update existing
                            self.lisp.set(car, Value::Cons { car: bound_name, cdr: value })?;
                            return Ok(value);
                        }
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::Generic, name)),
            }
        }
    }
    
    // ========================================================================
    // Main Evaluation - Full Trampoline (No Rust Recursion)
    // ========================================================================
    
    /// Push a continuation onto the stack
    #[inline]
    fn push_cont(&mut self, cont: Cont) -> Result<(), EvalError> {
        if self.cont_depth >= MAX_CONT_DEPTH {
            return Err(EvalError::new(ErrorKind::StackOverflow));
        }
        self.cont_stack[self.cont_depth] = cont;
        self.cont_depth += 1;
        Ok(())
    }
    
    /// Pop a continuation from the stack
    #[inline]
    fn pop_cont(&mut self) -> Cont {
        if self.cont_depth == 0 {
            Cont::Done
        } else {
            self.cont_depth -= 1;
            self.cont_stack[self.cont_depth]
        }
    }
    
    
    /// Evaluate an expression (entry point)
    pub fn eval(&mut self, expr: ArenaIndex) -> EvalResult {
        // Reset continuation stack
        self.cont_depth = 0;
        // Start evaluation
        self.trampoline(TrampolineState::Eval { expr, env: self.global_env })
    }
    
    /// Evaluate an expression in a given environment
    /// Uses full trampolining - no Rust recursion
    /// 
    /// This is public so the REPL can evaluate expressions for display
    pub fn eval_in_env(&mut self, expr: ArenaIndex, env: ArenaIndex) -> EvalResult {
        // Reset continuation stack and run
        self.cont_depth = 0;
        self.trampoline(TrampolineState::Eval { expr, env })
    }
    
    /// Evaluate an expression while preserving the current continuation stack.
    ///
    /// This is used for synchronous evaluation during native function calls,
    /// where we need to evaluate arguments without disturbing the main
    /// continuation stack that will process the result.
    ///
    /// # Panics
    ///
    /// Panics if the continuation stack depth exceeds MAX_SAVE (64).
    /// This limit is sufficient for typical native function call chains.
    fn eval_preserving_stack(&mut self, expr: ArenaIndex, env: ArenaIndex) -> EvalResult {
        // Save current continuation stack state
        let saved_depth = self.cont_depth;
        
        // We need to save the actual continuations because the nested evaluation
        // will overwrite them. We save them to a temporary buffer.
        const MAX_SAVE: usize = 64;
        assert!(
            saved_depth <= MAX_SAVE,
            "Continuation stack too deep for native function call (depth={}, max={})",
            saved_depth, MAX_SAVE
        );
        
        let mut saved_conts: [Cont; MAX_SAVE] = [Cont::Done; MAX_SAVE];
        for i in 0..saved_depth {
            saved_conts[i] = self.cont_stack[i];
        }
        
        // Reset for the nested evaluation
        self.cont_depth = 0;
        
        let result = self.trampoline(TrampolineState::Eval { expr, env });
        
        // Restore the continuation stack
        for i in 0..saved_depth {
            self.cont_stack[i] = saved_conts[i];
        }
        self.cont_depth = saved_depth;
        
        result
    }
    
    /// The main trampoline loop - processes states and continuations
    /// This is the ONLY place where looping happens - no Rust recursion!
    fn trampoline(&mut self, mut state: TrampolineState) -> EvalResult {
        // Counter for periodic GC checks (every 2000 steps)
        let mut step_count: usize = 0;
        const GC_CHECK_INTERVAL: usize = 2000;
        const GC_THRESHOLD_PERCENT: usize = 85;
        
        loop {
            // Aggressive GC: Check memory usage periodically
            step_count = step_count.wrapping_add(1);
            if step_count % GC_CHECK_INTERVAL == 0 {
                let stats = self.lisp.stats();
                let usage_percent = (stats.allocated * 100) / stats.capacity;
                if usage_percent >= GC_THRESHOLD_PERCENT {
                    // Memory is getting full - run GC (marking continuations AND current state as roots)
                    self.gc_with_state(&state);
                }
            }
            
            state = match state {
                TrampolineState::Eval { expr, env } => {
                    match self.step_eval(expr, env) {
                        Ok(s) => s,
                        Err(e) if e.kind == ErrorKind::OutOfMemory => {
                            // Auto-GC: Run GC and retry on out of memory
                            self.gc_with_state(&state);
                            self.step_eval(expr, env)?
                        }
                        Err(e) => return Err(e),
                    }
                }
                TrampolineState::Return { val } => {
                    match self.step_return(val) {
                        Ok(Some(new_state)) => new_state,
                        Ok(None) => return Ok(val), // Done!
                        Err(e) if e.kind == ErrorKind::OutOfMemory => {
                            // Auto-GC: Run GC and retry on out of memory
                            self.gc_with_state(&state);
                            match self.step_return(val)? {
                                Some(new_state) => new_state,
                                None => return Ok(val),
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }
            };
        }
    }
    
    /// One step of evaluation
    fn step_eval(&mut self, expr: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let val = self.lisp.get(expr)?;
        
        match val {
            // Self-evaluating values
            Value::Nil | Value::True | Value::False | 
            Value::Number(_) | Value::Char(_) | 
            Value::Builtin(_) | Value::StdLib { .. } | Value::Lambda { .. } |
            Value::Array { .. } | Value::String { .. } | Value::Native { .. } => {
                Ok(TrampolineState::Return { val: expr })
            }
            
            // Symbol - variable lookup
            Value::Symbol { .. } => {
                let val = self.env_lookup(env, expr)?;
                Ok(TrampolineState::Return { val })
            }
            
            // List - special form or function application
            Value::Cons { car, cdr } => {
                self.step_eval_list(car, cdr, expr, env)
            }
        }
    }
    
    /// Evaluate a list (special form or application)
    fn step_eval_list(&mut self, car: ArenaIndex, cdr: ArenaIndex, expr: ArenaIndex, env: ArenaIndex) 
        -> Result<TrampolineState, EvalError> 
    {
        let head = self.lisp.get(car)?;
        
        // Check for special forms
        if let Value::Symbol { .. } = head {
            // quote
            if self.lisp.symbol_matches(car, "quote")? {
                let val = self.lisp.car(cdr)?;
                return Ok(TrampolineState::Return { val });
            }
            
            // if - condition evaluated, then one branch selected
            if self.lisp.symbol_matches(car, "if")? {
                let cond_expr = self.lisp.car(cdr)?;
                let rest = self.lisp.cdr(cdr)?;
                let then_expr = self.lisp.car(rest)?;
                let else_rest = self.lisp.cdr(rest)?;
                let else_expr = if self.lisp.get(else_rest)?.is_nil() {
                    self.lisp.nil()?
                } else {
                    self.lisp.car(else_rest)?
                };
                
                // Push continuation for after condition is evaluated
                self.push_cont(Cont::IfBranch { then_expr, else_expr, env })?;
                
                // Evaluate condition
                return Ok(TrampolineState::Eval { expr: cond_expr, env });
            }
            
            // cond - TCO in final clause
            if self.lisp.symbol_matches(car, "cond")? {
                return self.step_eval_cond(cdr, env);
            }
            
            // lambda
            if self.lisp.symbol_matches(car, "lambda")? {
                let val = self.eval_lambda(cdr, env)?;
                return Ok(TrampolineState::Return { val });
            }
            
            // define
            if self.lisp.symbol_matches(car, "define")? {
                let val = self.eval_define(cdr, env)?;
                return Ok(TrampolineState::Return { val });
            }
            
            // set! - mutate variable binding
            if self.lisp.symbol_matches(car, "set!")? {
                let val = self.eval_set(cdr, env)?;
                return Ok(TrampolineState::Return { val });
            }
            
            // let - TCO in body
            if self.lisp.symbol_matches(car, "let")? {
                let (new_expr, new_env) = self.eval_let_tco(cdr, env)?;
                return Ok(TrampolineState::Eval { expr: new_expr, env: new_env });
            }
            
            // let* - TCO in body
            if self.lisp.symbol_matches(car, "let*")? {
                let (new_expr, new_env) = self.eval_let_star_tco(cdr, env)?;
                return Ok(TrampolineState::Eval { expr: new_expr, env: new_env });
            }
            
            // begin - TCO in last expression
            if self.lisp.symbol_matches(car, "begin")? {
                match self.eval_begin_tco(cdr, env)? {
                    TcoResult::Return(val) => return Ok(TrampolineState::Return { val }),
                    TcoResult::TailCall { new_expr, new_env } => {
                        return Ok(TrampolineState::Eval { expr: new_expr, env: new_env });
                    }
                }
            }
            
            // and - short circuit
            if self.lisp.symbol_matches(car, "and")? {
                match self.eval_and_tco(cdr, env)? {
                    TcoResult::Return(val) => return Ok(TrampolineState::Return { val }),
                    TcoResult::TailCall { new_expr, new_env } => {
                        return Ok(TrampolineState::Eval { expr: new_expr, env: new_env });
                    }
                }
            }
            
            // or - short circuit
            if self.lisp.symbol_matches(car, "or")? {
                match self.eval_or_tco(cdr, env)? {
                    TcoResult::Return(val) => return Ok(TrampolineState::Return { val }),
                    TcoResult::TailCall { new_expr, new_env } => {
                        return Ok(TrampolineState::Eval { expr: new_expr, env: new_env });
                    }
                }
            }
            
            // case - pattern matching
            if self.lisp.symbol_matches(car, "case")? {
                return self.step_eval_case(cdr, env);
            }
            
            // do - iteration construct
            if self.lisp.symbol_matches(car, "do")? {
                return self.step_eval_do(cdr, env);
            }
            
            // quasiquote - template with unquote
            if self.lisp.symbol_matches(car, "quasiquote")? {
                let val = self.eval_quasiquote(self.lisp.car(cdr)?, env)?;
                return Ok(TrampolineState::Return { val });
            }
            
            // eval - evaluate expression at runtime
            if self.lisp.symbol_matches(car, "eval")? {
                let expr_to_eval = self.lisp.car(cdr)?;
                let evaluated_expr = self.eval_in_env(expr_to_eval, env)?;
                // Evaluate the result in the global environment
                return Ok(TrampolineState::Eval { expr: evaluated_expr, env: self.global_env });
            }
            
            // apply - apply function to list of arguments
            if self.lisp.symbol_matches(car, "apply")? {
                return self.step_eval_apply(cdr, env);
            }
            
            // values - return multiple values (as a special list)
            if self.lisp.symbol_matches(car, "values")? {
                let vals = self.eval_values(cdr, env)?;
                return Ok(TrampolineState::Return { val: vals });
            }
        }
        
        // Function application - HYBRID EVALUATION
        self.push_frame(expr, car)?;
        
        // Push continuation: after evaluating func, apply it
        self.push_cont(Cont::ApplyForced { args_expr: cdr, env, call_expr: expr })?;
        
        // Evaluate the function expression
        Ok(TrampolineState::Eval { expr: car, env })
    }
    
    /// Evaluate cond (trampolined)
    fn step_eval_cond(&mut self, clauses: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        match self.eval_cond_tco(clauses, env)? {
            TcoResult::Return(val) => Ok(TrampolineState::Return { val }),
            TcoResult::TailCall { new_expr, new_env } => {
                Ok(TrampolineState::Eval { expr: new_expr, env: new_env })
            }
        }
    }
    
    /// Process a return value with the current continuation
    fn step_return(&mut self, val: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
        let cont = self.pop_cont();
        
        match cont {
            Cont::Done => {
                // No more continuations - we're done
                Ok(None)
            }
            
            Cont::IfBranch { then_expr, else_expr, env } => {
                // val is the evaluated condition
                let branch = if !self.is_false(val)? { then_expr } else { else_expr };
                if branch.is_null() {
                    let nil = self.lisp.nil()?;
                    Ok(Some(TrampolineState::Return { val: nil }))
                } else {
                    Ok(Some(TrampolineState::Eval { expr: branch, env }))
                }
            }
            
            Cont::ApplyForced { args_expr, env, call_expr } => {
                // val is the evaluated function
                match self.lisp.get(val)? {
                    Value::Builtin(b) => {
                        // Builtins: STRICT - evaluate args and apply
                        self.pop_frame();
                        
                        if self.lisp.get(args_expr)?.is_nil() {
                            // No args - apply directly
                            let nil = self.lisp.nil()?;
                            let result = self.apply_builtin_trampolined(b, nil, call_expr)?;
                            Ok(Some(result))
                        } else {
                            // Evaluate args before applying builtin
                            self.apply_builtin_with_args(b, args_expr, env, call_expr)
                        }
                    }
                    Value::Lambda { .. } => {
                        // Lambda: STRICT - evaluate args and bind directly to params
                        self.pop_frame();
                        
                        // Extract lambda parts: (params, body, env)
                        let (params, body, closure_env) = self.lisp.lambda_parts(val)?;
                        
                        if self.lisp.get(args_expr)?.is_nil() {
                            // No args - check params are also empty
                            if !self.lisp.get(params)?.is_nil() {
                                let expected = self.count_list(params)?;
                                return Err(self.arg_error(call_expr, expected, 0));
                            }
                            Ok(Some(TrampolineState::Eval { expr: body, env: closure_env }))
                        } else {
                            // Start evaluating first arg and binding
                            let first_expr = self.lisp.car(args_expr)?;
                            let rest_exprs = self.lisp.cdr(args_expr)?;
                            
                            // Check we have params to bind
                            if self.lisp.get(params)?.is_nil() {
                                let got = self.count_list(args_expr)?;
                                return Err(self.arg_error(call_expr, 0, got));
                            }
                            
                            let first_param = self.lisp.car(params)?;
                            let rest_params = self.lisp.cdr(params)?;
                            
                            // Start with closure_env, we'll extend as we bind
                            self.push_cont(Cont::LambdaBindArg {
                                remaining_exprs: rest_exprs, eval_env: env,
                                remaining_params: rest_params, body, 
                                new_env: closure_env, call_expr
                            })?;
                            // Push binding continuation for first param
                            self.push_cont(Cont::LambdaFirstBind { param: first_param })?;
                            
                            Ok(Some(TrampolineState::Eval { expr: first_expr, env }))
                        }
                    }
                    Value::StdLib { func: s, cache } => {
                        // StdLib: Use cached body/params if available, otherwise parse and cache
                        self.pop_frame();
                        
                        // Check if we have cached values, otherwise parse and cache
                        let (body, params) = if !cache.is_null() {
                            // Use cached values (fast path) - cache is (body . params)
                            let body = self.lisp.car(cache)?;
                            let params = self.lisp.cdr(cache)?;
                            (body, params)
                        } else {
                            // First call - parse body and create param list, then cache
                            let parsed_body = parse(self.lisp, s.body())
                                .map_err(|e| self.parse_error_to_eval(e, call_expr, s.name()))?;
                            let parsed_params = self.make_stdlib_param_list(s.params())?;
                            
                            // Update the StdLib value in the arena with cached values
                            self.lisp.set_stdlib_cache(val, parsed_body, parsed_params)?;
                            
                            (parsed_body, parsed_params)
                        };
                        
                        // Use the global env for stdlib functions (they're defined at top level)
                        let closure_env = self.global_env;
                        
                        if self.lisp.get(args_expr)?.is_nil() {
                            // No args - check params are also empty
                            if !self.lisp.get(params)?.is_nil() {
                                let expected = self.count_list(params)?;
                                return Err(self.arg_error(call_expr, expected, 0));
                            }
                            Ok(Some(TrampolineState::Eval { expr: body, env: closure_env }))
                        } else {
                            // Start evaluating first arg and binding
                            let first_expr = self.lisp.car(args_expr)?;
                            let rest_exprs = self.lisp.cdr(args_expr)?;
                            
                            // Check we have params to bind
                            if self.lisp.get(params)?.is_nil() {
                                let got = self.count_list(args_expr)?;
                                return Err(self.arg_error(call_expr, 0, got));
                            }
                            
                            let first_param = self.lisp.car(params)?;
                            let rest_params = self.lisp.cdr(params)?;
                            
                            // Start with closure_env, we'll extend as we bind
                            self.push_cont(Cont::LambdaBindArg {
                                remaining_exprs: rest_exprs, eval_env: env,
                                remaining_params: rest_params, body, 
                                new_env: closure_env, call_expr
                            })?;
                            // Push binding continuation for first param
                            self.push_cont(Cont::LambdaFirstBind { param: first_param })?;
                            
                            Ok(Some(TrampolineState::Eval { expr: first_expr, env }))
                        }
                    }
                    Value::Native { id, .. } => {
                        // Native (Rust) function: STRICT - evaluate args and pass to Rust fn
                        self.pop_frame();
                        
                        // Evaluate all arguments first
                        let args = self.eval_args_list(args_expr, env)?;
                        
                        // Look up the native function and call it
                        if let Some(native_fn) = self.native_registry.lookup_by_id(id) {
                            let result = native_fn(self.lisp, args)?;
                            Ok(Some(TrampolineState::Return { val: result }))
                        } else {
                            Err(self.make_error(ErrorKind::NotAFunction, call_expr)
                                .with_message("native function not found"))
                        }
                    }
                    _ => {
                        self.pop_frame();
                        Err(self.type_error(call_expr, "procedure", self.lisp.get(val)?.type_name()))
                    }
                }
            }
            
            Cont::LambdaFirstBind { param } => {
                // val is evaluated first arg - bind to param
                // Pop LambdaBindArg, extend env, push it back
                let cont = self.pop_cont();
                if let Cont::LambdaBindArg { remaining_exprs, eval_env, remaining_params, body, new_env, call_expr } = cont {
                    // Extend environment with binding
                    let extended_env = self.env_extend(new_env, param, val)?;
                    
                    if self.lisp.get(remaining_exprs)?.is_nil() {
                        // No more args - check params match
                        if !self.lisp.get(remaining_params)?.is_nil() {
                            let expected = self.count_list(remaining_params)? + 1;
                            return Err(self.arg_error(call_expr, expected, 1));
                        }
                        // Evaluate body with extended env
                        Ok(Some(TrampolineState::Eval { expr: body, env: extended_env }))
                    } else {
                        // More args - get next param
                        if self.lisp.get(remaining_params)?.is_nil() {
                            let got = self.count_list(remaining_exprs)? + 1;
                            return Err(self.arg_error(call_expr, 1, got));
                        }
                        
                        let next_param = self.lisp.car(remaining_params)?;
                        let rest_params = self.lisp.cdr(remaining_params)?;
                        let next_expr = self.lisp.car(remaining_exprs)?;
                        let rest_exprs = self.lisp.cdr(remaining_exprs)?;
                        
                        // Continue with remaining args
                        self.push_cont(Cont::LambdaBindArg {
                            remaining_exprs: rest_exprs, eval_env,
                            remaining_params: rest_params, body,
                            new_env: extended_env, call_expr
                        })?;
                        self.push_cont(Cont::LambdaFirstBind { param: next_param })?;
                        
                        Ok(Some(TrampolineState::Eval { expr: next_expr, env: eval_env }))
                    }
                } else {
                    // This shouldn't happen
                    Err(self.make_error(ErrorKind::Generic, val))
                }
            }
            
            Cont::LambdaBindArg { .. } => {
                // This shouldn't be hit directly - LambdaFirstBind pops it
                Err(self.make_error(ErrorKind::Generic, val))
            }
            
            Cont::BuiltinForceArg { builtin, remaining_args, collected, call_expr, eval_env } => {
                // val is an evaluated argument for a builtin
                let new_collected = self.lisp.cons(val, collected)?;
                
                if self.lisp.get(remaining_args)?.is_nil() {
                    // All args evaluated - apply builtin
                    let args = self.reverse_list(new_collected)?;
                    let result = self.apply_builtin(builtin, args, call_expr)?;
                    Ok(Some(TrampolineState::Return { val: result }))
                } else {
                    // More args to evaluate
                    let next_arg = self.lisp.car(remaining_args)?;
                    let rest_args = self.lisp.cdr(remaining_args)?;
                    
                    self.push_cont(Cont::BuiltinForceArg {
                        builtin, remaining_args: rest_args, collected: new_collected, call_expr, eval_env
                    })?;
                    
                    Ok(Some(TrampolineState::Eval { expr: next_arg, env: eval_env }))
                }
            }
            
            Cont::BinaryBuiltinFirst { builtin, second_arg, call_expr, eval_env } => {
                // val is first evaluated arg - now evaluate second
                self.push_cont(Cont::BinaryBuiltinSecond { builtin, first_val: val, call_expr })?;
                Ok(Some(TrampolineState::Eval { expr: second_arg, env: eval_env }))
            }
            
            Cont::BinaryBuiltinSecond { builtin, first_val, call_expr } => {
                // val is second evaluated arg - apply binary operation directly
                let result = self.apply_binary_builtin(builtin, first_val, val, call_expr)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }
        }
    }
    
    /// Apply a builtin with unevaluated args - sets up evaluation continuations
    fn apply_builtin_with_args(&mut self, builtin: Builtin, args_expr: ArenaIndex, env: ArenaIndex, call_expr: ArenaIndex) 
        -> Result<Option<TrampolineState>, EvalError> 
    {
        // Get first arg expression
        let first_arg = self.lisp.car(args_expr)?;
        let rest_args = self.lisp.cdr(args_expr)?;
        
        // Check for binary builtins optimization
        if is_binary_builtin(builtin) {
            // Check if exactly 2 args
            if !self.lisp.get(rest_args)?.is_nil() {
                let second_arg_expr = self.lisp.car(rest_args)?;
                let third_check = self.lisp.cdr(rest_args)?;
                if self.lisp.get(third_check)?.is_nil() {
                    // Exactly 2 args - use optimized binary path
                    // Evaluate second arg expr (store for later), then evaluate first
                    self.push_cont(Cont::BinaryBuiltinFirst { 
                        builtin, 
                        second_arg: second_arg_expr, 
                        call_expr,
                        eval_env: env,
                    })?;
                    return Ok(Some(TrampolineState::Eval { expr: first_arg, env }));
                }
            }
        }
        
        // General case: collect args and apply
        let nil = self.lisp.nil()?;
        self.push_cont(Cont::BuiltinForceArg {
            builtin,
            remaining_args: rest_args,
            collected: nil,
            call_expr,
            eval_env: env,
        })?;
        
        Ok(Some(TrampolineState::Eval { expr: first_arg, env }))
    }
    
    /// Reverse a list (used for BuiltinForceArg fallback path)
    fn reverse_list(&self, mut list: ArenaIndex) -> EvalResult {
        let mut result = self.lisp.nil()?;
        loop {
            match self.lisp.get(list)? {
                Value::Nil => return Ok(result),
                Value::Cons { car, cdr } => {
                    result = self.lisp.cons(car, result)?;
                    list = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, list)),
            }
        }
    }
    
    /// Apply a builtin function (trampolined version)
    /// Arguments are already evaluated in strict mode
    fn apply_builtin_trampolined(&mut self, builtin: Builtin, args: ArenaIndex, call_expr: ArenaIndex) 
        -> Result<TrampolineState, EvalError> 
    {
        // In strict evaluation, args are already evaluated values
        let result = self.apply_builtin(builtin, args, call_expr)?;
        Ok(TrampolineState::Return { val: result })
    }
    
    /// Apply a builtin with already-evaluated arguments
    fn apply_builtin(&mut self, builtin: Builtin, args: ArenaIndex, call_expr: ArenaIndex) 
        -> EvalResult 
    {
        match builtin {
            Builtin::Car => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Cons { car, .. } => Ok(car),
                    // Scheme R7RS: car of empty list is an error
                    Value::Nil => Err(self.type_error(call_expr, "pair", "null")),
                    _ => Err(self.type_error(call_expr, "pair", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::Cdr => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Cons { cdr, .. } => Ok(cdr),
                    // Scheme R7RS: cdr of empty list is an error
                    Value::Nil => Err(self.type_error(call_expr, "pair", "null")),
                    _ => Err(self.type_error(call_expr, "pair", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::Cons => {
                extract_args!(self, args, a, b);
                self.lisp.cons(a, b).map_err(Into::into)
            }
            
            Builtin::List => Ok(args),
            
            // Scheme-compliant equality predicates
            Builtin::EqP => {
                // eq? - tests whether two objects are the same object
                extract_args!(self, args, a, b);
                
                let val_a = self.lisp.get(a)?;
                let val_b = self.lisp.get(b)?;
                
                let eq = match (val_a, val_b) {
                    (Value::Nil, Value::Nil) => true,
                    (Value::True, Value::True) => true,
                    (Value::False, Value::False) => true,
                    (Value::Number(x), Value::Number(y)) => x == y,
                    (Value::Char(x), Value::Char(y)) => x == y,
                    (Value::Symbol { .. }, Value::Symbol { .. }) => self.lisp.symbol_eq(a, b)?,
                    _ => a == b,
                };
                
                self.lisp.boolean(eq).map_err(Into::into)
            }
            
            Builtin::EqvP => {
                // eqv? - tests value equivalence (same as eq? for most types in our impl)
                extract_args!(self, args, a, b);
                
                let val_a = self.lisp.get(a)?;
                let val_b = self.lisp.get(b)?;
                
                let eqv = match (val_a, val_b) {
                    (Value::Nil, Value::Nil) => true,
                    (Value::True, Value::True) => true,
                    (Value::False, Value::False) => true,
                    (Value::Number(x), Value::Number(y)) => x == y,
                    (Value::Char(x), Value::Char(y)) => x == y,
                    (Value::Symbol { .. }, Value::Symbol { .. }) => self.lisp.symbol_eq(a, b)?,
                    _ => a == b,
                };
                
                self.lisp.boolean(eqv).map_err(Into::into)
            }
            
            Builtin::EqualP => {
                // equal? - tests structural equality recursively
                let result = self.equal_recursive(self.lisp.car(args)?, self.lisp.car(self.lisp.cdr(args)?)?)?;
                self.lisp.boolean(result).map_err(Into::into)
            }
            
            Builtin::Null => builtin_unary_pred!(self, args, |v: Value| v.is_nil()),
            
            Builtin::Pairp => builtin_unary_pred!(self, args, |v: Value| v.is_cons()),
            
            Builtin::Numberp => builtin_unary_pred!(self, args, |v: Value| v.is_number()),
            
            Builtin::Booleanp => builtin_unary_pred!(self, args, |v: Value| v.is_boolean()),
            
            Builtin::Procedurep => builtin_unary_pred!(self, args, |v: Value| v.is_procedure()),
            
            Builtin::Symbolp => builtin_unary_pred!(self, args, |v: Value| v.is_symbol()),
            
            Builtin::Not => builtin_unary_pred!(self, args, |v: Value| v.is_false()),
            
            Builtin::Add => self.numeric_fold(args, 0, |a, b| a.checked_add(b), call_expr),
            
            Builtin::Sub => {
                let first = self.get_number(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                if self.lisp.get(rest)?.is_nil() {
                    self.lisp.number(-first).map_err(Into::into)
                } else {
                    self.numeric_fold_start(rest, first, |a, b| a.checked_sub(b), call_expr)
                }
            }
            
            Builtin::Mul => self.numeric_fold(args, 1, |a, b| a.checked_mul(b), call_expr),
            
            Builtin::Div => {
                let first = self.get_number(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold_start(rest, first, |a, b| {
                    if b == 0 { None } else { a.checked_div(b) }
                }, call_expr)
            }
            
            Builtin::Modulo => {
                // Scheme modulo: result has the sign of the divisor
                let a = self.get_number(self.lisp.car(args)?, call_expr)?;
                let b = self.get_number(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                if b == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                // Scheme modulo: ((a % b) + b) % b
                let result = ((a % b) + b) % b;
                self.lisp.number(result).map_err(Into::into)
            }
            
            Builtin::Remainder => {
                // Scheme remainder: result has the sign of the dividend
                let a = self.get_number(self.lisp.car(args)?, call_expr)?;
                let b = self.get_number(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                if b == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                // Rust's % operator already gives remainder with sign of dividend
                self.lisp.number(a % b).map_err(Into::into)
            }
            
            Builtin::Quotient => {
                // Integer quotient (truncated towards zero)
                let a = self.get_number(self.lisp.car(args)?, call_expr)?;
                let b = self.get_number(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                if b == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                // Rust's / operator truncates towards zero for integers
                self.lisp.number(a / b).map_err(Into::into)
            }
            
            Builtin::Abs => {
                // Absolute value
                let n = self.get_number(self.lisp.car(args)?, call_expr)?;
                self.lisp.number(n.abs()).map_err(Into::into)
            }
            
            Builtin::Max => {
                // Maximum of one or more numbers
                let first = self.get_number(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold_start(rest, first, |a, b| Some(if a > b { a } else { b }), call_expr)
            }
            
            Builtin::Min => {
                // Minimum of one or more numbers
                let first = self.get_number(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold_start(rest, first, |a, b| Some(if a < b { a } else { b }), call_expr)
            }
            
            Builtin::Gcd => {
                // Greatest common divisor
                // gcd() with no args returns 0, gcd(n) returns |n|
                if self.lisp.get(args)?.is_nil() {
                    return self.lisp.number(0).map_err(Into::into);
                }
                let first = self.get_number(self.lisp.car(args)?, call_expr)?.abs();
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold_start(rest, first, |a, b| Some(Self::gcd_helper(a, b.abs())), call_expr)
            }
            
            Builtin::Lcm => {
                // Least common multiple
                // lcm() with no args returns 1, lcm(n) returns |n|
                if self.lisp.get(args)?.is_nil() {
                    return self.lisp.number(1).map_err(Into::into);
                }
                let first = self.get_number(self.lisp.car(args)?, call_expr)?.abs();
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold_start(rest, first, |a, b| {
                    let b_abs = b.abs();
                    if a == 0 || b_abs == 0 {
                        Some(0)
                    } else {
                        // lcm(a, b) = |a * b| / gcd(a, b)
                        // Use saturating_mul to prevent overflow
                        let g = Self::gcd_helper(a, b_abs);
                        Some((a / g).saturating_mul(b_abs))
                    }
                }, call_expr)
            }
            
            Builtin::Expt => {
                // Exponentiation: (expt base power)
                let base = self.get_number(self.lisp.car(args)?, call_expr)?;
                let power = self.get_number(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                
                if power < 0 {
                    // Negative exponents would give fractions, which we can't represent
                    // For integer arithmetic, return 0 for base > 1, error for base <= 1
                    if base == 0 {
                        return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                    }
                    // For integers, x^(-n) = 1/(x^n), which is 0 for |x| > 1
                    if base == 1 { return self.lisp.number(1).map_err(Into::into); }
                    if base == -1 { 
                        // (-1)^(-n) = (-1)^|n|, use absolute value for parity check
                        return self.lisp.number(if power.abs() % 2 == 0 { 1 } else { -1 }).map_err(Into::into);
                    }
                    return self.lisp.number(0).map_err(Into::into);
                }
                
                // Use integer exponentiation with overflow checking
                let result = Self::int_pow(base, power as usize);
                self.lisp.number(result).map_err(Into::into)
            }
            
            Builtin::Square => {
                // Square of a number
                let n = self.get_number(self.lisp.car(args)?, call_expr)?;
                self.lisp.number(n.saturating_mul(n)).map_err(Into::into)
            }
            
            // Numeric predicates
            Builtin::Zerop => {
                let n = self.get_number(self.lisp.car(args)?, call_expr)?;
                self.lisp.boolean(n == 0).map_err(Into::into)
            }
            
            Builtin::Positivep => {
                let n = self.get_number(self.lisp.car(args)?, call_expr)?;
                self.lisp.boolean(n > 0).map_err(Into::into)
            }
            
            Builtin::Negativep => {
                let n = self.get_number(self.lisp.car(args)?, call_expr)?;
                self.lisp.boolean(n < 0).map_err(Into::into)
            }
            
            Builtin::Oddp => {
                let n = self.get_number(self.lisp.car(args)?, call_expr)?;
                self.lisp.boolean(n % 2 != 0).map_err(Into::into)
            }
            
            Builtin::Evenp => {
                let n = self.get_number(self.lisp.car(args)?, call_expr)?;
                self.lisp.boolean(n % 2 == 0).map_err(Into::into)
            }
            
            Builtin::Integerp => {
                // In our implementation, all numbers are integers
                let val = self.lisp.car(args)?;
                let is_int = matches!(self.lisp.get(val)?, Value::Number(_));
                self.lisp.boolean(is_int).map_err(Into::into)
            }
            
            Builtin::Exactp => {
                // In our implementation, all numbers are exact integers
                let val = self.lisp.car(args)?;
                let is_num = matches!(self.lisp.get(val)?, Value::Number(_));
                self.lisp.boolean(is_num).map_err(Into::into)
            }
            
            Builtin::Inexactp => {
                // In our implementation, we don't have inexact numbers
                let val = self.lisp.car(args)?;
                // Verify it's a number, then return false
                match self.lisp.get(val)? {
                    Value::Number(_) => self.lisp.boolean(false).map_err(Into::into),
                    _ => Err(self.type_error(call_expr, "number", self.lisp.get(val)?.type_name())),
                }
            }
            
            Builtin::Lt => self.compare_numbers(args, |a, b| a < b, call_expr),
            Builtin::Gt => self.compare_numbers(args, |a, b| a > b, call_expr),
            Builtin::Le => self.compare_numbers(args, |a, b| a <= b, call_expr),
            Builtin::Ge => self.compare_numbers(args, |a, b| a >= b, call_expr),
            Builtin::NumEq => self.compare_numbers(args, |a, b| a == b, call_expr),
            
            Builtin::Display => {
                Ok(self.lisp.car(args)?)
            }
            
            Builtin::Newline => {
                self.lisp.nil().map_err(Into::into)
            }
            
            Builtin::Error => {
                let msg = self.lisp.car(args)?;
                Err(self.make_error(ErrorKind::UserError, msg))
            }
            
            Builtin::SetCar => {
                // (set-car! pair value) - mutate the car of a cons cell
                extract_args!(self, args, pair, value);
                
                // Verify it's a pair
                match self.lisp.get(pair)? {
                    Value::Cons { .. } => {
                        self.lisp.set_car(pair, value).map_err(Into::into)
                    }
                    _ => Err(self.make_error(ErrorKind::NotAPair, call_expr)),
                }
            }
            
            Builtin::SetCdr => {
                // (set-cdr! pair value) - mutate the cdr of a cons cell
                extract_args!(self, args, pair, value);
                
                // Verify it's a pair
                match self.lisp.get(pair)? {
                    Value::Cons { .. } => {
                        self.lisp.set_cdr(pair, value).map_err(Into::into)
                    }
                    _ => Err(self.make_error(ErrorKind::NotAPair, call_expr)),
                }
            }
            
            Builtin::MakeArray => {
                // (make-array len default) - create an array of given length
                extract_args!(self, args, len_val, default);
                
                let len = match self.lisp.get(len_val)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                self.lisp.make_array(len, default).map_err(Into::into)
            }
            
            Builtin::ArrayRef => {
                // (array-ref arr index) - get element at index
                extract_args!(self, args, arr, index_val);
                
                let index = match self.lisp.get(index_val)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                match self.lisp.get(arr)? {
                    Value::Array { .. } => {
                        self.lisp.array_get(arr, index).map_err(Into::into)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::ArraySet => {
                // (array-set! arr index value) - set element at index
                extract_args!(self, args, arr, index_val, value);
                
                let index = match self.lisp.get(index_val)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                match self.lisp.get(arr)? {
                    Value::Array { .. } => {
                        self.lisp.array_set(arr, index, value)?;
                        Ok(arr) // Return the array
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::ArrayLength => {
                // (array-length arr) - get length of array
                let arr = self.lisp.car(args)?;
                
                match self.lisp.get(arr)? {
                    Value::Array { len, .. } => {
                        self.lisp.number(len as isize).map_err(Into::into)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::Arrayp => {
                // (array? x) - check if x is an array
                let val = self.lisp.car(args)?;
                let is_array = matches!(self.lisp.get(val)?, Value::Array { .. });
                self.lisp.boolean(is_array).map_err(Into::into)
            }
            
            Builtin::Gc => {
                // (gc) - Manually trigger garbage collection
                // Returns a list: (marked collected total-before)
                // Built right-to-left since cons prepends
                let stats = self.gc();
                let marked = self.lisp.number(stats.marked as isize)?;
                let collected = self.lisp.number(stats.collected as isize)?;
                let total_before = self.lisp.number(stats.total_before as isize)?;
                let nil = self.lisp.nil()?;
                let list = self.lisp.cons(total_before, nil)?;
                let list = self.lisp.cons(collected, list)?;
                let list = self.lisp.cons(marked, list)?;
                Ok(list)
            }
            
            Builtin::GcEnable => {
                // (gc-enable) - Enable automatic garbage collection
                self.lisp.arena().set_gc_enabled(true);
                self.lisp.true_val().map_err(Into::into)
            }
            
            Builtin::GcDisable => {
                // (gc-disable) - Disable automatic garbage collection
                self.lisp.arena().set_gc_enabled(false);
                self.lisp.false_val().map_err(Into::into)
            }
            
            Builtin::GcEnabledP => {
                // (gc-enabled?) - Check if GC is enabled
                let enabled = self.lisp.arena().is_gc_enabled();
                self.lisp.boolean(enabled).map_err(Into::into)
            }
            
            Builtin::ArenaStats => {
                // (arena-stats) - Get arena statistics
                // Returns a list: (capacity allocated free usage-percent)
                let stats = self.lisp.stats();
                let capacity = self.lisp.number(stats.capacity as isize)?;
                let allocated = self.lisp.number(stats.allocated as isize)?;
                let free = self.lisp.number(stats.free as isize)?;
                let usage = self.lisp.number(stats.usage_percent() as isize)?;
                let nil = self.lisp.nil()?;
                let list = self.lisp.cons(usage, nil)?;
                let list = self.lisp.cons(free, list)?;
                let list = self.lisp.cons(allocated, list)?;
                let list = self.lisp.cons(capacity, list)?;
                Ok(list)
            }
        }
    }
    
    /// OPTIMIZED: Apply binary builtin directly without list allocation
    fn apply_binary_builtin(&mut self, builtin: Builtin, a: ArenaIndex, b: ArenaIndex, call_expr: ArenaIndex) 
        -> EvalResult 
    {
        match builtin {
            Builtin::Add => {
                let x = self.get_number(a, call_expr)?;
                let y = self.get_number(b, call_expr)?;
                x.checked_add(y)
                    .map(|n| self.lisp.number(n))
                    .ok_or_else(|| self.make_error(ErrorKind::DivisionByZero, call_expr))?
                    .map_err(Into::into)
            }
            Builtin::Sub => {
                let x = self.get_number(a, call_expr)?;
                let y = self.get_number(b, call_expr)?;
                x.checked_sub(y)
                    .map(|n| self.lisp.number(n))
                    .ok_or_else(|| self.make_error(ErrorKind::DivisionByZero, call_expr))?
                    .map_err(Into::into)
            }
            Builtin::Mul => {
                let x = self.get_number(a, call_expr)?;
                let y = self.get_number(b, call_expr)?;
                x.checked_mul(y)
                    .map(|n| self.lisp.number(n))
                    .ok_or_else(|| self.make_error(ErrorKind::DivisionByZero, call_expr))?
                    .map_err(Into::into)
            }
            Builtin::Div => {
                let x = self.get_number(a, call_expr)?;
                let y = self.get_number(b, call_expr)?;
                if y == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                self.lisp.number(x / y).map_err(Into::into)
            }
            Builtin::Modulo => {
                // Scheme modulo: result has the sign of the divisor
                let x = self.get_number(a, call_expr)?;
                let y = self.get_number(b, call_expr)?;
                if y == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                let result = ((x % y) + y) % y;
                self.lisp.number(result).map_err(Into::into)
            }
            Builtin::Remainder => {
                // Scheme remainder: result has the sign of the dividend
                let x = self.get_number(a, call_expr)?;
                let y = self.get_number(b, call_expr)?;
                if y == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                self.lisp.number(x % y).map_err(Into::into)
            }
            Builtin::Lt => {
                let x = self.get_number(a, call_expr)?;
                let y = self.get_number(b, call_expr)?;
                self.lisp.boolean(x < y).map_err(Into::into)
            }
            Builtin::Gt => {
                let x = self.get_number(a, call_expr)?;
                let y = self.get_number(b, call_expr)?;
                self.lisp.boolean(x > y).map_err(Into::into)
            }
            Builtin::Le => {
                let x = self.get_number(a, call_expr)?;
                let y = self.get_number(b, call_expr)?;
                self.lisp.boolean(x <= y).map_err(Into::into)
            }
            Builtin::Ge => {
                let x = self.get_number(a, call_expr)?;
                let y = self.get_number(b, call_expr)?;
                self.lisp.boolean(x >= y).map_err(Into::into)
            }
            Builtin::NumEq => {
                let x = self.get_number(a, call_expr)?;
                let y = self.get_number(b, call_expr)?;
                self.lisp.boolean(x == y).map_err(Into::into)
            }
            Builtin::EqP | Builtin::EqvP => {
                let val_a = self.lisp.get(a)?;
                let val_b = self.lisp.get(b)?;
                
                let eq = match (val_a, val_b) {
                    (Value::Nil, Value::Nil) => true,
                    (Value::True, Value::True) => true,
                    (Value::False, Value::False) => true,
                    (Value::Number(x), Value::Number(y)) => x == y,
                    (Value::Char(x), Value::Char(y)) => x == y,
                    (Value::Symbol { .. }, Value::Symbol { .. }) => self.lisp.symbol_eq(a, b)?,
                    _ => a == b,
                };
                
                self.lisp.boolean(eq).map_err(Into::into)
            }
            Builtin::Cons => {
                self.lisp.cons(a, b).map_err(Into::into)
            }
            // For other builtins, fall back to list-based approach
            _ => {
                let rest = self.lisp.cons(b, self.lisp.nil()?)?;
                let args = self.lisp.cons(a, rest)?;
                self.apply_builtin(builtin, args, call_expr)
            }
        }
    }
    
    /// Get number from already-evaluated value
    fn get_number(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<isize, EvalError> {
        match self.lisp.get(idx)? {
            Value::Number(n) => Ok(n),
            v => Err(self.type_error(call_expr, "number", v.type_name())),
        }
    }
    
    /// Numeric fold with already-evaluated args
    fn numeric_fold<F>(&self, args: ArenaIndex, init: isize, f: F, call_expr: ArenaIndex) -> EvalResult
    where F: Fn(isize, isize) -> Option<isize>
    {
        self.numeric_fold_start(args, init, f, call_expr)
    }
    
    fn numeric_fold_start<F>(&self, args: ArenaIndex, mut acc: isize, f: F, call_expr: ArenaIndex) -> EvalResult
    where F: Fn(isize, isize) -> Option<isize>
    {
        let mut current = args;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.lisp.number(acc).map_err(Into::into),
                Value::Cons { car, cdr } => {
                    let n = self.get_number(car, call_expr)?;
                    acc = f(acc, n).ok_or_else(|| self.make_error(ErrorKind::DivisionByZero, call_expr))?;
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Compare two numbers with already-evaluated args
    fn compare_numbers<F>(&self, args: ArenaIndex, cmp: F, call_expr: ArenaIndex) -> EvalResult
    where F: Fn(isize, isize) -> bool
    {
        let a = self.get_number(self.lisp.car(args)?, call_expr)?;
        let b = self.get_number(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
        self.lisp.boolean(cmp(a, b)).map_err(Into::into)
    }
    
    /// Helper for computing GCD using Euclidean algorithm
    fn gcd_helper(mut a: isize, mut b: isize) -> isize {
        while b != 0 {
            let t = b;
            b = a % b;
            a = t;
        }
        a.abs()
    }
    
    /// Helper for integer exponentiation (base^power) with overflow checking
    fn int_pow(base: isize, power: usize) -> isize {
        if power == 0 {
            return 1;
        }
        if base == 0 {
            return 0;
        }
        if base == 1 {
            return 1;
        }
        if base == -1 {
            return if power % 2 == 0 { 1 } else { -1 };
        }
        
        // Use exponentiation by squaring with saturating operations
        let mut result: isize = 1;
        let mut base = base;
        let mut exp = power;
        
        while exp > 0 {
            if exp % 2 == 1 {
                result = result.saturating_mul(base);
            }
            exp /= 2;
            if exp > 0 {
                base = base.saturating_mul(base);
            }
        }
        result
    }
    
    /// Recursive structural equality for equal? predicate
    fn equal_recursive(&self, a: ArenaIndex, b: ArenaIndex) -> Result<bool, EvalError> {
        // Check if they're the same index first
        if a == b {
            return Ok(true);
        }
        
        let val_a = self.lisp.get(a)?;
        let val_b = self.lisp.get(b)?;
        
        match (val_a, val_b) {
            (Value::Nil, Value::Nil) => Ok(true),
            (Value::True, Value::True) => Ok(true),
            (Value::False, Value::False) => Ok(true),
            (Value::Number(x), Value::Number(y)) => Ok(x == y),
            (Value::Char(x), Value::Char(y)) => Ok(x == y),
            (Value::Symbol { .. }, Value::Symbol { .. }) => self.lisp.symbol_eq(a, b).map_err(Into::into),
            (Value::Cons { car: car_a, cdr: cdr_a }, Value::Cons { car: car_b, cdr: cdr_b }) => {
                // Recursively check car and cdr
                if !self.equal_recursive(car_a, car_b)? {
                    return Ok(false);
                }
                self.equal_recursive(cdr_a, cdr_b)
            }
            _ => Ok(false),
        }
    }
    
    // ========================================================================
    // Special Form Helpers
    // ========================================================================
    
    /// Evaluate cond with TCO
    fn eval_cond_tco(&mut self, clauses: ArenaIndex, env: ArenaIndex) -> Result<TcoResult, EvalError> {
        let mut current = clauses;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return Ok(TcoResult::Return(self.lisp.nil()?)),
                Value::Cons { car: clause, cdr: rest } => {
                    let test = self.lisp.car(clause)?;
                    let body = self.lisp.cdr(clause)?;
                    
                    // Check for 'else' clause
                    let is_else = self.lisp.symbol_matches(test, "else").unwrap_or(false);
                    
                    let test_result = if is_else {
                        self.lisp.true_val()?
                    } else {
                        self.eval_preserving_stack(test, env)?
                    };
                    
                    if !self.is_false(test_result)? {
                        // Evaluate body - last expr is tail position
                        return self.eval_begin_tco(body, env);
                    }
                    
                    current = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, clauses)),
            }
        }
    }
    
    /// Evaluate begin with TCO
    fn eval_begin_tco(&mut self, exprs: ArenaIndex, env: ArenaIndex) -> Result<TcoResult, EvalError> {
        let mut current = exprs;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return Ok(TcoResult::Return(self.lisp.nil()?)),
                Value::Cons { car: expr, cdr: rest } => {
                    if self.lisp.get(rest)?.is_nil() {
                        // Last expression - tail position
                        return Ok(TcoResult::TailCall { new_expr: expr, new_env: env });
                    } else {
                        // Not last - evaluate and continue (preserve stack)
                        self.eval_preserving_stack(expr, env)?;
                        current = rest;
                    }
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Evaluate and with TCO
    fn eval_and_tco(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TcoResult, EvalError> {
        let mut current = args;
        
        // Empty and returns #t
        if self.lisp.get(current)?.is_nil() {
            return Ok(TcoResult::Return(self.lisp.true_val()?));
        }
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return Ok(TcoResult::Return(self.lisp.true_val()?)),
                Value::Cons { car: expr, cdr: rest } => {
                    if self.lisp.get(rest)?.is_nil() {
                        // Last expression - tail position
                        return Ok(TcoResult::TailCall { new_expr: expr, new_env: env });
                    } else {
                        let result = self.eval_preserving_stack(expr, env)?;
                        if self.is_false(result)? {
                            return Ok(TcoResult::Return(self.lisp.false_val()?));
                        }
                        current = rest;
                    }
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Evaluate or with TCO
    fn eval_or_tco(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TcoResult, EvalError> {
        let mut current = args;
        
        // Empty or returns #f
        if self.lisp.get(current)?.is_nil() {
            return Ok(TcoResult::Return(self.lisp.false_val()?));
        }
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return Ok(TcoResult::Return(self.lisp.false_val()?)),
                Value::Cons { car: expr, cdr: rest } => {
                    if self.lisp.get(rest)?.is_nil() {
                        // Last expression - tail position
                        return Ok(TcoResult::TailCall { new_expr: expr, new_env: env });
                    } else {
                        let result = self.eval_preserving_stack(expr, env)?;
                        if !self.is_false(result)? {
                            return Ok(TcoResult::Return(result));
                        }
                        current = rest;
                    }
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Evaluate case - pattern matching
    /// (case key ((datum1 ...) expr1 ...) ((datum2 ...) expr2 ...) (else exprn ...))
    fn step_eval_case(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let key_expr = self.lisp.car(args)?;
        let clauses = self.lisp.cdr(args)?;
        
        // Evaluate the key expression
        let key = self.eval_in_env(key_expr, env)?;
        
        // Check each clause
        let mut current = clauses;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    // No match found, return nil
                    let nil = self.lisp.nil()?;
                    return Ok(TrampolineState::Return { val: nil });
                }
                Value::Cons { car: clause, cdr: rest } => {
                    let datums = self.lisp.car(clause)?;
                    let body = self.lisp.cdr(clause)?;
                    
                    // Check for 'else' clause
                    if self.lisp.symbol_matches(datums, "else").unwrap_or(false) {
                        return self.eval_case_body(body, env);
                    }
                    
                    // Check if key matches any datum
                    if self.case_matches(key, datums)? {
                        return self.eval_case_body(body, env);
                    }
                    
                    current = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, clauses)),
            }
        }
    }
    
    /// Check if two values are structurally equal (for case matching)
    fn values_equal(&self, a: ArenaIndex, b: ArenaIndex) -> Result<bool, EvalError> {
        if a == b {
            return Ok(true);
        }
        
        let val_a = self.lisp.get(a)?;
        let val_b = self.lisp.get(b)?;
        
        match (val_a, val_b) {
            (Value::Nil, Value::Nil) => Ok(true),
            (Value::True, Value::True) => Ok(true),
            (Value::False, Value::False) => Ok(true),
            (Value::Number(x), Value::Number(y)) => Ok(x == y),
            (Value::Char(x), Value::Char(y)) => Ok(x == y),
            (Value::Symbol { .. }, Value::Symbol { .. }) => {
                self.lisp.symbol_eq(a, b).map_err(Into::into)
            }
            (Value::Cons { car: car_a, cdr: cdr_a }, Value::Cons { car: car_b, cdr: cdr_b }) => {
                // Recursively compare (limited depth to avoid stack overflow)
                if self.values_equal(car_a, car_b)? {
                    self.values_equal(cdr_a, cdr_b)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }
    
    /// Check if key matches any datum in the list
    fn case_matches(&self, key: ArenaIndex, datums: ArenaIndex) -> Result<bool, EvalError> {
        let mut current = datums;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return Ok(false),
                Value::Cons { car: datum, cdr: rest } => {
                    if self.values_equal(key, datum)? {
                        return Ok(true);
                    }
                    current = rest;
                }
                _ => {
                    // Single datum (not a list)
                    return self.values_equal(key, datums);
                }
            }
        }
    }
    
    /// Evaluate case clause body
    fn eval_case_body(&mut self, body: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        match self.eval_begin_tco(body, env)? {
            TcoResult::Return(val) => Ok(TrampolineState::Return { val }),
            TcoResult::TailCall { new_expr, new_env } => {
                Ok(TrampolineState::Eval { expr: new_expr, env: new_env })
            }
        }
    }
    
    /// Evaluate do - iteration construct
    /// (do ((var init step) ...) (test result ...) body ...)
    fn step_eval_do(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let test_clause = self.lisp.car(rest)?;
        let body = self.lisp.cdr(rest)?;
        
        // Initialize variables
        let mut loop_env = env;
        let mut var_info: [(ArenaIndex, ArenaIndex); 16] = [(ArenaIndex::NULL, ArenaIndex::NULL); 16]; // (var, step)
        let mut var_count = 0;
        
        let mut current = bindings;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { car: binding, cdr: rest } => {
                    let var = self.lisp.car(binding)?;
                    let init_rest = self.lisp.cdr(binding)?;
                    let init = self.lisp.car(init_rest)?;
                    let step_rest = self.lisp.cdr(init_rest)?;
                    let step = if self.lisp.get(step_rest)?.is_nil() {
                        var // No step, use variable itself
                    } else {
                        self.lisp.car(step_rest)?
                    };
                    
                    let init_val = self.eval_in_env(init, env)?;
                    loop_env = self.env_extend(loop_env, var, init_val)?;
                    
                    if var_count >= 16 {
                        return Err(self.make_error(ErrorKind::StackOverflow, bindings)
                            .with_message("do: too many variables (max 16)"));
                    }
                    var_info[var_count] = (var, step);
                    var_count += 1;
                    
                    current = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, bindings)),
            }
        }
        
        // Iteration loop
        loop {
            // Evaluate test
            let test = self.lisp.car(test_clause)?;
            let test_result = self.eval_in_env(test, loop_env)?;
            
            if !self.is_false(test_result)? {
                // Test passed - evaluate result expressions
                let result_exprs = self.lisp.cdr(test_clause)?;
                if self.lisp.get(result_exprs)?.is_nil() {
                    return Ok(TrampolineState::Return { val: test_result });
                } else {
                    return self.eval_case_body(result_exprs, loop_env);
                }
            }
            
            // Evaluate body (for side effects in non-pure case)
            let mut body_cur = body;
            loop {
                match self.lisp.get(body_cur)? {
                    Value::Nil => break,
                    Value::Cons { car: expr, cdr: rest } => {
                        self.eval_in_env(expr, loop_env)?;
                        body_cur = rest;
                    }
                    _ => break,
                }
            }
            
            // Evaluate step expressions and update variables
            let mut new_vals: [ArenaIndex; 16] = [ArenaIndex::NULL; 16];
            for i in 0..var_count {
                new_vals[i] = self.eval_in_env(var_info[i].1, loop_env)?;
            }
            
            // Update environment with new values
            for i in 0..var_count {
                loop_env = self.env_extend(loop_env, var_info[i].0, new_vals[i])?;
            }
        }
    }
    
    /// Evaluate quasiquote - template with unquote
    fn eval_quasiquote(&mut self, template: ArenaIndex, env: ArenaIndex) -> EvalResult {
        self.eval_quasiquote_impl(template, env, 1)
    }
    
    fn eval_quasiquote_impl(&mut self, template: ArenaIndex, env: ArenaIndex, depth: usize) -> EvalResult {
        match self.lisp.get(template)? {
            Value::Cons { car, cdr } => {
                // Check for unquote
                if self.lisp.symbol_matches(car, "unquote").unwrap_or(false) {
                    if depth == 1 {
                        // Evaluate the unquoted expression
                        return self.eval_in_env(self.lisp.car(cdr)?, env);
                    } else {
                        // Nested quasiquote - decrease depth
                        let unquote_sym = self.lisp.symbol("unquote")?;
                        let inner = self.eval_quasiquote_impl(self.lisp.car(cdr)?, env, depth - 1)?;
                        let nil = self.lisp.nil()?;
                        let inner_list = self.lisp.cons(inner, nil)?;
                        return self.lisp.cons(unquote_sym, inner_list).map_err(Into::into);
                    }
                }
                
                // Check for unquote-splicing
                if self.lisp.symbol_matches(car, "unquote-splicing").unwrap_or(false) {
                    if depth == 1 {
                        // Return the evaluated list (caller handles splicing)
                        return self.eval_in_env(self.lisp.car(cdr)?, env);
                    }
                }
                
                // Check for nested quasiquote
                if self.lisp.symbol_matches(car, "quasiquote").unwrap_or(false) {
                    let inner = self.eval_quasiquote_impl(self.lisp.car(cdr)?, env, depth + 1)?;
                    let qq_sym = self.lisp.symbol("quasiquote")?;
                    let nil = self.lisp.nil()?;
                    let inner_list = self.lisp.cons(inner, nil)?;
                    return self.lisp.cons(qq_sym, inner_list).map_err(Into::into);
                }
                
                // Check for unquote-splicing in car position (special handling)
                if let Value::Cons { car: inner_car, cdr: inner_cdr } = self.lisp.get(car)? {
                    if self.lisp.symbol_matches(inner_car, "unquote-splicing").unwrap_or(false) && depth == 1 {
                        // Splice the result into the list
                        let splice_val = self.eval_in_env(self.lisp.car(inner_cdr)?, env)?;
                        let rest = self.eval_quasiquote_impl(cdr, env, depth)?;
                        return self.append_lists(splice_val, rest);
                    }
                }
                
                // Recursively process car and cdr
                let new_car = self.eval_quasiquote_impl(car, env, depth)?;
                let new_cdr = self.eval_quasiquote_impl(cdr, env, depth)?;
                self.lisp.cons(new_car, new_cdr).map_err(Into::into)
            }
            _ => {
                // Atoms are returned as-is
                Ok(template)
            }
        }
    }
    
    /// Append two lists
    fn append_lists(&self, a: ArenaIndex, b: ArenaIndex) -> EvalResult {
        match self.lisp.get(a)? {
            Value::Nil => Ok(b),
            Value::Cons { car, cdr } => {
                let rest = self.append_lists(cdr, b)?;
                self.lisp.cons(car, rest).map_err(Into::into)
            }
            _ => Err(self.make_error(ErrorKind::TypeError, a)),
        }
    }
    
    /// Evaluate apply - apply function to list of arguments
    fn step_eval_apply(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let func_expr = self.lisp.car(args)?;
        let args_list_expr = self.lisp.car(self.lisp.cdr(args)?)?;
        
        // Evaluate function and arguments list
        let func = self.eval_in_env(func_expr, env)?;
        let args_list = self.eval_in_env(args_list_expr, env)?;
        
        // Evaluate the application using the args list directly (already evaluated)
        let call_expr = self.lisp.cons(func, args_list)?;
        self.push_frame(call_expr, func)?;
        self.push_cont(Cont::ApplyForced { args_expr: args_list, env, call_expr })?;
        Ok(TrampolineState::Return { val: func })
    }
    
    /// Evaluate values - create a multi-value return
    fn eval_values(&mut self, args: ArenaIndex, env: ArenaIndex) -> EvalResult {
        // Evaluate all arguments and return as a list
        // Note: Limited to 16 values due to no_std constraints
        let mut result = self.lisp.nil()?;
        let mut current = args;
        let mut vals: [ArenaIndex; 16] = [ArenaIndex::NULL; 16];
        let mut count = 0;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { car, cdr } => {
                    if count >= 16 {
                        return Err(self.make_error(ErrorKind::StackOverflow, args)
                            .with_message("values: too many values (max 16)"));
                    }
                    vals[count] = self.eval_in_env(car, env)?;
                    count += 1;
                    current = cdr;
                }
                _ => break,
            }
        }
        
        // Build result list in reverse
        for i in (0..count).rev() {
            result = self.lisp.cons(vals[i], result)?;
        }
        
        Ok(result)
    }
    
    /// Evaluate let with TCO in body
    fn eval_let_tco(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<(ArenaIndex, ArenaIndex), EvalError> {
        let bindings = self.lisp.car(args)?;
        let body = self.lisp.cdr(args)?;
        
        // Extend environment with all bindings
        let mut new_env = env;
        let mut current = bindings;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { car: binding, cdr: rest } => {
                    let name = self.lisp.car(binding)?;
                    let value_expr = self.lisp.car(self.lisp.cdr(binding)?)?;
                    let value = self.eval_in_env(value_expr, env)?; // Use original env
                    new_env = self.env_extend(new_env, name, value)?;
                    current = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, bindings)),
            }
        }
        
        // Body becomes a begin block for TCO
        let body_expr = if self.lisp.get(self.lisp.cdr(body)?)?.is_nil() {
            self.lisp.car(body)?
        } else {
            // Wrap in begin
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body)?
        };
        
        Ok((body_expr, new_env))
    }
    
    /// Evaluate let* with TCO in body
    fn eval_let_star_tco(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<(ArenaIndex, ArenaIndex), EvalError> {
        let bindings = self.lisp.car(args)?;
        let body = self.lisp.cdr(args)?;
        
        // Extend environment sequentially
        let mut new_env = env;
        let mut current = bindings;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { car: binding, cdr: rest } => {
                    let name = self.lisp.car(binding)?;
                    let value_expr = self.lisp.car(self.lisp.cdr(binding)?)?;
                    let value = self.eval_in_env(value_expr, new_env)?; // Use NEW env
                    new_env = self.env_extend(new_env, name, value)?;
                    current = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, bindings)),
            }
        }
        
        let body_expr = if self.lisp.get(self.lisp.cdr(body)?)?.is_nil() {
            self.lisp.car(body)?
        } else {
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body)?
        };
        
        Ok((body_expr, new_env))
    }
    
    /// Evaluate lambda
    fn eval_lambda(&mut self, args: ArenaIndex, env: ArenaIndex) -> EvalResult {
        let params = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;
        
        // Wrap body in begin if multiple expressions
        let body = if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
            self.lisp.car(body_list)?
        } else {
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body_list)?
        };
        
        self.lisp.lambda(params, body, env).map_err(Into::into)
    }
    
    /// Evaluate define
    fn eval_define(&mut self, args: ArenaIndex, env: ArenaIndex) -> EvalResult {
        let first = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        
        match self.lisp.get(first)? {
            // (define name value)
            Value::Symbol { .. } => {
                let value_expr = self.lisp.car(rest)?;
                let value = self.eval_in_env(value_expr, env)?;
                self.define(first, value)
            }
            // (define (name params...) body...) -> (define name (lambda (params...) body...))
            Value::Cons { car: name, cdr: params } => {
                let body_list = rest;
                let body = if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
                    self.lisp.car(body_list)?
                } else {
                    let begin = self.lisp.symbol("begin")?;
                    self.lisp.cons(begin, body_list)?
                };
                let lambda = self.lisp.lambda(params, body, env)?;
                self.define(name, lambda)
            }
            _ => Err(self.type_error(first, "symbol or list", self.lisp.get(first)?.type_name())),
        }
    }
    
    /// Evaluate (set! name value) - mutate an existing variable binding
    fn eval_set(&mut self, args: ArenaIndex, env: ArenaIndex) -> EvalResult {
        extract_args!(self, args, name, value_expr);
        
        // Verify name is a symbol
        match self.lisp.get(name)? {
            Value::Symbol { .. } => {
                // Evaluate the value expression
                let value = self.eval_in_env(value_expr, env)?;
                // Find and mutate the binding
                self.env_set(env, name, value)
            }
            _ => Err(self.type_error(name, "symbol", self.lisp.get(name)?.type_name())),
        }
    }
    
    // ========================================================================
    // Helpers
    // ========================================================================
    
    // NOTE: eval_list removed - trampoline handles evaluation directly
    
    /// Evaluate all arguments in a list (synchronously).
    ///
    /// This is used for native function calls where we need all arguments
    /// evaluated before calling the Rust function.
    ///
    /// ITERATIVE implementation to avoid Rust stack overflow
    fn eval_args_list(&mut self, list: ArenaIndex, env: ArenaIndex) -> EvalResult {
        const MAX_ARGS: usize = 64;
        let mut evaluated: [ArenaIndex; MAX_ARGS] = [ArenaIndex::NULL; MAX_ARGS];
        let mut count = 0;
        let mut current = list;
        
        // First pass: evaluate each argument
        loop {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { car, cdr } => {
                    if count >= MAX_ARGS {
                        return Err(self.make_error(ErrorKind::StackOverflow, list));
                    }
                    // Evaluate the expression
                    let evaled = self.eval_in_env(car, env)?;
                    evaluated[count] = evaled;
                    count += 1;
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, list)),
            }
        }
        
        // Second pass: build result list (backwards to preserve order)
        let mut result = self.lisp.nil()?;
        for i in (0..count).rev() {
            result = self.lisp.cons(evaluated[i], result)?;
        }
        
        Ok(result)
    }
    
    /// Count elements in a list
    fn count_list(&self, mut list: ArenaIndex) -> Result<usize, EvalError> {
        let mut count = 0;
        loop {
            match self.lisp.get(list)? {
                Value::Nil => return Ok(count),
                Value::Cons { cdr, .. } => {
                    count += 1;
                    list = cdr;
                }
                _ => return Ok(count), // Rest parameter
            }
        }
    }
    
    /// Create a parameter list from static parameter names
    /// 
    /// This is used by StdLib functions to create their parameter list
    /// from the static &[&str] param names.
    fn make_stdlib_param_list(&self, params: &[&str]) -> Result<ArenaIndex, EvalError> {
        let mut result = self.lisp.nil()?;
        for name in params.iter().rev() {
            let sym = self.lisp.symbol(name)?;
            result = self.lisp.cons(sym, result)?;
        }
        Ok(result)
    }
    
    /// Convert a ParseError to EvalError with stdlib function name context
    fn parse_error_to_eval(&self, err: ParseError, expr: ArenaIndex, func_name: &str) -> EvalError {
        // Build a more descriptive message including the function name
        // We build it manually since we're in no_std
        let mut msg = ErrorMessage::empty();
        let prefix = "stdlib ";
        let suffix = " parse error";
        
        // Copy prefix
        let prefix_bytes = prefix.as_bytes();
        let prefix_len = prefix_bytes.len().min(64);
        msg.buf[..prefix_len].copy_from_slice(&prefix_bytes[..prefix_len]);
        let mut pos = prefix_len;
        
        // Copy function name
        let name_bytes = func_name.as_bytes();
        let name_len = name_bytes.len().min(64 - pos);
        msg.buf[pos..pos + name_len].copy_from_slice(&name_bytes[..name_len]);
        pos += name_len;
        
        // Copy suffix
        let suffix_bytes = suffix.as_bytes();
        let suffix_len = suffix_bytes.len().min(64 - pos);
        msg.buf[pos..pos + suffix_len].copy_from_slice(&suffix_bytes[..suffix_len]);
        pos += suffix_len;
        
        msg.len = pos;
        
        EvalError {
            kind: ErrorKind::Parse,
            message: msg,
            expr,
            expected: None,
            got: None,
            expected_args: None,
            got_args: None,
            backtrace: [StackFrame::default(); MAX_BACKTRACE],
            backtrace_len: 0,
            parse_error: Some(err),
        }
    }
    
    /// Check if a value is false (ONLY #f is false)
    #[inline]
    fn is_false(&self, val: ArenaIndex) -> Result<bool, EvalError> {
        Ok(self.lisp.get(val)?.is_false())
    }
    
    // ========================================================================
    // Convenience
    // ========================================================================
    
    /// Evaluate a string
    pub fn eval_str(&mut self, input: &str) -> EvalResult {
        // Try to parse, with auto-GC retry on out of memory
        let expr = match parse(self.lisp, input) {
            Ok(e) => e,
            Err(e) if matches!(e.kind, ParseErrorKind::OutOfMemory) => {
                // Auto-GC: Run GC and retry parsing
                self.gc();
                parse(self.lisp, input)?
            }
            Err(e) => return Err(e.into()),
        };
        self.eval(expr)
    }
}

// ============================================================================
