//! Core evaluator implementation.
//!
//! Contains the Evaluator constructor, GC, environment management,
//! continuation management (arena-based), trampoline loop, and basic evaluation steps.

use grift_parser::{
    ArenaIndex, GcStats, Lisp, Value, Builtin, StdLib, parse, parse_all, ParseError, ParseErrorKind,
};

use crate::error::{
    ErrorKind, StackFrame, EvalError, EvalResult,
    MAX_STACK_DEPTH,
};
use crate::continuation::{TrampolineState,
    CONT_DONE, CONT_APPLY_FORCED, CONT_IF_BRANCH, CONT_EVAL_EXPR,
};
use crate::native::{NativeRegistry, NativeFn, simple_hash};

use super::Evaluator;

/// Standard macro definitions (loaded at startup)
const STANDARD_MACROS: &str = include_str!("macros.scm");

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Create a new evaluator with standard environment
    pub fn new(lisp: &'a Lisp<N>) -> Result<Self, EvalError> {
        let nil = lisp.nil()?;
        let mut eval = Evaluator {
            lisp,
            global_env: nil,
            call_stack: [StackFrame::default(); MAX_STACK_DEPTH],
            call_stack_depth: 0,
            current_cont: nil, // Empty continuation (Done)
            native_registry: NativeRegistry::new(),
            macro_env: nil,
            gensym_counter: 0,
            dynamic_wind_chain: nil, // Empty dynamic-wind chain
            output_callback: None, // No output callback by default
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
    /// let lisp: Lisp<20000> = Lisp::new();
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
    
    /// Set an output callback for display/newline operations.
    ///
    /// When set, the `display` and `newline` builtins will call this function
    /// to produce output. This enables side effects during macro expansion to be visible.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_eval::{Lisp, Evaluator, ArenaIndex};
    ///
    /// fn my_output_handler<const N: usize>(lisp: &Lisp<N>, val: ArenaIndex) {
    ///     // Handle output - check if val.is_nil() for newline vs display
    ///     // In a real std implementation, you would write to stdout here
    /// }
    ///
    /// let lisp: Lisp<20000> = Lisp::new();
    /// let mut eval = Evaluator::new(&lisp).unwrap();
    /// 
    /// // Set output callback
    /// eval.set_output_callback(Some(my_output_handler));
    /// ```
    pub fn set_output_callback(&mut self, callback: Option<crate::evaluator::OutputCallback<N>>) {
        self.output_callback = callback;
    }
    
    /// Run GC with minimal roots (global env, macro env, current continuation, and dynamic-wind chain)
    /// 
    /// Use `gc_with_state()` during evaluation to also root the current expression/value.
    pub fn gc(&self) -> GcStats {
        self.lisp.gc(&[self.global_env, self.macro_env, self.current_cont, self.dynamic_wind_chain])
    }
    
    /// Run GC during evaluation - marks continuation chain AND current state as roots
    /// 
    /// Roots array size is 8 to accommodate:
    /// - global_env, macro_env, current_cont, dynamic_wind_chain (4 static roots)
    /// - expr, env from TrampolineState::Eval (2 roots)
    /// - val from TrampolineState::Return (1 root)
    /// Plus some headroom for future additions.
    pub(super) fn gc_with_state(&self, state: &TrampolineState) -> GcStats {
        // With arena-based continuations, we just need to root the current_cont pointer.
        // The GC will trace through the ContFrame linked list automatically.
        const MAX_ROOTS: usize = 8;
        let mut roots = [ArenaIndex::NIL; MAX_ROOTS];
        let mut root_count = 0;
        
        // Always include global env, macro env, current continuation chain, and dynamic-wind chain
        roots[root_count] = self.global_env;
        root_count += 1;
        roots[root_count] = self.macro_env;
        root_count += 1;
        roots[root_count] = self.current_cont;
        root_count += 1;
        roots[root_count] = self.dynamic_wind_chain;
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
    
    /// Push a continuation onto the arena-based stack
    ///
    /// Creates a new ContFrame in the arena and links it to the current continuation chain.
    /// This is O(1) allocation and enables O(1) capture for call/cc.
    #[inline]
    pub(super) fn push_cont(&mut self, cont_type: usize, data: ArenaIndex, env: ArenaIndex) -> Result<(), EvalError> {
        let new_frame = self.lisp.cont_frame(cont_type, data, self.current_cont, env)?;
        self.current_cont = new_frame;
        Ok(())
    }
    
    /// Pop a continuation from the arena-based stack
    ///
    /// Returns the continuation type, data, and environment from the current frame,
    /// then updates current_cont to point to the parent frame.
    ///
    /// Returns (CONT_DONE, nil, nil) if the continuation stack is empty.
    #[inline]
    pub(super) fn pop_cont(&mut self) -> Result<(usize, ArenaIndex, ArenaIndex), EvalError> {
        if self.current_cont.is_nil() {
            let nil = self.lisp.nil()?;
            return Ok((CONT_DONE, nil, nil));
        }
        
        let (cont_type, data, parent, env) = self.lisp.cont_frame_parts(self.current_cont)?;
        self.current_cont = parent;
        Ok((cont_type, data, env))
    }
    
    /// Evaluate an expression (entry point)
    /// 
    /// Macros are now expanded during evaluation (not pre-processed).
    /// This enables evaluation-time macro expansion per the R7RS model.
    pub fn eval(&mut self, expr: ArenaIndex) -> EvalResult {
        // Reset continuation to empty (Done)
        self.current_cont = self.lisp.nil()?;
        // Start evaluation - macros are expanded on-demand during eval
        self.trampoline(TrampolineState::Eval { expr, env: self.global_env })
    }
    
    /// Evaluate an already-expanded expression (internal)
    /// 
    /// This is now equivalent to `eval()` since macro expansion 
    /// happens during evaluation. Kept for API compatibility.
    pub fn eval_expanded(&mut self, expr: ArenaIndex) -> EvalResult {
        // Reset continuation to empty (Done)
        self.current_cont = self.lisp.nil()?;
        // Start evaluation
        self.trampoline(TrampolineState::Eval { expr, env: self.global_env })
    }
    
    /// Evaluate an expression in a given environment
    /// Uses full trampolining - no Rust recursion
    /// 
    /// This is public so the REPL can evaluate expressions for display
    pub fn eval_in_env(&mut self, expr: ArenaIndex, env: ArenaIndex) -> EvalResult {
        // Reset continuation to empty (Done)
        self.current_cont = self.lisp.nil()?;
        self.trampoline(TrampolineState::Eval { expr, env })
    }
    
    /// Evaluate an expression for macro expansion
    /// 
    /// This is similar to eval_in_env but saves and restores the continuation
    /// state so that macro expansion can be nested within outer evaluation.
    /// This is crucial when macro expansion happens while evaluating arguments
    /// or in other nested contexts.
    pub(crate) fn eval_for_macro(&mut self, expr: ArenaIndex, env: ArenaIndex) -> EvalResult {
        // Save current continuation and call stack state
        let saved_cont = self.current_cont;
        let saved_depth = self.call_stack_depth;
        
        // Initialize for new evaluation
        self.current_cont = self.lisp.nil()?;
        
        // Run trampoline until completion
        let result = self.trampoline(TrampolineState::Eval { expr, env });
        
        // Restore saved state
        self.current_cont = saved_cont;
        self.call_stack_depth = saved_depth;
        
        result
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
            Value::Nil | Value::Void | Value::True | Value::False | 
            Value::Number(_) | Value::Char(_) | 
            Value::Builtin(_) | Value::StdLib(_) | Value::Lambda { .. } |
            Value::Array { .. } | Value::String { .. } | Value::Native { .. } |
            Value::Ref(_) | Value::Usize(_) |
            Value::ContFrame { .. } | Value::Continuation { .. } => {
                Ok(TrampolineState::Return { val: expr })
            }
            
            // Syntax object - special handling for lexically-scoped identifiers
            Value::Syntax { .. } => {
                // Get the wrapped datum
                let (datum, marks, subst, lex_env) = self.lisp.syntax_parts_with_env(expr)?;
                
                match self.lisp.get(datum)? {
                    // Syntax-wrapped identifier: resolve in captured lexical environment
                    // This enables lexically-scoped syntax objects
                    Value::Symbol(_) => {
                        self.eval_syntax_identifier(datum, marks, subst, lex_env, env)
                    }
                    
                    // Syntax-wrapped list: this could be code that needs evaluation
                    // We unwrap it and evaluate in the merged environment
                    Value::Cons { .. } => {
                        // Check if lex_env has any bindings
                        if self.lisp.get(lex_env)?.is_nil() {
                            // No captured environment - evaluate datum directly
                            Ok(TrampolineState::Eval { expr: datum, env })
                        } else {
                            // Merge captured environment with current environment
                            let merged_env = self.merge_environments(lex_env, env)?;
                            Ok(TrampolineState::Eval { expr: datum, env: merged_env })
                        }
                    }
                    
                    // Other syntax-wrapped values are self-evaluating
                    _ => Ok(TrampolineState::Return { val: datum })
                }
            }
            
            // Symbol - variable lookup
            Value::Symbol(_) => {
                let val = self.env_lookup(env, expr)?;
                // Return the looked-up value directly, even if it's a syntax object.
                // This allows syntax objects to be passed as arguments to functions
                // like bound-identifier=? that expect syntax object data.
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
    
    /// Evaluate a syntax-wrapped identifier using its captured lexical environment
    fn eval_syntax_identifier(
        &self,
        name: ArenaIndex,
        _marks: ArenaIndex,
        subst: ArenaIndex,
        lex_env: ArenaIndex,
        current_env: ArenaIndex,
    ) -> Result<TrampolineState, EvalError> {
        // 1. Check substitution environment (explicit renames from macros)
        if let Some(val) = self.lookup_in_subst(name, subst)? {
            return Ok(TrampolineState::Return { val });
        }
        
        // 2. Check captured lexical environment (creation-site bindings)
        // This is the key for lexically-scoped syntax objects
        if !self.lisp.get(lex_env)?.is_nil() {
            if let Some(val) = self.lookup_in_env_optional(lex_env, name)? {
                return Ok(TrampolineState::Return { val });
            }
        }
        
        // 3. Fall back to current environment
        if let Some(val) = self.lookup_in_env_optional(current_env, name)? {
            return Ok(TrampolineState::Return { val });
        }
        
        // 4. Finally, try global environment
        self.env_lookup(self.global_env, name)
            .map(|val| TrampolineState::Return { val })
    }
    
    /// Merge two environments, with the first taking precedence
    /// Creates a new environment where bindings from env1 shadow env2
    fn merge_environments(&mut self, env1: ArenaIndex, env2: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        // If env1 is nil, just return env2
        if self.lisp.get(env1)?.is_nil() {
            return Ok(env2);
        }
        
        // Create a copy of env1 with its tail pointing to env2
        // Collect env1 bindings
        let mut bindings = [ArenaIndex::new(0); 64];
        let mut count = 0;
        let mut current = env1;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            if count >= bindings.len() {
                // Buffer overflow: too many local bindings to copy.
                // Fall back to recursive processing for the remaining bindings.
                let car = self.lisp.car(current)?;
                let cdr = self.lisp.cdr(current)?;
                let rest_merged = self.merge_environments(cdr, env2)?;
                let mut result = rest_merged;
                result = self.lisp.cons(car, result)?;
                
                // Add the already-collected bindings in reverse order
                for i in (0..count).rev() {
                    result = self.lisp.cons(bindings[i], result)?;
                }
                return Ok(result);
            }
            bindings[count] = self.lisp.car(current)?;
            count += 1;
            current = self.lisp.cdr(current)?;
        }
        
        // Build new env chain: bindings from env1 -> env2
        let mut result = env2;
        for i in (0..count).rev() {
            result = self.lisp.cons(bindings[i], result)?;
        }
        
        Ok(result)
    }
    
    /// Look up a symbol in an environment, returning None if not found
    fn lookup_in_env_optional(&self, env: ArenaIndex, name: ArenaIndex) -> Result<Option<ArenaIndex>, EvalError> {
        let mut current = env;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let binding = self.lisp.car(current)?;
            
            if let Value::Cons { .. } = self.lisp.get(binding)? {
                let bound_name = self.lisp.car(binding)?;
                
                if self.lisp.symbol_eq(bound_name, name)? {
                    return Ok(Some(self.lisp.cdr(binding)?));
                }
            }
            
            current = self.lisp.cdr(current)?;
        }
        
        Ok(None)
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
            // and special form handling, treating it as a function application.
            let is_var_bound = self.is_variable_bound(env, car)?;
            
            // Check for macro invocation (evaluation-time expansion)
            // Variable bindings shadow macros, so only expand if not bound as a variable
            if !is_var_bound {
                if let Some(transformer) = self.lookup_macro(car)? {
                    // DON'T wrap macro inputs with lexical context here.
                    // The `syntax` form handles lexical capture when needed.
                    // Wrapping all macro inputs causes issues when the macro
                    // produces code that references the same identifiers
                    // (e.g., set! on a lambda parameter).
                    
                    // Use continuation-based macro expansion to avoid Rust stack growth
                    // for recursive macros. This pushes a CONT_MACRO_RESULT continuation
                    // and evaluates the transformer body, allowing arbitrarily deep
                    // macro recursion without stack overflow.
                    return self.apply_macro_trampolined(transformer, expr, env);
                }
            }
            
            // Special forms can be shadowed by variable bindings (R5RS compliance)
            // Only handle as special forms if NOT bound as a variable
            if !is_var_bound {
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
                
                // Note: with-syntax is now implemented as a macro in macros.scm
                // It uses syntax-case directly to bind patterns.
                
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
                    // Data: (then_expr . (else_expr . env))
                    let data = self.pack3(then_expr, else_expr, env)?;
                    self.push_cont(CONT_IF_BRANCH, data, env)?;
                    
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
                    // Data: env (single value)
                    let data = self.pack1(self.global_env)?;
                    self.push_cont(CONT_EVAL_EXPR, data, env)?;
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
                
                // dynamic-wind - establish dynamic extent with before/after thunks
                if self.lisp.symbol_matches(car, "dynamic-wind")? {
                    return self.step_eval_dynamic_wind(cdr, env);
                }
            }
        }
        
        // Function application - HYBRID EVALUATION
        self.push_frame(expr, car)?;
        
        // Push continuation: after evaluating func, apply it
        // Data: (args_expr . (env . call_expr))
        let data = self.pack3(cdr, env, expr)?;
        self.push_cont(CONT_APPLY_FORCED, data, env)?;
        
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
    // Arena-Based Pack/Unpack helpers for continuation data
    // ========================================================================
    //
    // These functions build and extract cons-cell chains in the arena for
    // storing continuation data. Each pack function returns an ArenaIndex
    // to the packed data, and each unpack function extracts values from
    // a packed ArenaIndex.
    
    /// Pack 1 value (just returns it as-is)
    #[inline]
    pub(super) fn pack1(&self, a: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        Ok(a)
    }
    
    /// Unpack 1 value (just returns it as-is)
    #[inline]
    pub(super) fn unpack1(&self, data: ArenaIndex) -> ArenaIndex {
        data
    }
    
    /// Pack 2 values into a cons cell: (a . b)
    #[inline]
    pub(super) fn pack2(&self, a: ArenaIndex, b: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        self.lisp.cons(a, b).map_err(Into::into)
    }
    
    /// Unpack 2 values from a cons cell: (a . b) -> (a, b)
    #[inline]
    pub(super) fn unpack2(&self, data: ArenaIndex) -> Result<(ArenaIndex, ArenaIndex), EvalError> {
        let a = self.lisp.car(data)?;
        let b = self.lisp.cdr(data)?;
        Ok((a, b))
    }
    
    /// Pack 3 values into nested cons: (a . (b . c))
    #[inline]
    pub(super) fn pack3(&self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        let bc = self.lisp.cons(b, c)?;
        self.lisp.cons(a, bc).map_err(Into::into)
    }
    
    /// Unpack 3 values from nested cons: (a . (b . c)) -> (a, b, c)
    #[inline]
    pub(super) fn unpack3(&self, data: ArenaIndex) -> Result<(ArenaIndex, ArenaIndex, ArenaIndex), EvalError> {
        let a = self.lisp.car(data)?;
        let bc = self.lisp.cdr(data)?;
        let b = self.lisp.car(bc)?;
        let c = self.lisp.cdr(bc)?;
        Ok((a, b, c))
    }
    
    /// Pack 4 values into nested cons: (a . (b . (c . d)))
    #[inline]
    pub(super) fn pack4(&self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        let cd = self.lisp.cons(c, d)?;
        let bcd = self.lisp.cons(b, cd)?;
        self.lisp.cons(a, bcd).map_err(Into::into)
    }
    
    /// Unpack 4 values from nested cons
    #[inline]
    pub(super) fn unpack4(&self, data: ArenaIndex) -> Result<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex), EvalError> {
        let a = self.lisp.car(data)?;
        let bcd = self.lisp.cdr(data)?;
        let b = self.lisp.car(bcd)?;
        let cd = self.lisp.cdr(bcd)?;
        let c = self.lisp.car(cd)?;
        let d = self.lisp.cdr(cd)?;
        Ok((a, b, c, d))
    }
    
    /// Pack 5 values into nested cons
    #[inline]
    pub(super) fn pack5(&self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex, e: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        let de = self.lisp.cons(d, e)?;
        let cde = self.lisp.cons(c, de)?;
        let bcde = self.lisp.cons(b, cde)?;
        self.lisp.cons(a, bcde).map_err(Into::into)
    }
    
    /// Unpack 5 values from nested cons
    #[inline]
    pub(super) fn unpack5(&self, data: ArenaIndex) -> Result<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex), EvalError> {
        let a = self.lisp.car(data)?;
        let bcde = self.lisp.cdr(data)?;
        let b = self.lisp.car(bcde)?;
        let cde = self.lisp.cdr(bcde)?;
        let c = self.lisp.car(cde)?;
        let de = self.lisp.cdr(cde)?;
        let d = self.lisp.car(de)?;
        let e = self.lisp.cdr(de)?;
        Ok((a, b, c, d, e))
    }
    
    /// Pack 6 values into nested cons
    #[inline]
    pub(super) fn pack6(&self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex, e: ArenaIndex, f: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        let ef = self.lisp.cons(e, f)?;
        let def = self.lisp.cons(d, ef)?;
        let cdef = self.lisp.cons(c, def)?;
        let bcdef = self.lisp.cons(b, cdef)?;
        self.lisp.cons(a, bcdef).map_err(Into::into)
    }
    
    /// Unpack 6 values from nested cons
    #[inline]
    pub(super) fn unpack6(&self, data: ArenaIndex) -> Result<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex), EvalError> {
        let a = self.lisp.car(data)?;
        let bcdef = self.lisp.cdr(data)?;
        let b = self.lisp.car(bcdef)?;
        let cdef = self.lisp.cdr(bcdef)?;
        let c = self.lisp.car(cdef)?;
        let def = self.lisp.cdr(cdef)?;
        let d = self.lisp.car(def)?;
        let ef = self.lisp.cdr(def)?;
        let e = self.lisp.car(ef)?;
        let f = self.lisp.cdr(ef)?;
        Ok((a, b, c, d, e, f))
    }
    
    /// Pack 7 values into nested cons
    #[inline]
    pub(super) fn pack7(&self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex, e: ArenaIndex, f: ArenaIndex, g: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        let fg = self.lisp.cons(f, g)?;
        let efg = self.lisp.cons(e, fg)?;
        let defg = self.lisp.cons(d, efg)?;
        let cdefg = self.lisp.cons(c, defg)?;
        let bcdefg = self.lisp.cons(b, cdefg)?;
        self.lisp.cons(a, bcdefg).map_err(Into::into)
    }
    
    /// Unpack 7 values from nested cons
    #[inline]
    pub(super) fn unpack7(&self, data: ArenaIndex) -> Result<(ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex), EvalError> {
        let a = self.lisp.car(data)?;
        let bcdefg = self.lisp.cdr(data)?;
        let b = self.lisp.car(bcdefg)?;
        let cdefg = self.lisp.cdr(bcdefg)?;
        let c = self.lisp.car(cdefg)?;
        let defg = self.lisp.cdr(cdefg)?;
        let d = self.lisp.car(defg)?;
        let efg = self.lisp.cdr(defg)?;
        let e = self.lisp.car(efg)?;
        let fg = self.lisp.cdr(efg)?;
        let f = self.lisp.car(fg)?;
        let g = self.lisp.cdr(fg)?;
        Ok((a, b, c, d, e, f, g))
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
    
    /// Encode usize as ArenaIndex
    #[inline]
    pub(super) fn encode_usize(val: usize) -> ArenaIndex {
        ArenaIndex::new(val)
    }
    
    /// Decode usize from ArenaIndex
    #[inline]
    pub(super) fn decode_usize(encoded: ArenaIndex) -> usize {
        encoded.raw()
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
