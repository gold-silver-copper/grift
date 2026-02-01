//! Main evaluator implementation.

use grift_parser::{
    ArenaIndex, GcStats, Lisp, Value, Builtin, StdLib, parse, ParseError, ParseErrorKind,
};

use crate::error::{
    ErrorKind, ErrorMessage, StackFrame, EvalError, EvalResult,
    MAX_STACK_DEPTH, MAX_BACKTRACE,
};
use crate::continuation::{Cont, TrampolineState, is_binary_builtin, MAX_CONT_DEPTH, MAX_DATA_STACK};
use crate::helpers::{
    gcd_helper, int_pow, equal_recursive, case_matches,
};
use crate::native::{NativeRegistry, NativeFn, simple_hash};

// Re-export macros from lib.rs (they're defined there)
use crate::{
    extract_args, builtin_unary_pred, builtin_numeric_pred, builtin_int_identity, builtin_div_op,
    define_cont_pack_unpack, define_cont_pack_unpack_builtin_first, define_cont_pack_unpack_with_usize,
};

// ============================================================================
// Evaluator
// ============================================================================

/// The Lisp evaluator with full trampolined TCO.
///
/// This evaluator uses continuation-passing style with an explicit stack,
/// enabling unlimited recursion depth without Rust stack overflow.
pub struct Evaluator<'a, const N: usize> {
    pub(crate) lisp: &'a Lisp<N>,
    /// Global environment
    global_env: ArenaIndex,
    /// Call stack for error reporting
    call_stack: [StackFrame; MAX_STACK_DEPTH],
    call_stack_depth: usize,
    /// Continuation stack for full trampolining
    cont_stack: [Cont; MAX_CONT_DEPTH],
    cont_depth: usize,
    /// Data stack for continuation data (separate from arena for performance)
    /// Stores raw ArenaIndex values without Value::Ref wrapper
    data_stack: [ArenaIndex; MAX_DATA_STACK],
    data_stack_top: usize,
    /// Native function registry
    native_registry: NativeRegistry<N>,
}

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Create a new evaluator with standard environment
    pub fn new(lisp: &'a Lisp<N>) -> Result<Self, EvalError> {
        let mut eval = Evaluator {
            lisp,
            global_env: ArenaIndex::NIL,
            call_stack: [StackFrame::default(); MAX_STACK_DEPTH],
            call_stack_depth: 0,
            cont_stack: [Cont::Done; MAX_CONT_DEPTH],
            cont_depth: 0,
            data_stack: [ArenaIndex::NIL; MAX_DATA_STACK],
            data_stack_top: 0,
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
    /// use grift_eval::{Lisp, Evaluator, ArenaIndex, ArenaResult, FromLisp, ToLisp};
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
        let mut roots = [ArenaIndex::NIL; MAX_ROOTS];
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
        // Each variant stores its data as a single ArenaIndex to a cons-list,
        // which the GC will trace recursively
        for i in 0..self.cont_depth {
            if root_count >= MAX_ROOTS - 2 {
                break; // Leave some room
            }
            
            // Continuation data is now stored in a separate data_stack, not in the arena.
            // The data_stack stores raw ArenaIndex values that may reference arena objects.
            // We need to trace all ArenaIndex values in the data_stack up to current top.
            let cont = self.cont_stack[i];
            let data_len = cont.data_len();
            if data_len > 0 {
                // Get the data_start from the continuation
                let ds = match cont {
                    Cont::Done | Cont::QuasiquoteUnquoteWrap | Cont::QuasiquoteNestedWrap => 0,
                    Cont::ApplyForced(data_start) |
                    Cont::IfBranch(data_start) |
                    Cont::BuiltinForceArg(data_start) |
                    Cont::BinaryBuiltinFirst(data_start) |
                    Cont::BinaryBuiltinSecond(data_start) |
                    Cont::LambdaFirstBind(data_start) |
                    Cont::LambdaBindArg(data_start) |
                    Cont::LetBinding(data_start) |
                    Cont::LetStarBinding(data_start) |
                    Cont::LetrecInit(data_start) |
                    Cont::When(data_start) |
                    Cont::Unless(data_start) |
                    Cont::EvalExpr(data_start) |
                    Cont::CondTest(data_start) |
                    Cont::And(data_start) |
                    Cont::Or(data_start) |
                    Cont::BeginSeq(data_start) |
                    Cont::CaseKey(data_start) |
                    Cont::DoInit(data_start) |
                    Cont::DoTestResult(data_start) |
                    Cont::DoBody(data_start) |
                    Cont::DoStep(data_start) |
                    Cont::ApplyFirst(data_start) |
                    Cont::ApplySecond(data_start) |
                    Cont::ValuesCollect(data_start) |
                    Cont::DefineValue(data_start) |
                    Cont::SetValue(data_start) |
                    Cont::NativeArgsCollect(data_start) |
                    Cont::QuasiquoteCar(data_start) |
                    Cont::QuasiquoteCdr(data_start) |
                    Cont::QuasiquoteSplice(data_start) |
                    Cont::QuasiquoteSpliceAppend(data_start) => data_start,
                };
                // Add all ArenaIndex values from this continuation's data to roots
                for j in 0..data_len {
                    let idx = self.data_stack[ds + j];
                    // Skip encoded builtins/raw usize values (they have very high values)
                    // Real arena indices are < N
                    if idx.raw() < N {
                        roots[root_count] = idx;
                        root_count += 1;
                    }
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
    
    pub(crate) fn make_error(&self, kind: ErrorKind, expr: ArenaIndex) -> EvalError {
        EvalError::new(kind)
            .with_expr(expr)
            .with_backtrace(&self.call_stack, self.call_stack_depth)
    }
    
    pub(crate) fn type_error(&self, expr: ArenaIndex, expected: &'static str, got: &'static str) -> EvalError {
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
    pub(crate) fn env_extend(&self, env: ArenaIndex, name: ArenaIndex, value: ArenaIndex) -> EvalResult {
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
                    // With inline cons, we get car and cdr directly
                    if let Value::Cons { car: bound_name, cdr: bound_value } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        return Ok(bound_value);
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
                    // With inline cons, we get car and cdr directly
                    if let Value::Cons { car: bound_name, cdr: bound_value } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        return Ok(bound_value);
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
                    if let Value::Cons { car: bound_name, .. } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        // Found it - mutate the binding
                        self.lisp.set_cdr(car, value)?;
                        return Ok(value);
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
                    if let Value::Cons { car: bound_name, .. } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        // Found it - mutate the binding
                        self.lisp.set_cdr(car, value)?;
                        return Ok(value);
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
                    if let Value::Cons { car: bound_name, .. } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        // Update existing
                        self.lisp.set_cdr(car, value)?;
                        return Ok(value);
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
    /// Also restores the data stack by popping the continuation's data
    #[inline]
    fn pop_cont(&mut self) -> Cont {
        if self.cont_depth == 0 {
            Cont::Done
        } else {
            self.cont_depth -= 1;
            let cont = self.cont_stack[self.cont_depth];
            // Restore data stack - pop this continuation's data
            // data_start points to where this continuation's data begins
            // After unpack, data_stack_top should be restored to data_start
            // (This happens automatically since unpack reads but doesn't modify data_stack_top,
            // and the next pack will overwrite from the current data_stack_top)
            // Actually we need to restore here since unpack doesn't change data_stack_top
            let data_len = cont.data_len();
            if data_len > 0 {
                self.data_stack_top -= data_len;
            }
            cont
        }
    }
    
    
    /// Evaluate an expression (entry point)
    pub fn eval(&mut self, expr: ArenaIndex) -> EvalResult {
        // Reset continuation stack and data stack
        self.cont_depth = 0;
        self.data_stack_top = 0;
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
    
    /// The main trampoline loop - processes states and continuations
    /// This is the ONLY place where looping happens - no Rust recursion!
    ///
    /// GC Strategy: Aggressive periodic collection
    /// - Checks memory pressure every GC_CHECK_INTERVAL steps
    /// - Runs GC proactively when usage exceeds threshold
    /// - Also handles OOM reactively as a fallback
    fn trampoline(&mut self, mut state: TrampolineState) -> EvalResult {
        let mut step_count: usize = 0;
        const GC_CHECK_INTERVAL: usize = 1000;
        const GC_THRESHOLD_PERCENT: usize = 80;
        
        loop {
            // Aggressive periodic GC check
            step_count = step_count.wrapping_add(1);
            if step_count % GC_CHECK_INTERVAL == 0 {
                let stats = self.lisp.stats();
                if stats.allocated * 100 / stats.capacity >= GC_THRESHOLD_PERCENT {
                    self.gc_with_state(&state);
                }
            }
            
            state = match state {
                TrampolineState::Eval { expr, env } => {
                    match self.step_eval(expr, env) {
                        Ok(s) => s,
                        Err(e) if e.kind == ErrorKind::OutOfMemory => {
                            // Fallback: run GC and retry once
                            self.gc_with_state(&state);
                            self.step_eval(expr, env)?
                        }
                        Err(e) => return Err(e),
                    }
                }
                TrampolineState::Return { val } => {
                    match self.step_return(val) {
                        Ok(Some(new_state)) => new_state,
                        Ok(None) => return Ok(val),
                        Err(e) if e.kind == ErrorKind::OutOfMemory => {
                            // Fallback: run GC and retry once
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
            Value::Builtin(_) | Value::StdLib(_) | Value::Lambda { .. } |
            Value::Array { .. } | Value::String { .. } | Value::Native { .. } |
            Value::Ref(_) | Value::Usize(_) => {
                Ok(TrampolineState::Return { val: expr })
            }
            
            // Symbol - variable lookup
            Value::Symbol(_) => {
                let val = self.env_lookup(env, expr)?;
                Ok(TrampolineState::Return { val })
            }
            
            // List - special form or function application
            Value::Cons { .. } => {
                let car = self.lisp.car(expr)?;
                let cdr = self.lisp.cdr(expr)?;
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
        if let Value::Symbol(_) = head {
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
                let data_start = self.pack_if_branch(then_expr, else_expr, env)?;
                self.push_cont(Cont::IfBranch(data_start))?;
                
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
                return self.eval_define(cdr, env);
            }
            
            // set! - mutate variable binding
            if self.lisp.symbol_matches(car, "set!")? {
                return self.eval_set(cdr, env);
            }
            
            // let - continuation-based evaluation
            if self.lisp.symbol_matches(car, "let")? {
                return self.step_eval_let(cdr, env);
            }

            // let* - continuation-based evaluation
            if self.lisp.symbol_matches(car, "let*")? {
                return self.step_eval_let_star(cdr, env);
            }

            // letrec - continuation-based evaluation (R7RS Section 4.2.2)
            if self.lisp.symbol_matches(car, "letrec")? {
                return self.step_eval_letrec(cdr, env);
            }

            // letrec* - continuation-based evaluation (R7RS Section 4.2.2)
            if self.lisp.symbol_matches(car, "letrec*")? {
                return self.step_eval_letrec(cdr, env); // Same as letrec for now
            }

            // when - continuation-based evaluation (R7RS Section 4.2.1)
            if self.lisp.symbol_matches(car, "when")? {
                let test_expr = self.lisp.car(cdr)?;
                let body = self.lisp.cdr(cdr)?;
                let data_start = self.pack_when(body, env)?;
                self.push_cont(Cont::When(data_start))?;
                return Ok(TrampolineState::Eval { expr: test_expr, env });
            }

            // unless - continuation-based evaluation (R7RS Section 4.2.1)
            if self.lisp.symbol_matches(car, "unless")? {
                let test_expr = self.lisp.car(cdr)?;
                let body = self.lisp.cdr(cdr)?;
                let data_start = self.pack_unless(body, env)?;
                self.push_cont(Cont::Unless(data_start))?;
                return Ok(TrampolineState::Eval { expr: test_expr, env });
            }

            // begin - continuation-based evaluation
            if self.lisp.symbol_matches(car, "begin")? {
                return self.step_eval_begin(cdr, env);
            }

            // and - continuation-based short circuit
            if self.lisp.symbol_matches(car, "and")? {
                return self.step_eval_and(cdr, env);
            }

            // or - continuation-based short circuit
            if self.lisp.symbol_matches(car, "or")? {
                return self.step_eval_or(cdr, env);
            }
            
            // case - pattern matching
            if self.lisp.symbol_matches(car, "case")? {
                return self.step_eval_case(cdr, env);
            }
            
            // do - iteration construct
            if self.lisp.symbol_matches(car, "do")? {
                return self.step_eval_do(cdr, env);
            }
            
            // quasiquote - template with unquote (trampolined)
            if self.lisp.symbol_matches(car, "quasiquote")? {
                return self.eval_quasiquote(self.lisp.car(cdr)?, env);
            }
            
            // eval - continuation-based evaluation at runtime
            if self.lisp.symbol_matches(car, "eval")? {
                let expr_to_eval = self.lisp.car(cdr)?;
                // Push continuation to evaluate the result in global environment
                let data_start = self.pack_eval_expr(self.global_env)?;
                self.push_cont(Cont::EvalExpr(data_start))?;
                // First evaluate the expression to get the code to eval
                return Ok(TrampolineState::Eval { expr: expr_to_eval, env });
            }
            
            // apply - apply function to list of arguments
            if self.lisp.symbol_matches(car, "apply")? {
                return self.step_eval_apply(cdr, env);
            }
            
            // values - return multiple values (as a special list)
            if self.lisp.symbol_matches(car, "values")? {
                return self.eval_values(cdr, env);
            }
        }
        
        // Function application - HYBRID EVALUATION
        self.push_frame(expr, car)?;
        
        // Push continuation: after evaluating func, apply it
        let data_start = self.pack_apply_forced(cdr, env, expr)?;
        self.push_cont(Cont::ApplyForced(data_start))?;
        
        // Evaluate the function expression
        Ok(TrampolineState::Eval { expr: car, env })
    }
    
    /// Evaluate cond using continuations
    fn step_eval_cond(&mut self, clauses: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        if self.lisp.get(clauses)?.is_nil() {
            // No clauses - return unspecified (nil)
            let nil = self.lisp.nil()?;
            return Ok(TrampolineState::Return { val: nil });
        }

        let clause = self.lisp.car(clauses)?;
        let rest_clauses = self.lisp.cdr(clauses)?;
        let test = self.lisp.car(clause)?;
        let then_exprs = self.lisp.cdr(clause)?;

        // Check for else clause
        if self.lisp.symbol_matches(test, "else")? {
            if self.lisp.get(then_exprs)?.is_nil() {
                let nil = self.lisp.nil()?;
                return Ok(TrampolineState::Return { val: nil });
            }
            let begin = self.lisp.symbol("begin")?;
            let new_expr = self.lisp.cons(begin, then_exprs)?;
            return Ok(TrampolineState::Eval { expr: new_expr, env });
        }

        // Push continuation for after evaluating test
        let data_start = self.pack_cond_test(then_exprs, rest_clauses, env)?;
        self.push_cont(Cont::CondTest(data_start))?;

        // Evaluate the test
        Ok(TrampolineState::Eval { expr: test, env })
    }
    
    /// Process a return value with the current continuation
    fn step_return(&mut self, val: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
        let cont = self.pop_cont();
        
        match cont {
            Cont::Done => {
                // No more continuations - we're done
                Ok(None)
            }
            
            Cont::IfBranch(data_start) => {
                let (then_expr, else_expr, env) = self.unpack_if_branch(data_start);
                // val is the evaluated condition
                let branch = if !self.is_false(val)? { then_expr } else { else_expr };
                if branch.is_nil() {
                    let nil = self.lisp.nil()?;
                    Ok(Some(TrampolineState::Return { val: nil }))
                } else {
                    Ok(Some(TrampolineState::Eval { expr: branch, env }))
                }
            }
            
            Cont::ApplyForced(data_start) => {
                let (args_expr, env, call_expr) = self.unpack_apply_forced(data_start);
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
                            let data_start = self.pack_lambda_bind_arg(
                                rest_exprs, env, rest_params, body, closure_env, call_expr
                            )?;
                            self.push_cont(Cont::LambdaBindArg(data_start))?;
                            // Push binding continuation for first param
                            let data_start = self.pack_lambda_first_bind(first_param)?;
                            self.push_cont(Cont::LambdaFirstBind(data_start))?;
                            
                            Ok(Some(TrampolineState::Eval { expr: first_expr, env }))
                        }
                    }
                    Value::StdLib(s) => {
                        // StdLib: Parse body and params on each call
                        self.pop_frame();
                        
                        // Parse body and create param list
                        let body = parse(self.lisp, s.body())
                            .map_err(|e| self.parse_error_to_eval(e, call_expr, s.name()))?;
                        let params = self.make_stdlib_param_list(s.params())?;
                        
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
                            let data_start = self.pack_lambda_bind_arg(
                                rest_exprs, env, rest_params, body, closure_env, call_expr
                            )?;
                            self.push_cont(Cont::LambdaBindArg(data_start))?;
                            // Push binding continuation for first param
                            let data_start = self.pack_lambda_first_bind(first_param)?;
                            self.push_cont(Cont::LambdaFirstBind(data_start))?;
                            
                            Ok(Some(TrampolineState::Eval { expr: first_expr, env }))
                        }
                    }
                    Value::Native { .. } => {
                        // Native (Rust) function: STRICT - evaluate args and pass to Rust fn
                        self.pop_frame();
                        let id = self.lisp.native_id(val)?;
                        
                        // If no args, call directly
                        if self.lisp.get(args_expr)?.is_nil() {
                            if let Some(native_fn) = self.native_registry.lookup_by_id(id) {
                                let nil = self.lisp.nil()?;
                                let result = native_fn(self.lisp, nil)?;
                                Ok(Some(TrampolineState::Return { val: result }))
                            } else {
                                Err(self.make_error(ErrorKind::NotAFunction, call_expr)
                                    .with_message("native function not found"))
                            }
                        } else {
                            // Evaluate arguments using continuation
                            let first_expr = self.lisp.car(args_expr)?;
                            let rest = self.lisp.cdr(args_expr)?;
                            let nil = self.lisp.nil()?;
                            
                            let data_start = self.pack_native_args_collect(rest, nil, id, env)?;
                            self.push_cont(Cont::NativeArgsCollect(data_start))?;
                            Ok(Some(TrampolineState::Eval { expr: first_expr, env }))
                        }
                    }
                    _ => {
                        self.pop_frame();
                        Err(self.type_error(call_expr, "procedure", self.lisp.get(val)?.type_name()))
                    }
                }
            }
            
            Cont::LambdaFirstBind(data_start) => {
                let param = self.unpack_lambda_first_bind(data_start);
                // val is evaluated first arg - bind to param
                // Pop LambdaBindArg, extend env, push it back
                let cont = self.pop_cont();
                if let Cont::LambdaBindArg(data_start) = cont {
                    // Unpack the cons-list
                    let (remaining_exprs, eval_env, remaining_params, body, new_env, call_expr) = 
                        self.unpack_lambda_bind_arg(data_start);
                    
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
                        let new_data_start = self.pack_lambda_bind_arg(
                            rest_exprs, eval_env, rest_params, body, extended_env, call_expr
                        )?;
                        self.push_cont(Cont::LambdaBindArg(new_data_start))?;
                        let data_start = self.pack_lambda_first_bind(next_param)?;
                        self.push_cont(Cont::LambdaFirstBind(data_start))?;
                        
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
            
            Cont::BuiltinForceArg(data_start) => {
                let (builtin, remaining_args, collected, call_expr, eval_env) = self.unpack_builtin_force_arg(data_start);
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
                    
                    let data_start = self.pack_builtin_force_arg(builtin, rest_args, new_collected, call_expr, eval_env)?;
                    self.push_cont(Cont::BuiltinForceArg(data_start))?;
                    
                    Ok(Some(TrampolineState::Eval { expr: next_arg, env: eval_env }))
                }
            }
            
            Cont::BinaryBuiltinFirst(data_start) => {
                let (builtin, second_arg, call_expr, eval_env) = self.unpack_binary_builtin_first(data_start);
                // val is first evaluated arg - now evaluate second
                let data_start = self.pack_binary_builtin_second(builtin, val, call_expr)?;
                self.push_cont(Cont::BinaryBuiltinSecond(data_start))?;
                Ok(Some(TrampolineState::Eval { expr: second_arg, env: eval_env }))
            }
            
            Cont::BinaryBuiltinSecond(data_start) => {
                let (builtin, first_val, call_expr) = self.unpack_binary_builtin_second(data_start);
                // val is second evaluated arg - apply binary operation directly
                let result = self.apply_binary_builtin(builtin, first_val, val, call_expr)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            Cont::LetBinding(data_start) => {
                let (remaining_bindings, new_env, original_env, body, name) = self.unpack_let_binding(data_start);
                // val is the evaluated value for the current binding
                // Extend environment with the binding
                let extended_env = self.env_extend(new_env, name, val)?;

                // Check for more bindings
                if self.lisp.get(remaining_bindings)?.is_nil() {
                    // All bindings done - evaluate body in extended environment
                    Ok(Some(TrampolineState::Eval { expr: body, env: extended_env }))
                } else {
                    // More bindings - get next binding
                    let next_binding = self.lisp.car(remaining_bindings)?;
                    let rest_bindings = self.lisp.cdr(remaining_bindings)?;
                    let next_name = self.lisp.car(next_binding)?;
                    let next_value_expr = self.lisp.car(self.lisp.cdr(next_binding)?)?;

                    // Push continuation for after evaluating this binding
                    let data_start = self.pack_let_binding(rest_bindings, extended_env, original_env, body, next_name)?;
                    self.push_cont(Cont::LetBinding(data_start))?;

                    // Evaluate the value expression in the ORIGINAL environment
                    Ok(Some(TrampolineState::Eval { expr: next_value_expr, env: original_env }))
                }
            }

            Cont::LetStarBinding(data_start) => {
                let (remaining_bindings, new_env, body, name) = self.unpack_let_star_binding(data_start);
                // val is the evaluated value for the current binding
                // Extend environment with the binding
                let extended_env = self.env_extend(new_env, name, val)?;

                // Check for more bindings
                if self.lisp.get(remaining_bindings)?.is_nil() {
                    // All bindings done - evaluate body in extended environment
                    Ok(Some(TrampolineState::Eval { expr: body, env: extended_env }))
                } else {
                    // More bindings - get next binding
                    let next_binding = self.lisp.car(remaining_bindings)?;
                    let rest_bindings = self.lisp.cdr(remaining_bindings)?;
                    let next_name = self.lisp.car(next_binding)?;
                    let next_value_expr = self.lisp.car(self.lisp.cdr(next_binding)?)?;

                    // Push continuation for after evaluating this binding
                    let data_start = self.pack_let_star_binding(rest_bindings, extended_env, body, next_name)?;
                    self.push_cont(Cont::LetStarBinding(data_start))?;

                    // Evaluate the value expression in the NEW (extended) environment
                    Ok(Some(TrampolineState::Eval { expr: next_value_expr, env: extended_env }))
                }
            }

            Cont::LetrecInit(data_start) => {
                let (remaining_bindings, new_env, body, name) = self.unpack_letrec_init(data_start);
                // val is the evaluated init expression - set! the variable
                self.env_set(new_env, name, val)?;

                // Check for more bindings
                if self.lisp.get(remaining_bindings)?.is_nil() {
                    // All inits done - evaluate body
                    Ok(Some(TrampolineState::Eval { expr: body, env: new_env }))
                } else {
                    // More bindings - get next binding
                    let next_binding = self.lisp.car(remaining_bindings)?;
                    let rest_bindings = self.lisp.cdr(remaining_bindings)?;
                    let next_name = self.lisp.car(next_binding)?;
                    let next_init_expr = self.lisp.car(self.lisp.cdr(next_binding)?)?;

                    // Push continuation for after evaluating this init
                    let data_start = self.pack_letrec_init(rest_bindings, new_env, body, next_name)?;
                    self.push_cont(Cont::LetrecInit(data_start))?;

                    // Evaluate the init expression in the letrec environment
                    Ok(Some(TrampolineState::Eval { expr: next_init_expr, env: new_env }))
                }
            }

            Cont::When(data_start) => {
                let (body, env) = self.unpack_when(data_start);
                // val is the evaluated test result
                let test_passed = !self.is_false(val)?;

                if test_passed {
                    // Evaluate body as begin
                    let begin = self.lisp.symbol("begin")?;
                    let new_expr = self.lisp.cons(begin, body)?;
                    Ok(Some(TrampolineState::Eval { expr: new_expr, env }))
                } else {
                    // Return unspecified value (nil)
                    let nil = self.lisp.nil()?;
                    Ok(Some(TrampolineState::Return { val: nil }))
                }
            }

            Cont::Unless(data_start) => {
                let (body, env) = self.unpack_unless(data_start);
                // val is the evaluated test result
                let test_passed = !self.is_false(val)?;

                if !test_passed {
                    // Evaluate body as begin
                    let begin = self.lisp.symbol("begin")?;
                    let new_expr = self.lisp.cons(begin, body)?;
                    Ok(Some(TrampolineState::Eval { expr: new_expr, env }))
                } else {
                    // Return unspecified value (nil)
                    let nil = self.lisp.nil()?;
                    Ok(Some(TrampolineState::Return { val: nil }))
                }
            }

            Cont::EvalExpr(data_start) => {
                let env = self.unpack_eval_expr(data_start);
                // val is the evaluated expression - now evaluate it
                Ok(Some(TrampolineState::Eval { expr: val, env }))
            }

            Cont::CondTest(data_start) => {
                let (then_exprs, remaining_clauses, env) = self.unpack_cond_test(data_start);
                // val is the evaluated test
                if !self.is_false(val)? {
                    // Test passed - evaluate body expressions
                    if self.lisp.get(then_exprs)?.is_nil() {
                        // No body - return the test value itself (cond => behavior)
                        Ok(Some(TrampolineState::Return { val }))
                    } else {
                        // Evaluate body as begin
                        let begin = self.lisp.symbol("begin")?;
                        let new_expr = self.lisp.cons(begin, then_exprs)?;
                        Ok(Some(TrampolineState::Eval { expr: new_expr, env }))
                    }
                } else {
                    // Test failed - try remaining clauses
                    self.step_eval_cond_cont(remaining_clauses, env)
                }
            }

            Cont::And(data_start) => {
                let (remaining, env) = self.unpack_and(data_start);
                // val is the evaluated expression
                // and: if false, short-circuit and return #f
                if self.is_false(val)? {
                    let false_val = self.lisp.boolean(false)?;
                    Ok(Some(TrampolineState::Return { val: false_val }))
                } else if self.lisp.get(remaining)?.is_nil() {
                    // Last expression - return its value
                    Ok(Some(TrampolineState::Return { val }))
                } else {
                    // More expressions - evaluate next
                    let next_expr = self.lisp.car(remaining)?;
                    let rest = self.lisp.cdr(remaining)?;
                    let data_start = self.pack_and(rest, env)?;
                    self.push_cont(Cont::And(data_start))?;
                    Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                }
            }

            Cont::Or(data_start) => {
                let (remaining, env) = self.unpack_or(data_start);
                // val is the evaluated expression
                // or: if truthy, short-circuit and return the value
                if !self.is_false(val)? {
                    Ok(Some(TrampolineState::Return { val }))
                } else if self.lisp.get(remaining)?.is_nil() {
                    // Last expression was false - return #f
                    let false_val = self.lisp.boolean(false)?;
                    Ok(Some(TrampolineState::Return { val: false_val }))
                } else {
                    // More expressions - evaluate next
                    let next_expr = self.lisp.car(remaining)?;
                    let rest = self.lisp.cdr(remaining)?;
                    let data_start = self.pack_or(rest, env)?;
                    self.push_cont(Cont::Or(data_start))?;
                    Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                }
            }

            Cont::BeginSeq(data_start) => {
                let (remaining, env) = self.unpack_begin_seq(data_start);
                // val is the result of the previous expression (discarded unless last)
                if self.lisp.get(remaining)?.is_nil() {
                    // This was the last expression - return its value
                    Ok(Some(TrampolineState::Return { val }))
                } else {
                    // More expressions - evaluate next
                    let next_expr = self.lisp.car(remaining)?;
                    let rest = self.lisp.cdr(remaining)?;
                    if self.lisp.get(rest)?.is_nil() {
                        // Next is the last - just evaluate it (tail call)
                        Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                    } else {
                        // More after next - push continuation
                        let data_start = self.pack_begin_seq(rest, env)?;
                        self.push_cont(Cont::BeginSeq(data_start))?;
                        Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                    }
                }
            }

            // ================================================================
            // New continuation types for fully trampolined evaluation
            // ================================================================

            Cont::CaseKey(data_start) => {
                let (clauses, env) = self.unpack_case_key(data_start);
                // val is the evaluated key - check clauses
                self.step_return_case_key(val, clauses, env)
            }

            Cont::DoInit(data_start) => {
                let (remaining_bindings, var_steps, test_clause, body, loop_env, original_env, current_var) = self.unpack_do_init(data_start);
                // val is the evaluated init expression - bind and continue
                let extended_env = self.env_extend(loop_env, current_var, val)?;
                self.step_return_do_init(remaining_bindings, var_steps, test_clause, body, extended_env, original_env)
            }

            Cont::DoTestResult(data_start) => {
                let (var_steps, test_clause, body, loop_env) = self.unpack_do_test_result(data_start);
                // val is the evaluated test result
                if !self.is_false(val)? {
                    // Test passed - evaluate result expressions
                    let result_exprs = self.lisp.cdr(test_clause)?;
                    if self.lisp.get(result_exprs)?.is_nil() {
                        Ok(Some(TrampolineState::Return { val }))
                    } else {
                        let state = self.step_eval_begin(result_exprs, loop_env)?;
                        Ok(Some(state))
                    }
                } else {
                    // Test failed - evaluate body for side effects, then steps
                    if self.lisp.get(body)?.is_nil() {
                        // No body - go straight to steps
                        self.step_do_start_steps(var_steps, test_clause, body, loop_env)
                    } else {
                        // Evaluate body expressions
                        let first_body = self.lisp.car(body)?;
                        let rest_body = self.lisp.cdr(body)?;
                        let data_start = self.pack_do_body(rest_body, var_steps, test_clause, body, loop_env)?;
                        self.push_cont(Cont::DoBody(data_start))?;
                        Ok(Some(TrampolineState::Eval { expr: first_body, env: loop_env }))
                    }
                }
            }

            Cont::DoBody(data_start) => {
                let (remaining_body, var_steps, test_clause, body, loop_env) = self.unpack_do_body(data_start);
                // val is discarded (body evaluated for side effects)
                if self.lisp.get(remaining_body)?.is_nil() {
                    // Body done - start evaluating step expressions
                    self.step_do_start_steps(var_steps, test_clause, body, loop_env)
                } else {
                    // More body expressions
                    let next_body = self.lisp.car(remaining_body)?;
                    let rest_body = self.lisp.cdr(remaining_body)?;
                    let data_start = self.pack_do_body(rest_body, var_steps, test_clause, body, loop_env)?;
                    self.push_cont(Cont::DoBody(data_start))?;
                    Ok(Some(TrampolineState::Eval { expr: next_body, env: loop_env }))
                }
            }

            Cont::DoStep(data_start) => {
                let (remaining_steps, collected_vals, var_steps, test_clause, body, loop_env, current_var) = self.unpack_do_step(data_start);
                // val is the evaluated step expression - collect and continue
                let new_collected = self.lisp.cons(current_var, val)?;
                let new_collected = self.lisp.cons(new_collected, collected_vals)?;
                
                if self.lisp.get(remaining_steps)?.is_nil() {
                    // All steps evaluated - update environment and loop
                    let new_env = self.apply_do_step_values(loop_env, new_collected)?;
                    // Continue to next iteration - evaluate test
                    let data_start = self.pack_do_test_result(var_steps, test_clause, body, new_env)?;
                    self.push_cont(Cont::DoTestResult(data_start))?;
                    let test = self.lisp.car(test_clause)?;
                    Ok(Some(TrampolineState::Eval { expr: test, env: new_env }))
                } else {
                    // More steps to evaluate
                    let next_pair = self.lisp.car(remaining_steps)?;
                    let rest_steps = self.lisp.cdr(remaining_steps)?;
                    let next_var = self.lisp.car(next_pair)?;
                    let next_step = self.lisp.cdr(next_pair)?;
                    
                    let data_start = self.pack_do_step(rest_steps, new_collected, var_steps, test_clause, body, loop_env, next_var)?;
                    self.push_cont(Cont::DoStep(data_start))?;
                    Ok(Some(TrampolineState::Eval { expr: next_step, env: loop_env }))
                }
            }

            Cont::ApplyFirst(data_start) => {
                let (args_list_expr, env) = self.unpack_apply_first(data_start);
                // val is the evaluated function - now evaluate args list
                let data_start = self.pack_apply_second(val, env)?;
                self.push_cont(Cont::ApplySecond(data_start))?;
                Ok(Some(TrampolineState::Eval { expr: args_list_expr, env }))
            }

            Cont::ApplySecond(data_start) => {
                let (func, env) = self.unpack_apply_second(data_start);
                // val is the evaluated args list - perform application
                let args_list = val;
                let call_expr = self.lisp.cons(func, args_list)?;
                self.push_frame(call_expr, func)?;
                let data_start = self.pack_apply_forced(args_list, env, call_expr)?;
                self.push_cont(Cont::ApplyForced(data_start))?;
                Ok(Some(TrampolineState::Return { val: func }))
            }

            Cont::ValuesCollect(data_start) => {
                let (remaining, collected, env) = self.unpack_values_collect(data_start);
                // val is an evaluated value - collect and continue
                let new_collected = self.lisp.cons(val, collected)?;
                
                if self.lisp.get(remaining)?.is_nil() {
                    // All values evaluated - build result list (reverse collected)
                    let result = self.reverse_list(new_collected)?;
                    Ok(Some(TrampolineState::Return { val: result }))
                } else {
                    // More values to evaluate
                    let next_expr = self.lisp.car(remaining)?;
                    let rest = self.lisp.cdr(remaining)?;
                    let data_start = self.pack_values_collect(rest, new_collected, env)?;
                    self.push_cont(Cont::ValuesCollect(data_start))?;
                    Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                }
            }

            Cont::DefineValue(data_start) => {
                let name = self.unpack_define_value(data_start);
                // val is the evaluated value - define the binding
                self.define(name, val)?;
                Ok(Some(TrampolineState::Return { val: name }))
            }

            Cont::SetValue(data_start) => {
                let (name, env) = self.unpack_set_value(data_start);
                // val is the evaluated value - set! the binding
                self.env_set(env, name, val)?;
                Ok(Some(TrampolineState::Return { val }))
            }

            Cont::NativeArgsCollect(data_start) => {
                let (remaining, collected, id, env) = self.unpack_native_args_collect(data_start);
                // val is an evaluated argument - collect and continue
                let new_collected = self.lisp.cons(val, collected)?;
                
                if self.lisp.get(remaining)?.is_nil() {
                    // All args evaluated - call native function
                    let args = self.reverse_list(new_collected)?;
                    if let Some(native_fn) = self.native_registry.lookup_by_id(id) {
                        let result = native_fn(self.lisp, args)?;
                        Ok(Some(TrampolineState::Return { val: result }))
                    } else {
                        Err(self.make_error(ErrorKind::NotAFunction, args)
                            .with_message("native function not found"))
                    }
                } else {
                    // More args to evaluate
                    let next_expr = self.lisp.car(remaining)?;
                    let rest = self.lisp.cdr(remaining)?;
                    let data_start = self.pack_native_args_collect(rest, new_collected, id, env)?;
                    self.push_cont(Cont::NativeArgsCollect(data_start))?;
                    Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                }
            }

            Cont::QuasiquoteCar(data_start) => {
                let (cdr, depth, env) = self.unpack_quasiquote_car(data_start);
                // val is the evaluated car - now process cdr
                let car_val = val;
                let data_start = self.pack_quasiquote_cdr(car_val)?;
                self.push_cont(Cont::QuasiquoteCdr(data_start))?;
                Ok(Some(self.step_quasiquote_trampoline(cdr, env, depth)?))
            }

            Cont::QuasiquoteCdr(data_start) => {
                let car_val = self.unpack_quasiquote_cdr(data_start);
                // val is the processed cdr - cons with car
                let result = self.lisp.cons(car_val, val)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            Cont::QuasiquoteUnquoteWrap => {
                // val is the inner processed value - wrap with unquote
                let unquote_sym = self.lisp.symbol("unquote")?;
                let nil = self.lisp.nil()?;
                let inner_list = self.lisp.cons(val, nil)?;
                let result = self.lisp.cons(unquote_sym, inner_list)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            Cont::QuasiquoteNestedWrap => {
                // val is the inner processed value - wrap with quasiquote
                let qq_sym = self.lisp.symbol("quasiquote")?;
                let nil = self.lisp.nil()?;
                let inner_list = self.lisp.cons(val, nil)?;
                let result = self.lisp.cons(qq_sym, inner_list)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            Cont::QuasiquoteSplice(data_start) => {
                let (cdr, depth, env) = self.unpack_quasiquote_splice(data_start);
                // val is the evaluated splice expression - process cdr then append
                let splice_val = val;
                let data_start = self.pack_quasiquote_splice_append(splice_val)?;
                self.push_cont(Cont::QuasiquoteSpliceAppend(data_start))?;
                Ok(Some(self.step_quasiquote_trampoline(cdr, env, depth)?))
            }

            Cont::QuasiquoteSpliceAppend(data_start) => {
                let splice_val = self.unpack_quasiquote_splice_append(data_start);
                // val is the processed cdr - append splice_val with it
                let result = self.append_lists(splice_val, val)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }
        }
    }

    /// Helper for cond continuation - process remaining clauses
    fn step_eval_cond_cont(&mut self, clauses: ArenaIndex, env: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
        if self.lisp.get(clauses)?.is_nil() {
            // No more clauses - return unspecified (nil)
            let nil = self.lisp.nil()?;
            return Ok(Some(TrampolineState::Return { val: nil }));
        }

        let clause = self.lisp.car(clauses)?;
        let rest_clauses = self.lisp.cdr(clauses)?;
        let test = self.lisp.car(clause)?;
        let then_exprs = self.lisp.cdr(clause)?;

        // Check for else clause
        if self.lisp.symbol_matches(test, "else")? {
            if self.lisp.get(then_exprs)?.is_nil() {
                let nil = self.lisp.nil()?;
                return Ok(Some(TrampolineState::Return { val: nil }));
            }
            let begin = self.lisp.symbol("begin")?;
            let new_expr = self.lisp.cons(begin, then_exprs)?;
            return Ok(Some(TrampolineState::Eval { expr: new_expr, env }));
        }

        // Push continuation for after evaluating test
        let data_start = self.pack_cond_test(then_exprs, rest_clauses, env)?;
        self.push_cont(Cont::CondTest(data_start))?;

        // Evaluate the test
        Ok(Some(TrampolineState::Eval { expr: test, env }))
    }

    // ========================================================================
    // Helper functions for fully trampolined evaluation
    // ========================================================================

    /// Helper for case - check clauses after key is evaluated
    fn step_return_case_key(&mut self, key: ArenaIndex, clauses: ArenaIndex, env: ArenaIndex) 
        -> Result<Option<TrampolineState>, EvalError> 
    {
        // Check each clause
        let mut current = clauses;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    // No match found, return nil
                    let nil = self.lisp.nil()?;
                    return Ok(Some(TrampolineState::Return { val: nil }));
                }
                Value::Cons { .. } => {
                    let clause = self.lisp.car(current)?;
                    let rest = self.lisp.cdr(current)?;
                    let datums = self.lisp.car(clause)?;
                    let body = self.lisp.cdr(clause)?;
                    
                    // Check for 'else' clause
                    if self.lisp.symbol_matches(datums, "else").unwrap_or(false) {
                        let state = self.step_eval_begin(body, env)?;
                        return Ok(Some(state));
                    }
                    
                    // Check if key matches any datum
                    if case_matches(self.lisp, key, datums)? {
                        let state = self.step_eval_begin(body, env)?;
                        return Ok(Some(state));
                    }
                    
                    current = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, clauses)),
            }
        }
    }

    /// Helper for do - continue processing init bindings after one is evaluated
    fn step_return_do_init(
        &mut self, 
        remaining_bindings: ArenaIndex, 
        var_steps: ArenaIndex, 
        test_clause: ArenaIndex, 
        body: ArenaIndex, 
        loop_env: ArenaIndex, 
        original_env: ArenaIndex
    ) -> Result<Option<TrampolineState>, EvalError> {
        if self.lisp.get(remaining_bindings)?.is_nil() {
            // All bindings done - start the loop by evaluating the test
            let data_start = self.pack_do_test_result(var_steps, test_clause, body, loop_env)?;
            self.push_cont(Cont::DoTestResult(data_start))?;
            let test = self.lisp.car(test_clause)?;
            Ok(Some(TrampolineState::Eval { expr: test, env: loop_env }))
        } else {
            // More bindings - get next binding
            let binding = self.lisp.car(remaining_bindings)?;
            let rest = self.lisp.cdr(remaining_bindings)?;
            
            let var = self.lisp.car(binding)?;
            let init_rest = self.lisp.cdr(binding)?;
            let init = self.lisp.car(init_rest)?;
            let step_rest = self.lisp.cdr(init_rest)?;
            let step = if self.lisp.get(step_rest)?.is_nil() {
                var // No step, use variable itself
            } else {
                self.lisp.car(step_rest)?
            };
            
            // Add (var . step) to var_steps
            let var_step_pair = self.lisp.cons(var, step)?;
            let new_var_steps = self.lisp.cons(var_step_pair, var_steps)?;
            
            // Push continuation and evaluate init
            let data_start = self.pack_do_init(rest, new_var_steps, test_clause, body, loop_env, original_env, var)?;
            self.push_cont(Cont::DoInit(data_start))?;
            Ok(Some(TrampolineState::Eval { expr: init, env: original_env }))
        }
    }

    /// Helper for do - start evaluating step expressions
    fn step_do_start_steps(
        &mut self,
        var_steps: ArenaIndex,
        test_clause: ArenaIndex,
        body: ArenaIndex,
        loop_env: ArenaIndex,
    ) -> Result<Option<TrampolineState>, EvalError> {
        if self.lisp.get(var_steps)?.is_nil() {
            // No variables - just loop back to test
            let data_start = self.pack_do_test_result(var_steps, test_clause, body, loop_env)?;
            self.push_cont(Cont::DoTestResult(data_start))?;
            let test = self.lisp.car(test_clause)?;
            Ok(Some(TrampolineState::Eval { expr: test, env: loop_env }))
        } else {
            // Start evaluating step expressions
            // var_steps is a list of (var . step) pairs, we need to reverse it first
            // since we built it in reverse order during init
            let reversed = self.reverse_list(var_steps)?;
            
            let first_pair = self.lisp.car(reversed)?;
            let rest_steps = self.lisp.cdr(reversed)?;
            let first_var = self.lisp.car(first_pair)?;
            let first_step = self.lisp.cdr(first_pair)?;
            
            let nil = self.lisp.nil()?;
            let data_start = self.pack_do_step(rest_steps, nil, reversed, test_clause, body, loop_env, first_var)?;
            self.push_cont(Cont::DoStep(data_start))?;
            Ok(Some(TrampolineState::Eval { expr: first_step, env: loop_env }))
        }
    }

    /// Helper for do - apply collected step values to create new environment
    fn apply_do_step_values(&mut self, base_env: ArenaIndex, collected: ArenaIndex) -> EvalResult {
        // collected is a list of ((var . val) ...) pairs in reverse order
        let mut result_env = base_env;
        let mut current = collected;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { .. } => {
                    let pair = self.lisp.car(current)?;
                    let rest = self.lisp.cdr(current)?;
                    let var = self.lisp.car(pair)?;
                    let val = self.lisp.cdr(pair)?;
                    result_env = self.env_extend(result_env, var, val)?;
                    current = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
        
        Ok(result_env)
    }

    /// Helper for quasiquote - trampolined processing
    fn step_quasiquote_trampoline(&mut self, template: ArenaIndex, env: ArenaIndex, depth: usize) 
        -> Result<TrampolineState, EvalError> 
    {
        match self.lisp.get(template)? {
            Value::Cons { .. } => {
                let car = self.lisp.car(template)?;
                let cdr = self.lisp.cdr(template)?;
                // Check for unquote
                if self.lisp.symbol_matches(car, "unquote").unwrap_or(false) {
                    let inner_expr = self.lisp.car(cdr)?;
                    if depth == 1 {
                        // Evaluate the unquoted expression directly
                        return Ok(TrampolineState::Eval { expr: inner_expr, env });
                    } else {
                        // Nested quasiquote - decrease depth and process
                        self.push_cont(Cont::QuasiquoteUnquoteWrap)?;
                        return self.step_quasiquote_trampoline(inner_expr, env, depth - 1);
                    }
                }
                
                // Check for unquote-splicing at top level
                if self.lisp.symbol_matches(car, "unquote-splicing").unwrap_or(false)
                    && depth == 1
                {
                    // Return the evaluated list (caller handles splicing)
                    let inner_expr = self.lisp.car(cdr)?;
                    return Ok(TrampolineState::Eval { expr: inner_expr, env });
                }
                
                // Check for nested quasiquote
                if self.lisp.symbol_matches(car, "quasiquote").unwrap_or(false) {
                    self.push_cont(Cont::QuasiquoteNestedWrap)?;
                    let inner_expr = self.lisp.car(cdr)?;
                    return self.step_quasiquote_trampoline(inner_expr, env, depth + 1);
                }
                
                // Check for unquote-splicing in car position (special handling)
                if let Value::Cons { .. } = self.lisp.get(car)? {
                    let inner_car = self.lisp.car(car)?;
                    let inner_cdr = self.lisp.cdr(car)?;
                    if self.lisp.symbol_matches(inner_car, "unquote-splicing").unwrap_or(false) && depth == 1 {
                        // Splice the result into the list
                        let splice_expr = self.lisp.car(inner_cdr)?;
                        let data_start = self.pack_quasiquote_splice(cdr, depth, env)?;
                        self.push_cont(Cont::QuasiquoteSplice(data_start))?;
                        return Ok(TrampolineState::Eval { expr: splice_expr, env });
                    }
                }
                
                // Recursively process car and cdr
                let data_start = self.pack_quasiquote_car(cdr, depth, env)?;
                self.push_cont(Cont::QuasiquoteCar(data_start))?;
                self.step_quasiquote_trampoline(car, env, depth)
            }
            _ => {
                // Atoms are returned as-is
                Ok(TrampolineState::Return { val: template })
            }
        }
    }

    /// Evaluate let using continuations (no Rust recursion)
    fn step_eval_let(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;

        // Build body expression
        let body = if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
            self.lisp.car(body_list)?
        } else {
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body_list)?
        };

        // If no bindings, just evaluate body
        if self.lisp.get(bindings)?.is_nil() {
            return Ok(TrampolineState::Eval { expr: body, env });
        }

        // Get first binding
        let first_binding = self.lisp.car(bindings)?;
        let rest_bindings = self.lisp.cdr(bindings)?;
        let name = self.lisp.car(first_binding)?;
        let value_expr = self.lisp.car(self.lisp.cdr(first_binding)?)?;

        // Push continuation for after evaluating this binding
        let data_start = self.pack_let_binding(rest_bindings, env, env, body, name)?;
        self.push_cont(Cont::LetBinding(data_start))?;

        // Evaluate the first value expression in the original environment
        Ok(TrampolineState::Eval { expr: value_expr, env })
    }

    /// Evaluate let* using continuations (no Rust recursion)
    fn step_eval_let_star(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;

        // Build body expression
        let body = if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
            self.lisp.car(body_list)?
        } else {
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body_list)?
        };

        // If no bindings, just evaluate body
        if self.lisp.get(bindings)?.is_nil() {
            return Ok(TrampolineState::Eval { expr: body, env });
        }

        // Get first binding
        let first_binding = self.lisp.car(bindings)?;
        let rest_bindings = self.lisp.cdr(bindings)?;
        let name = self.lisp.car(first_binding)?;
        let value_expr = self.lisp.car(self.lisp.cdr(first_binding)?)?;

        // Push continuation for after evaluating this binding
        let data_start = self.pack_let_star_binding(rest_bindings, env, body, name)?;
        self.push_cont(Cont::LetStarBinding(data_start))?;

        // Evaluate the first value expression
        Ok(TrampolineState::Eval { expr: value_expr, env })
    }

    /// Evaluate letrec/letrec* using continuations (no Rust recursion)
    fn step_eval_letrec(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;

        // Build body expression
        let body = if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
            self.lisp.car(body_list)?
        } else {
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body_list)?
        };

        // If no bindings, just evaluate body
        if self.lisp.get(bindings)?.is_nil() {
            return Ok(TrampolineState::Eval { expr: body, env });
        }

        // First, create environment with all names bound to nil
        let mut new_env = env;
        let mut current = bindings;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { .. } => {
                    let binding = self.lisp.car(current)?;
                    let rest = self.lisp.cdr(current)?;
                    let name = self.lisp.car(binding)?;
                    let undefined = self.lisp.nil()?;
                    new_env = self.env_extend(new_env, name, undefined)?;
                    current = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, bindings)),
            }
        }

        // Now start evaluating init expressions
        let first_binding = self.lisp.car(bindings)?;
        let rest_bindings = self.lisp.cdr(bindings)?;
        let name = self.lisp.car(first_binding)?;
        let init_expr = self.lisp.car(self.lisp.cdr(first_binding)?)?;

        // Push continuation for after evaluating this init
        let data_start = self.pack_letrec_init(rest_bindings, new_env, body, name)?;
        self.push_cont(Cont::LetrecInit(data_start))?;

        // Evaluate the init expression in the new environment (where all names are visible)
        Ok(TrampolineState::Eval { expr: init_expr, env: new_env })
    }

    /// Evaluate begin using continuations (no Rust recursion)
    fn step_eval_begin(&mut self, exprs: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        if self.lisp.get(exprs)?.is_nil() {
            // Empty begin - return nil
            let nil = self.lisp.nil()?;
            return Ok(TrampolineState::Return { val: nil });
        }

        let first_expr = self.lisp.car(exprs)?;
        let rest = self.lisp.cdr(exprs)?;

        if self.lisp.get(rest)?.is_nil() {
            // Single expression - tail call
            Ok(TrampolineState::Eval { expr: first_expr, env })
        } else {
            // Multiple expressions - push continuation for the rest
            let data_start = self.pack_begin_seq(rest, env)?;
            self.push_cont(Cont::BeginSeq(data_start))?;
            Ok(TrampolineState::Eval { expr: first_expr, env })
        }
    }

    /// Evaluate and using continuations (no Rust recursion)
    fn step_eval_and(&mut self, exprs: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        if self.lisp.get(exprs)?.is_nil() {
            // (and) with no args returns #t
            let true_val = self.lisp.boolean(true)?;
            return Ok(TrampolineState::Return { val: true_val });
        }

        let first_expr = self.lisp.car(exprs)?;
        let rest = self.lisp.cdr(exprs)?;

        if self.lisp.get(rest)?.is_nil() {
            // Single expression - evaluate it (its value is the result)
            Ok(TrampolineState::Eval { expr: first_expr, env })
        } else {
            // Multiple expressions - push continuation
            let data_start = self.pack_and(rest, env)?;
            self.push_cont(Cont::And(data_start))?;
            Ok(TrampolineState::Eval { expr: first_expr, env })
        }
    }

    /// Evaluate or using continuations (no Rust recursion)
    fn step_eval_or(&mut self, exprs: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        if self.lisp.get(exprs)?.is_nil() {
            // (or) with no args returns #f
            let false_val = self.lisp.boolean(false)?;
            return Ok(TrampolineState::Return { val: false_val });
        }

        let first_expr = self.lisp.car(exprs)?;
        let rest = self.lisp.cdr(exprs)?;

        if self.lisp.get(rest)?.is_nil() {
            // Single expression - evaluate it (its value is the result)
            Ok(TrampolineState::Eval { expr: first_expr, env })
        } else {
            // Multiple expressions - push continuation
            let data_start = self.pack_or(rest, env)?;
            self.push_cont(Cont::Or(data_start))?;
            Ok(TrampolineState::Eval { expr: first_expr, env })
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
                    let data_start = self.pack_binary_builtin_first(builtin, second_arg_expr, call_expr, env)?;
                    self.push_cont(Cont::BinaryBuiltinFirst(data_start))?;
                    return Ok(Some(TrampolineState::Eval { expr: first_arg, env }));
                }
            }
        }
        
        // General case: collect args and apply
        let nil = self.lisp.nil()?;
        let data_start = self.pack_builtin_force_arg(builtin, rest_args, nil, call_expr, env)?;
        self.push_cont(Cont::BuiltinForceArg(data_start))?;
        
        Ok(Some(TrampolineState::Eval { expr: first_arg, env }))
    }
    
    /// Reverse a list (used for BuiltinForceArg fallback path)
    fn reverse_list(&self, mut list: ArenaIndex) -> EvalResult {
        let mut result = self.lisp.nil()?;
        loop {
            match self.lisp.get(list)? {
                Value::Nil => return Ok(result),
                Value::Cons { .. } => {
                    let car = self.lisp.car(list)?;
                    let cdr = self.lisp.cdr(list)?;
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
                    Value::Cons { .. } => self.lisp.car(arg).map_err(Into::into),
                    // Scheme R7RS: car of empty list is an error
                    Value::Nil => Err(self.type_error(call_expr, "pair", "null")),
                    _ => Err(self.type_error(call_expr, "pair", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::Cdr => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Cons { .. } => self.lisp.cdr(arg).map_err(Into::into),
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
                    (Value::Symbol(_), Value::Symbol(_)) => self.lisp.symbol_eq(a, b)?,
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
                    (Value::Symbol(_), Value::Symbol(_)) => self.lisp.symbol_eq(a, b)?,
                    _ => a == b,
                };
                
                self.lisp.boolean(eqv).map_err(Into::into)
            }
            
            Builtin::EqualP => {
                // equal? - tests structural equality recursively
                let result = equal_recursive(self.lisp, self.lisp.car(args)?, self.lisp.car(self.lisp.cdr(args)?)?)?;
                self.lisp.boolean(result).map_err(Into::into)
            }
            
            Builtin::Null => builtin_unary_pred!(self, args, |v: Value| v.is_nil()),
            
            Builtin::Pairp => builtin_unary_pred!(self, args, |v: Value| v.is_cons()),
            
            Builtin::Numberp => builtin_unary_pred!(self, args, |v: Value| v.is_number()),
            
            Builtin::Booleanp => builtin_unary_pred!(self, args, |v: Value| v.is_boolean()),
            
            Builtin::Procedurep => builtin_unary_pred!(self, args, |v: Value| v.is_procedure()),
            
            Builtin::Symbolp => builtin_unary_pred!(self, args, |v: Value| v.is_symbol()),
            
            Builtin::Not => builtin_unary_pred!(self, args, |v: Value| v.is_false()),
            
            Builtin::Add => self.numeric_fold(args, 0, 
                |a, b| a.checked_add(b), 
                call_expr),
            
            Builtin::Sub => {
                let first = self.get_int(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                if self.lisp.get(rest)?.is_nil() {
                    // Unary minus
                    self.lisp.number(-first).map_err(Into::into)
                } else {
                    self.numeric_fold(rest, first, 
                        |a, b| a.checked_sub(b), 
                        call_expr)
                }
            }
            
            Builtin::Mul => self.numeric_fold(args, 1, 
                |a, b| a.checked_mul(b), 
                call_expr),
            
            Builtin::Div => {
                // Integer division
                let first = self.get_int(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold(rest, first, 
                    |a, b| if b == 0 { None } else { a.checked_div(b) },
                    call_expr)
            }
            
            // Division operations with zero check - generated by builtin_div_op! macro
            // Scheme modulo: result has the sign of the divisor
            Builtin::Modulo => builtin_div_op!(self, args, call_expr, |a, b| ((a % b) + b) % b),
            // Scheme remainder: result has the sign of the dividend
            Builtin::Remainder => builtin_div_op!(self, args, call_expr, |a, b| a % b),
            // Integer quotient (truncated towards zero)
            Builtin::Quotient => builtin_div_op!(self, args, call_expr, |a, b| a / b),
            
            Builtin::Abs => {
                // Absolute value
                let n = self.get_int(self.lisp.car(args)?, call_expr)?;
                self.lisp.number(n.abs()).map_err(Into::into)
            }
            
            Builtin::Max => {
                // Maximum of one or more numbers
                let first = self.get_int(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold(rest, first, 
                    |a, b| Some(if a > b { a } else { b }),
                    call_expr)
            }
            
            Builtin::Min => {
                // Minimum of one or more numbers
                let first = self.get_int(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold(rest, first,
                    |a, b| Some(if a < b { a } else { b }),
                    call_expr)
            }
            
            Builtin::Gcd => {
                // Greatest common divisor (integers only)
                // gcd() with no args returns 0, gcd(n) returns |n|
                if self.lisp.get(args)?.is_nil() {
                    return self.lisp.number(0).map_err(Into::into);
                }
                let first = self.get_int(self.lisp.car(args)?, call_expr)?.abs();
                let rest = self.lisp.cdr(args)?;
                let mut acc = first;
                let mut current = rest;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => return self.lisp.number(acc).map_err(Into::into),
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            let n = self.get_int(car, call_expr)?.abs();
                            acc = gcd_helper(acc, n);
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
            }
            
            Builtin::Lcm => {
                // Least common multiple (integers only)
                // lcm() with no args returns 1, lcm(n) returns |n|
                if self.lisp.get(args)?.is_nil() {
                    return self.lisp.number(1).map_err(Into::into);
                }
                let first = self.get_int(self.lisp.car(args)?, call_expr)?.abs();
                let rest = self.lisp.cdr(args)?;
                let mut acc = first;
                let mut current = rest;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => return self.lisp.number(acc).map_err(Into::into),
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            let b_abs = self.get_int(car, call_expr)?.abs();
                            if acc == 0 || b_abs == 0 {
                                acc = 0;
                            } else {
                                let g = gcd_helper(acc, b_abs);
                                acc = (acc / g).saturating_mul(b_abs);
                            }
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
            }
            
            Builtin::Expt => {
                // Exponentiation: (expt base power)
                let base = self.get_int(self.lisp.car(args)?, call_expr)?;
                let power = self.get_int(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                
                if power < 0 {
                    // Negative integer exponent - error for integer-only mode
                    if base == 0 {
                        return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                    }
                    // Return 0 for |base| > 1, 1 for |base| == 1
                    let result = if base.abs() == 1 { 1 } else { 0 };
                    self.lisp.number(result).map_err(Into::into)
                } else {
                    let result = int_pow(base, power as usize);
                    self.lisp.number(result).map_err(Into::into)
                }
            }
            
            Builtin::Square => {
                // Square of a number
                let n = self.get_int(self.lisp.car(args)?, call_expr)?;
                self.lisp.number(n.saturating_mul(n)).map_err(Into::into)
            }
            
            // Numeric predicates - generated by builtin_numeric_pred! macro
            Builtin::Zerop => builtin_numeric_pred!(self, args, call_expr, |n| n == 0),
            Builtin::Positivep => builtin_numeric_pred!(self, args, call_expr, |n| n > 0),
            Builtin::Negativep => builtin_numeric_pred!(self, args, call_expr, |n| n < 0),
            Builtin::Oddp => builtin_numeric_pred!(self, args, call_expr, |n| n % 2 != 0),
            Builtin::Evenp => builtin_numeric_pred!(self, args, call_expr, |n| n % 2 == 0),
            
            Builtin::Integerp => {
                // All numbers are integers now
                let val = self.lisp.car(args)?;
                let is_int = matches!(self.lisp.get(val)?, Value::Number(_));
                self.lisp.boolean(is_int).map_err(Into::into)
            }
            
            Builtin::Exactp => {
                // All numbers are exact (integers)
                let val = self.lisp.car(args)?;
                let is_exact = matches!(self.lisp.get(val)?, Value::Number(_));
                self.lisp.boolean(is_exact).map_err(Into::into)
            }
            
            Builtin::Inexactp => {
                // No inexact numbers - always false for valid numbers
                let val = self.lisp.car(args)?;
                match self.lisp.get(val)? {
                    Value::Number(_) => self.lisp.boolean(false).map_err(Into::into),
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::ExactIntegerp => {
                // All numbers are exact integers
                let val = self.lisp.car(args)?;
                let is_exact_int = matches!(self.lisp.get(val)?, Value::Number(_));
                self.lisp.boolean(is_exact_int).map_err(Into::into)
            }
            
            // Rounding operations (identity for integers) - generated by builtin_int_identity! macro
            Builtin::Floor => builtin_int_identity!(self, args, call_expr),
            Builtin::Ceiling => builtin_int_identity!(self, args, call_expr),
            Builtin::Truncate => builtin_int_identity!(self, args, call_expr),
            Builtin::Round => builtin_int_identity!(self, args, call_expr),
            
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
            
            // ============================================================
            // Vector operations (R7RS Section 6.8)
            // ============================================================
            
            Builtin::Vectorp => {
                // (vector? x) - check if x is a vector
                let val = self.lisp.car(args)?;
                let is_vector = matches!(self.lisp.get(val)?, Value::Array { .. });
                self.lisp.boolean(is_vector).map_err(Into::into)
            }
            
            Builtin::MakeVector => {
                // (make-vector k) or (make-vector k fill) - create a vector
                let len_val = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                
                let len = match self.lisp.get(len_val)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                // Default fill is 0 (unspecified in R7RS, we use 0)
                let fill = if self.lisp.get(rest)?.is_nil() {
                    self.lisp.number(0)?
                } else {
                    self.lisp.car(rest)?
                };
                
                self.lisp.make_array(len, fill).map_err(Into::into)
            }
            
            Builtin::Vector => {
                // (vector obj ...) - create vector from arguments
                // First count the arguments (args is always a proper list from evaluator)
                let mut count = 0usize;
                let mut current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            count += 1;
                            current = self.lisp.cdr(current)?;
                        }
                        _ => break, // Should not happen for function args
                    }
                }
                
                // Create vector with placeholder
                let placeholder = self.lisp.number(0)?;
                let vec = self.lisp.make_array(count, placeholder)?;
                
                // Fill in the elements
                current = args;
                for i in 0..count {
                    let val = self.lisp.car(current)?;
                    self.lisp.array_set(vec, i, val)?;
                    current = self.lisp.cdr(current)?;
                }
                
                Ok(vec)
            }
            
            Builtin::VectorLength => {
                // (vector-length vec) - get length of vector
                let vec = self.lisp.car(args)?;
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
                        self.lisp.number(len as isize).map_err(Into::into)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::VectorRef => {
                // (vector-ref vec k) - get element at index k
                extract_args!(self, args, vec, index_val);
                
                let index = match self.lisp.get(index_val)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        self.lisp.array_get(vec, index).map_err(Into::into)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::VectorSet => {
                // (vector-set! vec k obj) - set element at index k
                extract_args!(self, args, vec, index_val, value);
                
                let index = match self.lisp.get(index_val)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        self.lisp.array_set(vec, index, value)?;
                        // R7RS: returns unspecified, we return the vector
                        Ok(vec)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::VectorToList => {
                // (vector->list vec) - convert vector to list
                let vec = self.lisp.car(args)?;
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
                        // Build list from end to front
                        let mut result = self.lisp.nil()?;
                        for i in (0..len).rev() {
                            let elem = self.lisp.array_get(vec, i)?;
                            result = self.lisp.cons(elem, result)?;
                        }
                        Ok(result)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::ListToVector => {
                // (list->vector lst) - convert list to vector
                let lst = self.lisp.car(args)?;
                
                // First count the list elements, validating it's a proper list
                let mut count = 0usize;
                let mut current = lst;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            count += 1;
                            current = self.lisp.cdr(current)?;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                    }
                }
                
                // Create vector with placeholder
                let placeholder = self.lisp.number(0)?;
                let vec = self.lisp.make_array(count, placeholder)?;
                
                // Fill in the elements
                current = lst;
                for i in 0..count {
                    let val = self.lisp.car(current)?;
                    self.lisp.array_set(vec, i, val)?;
                    current = self.lisp.cdr(current)?;
                }
                
                Ok(vec)
            }
            
            Builtin::VectorFill => {
                // (vector-fill! vec fill) - fill vector with value
                extract_args!(self, args, vec, fill);
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
                        for i in 0..len {
                            self.lisp.array_set(vec, i, fill)?;
                        }
                        // R7RS: returns unspecified, we return the vector
                        Ok(vec)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::VectorCopy => {
                // (vector-copy vec) - copy a vector
                let vec = self.lisp.car(args)?;
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
                        // Create new vector with same length
                        let placeholder = self.lisp.number(0)?;
                        let new_vec = self.lisp.make_array(len, placeholder)?;
                        
                        // Copy elements
                        for i in 0..len {
                            let elem = self.lisp.array_get(vec, i)?;
                            self.lisp.array_set(new_vec, i, elem)?;
                        }
                        
                        Ok(new_vec)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
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
            
            // ============================================================
            // Character operations (R7RS Section 6.6)
            // ============================================================
            
            Builtin::Charp => {
                // (char? obj) - Check if value is a character
                builtin_unary_pred!(self, args, |v: Value| matches!(v, Value::Char(_)))
            }
            
            Builtin::CharEq => {
                // (char=? char1 char2 ...) - Character equality
                self.char_chain_compare(args, |a, b| a == b, call_expr)
            }
            
            Builtin::CharLt => {
                // (char<? char1 char2 ...) - Monotonically increasing
                self.char_chain_compare(args, |a, b| a < b, call_expr)
            }
            
            Builtin::CharGt => {
                // (char>? char1 char2 ...) - Monotonically decreasing
                self.char_chain_compare(args, |a, b| a > b, call_expr)
            }
            
            Builtin::CharLe => {
                // (char<=? char1 char2 ...) - Monotonically non-decreasing
                self.char_chain_compare(args, |a, b| a <= b, call_expr)
            }
            
            Builtin::CharGe => {
                // (char>=? char1 char2 ...) - Monotonically non-increasing
                self.char_chain_compare(args, |a, b| a >= b, call_expr)
            }
            
            Builtin::CharToInteger => {
                // (char->integer char) - Convert char to Unicode code point
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                self.lisp.number(c as isize).map_err(Into::into)
            }
            
            Builtin::IntegerToChar => {
                // (integer->char n) - Convert Unicode code point to char
                let n = self.get_int(self.lisp.car(args)?, call_expr)?;
                if n < 0 {
                    return Err(self.type_error(call_expr, "non-negative integer", "negative integer"));
                }
                match char::from_u32(n as u32) {
                    Some(c) => self.lisp.char(c).map_err(Into::into),
                    None => Err(self.type_error(call_expr, "valid Unicode code point", "invalid code point")),
                }
            }
            
            Builtin::CharUpcase => {
                // (char-upcase char) - Convert to uppercase
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                // Simple ASCII uppercase for no_std
                let upper = if c >= 'a' && c <= 'z' {
                    ((c as u8) - b'a' + b'A') as char
                } else {
                    c
                };
                self.lisp.char(upper).map_err(Into::into)
            }
            
            Builtin::CharDowncase => {
                // (char-downcase char) - Convert to lowercase
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                // Simple ASCII lowercase for no_std
                let lower = if c >= 'A' && c <= 'Z' {
                    ((c as u8) - b'A' + b'a') as char
                } else {
                    c
                };
                self.lisp.char(lower).map_err(Into::into)
            }
            
            // ============================================================
            // String operations (R7RS Section 6.7)
            // ============================================================
            
            Builtin::Stringp => {
                // (string? obj) - Check if value is a string
                builtin_unary_pred!(self, args, |v: Value| matches!(v, Value::String { .. }))
            }
            
            Builtin::MakeString => {
                // (make-string k) or (make-string k char)
                let k = self.get_int(self.lisp.car(args)?, call_expr)?;
                if k < 0 {
                    return Err(self.type_error(call_expr, "non-negative integer", "negative integer"));
                }
                let rest = self.lisp.cdr(args)?;
                let fill = if self.lisp.get(rest)?.is_nil() {
                    ' ' // Default fill character
                } else {
                    self.get_char(self.lisp.car(rest)?, call_expr)?
                };
                
                // Create string of k characters
                const MAX_MAKE_STRING_LEN: usize = 1024;
                let len = k as usize;
                if len > MAX_MAKE_STRING_LEN {
                    return Err(self.make_error(ErrorKind::TypeError, call_expr));
                }
                let mut chars = ['\0'; MAX_MAKE_STRING_LEN];
                chars[..len].fill(fill);
                self.lisp.string_from_chars(&chars[..len]).map_err(Into::into)
            }
            
            Builtin::String => {
                // (string char ...) - Create string from characters
                const MAX_STRING_LEN: usize = 1024;
                let mut chars = ['\0'; MAX_STRING_LEN];
                let mut len = 0;
                let mut current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            if len >= MAX_STRING_LEN {
                                return Err(self.make_error(ErrorKind::TypeError, call_expr));
                            }
                            chars[len] = self.get_char(car, call_expr)?;
                            len += 1;
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
                self.lisp.string_from_chars(&chars[..len]).map_err(Into::into)
            }
            
            Builtin::StringLength => {
                // (string-length string) - Get length
                let str_idx = self.lisp.car(args)?;
                match self.lisp.get(str_idx)? {
                    Value::String { .. } => {
                        let len = self.lisp.string_len(str_idx)?;
                        self.lisp.number(len as isize).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringRef => {
                // (string-ref string k) - Get character at index
                extract_args!(self, args, str_idx, k_idx);
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let k = self.get_int(k_idx, call_expr)?;
                        if k < 0 || (k as usize) >= len {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        // Characters start at data (no header with inline length)
                        let char_slot = self.lisp.arena_index_at_offset(data, k as usize)?;
                        match self.lisp.get(char_slot)? {
                            Value::Char(c) => self.lisp.char(c).map_err(Into::into),
                            _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                        }
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringSet => {
                // (string-set! string k char) - Set character at index
                let str_idx = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let k_idx = self.lisp.car(rest)?;
                let rest2 = self.lisp.cdr(rest)?;
                let char_arg = self.lisp.car(rest2)?;
                
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let k = self.get_int(k_idx, call_expr)?;
                        if k < 0 || (k as usize) >= len {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        let c = self.get_char(char_arg, call_expr)?;
                        // Characters start at data (no header with inline length)
                        let char_slot = self.lisp.arena_index_at_offset(data, k as usize)?;
                        self.lisp.set(char_slot, Value::Char(c))?;
                        // Return unspecified value (we use the string itself)
                        Ok(str_idx)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringEq => {
                // (string=? string1 string2 ...) - String equality
                self.string_chain_compare(args, |ordering| ordering == core::cmp::Ordering::Equal, call_expr)
            }
            
            Builtin::StringLt => {
                // (string<? string1 string2 ...) - Monotonically increasing
                self.string_chain_compare(args, |ordering| ordering == core::cmp::Ordering::Less, call_expr)
            }
            
            Builtin::StringGt => {
                // (string>? string1 string2 ...) - Monotonically decreasing
                self.string_chain_compare(args, |ordering| ordering == core::cmp::Ordering::Greater, call_expr)
            }
            
            Builtin::StringLe => {
                // (string<=? string1 string2 ...) - Monotonically non-decreasing
                self.string_chain_compare(args, |ordering| ordering != core::cmp::Ordering::Greater, call_expr)
            }
            
            Builtin::StringGe => {
                // (string>=? string1 string2 ...) - Monotonically non-increasing
                self.string_chain_compare(args, |ordering| ordering != core::cmp::Ordering::Less, call_expr)
            }
            
            Builtin::StringAppend => {
                // (string-append string ...) - Concatenate strings
                const MAX_TOTAL_LEN: usize = 4096;
                let mut chars = ['\0'; MAX_TOTAL_LEN];
                let mut total_len = 0;
                
                let mut current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            match self.lisp.get(car)? {
                                Value::String { len, data } => {
                                    if total_len + len > MAX_TOTAL_LEN {
                                        return Err(self.make_error(ErrorKind::TypeError, call_expr));
                                    }
                                    for i in 0..len {
                                        // Characters start at data (no header with inline length)
                                        let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                                        match self.lisp.get(char_slot)? {
                                            Value::Char(c) => {
                                                chars[total_len] = c;
                                                total_len += 1;
                                            }
                                            _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                                        }
                                    }
                                }
                                v => return Err(self.type_error(call_expr, "string", v.type_name())),
                            }
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
                
                self.lisp.string_from_chars(&chars[..total_len]).map_err(Into::into)
            }
            
            Builtin::StringToList => {
                // (string->list string) - Convert string to list of characters
                let str_idx = self.lisp.car(args)?;
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let mut result = self.lisp.nil()?;
                        // Build list from end to start
                        for i in (0..len).rev() {
                            // Characters start at data (no header with inline length)
                            let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                            match self.lisp.get(char_slot)? {
                                Value::Char(c) => {
                                    let char_val = self.lisp.char(c)?;
                                    result = self.lisp.cons(char_val, result)?;
                                }
                                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                            }
                        }
                        Ok(result)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::ListToString => {
                // (list->string list) - Convert list of characters to string
                const MAX_STRING_LEN: usize = 1024;
                let mut chars = ['\0'; MAX_STRING_LEN];
                let mut len = 0;
                
                let mut current = self.lisp.car(args)?;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            if len >= MAX_STRING_LEN {
                                return Err(self.make_error(ErrorKind::TypeError, call_expr));
                            }
                            chars[len] = self.get_char(car, call_expr)?;
                            len += 1;
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
                
                self.lisp.string_from_chars(&chars[..len]).map_err(Into::into)
            }
            
            Builtin::Substring => {
                // (substring string start end) - Extract substring
                let str_idx = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let start_idx = self.lisp.car(rest)?;
                let rest2 = self.lisp.cdr(rest)?;
                let end_idx = self.lisp.car(rest2)?;
                
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let start = self.get_int(start_idx, call_expr)?;
                        let end = self.get_int(end_idx, call_expr)?;
                        
                        if start < 0 || end < 0 || (start as usize) > len || (end as usize) > len || start > end {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        
                        let sub_len = (end - start) as usize;
                        const MAX_SUBSTRING_LEN: usize = 1024;
                        let mut chars = ['\0'; MAX_SUBSTRING_LEN];
                        if sub_len > MAX_SUBSTRING_LEN {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        
                        for i in 0..sub_len {
                            // Characters start at data (no header with inline length)
                            let char_slot = self.lisp.arena_index_at_offset(data, (start as usize) + i)?;
                            match self.lisp.get(char_slot)? {
                                Value::Char(c) => chars[i] = c,
                                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                            }
                        }
                        
                        self.lisp.string_from_chars(&chars[..sub_len]).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringCopy => {
                // (string-copy string) - Copy a string
                let str_idx = self.lisp.car(args)?;
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        const MAX_STRING_LEN: usize = 1024;
                        let mut chars = ['\0'; MAX_STRING_LEN];
                        if len > MAX_STRING_LEN {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        
                        for i in 0..len {
                            // Characters start at data (no header with inline length)
                            let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                            match self.lisp.get(char_slot)? {
                                Value::Char(c) => chars[i] = c,
                                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                            }
                        }
                        
                        self.lisp.string_from_chars(&chars[..len]).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
        }
    }
    
    /// OPTIMIZED: Apply binary builtin directly without list allocation
    fn apply_binary_builtin(&mut self, builtin: Builtin, a: ArenaIndex, b: ArenaIndex, call_expr: ArenaIndex) 
        -> EvalResult 
    {
        match builtin {
            Builtin::Add => {
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                match x.checked_add(y) {
                    Some(n) => self.lisp.number(n).map_err(Into::into),
                    None => Err(self.make_error(ErrorKind::DivisionByZero, call_expr)),
                }
            }
            Builtin::Sub => {
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                match x.checked_sub(y) {
                    Some(n) => self.lisp.number(n).map_err(Into::into),
                    None => Err(self.make_error(ErrorKind::DivisionByZero, call_expr)),
                }
            }
            Builtin::Mul => {
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                match x.checked_mul(y) {
                    Some(n) => self.lisp.number(n).map_err(Into::into),
                    None => Err(self.make_error(ErrorKind::DivisionByZero, call_expr)),
                }
            }
            Builtin::Div => {
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                if y == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                self.lisp.number(x / y).map_err(Into::into)
            }
            Builtin::Modulo => {
                // Scheme modulo: result has the sign of the divisor
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                if y == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                let result = ((x % y) + y) % y;
                self.lisp.number(result).map_err(Into::into)
            }
            Builtin::Remainder => {
                // Scheme remainder: result has the sign of the dividend
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                if y == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                self.lisp.number(x % y).map_err(Into::into)
            }
            Builtin::Lt => {
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                self.lisp.boolean(x < y).map_err(Into::into)
            }
            Builtin::Gt => {
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                self.lisp.boolean(x > y).map_err(Into::into)
            }
            Builtin::Le => {
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                self.lisp.boolean(x <= y).map_err(Into::into)
            }
            Builtin::Ge => {
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                self.lisp.boolean(x >= y).map_err(Into::into)
            }
            Builtin::NumEq => {
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
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
                    (Value::Symbol(_), Value::Symbol(_)) => self.lisp.symbol_eq(a, b)?,
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
    
    /// Get integer from already-evaluated value
    fn get_int(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<isize, EvalError> {
        match self.lisp.get(idx)? {
            Value::Number(n) => Ok(n),
            v => Err(self.type_error(call_expr, "integer", v.type_name())),
        }
    }
    
    /// Get character from already-evaluated value
    fn get_char(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<char, EvalError> {
        match self.lisp.get(idx)? {
            Value::Char(c) => Ok(c),
            v => Err(self.type_error(call_expr, "char", v.type_name())),
        }
    }
    
    /// Numeric fold with already-evaluated integer args
    fn numeric_fold<F>(&self, args: ArenaIndex, init: isize, int_f: F, call_expr: ArenaIndex) -> EvalResult
    where 
        F: Fn(isize, isize) -> Option<isize>,
    {
        let mut acc = init;
        let mut current = args;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.lisp.number(acc).map_err(Into::into),
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    let n = self.get_int(car, call_expr)?;
                    acc = match int_f(acc, n) {
                        Some(r) => r,
                        None => return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)),
                    };
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Compare two integer numbers
    fn compare_numbers<F>(&self, args: ArenaIndex, cmp: F, call_expr: ArenaIndex) -> EvalResult
    where F: Fn(isize, isize) -> bool
    {
        let a = self.get_int(self.lisp.car(args)?, call_expr)?;
        let b = self.get_int(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
        self.lisp.boolean(cmp(a, b)).map_err(Into::into)
    }
    
    /// Helper for character chain comparisons (char=?, char<?, etc.)
    fn char_chain_compare<F>(&self, args: ArenaIndex, compare_fn: F, call_expr: ArenaIndex) -> EvalResult
    where
        F: Fn(char, char) -> bool
    {
        // Need at least 2 arguments
        let first = self.get_char(self.lisp.car(args)?, call_expr)?;
        let rest = self.lisp.cdr(args)?;
        
        if self.lisp.get(rest)?.is_nil() {
            return Err(self.type_error(call_expr, "at least 2 arguments", "1 argument"));
        }
        
        let mut prev = first;
        let mut current = rest;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.lisp.true_val().map_err(Into::into),
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    let c = self.get_char(car, call_expr)?;
                    if !compare_fn(prev, c) {
                        return self.lisp.false_val().map_err(Into::into);
                    }
                    prev = c;
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Helper for string chain comparisons (string=?, string<?, etc.)
    fn string_chain_compare<F>(&self, args: ArenaIndex, compare_fn: F, call_expr: ArenaIndex) -> EvalResult
    where
        F: Fn(core::cmp::Ordering) -> bool
    {
        // Need at least 2 arguments
        let first_idx = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        
        if self.lisp.get(rest)?.is_nil() {
            return Err(self.type_error(call_expr, "at least 2 arguments", "1 argument"));
        }
        
        let mut prev_idx = first_idx;
        let mut current = rest;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.lisp.true_val().map_err(Into::into),
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    let ordering = self.compare_strings(prev_idx, car, call_expr)?;
                    if !compare_fn(ordering) {
                        return self.lisp.false_val().map_err(Into::into);
                    }
                    prev_idx = car;
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Compare two strings lexicographically
    fn compare_strings(&self, a: ArenaIndex, b: ArenaIndex, call_expr: ArenaIndex) -> Result<core::cmp::Ordering, EvalError> {
        let (len_a, data_a) = match self.lisp.get(a)? {
            Value::String { len, data } => (len, data),
            v => return Err(self.type_error(call_expr, "string", v.type_name())),
        };
        let (len_b, data_b) = match self.lisp.get(b)? {
            Value::String { len, data } => (len, data),
            v => return Err(self.type_error(call_expr, "string", v.type_name())),
        };
        
        let min_len = if len_a < len_b { len_a } else { len_b };
        
        for i in 0..min_len {
            // Characters start at data (no header with inline length)
            let slot_a = self.lisp.arena_index_at_offset(data_a, i)?;
            let char_a = match self.lisp.get(slot_a)? {
                Value::Char(c) => c,
                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
            };
            let slot_b = self.lisp.arena_index_at_offset(data_b, i)?;
            let char_b = match self.lisp.get(slot_b)? {
                Value::Char(c) => c,
                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
            };
            
            if char_a < char_b {
                return Ok(core::cmp::Ordering::Less);
            } else if char_a > char_b {
                return Ok(core::cmp::Ordering::Greater);
            }
        }
        
        // All compared characters are equal, compare lengths
        if len_a < len_b {
            Ok(core::cmp::Ordering::Less)
        } else if len_a > len_b {
            Ok(core::cmp::Ordering::Greater)
        } else {
            Ok(core::cmp::Ordering::Equal)
        }
    }
    
    /// Evaluate case - pattern matching
    /// (case key ((datum1 ...) expr1 ...) ((datum2 ...) expr2 ...) (else exprn ...))
    fn step_eval_case(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let key_expr = self.lisp.car(args)?;
        let clauses = self.lisp.cdr(args)?;
        
        // Push continuation to check clauses after key is evaluated
        let data_start = self.pack_case_key(clauses, env)?;
        self.push_cont(Cont::CaseKey(data_start))?;
        
        // Evaluate the key expression
        Ok(TrampolineState::Eval { expr: key_expr, env })
    }
    
    /// Evaluate do - iteration construct
    /// (do ((var init step) ...) (test result ...) body ...)
    fn step_eval_do(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let test_clause = self.lisp.car(rest)?;
        let body = self.lisp.cdr(rest)?;
        
        // If no bindings, go straight to the loop
        if self.lisp.get(bindings)?.is_nil() {
            // No variables - just evaluate test
            let nil = self.lisp.nil()?;
            let data_start = self.pack_do_test_result(nil, test_clause, body, env)?;
            self.push_cont(Cont::DoTestResult(data_start))?;
            let test = self.lisp.car(test_clause)?;
            return Ok(TrampolineState::Eval { expr: test, env });
        }
        
        // Get the first binding
        let binding = self.lisp.car(bindings)?;
        let rest_bindings = self.lisp.cdr(bindings)?;
        
        let var = self.lisp.car(binding)?;
        let init_rest = self.lisp.cdr(binding)?;
        let init = self.lisp.car(init_rest)?;
        let step_rest = self.lisp.cdr(init_rest)?;
        let step = if self.lisp.get(step_rest)?.is_nil() {
            var // No step, use variable itself
        } else {
            self.lisp.car(step_rest)?
        };
        
        // Build first (var . step) pair
        let var_step_pair = self.lisp.cons(var, step)?;
        let nil = self.lisp.nil()?;
        let var_steps = self.lisp.cons(var_step_pair, nil)?;
        
        // Push continuation and evaluate first init
        let data_start = self.pack_do_init(rest_bindings, var_steps, test_clause, body, env, env, var)?;
        self.push_cont(Cont::DoInit(data_start))?;
        Ok(TrampolineState::Eval { expr: init, env })
    }
    
    /// Evaluate quasiquote - template with unquote (trampolined version)
    fn eval_quasiquote(&mut self, template: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        self.step_quasiquote_trampoline(template, env, 1)
    }
    
    /// Append two lists
    fn append_lists(&self, a: ArenaIndex, b: ArenaIndex) -> EvalResult {
        match self.lisp.get(a)? {
            Value::Nil => Ok(b),
            Value::Cons { .. } => {
                let car = self.lisp.car(a)?;
                let cdr = self.lisp.cdr(a)?;
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
        
        // Push continuation to evaluate args_list after func is evaluated
        let data_start = self.pack_apply_first(args_list_expr, env)?;
        self.push_cont(Cont::ApplyFirst(data_start))?;
        
        // Evaluate function first
        Ok(TrampolineState::Eval { expr: func_expr, env })
    }
    
    /// Evaluate values - create a multi-value return (trampolined)
    fn eval_values(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        if self.lisp.get(args)?.is_nil() {
            // No values - return empty list
            let nil = self.lisp.nil()?;
            return Ok(TrampolineState::Return { val: nil });
        }
        
        // Start evaluating first value
        let first_expr = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let nil = self.lisp.nil()?;
        
        let data_start = self.pack_values_collect(rest, nil, env)?;
        self.push_cont(Cont::ValuesCollect(data_start))?;
        Ok(TrampolineState::Eval { expr: first_expr, env })
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
    
    /// Evaluate define (trampolined)
    fn eval_define(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let first = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        
        match self.lisp.get(first)? {
            // (define name value)
            Value::Symbol(_) => {
                let value_expr = self.lisp.car(rest)?;
                // Push continuation and evaluate value
                let data_start = self.pack_define_value(first)?;
                self.push_cont(Cont::DefineValue(data_start))?;
                Ok(TrampolineState::Eval { expr: value_expr, env })
            }
            // (define (name params...) body...) -> (define name (lambda (params...) body...))
            Value::Cons { .. } => {
                let name = self.lisp.car(first)?;
                let params = self.lisp.cdr(first)?;
                let body_list = rest;
                let body = if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
                    self.lisp.car(body_list)?
                } else {
                    let begin = self.lisp.symbol("begin")?;
                    self.lisp.cons(begin, body_list)?
                };
                let lambda = self.lisp.lambda(params, body, env)?;
                self.define(name, lambda)?;
                Ok(TrampolineState::Return { val: name })
            }
            _ => Err(self.type_error(first, "symbol or list", self.lisp.get(first)?.type_name())),
        }
    }
    
    /// Evaluate (set! name value) - mutate an existing variable binding (trampolined)
    fn eval_set(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        extract_args!(self, args, name, value_expr);
        
        // Verify name is a symbol
        match self.lisp.get(name)? {
            Value::Symbol(_) => {
                // Push continuation and evaluate value
                let data_start = self.pack_set_value(name, env)?;
                self.push_cont(Cont::SetValue(data_start))?;
                Ok(TrampolineState::Eval { expr: value_expr, env })
            }
            _ => Err(self.type_error(name, "symbol", self.lisp.get(name)?.type_name())),
        }
    }
    
    // ========================================================================
    // Helpers
    // ========================================================================
    
    /// Count elements in a list
    fn count_list(&self, mut list: ArenaIndex) -> Result<usize, EvalError> {
        let mut count = 0;
        loop {
            match self.lisp.get(list)? {
                Value::Nil => return Ok(count),
                Value::Cons { .. } => {
                    count += 1;
                    list = self.lisp.cdr(list)?;
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
    
    // ========================================================================
    // Pack/Unpack helpers for continuation data (using separate data stack)
    // ========================================================================
    //
    // These push/pop ArenaIndex values directly to the data_stack, avoiding
    // arena allocation entirely. No GC pressure, no RefCell overhead.
    
    /// Push values to data stack, return start index
    #[inline]
    fn push_data(&mut self, values: &[ArenaIndex]) -> Result<usize, EvalError> {
        let start = self.data_stack_top;
        let new_top = start + values.len();
        if new_top > MAX_DATA_STACK {
            return Err(EvalError::new(ErrorKind::StackOverflow));
        }
        self.data_stack[start..new_top].copy_from_slice(values);
        self.data_stack_top = new_top;
        Ok(start)
    }
    
    /// Read values from data stack (does not modify stack - pop_cont handles cleanup)
    #[inline]
    fn read_data1(&self, start: usize) -> ArenaIndex {
        self.data_stack[start]
    }
    
    #[inline]
    fn read_data2(&self, start: usize) -> (ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1])
    }
    
    #[inline]
    fn read_data3(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2])
    }
    
    #[inline]
    fn read_data4(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2], self.data_stack[start + 3])
    }
    
    #[inline]
    fn read_data5(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2], self.data_stack[start + 3], self.data_stack[start + 4])
    }
    
    #[inline]
    fn read_data6(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2], self.data_stack[start + 3], self.data_stack[start + 4], self.data_stack[start + 5])
    }
    
    #[inline]
    fn read_data7(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2], self.data_stack[start + 3], self.data_stack[start + 4], self.data_stack[start + 5], self.data_stack[start + 6])
    }
    
    /// Encode Builtin as ArenaIndex (store discriminant as raw usize)
    #[inline]
    fn encode_builtin(builtin: Builtin) -> ArenaIndex {
        ArenaIndex::new(builtin as usize)
    }
    
    /// Decode Builtin from ArenaIndex
    #[inline]
    fn decode_builtin(encoded: ArenaIndex) -> Builtin {
        Builtin::from_usize(encoded.raw())
    }

    // ========================================================================
    // Generated pack/unpack functions (via macro)
    // ========================================================================
    //
    // Simple ArenaIndex-only pack/unpack pairs are generated by this macro.
    // Special cases with Builtin or usize encoding are defined manually below.
    
    define_cont_pack_unpack! {
        // 1-field continuations
        pack_lambda_first_bind / unpack_lambda_first_bind => [param];
        pack_eval_expr / unpack_eval_expr => [env];
        pack_define_value / unpack_define_value => [name];
        pack_quasiquote_cdr / unpack_quasiquote_cdr => [car_val];
        pack_quasiquote_splice_append / unpack_quasiquote_splice_append => [splice_val];
        
        // 2-field continuations
        pack_when / unpack_when => [body, env];
        pack_unless / unpack_unless => [body, env];
        pack_and / unpack_and => [remaining, env];
        pack_or / unpack_or => [remaining, env];
        pack_begin_seq / unpack_begin_seq => [remaining, env];
        pack_case_key / unpack_case_key => [clauses, env];
        pack_apply_first / unpack_apply_first => [args_list_expr, env];
        pack_apply_second / unpack_apply_second => [func, env];
        pack_set_value / unpack_set_value => [name, env];
        
        // 3-field continuations
        pack_apply_forced / unpack_apply_forced => [args_expr, env, call_expr];
        pack_if_branch / unpack_if_branch => [then_expr, else_expr, env];
        pack_cond_test / unpack_cond_test => [then_exprs, remaining_clauses, env];
        pack_values_collect / unpack_values_collect => [remaining, collected, env];
        
        // 4-field continuations
        pack_let_star_binding / unpack_let_star_binding => [remaining_bindings, new_env, body, name];
        pack_letrec_init / unpack_letrec_init => [remaining_bindings, new_env, body, name];
        pack_do_test_result / unpack_do_test_result => [var_steps, test_clause, body, loop_env];
        
        // 5-field continuations
        pack_let_binding / unpack_let_binding => [remaining_bindings, new_env, original_env, body, name];
        pack_do_body / unpack_do_body => [remaining_body, var_steps, test_clause, body, loop_env];
        
        // 6-field continuations
        pack_lambda_bind_arg / unpack_lambda_bind_arg => [remaining_exprs, eval_env, remaining_params, body, new_env, call_expr];
        
        // 7-field continuations
        pack_do_init / unpack_do_init => [remaining_bindings, var_steps, test_clause, body, loop_env, original_env, current_var];
        pack_do_step / unpack_do_step => [remaining_steps, collected_vals, var_steps, test_clause, body, loop_env, current_var]
    }

    // ========================================================================
    // Generated pack/unpack functions for Builtin-first patterns
    // ========================================================================
    
    define_cont_pack_unpack_builtin_first! {
        pack_builtin_force_arg / unpack_builtin_force_arg =>
            builtin, [remaining_args, collected, call_expr, eval_env];
        pack_binary_builtin_first / unpack_binary_builtin_first =>
            builtin, [second_arg, call_expr, eval_env];
        pack_binary_builtin_second / unpack_binary_builtin_second =>
            builtin, [first_val, call_expr]
    }

    // ========================================================================
    // Generated pack/unpack functions with usize field
    // ========================================================================
    
    define_cont_pack_unpack_with_usize! {
        pack_native_args_collect / unpack_native_args_collect =>
            [remaining, collected], id, [env];
        pack_quasiquote_car / unpack_quasiquote_car =>
            [cdr], depth, [env];
        pack_quasiquote_splice / unpack_quasiquote_splice =>
            [cdr], depth, [env]
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
