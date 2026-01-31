//! Core evaluation loop and trampoline.
//!
//! This module contains the main evaluation functions including the trampoline,
//! step_eval, step_return, and pack/unpack helpers for continuation data.

use grift_parser::{ArenaIndex, Value, Builtin, parse};

use crate::error::{ErrorKind, EvalError, EvalResult};
use crate::continuation::{Cont, TrampolineState, MAX_DATA_STACK};
use super::Evaluator;

impl<'a, const N: usize> Evaluator<'a, N> {
    // ========================================================================
    // Main Evaluation - Full Trampoline (No Rust Recursion)
    // ========================================================================
    
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
    pub(super) fn trampoline(&mut self, mut state: TrampolineState) -> EvalResult {
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
    pub(super) fn step_eval(&mut self, expr: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
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
    pub(super) fn step_eval_list(&mut self, car: ArenaIndex, cdr: ArenaIndex, expr: ArenaIndex, env: ArenaIndex) 
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
    
    /// Process a return value with the current continuation
    pub(super) fn step_return(&mut self, val: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
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
                self.step_return_apply_forced(val, data_start)
            }
            
            Cont::LambdaFirstBind(data_start) => {
                self.step_return_lambda_first_bind(val, data_start)
            }
            
            Cont::LambdaBindArg { .. } => {
                // This shouldn't be hit directly - LambdaFirstBind pops it
                Err(self.make_error(ErrorKind::Generic, val))
            }
            
            Cont::BuiltinForceArg(data_start) => {
                self.step_return_builtin_force_arg(val, data_start)
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
                self.step_return_let_binding(val, data_start)
            }

            Cont::LetStarBinding(data_start) => {
                self.step_return_let_star_binding(val, data_start)
            }

            Cont::LetrecInit(data_start) => {
                self.step_return_letrec_init(val, data_start)
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
                self.step_return_cond_test(val, data_start)
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
                self.step_return_do_test_result(val, data_start)
            }

            Cont::DoBody(data_start) => {
                self.step_return_do_body(val, data_start)
            }

            Cont::DoStep(data_start) => {
                self.step_return_do_step(val, data_start)
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
                self.step_return_native_args_collect(val, data_start)
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
    
    // ========================================================================
    // GC with current evaluation state
    // ========================================================================
    
    /// Run garbage collection while preserving evaluation state
    pub(super) fn gc_with_state(&mut self, state: &TrampolineState) {
        // Collect roots from:
        // 1. Global environment
        // 2. Current evaluation state (expr and env in Eval, val in Return)
        // 3. Data stack (contains ArenaIndex values for continuations)
        
        // Build root array
        let mut roots = [ArenaIndex::NIL; 3 + MAX_DATA_STACK];
        roots[0] = self.global_env;
        
        // Add state roots
        let mut root_count = 1;
        match state {
            TrampolineState::Eval { expr, env } => {
                roots[root_count] = *expr;
                root_count += 1;
                roots[root_count] = *env;
                root_count += 1;
            }
            TrampolineState::Return { val } => {
                roots[root_count] = *val;
                root_count += 1;
            }
        }
        
        // Add data stack roots
        for i in 0..self.data_stack_top {
            if root_count < roots.len() {
                roots[root_count] = self.data_stack[i];
                root_count += 1;
            }
        }
        
        // Run GC with these roots
        self.lisp.gc_with_roots(&roots[..root_count]);
    }
    
    // ========================================================================
    // Convenience
    // ========================================================================
    
    /// Evaluate a string
    pub fn eval_str(&mut self, input: &str) -> EvalResult {
        // Try to parse, with auto-GC retry on out of memory
        let expr = match parse(self.lisp, input) {
            Ok(e) => e,
            Err(e) if matches!(e.kind, grift_parser::ParseErrorKind::OutOfMemory) => {
                // Auto-GC: Run GC and retry parsing
                self.gc();
                parse(self.lisp, input)?
            }
            Err(e) => return Err(e.into()),
        };
        self.eval(expr)
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
    
    #[inline]
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

    // Pack/Unpack for specific continuation types
    
    pub(super) fn pack_if_branch(&mut self, then_expr: ArenaIndex, else_expr: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[then_expr, else_expr, env])
    }
    
    pub(super) fn unpack_if_branch(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex) {
        self.read_data3(data_start)
    }
    
    pub(super) fn pack_apply_forced(&mut self, args_expr: ArenaIndex, env: ArenaIndex, call_expr: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[args_expr, env, call_expr])
    }
    
    pub(super) fn unpack_apply_forced(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex) {
        self.read_data3(data_start)
    }
    
    pub(super) fn pack_when(&mut self, body: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[body, env])
    }
    
    pub(super) fn unpack_when(&self, data_start: usize) -> (ArenaIndex, ArenaIndex) {
        self.read_data2(data_start)
    }
    
    pub(super) fn pack_unless(&mut self, body: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[body, env])
    }
    
