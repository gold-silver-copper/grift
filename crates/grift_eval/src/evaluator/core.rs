//! Core evaluator implementation.
//!
//! Contains the Evaluator constructor, GC, environment management,
//! stack management, trampoline loop, and basic evaluation steps.

use grift_parser::{
    ArenaIndex, GcStats, Lisp, Value, Builtin, StdLib, parse, parse_all, ParseError, ParseErrorKind,
};

use crate::error::{
    ErrorKind, StackFrame, EvalError, EvalResult,
    MAX_STACK_DEPTH,
};
use crate::continuation::{Cont, TrampolineState, MAX_CONT_DEPTH, MAX_DATA_STACK};
use crate::native::{NativeRegistry, NativeFn, simple_hash};
use crate::{
    define_cont_pack_unpack, define_cont_pack_unpack_builtin_first, define_cont_pack_unpack_with_usize,
};

use super::Evaluator;

/// Standard macro definitions (loaded at startup)
const STANDARD_MACROS: &str = include_str!("macros.scm");

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
            macro_env: ArenaIndex::NIL,
            gensym_counter: 0,
        };
        
        // Initialize global environment with builtins
        eval.global_env = lisp.nil()?;
        
        // Initialize macro environment
        eval.macro_env = lisp.nil()?;
        
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
        
        // Load standard macros
        eval.load_standard_macros()?;
        
        Ok(eval)
    }
    
    /// Load standard macro definitions from macros.scm
    /// 
    /// These are evaluated (not just expanded) since define-syntax
    /// is now handled during evaluation.
    fn load_standard_macros(&mut self) -> Result<(), EvalError> {
        let forms = parse_all(self.lisp, STANDARD_MACROS)?;
        let mut current = forms;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let form = self.lisp.car(current)?;
            // Evaluate the form - define-syntax is handled during evaluation
            self.eval(form)?;
            current = self.lisp.cdr(current)?;
        }
        Ok(())
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
        self.lisp.gc(&[self.global_env, self.macro_env])
    }
    
    /// Run GC during evaluation - marks continuation stack AND current state as roots
    pub(super) fn gc_with_state(&self, state: &TrampolineState) -> GcStats {
        // Collect all roots: global env + macro env + current state + all ArenaIndex values in continuations
        const MAX_ROOTS: usize = 512;
        let mut roots = [ArenaIndex::NIL; MAX_ROOTS];
        let mut root_count = 0;
        
        // Always include global env and macro env
        roots[root_count] = self.global_env;
        root_count += 1;
        roots[root_count] = self.macro_env;
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
                    Cont::LambdaRestCollect(data_start) |
                    Cont::EvalExpr(data_start) |
                    Cont::BeginSeq(data_start) |
                    // Note: CaseKey, DoInit, DoTestResult, DoBody, DoStep removed - now handled by macros (Phase 9)
                    Cont::ApplyFirst(data_start) |
                    Cont::ApplySecond(data_start) |
                    Cont::ValuesCollect(data_start) |
                    Cont::DefineValue(data_start) |
                    Cont::SetValue(data_start) |
                    Cont::NativeArgsCollect(data_start) |
                    Cont::QuasiquoteCar(data_start) |
                    Cont::QuasiquoteCdr(data_start) |
                    Cont::QuasiquoteSplice(data_start) |
                    Cont::QuasiquoteSpliceAppend(data_start) |
                    Cont::LetSyntaxBody(data_start) |
                    Cont::CallWithValuesProducer(data_start) |
                    Cont::CallWithValuesConsumer(data_start) |
                    Cont::CallWithValuesApply(data_start) |
                    Cont::SyntaxCaseMatch(data_start) |
                    Cont::SyntaxCaseFender(data_start) |
                    Cont::CallCcApply(data_start) |
                    Cont::ContinuationApply(data_start) => data_start,
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
    
    pub(super) fn push_frame(&mut self, expr: ArenaIndex, func: ArenaIndex) -> Result<(), EvalError> {
        if self.call_stack_depth >= MAX_STACK_DEPTH {
            return Err(self.make_error(ErrorKind::StackOverflow, expr));
        }
        self.call_stack[self.call_stack_depth] = StackFrame { expr, func };
        self.call_stack_depth += 1;
        Ok(())
    }
    
    pub(super) fn pop_frame(&mut self) {
        if self.call_stack_depth > 0 {
            self.call_stack_depth -= 1;
        }
    }
    
    pub(crate) fn make_error(&self, kind: ErrorKind, expr: ArenaIndex) -> EvalError {
        EvalError::new(kind)
            .with_expr(expr)
    }
    
    pub(crate) fn type_error(&self, expr: ArenaIndex, expected: &'static str, got: &'static str) -> EvalError {
        self.make_error(ErrorKind::TypeError, expr)
            .with_types(expected, got)
    }
    
    pub(super) fn arg_error(&self, expr: ArenaIndex, expected: usize, got: usize) -> EvalError {
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
    pub(super) fn env_lookup(&self, env: ArenaIndex, name: ArenaIndex) -> EvalResult {
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
    
    /// Check if a variable is bound in the environment (local or global)
    /// Returns true if the variable exists, false otherwise.
    /// This is used to determine if a variable binding shadows a macro.
    pub(super) fn is_variable_bound(&self, env: ArenaIndex, name: ArenaIndex) -> Result<bool, EvalError> {
        // Check local environment first
        if self.env_contains(env, name)? {
            return Ok(true);
        }
        
        // Check global environment
        self.env_contains(self.global_env, name)
    }
    
    /// Helper to check if a name exists in a specific environment chain
    fn env_contains(&self, mut env: ArenaIndex, name: ArenaIndex) -> Result<bool, EvalError> {
        loop {
            match self.lisp.get(env)? {
                Value::Nil => {
                    return Ok(false);
                }
                Value::Cons { car, cdr } => {
                    if let Value::Cons { car: bound_name, cdr: _ } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        return Ok(true);
                    }
                    env = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::Generic, name)),
            }
        }
    }
    
    /// Set a variable in an environment (mutation operation)
    /// Searches both local and global environments
    /// Returns the new value on success
    pub(super) fn env_set(&self, env: ArenaIndex, name: ArenaIndex, value: ArenaIndex) -> EvalResult {
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
    pub(super) fn push_cont(&mut self, cont: Cont) -> Result<(), EvalError> {
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
    pub(super) fn pop_cont(&mut self) -> Cont {
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
    /// 
    /// Macros are now expanded during evaluation (not pre-processed).
    /// This enables evaluation-time macro expansion per the R7RS model.
    pub fn eval(&mut self, expr: ArenaIndex) -> EvalResult {
        // Reset continuation stack and data stack
        self.cont_depth = 0;
        self.data_stack_top = 0;
        // Start evaluation - macros are expanded on-demand during eval
        self.trampoline(TrampolineState::Eval { expr, env: self.global_env })
    }
    
    /// Evaluate an already-expanded expression (internal)
    /// 
    /// This is now equivalent to `eval()` since macro expansion 
    /// happens during evaluation. Kept for API compatibility.
    pub fn eval_expanded(&mut self, expr: ArenaIndex) -> EvalResult {
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
        // Reset continuation stack and run - macros expanded during eval
        self.cont_depth = 0;
        self.data_stack_top = 0;
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
        const GC_CHECK_INTERVAL: usize = 500;
        const GC_THRESHOLD_PERCENT: usize = 60;
        
        loop {
            // Aggressive periodic GC check
            step_count = step_count.wrapping_add(1);
            if step_count % GC_CHECK_INTERVAL == 0 {
                let stats = self.lisp.stats();
                // Compare allocated >= capacity * threshold / 100 to avoid overflow
                if stats.allocated >= stats.capacity * GC_THRESHOLD_PERCENT / 100 {
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
    pub(super) fn step_eval(&mut self, expr: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let val = self.lisp.get(expr)?;
        
        match val {
            // Self-evaluating values
            Value::Nil | Value::True | Value::False | 
            Value::Number(_) | Value::Char(_) | 
            Value::Builtin(_) | Value::StdLib(_) | Value::Lambda { .. } |
            Value::Array { .. } | Value::String { .. } | Value::Native { .. } |
            Value::Ref(_) | Value::Usize(_) | Value::SyntaxRules { .. } |
            Value::Syntax { .. } | Value::ContFrame { .. } | Value::Continuation { .. } => {
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
    pub(super) fn step_eval_list(&mut self, car: ArenaIndex, cdr: ArenaIndex, expr: ArenaIndex, env: ArenaIndex) 
        -> Result<TrampolineState, EvalError> 
    {
        let head = self.lisp.get(car)?;
        
        // Check for special forms and macros
        if let Value::Symbol(_) = head {
            // Per R7RS §4.3: "local variable bindings can shadow syntactic bindings"
            // Check if this symbol is bound as a variable. If so, skip macro expansion
            // and treat it as a function application.
            let is_var_bound = self.is_variable_bound(env, car)?;
            
            // Check for macro invocation (evaluation-time expansion)
            // Variable bindings shadow macros, so only expand if not bound as a variable
            if !is_var_bound {
                if let Some(transformer) = self.lookup_macro(car)? {
                    let expanded = self.apply_macro(transformer, expr)?;
                    // Continue evaluating the expanded form
                    return Ok(TrampolineState::Eval { expr: expanded, env });
                }
            }
            
            // quote
            if self.lisp.symbol_matches(car, "quote")? {
                let val = self.lisp.car(cdr)?;
                return Ok(TrampolineState::Return { val });
            }
            
            // define-syntax - add macro to environment
            if self.lisp.symbol_matches(car, "define-syntax")? {
                return self.step_eval_define_syntax(cdr, env);
            }
            
            // let-syntax - local macro bindings
            if self.lisp.symbol_matches(car, "let-syntax")? {
                return self.step_eval_let_syntax(cdr, env);
            }
            
            // syntax-case - procedural macro pattern matching
            if self.lisp.symbol_matches(car, "syntax-case")? {
                return self.step_eval_syntax_case(cdr, env);
            }
            
            // syntax - create syntax template
            if self.lisp.symbol_matches(car, "syntax")? {
                return self.step_eval_syntax(cdr, env);
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
            
            // Note: let, let*, letrec, letrec* are now macros and
            // are expanded during evaluation, so they never reach here.

            // begin - continuation-based evaluation
            if self.lisp.symbol_matches(car, "begin")? {
                return self.step_eval_begin(cdr, env);
            }

            // Note: when, unless, and, or, cond are now macros and
            // are expanded during evaluation, so they never reach here.
            
            // Note: case and do are now macros (Phase 9)
            // and are expanded during evaluation, so they never reach here.
            
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
            
            // call-with-values - call producer, apply consumer to results
            if self.lisp.symbol_matches(car, "call-with-values")? {
                return self.step_eval_call_with_values(cdr, env);
            }
            
            // call-with-current-continuation / call/cc - capture the current continuation
            if self.lisp.symbol_matches(car, "call-with-current-continuation")? 
                || self.lisp.symbol_matches(car, "call/cc")? {
                return self.step_eval_call_cc(cdr, env);
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
    
    // Note: step_eval_cond removed - cond is now handled by macros
    
    // ========================================================================
    // Helpers
    // ========================================================================
    
    /// Count elements in a list
    pub(super) fn count_list(&self, mut list: ArenaIndex) -> Result<usize, EvalError> {
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
    pub(super) fn make_stdlib_param_list(&self, params: &[&str]) -> Result<ArenaIndex, EvalError> {
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
    pub(super) fn push_data(&mut self, values: &[ArenaIndex]) -> Result<usize, EvalError> {
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
    pub(super) fn read_data1(&self, start: usize) -> ArenaIndex {
        self.data_stack[start]
    }
    
    #[inline]
    pub(super) fn read_data2(&self, start: usize) -> (ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1])
    }
    
    #[inline]
    pub(super) fn read_data3(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2])
    }
    
    #[inline]
    pub(super) fn read_data4(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2], self.data_stack[start + 3])
    }
    
    #[inline]
    pub(super) fn read_data5(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2], self.data_stack[start + 3], self.data_stack[start + 4])
    }
    
    #[inline]
    pub(super) fn read_data6(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2], self.data_stack[start + 3], self.data_stack[start + 4], self.data_stack[start + 5])
    }
    
    /// Note: No longer used since do is now a macro (Phase 9), but kept for potential future use.
    #[inline]
    #[allow(dead_code)]
    pub(super) fn read_data7(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2], self.data_stack[start + 3], self.data_stack[start + 4], self.data_stack[start + 5], self.data_stack[start + 6])
    }
    
    /// Encode Builtin as ArenaIndex (store discriminant as raw usize)
    #[inline]
    pub(super) fn encode_builtin(builtin: Builtin) -> ArenaIndex {
        ArenaIndex::new(builtin as usize)
    }
    
    /// Decode Builtin from ArenaIndex
    #[inline]
    pub(super) fn decode_builtin(encoded: ArenaIndex) -> Builtin {
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
        pack_let_syntax_body / unpack_let_syntax_body => [saved_macro_env];
        pack_call_cc_apply / unpack_call_cc_apply => [captured_continuation];
        pack_continuation_apply / unpack_continuation_apply => [captured_continuation];
        
        // 2-field continuations
        pack_begin_seq / unpack_begin_seq => [remaining, env];
        // Note: pack_case_key removed - case is now handled by macros (Phase 9)
        pack_apply_first / unpack_apply_first => [args_list_expr, env];
        pack_apply_second / unpack_apply_second => [func, env];
        pack_set_value / unpack_set_value => [name, env];
        pack_call_with_values_producer / unpack_call_with_values_producer => [consumer_expr, env];
        pack_call_with_values_consumer / unpack_call_with_values_consumer => [consumer_expr, env];
        pack_call_with_values_apply / unpack_call_with_values_apply => [producer_result, env];
        
        // 3-field continuations
        pack_apply_forced / unpack_apply_forced => [args_expr, env, call_expr];
        pack_if_branch / unpack_if_branch => [then_expr, else_expr, env];
        pack_values_collect / unpack_values_collect => [remaining, collected, env];
        
        // 4-field continuations
        pack_syntax_case_match / unpack_syntax_case_match => [literals, clauses, env, pattern_bindings];
        
        // 6-field continuations (syntax-case fender)
        pack_syntax_case_fender / unpack_syntax_case_fender => [output, bindings, literals, remaining_clauses, env, stx];
        
        // Note: pack_do_test_result, pack_do_body, pack_do_init, pack_do_step removed - do is now handled by macros (Phase 9)
        // Note: pack_let_star_binding, pack_letrec_init removed - now handled by macros
        // Note: pack_let_binding removed - now handled by macros
    }

    // ========================================================================
    // Generated pack/unpack functions for 6-field lambda binding continuation
    // ========================================================================
    
    define_cont_pack_unpack! {
        pack_lambda_bind_arg / unpack_lambda_bind_arg => [remaining_exprs, eval_env, remaining_params, body, new_env, call_expr];
        pack_lambda_rest_collect / unpack_lambda_rest_collect => [remaining_exprs, eval_env, rest_param, body, new_env, collected, call_expr]
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
    
    /// Convert a ParseError to EvalError with expression context
    /// 
    /// Note: In the optimized EvalError, we use static messages only.
    /// The function name context is available from the expression itself.
    pub(super) fn parse_error_to_eval(&self, err: ParseError, expr: ArenaIndex, _func_name: &str) -> EvalError {
        EvalError {
            kind: ErrorKind::Parse,
            expr,
            message: "stdlib parse error",
            expected: None,
            got: None,
            arg_info: None,
            parse_error: Some(err),
        }
    }
    
    /// Check if a value is false (ONLY #f is false)
    #[inline]
    pub(super) fn is_false(&self, val: ArenaIndex) -> Result<bool, EvalError> {
        Ok(self.lisp.get(val)?.is_false())
    }
    
    // ========================================================================
    // Convenience
    // ========================================================================
    
    /// Maximum number of GC retry attempts for memory-related errors
    const MAX_GC_RETRIES: usize = 3;
    
    /// Memory usage threshold (percentage) above which GC runs before eval_str
    const GC_USAGE_THRESHOLD: usize = 50;
    
    /// Evaluate a string
    pub fn eval_str(&mut self, input: &str) -> EvalResult {
        // Auto-GC before evaluation: run GC if memory usage exceeds threshold
        // This prevents garbage accumulation across multiple eval_str calls
        // Running GC at the start (not end) ensures we don't collect the result
        let stats = self.lisp.stats();
        // Compare allocated >= capacity * threshold / 100 to avoid overflow
        if stats.allocated >= stats.capacity * Self::GC_USAGE_THRESHOLD / 100 {
            self.gc();
        }
        
        // Try to parse with auto-GC retry on out of memory
        // Attempts: 1 initial + up to MAX_GC_RETRIES retries after GC
        let expr = self.parse_with_gc_retry(input)?;
        
        // Try to evaluate with auto-GC retry on out of memory
        self.eval_with_gc_retry(expr, input)
    }
    
    /// Parse an input string with auto-GC retry on out of memory
    fn parse_with_gc_retry(&mut self, input: &str) -> EvalResult {
        for attempt in 0..=Self::MAX_GC_RETRIES {
            match parse(self.lisp, input) {
                Ok(e) => return Ok(e),
                Err(e) if matches!(e.kind, ParseErrorKind::OutOfMemory) => {
                    if attempt < Self::MAX_GC_RETRIES {
                        self.gc();
                        continue;
                    }
                    return Err(e.into());
                }
                Err(e) => return Err(e.into()),
            }
        }
        // This is unreachable, but the compiler doesn't know that
        Err(EvalError::new(ErrorKind::OutOfMemory))
    }
    
    /// Evaluate an expression with auto-GC retry on out of memory
    fn eval_with_gc_retry(&mut self, expr: ArenaIndex, input: &str) -> EvalResult {
        match self.eval(expr) {
            Ok(result) => Ok(result),
            Err(e) if matches!(e.kind, ErrorKind::OutOfMemory) => {
                // Auto-GC: Run GC and retry evaluation
                // Only one retry since eval failures are less common than parse failures
                self.gc();
                // Re-parse after GC since the old expr may have been garbage collected
                let expr = self.parse_with_gc_retry(input)?;
                self.eval(expr)
            }
            Err(e) => Err(e),
        }
    }
}
