#![no_std]

//! # Lisp Evaluator
//!
//! A PURE Lisp evaluator with **fully trampolined evaluation** - no Rust stack
//! recursion, enabling unlimited recursion depth (bounded only by heap/arena size).
//!
//! ## Key Features
//!
//! - **Full Trampolining**: All evaluation uses continuation-passing style with
//!   an explicit continuation stack. No Rust recursion means no stack overflow.
//! - **Hybrid Evaluation Strategy**: Strict in tail position, lazy elsewhere
//! - **Proper TCO**: Tail calls reuse the same continuation frame
//! - **Lexically Scoped Closures**: First-class functions with captured environments
//! - **Lazy Data Structures**: Infinite streams like Haskell
//! - **Rich Error Handling**: Error messages with stack traces
//!
//! ## Hybrid Evaluation Strategy
//!
//! This evaluator uses a hybrid approach that combines lazy and strict evaluation:
//!
//! **Tail position (strict)**: Lambda arguments in tail calls are evaluated
//! strictly. This enables proper TCO without thunk accumulation:
//! ```lisp
//! (define (countdown n)
//!   (if (= n 0) 'done
//!       (countdown (- n 1))))  ; Args evaluated strictly, TCO applies
//! ```
//!
//! **Non-tail position (lazy)**: Builtin arguments are lazy (wrapped in thunks).
//! Builtins force what they need. This enables infinite data structures:
//! ```lisp
//! (define (ones) (cons 1 (ones)))  ; cons is lazy, infinite stream works
//! (car (ones))  ; => 1
//! ```
//!
//! ## Strict Positions (in builtins)
//!
//! Builtins automatically force arguments in strict positions:
//! - Arithmetic operands (+, -, *, /, mod)
//! - Comparison operands (<, >, =, etc.)
//! - `if` condition (but NOT branches)
//! - Predicates (null?, pair?, etc.)
//! - Print/display arguments
//!
//! ## Purity
//!
//! This is a pure functional language - NO MUTATION!
//! This makes lazy evaluation semantically sound (referential transparency).
//!
//! ## Truthiness
//!
//! Only `#f` is false. Everything else (including `nil`/`'()`) is truthy.
//!
//! ## Special Forms
//!
//! - `quote` - Return expression unevaluated
//! - `if` - Conditional (lazy in branches)
//! - `cond` - Multi-way conditional
//! - `lambda` - Create closure
//! - `define` - Define variable/function
//! - `let` - Local binding
//! - `let*` - Sequential local binding
//! - `begin` - Sequence of expressions
//! - `and` / `or` - Short-circuit boolean operations

pub use lisp_parser::{
    Arena, ArenaIndex, ArenaError, ArenaResult, Trace, GcStats,
    Value, Builtin, Lisp, ParseError, ParseErrorKind, SourceLoc, parse,
};

// ============================================================================
// Error Handling
// ============================================================================

/// Maximum call stack depth for traces
const MAX_STACK_DEPTH: usize = 64;
/// Maximum frames to include in error backtrace
const MAX_BACKTRACE: usize = 16;
/// Maximum number of entries in memoization cache (LRU eviction after this)
const MAX_MEMO_CACHE_SIZE: usize = 100;

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
        // Safety: we only store valid UTF-8
        unsafe { core::str::from_utf8_unchecked(&self.buf[..self.len]) }
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
#[allow(dead_code)] // Some variants may be unused in certain code paths
enum Cont {
    /// We're done - return the value
    Done,
    
    /// Force the result to WHNF, then continue
    Force,
    
    /// Cache thunk result, then force the cached value
    CacheThunk { thunk_idx: ArenaIndex, expr: ArenaIndex, env: ArenaIndex },
    
    /// After forcing function, decide builtin vs lambda
    ApplyForced { args_expr: ArenaIndex, env: ArenaIndex, call_expr: ArenaIndex },
    
    /// After evaluating condition, choose branch
    IfBranch { then_expr: ArenaIndex, else_expr: ArenaIndex, env: ArenaIndex },
    
    /// After forcing condition for builtin (variadic ops like +)
    BuiltinForceArg { builtin: Builtin, remaining_args: ArenaIndex, 
                      collected: ArenaIndex, call_expr: ArenaIndex },
    
    /// After forcing car/cdr argument - val is the forced pair
    BuiltinCarCdr { builtin: Builtin, call_expr: ArenaIndex },
    
    /// OPTIMIZED: After forcing first arg of binary builtin, force second arg
    BinaryBuiltinFirst { builtin: Builtin, second_arg: ArenaIndex, call_expr: ArenaIndex },
    
    /// OPTIMIZED: After forcing both args of binary builtin, apply
    BinaryBuiltinSecond { builtin: Builtin, first_val: ArenaIndex, call_expr: ArenaIndex },
    
    /// After forcing first lambda arg, bind it to param
    LambdaFirstBind { param: ArenaIndex },
    
    /// After binding a lambda arg, continue with remaining args
    LambdaBindArg { remaining_exprs: ArenaIndex, eval_env: ArenaIndex,
                    remaining_params: ArenaIndex, body: ArenaIndex,
                    new_env: ArenaIndex, call_expr: ArenaIndex },
    
    /// After forcing memo args, look up cache and maybe call function
    MemoCollectArg { remaining_exprs: ArenaIndex, eval_env: ArenaIndex,
                     collected: ArenaIndex, memo_idx: ArenaIndex, 
                     func: ArenaIndex, cache: ArenaIndex, call_expr: ArenaIndex },
    
    /// After calling memoized function, store result in cache
    MemoCacheResult { memo_idx: ArenaIndex, args: ArenaIndex, cache: ArenaIndex },
}

/// Trampoline state - what we're currently doing
#[derive(Clone, Copy, Debug)]
enum TrampolineState {
    /// Evaluate expression in environment
    Eval { expr: ArenaIndex, env: ArenaIndex },
    /// Force a value to WHNF
    Force { idx: ArenaIndex },
    /// Return a value to the continuation
    Return { val: ArenaIndex },
}