    pub(super) fn unpack_unless(&self, data_start: usize) -> (ArenaIndex, ArenaIndex) {
        self.read_data2(data_start)
    }
    
    pub(super) fn pack_eval_expr(&mut self, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[env])
    }
    
    pub(super) fn unpack_eval_expr(&self, data_start: usize) -> ArenaIndex {
        self.read_data1(data_start)
    }
    
    pub(super) fn pack_and(&mut self, remaining: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[remaining, env])
    }
    
    pub(super) fn unpack_and(&self, data_start: usize) -> (ArenaIndex, ArenaIndex) {
        self.read_data2(data_start)
    }
    
    pub(super) fn pack_or(&mut self, remaining: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[remaining, env])
    }
    
    pub(super) fn unpack_or(&self, data_start: usize) -> (ArenaIndex, ArenaIndex) {
        self.read_data2(data_start)
    }
    
    pub(super) fn pack_begin_seq(&mut self, remaining: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[remaining, env])
    }
    
    pub(super) fn unpack_begin_seq(&self, data_start: usize) -> (ArenaIndex, ArenaIndex) {
        self.read_data2(data_start)
    }
    
    pub(super) fn pack_apply_first(&mut self, args_list_expr: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[args_list_expr, env])
    }
    
    pub(super) fn unpack_apply_first(&self, data_start: usize) -> (ArenaIndex, ArenaIndex) {
        self.read_data2(data_start)
    }
    
    pub(super) fn pack_apply_second(&mut self, func: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[func, env])
    }
    
    pub(super) fn unpack_apply_second(&self, data_start: usize) -> (ArenaIndex, ArenaIndex) {
        self.read_data2(data_start)
    }
    
    pub(super) fn pack_values_collect(&mut self, remaining: ArenaIndex, collected: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[remaining, collected, env])
    }
    
    pub(super) fn unpack_values_collect(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex) {
        self.read_data3(data_start)
    }
    
    pub(super) fn pack_define_value(&mut self, name: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[name])
    }
    
    pub(super) fn unpack_define_value(&self, data_start: usize) -> ArenaIndex {
        self.read_data1(data_start)
    }
    
    pub(super) fn pack_set_value(&mut self, name: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[name, env])
    }
    
    pub(super) fn unpack_set_value(&self, data_start: usize) -> (ArenaIndex, ArenaIndex) {
        self.read_data2(data_start)
    }
    
    pub(super) fn pack_native_args_collect(&mut self, remaining: ArenaIndex, collected: ArenaIndex, id: u32, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[remaining, collected, ArenaIndex::new(id as usize), env])
    }
    
    pub(super) fn unpack_native_args_collect(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, u32, ArenaIndex) {
        let (remaining, collected, id_idx, env) = self.read_data4(data_start);
        (remaining, collected, id_idx.raw() as u32, env)
    }
    
    pub(super) fn pack_quasiquote_car(&mut self, cdr: ArenaIndex, depth: usize, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[cdr, ArenaIndex::new(depth), env])
    }
    
    pub(super) fn unpack_quasiquote_car(&self, data_start: usize) -> (ArenaIndex, usize, ArenaIndex) {
        let (cdr, depth_idx, env) = self.read_data3(data_start);
        (cdr, depth_idx.raw(), env)
    }
    
    pub(super) fn pack_quasiquote_cdr(&mut self, car_val: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[car_val])
    }
    
    pub(super) fn unpack_quasiquote_cdr(&self, data_start: usize) -> ArenaIndex {
        self.read_data1(data_start)
    }
    
    pub(super) fn pack_quasiquote_splice(&mut self, cdr: ArenaIndex, depth: usize, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[cdr, ArenaIndex::new(depth), env])
    }
    
    pub(super) fn unpack_quasiquote_splice(&self, data_start: usize) -> (ArenaIndex, usize, ArenaIndex) {
        let (cdr, depth_idx, env) = self.read_data3(data_start);
        (cdr, depth_idx.raw(), env)
    }
    
    pub(super) fn pack_quasiquote_splice_append(&mut self, splice_val: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[splice_val])
    }
    
    pub(super) fn unpack_quasiquote_splice_append(&self, data_start: usize) -> ArenaIndex {
        self.read_data1(data_start)
    }
}
