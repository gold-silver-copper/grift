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
use crate::continuation::{TrampolineState, GcRoots, ContType, EnvRef, ExprRef};
use crate::native::{NativeRegistry, NativeFn};

use super::Evaluator;

/// Standard macro definitions (loaded at startup)
const STANDARD_MACROS: &str = include_str!("macros.scm");

impl<'a, const N: usize> GcRoots for Evaluator<'a, N> {
    fn trace_roots(&self, tracer: &mut dyn FnMut(ArenaIndex)) {
        tracer(self.global_env.0);
        tracer(self.macro_env.0);
        tracer(self.current_cont);
        tracer(self.dynamic_wind_chain);
    }
}

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Create a new evaluator with standard environment
    pub fn new(lisp: &'a Lisp<N>) -> Result<Self, EvalError> {
        let nil = lisp.nil()?;
        let mut eval = Evaluator {
            lisp,
            global_env: EnvRef(nil),
            call_stack: [StackFrame::default(); MAX_STACK_DEPTH],
            call_stack_depth: 0,
            current_cont: nil, // Empty continuation (Done)
            native_registry: NativeRegistry::new(),
            macro_env: EnvRef(nil),
            gensym_counter: 0,
            dynamic_wind_chain: nil, // Empty dynamic-wind chain
            output_callback: None, // No output callback by default
            call_site_env: EnvRef(nil), // No call-site env initially
        };
        
        // Initialize global environment with builtins
        eval.global_env = EnvRef(lisp.nil()?);
        
        // Initialize macro environment
        eval.macro_env = EnvRef(lisp.nil()?);
        
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
            self.eval(ExprRef(form))?;
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
        self.global_env.0
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
        
        // Register in the native registry
        self.native_registry.register(name, func);
        
        // Create a symbol and a Native value, then bind in global env
        let name_sym = self.lisp.symbol(name)?;
        let native_val = self.lisp.native(id)?;
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
    
    /// Run GC with minimal roots (evaluator-owned roots only).
    /// 
    /// Use `gc_with_state()` during evaluation to also root the current expression/value.
    pub fn gc(&self) -> GcStats {
        const MAX_ROOTS: usize = 8;
        let mut roots = [ArenaIndex::NIL; MAX_ROOTS];
        let mut root_count = 0;
        
        self.trace_roots(&mut |idx| {
            debug_assert!(root_count < MAX_ROOTS, "too many GC roots for buffer");
            roots[root_count] = idx;
            root_count += 1;
        });
        
        self.lisp.gc(&roots[..root_count])
    }
    
    /// Run GC during evaluation - marks both evaluator roots and current trampoline state as roots.
    ///
    /// Uses the [`GcRoots`] trait on both `self` and `state` so that adding a
    /// new [`ArenaIndex`] field to either type only requires updating the
    /// corresponding `trace_roots` implementation.
    pub(super) fn gc_with_state(&self, state: &TrampolineState) -> GcStats {
        const MAX_ROOTS: usize = 8;
        let mut roots = [ArenaIndex::NIL; MAX_ROOTS];
        let mut root_count = 0;
        
        // Collect evaluator roots via GcRoots trait
        self.trace_roots(&mut |idx| {
            debug_assert!(root_count < MAX_ROOTS, "too many GC roots for buffer");
            roots[root_count] = idx;
            root_count += 1;
        });
        
        // Collect trampoline state roots via GcRoots trait
        state.trace_roots(&mut |idx| {
            debug_assert!(root_count < MAX_ROOTS, "too many GC roots for buffer");
            roots[root_count] = idx;
            root_count += 1;
        });
        
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
    pub(crate) fn env_extend(&self, env: EnvRef, name: ArenaIndex, value: ArenaIndex) -> Result<EnvRef, EvalError> {
        let binding = self.lisp.cons(name, value)?;
        Ok(EnvRef(self.lisp.cons(binding, env.0)?))
    }
    
    /// Look up a variable in an environment
    #[inline]
    pub(super) fn env_lookup(&self, env: EnvRef, name: ArenaIndex) -> EvalResult {
        let mut current = env.0;
        
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
        let mut current = self.global_env.0;
        
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
    pub(super) fn is_variable_bound(&self, env: EnvRef, name: ArenaIndex) -> Result<bool, EvalError> {
        // Check local environment first
        if self.env_contains(env.0, name)? {
            return Ok(true);
        }
        
        // Check global environment
        self.env_contains(self.global_env.0, name)
    }
    
    /// Helper to check if a name exists in a specific environment chain
    #[inline]
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
    pub(super) fn env_set(&self, env: EnvRef, name: ArenaIndex, value: ArenaIndex) -> EvalResult {
        // First search local environment
        let mut current = env.0;
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
        let mut current = self.global_env.0;
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
        let mut current = self.global_env.0;
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
    pub(super) fn push_cont(&mut self, cont_type: ContType, data: ArenaIndex, env: ArenaIndex) -> Result<(), EvalError> {
        let new_frame = self.lisp.cont_frame(cont_type.as_usize(), data, self.current_cont, env)?;
        self.current_cont = new_frame;
        Ok(())
    }
    
    /// Create a continuation builder for the given type and environment.
    ///
    /// The builder packs data and pushes the continuation in a single chain,
    /// making the data layout explicit and reducing pack+push boilerplate.
    ///
    /// # Examples
    ///
    /// ```text
    /// // Instead of:
    /// let data = self.pack3(then_expr, else_expr, env)?;
    /// self.push_cont(ContType::IfBranch, data, env)?;
    ///
    /// // Use:
    /// self.cont(ContType::IfBranch, env).data3(then_expr, else_expr, env)?;
    /// ```
    #[inline]
    pub(super) fn cont(&mut self, cont_type: ContType, env: EnvRef) -> ContBuilder<'_, 'a, N> {
        ContBuilder { evaluator: self, cont_type, env }
    }
    
    /// Pop a continuation from the arena-based stack
    ///
    /// Returns the continuation type, data, and environment from the current frame,
    /// then updates current_cont to point to the parent frame.
    ///
    /// Returns (ContType::Done, nil, nil) if the continuation stack is empty.
    #[inline]
    pub(super) fn pop_cont(&mut self) -> Result<(ContType, ArenaIndex, ArenaIndex), EvalError> {
        if self.current_cont.is_nil() {
            let nil = self.lisp.nil()?;
            return Ok((ContType::Done, nil, nil));
        }
        
        let (cont_type_raw, data, parent, env) = self.lisp.cont_frame_parts(self.current_cont)?;
        self.current_cont = parent;
        let cont_type = ContType::from_usize(cont_type_raw)
            .ok_or_else(|| self.make_error(crate::error::ErrorKind::Generic, data)
                .with_message("invalid continuation type"))?;
        Ok((cont_type, data, env))
    }
    
    /// Evaluate an expression (entry point)
    /// 
    /// Macros are now expanded during evaluation (not pre-processed).
    /// This enables evaluation-time macro expansion per the R7RS model.
    pub fn eval(&mut self, expr: ExprRef) -> EvalResult {
        // Reset continuation to empty (Done)
        self.current_cont = self.lisp.nil()?;
        // Start evaluation - macros are expanded on-demand during eval
        self.trampoline(TrampolineState::Eval { expr, env: self.global_env })
    }
    
    /// Evaluate an already-expanded expression (internal)
    /// 
    /// This is now equivalent to `eval()` since macro expansion 
    /// happens during evaluation. Kept for API compatibility.
    pub fn eval_expanded(&mut self, expr: ExprRef) -> EvalResult {
        // Reset continuation to empty (Done)
        self.current_cont = self.lisp.nil()?;
        // Start evaluation
        self.trampoline(TrampolineState::Eval { expr, env: self.global_env })
    }
    
    /// Evaluate an expression in a given environment
    /// Uses full trampolining - no Rust recursion
    /// 
    /// This is public so the REPL can evaluate expressions for display
    pub fn eval_in_env(&mut self, expr: ExprRef, env: EnvRef) -> EvalResult {
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
    pub(crate) fn eval_for_macro(&mut self, expr: ExprRef, env: EnvRef) -> EvalResult {
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
    pub(super) fn step_eval(&mut self, expr: ExprRef, env: EnvRef) -> Result<TrampolineState, EvalError> {
        let val = self.lisp.get(expr.0)?;
        
        match val {
            // Self-evaluating values
            Value::Nil | Value::Void | Value::True | Value::False | 
            Value::Number(_) | Value::Char(_) | 
            Value::Builtin(_) | Value::StdLib(_) | Value::Lambda { .. } |
            Value::Array { .. } | Value::String { .. } | Value::Native { .. } |
            Value::Ref(_) | Value::Usize(_) |
            Value::ContFrame { .. } | Value::Continuation { .. } => {
                Ok(TrampolineState::Return { val: expr.0 })
            }
            
            // Syntax object - special handling for lexically-scoped identifiers
            Value::Syntax { .. } => {
                // Get the wrapped datum
                let (datum, marks, subst, lex_env) = self.lisp.syntax_parts_with_env(expr.0)?;
                
                match self.lisp.get(datum)? {
                    // Syntax-wrapped identifier: resolve in captured lexical environment
                    // This enables lexically-scoped syntax objects
                    Value::Symbol(_) => {
                        self.eval_syntax_identifier(datum, marks, subst, lex_env, env.0)
                    }
                    
                    // Syntax-wrapped list: this could be code that needs evaluation
                    // We unwrap it and evaluate in the merged environment
                    Value::Cons { .. } => {
                        // Check if lex_env has any bindings
                        if self.lisp.get(lex_env)?.is_nil() {
                            // No captured environment - evaluate datum directly
                            Ok(TrampolineState::Eval { expr: ExprRef(datum), env })
                        } else {
                            // Merge captured environment with current environment
                            let merged_env = self.merge_environments(lex_env, env.0)?;
                            Ok(TrampolineState::Eval { expr: ExprRef(datum), env: EnvRef(merged_env) })
                        }
                    }
                    
                    // Other syntax-wrapped values are self-evaluating
                    _ => Ok(TrampolineState::Return { val: datum })
                }
            }
            
            // Symbol - variable lookup or identifier macro expansion
            //
            // Optimized: single-pass lookup instead of is_variable_bound + env_lookup.
            // If the variable is found in local or global env, return immediately.
            // Only check for identifier macros when the symbol is unbound.
            Value::Symbol(_) => {
                // Try local environment first
                if let Some(val) = self.lookup_in_env_optional(env.0, expr.0)? {
                    return Ok(TrampolineState::Return { val });
                }
                // Try global environment
                if let Some(val) = self.lookup_in_env_optional(self.global_env.0, expr.0)? {
                    return Ok(TrampolineState::Return { val });
                }
                // Not found as variable - check for identifier macros
                if let Some(transformer) = self.lookup_macro(expr.0)? {
                    return self.apply_macro_trampolined(transformer, expr.0, env);
                }
                Err(self.make_error(ErrorKind::UnboundVariable, expr.0))
            }
            
            // List - special form or function application
            Value::Cons { .. } => {
                let car = self.lisp.car(expr.0)?;
                let cdr = self.lisp.cdr(expr.0)?;
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
                let (car, cdr) = self.lisp.car_cdr(current)?;
                let rest_merged = self.merge_environments(cdr, env2)?;
                let mut result = rest_merged;
                result = self.lisp.cons(car, result)?;
                
                // Add the already-collected bindings in reverse order
                for i in (0..count).rev() {
                    result = self.lisp.cons(bindings[i], result)?;
                }
                return Ok(result);
            }
            let (car, cdr) = self.lisp.car_cdr(current)?;
            bindings[count] = car;
            count += 1;
            current = cdr;
        }
        
        // Build new env chain: bindings from env1 -> env2
        let mut result = env2;
        for i in (0..count).rev() {
            result = self.lisp.cons(bindings[i], result)?;
        }
        
        Ok(result)
    }
    
    /// Look up a symbol in an environment, returning None if not found
    #[inline]
    fn lookup_in_env_optional(&self, env: ArenaIndex, name: ArenaIndex) -> Result<Option<ArenaIndex>, EvalError> {
        let mut current = env;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return Ok(None),
                Value::Cons { car, cdr } => {
                    if let Value::Cons { car: bound_name, cdr: bound_value } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        return Ok(Some(bound_value));
                    }
                    current = cdr;
                }
                _ => return Ok(None),
            }
        }
    }
    
    /// Evaluate a list (special form or application)
    ///
    /// Optimized: avoids expensive `is_variable_bound` env scan on every call.
    /// Instead, checks macros first (small env), then keyword matches (cheap
    /// string comparisons), and only calls `is_variable_bound` when a macro
    /// or keyword actually matches — which is rare for regular function calls.
    pub(super) fn step_eval_list(&mut self, car: ArenaIndex, cdr: ArenaIndex, expr: ExprRef, env: EnvRef) 
        -> Result<TrampolineState, EvalError> 
    {
        let head = self.lisp.get(car)?;
        
        // Check for special forms and macros
        if let Value::Symbol(_) = head {
            // Macro check first (macro env is small, so this is cheap).
            // Macros defined via let-syntax/define-syntax take priority over all
            // special forms, including core forms like `if`. This matches R7RS
            // semantics where syntactic bindings override built-in syntax.
            // Per R7RS §4.3: "local variable bindings can shadow syntactic bindings"
            // Only apply the macro if the symbol is NOT bound as a variable.
            let macro_found = self.lookup_macro(car)?;
            if let Some(transformer) = macro_found {
                if !self.is_variable_bound(env, car)? {
                    return self.apply_macro_trampolined(transformer, expr.0, env);
                }
                // Variable shadows macro - fall through (core special forms still recognized)
            }
            
            // Core special forms: always recognized regardless of variable bindings.
            // Only reached when no macro overrides this name (or macro was variable-shadowed).
            if let Some(result) = self.try_dispatch_special_form(car, cdr, env)? {
                return Ok(result);
            }
            
            // Non-core special forms: only checked when no macro with this name exists.
            // Uses cheap keyword matching — only calls is_variable_bound (expensive)
            // when a keyword actually matches, which is rare for regular function calls.
            if macro_found.is_none() {
                if let Some(result) = self.try_dispatch_non_core_form(car, cdr, env)? {
                    return Ok(result);
                }
            }
        }
        
        // Function application - HYBRID EVALUATION
        self.push_frame(expr.0, car)?;
        
        // Push continuation: after evaluating func, apply it
        // Data: (args_expr . (env . call_expr))
        self.cont(ContType::ApplyForced, env).data3(cdr, env.0, expr.0)?;
        
        // Evaluate the function expression
        Ok(TrampolineState::Eval { expr: ExprRef(car), env })
    }
    
    // Note: step_eval_cond removed - cond is now handled by macros
    
    /// Try to dispatch a symbol as the `if` special form.
    /// Returns Some(TrampolineState) if the symbol is `if`, None otherwise.
    /// `if` is always recognized regardless of variable bindings - this ensures
    /// hygienic macros that use `if` (like `or`, `and`, `cond`) work correctly
    /// even when the user has locally rebound `if`.
    fn try_dispatch_special_form(&mut self, name: ArenaIndex, cdr: ArenaIndex, env: EnvRef) 
        -> Result<Option<TrampolineState>, EvalError> 
    {
        if self.lisp.symbol_matches(name, "if")? {
            let cond_expr = self.lisp.car(cdr)?;
            let rest = self.lisp.cdr(cdr)?;
            let then_expr = self.lisp.car(rest)?;
            let else_rest = self.lisp.cdr(rest)?;
            let else_expr = if self.lisp.get(else_rest)?.is_nil() {
                self.lisp.nil()?
            } else {
                self.lisp.car(else_rest)?
            };
            self.cont(ContType::IfBranch, env).data3(then_expr, else_expr, env.0)?;
            return Ok(Some(TrampolineState::Eval { expr: ExprRef(cond_expr), env }));
        }
        Ok(None)
    }
    
    /// Try to dispatch a non-core special form (can be shadowed by variable bindings).
    /// 
    /// Unlike `try_dispatch_special_form`, these forms can be overridden by
    /// variable bindings (per R7RS §4.3). This method uses cheap keyword matching
    /// first, and only calls the expensive `is_variable_bound` when a keyword
    /// actually matches — avoiding the full env scan for regular function calls.
    fn try_dispatch_non_core_form(&mut self, car: ArenaIndex, cdr: ArenaIndex, env: EnvRef) 
        -> Result<Option<TrampolineState>, EvalError> 
    {
        // quote
        if self.lisp.symbol_matches(car, "quote")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            let val = self.lisp.car(cdr)?;
            return Ok(Some(TrampolineState::Return { val }));
        }
        
        // define-syntax - add macro to environment
        if self.lisp.symbol_matches(car, "define-syntax")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.step_eval_define_syntax(cdr, env).map(Some);
        }
        
        // let-syntax - local macro bindings
        if self.lisp.symbol_matches(car, "let-syntax")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.step_eval_let_syntax(cdr, env).map(Some);
        }
        
        // letrec-syntax - local macro bindings with mutual visibility
        if self.lisp.symbol_matches(car, "letrec-syntax")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.step_eval_letrec_syntax(cdr, env).map(Some);
        }
        
        // syntax-case - procedural macro pattern matching
        if self.lisp.symbol_matches(car, "syntax-case")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.step_eval_syntax_case(cdr, env).map(Some);
        }
        
        // syntax - create syntax template
        if self.lisp.symbol_matches(car, "syntax")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.step_eval_syntax(cdr, env).map(Some);
        }
        
        // lambda
        if self.lisp.symbol_matches(car, "lambda")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            let val = self.eval_lambda(cdr, env)?;
            return Ok(Some(TrampolineState::Return { val }));
        }
        
        // define
        if self.lisp.symbol_matches(car, "define")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.eval_define(cdr, env).map(Some);
        }
        
        // set! - mutate variable binding
        if self.lisp.symbol_matches(car, "set!")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.eval_set(cdr, env).map(Some);
        }
        
        // begin - continuation-based evaluation
        if self.lisp.symbol_matches(car, "begin")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.step_eval_begin(cdr, env).map(Some);
        }
        
        // quasiquote - template with unquote (trampolined)
        if self.lisp.symbol_matches(car, "quasiquote")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.eval_quasiquote(self.lisp.car(cdr)?, env).map(Some);
        }
        
        // eval - continuation-based evaluation at runtime
        if self.lisp.symbol_matches(car, "eval")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            let expr_to_eval = self.lisp.car(cdr)?;
            let global = self.global_env.0;
            self.cont(ContType::EvalExpr, env).data1(global)?;
            return Ok(Some(TrampolineState::Eval { expr: ExprRef(expr_to_eval), env }));
        }
        
        // apply - apply function to list of arguments
        if self.lisp.symbol_matches(car, "apply")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.step_eval_apply(cdr, env).map(Some);
        }
        
        // values - return multiple values (as a special list)
        if self.lisp.symbol_matches(car, "values")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.eval_values(cdr, env).map(Some);
        }
        
        // call-with-values - call producer, apply consumer to results
        if self.lisp.symbol_matches(car, "call-with-values")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.step_eval_call_with_values(cdr, env).map(Some);
        }
        
        // call-with-current-continuation / call/cc - capture the current continuation
        if self.lisp.symbol_matches(car, "call-with-current-continuation")? 
            || self.lisp.symbol_matches(car, "call/cc")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.step_eval_call_cc(cdr, env).map(Some);
        }
        
        // dynamic-wind - establish dynamic extent with before/after thunks
        if self.lisp.symbol_matches(car, "dynamic-wind")? {
            if self.is_variable_bound(env, car)? { return Ok(None); }
            return self.step_eval_dynamic_wind(cdr, env).map(Some);
        }
        
        Ok(None)
    }
    
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
    #[allow(dead_code)]
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
        self.lisp.car_cdr(data).map_err(Into::into)
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
        let (a, bc) = self.lisp.car_cdr(data)?;
        let (b, c) = self.lisp.car_cdr(bc)?;
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
        let (a, bcd) = self.lisp.car_cdr(data)?;
        let (b, cd) = self.lisp.car_cdr(bcd)?;
        let (c, d) = self.lisp.car_cdr(cd)?;
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
        let (a, bcde) = self.lisp.car_cdr(data)?;
        let (b, cde) = self.lisp.car_cdr(bcde)?;
        let (c, de) = self.lisp.car_cdr(cde)?;
        let (d, e) = self.lisp.car_cdr(de)?;
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
        let (a, bcdef) = self.lisp.car_cdr(data)?;
        let (b, cdef) = self.lisp.car_cdr(bcdef)?;
        let (c, def) = self.lisp.car_cdr(cdef)?;
        let (d, ef) = self.lisp.car_cdr(def)?;
        let (e, f) = self.lisp.car_cdr(ef)?;
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
        let (a, bcdefg) = self.lisp.car_cdr(data)?;
        let (b, cdefg) = self.lisp.car_cdr(bcdefg)?;
        let (c, defg) = self.lisp.car_cdr(cdefg)?;
        let (d, efg) = self.lisp.car_cdr(defg)?;
        let (e, fg) = self.lisp.car_cdr(efg)?;
        let (f, g) = self.lisp.car_cdr(fg)?;
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
        match self.eval(ExprRef(expr)) {
            Ok(result) => Ok(result),
            Err(e) if matches!(e.kind, ErrorKind::OutOfMemory) => {
                // Auto-GC: Run GC and retry evaluation
                // Only one retry since eval failures are less common than parse failures
                self.gc();
                // Re-parse after GC since the old expr may have been garbage collected
                let expr = self.parse_with_gc_retry(input)?;
                self.eval(ExprRef(expr))
            }
            Err(e) => Err(e),
        }
    }
}

