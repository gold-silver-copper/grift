//! Main evaluator implementation.

use grift_parser::{
    ArenaIndex, ArenaResult, GcStats, Lisp, Value, Builtin, StdLib, parse, ParseError, ParseErrorKind,
};

use crate::error::{
    ErrorKind, ErrorMessage, StackFrame, EvalError, EvalResult,
    MAX_STACK_DEPTH, MAX_BACKTRACE,
};
use crate::num::{Num, fract_f64, abs_f64};
use crate::continuation::{Cont, TrampolineState, is_binary_builtin, MAX_CONT_DEPTH};
use crate::helpers::{
    gcd_helper, int_pow, floor_f64, ceil_f64, trunc_f64, round_f64,
    pow_float, equal_recursive, case_matches,
};
use crate::native::{NativeRegistry, NativeFn, simple_hash};

// Re-export macros from lib.rs (they're defined there)
use crate::{extract_args, builtin_unary_pred};

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
                Cont::LetBinding { remaining_bindings, new_env, original_env, body, name } => {
                    roots[root_count] = remaining_bindings; root_count += 1;
                    roots[root_count] = new_env; root_count += 1;
                    roots[root_count] = original_env; root_count += 1;
                    roots[root_count] = body; root_count += 1;
                    roots[root_count] = name; root_count += 1;
                }
                Cont::LetStarBinding { remaining_bindings, new_env, body, name } => {
                    roots[root_count] = remaining_bindings; root_count += 1;
                    roots[root_count] = new_env; root_count += 1;
                    roots[root_count] = body; root_count += 1;
                    roots[root_count] = name; root_count += 1;
                }
                Cont::LetrecInit { remaining_bindings, new_env, body, name } => {
                    roots[root_count] = remaining_bindings; root_count += 1;
                    roots[root_count] = new_env; root_count += 1;
                    roots[root_count] = body; root_count += 1;
                    roots[root_count] = name; root_count += 1;
                }
                Cont::WhenUnless { body, env, .. } => {
                    roots[root_count] = body; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::EvalExpr { env } => {
                    roots[root_count] = env; root_count += 1;
                }
                Cont::CondTest { then_exprs, remaining_clauses, env } => {
                    roots[root_count] = then_exprs; root_count += 1;
                    roots[root_count] = remaining_clauses; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::AndOr { remaining, env, .. } => {
                    roots[root_count] = remaining; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::BeginSeq { remaining, env } => {
                    roots[root_count] = remaining; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                // New continuation types for fully trampolined evaluation
                Cont::CaseKey { clauses, env } => {
                    roots[root_count] = clauses; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::DoInit { remaining_bindings, var_steps, test_clause, body, loop_env, original_env, current_var } => {
                    roots[root_count] = remaining_bindings; root_count += 1;
                    roots[root_count] = var_steps; root_count += 1;
                    roots[root_count] = test_clause; root_count += 1;
                    roots[root_count] = body; root_count += 1;
                    roots[root_count] = loop_env; root_count += 1;
                    roots[root_count] = original_env; root_count += 1;
                    roots[root_count] = current_var; root_count += 1;
                }
                Cont::DoTestResult { var_steps, test_clause, body, loop_env } => {
                    roots[root_count] = var_steps; root_count += 1;
                    roots[root_count] = test_clause; root_count += 1;
                    roots[root_count] = body; root_count += 1;
                    roots[root_count] = loop_env; root_count += 1;
                }
                Cont::DoBody { remaining_body, var_steps, test_clause, body, loop_env } => {
                    roots[root_count] = remaining_body; root_count += 1;
                    roots[root_count] = var_steps; root_count += 1;
                    roots[root_count] = test_clause; root_count += 1;
                    roots[root_count] = body; root_count += 1;
                    roots[root_count] = loop_env; root_count += 1;
                }
                Cont::DoStep { remaining_steps, collected_vals, var_steps, test_clause, body, loop_env, current_var } => {
                    roots[root_count] = remaining_steps; root_count += 1;
                    roots[root_count] = collected_vals; root_count += 1;
                    roots[root_count] = var_steps; root_count += 1;
                    roots[root_count] = test_clause; root_count += 1;
                    roots[root_count] = body; root_count += 1;
                    roots[root_count] = loop_env; root_count += 1;
                    roots[root_count] = current_var; root_count += 1;
                }
                Cont::ApplyFirst { args_list_expr, env } => {
                    roots[root_count] = args_list_expr; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::ApplySecond { func, env } => {
                    roots[root_count] = func; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::ValuesCollect { remaining, collected, env } => {
                    roots[root_count] = remaining; root_count += 1;
                    roots[root_count] = collected; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::DefineValue { name } => {
                    roots[root_count] = name; root_count += 1;
                }
                Cont::SetValue { name, env } => {
                    roots[root_count] = name; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::NativeArgsCollect { remaining, collected, env, .. } => {
                    roots[root_count] = remaining; root_count += 1;
                    roots[root_count] = collected; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::QuasiquoteCar { cdr, env, .. } => {
                    roots[root_count] = cdr; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::QuasiquoteCdr { car_val } => {
                    roots[root_count] = car_val; root_count += 1;
                }
                Cont::QuasiquoteUnquoteWrap { .. } => {}
                Cont::QuasiquoteNestedWrap => {}
                Cont::QuasiquoteSplice { cdr, env, .. } => {
                    roots[root_count] = cdr; root_count += 1;
                    roots[root_count] = env; root_count += 1;
                }
                Cont::QuasiquoteSpliceAppend { splice_val } => {
                    roots[root_count] = splice_val; root_count += 1;
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
            Value::Number(_) | Value::Float(_) | Value::Char(_) | 
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
                self.push_cont(Cont::WhenUnless { body, env, is_when: true })?;
                return Ok(TrampolineState::Eval { expr: test_expr, env });
            }

            // unless - continuation-based evaluation (R7RS Section 4.2.1)
            if self.lisp.symbol_matches(car, "unless")? {
                let test_expr = self.lisp.car(cdr)?;
                let body = self.lisp.cdr(cdr)?;
                self.push_cont(Cont::WhenUnless { body, env, is_when: false })?;
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
                self.push_cont(Cont::EvalExpr { env: self.global_env })?;
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
        self.push_cont(Cont::ApplyForced { args_expr: cdr, env, call_expr: expr })?;
        
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
        self.push_cont(Cont::CondTest { then_exprs, remaining_clauses: rest_clauses, env })?;

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
                            
                            self.push_cont(Cont::NativeArgsCollect { remaining: rest, collected: nil, id, env })?;
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

            Cont::LetBinding { remaining_bindings, new_env, original_env, body, name } => {
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
                    self.push_cont(Cont::LetBinding {
                        remaining_bindings: rest_bindings,
                        new_env: extended_env,
                        original_env,
                        body,
                        name: next_name,
                    })?;

                    // Evaluate the value expression in the ORIGINAL environment
                    Ok(Some(TrampolineState::Eval { expr: next_value_expr, env: original_env }))
                }
            }

            Cont::LetStarBinding { remaining_bindings, new_env, body, name } => {
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
                    self.push_cont(Cont::LetStarBinding {
                        remaining_bindings: rest_bindings,
                        new_env: extended_env,
                        body,
                        name: next_name,
                    })?;

                    // Evaluate the value expression in the NEW (extended) environment
                    Ok(Some(TrampolineState::Eval { expr: next_value_expr, env: extended_env }))
                }
            }

            Cont::LetrecInit { remaining_bindings, new_env, body, name } => {
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
                    self.push_cont(Cont::LetrecInit {
                        remaining_bindings: rest_bindings,
                        new_env,
                        body,
                        name: next_name,
                    })?;

                    // Evaluate the init expression in the letrec environment
                    Ok(Some(TrampolineState::Eval { expr: next_init_expr, env: new_env }))
                }
            }

            Cont::WhenUnless { body, env, is_when } => {
                // val is the evaluated test result
                let test_passed = !self.is_false(val)?;
                let should_run = if is_when { test_passed } else { !test_passed };

                if should_run {
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

            Cont::EvalExpr { env } => {
                // val is the evaluated expression - now evaluate it
                Ok(Some(TrampolineState::Eval { expr: val, env }))
            }

            Cont::CondTest { then_exprs, remaining_clauses, env } => {
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

            Cont::AndOr { remaining, env, is_and } => {
                // val is the evaluated expression
                if is_and {
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
                        self.push_cont(Cont::AndOr { remaining: rest, env, is_and: true })?;
                        Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                    }
                } else {
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
                        self.push_cont(Cont::AndOr { remaining: rest, env, is_and: false })?;
                        Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                    }
                }
            }

            Cont::BeginSeq { remaining, env } => {
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
                        self.push_cont(Cont::BeginSeq { remaining: rest, env })?;
                        Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                    }
                }
            }

            // ================================================================
            // New continuation types for fully trampolined evaluation
            // ================================================================

            Cont::CaseKey { clauses, env } => {
                // val is the evaluated key - check clauses
                self.step_return_case_key(val, clauses, env)
            }

            Cont::DoInit { remaining_bindings, var_steps, test_clause, body, loop_env, original_env, current_var } => {
                // val is the evaluated init expression - bind and continue
                let extended_env = self.env_extend(loop_env, current_var, val)?;
                self.step_return_do_init(remaining_bindings, var_steps, test_clause, body, extended_env, original_env)
            }

            Cont::DoTestResult { var_steps, test_clause, body, loop_env } => {
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
                        self.push_cont(Cont::DoBody { remaining_body: rest_body, var_steps, test_clause, body, loop_env })?;
                        Ok(Some(TrampolineState::Eval { expr: first_body, env: loop_env }))
                    }
                }
            }

            Cont::DoBody { remaining_body, var_steps, test_clause, body, loop_env } => {
                // val is discarded (body evaluated for side effects)
                if self.lisp.get(remaining_body)?.is_nil() {
                    // Body done - start evaluating step expressions
                    self.step_do_start_steps(var_steps, test_clause, body, loop_env)
                } else {
                    // More body expressions
                    let next_body = self.lisp.car(remaining_body)?;
                    let rest_body = self.lisp.cdr(remaining_body)?;
                    self.push_cont(Cont::DoBody { remaining_body: rest_body, var_steps, test_clause, body, loop_env })?;
                    Ok(Some(TrampolineState::Eval { expr: next_body, env: loop_env }))
                }
            }

            Cont::DoStep { remaining_steps, collected_vals, var_steps, test_clause, body, loop_env, current_var } => {
                // val is the evaluated step expression - collect and continue
                let new_collected = self.lisp.cons(current_var, val)?;
                let new_collected = self.lisp.cons(new_collected, collected_vals)?;
                
                if self.lisp.get(remaining_steps)?.is_nil() {
                    // All steps evaluated - update environment and loop
                    let new_env = self.apply_do_step_values(loop_env, new_collected)?;
                    // Continue to next iteration - evaluate test
                    self.push_cont(Cont::DoTestResult { var_steps, test_clause, body, loop_env: new_env })?;
                    let test = self.lisp.car(test_clause)?;
                    Ok(Some(TrampolineState::Eval { expr: test, env: new_env }))
                } else {
                    // More steps to evaluate
                    let next_pair = self.lisp.car(remaining_steps)?;
                    let rest_steps = self.lisp.cdr(remaining_steps)?;
                    let next_var = self.lisp.car(next_pair)?;
                    let next_step = self.lisp.cdr(next_pair)?;
                    
                    self.push_cont(Cont::DoStep { 
                        remaining_steps: rest_steps, collected_vals: new_collected, 
                        var_steps, test_clause, body, loop_env, current_var: next_var 
                    })?;
                    Ok(Some(TrampolineState::Eval { expr: next_step, env: loop_env }))
                }
            }

            Cont::ApplyFirst { args_list_expr, env } => {
                // val is the evaluated function - now evaluate args list
                self.push_cont(Cont::ApplySecond { func: val, env })?;
                Ok(Some(TrampolineState::Eval { expr: args_list_expr, env }))
            }

            Cont::ApplySecond { func, env } => {
                // val is the evaluated args list - perform application
                let args_list = val;
                let call_expr = self.lisp.cons(func, args_list)?;
                self.push_frame(call_expr, func)?;
                self.push_cont(Cont::ApplyForced { args_expr: args_list, env, call_expr })?;
                Ok(Some(TrampolineState::Return { val: func }))
            }

            Cont::ValuesCollect { remaining, collected, env } => {
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
                    self.push_cont(Cont::ValuesCollect { remaining: rest, collected: new_collected, env })?;
                    Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                }
            }

            Cont::DefineValue { name } => {
                // val is the evaluated value - define the binding
                self.define(name, val)?;
                Ok(Some(TrampolineState::Return { val: name }))
            }

            Cont::SetValue { name, env } => {
                // val is the evaluated value - set! the binding
                self.env_set(env, name, val)?;
                Ok(Some(TrampolineState::Return { val }))
            }

            Cont::NativeArgsCollect { remaining, collected, id, env } => {
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
                    self.push_cont(Cont::NativeArgsCollect { remaining: rest, collected: new_collected, id, env })?;
                    Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                }
            }

            Cont::QuasiquoteCar { cdr, depth, env } => {
                // val is the evaluated car - now process cdr
                let car_val = val;
                self.push_cont(Cont::QuasiquoteCdr { car_val })?;
                Ok(Some(self.step_quasiquote_trampoline(cdr, env, depth)?))
            }

            Cont::QuasiquoteCdr { car_val } => {
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

            Cont::QuasiquoteSplice { cdr, depth, env } => {
                // val is the evaluated splice expression - process cdr then append
                let splice_val = val;
                self.push_cont(Cont::QuasiquoteSpliceAppend { splice_val })?;
                Ok(Some(self.step_quasiquote_trampoline(cdr, env, depth)?))
            }

            Cont::QuasiquoteSpliceAppend { splice_val } => {
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
        self.push_cont(Cont::CondTest { then_exprs, remaining_clauses: rest_clauses, env })?;

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
                Value::Cons { car: clause, cdr: rest } => {
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
            self.push_cont(Cont::DoTestResult { var_steps, test_clause, body, loop_env })?;
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
            self.push_cont(Cont::DoInit { 
                remaining_bindings: rest, 
                var_steps: new_var_steps, 
                test_clause, 
                body, 
                loop_env, 
                original_env, 
                current_var: var 
            })?;
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
            self.push_cont(Cont::DoTestResult { var_steps, test_clause, body, loop_env })?;
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
            self.push_cont(Cont::DoStep { 
                remaining_steps: rest_steps, 
                collected_vals: nil, 
                var_steps: reversed,  // Use reversed for future iterations
                test_clause, 
                body, 
                loop_env, 
                current_var: first_var 
            })?;
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
                Value::Cons { car: pair, cdr: rest } => {
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
    fn step_quasiquote_trampoline(&mut self, template: ArenaIndex, env: ArenaIndex, depth: u8) 
        -> Result<TrampolineState, EvalError> 
    {
        match self.lisp.get(template)? {
            Value::Cons { car, cdr } => {
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
                if self.lisp.symbol_matches(car, "unquote-splicing").unwrap_or(false) {
                    if depth == 1 {
                        // Return the evaluated list (caller handles splicing)
                        let inner_expr = self.lisp.car(cdr)?;
                        return Ok(TrampolineState::Eval { expr: inner_expr, env });
                    }
                }
                
                // Check for nested quasiquote
                if self.lisp.symbol_matches(car, "quasiquote").unwrap_or(false) {
                    self.push_cont(Cont::QuasiquoteNestedWrap)?;
                    let inner_expr = self.lisp.car(cdr)?;
                    return self.step_quasiquote_trampoline(inner_expr, env, depth + 1);
                }
                
                // Check for unquote-splicing in car position (special handling)
                if let Value::Cons { car: inner_car, cdr: inner_cdr } = self.lisp.get(car)? {
                    if self.lisp.symbol_matches(inner_car, "unquote-splicing").unwrap_or(false) && depth == 1 {
                        // Splice the result into the list
                        let splice_expr = self.lisp.car(inner_cdr)?;
                        self.push_cont(Cont::QuasiquoteSplice { cdr, depth, env })?;
                        return Ok(TrampolineState::Eval { expr: splice_expr, env });
                    }
                }
                
                // Recursively process car and cdr
                self.push_cont(Cont::QuasiquoteCar { cdr, depth, env })?;
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
        self.push_cont(Cont::LetBinding {
            remaining_bindings: rest_bindings,
            new_env: env,
            original_env: env,
            body,
            name,
        })?;

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
        self.push_cont(Cont::LetStarBinding {
            remaining_bindings: rest_bindings,
            new_env: env,
            body,
            name,
        })?;

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
                Value::Cons { car: binding, cdr: rest } => {
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
        self.push_cont(Cont::LetrecInit {
            remaining_bindings: rest_bindings,
            new_env,
            body,
            name,
        })?;

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
            self.push_cont(Cont::BeginSeq { remaining: rest, env })?;
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
            self.push_cont(Cont::AndOr { remaining: rest, env, is_and: true })?;
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
            self.push_cont(Cont::AndOr { remaining: rest, env, is_and: false })?;
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
                    (Value::Float(x), Value::Float(y)) => x == y,
                    (Value::Number(x), Value::Float(y)) | (Value::Float(y), Value::Number(x)) => x as f64 == y,
                    (Value::Char(x), Value::Char(y)) => x == y,
                    (Value::Symbol { .. }, Value::Symbol { .. }) => self.lisp.symbol_eq(a, b)?,
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
            
            Builtin::Add => self.numeric_fold_mixed(args, Num::Int(0), 
                |a, b| a.checked_add(b), 
                |a, b| a + b, 
                call_expr),
            
            Builtin::Sub => {
                let first = self.get_num(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                if self.lisp.get(rest)?.is_nil() {
                    // Unary minus
                    match first {
                        Num::Int(n) => self.lisp.number(-n).map_err(Into::into),
                        Num::Float(f) => self.lisp.float(-f).map_err(Into::into),
                    }
                } else {
                    self.numeric_fold_mixed(rest, first, 
                        |a, b| a.checked_sub(b), 
                        |a, b| a - b, 
                        call_expr)
                }
            }
            
            Builtin::Mul => self.numeric_fold_mixed(args, Num::Int(1), 
                |a, b| a.checked_mul(b), 
                |a, b| a * b, 
                call_expr),
            
            Builtin::Div => {
                // Division always produces float for consistency with Scheme
                let first = self.get_num(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold_mixed(rest, first, 
                    |a, b| if b == 0 { None } else { a.checked_div(b) },
                    |a, b| a / b,
                    call_expr)
            }
            
            Builtin::Modulo => {
                // Scheme modulo: result has the sign of the divisor
                let a = self.get_int(self.lisp.car(args)?, call_expr)?;
                let b = self.get_int(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                if b == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                // Scheme modulo: ((a % b) + b) % b
                let result = ((a % b) + b) % b;
                self.lisp.number(result).map_err(Into::into)
            }
            
            Builtin::Remainder => {
                // Scheme remainder: result has the sign of the dividend
                let a = self.get_int(self.lisp.car(args)?, call_expr)?;
                let b = self.get_int(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                if b == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                // Rust's % operator already gives remainder with sign of dividend
                self.lisp.number(a % b).map_err(Into::into)
            }
            
            Builtin::Quotient => {
                // Integer quotient (truncated towards zero)
                let a = self.get_int(self.lisp.car(args)?, call_expr)?;
                let b = self.get_int(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                if b == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                // Rust's / operator truncates towards zero for integers
                self.lisp.number(a / b).map_err(Into::into)
            }
            
            Builtin::Abs => {
                // Absolute value
                let n = self.get_num(self.lisp.car(args)?, call_expr)?;
                match n {
                    Num::Int(i) => self.lisp.number(i.abs()).map_err(Into::into),
                    Num::Float(f) => self.lisp.float(abs_f64(f)).map_err(Into::into),
                }
            }
            
            Builtin::Max => {
                // Maximum of one or more numbers
                let first = self.get_num(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold_mixed(rest, first, 
                    |a, b| Some(if a > b { a } else { b }),
                    |a, b| if a > b { a } else { b },
                    call_expr)
            }
            
            Builtin::Min => {
                // Minimum of one or more numbers
                let first = self.get_num(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold_mixed(rest, first,
                    |a, b| Some(if a < b { a } else { b }),
                    |a, b| if a < b { a } else { b },
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
                        Value::Cons { car, cdr } => {
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
                        Value::Cons { car, cdr } => {
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
                let base = self.get_num(self.lisp.car(args)?, call_expr)?;
                let power = self.get_num(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                
                match (base, power) {
                    (Num::Int(b), Num::Int(p)) => {
                        if p < 0 {
                            // Negative integer exponent produces float result
                            if b == 0 {
                                return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                            }
                            let result = 1.0 / int_pow(b, (-p) as usize) as f64;
                            self.lisp.float(result).map_err(Into::into)
                        } else {
                            let result = int_pow(b, p as usize);
                            self.lisp.number(result).map_err(Into::into)
                        }
                    }
                    (b, p) => {
                        // Float exponentiation using exp(p * ln(b))
                        let b_f = b.to_f64();
                        let p_f = p.to_f64();
                        
                        if b_f == 0.0 {
                            if p_f > 0.0 {
                                return self.lisp.float(0.0).map_err(Into::into);
                            } else if p_f == 0.0 {
                                return self.lisp.float(1.0).map_err(Into::into);
                            } else {
                                return self.lisp.float(f64::INFINITY).map_err(Into::into);
                            }
                        }
                        
                        if b_f < 0.0 && fract_f64(p_f) != 0.0 {
                            // Complex result - return NaN
                            return self.lisp.float(f64::NAN).map_err(Into::into);
                        }
                        
                        // Use x^y = exp(y * ln(x)) - but we can't use libm
                        // For now, handle integer powers and simple cases
                        if fract_f64(p_f) == 0.0 && abs_f64(p_f) < 1000.0 {
                            let exp_int = p_f as i64;
                            if exp_int >= 0 {
                                let mut result = 1.0;
                                for _ in 0..exp_int {
                                    result *= b_f;
                                }
                                self.lisp.float(result).map_err(Into::into)
                            } else {
                                let mut result = 1.0;
                                for _ in 0..(-exp_int) {
                                    result *= b_f;
                                }
                                self.lisp.float(1.0 / result).map_err(Into::into)
                            }
                        } else {
                            // For non-integer powers, we need exp and log which are in stdlib
                            // For now, use iterative approximation for positive bases
                            let result = pow_float(b_f, p_f);
                            self.lisp.float(result).map_err(Into::into)
                        }
                    }
                }
            }
            
            Builtin::Square => {
                // Square of a number
                let n = self.get_num(self.lisp.car(args)?, call_expr)?;
                match n {
                    Num::Int(i) => self.lisp.number(i.saturating_mul(i)).map_err(Into::into),
                    Num::Float(f) => self.lisp.float(f * f).map_err(Into::into),
                }
            }
            
            // Numeric predicates
            Builtin::Zerop => {
                let n = self.get_num(self.lisp.car(args)?, call_expr)?;
                let is_zero = match n {
                    Num::Int(i) => i == 0,
                    Num::Float(f) => f == 0.0,
                };
                self.lisp.boolean(is_zero).map_err(Into::into)
            }
            
            Builtin::Positivep => {
                let n = self.get_num(self.lisp.car(args)?, call_expr)?;
                let is_pos = match n {
                    Num::Int(i) => i > 0,
                    Num::Float(f) => f > 0.0,
                };
                self.lisp.boolean(is_pos).map_err(Into::into)
            }
            
            Builtin::Negativep => {
                let n = self.get_num(self.lisp.car(args)?, call_expr)?;
                let is_neg = match n {
                    Num::Int(i) => i < 0,
                    Num::Float(f) => f < 0.0,
                };
                self.lisp.boolean(is_neg).map_err(Into::into)
            }
            
            Builtin::Oddp => {
                let n = self.get_int(self.lisp.car(args)?, call_expr)?;
                self.lisp.boolean(n % 2 != 0).map_err(Into::into)
            }
            
            Builtin::Evenp => {
                let n = self.get_int(self.lisp.car(args)?, call_expr)?;
                self.lisp.boolean(n % 2 == 0).map_err(Into::into)
            }
            
            Builtin::Integerp => {
                let val = self.lisp.car(args)?;
                let is_int = match self.lisp.get(val)? {
                    Value::Number(_) => true,
                    Value::Float(f) => fract_f64(f) == 0.0 && f.is_finite(),
                    _ => false,
                };
                self.lisp.boolean(is_int).map_err(Into::into)
            }
            
            Builtin::Exactp => {
                // Exact numbers are integers
                let val = self.lisp.car(args)?;
                let is_exact = matches!(self.lisp.get(val)?, Value::Number(_));
                self.lisp.boolean(is_exact).map_err(Into::into)
            }
            
            Builtin::Inexactp => {
                // Inexact numbers are floats
                let val = self.lisp.car(args)?;
                let is_inexact = matches!(self.lisp.get(val)?, Value::Float(_));
                match self.lisp.get(val)? {
                    Value::Number(_) | Value::Float(_) => self.lisp.boolean(is_inexact).map_err(Into::into),
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::ExactIntegerp => {
                // exact-integer? returns #t if z is both exact and an integer
                let val = self.lisp.car(args)?;
                let is_exact_int = matches!(self.lisp.get(val)?, Value::Number(_));
                self.lisp.boolean(is_exact_int).map_err(Into::into)
            }
            
            // Rounding operations
            Builtin::Floor => {
                // floor: largest integer not greater than x
                let n = self.get_num(self.lisp.car(args)?, call_expr)?;
                match n {
                    Num::Int(i) => self.lisp.number(i).map_err(Into::into),
                    Num::Float(f) => self.lisp.float(floor_f64(f)).map_err(Into::into),
                }
            }
            
            Builtin::Ceiling => {
                // ceiling: smallest integer not less than x
                let n = self.get_num(self.lisp.car(args)?, call_expr)?;
                match n {
                    Num::Int(i) => self.lisp.number(i).map_err(Into::into),
                    Num::Float(f) => self.lisp.float(ceil_f64(f)).map_err(Into::into),
                }
            }
            
            Builtin::Truncate => {
                // truncate: integer closest to x whose absolute value is not larger
                let n = self.get_num(self.lisp.car(args)?, call_expr)?;
                match n {
                    Num::Int(i) => self.lisp.number(i).map_err(Into::into),
                    Num::Float(f) => self.lisp.float(trunc_f64(f)).map_err(Into::into),
                }
            }
            
            Builtin::Round => {
                // round: closest integer to x, rounding to even when x is halfway
                let n = self.get_num(self.lisp.car(args)?, call_expr)?;
                match n {
                    Num::Int(i) => self.lisp.number(i).map_err(Into::into),
                    Num::Float(f) => self.lisp.float(round_f64(f)).map_err(Into::into),
                }
            }
            
            Builtin::Lt => self.compare_numbers_mixed(args, |a, b| a < b, call_expr),
            Builtin::Gt => self.compare_numbers_mixed(args, |a, b| a > b, call_expr),
            Builtin::Le => self.compare_numbers_mixed(args, |a, b| a <= b, call_expr),
            Builtin::Ge => self.compare_numbers_mixed(args, |a, b| a >= b, call_expr),
            Builtin::NumEq => self.compare_numbers_mixed(args, |a, b| a == b, call_expr),
            
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
                        Value::Cons { cdr, .. } => {
                            count += 1;
                            current = cdr;
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
                    Value::Array { len, .. } => {
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
                    Value::Array { len, .. } => {
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
                        Value::Cons { cdr, .. } => {
                            count += 1;
                            current = cdr;
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
                    Value::Array { len, .. } => {
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
                    Value::Array { len, .. } => {
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
                for i in 0..len {
                    chars[i] = fill;
                }
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
                        Value::Cons { car, cdr } => {
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
                    Value::String { len, .. } => self.lisp.number(len as isize).map_err(Into::into),
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringRef => {
                // (string-ref string k) - Get character at index
                extract_args!(self, args, str_idx, k_idx);
                match self.lisp.get(str_idx)? {
                    Value::String { data, len } => {
                        let k = self.get_int(k_idx, call_expr)?;
                        if k < 0 || (k as usize) >= len {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
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
                    Value::String { data, len } => {
                        let k = self.get_int(k_idx, call_expr)?;
                        if k < 0 || (k as usize) >= len {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        let c = self.get_char(char_arg, call_expr)?;
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
                        Value::Cons { car, cdr } => {
                            match self.lisp.get(car)? {
                                Value::String { data, len } => {
                                    if total_len + len > MAX_TOTAL_LEN {
                                        return Err(self.make_error(ErrorKind::TypeError, call_expr));
                                    }
                                    for i in 0..len {
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
                    Value::String { data, len } => {
                        let mut result = self.lisp.nil()?;
                        // Build list from end to start
                        for i in (0..len).rev() {
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
                        Value::Cons { car, cdr } => {
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
                    Value::String { data, len } => {
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
                    Value::String { data, len } => {
                        const MAX_STRING_LEN: usize = 1024;
                        let mut chars = ['\0'; MAX_STRING_LEN];
                        if len > MAX_STRING_LEN {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        
                        for i in 0..len {
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
                let x = self.get_num(a, call_expr)?;
                let y = self.get_num(b, call_expr)?;
                match (x, y) {
                    (Num::Int(xi), Num::Int(yi)) => {
                        match xi.checked_add(yi) {
                            Some(n) => self.lisp.number(n).map_err(Into::into),
                            None => Err(self.make_error(ErrorKind::DivisionByZero, call_expr)),
                        }
                    }
                    _ => self.lisp.float(x.to_f64() + y.to_f64()).map_err(Into::into),
                }
            }
            Builtin::Sub => {
                let x = self.get_num(a, call_expr)?;
                let y = self.get_num(b, call_expr)?;
                match (x, y) {
                    (Num::Int(xi), Num::Int(yi)) => {
                        match xi.checked_sub(yi) {
                            Some(n) => self.lisp.number(n).map_err(Into::into),
                            None => Err(self.make_error(ErrorKind::DivisionByZero, call_expr)),
                        }
                    }
                    _ => self.lisp.float(x.to_f64() - y.to_f64()).map_err(Into::into),
                }
            }
            Builtin::Mul => {
                let x = self.get_num(a, call_expr)?;
                let y = self.get_num(b, call_expr)?;
                match (x, y) {
                    (Num::Int(xi), Num::Int(yi)) => {
                        match xi.checked_mul(yi) {
                            Some(n) => self.lisp.number(n).map_err(Into::into),
                            None => Err(self.make_error(ErrorKind::DivisionByZero, call_expr)),
                        }
                    }
                    _ => self.lisp.float(x.to_f64() * y.to_f64()).map_err(Into::into),
                }
            }
            Builtin::Div => {
                let x = self.get_num(a, call_expr)?;
                let y = self.get_num(b, call_expr)?;
                match (x, y) {
                    (Num::Int(xi), Num::Int(yi)) => {
                        if yi == 0 {
                            return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                        }
                        self.lisp.number(xi / yi).map_err(Into::into)
                    }
                    _ => {
                        let yf = y.to_f64();
                        if yf == 0.0 {
                            return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                        }
                        self.lisp.float(x.to_f64() / yf).map_err(Into::into)
                    }
                }
            }
            Builtin::Modulo => {
                // Scheme modulo: result has the sign of the divisor (integers only)
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                if y == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                let result = ((x % y) + y) % y;
                self.lisp.number(result).map_err(Into::into)
            }
            Builtin::Remainder => {
                // Scheme remainder: result has the sign of the dividend (integers only)
                let x = self.get_int(a, call_expr)?;
                let y = self.get_int(b, call_expr)?;
                if y == 0 {
                    return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                }
                self.lisp.number(x % y).map_err(Into::into)
            }
            Builtin::Lt => {
                let x = self.get_num(a, call_expr)?;
                let y = self.get_num(b, call_expr)?;
                self.lisp.boolean(x.to_f64() < y.to_f64()).map_err(Into::into)
            }
            Builtin::Gt => {
                let x = self.get_num(a, call_expr)?;
                let y = self.get_num(b, call_expr)?;
                self.lisp.boolean(x.to_f64() > y.to_f64()).map_err(Into::into)
            }
            Builtin::Le => {
                let x = self.get_num(a, call_expr)?;
                let y = self.get_num(b, call_expr)?;
                self.lisp.boolean(x.to_f64() <= y.to_f64()).map_err(Into::into)
            }
            Builtin::Ge => {
                let x = self.get_num(a, call_expr)?;
                let y = self.get_num(b, call_expr)?;
                self.lisp.boolean(x.to_f64() >= y.to_f64()).map_err(Into::into)
            }
            Builtin::NumEq => {
                let x = self.get_num(a, call_expr)?;
                let y = self.get_num(b, call_expr)?;
                self.lisp.boolean(x.to_f64() == y.to_f64()).map_err(Into::into)
            }
            Builtin::EqP | Builtin::EqvP => {
                let val_a = self.lisp.get(a)?;
                let val_b = self.lisp.get(b)?;
                
                let eq = match (val_a, val_b) {
                    (Value::Nil, Value::Nil) => true,
                    (Value::True, Value::True) => true,
                    (Value::False, Value::False) => true,
                    (Value::Number(x), Value::Number(y)) => x == y,
                    (Value::Float(x), Value::Float(y)) => x == y,
                    (Value::Number(x), Value::Float(y)) | (Value::Float(y), Value::Number(x)) => x as f64 == y,
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
    
    /// Get number from already-evaluated value as a Num
    fn get_num(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<Num, EvalError> {
        match self.lisp.get(idx)? {
            Value::Number(n) => Ok(Num::Int(n)),
            Value::Float(f) => Ok(Num::Float(f)),
            v => Err(self.type_error(call_expr, "number", v.type_name())),
        }
    }
    
    /// Get integer from already-evaluated value (for operations that require integers)
    fn get_int(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<isize, EvalError> {
        match self.lisp.get(idx)? {
            Value::Number(n) => Ok(n),
            Value::Float(f) => {
                // Allow floats that are exact integers
                if fract_f64(f) == 0.0 && f >= isize::MIN as f64 && f <= isize::MAX as f64 {
                    Ok(f as isize)
                } else {
                    Err(self.type_error(call_expr, "integer", "inexact number"))
                }
            }
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
    
    /// Allocate a Num value in the arena
    fn alloc_num(&self, n: Num) -> ArenaResult<ArenaIndex> {
        match n {
            Num::Int(i) => self.lisp.number(i),
            Num::Float(f) => self.lisp.float(f),
        }
    }
    
    /// Numeric fold with already-evaluated args (supports mixed int/float)
    fn numeric_fold_mixed<F, G>(&self, args: ArenaIndex, init: Num, int_f: F, float_f: G, call_expr: ArenaIndex) -> EvalResult
    where 
        F: Fn(isize, isize) -> Option<isize>,
        G: Fn(f64, f64) -> f64,
    {
        let mut acc = init;
        let mut current = args;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.alloc_num(acc).map_err(Into::into),
                Value::Cons { car, cdr } => {
                    let n = self.get_num(car, call_expr)?;
                    acc = match (acc, n) {
                        (Num::Int(a), Num::Int(b)) => {
                            match int_f(a, b) {
                                Some(r) => Num::Int(r),
                                None => return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)),
                            }
                        }
                        (a, b) => Num::Float(float_f(a.to_f64(), b.to_f64())),
                    };
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Compare two numbers with already-evaluated args (supports mixed int/float)
    fn compare_numbers_mixed<F>(&self, args: ArenaIndex, cmp: F, call_expr: ArenaIndex) -> EvalResult
    where F: Fn(f64, f64) -> bool
    {
        let a = self.get_num(self.lisp.car(args)?, call_expr)?;
        let b = self.get_num(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
        self.lisp.boolean(cmp(a.to_f64(), b.to_f64())).map_err(Into::into)
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
                Value::Cons { car, cdr } => {
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
                Value::Cons { car, cdr } => {
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
        let (data_a, len_a) = match self.lisp.get(a)? {
            Value::String { data, len } => (data, len),
            v => return Err(self.type_error(call_expr, "string", v.type_name())),
        };
        let (data_b, len_b) = match self.lisp.get(b)? {
            Value::String { data, len } => (data, len),
            v => return Err(self.type_error(call_expr, "string", v.type_name())),
        };
        
        let min_len = if len_a < len_b { len_a } else { len_b };
        
        for i in 0..min_len {
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
        self.push_cont(Cont::CaseKey { clauses, env })?;
        
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
            self.push_cont(Cont::DoTestResult { var_steps: nil, test_clause, body, loop_env: env })?;
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
        self.push_cont(Cont::DoInit { 
            remaining_bindings: rest_bindings, 
            var_steps, 
            test_clause, 
            body, 
            loop_env: env,  // Will be extended after init is evaluated
            original_env: env, 
            current_var: var 
        })?;
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
        
        // Push continuation to evaluate args_list after func is evaluated
        self.push_cont(Cont::ApplyFirst { args_list_expr, env })?;
        
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
        
        self.push_cont(Cont::ValuesCollect { remaining: rest, collected: nil, env })?;
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
            Value::Symbol { .. } => {
                let value_expr = self.lisp.car(rest)?;
                // Push continuation and evaluate value
                self.push_cont(Cont::DefineValue { name: first })?;
                Ok(TrampolineState::Eval { expr: value_expr, env })
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
            Value::Symbol { .. } => {
                // Push continuation and evaluate value
                self.push_cont(Cont::SetValue { name, env })?;
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
