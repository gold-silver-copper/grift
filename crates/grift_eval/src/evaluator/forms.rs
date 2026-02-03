//! Special form handling for the evaluator.
//!
//! Contains step_return (continuation handling) and all step_eval_* forms
//! (begin, quasiquote, apply, values, etc.)
//!
//! Note: let, let*, letrec, letrec*, and, or, cond, case, do are now handled by macros.

use grift_parser::{ArenaIndex, Value, parse};

use crate::error::{ErrorKind, EvalError, EvalResult};
use crate::continuation::{Cont, TrampolineState};
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
                        
                        // Check for rest-argument lambda: (lambda args body) where args is a symbol
                        if self.lisp.get(params)?.is_symbol() {
                            // Rest-only lambda: all args collected into a single list
                            if self.lisp.get(args_expr)?.is_nil() {
                                // No args - bind to empty list
                                let nil = self.lisp.nil()?;
                                let extended_env = self.env_extend(closure_env, params, nil)?;
                                Ok(Some(TrampolineState::Eval { expr: body, env: extended_env }))
                            } else {
                                // Evaluate first arg and start collecting
                                let first_expr = self.lisp.car(args_expr)?;
                                let rest_exprs = self.lisp.cdr(args_expr)?;
                                let nil = self.lisp.nil()?;
                                
                                let data_start = self.pack_lambda_rest_collect(
                                    rest_exprs, env, params, body, closure_env, nil, call_expr
                                )?;
                                self.push_cont(Cont::LambdaRestCollect(data_start))?;
                                
                                Ok(Some(TrampolineState::Eval { expr: first_expr, env }))
                            }
                        } else if self.lisp.get(args_expr)?.is_nil() {
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
                    
                    // Check if remaining_params is a symbol (rest parameter for dotted lambda)
                    if self.lisp.get(remaining_params)?.is_symbol() {
                        // Dotted parameter: (a b . rest) - collect remaining args into rest
                        if self.lisp.get(remaining_exprs)?.is_nil() {
                            // No more args - bind rest param to empty list
                            let nil = self.lisp.nil()?;
                            let final_env = self.env_extend(extended_env, remaining_params, nil)?;
                            Ok(Some(TrampolineState::Eval { expr: body, env: final_env }))
                        } else {
                            // Start collecting rest args
                            let first_expr = self.lisp.car(remaining_exprs)?;
                            let rest_exprs = self.lisp.cdr(remaining_exprs)?;
                            let nil = self.lisp.nil()?;
                            
                            let data_start = self.pack_lambda_rest_collect(
                                rest_exprs, eval_env, remaining_params, body, extended_env, nil, call_expr
                            )?;
                            self.push_cont(Cont::LambdaRestCollect(data_start))?;
                            
                            Ok(Some(TrampolineState::Eval { expr: first_expr, env: eval_env }))
                        }
                    } else if self.lisp.get(remaining_exprs)?.is_nil() {
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
            
            Cont::LambdaRestCollect(data_start) => {
                // Collecting rest arguments into a list
                let (remaining_exprs, eval_env, rest_param, body, new_env, collected, call_expr) = 
                    self.unpack_lambda_rest_collect(data_start);
                
                // Add evaluated value to collected list
                let new_collected = self.lisp.cons(val, collected)?;
                
                if self.lisp.get(remaining_exprs)?.is_nil() {
                    // No more args - reverse collected and bind to rest_param
                    let rest_list = self.reverse_list(new_collected)?;
                    let extended_env = self.env_extend(new_env, rest_param, rest_list)?;
                    Ok(Some(TrampolineState::Eval { expr: body, env: extended_env }))
                } else {
                    // More args to collect
                    let next_expr = self.lisp.car(remaining_exprs)?;
                    let rest_exprs = self.lisp.cdr(remaining_exprs)?;
                    
                    let data_start = self.pack_lambda_rest_collect(
                        rest_exprs, eval_env, rest_param, body, new_env, new_collected, call_expr
                    )?;
                    self.push_cont(Cont::LambdaRestCollect(data_start))?;
                    
                    Ok(Some(TrampolineState::Eval { expr: next_expr, env: eval_env }))
                }
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

            // Note: CaseKey, DoInit, DoTestResult, DoBody, DoStep removed - now handled by macros (Phase 9)

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

            Cont::CallWithValuesProducer(data_start) => {
                let (consumer_expr, env) = self.unpack_call_with_values_producer(data_start);
                // val is the producer function - call it with no arguments
                // Build call expression: (producer)
                let nil = self.lisp.nil()?;
                let call_expr = self.lisp.cons(val, nil)?;
                
                // Push continuation to apply consumer after producer returns
                let data_start = self.pack_call_with_values_consumer(consumer_expr, env)?;
                self.push_cont(Cont::CallWithValuesConsumer(data_start))?;
                
                // Apply the producer (no arguments)
                self.push_frame(call_expr, val)?;
                let data_start = self.pack_apply_forced(nil, env, call_expr)?;
                self.push_cont(Cont::ApplyForced(data_start))?;
                Ok(Some(TrampolineState::Return { val }))
            }

            Cont::CallWithValuesConsumer(data_start) => {
                let (consumer_expr, env) = self.unpack_call_with_values_consumer(data_start);
                // val is the result from producer - could be a single value or a list from (values ...)
                // Now we need to evaluate consumer and apply it to the producer's result(s)
                
                // Store the producer result and push continuation to apply consumer
                let data_start = self.pack_call_with_values_apply(val, env)?;
                self.push_cont(Cont::CallWithValuesApply(data_start))?;
                
                // Evaluate the consumer expression
                Ok(Some(TrampolineState::Eval { expr: consumer_expr, env }))
            }

            Cont::CallWithValuesApply(data_start) => {
                let (producer_result, env) = self.unpack_call_with_values_apply(data_start);
                // val is the consumer function - apply it to producer_result
                // 
                // The producer_result handling depends on how the producer returned:
                // - If producer used (values a b c), producer_result is already (a b c) - a list
                // - If producer returned a single value normally, producer_result is that value
                //
                // R7RS semantics: A producer that doesn't explicitly call values returns
                // a single value, which becomes a single argument to consumer.
                // We need to wrap non-list single values in a list.
                let args_list = match self.lisp.get(producer_result)? {
                    // If it's nil (empty list from (values)), use it directly
                    Value::Nil => producer_result,
                    // If it's a cons (list from (values a b ...)), use it directly
                    Value::Cons { .. } => producer_result,
                    // If it's any other value (single return value), wrap it in a list
                    _ => {
                        let nil = self.lisp.nil()?;
                        self.lisp.cons(producer_result, nil)?
                    }
                };
                
                let call_expr = self.lisp.cons(val, args_list)?;
                
                self.push_frame(call_expr, val)?;
                let data_start = self.pack_apply_forced(args_list, env, call_expr)?;
                self.push_cont(Cont::ApplyForced(data_start))?;
                Ok(Some(TrampolineState::Return { val }))
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

            Cont::LetSyntaxBody(data_start) => {
                // Restore macro environment after let-syntax body evaluation
                let saved_macro_env = self.unpack_let_syntax_body(data_start);
                self.macro_env = saved_macro_env;
                // Return the value from the body
                Ok(Some(TrampolineState::Return { val }))
            }

            Cont::SyntaxCaseMatch(data_start) => {
                // val is the evaluated stx-expr
                // Now try to match it against clauses
                let (literals, clauses, env, pattern_bindings) = self.unpack_syntax_case_match(data_start);
                self.step_syntax_case_match(val, literals, clauses, env, pattern_bindings)
            }
            
            Cont::SyntaxCaseFender(data_start) => {
                // val is the evaluated fender result
                // If truthy, evaluate the output. Otherwise, continue with remaining clauses.
                let (output, bindings, literals, remaining_clauses, env, stx) = self.unpack_syntax_case_fender(data_start);
                
                if self.is_false(val)? {
                    // Fender failed - continue with remaining clauses
                    self.step_syntax_case_match(stx, literals, remaining_clauses, env, self.lisp.nil()?)
                } else {
                    // Fender passed - evaluate output with bindings
                    let output_env = self.extend_env_with_bindings(env, bindings)?;
                    Ok(Some(TrampolineState::Eval { expr: output, env: output_env }))
                }
            }
        }
    }

    // Note: step_eval_cond_cont removed - cond is now handled by macros

    // ========================================================================
    // Helper functions for fully trampolined evaluation
    // ========================================================================

    // Note: step_return_case_key, step_return_do_init, step_do_start_steps,
    // and apply_do_step_values removed - case and do are now handled by macros (Phase 9)

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
    // Note: step_eval_case and step_eval_do removed - case and do are now handled by macros (Phase 9)
    
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
    
    /// Evaluate call-with-values - call producer, apply consumer to results
    /// 
    /// (call-with-values producer consumer)
    /// 
    /// Calls producer with no arguments, then applies consumer to the values
    /// returned by producer. If producer returns multiple values (via values),
    /// those become the arguments to consumer.
    pub(super) fn step_eval_call_with_values(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let producer_expr = self.lisp.car(args)?;
        let consumer_expr = self.lisp.car(self.lisp.cdr(args)?)?;
        
        // Push continuation to call consumer after producer is evaluated and called
        let data_start = self.pack_call_with_values_producer(consumer_expr, env)?;
        self.push_cont(Cont::CallWithValuesProducer(data_start))?;
        
        // Evaluate producer first
        Ok(TrampolineState::Eval { expr: producer_expr, env })
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
    /// After body evaluation, the macro environment is restored via the 
    /// LetSyntaxBody continuation.
    pub(super) fn step_eval_let_syntax(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;
        
        // Save current macro environment for restoration after body
        let saved_macro_env = self.macro_env;
        
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
        
        // Push continuation to restore macro environment after body evaluation
        let data_start = self.pack_let_syntax_body(saved_macro_env)?;
        self.push_cont(Cont::LetSyntaxBody(data_start))?;
        
        // Evaluate body with extended macro environment
        Ok(TrampolineState::Eval { expr: body, env })
    }

    // ========================================================================
    // syntax-case - Procedural Macro Pattern Matching
    // ========================================================================

    /// Evaluate (syntax-case stx-expr (literal ...) clause ...)
    ///
    /// Each clause is either:
    /// - (pattern output)
    /// - (pattern fender output)
    ///
    /// The stx-expr is evaluated first, then matched against patterns.
    pub(super) fn step_eval_syntax_case(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> Result<TrampolineState, EvalError> {
        // Parse: (stx-expr (literal ...) clause ...)
        let stx_expr = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let literals = self.lisp.car(rest)?;
        let clauses = self.lisp.cdr(rest)?;

        // Start with empty pattern bindings (will be populated by caller via with-syntax)
        let pattern_bindings = self.get_pattern_bindings_from_env(env)?;

        // Push continuation to handle pattern matching after stx-expr is evaluated
        let data_start = self.pack_syntax_case_match(literals, clauses, env, pattern_bindings)?;
        self.push_cont(Cont::SyntaxCaseMatch(data_start))?;

        // Evaluate stx-expr first
        Ok(TrampolineState::Eval { expr: stx_expr, env })
    }

    /// Get pattern bindings from the current environment
    /// 
    /// In syntax-case, pattern bindings are stored in the environment.
    /// Uses a gensym-style internal symbol `#:pattern-bindings` to avoid
    /// conflicts with user code.
    fn get_pattern_bindings_from_env(&self, env: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        // Look for #:pattern-bindings in env (internal gensym-style name)
        let key = self.lisp.symbol("#:pattern-bindings")?;
        let mut current = env;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let binding = self.lisp.car(current)?;
            if let Value::Cons { .. } = self.lisp.get(binding)? {
                let name = self.lisp.car(binding)?;
                if self.lisp.symbol_eq(name, key)? {
                    return self.lisp.cdr(binding).map_err(Into::into);
                }
            }
            current = self.lisp.cdr(current)?;
        }
        
        // No pattern bindings - return empty
        self.lisp.nil().map_err(Into::into)
    }

    /// Continue syntax-case after stx-expr is evaluated
    fn step_syntax_case_match(
        &mut self,
        stx: ArenaIndex,
        literals: ArenaIndex,
        clauses: ArenaIndex,
        env: ArenaIndex,
        _pattern_bindings: ArenaIndex,
    ) -> Result<Option<TrampolineState>, EvalError> {
        // Try each clause in order
        let mut current = clauses;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let clause = self.lisp.car(current)?;
            let pattern = self.lisp.car(clause)?;
            let clause_cdr = self.lisp.cdr(clause)?;

            // Unwrap syntax object if present to get the datum
            let datum = self.lisp.syntax_to_datum(stx)?;

            // Try to match pattern against datum
            let empty = self.lisp.nil()?;
            if let Some(bindings) = self.match_pattern(pattern, datum, literals, empty)? {
                // Match succeeded! 
                // Check for fender (optional guard) - clause is (pattern fender output) or (pattern output)
                let (fender, output) = self.extract_fender_and_output(clause_cdr)?;

                // If there's a fender, use trampolined evaluation
                if let Some(fender_expr) = fender {
                    let fender_env = self.extend_env_with_bindings(env, bindings)?;
                    
                    // Get remaining clauses for if fender fails
                    let remaining_clauses = self.lisp.cdr(current)?;
                    
                    // Push continuation to handle fender result
                    let data_start = self.pack_syntax_case_fender(output, bindings, literals, remaining_clauses, env, stx)?;
                    self.push_cont(Cont::SyntaxCaseFender(data_start))?;
                    
                    // Evaluate fender with trampolined evaluation
                    return Ok(Some(TrampolineState::Eval { expr: fender_expr, env: fender_env }));
                }

                // No fender - evaluate output expression with pattern bindings
                let output_env = self.extend_env_with_bindings(env, bindings)?;
                return Ok(Some(TrampolineState::Eval { expr: output, env: output_env }));
            }

            current = self.lisp.cdr(current)?;
        }

        // No pattern matched - error
        Err(self.make_error(ErrorKind::Generic, stx)
            .with_message("syntax-case: no pattern matched"))
    }

    /// Extract fender and output from clause tail
    /// 
    /// Clause tail is either (output) or (fender output)
    fn extract_fender_and_output(
        &self,
        clause_cdr: ArenaIndex,
    ) -> Result<(Option<ArenaIndex>, ArenaIndex), EvalError> {
        let first = self.lisp.car(clause_cdr)?;
        let rest = self.lisp.cdr(clause_cdr)?;

        if self.lisp.get(rest)?.is_nil() {
            // Only one element - it's the output, no fender
            Ok((None, first))
        } else {
            // Two elements - first is fender, second is output
            let output = self.lisp.car(rest)?;
            Ok((Some(first), output))
        }
    }

    /// Extend environment with pattern bindings
    fn extend_env_with_bindings(
        &self,
        env: ArenaIndex,
        bindings: ArenaIndex,
    ) -> Result<ArenaIndex, EvalError> {
        // bindings is an alist of (var . value) pairs
        // Prepend each binding to env
        let mut result = env;
        let mut current = bindings;

        while let Value::Cons { .. } = self.lisp.get(current)? {
            let pair = self.lisp.car(current)?;
            result = self.lisp.cons(pair, result)?;
            current = self.lisp.cdr(current)?;
        }

        Ok(result)
    }

    // ========================================================================
    // syntax - Template Transcription
    // ========================================================================

    /// Evaluate (syntax template) - create syntax object from template
    ///
    /// This is similar to transcribe_template but operates at runtime
    /// using pattern bindings from the current environment.
    pub(super) fn step_eval_syntax(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> Result<TrampolineState, EvalError> {
        let template = self.lisp.car(args)?;

        // Get pattern bindings from environment
        let bindings = self.get_pattern_bindings_from_env(env)?;

        // Transcribe the template with pattern bindings
        let empty_renames = self.lisp.nil()?;
        let result = self.transcribe_template(template, bindings, empty_renames, self.global_env)?;

        Ok(TrampolineState::Return { val: result })
    }
}