// ============================================================================
// Continuation Builder
// ============================================================================

/// Builder for packing data and pushing a continuation in one step.
///
/// Created via [`Evaluator::cont`]. The builder consumes itself on each
/// terminal method (`data1` through `data7`), releasing the mutable borrow
/// on the evaluator.
///
/// # Data Layout
///
/// Each `dataN` method packs N values into nested cons cells and pushes
/// the resulting continuation frame:
///
/// - `data1(a)` — single value (no packing)
/// - `data2(a, b)` — `(a . b)`
/// - `data3(a, b, c)` — `(a . (b . c))`
/// - `data4(a, b, c, d)` — `(a . (b . (c . d)))`
/// - etc.
pub(super) struct ContBuilder<'e, 'a, const N: usize> {
    evaluator: &'e mut Evaluator<'a, N>,
    cont_type: ContType,
    env: EnvRef,
}

impl<'e, 'a, const N: usize> ContBuilder<'e, 'a, N> {
    /// Push with a single value (no packing needed).
    #[inline]
    pub fn data1(self, a: ArenaIndex) -> Result<(), EvalError> {
        self.evaluator.push_cont(self.cont_type, a, self.env.0)
    }
    
    /// Pack 2 values as `(a . b)` and push.
    #[inline]
    pub fn data2(self, a: ArenaIndex, b: ArenaIndex) -> Result<(), EvalError> {
        let data = self.evaluator.pack2(a, b)?;
        self.evaluator.push_cont(self.cont_type, data, self.env.0)
    }
    