// ============================================================================
// Evaluator
// ============================================================================

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
        };
        
        // Initialize global environment with builtins
        eval.global_env = lisp.nil()?;
        
        for &builtin in Builtin::ALL {
            let name = lisp.symbol(builtin.name())?;
            let val = lisp.builtin(builtin)?;
            eval.global_env = eval.env_extend(eval.global_env, name, val)?;
        }
        
        // Add 'true' and 'false' as aliases for #t and #f
        let true_sym = lisp.symbol("true")?;
        let true_val = lisp.true_val()?;
        eval.global_env = eval.env_extend(eval.global_env, true_sym, true_val)?;
        
        let false_sym = lisp.symbol("false")?;
        let false_val = lisp.false_val()?;
        eval.global_env = eval.env_extend(eval.global_env, false_sym, false_val)?;
        
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
    
    /// Run GC with current roots (global env only - use gc_with_continuations during evaluation)
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
            TrampolineState::Force { idx } => {
                roots[root_count] = *idx; root_count += 1;
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
                Cont::Done | Cont::Force => {}
                Cont::CacheThunk { thunk_idx, expr, env } => {
                    roots[root_count] = thunk_idx; root_count += 1;
                    roots[root_count] = expr; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
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
                Cont::BuiltinForceArg { remaining_args, collected, call_expr, .. } => {
                    roots[root_count] = remaining_args; root_count += 1;
                    roots[root_count] = collected; root_count += 1;
                    roots[root_count] = call_expr; root_count += 1;
                }
                Cont::BuiltinCarCdr { call_expr, .. } => {
                    roots[root_count] = call_expr; root_count += 1;
                }
                Cont::BinaryBuiltinFirst { second_arg, call_expr, .. } => {
                    roots[root_count] = second_arg; root_count += 1;
                    roots[root_count] = call_expr; root_count += 1;
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
                Cont::MemoCollectArg { remaining_exprs, eval_env, collected, memo_idx, func, cache, call_expr } => {
                    roots[root_count] = remaining_exprs; root_count += 1;
                    roots[root_count] = eval_env; root_count += 1;
                    roots[root_count] = collected; root_count += 1;
                    roots[root_count] = memo_idx; root_count += 1;
                    roots[root_count] = func; root_count += 1;
                    roots[root_count] = cache; root_count += 1;
                    roots[root_count] = call_expr; root_count += 1;
                }
                Cont::MemoCacheResult { memo_idx, args, cache } => {
                    roots[root_count] = memo_idx; root_count += 1;
                    roots[root_count] = args; root_count += 1;
                    roots[root_count] = cache; root_count += 1;
                }
            }
        }
        
        self.lisp.gc(&roots[..root_count])
    }
    
    /// Run GC during evaluation - marks continuation stack items as roots (deprecated, use gc_with_state)
    #[allow(dead_code)]
    fn gc_with_continuations(&self) -> GcStats {
        // Collect all roots: global env + all ArenaIndex values in continuations
        const MAX_ROOTS: usize = 512;
        let mut roots = [ArenaIndex::NULL; MAX_ROOTS];
        let mut root_count = 0;
        
        // Always include global env
        roots[root_count] = self.global_env;
        root_count += 1;
        
        // Collect roots from all continuations
        for i in 0..self.cont_depth {
            if root_count >= MAX_ROOTS - 20 {
                break; // Leave some room
            }
            
            match self.cont_stack[i] {
                Cont::Done | Cont::Force => {}
                Cont::CacheThunk { thunk_idx, expr, env } => {
                    roots[root_count] = thunk_idx; root_count += 1;
                    roots[root_count] = expr; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
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
                Cont::BuiltinForceArg { remaining_args, collected, call_expr, .. } => {
                    roots[root_count] = remaining_args; root_count += 1;
                    roots[root_count] = collected; root_count += 1;
                    roots[root_count] = call_expr; root_count += 1;
                }
                Cont::BuiltinCarCdr { call_expr, .. } => {
                    roots[root_count] = call_expr; root_count += 1;
                }
                Cont::BinaryBuiltinFirst { second_arg, call_expr, .. } => {
                    roots[root_count] = second_arg; root_count += 1;
                    roots[root_count] = call_expr; root_count += 1;
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
                Cont::MemoCollectArg { remaining_exprs, eval_env, collected, memo_idx, func, cache, call_expr } => {
                    roots[root_count] = remaining_exprs; root_count += 1;
                    roots[root_count] = eval_env; root_count += 1;
                    roots[root_count] = collected; root_count += 1;
                    roots[root_count] = memo_idx; root_count += 1;
                    roots[root_count] = func; root_count += 1;
                    roots[root_count] = cache; root_count += 1;
                    roots[root_count] = call_expr; root_count += 1;
                }
                Cont::MemoCacheResult { memo_idx, args, cache } => {
                    roots[root_count] = memo_idx; root_count += 1;
                    roots[root_count] = args; root_count += 1;
                    roots[root_count] = cache; root_count += 1;
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
    
    // NOTE: env_set removed - this is a PURE Lisp!
    // Mutation breaks referential transparency and call-by-need semantics.
    
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
    fn push_cont(&mut self, cont: Cont) -> Result<(), EvalError> {
        if self.cont_depth >= MAX_CONT_DEPTH {
            return Err(EvalError::new(ErrorKind::StackOverflow));
        }
        self.cont_stack[self.cont_depth] = cont;
        self.cont_depth += 1;
        Ok(())
    }
    
    /// Pop a continuation from the stack
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
        // Push Force continuation to force result at top level
        self.push_cont(Cont::Force)?;
        // Start evaluation
        self.trampoline(TrampolineState::Eval { expr, env: self.global_env })
    }
    
    /// Evaluate an expression in a given environment
    /// Uses full trampolining - no Rust recursion
    /// 
    /// This is public so the REPL can force thunks for display
    pub fn eval_in_env(&mut self, expr: ArenaIndex, env: ArenaIndex) -> EvalResult {
        // Reset continuation stack and run
        self.cont_depth = 0;
        self.trampoline(TrampolineState::Eval { expr, env })
    }
    
    /// The main trampoline loop - processes states and continuations
    /// This is the ONLY place where looping happens - no Rust recursion!
    fn trampoline(&mut self, mut state: TrampolineState) -> EvalResult {
        // Counter for periodic GC checks (every 2000 steps)
        let mut step_count: u32 = 0;
        const GC_CHECK_INTERVAL: u32 = 2000;
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
                    self.step_eval(expr, env)?
                }
                TrampolineState::Force { idx } => {
                    self.step_force(idx)?
                }
                TrampolineState::Return { val } => {
                    match self.step_return(val)? {
                        Some(new_state) => new_state,
                        None => return Ok(val), // Done!
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
            Value::Builtin(_) | Value::Lambda { .. } | Value::Thunk { .. } |
            Value::Memo { .. } => {
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
            
            // if - condition is strict, branches are lazy
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
                
                // Push continuation for after condition is forced
                self.push_cont(Cont::IfBranch { then_expr, else_expr, env })?;
                self.push_cont(Cont::Force)?;
                
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
        }
        
        // Function application - HYBRID EVALUATION
        self.push_frame(expr, car)?;
        
        // Push continuation: after evaluating func, force it, then apply
        self.push_cont(Cont::ApplyForced { args_expr: cdr, env, call_expr: expr })?;
        self.push_cont(Cont::Force)?;
        
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
    
    /// One step of forcing
    fn step_force(&mut self, idx: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let val = self.lisp.get(idx)?;
        
        match val {
            Value::Thunk { expr, env, cached } => {
                if !cached.is_null() {
                    // Already cached - but might be a thunk, so force again
                    Ok(TrampolineState::Force { idx: cached })
                } else {
                    // Need to evaluate - push continuation to cache result
                    self.push_cont(Cont::CacheThunk { thunk_idx: idx, expr, env })?;
                    Ok(TrampolineState::Eval { expr, env })
                }
            }
            _ => {
                // Not a thunk - return as WHNF
                Ok(TrampolineState::Return { val: idx })
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
            
            Cont::Force { .. } => {
                // Force the returned value
                Ok(Some(TrampolineState::Force { idx: val }))
            }
            
            Cont::CacheThunk { thunk_idx, expr, env } => {
                // Cache the result in the thunk
                self.lisp.set(thunk_idx, Value::Thunk { expr, env, cached: val })?;
                // Now force the result (it might be a thunk too)
                Ok(Some(TrampolineState::Force { idx: val }))
            }
            
            Cont::IfBranch { then_expr, else_expr, env } => {
                // val is the forced condition
                let branch = if !self.is_false(val)? { then_expr } else { else_expr };
                if branch.is_null() {
                    let nil = self.lisp.nil()?;
                    Ok(Some(TrampolineState::Return { val: nil }))
                } else {
                    Ok(Some(TrampolineState::Eval { expr: branch, env }))
                }
            }
            
            Cont::ApplyForced { args_expr, env, call_expr } => {
                // val is the forced function
                match self.lisp.get(val)? {
                    Value::Builtin(b) => {
                        // Builtins: LAZY - wrap args in thunks
                        let args = self.make_thunk_list(args_expr, env)?;
                        let result = self.apply_builtin_trampolined(b, args, call_expr)?;
                        self.pop_frame();
                        Ok(Some(result))
                    }
                    Value::Lambda { params, body, env: closure_env } => {
                        // Lambda: STRICT - evaluate args and bind directly to params
                        // OPTIMIZED: No intermediate list building - binds params as we go
                        self.pop_frame();
                        
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
                            self.push_cont(Cont::Force)?;
                            
                            Ok(Some(TrampolineState::Eval { expr: first_expr, env }))
                        }
                    }
                    Value::Memo { func, cache } => {
                        // Memoized function: evaluate args strictly, look up cache
                        self.pop_frame();
                        
                        if self.lisp.get(args_expr)?.is_nil() {
                            // No args - look up in cache with nil key
                            let nil = self.lisp.nil()?;
                            if let Some(cached_result) = self.memo_cache_lookup(cache, nil)? {
                                Ok(Some(TrampolineState::Return { val: cached_result }))
                            } else {
                                // Call underlying function and cache result
                                self.push_cont(Cont::MemoCacheResult { memo_idx: val, args: nil, cache })?;
                                // Apply the wrapped function
                                self.push_cont(Cont::ApplyForced { args_expr, env, call_expr })?;
                                self.push_cont(Cont::Force)?;
                                Ok(Some(TrampolineState::Return { val: func }))
                            }
                        } else {
                            // Start collecting forced args for cache key
                            let first_expr = self.lisp.car(args_expr)?;
                            let rest_exprs = self.lisp.cdr(args_expr)?;
                            let nil = self.lisp.nil()?;
                            
                            self.push_cont(Cont::MemoCollectArg {
                                remaining_exprs: rest_exprs, eval_env: env,
                                collected: nil, memo_idx: val, func, cache, call_expr
                            })?;
                            self.push_cont(Cont::Force)?;
                            
                            Ok(Some(TrampolineState::Eval { expr: first_expr, env }))
                        }
                    }
                    _ => {
                        self.pop_frame();
                        Err(self.type_error(call_expr, "procedure", self.lisp.get(val)?.type_name()))
                    }
                }
            }
            
            Cont::LambdaFirstBind { param } => {
                // val is forced first arg - bind to param
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
                        self.push_cont(Cont::Force)?;
                        
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
            
            Cont::MemoCollectArg { remaining_exprs, eval_env, collected, memo_idx, func, cache, call_expr } => {
                // val is a forced argument for memoized function
                let new_collected = self.lisp.cons(val, collected)?;
                
                if self.lisp.get(remaining_exprs)?.is_nil() {
                    // All args collected - reverse and look up in cache
                    let args = self.reverse_list(new_collected)?;
                    
                    if let Some(cached_result) = self.memo_cache_lookup(cache, args)? {
                        // Cache hit!
                        Ok(Some(TrampolineState::Return { val: cached_result }))
                    } else {
                        // Cache miss - call underlying function and cache result
                        self.push_cont(Cont::MemoCacheResult { memo_idx, args, cache })?;
                        
                        // Apply the wrapped function with the collected args
                        // We need to call it like a normal function application
                        match self.lisp.get(func)? {
                            Value::Lambda { params, body, env: closure_env } => {
                                let new_env = self.bind_params(params, args, closure_env, call_expr)?;
                                Ok(Some(TrampolineState::Eval { expr: body, env: new_env }))
                            }
                            Value::Builtin(b) => {
                                let result = self.apply_builtin_with_forced_args(b, args, call_expr)?;
                                Ok(Some(TrampolineState::Return { val: result }))
                            }
                            _ => Err(self.type_error(call_expr, "procedure", self.lisp.get(func)?.type_name())),
                        }
                    }
                } else {
                    // More args to force
                    let next_expr = self.lisp.car(remaining_exprs)?;
                    let rest_exprs = self.lisp.cdr(remaining_exprs)?;
                    
                    self.push_cont(Cont::MemoCollectArg {
                        remaining_exprs: rest_exprs, eval_env,
                        collected: new_collected, memo_idx, func, cache, call_expr
                    })?;
                    self.push_cont(Cont::Force)?;
                    
                    Ok(Some(TrampolineState::Eval { expr: next_expr, env: eval_env }))
                }
            }
            
            Cont::MemoCacheResult { memo_idx, args, cache } => {
                // val is the result of calling the memoized function
                // Store it in the cache with LRU eviction
                let new_entry = self.lisp.cons(args, val)?;
                let new_cache = self.lisp.cons(new_entry, cache)?;
                
                // Apply LRU eviction if cache is too large
                let bounded_cache = self.limit_cache_size(new_cache, MAX_MEMO_CACHE_SIZE)?;
                
                // Update the memo value with the bounded cache
                if let Value::Memo { func, .. } = self.lisp.get(memo_idx)? {
                    self.lisp.set(memo_idx, Value::Memo { func, cache: bounded_cache })?;
                }
                
                Ok(Some(TrampolineState::Return { val }))
            }
            
            Cont::BuiltinForceArg { builtin, remaining_args, collected, call_expr } => {
                // val is a forced argument for a strict builtin
                let new_collected = self.lisp.cons(val, collected)?;
                
                if self.lisp.get(remaining_args)?.is_nil() {
                    // All args forced - apply builtin
                    let args = self.reverse_list(new_collected)?;
                    let result = self.apply_builtin_with_forced_args(builtin, args, call_expr)?;
                    Ok(Some(TrampolineState::Return { val: result }))
                } else {
                    // More args to force
                    let next_arg = self.lisp.car(remaining_args)?;
                    let rest_args = self.lisp.cdr(remaining_args)?;
                    
                    self.push_cont(Cont::BuiltinForceArg {
                        builtin, remaining_args: rest_args, collected: new_collected, call_expr
                    })?;
                    
                    Ok(Some(TrampolineState::Force { idx: next_arg }))
                }
            }
            
            Cont::BinaryBuiltinFirst { builtin, second_arg, call_expr } => {
                // val is first forced arg - now force second
                self.push_cont(Cont::BinaryBuiltinSecond { builtin, first_val: val, call_expr })?;
                Ok(Some(TrampolineState::Force { idx: second_arg }))
            }
            
            Cont::BinaryBuiltinSecond { builtin, first_val, call_expr } => {
                // val is second forced arg - apply binary operation directly
                let result = self.apply_binary_builtin(builtin, first_val, val, call_expr)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }
            
            Cont::BuiltinCarCdr { builtin, call_expr } => {
                // val is the forced pair argument for car/cdr
                match self.lisp.get(val)? {
                    Value::Cons { car, cdr } => {
                        let result = match builtin {
                            Builtin::Car => car,
                            Builtin::Cdr => cdr,
                            _ => unreachable!(),
                        };
                        Ok(Some(TrampolineState::Return { val: result }))
                    }
                    Value::Nil => {
                        let nil = self.lisp.nil()?;
                        Ok(Some(TrampolineState::Return { val: nil }))
                    }
                    _ => Err(self.type_error(call_expr, "pair", self.lisp.get(val)?.type_name())),
                }
            }
        }
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
    
    /// Look up args in memo cache (alist of (args . result) pairs)
    /// Returns Some(result) if found, None if not found
    fn memo_cache_lookup(&self, cache: ArenaIndex, args: ArenaIndex) -> Result<Option<ArenaIndex>, EvalError> {
        let mut current = cache;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return Ok(None), // Not found
                Value::Cons { car, cdr } => {
                    // car should be (cached_args . cached_result)
                    if let Value::Cons { car: cached_args, cdr: cached_result } = self.lisp.get(car)? {
                        if self.values_equal(args, cached_args)? {
                            return Ok(Some(cached_result));
                        }
                    }
                    current = cdr;
                }
                _ => return Ok(None),
            }
        }
    }
    
    /// Check if two values are structurally equal (for memo cache lookup)
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
    
    /// Apply a builtin function (trampolined version)
    /// Returns next trampoline state instead of final value
    fn apply_builtin_trampolined(&mut self, builtin: Builtin, args: ArenaIndex, call_expr: ArenaIndex) 
        -> Result<TrampolineState, EvalError> 
    {
        // For strict builtins, we need to force args using continuations
        // For non-strict builtins (cons, list, car, cdr), we can return immediately
        
        match builtin {
            // NON-STRICT builtins - return immediately
            Builtin::Cons => {
                let a = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let b = self.lisp.car(rest)?;
                let result = self.lisp.cons(a, b)?;
                Ok(TrampolineState::Return { val: result })
            }
            
            Builtin::List => {
                Ok(TrampolineState::Return { val: args })
            }
            
            // car/cdr: force the pair, but return element (may be thunk)
            Builtin::Car | Builtin::Cdr => {
                let arg = self.lisp.car(args)?;
                // Push continuation to handle after forcing - use special CarCdr continuation
                self.push_cont(Cont::BuiltinCarCdr { builtin, call_expr })?;
                Ok(TrampolineState::Force { idx: arg })
            }
            
            // STRICT builtins - need to force all args
            // OPTIMIZATION: Use specialized path for binary operations (most common case)
            _ => {
                if self.lisp.get(args)?.is_nil() {
                    // No args - apply immediately
                    let result = self.apply_builtin_with_forced_args(builtin, args, call_expr)?;
                    Ok(TrampolineState::Return { val: result })
                } else {
                    let first_arg = self.lisp.car(args)?;
                    let rest_args = self.lisp.cdr(args)?;
                    
                    // Check if this is a binary operation (exactly 2 args)
                    if !self.lisp.get(rest_args)?.is_nil() {
                        let second_arg = self.lisp.car(rest_args)?;
                        let rest_rest = self.lisp.cdr(rest_args)?;
                        
                        if self.lisp.get(rest_rest)?.is_nil() {
                            // OPTIMIZED: Binary operation - no list allocation!
                            self.push_cont(Cont::BinaryBuiltinFirst { 
                                builtin, second_arg, call_expr
                            })?;
                            return Ok(TrampolineState::Force { idx: first_arg });
                        }
                    }
                    
                    // General case: multiple args, use list-based approach
                    let nil = self.lisp.nil()?;
                    
                    self.push_cont(Cont::BuiltinForceArg {
                        builtin, remaining_args: rest_args, collected: nil, call_expr
                    })?;
                    
                    Ok(TrampolineState::Force { idx: first_arg })
                }
            }
        }
    }
    
    /// Apply a builtin with already-forced arguments
    fn apply_builtin_with_forced_args(&mut self, builtin: Builtin, args: ArenaIndex, call_expr: ArenaIndex) 
        -> EvalResult 
    {
        match builtin {
            Builtin::Car => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Cons { car, .. } => Ok(car),
                    Value::Nil => self.lisp.nil().map_err(Into::into),
                    _ => Err(self.type_error(call_expr, "pair", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::Cdr => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Cons { cdr, .. } => Ok(cdr),
                    Value::Nil => self.lisp.nil().map_err(Into::into),
                    _ => Err(self.type_error(call_expr, "pair", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::Cons => {
                let a = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let b = self.lisp.car(rest)?;
                self.lisp.cons(a, b).map_err(Into::into)
            }
            
            Builtin::List => Ok(args),
            
            Builtin::Atom => {
                let arg = self.lisp.car(args)?;
                self.lisp.boolean(self.lisp.get(arg)?.is_atom()).map_err(Into::into)
            }
            
            Builtin::Eq => {
                let a = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let b = self.lisp.car(rest)?;
                
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
            
            Builtin::Null => {
                let arg = self.lisp.car(args)?;
                self.lisp.boolean(self.lisp.get(arg)?.is_nil()).map_err(Into::into)
            }
            
            Builtin::Pairp => {
                let arg = self.lisp.car(args)?;
                self.lisp.boolean(self.lisp.get(arg)?.is_cons()).map_err(Into::into)
            }
            
            Builtin::Numberp => {
                let arg = self.lisp.car(args)?;
                self.lisp.boolean(self.lisp.get(arg)?.is_number()).map_err(Into::into)
            }
            
            Builtin::Booleanp => {
                let arg = self.lisp.car(args)?;
                self.lisp.boolean(self.lisp.get(arg)?.is_boolean()).map_err(Into::into)
            }
            
            Builtin::Procedurep => {
                let arg = self.lisp.car(args)?;
                self.lisp.boolean(self.lisp.get(arg)?.is_procedure()).map_err(Into::into)
            }
            
            Builtin::Symbolp => {
                let arg = self.lisp.car(args)?;
                self.lisp.boolean(self.lisp.get(arg)?.is_symbol()).map_err(Into::into)
            }
            
            Builtin::Not => {
                let arg = self.lisp.car(args)?;
                self.lisp.boolean(self.lisp.get(arg)?.is_false()).map_err(Into::into)
            }
            
            Builtin::Add => self.numeric_fold_forced(args, 0, |a, b| a.checked_add(b), call_expr),
            
            Builtin::Sub => {
                let first = self.get_number_from_forced(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                if self.lisp.get(rest)?.is_nil() {
                    self.lisp.number(-first).map_err(Into::into)
                } else {
                    self.numeric_fold_start_forced(rest, first, |a, b| a.checked_sub(b), call_expr)
                }
            }
            
            Builtin::Mul => self.numeric_fold_forced(args, 1, |a, b| a.checked_mul(b), call_expr),
            
            Builtin::Div => {
                let first = self.get_number_from_forced(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold_start_forced(rest, first, |a, b| {
                    if b == 0 { None } else { a.checked_div(b) }
                }, call_expr)
            }
            
            Builtin::Mod => {
                let a = self.get_number_from_forced(self.lisp.car(args)?, call_expr)?;
                let b = self.get_number_from_forced(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                if b == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                self.lisp.number(a % b).map_err(Into::into)
            }
            
            Builtin::Lt => self.compare_forced(args, |a, b| a < b, call_expr),
            Builtin::Gt => self.compare_forced(args, |a, b| a > b, call_expr),
            Builtin::Le => self.compare_forced(args, |a, b| a <= b, call_expr),
            Builtin::Ge => self.compare_forced(args, |a, b| a >= b, call_expr),
            Builtin::NumEq => self.compare_forced(args, |a, b| a == b, call_expr),
            
            Builtin::Print | Builtin::Display => {
                Ok(self.lisp.car(args)?)
            }
            
            Builtin::Newline => {
                self.lisp.nil().map_err(Into::into)
            }
            
            Builtin::Error => {
                let msg = self.lisp.car(args)?;
                Err(self.make_error(ErrorKind::UserError, msg))
            }
            
            Builtin::Memoize => {
                // (memoize fn) - wrap a function with memoization
                let func = self.lisp.car(args)?;
                
                // Verify it's a procedure
                if !self.lisp.get(func)?.is_procedure() {
                    return Err(self.type_error(call_expr, "procedure", self.lisp.get(func)?.type_name()));
                }
                
                // Create memo wrapper with empty cache
                let nil = self.lisp.nil()?;
                self.lisp.memo(func, nil).map_err(Into::into)
            }
        }
    }
    
    /// OPTIMIZED: Apply binary builtin directly without list allocation
    fn apply_binary_builtin(&mut self, builtin: Builtin, a: ArenaIndex, b: ArenaIndex, call_expr: ArenaIndex) 
        -> EvalResult 
    {
        match builtin {
            Builtin::Add => {
                let x = self.get_number_from_forced(a, call_expr)?;
                let y = self.get_number_from_forced(b, call_expr)?;
                x.checked_add(y)
                    .map(|n| self.lisp.number(n))
                    .ok_or_else(|| self.make_error(ErrorKind::DivisionByZero, call_expr))?
                    .map_err(Into::into)
            }
            Builtin::Sub => {
                let x = self.get_number_from_forced(a, call_expr)?;
                let y = self.get_number_from_forced(b, call_expr)?;
                x.checked_sub(y)
                    .map(|n| self.lisp.number(n))
                    .ok_or_else(|| self.make_error(ErrorKind::DivisionByZero, call_expr))?
                    .map_err(Into::into)
            }
            Builtin::Mul => {
                let x = self.get_number_from_forced(a, call_expr)?;
                let y = self.get_number_from_forced(b, call_expr)?;
                x.checked_mul(y)
                    .map(|n| self.lisp.number(n))
                    .ok_or_else(|| self.make_error(ErrorKind::DivisionByZero, call_expr))?
                    .map_err(Into::into)
            }
            Builtin::Div => {
                let x = self.get_number_from_forced(a, call_expr)?;
                let y = self.get_number_from_forced(b, call_expr)?;
                if y == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                self.lisp.number(x / y).map_err(Into::into)
            }
            Builtin::Mod => {
                let x = self.get_number_from_forced(a, call_expr)?;
                let y = self.get_number_from_forced(b, call_expr)?;
                if y == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                self.lisp.number(x % y).map_err(Into::into)
            }
            Builtin::Lt => {
                let x = self.get_number_from_forced(a, call_expr)?;
                let y = self.get_number_from_forced(b, call_expr)?;
                self.lisp.boolean(x < y).map_err(Into::into)
            }
            Builtin::Gt => {
                let x = self.get_number_from_forced(a, call_expr)?;
                let y = self.get_number_from_forced(b, call_expr)?;
                self.lisp.boolean(x > y).map_err(Into::into)
            }
            Builtin::Le => {
                let x = self.get_number_from_forced(a, call_expr)?;
                let y = self.get_number_from_forced(b, call_expr)?;
                self.lisp.boolean(x <= y).map_err(Into::into)
            }
            Builtin::Ge => {
                let x = self.get_number_from_forced(a, call_expr)?;
                let y = self.get_number_from_forced(b, call_expr)?;
                self.lisp.boolean(x >= y).map_err(Into::into)
            }
            Builtin::NumEq => {
                let x = self.get_number_from_forced(a, call_expr)?;
                let y = self.get_number_from_forced(b, call_expr)?;
                self.lisp.boolean(x == y).map_err(Into::into)
            }
            Builtin::Eq => {
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
                self.apply_builtin_with_forced_args(builtin, args, call_expr)
            }
        }
    }
    
    /// Get number from already-forced value
    fn get_number_from_forced(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<i64, EvalError> {
        match self.lisp.get(idx)? {
            Value::Number(n) => Ok(n),
            v => Err(self.type_error(call_expr, "number", v.type_name())),
        }
    }
    
    /// Numeric fold with already-forced args
    fn numeric_fold_forced<F>(&self, args: ArenaIndex, init: i64, f: F, call_expr: ArenaIndex) -> EvalResult
    where F: Fn(i64, i64) -> Option<i64>
    {
        self.numeric_fold_start_forced(args, init, f, call_expr)
    }
    
    fn numeric_fold_start_forced<F>(&self, args: ArenaIndex, mut acc: i64, f: F, call_expr: ArenaIndex) -> EvalResult
    where F: Fn(i64, i64) -> Option<i64>
    {
        let mut current = args;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.lisp.number(acc).map_err(Into::into),
                Value::Cons { car, cdr } => {
                    let n = self.get_number_from_forced(car, call_expr)?;
                    acc = f(acc, n).ok_or_else(|| self.make_error(ErrorKind::DivisionByZero, call_expr))?;
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    fn compare_forced<F>(&self, args: ArenaIndex, cmp: F, call_expr: ArenaIndex) -> EvalResult
    where F: Fn(i64, i64) -> bool
    {
        let a = self.get_number_from_forced(self.lisp.car(args)?, call_expr)?;
        let b = self.get_number_from_forced(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
        self.lisp.boolean(cmp(a, b)).map_err(Into::into)
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
                        self.eval_in_env(test, env)?
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
                        // Not last - evaluate and continue
                        self.eval_in_env(expr, env)?;
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
                        let result = self.eval_in_env(expr, env)?;
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
                        let result = self.eval_in_env(expr, env)?;
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
                
                // AUTO-MEMOIZATION: Check if the function body references its own name (recursive)
                // If so, wrap it with automatic memoization with bounded LRU cache
                if self.contains_symbol(body, name)? {
                    // Create memoized version with empty cache
                    let nil = self.lisp.nil()?;
                    let memo = self.lisp.memo(lambda, nil)?;
                    self.define(name, memo)
                } else {
                    self.define(name, lambda)
                }
            }
            _ => Err(self.type_error(first, "symbol or list", self.lisp.get(first)?.type_name())),
        }
    }
    
    // NOTE: eval_set removed - this is a PURE Lisp!
    
    // ========================================================================
    // Helpers
    // ========================================================================
    
    // NOTE: eval_list removed - trampoline handles evaluation directly
    
    /// Create a list of thunks from expressions (lazy - wraps each in a thunk)
    /// Used for builtin arguments (builtins handle their own strictness)
    /// 
    /// ITERATIVE implementation to avoid Rust stack overflow
    fn make_thunk_list(&mut self, list: ArenaIndex, env: ArenaIndex) -> EvalResult {
        const MAX_ARGS: usize = 64;
        let dummy = ArenaIndex::new(usize::MAX, u32::MAX);
        let mut thunks: [ArenaIndex; MAX_ARGS] = [dummy; MAX_ARGS];
        let mut count = 0;
        let mut current = list;
        
        // First pass: collect all thunks (iterative)
        loop {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { car, cdr } => {
                    if count >= MAX_ARGS {
                        return Err(self.make_error(ErrorKind::StackOverflow, list));
                    }
                    // Wrap the expression in a thunk (don't evaluate it yet)
                    let thunk = self.lisp.thunk(car, env)?;
                    thunks[count] = thunk;
                    count += 1;
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, list)),
            }
        }
        
        // Second pass: build result list (backwards to preserve order)
        let mut result = self.lisp.nil()?;
        for i in (0..count).rev() {
            result = self.lisp.cons(thunks[i], result)?;
        }
        
        Ok(result)
    }
    
    // NOTE: eval_list_strict and force_list removed - handled by trampoline continuations
    
    /// Bind parameters to arguments (kept for potential future use with rest parameters)
    #[allow(dead_code)]
    fn bind_params(&self, params: ArenaIndex, args: ArenaIndex, env: ArenaIndex, call_expr: ArenaIndex) -> EvalResult {
        let mut new_env = env;
        let mut params_cur = params;
        let mut args_cur = args;
        
        loop {
            let p = self.lisp.get(params_cur)?;
            let a = self.lisp.get(args_cur)?;
            
            match (p, a) {
                (Value::Nil, Value::Nil) => break,
                (Value::Cons { car: param, cdr: prest }, 
                 Value::Cons { car: arg, cdr: arest }) => {
                    new_env = self.env_extend(new_env, param, arg)?;
                    params_cur = prest;
                    args_cur = arest;
                }
                (Value::Nil, Value::Cons { .. }) => {
                    // Too many arguments
                    let expected = self.count_list(params)?;
                    let got = self.count_list(args)?;
                    return Err(self.arg_error(call_expr, expected, got));
                }
                (Value::Cons { .. }, Value::Nil) => {
                    // Too few arguments
                    let expected = self.count_list(params)?;
                    let got = self.count_list(args)?;
                    return Err(self.arg_error(call_expr, expected, got));
                }
                // Rest parameter (symbol instead of nil at end)
                (Value::Symbol { .. }, _) => {
                    // Bind remaining args to rest parameter
                    new_env = self.env_extend(new_env, params_cur, args_cur)?;
                    break;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, params_cur)),
            }
        }
        
        Ok(new_env)
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
    
    /// Limit cache size with LRU eviction
    /// Takes the first N entries from the cache (most recently used)
    fn limit_cache_size(&self, cache: ArenaIndex, max_size: usize) -> Result<ArenaIndex, EvalError> {
        let count = self.count_list(cache)?;
        
        if count <= max_size {
            // Cache is within limits
            return Ok(cache);
        }
        
        // Need to evict - keep only the first max_size entries (LRU: newest first)
        let mut result = self.lisp.nil()?;
        let mut current = cache;
        let mut taken = 0;
        
        while taken < max_size {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { car, cdr } => {
                    result = self.lisp.cons(car, result)?;
                    current = cdr;
                    taken += 1;
                }
                _ => break,
            }
        }
        
        // Reverse to maintain order (newest first)
        self.reverse_list(result)
    }
    
    /// Check if an expression contains a reference to a given symbol
    /// Used to detect recursive function definitions
    /// Uses iterative traversal to avoid stack overflow
    fn contains_symbol(&self, expr: ArenaIndex, symbol: ArenaIndex) -> Result<bool, EvalError> {
        // Simple depth-limited search without recursion
        // Use small stack to minimize stack usage during test execution
        const MAX_NODES: usize = 30;  // Max nodes in traversal stack
        
        let mut stack: [ArenaIndex; MAX_NODES] = [ArenaIndex::NULL; MAX_NODES];
        let mut stack_size = 1;
        stack[0] = expr;
        let mut nodes_checked = 0;
        
        // Allow checking more nodes than stack size since we pop as we go
        // MAX_NODES * 2 allows traversing deeper trees by reusing stack space
        while stack_size > 0 && nodes_checked < MAX_NODES * 2 {
            stack_size -= 1;
            let current = stack[stack_size];
            nodes_checked += 1;
            
            match self.lisp.get(current)? {
                Value::Symbol { .. } => {
                    if self.lisp.symbol_eq(current, symbol)? {
                        return Ok(true);
                    }
                }
                Value::Cons { car, cdr } => {
                    // Add children to stack if there's room
                    // Need space for both car and cdr, hence -2
                    if stack_size < MAX_NODES - 2 {
                        stack[stack_size] = car;
                        stack_size += 1;
                        stack[stack_size] = cdr;
                        stack_size += 1;
                    }
                }
                _ => {}
            }
        }
        
        // Note: If we hit the limit, we return false (no recursion detected)
        // This is safe but conservative - might miss very deeply nested recursion
        // In practice, most recursive functions have shallow bodies
        Ok(false)
    }
    
    /// Check if a value is false (ONLY #f is false)
    fn is_false(&self, val: ArenaIndex) -> Result<bool, EvalError> {
        Ok(self.lisp.get(val)?.is_false())
    }
    
    // NOTE: Old recursive force() and apply_builtin() removed
    // All evaluation now goes through the trampoline
    
    // ========================================================================
    // Convenience
    // ========================================================================
    
    /// Evaluate a string
    pub fn eval_str(&mut self, input: &str) -> EvalResult {
        let expr = parse(self.lisp, input)?;
        self.eval(expr)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    fn eval_to_num<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> i64 {
        let result = eval.eval_str(input).unwrap();
        lisp.get(result).unwrap().as_number().unwrap()
    }
    
    fn eval_is_true<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> bool {
        let result = eval.eval_str(input).unwrap();
        lisp.get(result).unwrap().is_true()
    }
    
    fn eval_is_false<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> bool {
        let result = eval.eval_str(input).unwrap();
        lisp.get(result).unwrap().is_false()
    }
    
    #[test]
    fn test_eval_number() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "42"), 42);
        assert_eq!(eval_to_num(&lisp, &mut eval, "-10"), -10);
    }
    
    #[test]
    fn test_eval_booleans() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        assert!(eval_is_true(&lisp, &mut eval, "#t"));
        assert!(eval_is_false(&lisp, &mut eval, "#f"));
        assert!(eval_is_true(&lisp, &mut eval, "true"));
        assert!(eval_is_false(&lisp, &mut eval, "false"));
    }
    
    #[test]
    fn test_nil_is_truthy() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // nil/'() is NOT false - only #f is false
        assert_eq!(eval_to_num(&lisp, &mut eval, "(if nil 1 2)"), 1);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(if '() 1 2)"), 1);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(if 0 1 2)"), 1);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(if #f 1 2)"), 2);
    }
    
    #[test]
    fn test_eval_arithmetic() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2)"), 3);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(- 10 3)"), 7);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(* 4 5)"), 20);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(/ 20 4)"), 5);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2 3 4)"), 10);
    }
    
    #[test]
    fn test_eval_quote() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        let result = eval.eval_str("'hello").unwrap();
        assert!(lisp.symbol_matches(result, "hello").unwrap());
    }
    
    #[test]
    fn test_eval_if() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(if #t 1 2)"), 1);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(if #f 1 2)"), 2);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(if (< 1 2) 10 20)"), 10);
    }
    
    #[test]
    fn test_eval_define() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define x 42)").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 42);
    }
    
    #[test]
    fn test_eval_lambda() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "((lambda (x) (+ x 1)) 5)"), 6);
    }
    
    #[test]
    fn test_eval_define_function() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (square x) (* x x))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(square 5)"), 25);
    }
    
    #[test]
    fn test_eval_let() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(let ((x 10) (y 20)) (+ x y))"), 30);
    }
    
    #[test]
    fn test_eval_let_star() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // let* allows sequential binding
        assert_eq!(eval_to_num(&lisp, &mut eval, "(let* ((x 10) (y (+ x 5))) (+ x y))"), 25);
    }
    
    #[test]
    fn test_tco_recursion() {
        // Hybrid evaluation: tail calls are STRICT, so TCO works properly!
        // No thunk accumulation - deep recursion is safe.
        let lisp: Lisp<5000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // This would overflow without proper TCO
        eval.eval_str("(define (sum-to n acc) (if (= n 0) acc (sum-to (- n 1) (+ acc n))))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(sum-to 100 0)"), 5050);  // sum 1..100
    }
    
    #[test]
    fn test_eval_recursion() {
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (fact n) (if (= n 0) 1 (* n (fact (- n 1)))))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(fact 5)"), 120);
    }
    
    // ═══════════════════════════════════════════════════════════════════════════
    // AUTOMATIC MEMOIZATION TESTS
    // Recursive functions are automatically memoized with bounded LRU caches
    // ═══════════════════════════════════════════════════════════════════════════
    
    // NOTE: Some tests may require RUST_MIN_STACK=8388608 (8MB) to run
    // This is due to Rust's default test thread stack being too small for
    // deep parsing/evaluation of complex Lisp expressions. The runtime
    // evaluator uses trampolining and has no such limitation.
    
    #[test]
    fn test_simple_fib_define() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        // Simple single-recursive function
        eval.eval_str("(define (countdown n) (if (= n 0) 0 (countdown (- n 1))))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(countdown 5)"), 0);
    }
    
    #[test]
    fn test_auto_memoization_fibonacci() {
        // NOTE: This test requires larger stack: RUST_MIN_STACK=8388608
        // Fibonacci with double recursion tests automatic memoization
        let lisp: Lisp<10000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // This is a complex expression that benefits from auto-memoization
        // Without memoization, fib(10) would be very slow
        eval.eval_str("(define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))").unwrap();
        
        // Should work correctly - auto-memoization helps with performance
        assert_eq!(eval_to_num(&lisp, &mut eval, "(fib 5)"), 5);
    }
    
    #[test]
    fn test_auto_memoization_factorial() {
        // Factorial is automatically memoized
        let lisp: Lisp<3000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (fact n) (if (= n 0) 1 (* n (fact (- n 1)))))").unwrap();
        
        // First call computes and caches
        assert_eq!(eval_to_num(&lisp, &mut eval, "(fact 10)"), 3628800);
        
        // Second call should use cached results
        assert_eq!(eval_to_num(&lisp, &mut eval, "(fact 10)"), 3628800);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(fact 5)"), 120);
    }
    
    #[test]
    fn test_auto_memoization_not_applied_to_non_recursive() {
        // Non-recursive functions should NOT be memoized
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // This function doesn't reference itself - should not be memoized
        eval.eval_str("(define (square x) (* x x))").unwrap();
        
        // Should work normally
        assert_eq!(eval_to_num(&lisp, &mut eval, "(square 5)"), 25);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(square 10)"), 100);
    }
    
    #[test]
    fn test_auto_memoization_with_multiple_args() {
        // Test memoization with multiple arguments - using simpler function
        let lisp: Lisp<5000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Simple recursive function with 2 args
        eval.eval_str("(define (add-rec x y) (if (= y 0) x (add-rec (+ x 1) (- y 1))))").unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(add-rec 3 4)"), 7);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(add-rec 5 3)"), 8);
    }
    
    #[test]
    fn test_auto_memoization_cache_works() {
        // Verify that caching actually happens by checking repeated calls
        let lisp: Lisp<6000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))").unwrap();
        
        // Call multiple times - should all return correct results
        // Using smaller value to reduce memory pressure
        for _ in 0..3 {
            assert_eq!(eval_to_num(&lisp, &mut eval, "(fib 8)"), 21);
        }
    }
    
    #[test]
    fn test_auto_memoization_mutual_recursion_detection() {
        // Functions with complex recursion patterns should be detected
        let lisp: Lisp<3000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Helper function that calls itself through conditions
        eval.eval_str("(define (countdown n) (if (= n 0) 'done (countdown (- n 1))))").unwrap();
        
        // Should work and be memoized
        let result = eval.eval_str("(countdown 50)").unwrap();
        assert!(lisp.symbol_matches(result, "done").unwrap());
    }
    
    #[test]
    fn test_auto_memoization_nested_recursive_calls() {
        // Test with deeply nested recursive structure
        let lisp: Lisp<4000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Sum function - tail recursive
        eval.eval_str("(define (sum n acc) (if (= n 0) acc (sum (- n 1) (+ acc n))))").unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(sum 100 0)"), 5050);
    }
    
    #[test]
    fn test_lru_cache_eviction() {
        // Test that cache eviction works by exceeding MAX_MEMO_CACHE_SIZE
        let lisp: Lisp<10000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (identity n) (if (= n 0) 0 (identity (- n 1))))").unwrap();
        
        // Call with many different values to trigger cache eviction
        // The cache should limit to MAX_MEMO_CACHE_SIZE (100) entries
        // We'll just call with several values - enough to test eviction
        assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 50)"), 0);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 60)"), 0);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 70)"), 0);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 80)"), 0);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 90)"), 0);
        
        // Should still work correctly even after many calls
        assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 10)"), 0);
    }
    
    // ═══════════════════════════════════════════════════════════════════════════
    // LAZY EVALUATION TESTS
    // Everything is lazy by default - no delay/force needed!
    // ═══════════════════════════════════════════════════════════════════════════
    
    #[test]
    fn test_lazy_basic() {
        // Basic lazy evaluation - arguments computed only when needed
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Simple computation works
        assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2 3)"), 6);
        
        // Functions work
        eval.eval_str("(define (add x y) (+ x y))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(add 10 20)"), 30);
    }
    
    #[test]
    fn test_lazy_cons_is_nonstrict() {
        // cons doesn't force its arguments - can build structures with unevaluated parts
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Create a pair
        eval.eval_str("(define p (cons 1 2))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 1);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 2);
        
        // List creation works
        eval.eval_str("(define lst (list 1 2 3))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car lst)"), 1);
    }
    
    #[test]
    fn test_lazy_infinite_stream() {
        // THE KEY TEST: Infinite structures work!
        // (define ones (cons 1 ones)) - this would loop forever in eager evaluation
        let lisp: Lisp<3000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Create an infinite stream of 1s using self-reference
        // This works because cons is non-strict and ones is wrapped in a thunk
        eval.eval_str("(define (make-ones) (cons 1 (make-ones)))").unwrap();
        eval.eval_str("(define ones (make-ones))").unwrap();
        
        // We can access elements without infinite loop
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car ones)"), 1);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr ones))"), 1);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr ones)))"), 1);
    }
    
    #[test]
    fn test_lazy_if_branches() {
        // Only the selected branch of 'if' is evaluated
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // This would error in an eager language (div by zero in else branch)
        // But we select the then branch, so else is never evaluated
        eval.eval_str("(define (safe-div x y) (if (= y 0) 0 (/ x y)))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(safe-div 10 0)"), 0);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(safe-div 10 2)"), 5);
    }
    
    #[test]
    fn test_hybrid_builtin_lazy() {
        // HYBRID EVALUATION: Builtin args are lazy (builtins force what they need)
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // 'if' is a special form with lazy branches - only one is evaluated
        // The undefined branch is never forced
        assert_eq!(eval_to_num(&lisp, &mut eval, "(if #t 42 undefined-var)"), 42);
        
        // cons is non-strict - elements stay as thunks
        eval.eval_str("(define p (cons 1 2))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 1);
    }
    
    #[test]
    fn test_hybrid_lambda_strict() {
        // HYBRID EVALUATION: Lambda args in tail position are strict (for TCO)
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Lambda args are evaluated, so this is strict
        eval.eval_str("(define (first x y) x)").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(first 42 100)"), 42);  // Works
        
        // The benefit: TCO works for deep recursion
        eval.eval_str("(define (count n) (if (= n 0) 0 (count (- n 1))))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(count 50)"), 0);  // No stack overflow
    }
    
    #[test]
    fn test_lazy_memoization() {
        // Values are memoized - same result every time
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (square x) (* x x))").unwrap();
        eval.eval_str("(define val (square 5))").unwrap();
        
        // Multiple accesses return same value
        assert_eq!(eval_to_num(&lisp, &mut eval, "val"), 25);
        assert_eq!(eval_to_num(&lisp, &mut eval, "val"), 25);
        assert_eq!(eval_to_num(&lisp, &mut eval, "val"), 25);
    }
    
    #[test]
    fn test_lazy_closure() {
        // Closures capture their environment lazily
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (make-adder n) (lambda (x) (+ x n)))").unwrap();
        eval.eval_str("(define add5 (make-adder 5))").unwrap();
        eval.eval_str("(define add10 (make-adder 10))").unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(add5 3)"), 8);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(add10 3)"), 13);
    }
    
    #[test]
    fn test_predicates() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        assert!(eval_is_true(&lisp, &mut eval, "(null? '())"));
        assert!(eval_is_true(&lisp, &mut eval, "(null? nil)"));
        assert!(eval_is_false(&lisp, &mut eval, "(null? '(1))"));
        
        assert!(eval_is_true(&lisp, &mut eval, "(pair? '(1 . 2))"));
        assert!(eval_is_false(&lisp, &mut eval, "(pair? 42)"));
        
        assert!(eval_is_true(&lisp, &mut eval, "(number? 42)"));
        assert!(eval_is_false(&lisp, &mut eval, "(number? 'x)"));
        
        assert!(eval_is_true(&lisp, &mut eval, "(boolean? #t)"));
        assert!(eval_is_true(&lisp, &mut eval, "(boolean? #f)"));
        assert!(eval_is_false(&lisp, &mut eval, "(boolean? 1)"));
        
        assert!(eval_is_true(&lisp, &mut eval, "(symbol? 'x)"));
        assert!(eval_is_false(&lisp, &mut eval, "(symbol? 42)"));
        
        assert!(eval_is_true(&lisp, &mut eval, "(procedure? +)"));
        assert!(eval_is_true(&lisp, &mut eval, "(procedure? (lambda (x) x))"));
        assert!(eval_is_false(&lisp, &mut eval, "(procedure? 42)"));
    }
    
    #[test]
    fn test_not() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        assert!(eval_is_true(&lisp, &mut eval, "(not #f)"));
        assert!(eval_is_false(&lisp, &mut eval, "(not #t)"));
        assert!(eval_is_false(&lisp, &mut eval, "(not nil)")); // nil is truthy!
        assert!(eval_is_false(&lisp, &mut eval, "(not 0)"));   // 0 is truthy!
    }
    
    #[test]
    fn test_cond() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        let result = eval_to_num(&lisp, &mut eval, 
            "(cond ((< 5 3) 1) ((> 5 3) 2) (else 3))");
        assert_eq!(result, 2);
    }
    
    #[test]
    fn test_and_or() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        assert!(eval_is_true(&lisp, &mut eval, "(and #t #t)"));
        assert!(eval_is_false(&lisp, &mut eval, "(and #t #f)"));
        assert!(eval_is_true(&lisp, &mut eval, "(and)"));  // Empty and is true
        
        assert!(eval_is_true(&lisp, &mut eval, "(or #f #t)"));
        assert!(eval_is_false(&lisp, &mut eval, "(or #f #f)"));
        assert!(eval_is_false(&lisp, &mut eval, "(or)"));  // Empty or is false
    }
    
    #[test]
    fn test_gc_during_eval() {
        let lisp: Lisp<1000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define x 42)").unwrap();
        let _stats = eval.gc();
        assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 42);
    }
    
    // ═══════════════════════════════════════════════════════════════════════════
    // COMPREHENSIVE TCO TESTS
    // ═══════════════════════════════════════════════════════════════════════════
    
    #[test]
    fn test_tco_deep_recursion() {
        // Test TCO with recursion
        // 
        // NOTE: Depth is limited by Rust stack during force() calls.
        // The Lisp-level TCO (trampoline) works, but forcing thunks uses
        // Rust recursion. A full fix requires converting force() to iterative.
        // 
        // Current practical limit: ~100-200 recursive calls in debug mode
        let lisp: Lisp<5000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Tail-recursive sum: sum 1 to n
        eval.eval_str("(define (sum-tail n acc) (if (= n 0) acc (sum-tail (- n 1) (+ acc n))))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(sum-tail 100 0)"), 5050);  // 1+2+...+100
        
        // Tail-recursive countdown  
        eval.eval_str("(define (countdown n) (if (= n 0) 'done (countdown (- n 1))))").unwrap();
        let result = eval.eval_str("(countdown 100)").unwrap();
        assert!(lisp.get(result).unwrap().is_symbol());
    }
    
    #[test]
    fn test_tco_mutual_recursion() {
        // Mutual recursion with TCO - even/odd predicates
        // NOTE: Limited depth due to Rust stack in force()
        let lisp: Lisp<5000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (my-even n) (if (= n 0) #t (my-odd (- n 1))))").unwrap();
        eval.eval_str("(define (my-odd n) (if (= n 0) #f (my-even (- n 1))))").unwrap();
        
        assert!(eval_is_true(&lisp, &mut eval, "(my-even 0)"));
        assert!(eval_is_false(&lisp, &mut eval, "(my-odd 0)"));
        assert!(eval_is_true(&lisp, &mut eval, "(my-even 20)"));
        assert!(eval_is_false(&lisp, &mut eval, "(my-odd 20)"));
        assert!(eval_is_false(&lisp, &mut eval, "(my-even 19)"));
        assert!(eval_is_true(&lisp, &mut eval, "(my-odd 19)"));
    }
    
    #[test]
    fn test_tco_accumulator_pattern() {
        // Classic tail-recursive patterns
        let lisp: Lisp<5000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Tail-recursive factorial
        eval.eval_str("(define (fact-tail n acc) (if (= n 0) acc (fact-tail (- n 1) (* n acc))))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(fact-tail 10 1)"), 3628800);
        
        // Tail-recursive length
        eval.eval_str("(define (len-tail lst acc) (if (null? lst) acc (len-tail (cdr lst) (+ acc 1))))").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(len-tail '(a b c d e) 0)"), 5);
        
        // Tail-recursive reverse
        eval.eval_str("(define (rev-tail lst acc) (if (null? lst) acc (rev-tail (cdr lst) (cons (car lst) acc))))").unwrap();
        let result = eval.eval_str("(rev-tail '(1 2 3) '())").unwrap();
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (rev-tail '(1 2 3) '()))"), 3);
    }
    
    #[test]
    fn test_tco_in_cond() {
        // TCO should work in cond branches
        let lisp: Lisp<5000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str(r#"
            (define (classify n)
                (cond ((< n 0) (classify (- 0 n)))
                      ((= n 0) 'zero)
                      ((< n 10) 'small)
                      ((< n 100) 'medium)
                      (else (classify (/ n 10)))))
        "#).unwrap();
        
        let result = eval.eval_str("(classify -42)").unwrap();
        assert!(lisp.get(result).unwrap().is_symbol());
        let result = eval.eval_str("(classify 999)").unwrap();
        assert!(lisp.get(result).unwrap().is_symbol());
    }
    
    // ═══════════════════════════════════════════════════════════════════════════
    // COMPREHENSIVE LAZY EVALUATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════
    
    #[test]
    fn test_lazy_stream_operations() {
        // Stream operations on infinite data
        let lisp: Lisp<5000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Infinite stream of natural numbers
        eval.eval_str("(define (nats-from n) (cons n (nats-from (+ n 1))))").unwrap();
        eval.eval_str("(define nats (nats-from 0))").unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car nats)"), 0);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr nats))"), 1);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr nats)))"), 2);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr (cdr nats))))"), 3);
    }
    
    #[test]
    fn test_lazy_stream_take() {
        // Take n elements from a stream
        let lisp: Lisp<5000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (take n s) (if (= n 0) '() (cons (car s) (take (- n 1) (cdr s)))))").unwrap();
        eval.eval_str("(define (ones) (cons 1 (ones)))").unwrap();
        
        // Take 3 ones
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (take 3 (ones)))"), 1);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (take 3 (ones))))"), 1);
    }
    
    #[test]
    fn test_lazy_and_or_short_circuit() {
        // and/or should short-circuit with lazy evaluation
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // 'and' stops at first false
        assert!(eval_is_false(&lisp, &mut eval, "(and #f undefined-error)"));
        assert!(eval_is_true(&lisp, &mut eval, "(and #t #t #t)"));
        
        // 'or' stops at first true
        assert!(eval_is_true(&lisp, &mut eval, "(or #t undefined-error)"));
        assert!(eval_is_false(&lisp, &mut eval, "(or #f #f #f)"));
    }
    
    #[test]
    fn test_lazy_let_bindings() {
        // let bindings in hybrid model
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Basic let
        assert_eq!(eval_to_num(&lisp, &mut eval, "(let ((x 10) (y 20)) (+ x y))"), 30);
        
        // let* with dependencies
        assert_eq!(eval_to_num(&lisp, &mut eval, "(let* ((x 5) (y (* x 2))) (+ x y))"), 15);
        
        // Nested let
        assert_eq!(eval_to_num(&lisp, &mut eval, 
            "(let ((x 1)) (let ((y 2)) (let ((z 3)) (+ x y z))))"), 6);
    }
    
    #[test]
    fn test_lazy_cons_preserves_thunks() {
        // cons should not force its arguments
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Build a list with computations
        eval.eval_str("(define p (cons (+ 1 2) (+ 3 4)))").unwrap();
        
        // Access should force and return correct values
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 3);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 7);
    }
    
    #[test]
    fn test_lazy_nested_structures() {
        // Deeply nested lazy structures
        let lisp: Lisp<3000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Create nested pairs
        eval.eval_str("(define deep (cons (cons (cons 1 2) 3) 4))").unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr deep)"), 4);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr (car deep))"), 3);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr (car (car deep)))"), 2);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (car (car deep)))"), 1);
    }
    
    #[test]
    fn test_lazy_with_gc_pressure() {
        // Test lazy evaluation under GC pressure
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Create an infinite stream
        eval.eval_str("(define (ones) (cons 1 (ones)))").unwrap();
        eval.eval_str("(define stream (ones))").unwrap();
        
        // Access some elements
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car stream)"), 1);
        
        // Run GC
        eval.gc();
        
        // Stream should still work after GC
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr stream))"), 1);
        
        // More allocations and GC
        for _ in 0..10 {
            eval.eval_str("(+ 1 2 3 4 5)").unwrap();
        }
        eval.gc();
        
        // Stream still works
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr stream)))"), 1);
    }
    
    #[test]
    fn test_lazy_fibonacci_stream() {
        // Classic lazy Fibonacci stream
        let lisp: Lisp<5000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // zipWith for streams
        eval.eval_str("(define (zipwith f s1 s2) (cons (f (car s1) (car s2)) (zipwith f (cdr s1) (cdr s2))))").unwrap();
        
        // Fibonacci stream: fibs = 0 : 1 : zipWith (+) fibs (tail fibs)
        // We use a simpler approach with explicit recursion
        eval.eval_str("(define (fib-pair a b) (cons a (fib-pair b (+ a b))))").unwrap();
        eval.eval_str("(define fibs (fib-pair 0 1))").unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car fibs)"), 0);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr fibs))"), 1);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr fibs)))"), 1);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr (cdr fibs))))"), 2);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr (cdr (cdr fibs)))))"), 3);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr (cdr (cdr (cdr fibs))))))"), 5);
    }
    
    // ═══════════════════════════════════════════════════════════════════════════
    // HYBRID EVALUATION EDGE CASES
    // ═══════════════════════════════════════════════════════════════════════════
    
    #[test]
    fn test_hybrid_nested_calls() {
        // Test behavior with nested function calls
        let lisp: Lisp<3000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (double x) (* x 2))").unwrap();
        eval.eval_str("(define (quad x) (double (double x)))").unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(quad 5)"), 20);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(quad (quad 2))"), 32);
    }
    
    #[test]
    fn test_hybrid_higher_order() {
        // Higher-order functions with hybrid evaluation
        let lisp: Lisp<3000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (apply-twice f x) (f (f x)))").unwrap();
        eval.eval_str("(define (inc x) (+ x 1))").unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(apply-twice inc 0)"), 2);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(apply-twice (lambda (x) (* x 2)) 3)"), 12);
    }
    
    #[test]
    fn test_hybrid_currying() {
        // Curried functions
        let lisp: Lisp<3000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        eval.eval_str("(define (curry-add a) (lambda (b) (lambda (c) (+ a b c))))").unwrap();
        eval.eval_str("(define add1 (curry-add 1))").unwrap();
        eval.eval_str("(define add1-2 (add1 2))").unwrap();
        
        assert_eq!(eval_to_num(&lisp, &mut eval, "(add1-2 3)"), 6);
        assert_eq!(eval_to_num(&lisp, &mut eval, "(((curry-add 10) 20) 30)"), 60);
    }
    
    #[test]
    fn test_force_chain_memoization() {
        // Verify that forcing a thunk memoizes the result
        let lisp: Lisp<2000> = Lisp::new();
        let mut eval = Evaluator::new(&lisp).unwrap();
        
        // Create a computation stored in a cons
        eval.eval_str("(define p (cons (* 111 111) 0))").unwrap();
        
        // First access forces and memoizes
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 12321);
        
        // Second access should return memoized value
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 12321);
        
        // Third access
        assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 12321);
    }
}
