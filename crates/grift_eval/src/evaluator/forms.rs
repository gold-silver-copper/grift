//! Special form handling for the evaluator.
//!
//! Contains step_return (continuation handling) and all step_eval_* forms
//! (let, let*, letrec, begin, and, or, case, do, quasiquote, apply, values, etc.)

use grift_parser::{ArenaIndex, Value, parse};

use crate::error::{ErrorKind, EvalError, EvalResult};
use crate::continuation::{Cont, TrampolineState};
use crate::helpers::case_matches;
use crate::extract_args;

use super::Evaluator;

impl<'a, const N: usize> Evaluator<'a, N> {
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
                        
                        // Parse body and expand macros
                        let parsed_body = parse(self.lisp, s.body())
                            .map_err(|e| self.parse_error_to_eval(e, call_expr, s.name()))?;
                        // Expand macros in the body
                        let body = self.expand(parsed_body)?;
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

            // Note: LetBinding, LetStarBinding, LetrecInit handlers removed
            // These forms are now handled by macros during evaluation

            // Note: When, Unless, CondTest, And, Or continuations removed
            // These forms are now handled by macros during expansion

            Cont::EvalExpr(data_start) => {
                let env = self.unpack_eval_expr(data_start);
                // val is the evaluated expression - now evaluate it
                Ok(Some(TrampolineState::Eval { expr: val, env }))
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

    // Note: step_eval_cond_cont removed - cond is now handled by macros

    // ========================================================================
    // Helper functions for fully trampolined evaluation
    // ========================================================================

    /// Helper for case - check clauses after key is evaluated
    pub(super) fn step_return_case_key(&mut self, key: ArenaIndex, clauses: ArenaIndex, env: ArenaIndex) 
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
    pub(super) fn step_return_do_init(
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
    pub(super) fn step_do_start_steps(
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
    pub(super) fn apply_do_step_values(&mut self, base_env: ArenaIndex, collected: ArenaIndex) -> EvalResult {
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
    pub(super) fn step_quasiquote_trampoline(&mut self, template: ArenaIndex, env: ArenaIndex, depth: usize) 
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

    // Note: step_eval_let, step_eval_let_star, step_eval_letrec removed
    // These forms are now handled by macros during evaluation

    /// Evaluate begin using continuations (no Rust recursion)
    pub(super) fn step_eval_begin(&mut self, exprs: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
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

    // Note: step_eval_and and step_eval_or removed - and/or are now handled by macros

    pub(super) fn step_eval_case(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
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
    pub(super) fn step_eval_do(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
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
    pub(super) fn eval_quasiquote(&mut self, template: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        self.step_quasiquote_trampoline(template, env, 1)
    }
    
    /// Append two lists
    pub(super) fn append_lists(&self, a: ArenaIndex, b: ArenaIndex) -> EvalResult {
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
    pub(super) fn step_eval_apply(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let func_expr = self.lisp.car(args)?;
        let args_list_expr = self.lisp.car(self.lisp.cdr(args)?)?;
        
        // Push continuation to evaluate args_list after func is evaluated
        let data_start = self.pack_apply_first(args_list_expr, env)?;
        self.push_cont(Cont::ApplyFirst(data_start))?;
        
        // Evaluate function first
        Ok(TrampolineState::Eval { expr: func_expr, env })
    }
    
    /// Evaluate values - create a multi-value return (trampolined)
    pub(super) fn eval_values(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
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
    /// 
    /// Supports R7RS internal definitions: `define` forms at the start of the body
    /// are transformed to `letrec` semantics.
    pub(super) fn eval_lambda(&mut self, args: ArenaIndex, env: ArenaIndex) -> EvalResult {
        let params = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;
        
        // Check for internal defines and transform to letrec
        let body = self.transform_internal_defines(body_list)?;
        
        self.lisp.lambda(params, body, env).map_err(Into::into)
    }
    
    /// Transform internal defines at the start of a body to letrec
    /// 
    /// (define a 1) (define b 2) expr... -> (letrec ((a 1) (b 2)) expr...)
    fn transform_internal_defines(&self, body_list: ArenaIndex) -> EvalResult {
        // Collect internal defines
        let mut defines = self.lisp.nil()?;
        let mut remaining = body_list;
        
        loop {
            if self.lisp.get(remaining)?.is_nil() {
                break;
            }
            
            let expr = self.lisp.car(remaining)?;
            
            // Check if this is a define form
            if let Value::Cons { .. } = self.lisp.get(expr)? {
                let head = self.lisp.car(expr)?;
                if let Value::Symbol(_) = self.lisp.get(head)? {
                    if self.lisp.symbol_matches(head, "define")? {
                        // Extract name and value from define
                        let define_args = self.lisp.cdr(expr)?;
                        let first = self.lisp.car(define_args)?;
                        let rest = self.lisp.cdr(define_args)?;
                        
                        let binding = match self.lisp.get(first)? {
                            // (define name value)
                            Value::Symbol(_) => {
                                let name = first;
                                let value = self.lisp.car(rest)?;
                                // Create binding (name value)
                                let val_list = self.lisp.cons(value, self.lisp.nil()?)?;
                                self.lisp.cons(name, val_list)?
                            }
                            // (define (name params...) body...) -> (name (lambda (params...) body...))
                            Value::Cons { .. } => {
                                let name = self.lisp.car(first)?;
                                let lambda_params = self.lisp.cdr(first)?;
                                let lambda_body_list = rest;
                                
                                // Build (lambda (params...) body...)
                                // We need to handle internal defines recursively
                                let lambda_body = self.transform_internal_defines(lambda_body_list)?;
                                let lambda_sym = self.lisp.symbol("lambda")?;
                                let lambda_body_cell = self.lisp.cons(lambda_body, self.lisp.nil()?)?;
                                let lambda_with_params = self.lisp.cons(lambda_params, lambda_body_cell)?;
                                let lambda_expr = self.lisp.cons(lambda_sym, lambda_with_params)?;
                                
                                // Create binding (name (lambda ...))
                                let val_list = self.lisp.cons(lambda_expr, self.lisp.nil()?)?;
                                self.lisp.cons(name, val_list)?
                            }
                            _ => break, // Not a valid define, stop collecting
                        };
                        
                        // Prepend to defines list (will reverse later)
                        defines = self.lisp.cons(binding, defines)?;
                        remaining = self.lisp.cdr(remaining)?;
                        continue;
                    }
                }
            }
            
            // Not a define form, stop collecting
            break;
        }
        
        // If no internal defines, just process the body normally
        if self.lisp.get(defines)?.is_nil() {
            // Wrap body in begin if multiple expressions
            if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
                return self.lisp.car(body_list).map_err(Into::into);
            } else {
                let begin = self.lisp.symbol("begin")?;
                return self.lisp.cons(begin, body_list).map_err(Into::into);
            }
        }
        
        // Reverse defines to maintain definition order
        let bindings = self.reverse_list(defines)?;
        
        // Build body expression from remaining forms
        let body_expr = if self.lisp.get(remaining)?.is_nil() {
            // No body after defines - R7RS says this is an error, but we'll return nil
            self.lisp.nil()?
        } else if self.lisp.get(self.lisp.cdr(remaining)?)?.is_nil() {
            // Single expression
            self.lisp.car(remaining)?
        } else {
            // Multiple expressions - wrap in begin
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, remaining)?
        };
        
        // Build (letrec ((name1 val1) (name2 val2) ...) body)
        let letrec_sym = self.lisp.symbol("letrec")?;
        let body_cell = self.lisp.cons(body_expr, self.lisp.nil()?)?;
        let bindings_and_body = self.lisp.cons(bindings, body_cell)?;
        self.lisp.cons(letrec_sym, bindings_and_body).map_err(Into::into)
    }
    
    /// Evaluate define (trampolined)
    pub(super) fn eval_define(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
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
    pub(super) fn eval_set(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
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
    
    /// Evaluate (define-syntax name transformer) at evaluation time
    /// 
    /// This allows macros to be defined during evaluation rather than
    /// only during pre-expansion.
    pub(super) fn step_eval_define_syntax(&mut self, args: ArenaIndex, _env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let name = self.lisp.car(args)?;
        let transformer_expr = self.lisp.car(self.lisp.cdr(args)?)?;
        
        // Parse the transformer
        let transformer = self.parse_transformer(transformer_expr)?;
        
        // Add to macro environment
        let binding = self.lisp.cons(name, transformer)?;
        self.macro_env = self.lisp.cons(binding, self.macro_env)?;
        
        // Return unspecified value (nil)
        let nil = self.lisp.nil()?;
        Ok(TrampolineState::Return { val: nil })
    }
    
    /// Evaluate (let-syntax ((name transformer) ...) body ...) at evaluation time
    /// 
    /// Creates local macro bindings for the duration of the body.
    /// 
    /// NOTE: This is a simplified implementation. The macro bindings will remain
    /// in effect after the body returns (they "leak"). A full implementation would
    /// need a continuation to restore the macro environment after body evaluation.
    pub(super) fn step_eval_let_syntax(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;
        
        // Add local macro bindings
        let mut current = bindings;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let binding = self.lisp.car(current)?;
            let name = self.lisp.car(binding)?;
            let transformer_expr = self.lisp.car(self.lisp.cdr(binding)?)?;
            
            let transformer = self.parse_transformer(transformer_expr)?;
            let macro_binding = self.lisp.cons(name, transformer)?;
            self.macro_env = self.lisp.cons(macro_binding, self.macro_env)?;
            
            current = self.lisp.cdr(current)?;
        }
        
        // Build body expression (wrap in begin if multiple)
        let body = if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
            self.lisp.car(body_list)?
        } else {
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body_list)?
        };
        
        // Evaluate body with extended macro environment
        // Note: The macro bindings will persist after body returns (known limitation)
        Ok(TrampolineState::Eval { expr: body, env })
    }
}