    /// Pack 3 values as `(a . (b . c))` and push.
    #[inline]
    pub fn data3(self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex) -> Result<(), EvalError> {
        let data = self.evaluator.pack3(a, b, c)?;
        self.evaluator.push_cont(self.cont_type, data, self.env.0)
    }
    
    /// Pack 4 values as `(a . (b . (c . d)))` and push.
    #[inline]
    pub fn data4(self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex) -> Result<(), EvalError> {
        let data = self.evaluator.pack4(a, b, c, d)?;
        self.evaluator.push_cont(self.cont_type, data, self.env.0)
    }
    
    /// Pack 5 values as `(a . (b . (c . (d . e))))` and push.
    #[inline]
    pub fn data5(self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex, e: ArenaIndex) -> Result<(), EvalError> {
        let data = self.evaluator.pack5(a, b, c, d, e)?;
        self.evaluator.push_cont(self.cont_type, data, self.env.0)
    }
    
    /// Pack 6 values as `(a . (b . (c . (d . (e . f)))))` and push.
    #[inline]
    pub fn data6(self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex, e: ArenaIndex, f: ArenaIndex) -> Result<(), EvalError> {
        let data = self.evaluator.pack6(a, b, c, d, e, f)?;
        self.evaluator.push_cont(self.cont_type, data, self.env.0)
    }
    
    /// Pack 7 values as `(a . (b . (c . (d . (e . (f . g))))))` and push.
    #[inline]
    pub fn data7(self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex, e: ArenaIndex, f: ArenaIndex, g: ArenaIndex) -> Result<(), EvalError> {
        let data = self.evaluator.pack7(a, b, c, d, e, f, g)?;
        self.evaluator.push_cont(self.cont_type, data, self.env.0)
    }
}
