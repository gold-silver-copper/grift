//! Special form handling for the evaluator.
//!
//! Contains step_return (continuation handling) and all step_eval_* forms
//! (begin, quasiquote, apply, values, dynamic-wind, etc.)
//!
//! Note: let, let*, letrec, letrec*, and, or, cond, case, do are now handled by macros.

use grift_parser::{ArenaIndex, Value, parse};

use crate::error::{ErrorKind, EvalError, EvalResult};
use crate::continuation::{TrampolineState,
    CONT_DONE, CONT_APPLY_FORCED, CONT_IF_BRANCH, CONT_BUILTIN_FORCE_ARG,
    CONT_BINARY_BUILTIN_FIRST, CONT_BINARY_BUILTIN_SECOND, CONT_LAMBDA_FIRST_BIND,
    CONT_LAMBDA_BIND_ARG, CONT_LAMBDA_REST_COLLECT, CONT_EVAL_EXPR, CONT_BEGIN_SEQ,
    CONT_APPLY_FIRST, CONT_APPLY_SECOND, CONT_VALUES_COLLECT, CONT_DEFINE_VALUE,
    CONT_SET_VALUE, CONT_NATIVE_ARGS_COLLECT, CONT_QUASIQUOTE_CAR, CONT_QUASIQUOTE_CDR,
    CONT_QUASIQUOTE_UNQUOTE_WRAP, CONT_QUASIQUOTE_NESTED_WRAP, CONT_QUASIQUOTE_SPLICE,
    CONT_QUASIQUOTE_SPLICE_APPEND, CONT_LET_SYNTAX_BODY, CONT_CALL_WITH_VALUES_PRODUCER,
    CONT_CALL_WITH_VALUES_CONSUMER, CONT_CALL_WITH_VALUES_APPLY, CONT_SYNTAX_CASE_MATCH,
    CONT_SYNTAX_CASE_FENDER, CONT_CALL_CC_APPLY, CONT_CONTINUATION_APPLY,
    CONT_DYNAMIC_WIND_BEFORE, CONT_DYNAMIC_WIND_BODY, CONT_DYNAMIC_WIND_AFTER,
    CONT_DYNAMIC_WIND_AFTER_CALL, CONT_WIND_IN, CONT_WIND_OUT, CONT_DYNAMIC_WIND_EVAL_AFTER,
    CONT_DYNAMIC_WIND_CALL_BODY, CONT_FINISH_CONTINUATION_RESTORE, CONT_WITH_SYNTAX_BIND,
};
use crate::extract_args;

use super::Evaluator;

impl<'a, const N: usize> Evaluator<'a, N> {
    pub(super) fn step_return(&mut self, val: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
        // The cont_env field is stored for potential future use (e.g., debugging, stack traces)
        // but is not currently used during normal continuation processing.
        let (cont_type, data, _) = self.pop_cont()?;
        
        match cont_type {
            CONT_DONE => {
                // No more continuations - we're done
                Ok(None)
            }
            
            CONT_IF_BRANCH => {
                // Data: (then_expr . (else_expr . env))
                let (then_expr, else_expr, env) = self.unpack3(data)?;
                // val is the evaluated condition
                let branch = if !self.is_false(val)? { then_expr } else { else_expr };
                if branch.is_nil() {
                    let nil = self.lisp.nil()?;
                    Ok(Some(TrampolineState::Return { val: nil }))
                } else {
                    Ok(Some(TrampolineState::Eval { expr: branch, env }))
                }
            }
            
            CONT_APPLY_FORCED => {
                // Data: (args_expr . (env . call_expr))
                let (args_expr, env, call_expr) = self.unpack3(data)?;
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
                                
                                // Data: (remaining_exprs . (eval_env . (rest_param . (body . (new_env . (collected . call_expr))))))
                                let data = self.pack7(rest_exprs, env, params, body, closure_env, nil, call_expr)?;
                                self.push_cont(CONT_LAMBDA_REST_COLLECT, data, env)?;
                                
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
                            // Data for LambdaBindArg: (remaining_exprs . (eval_env . (remaining_params . (body . (new_env . call_expr)))))
                            let data = self.pack6(rest_exprs, env, rest_params, body, closure_env, call_expr)?;
                            self.push_cont(CONT_LAMBDA_BIND_ARG, data, env)?;
                            // Push binding continuation for first param
                            // Data for LambdaFirstBind: param
                            let data = self.pack1(first_param)?;
                            self.push_cont(CONT_LAMBDA_FIRST_BIND, data, env)?;
                            
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
                            let data = self.pack6(rest_exprs, env, rest_params, body, closure_env, call_expr)?;
                            self.push_cont(CONT_LAMBDA_BIND_ARG, data, env)?;
                            // Push binding continuation for first param
                            let data = self.pack1(first_param)?;
                            self.push_cont(CONT_LAMBDA_FIRST_BIND, data, env)?;
                            
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
                            
                            // Data: (remaining . (collected . (id_encoded . env)))
                            let id_encoded = Self::encode_usize(id);
                            let data = self.pack4(rest, nil, id_encoded, env)?;
                            self.push_cont(CONT_NATIVE_ARGS_COLLECT, data, env)?;
                            Ok(Some(TrampolineState::Eval { expr: first_expr, env }))
                        }
                    }
                    Value::Continuation { .. } => {
                        // Continuation invocation: (k arg)
                        // Continuations take exactly one argument
                        self.pop_frame();
                        
                        // Check we have exactly one argument
                        if self.lisp.get(args_expr)?.is_nil() {
                            return Err(self.make_error(ErrorKind::WrongArgCount, call_expr)
                                .with_message("continuation requires exactly 1 argument"));
                        }
                        let arg_expr = self.lisp.car(args_expr)?;
                        let rest = self.lisp.cdr(args_expr)?;
                        if !self.lisp.get(rest)?.is_nil() {
                            return Err(self.make_error(ErrorKind::WrongArgCount, call_expr)
                                .with_message("continuation requires exactly 1 argument"));
                        }
                        
                        // Push continuation to restore when arg is evaluated
                        // Data: captured_continuation
                        let data = self.pack1(val)?;
                        self.push_cont(CONT_CONTINUATION_APPLY, data, env)?;
                        
                        // Evaluate the argument
                        Ok(Some(TrampolineState::Eval { expr: arg_expr, env }))
                    }
                    _ => {
                        self.pop_frame();
                        Err(self.type_error(call_expr, "procedure", self.lisp.get(val)?.type_name()))
                    }
                }
            }
            
            CONT_LAMBDA_FIRST_BIND => {
                // Data: param
                let param = self.unpack1(data);
                // val is evaluated first arg - bind to param
                // Pop LambdaBindArg, extend env, push it back
                let (next_type, next_data, _) = self.pop_cont()?;
                if next_type == CONT_LAMBDA_BIND_ARG {
                    // Data: (remaining_exprs . (eval_env . (remaining_params . (body . (new_env . call_expr)))))
                    let (remaining_exprs, eval_env, remaining_params, body, new_env, call_expr) = 
                        self.unpack6(next_data)?;
                    
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
                            
                            let data = self.pack7(rest_exprs, eval_env, remaining_params, body, extended_env, nil, call_expr)?;
                            self.push_cont(CONT_LAMBDA_REST_COLLECT, data, eval_env)?;
                            
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
                        let new_data = self.pack6(rest_exprs, eval_env, rest_params, body, extended_env, call_expr)?;
                        self.push_cont(CONT_LAMBDA_BIND_ARG, new_data, eval_env)?;
                        let data = self.pack1(next_param)?;
                        self.push_cont(CONT_LAMBDA_FIRST_BIND, data, eval_env)?;
                        
                        Ok(Some(TrampolineState::Eval { expr: next_expr, env: eval_env }))
                    }
                } else {
                    // This shouldn't happen
                    Err(self.make_error(ErrorKind::Generic, val))
                }
            }
            
            CONT_LAMBDA_BIND_ARG => {
                // This shouldn't be hit directly - LambdaFirstBind pops it
                Err(self.make_error(ErrorKind::Generic, val))
            }
            
            CONT_LAMBDA_REST_COLLECT => {
                // Collecting rest arguments into a list
                // Data: (remaining_exprs . (eval_env . (rest_param . (body . (new_env . (collected . call_expr))))))
                let (remaining_exprs, eval_env, rest_param, body, new_env, collected, call_expr) = 
                    self.unpack7(data)?;
                
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
                    
                    let data = self.pack7(rest_exprs, eval_env, rest_param, body, new_env, new_collected, call_expr)?;
                    self.push_cont(CONT_LAMBDA_REST_COLLECT, data, eval_env)?;
                    
                    Ok(Some(TrampolineState::Eval { expr: next_expr, env: eval_env }))
                }
            }
            
            CONT_BUILTIN_FORCE_ARG => {
                // Data: (builtin_encoded . (remaining_args . (collected . (call_expr . eval_env))))
                let (builtin_encoded, remaining_args, collected, call_expr, eval_env) = self.unpack5(data)?;
                let builtin = Self::decode_builtin(builtin_encoded);
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
                    
                    let builtin_encoded = Self::encode_builtin(builtin);
                    let data = self.pack5(builtin_encoded, rest_args, new_collected, call_expr, eval_env)?;
                    self.push_cont(CONT_BUILTIN_FORCE_ARG, data, eval_env)?;
                    
                    Ok(Some(TrampolineState::Eval { expr: next_arg, env: eval_env }))
                }
            }
            
            CONT_BINARY_BUILTIN_FIRST => {
                // Data: (builtin_encoded . (second_arg . (call_expr . eval_env)))
                let (builtin_encoded, second_arg, call_expr, eval_env) = self.unpack4(data)?;
                // val is first evaluated arg - now evaluate second
                // Data for BinaryBuiltinSecond: (builtin_encoded . (first_val . call_expr))
                let data = self.pack3(builtin_encoded, val, call_expr)?;
                self.push_cont(CONT_BINARY_BUILTIN_SECOND, data, eval_env)?;
                Ok(Some(TrampolineState::Eval { expr: second_arg, env: eval_env }))
            }
            
            CONT_BINARY_BUILTIN_SECOND => {
                // Data: (builtin_encoded . (first_val . call_expr))
                let (builtin_encoded, first_val, call_expr) = self.unpack3(data)?;
                let builtin = Self::decode_builtin(builtin_encoded);
                // val is second evaluated arg - apply binary operation directly
                let result = self.apply_binary_builtin(builtin, first_val, val, call_expr)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            // Note: LetBinding, LetStarBinding, LetrecInit handlers removed
            // These forms are now handled by macros during evaluation

            // Note: When, Unless, CondTest, And, Or continuations removed
            // These forms are now handled by macros during expansion

            CONT_EVAL_EXPR => {
                // Data: env
                let env = self.unpack1(data);
                // val is the evaluated expression - now evaluate it
                Ok(Some(TrampolineState::Eval { expr: val, env }))
            }

            CONT_BEGIN_SEQ => {
                // Data: (remaining . env)
                let (remaining, env) = self.unpack2(data)?;
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
                        let data = self.pack2(rest, env)?;
                        self.push_cont(CONT_BEGIN_SEQ, data, env)?;
                        Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                    }
                }
            }

            // ================================================================
            // New continuation types for fully trampolined evaluation
            // ================================================================

            // Note: CaseKey, DoInit, DoTestResult, DoBody, DoStep removed - now handled by macros (Phase 9)

            CONT_APPLY_FIRST => {
                // Data: (args_list_expr . env)
                let (args_list_expr, env) = self.unpack2(data)?;
                // val is the evaluated function - now evaluate args list
                let data = self.pack2(val, env)?;
                self.push_cont(CONT_APPLY_SECOND, data, env)?;
                Ok(Some(TrampolineState::Eval { expr: args_list_expr, env }))
            }

            CONT_APPLY_SECOND => {
                // Data: (func . env)
                let (func, env) = self.unpack2(data)?;
                // val is the evaluated args list - perform application
                let args_list = val;
                let call_expr = self.lisp.cons(func, args_list)?;
                self.push_frame(call_expr, func)?;
                let data = self.pack3(args_list, env, call_expr)?;
                self.push_cont(CONT_APPLY_FORCED, data, env)?;
                Ok(Some(TrampolineState::Return { val: func }))
            }

            CONT_VALUES_COLLECT => {
                // Data: (remaining . (collected . env))
                let (remaining, collected, env) = self.unpack3(data)?;
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
                    let data = self.pack3(rest, new_collected, env)?;
                    self.push_cont(CONT_VALUES_COLLECT, data, env)?;
                    Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                }
            }

            CONT_CALL_WITH_VALUES_PRODUCER => {
                // Data: (consumer_expr . env)
                let (consumer_expr, env) = self.unpack2(data)?;
                // val is the producer function - call it with no arguments
                // Build call expression: (producer)
                let nil = self.lisp.nil()?;
                let call_expr = self.lisp.cons(val, nil)?;
                
                // Push continuation to apply consumer after producer returns
                let data = self.pack2(consumer_expr, env)?;
                self.push_cont(CONT_CALL_WITH_VALUES_CONSUMER, data, env)?;
                
                // Apply the producer (no arguments)
                self.push_frame(call_expr, val)?;
                let data = self.pack3(nil, env, call_expr)?;
                self.push_cont(CONT_APPLY_FORCED, data, env)?;
                Ok(Some(TrampolineState::Return { val }))
            }

            CONT_CALL_WITH_VALUES_CONSUMER => {
                // Data: (consumer_expr . env)
                let (consumer_expr, env) = self.unpack2(data)?;
                // val is the result from producer - could be a single value or a list from (values ...)
                // Now we need to evaluate consumer and apply it to the producer's result(s)
                
                // Store the producer result and push continuation to apply consumer
                let data = self.pack2(val, env)?;
                self.push_cont(CONT_CALL_WITH_VALUES_APPLY, data, env)?;
                
                // Evaluate the consumer expression
                Ok(Some(TrampolineState::Eval { expr: consumer_expr, env }))
            }

            CONT_CALL_WITH_VALUES_APPLY => {
                // Data: (producer_result . env)
                let (producer_result, env) = self.unpack2(data)?;
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
                let data = self.pack3(args_list, env, call_expr)?;
                self.push_cont(CONT_APPLY_FORCED, data, env)?;
                Ok(Some(TrampolineState::Return { val }))
            }

            CONT_DEFINE_VALUE => {
                // Data: name
                let name = self.unpack1(data);
                // val is the evaluated value - define the binding
                self.define(name, val)?;
                Ok(Some(TrampolineState::Return { val: name }))
            }

            CONT_SET_VALUE => {
                // Data: (name . env)
                let (name, env) = self.unpack2(data)?;
                // val is the evaluated value - set! the binding
                self.env_set(env, name, val)?;
                Ok(Some(TrampolineState::Return { val }))
            }

            CONT_NATIVE_ARGS_COLLECT => {
                // Data: (remaining . (collected . (id_encoded . env)))
                let (remaining, collected, id_encoded, env) = self.unpack4(data)?;
                let id = Self::decode_usize(id_encoded);
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
                    let id_encoded = Self::encode_usize(id);
                    let data = self.pack4(rest, new_collected, id_encoded, env)?;
                    self.push_cont(CONT_NATIVE_ARGS_COLLECT, data, env)?;
                    Ok(Some(TrampolineState::Eval { expr: next_expr, env }))
                }
            }

            CONT_QUASIQUOTE_CAR => {
                // Data: (cdr . (depth_encoded . env))
                let (cdr, depth_encoded, env) = self.unpack3(data)?;
                let depth = Self::decode_usize(depth_encoded);
                // val is the evaluated car - now process cdr
                let car_val = val;
                let data = self.pack1(car_val)?;
                self.push_cont(CONT_QUASIQUOTE_CDR, data, env)?;
                Ok(Some(self.step_quasiquote_trampoline(cdr, env, depth)?))
            }

            CONT_QUASIQUOTE_CDR => {
                // Data: car_val
                let car_val = self.unpack1(data);
                // val is the processed cdr - cons with car
                let result = self.lisp.cons(car_val, val)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            CONT_QUASIQUOTE_UNQUOTE_WRAP => {
                // val is the inner processed value - wrap with unquote
                let unquote_sym = self.lisp.symbol("unquote")?;
                let nil = self.lisp.nil()?;
                let inner_list = self.lisp.cons(val, nil)?;
                let result = self.lisp.cons(unquote_sym, inner_list)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            CONT_QUASIQUOTE_NESTED_WRAP => {
                // val is the inner processed value - wrap with quasiquote
                let qq_sym = self.lisp.symbol("quasiquote")?;
                let nil = self.lisp.nil()?;
                let inner_list = self.lisp.cons(val, nil)?;
                let result = self.lisp.cons(qq_sym, inner_list)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            CONT_QUASIQUOTE_SPLICE => {
                // Data: (cdr . (depth_encoded . env))
                let (cdr, depth_encoded, env) = self.unpack3(data)?;
                let depth = Self::decode_usize(depth_encoded);
                // val is the evaluated splice expression - process cdr then append
                let splice_val = val;
                let data = self.pack1(splice_val)?;
                self.push_cont(CONT_QUASIQUOTE_SPLICE_APPEND, data, env)?;
                Ok(Some(self.step_quasiquote_trampoline(cdr, env, depth)?))
            }

            CONT_QUASIQUOTE_SPLICE_APPEND => {
                // Data: splice_val
                let splice_val = self.unpack1(data);
                // val is the processed cdr - append splice_val with it
                let result = self.append_lists(splice_val, val)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            CONT_LET_SYNTAX_BODY => {
                // Restore macro environment after let-syntax body evaluation
                // Data: saved_macro_env
                let saved_macro_env = self.unpack1(data);
                self.macro_env = saved_macro_env;
                // Return the value from the body
                Ok(Some(TrampolineState::Return { val }))
            }

            CONT_SYNTAX_CASE_MATCH => {
                // val is the evaluated stx-expr
                // Now try to match it against clauses
                // Data: (literals . (clauses . (env . pattern_bindings)))
                let (literals, clauses, env, pattern_bindings) = self.unpack4(data)?;
                self.step_syntax_case_match(val, literals, clauses, env, pattern_bindings)
            }
            
            CONT_SYNTAX_CASE_FENDER => {
                // val is the evaluated fender result
                // If truthy, evaluate the output. Otherwise, continue with remaining clauses.
                // Data: (output . (bindings . (literals . (remaining_clauses . (env . stx)))))
                let (output, bindings, literals, remaining_clauses, env, stx) = self.unpack6(data)?;
                
                if self.is_false(val)? {
                    // Fender failed - continue with remaining clauses
                    self.step_syntax_case_match(stx, literals, remaining_clauses, env, self.lisp.nil()?)
                } else {
                    // Fender passed - evaluate output with bindings
                    let output_env = self.extend_env_with_bindings(env, bindings)?;
                    Ok(Some(TrampolineState::Eval { expr: output, env: output_env }))
                }
            }
            
            CONT_CALL_CC_APPLY => {
                // val is the evaluated procedure from (call/cc proc)
                // We now need to apply it to the captured continuation
                // Data: captured_continuation
                let captured_continuation = self.unpack1(data);
                
                // Create argument list with just the captured continuation
                let nil = self.lisp.nil()?;
                let args = self.lisp.cons(captured_continuation, nil)?;
                
                // Apply the procedure to the continuation
                // We need to determine if it's a lambda, builtin, etc.
                match self.lisp.get(val)? {
                    Value::Lambda { .. } => {
                        let (params, body, closure_env) = self.lisp.lambda_parts(val)?;
                        
                        // Check param count - should be exactly 1
                        if self.lisp.get(params)?.is_nil() {
                            return Err(self.make_error(ErrorKind::WrongArgCount, val)
                                .with_message("call/cc procedure must accept 1 argument"));
                        }
                        let first_param = self.lisp.car(params)?;
                        let rest_params = self.lisp.cdr(params)?;
                        if !self.lisp.get(rest_params)?.is_nil() {
                            return Err(self.make_error(ErrorKind::WrongArgCount, val)
                                .with_message("call/cc procedure must accept exactly 1 argument"));
                        }
                        
                        // Bind the captured continuation to the parameter
                        let extended_env = self.env_extend(closure_env, first_param, captured_continuation)?;
                        Ok(Some(TrampolineState::Eval { expr: body, env: extended_env }))
                    }
                    _ => {
                        // For other callable types, use the standard apply mechanism
                        // Create a call expression and go through ApplyForced
                        let call_expr = self.lisp.cons(val, args)?;
                        self.push_frame(call_expr, val)?;
                        let data = self.pack3(args, self.global_env, call_expr)?;
                        self.push_cont(CONT_APPLY_FORCED, data, self.global_env)?;
                        Ok(Some(TrampolineState::Return { val }))
                    }
                }
            }
            
            CONT_CONTINUATION_APPLY => {
                // val is the evaluated argument to the captured continuation
                // Now we restore the captured continuation and return val as the result
                // Data: captured_continuation
                let captured_continuation = self.unpack1(data);
                
                // Extract target dynamic-wind chain from the continuation
                let (_cont_chain, _capture_env, target_dw_chain) = 
                    self.lisp.continuation_parts(captured_continuation)?;
                
                // Check if we need to execute dynamic-wind thunks
                if !self.dynamic_wind_chains_equal(self.dynamic_wind_chain, target_dw_chain)? {
                    // Need to wind out of current chain and wind into target chain
                    // First, compute the frames to wind out and wind in
                    let (wind_out_frames, wind_in_frames) = 
                        self.compute_wind_frames(self.dynamic_wind_chain, target_dw_chain)?;
                    
                    // Push continuation to finish restoration after winding
                    let finish_data = self.pack2(captured_continuation, val)?;
                    self.push_cont(CONT_FINISH_CONTINUATION_RESTORE, finish_data, self.global_env)?;
                    
                    // Start the winding process - first wind out, then wind in
                    return self.start_wind_transition(wind_out_frames, wind_in_frames, val, target_dw_chain);
                }
                
                // No dynamic-wind transitions needed - restore directly
                self.restore_continuation(captured_continuation)?;
                
                // Return val as the result of the original call/cc
                Ok(Some(TrampolineState::Return { val }))
            }
            
            CONT_FINISH_CONTINUATION_RESTORE => {
                // val is the result from winding (ignored)
                // Data: (captured_continuation . return_val)
                let (captured_continuation, return_val) = self.unpack2(data)?;
                
                // Dynamic-wind chain should already be updated by the winding process
                // Just restore the continuation and return the value
                self.restore_continuation(captured_continuation)?;
                
                // Return the original value that was passed to the continuation
                Ok(Some(TrampolineState::Return { val: return_val }))
            }
            
            CONT_DYNAMIC_WIND_BEFORE => {
                // val is the evaluated before thunk
                // Now we need to call it (no args) before running the body
                // Data: (body_expr . (after_expr . (env . saved_dw_chain)))
                let (body_expr, after_expr, env, saved_dw_chain) = self.unpack4(data)?;
                
                // Push continuation for after calling before thunk
                // We need to pass body_expr and after_expr to the next stage
                let data2 = self.pack4(body_expr, after_expr, env, saved_dw_chain)?;
                // Also save the before thunk in the data so we can add it to the dw chain
                let data3 = self.pack2(val, data2)?; // (before_thunk . (body_expr . (after_expr . (env . saved_dw_chain))))
                self.push_cont(CONT_DYNAMIC_WIND_BODY, data3, env)?;
                
                // Call the before thunk (no args)
                self.apply_thunk(val, env)
            }
            
            CONT_DYNAMIC_WIND_BODY => {
                // val is the result of calling the before thunk (ignored)
                // Now we need to evaluate the body thunk
                // Data: (before_thunk . (body_expr . (after_expr . (env . saved_dw_chain))))
                let (before_thunk, rest) = self.unpack2(data)?;
                let (body_expr, after_expr, env, saved_dw_chain) = self.unpack4(rest)?;
                
                // Now we need to evaluate after_expr, then body_expr, then call body
                // But we also need to add (before, after) to the dynamic-wind chain
                // before calling body.
                
                // Push continuation for after evaluating after_expr
                // Data: (before_thunk . (body_expr . (env . saved_dw_chain)))
                let data2 = self.pack4(before_thunk, body_expr, env, saved_dw_chain)?;
                self.push_cont(CONT_DYNAMIC_WIND_EVAL_AFTER, data2, env)?;
                
                // Evaluate the after thunk expression
                Ok(Some(TrampolineState::Eval { expr: after_expr, env }))
            }
            
            CONT_DYNAMIC_WIND_EVAL_AFTER => {
                // val is the evaluated after thunk
                // Now we need to evaluate the body thunk
                // Data: (before_thunk . (body_expr . (env . saved_dw_chain)))
                let (before_thunk, body_expr, env, saved_dw_chain) = self.unpack4(data)?;
                
                // Push continuation for after evaluating body_expr
                // Data: (before_thunk . (after_thunk . (env . saved_dw_chain)))
                let data2 = self.pack4(before_thunk, val, env, saved_dw_chain)?;
                self.push_cont(CONT_DYNAMIC_WIND_CALL_BODY, data2, env)?;
                
                // Evaluate the body thunk expression
                Ok(Some(TrampolineState::Eval { expr: body_expr, env }))
            }
            
            CONT_DYNAMIC_WIND_CALL_BODY => {
                // val is the evaluated body thunk
                // Now we need to push the dynamic-wind frame and call the body thunk
                // Data: (before_thunk . (after_thunk . (env . saved_dw_chain)))
                let (before_thunk, after_thunk, env, saved_dw_chain) = self.unpack4(data)?;
                
                // Create the before/after pair and push onto dynamic-wind chain
                let before_after = self.lisp.cons(before_thunk, after_thunk)?;
                self.dynamic_wind_chain = self.lisp.cons(before_after, saved_dw_chain)?;
                
                // Push continuation for after body thunk completes
                // Data: (after_thunk . saved_dw_chain)
                let data2 = self.pack2(after_thunk, saved_dw_chain)?;
                self.push_cont(CONT_DYNAMIC_WIND_AFTER, data2, env)?;
                
                // Call the body thunk
                self.apply_thunk(val, env)
            }
            
            CONT_DYNAMIC_WIND_AFTER => {
                // val is the result of calling the body thunk
                // Now we need to call the after thunk (already evaluated)
                // Data: (after_thunk . saved_dw_chain)
                let (after_thunk, saved_dw_chain) = self.unpack2(data)?;
                
                // Save the body result for after the after thunk runs
                let data2 = self.pack2(val, saved_dw_chain)?;
                self.push_cont(CONT_DYNAMIC_WIND_AFTER_CALL, data2, self.global_env)?;
                
                // Call the after thunk (no args)
                self.apply_thunk(after_thunk, self.global_env)
            }
            
            CONT_DYNAMIC_WIND_AFTER_CALL => {
                // val is the result of calling after thunk (ignored)
                // Return the body result and restore dynamic-wind chain
                // Data: (body_result . saved_dw_chain)
                let (body_result, saved_dw_chain) = self.unpack2(data)?;
                
                // Restore the dynamic-wind chain
                self.dynamic_wind_chain = saved_dw_chain;
                
                // Return the body's result
                Ok(Some(TrampolineState::Return { val: body_result }))
            }
            
            CONT_WIND_OUT => {
                // val is result of calling an after thunk (ignored)
                // Data: (remaining_frames . (return_val . (target_chain . wind_in_frames)))
                let (remaining_frames, return_val, target_chain, wind_in_frames) = self.unpack4(data)?;
                
                if self.lisp.get(remaining_frames)?.is_nil() {
                    // Done winding out, now wind in
                    self.start_wind_in(wind_in_frames, return_val, target_chain)
                } else {
                    // More frames to wind out
                    let frame = self.lisp.car(remaining_frames)?;
                    let rest = self.lisp.cdr(remaining_frames)?;
                    
                    // Get the after thunk from this frame: ((before . after) . parent)
                    let (before_after, _parent) = self.unpack2(frame)?;
                    let (_before, after) = self.unpack2(before_after)?;
                    
                    // Pop this frame from the current dynamic-wind chain
                    self.dynamic_wind_chain = self.lisp.cdr(self.dynamic_wind_chain)?;
                    
                    // Push continuation for next wind-out step
                    let data2 = self.pack4(rest, return_val, target_chain, wind_in_frames)?;
                    self.push_cont(CONT_WIND_OUT, data2, self.global_env)?;
                    
                    // Call the after thunk
                    self.apply_thunk(after, self.global_env)
                }
            }
            
            CONT_WIND_IN => {
                // val is result of calling a before thunk (ignored)
                // Data: (remaining_frames . (return_val . (target_chain . original_target)))
                let (remaining_frames, return_val, target_chain, original_target) = self.unpack4(data)?;
                
                if self.lisp.get(remaining_frames)?.is_nil() {
                    // Done winding in, now we can complete the continuation restore
                    self.dynamic_wind_chain = original_target;
                    Ok(Some(TrampolineState::Return { val: return_val }))
                } else {
                    // More frames to wind in
                    let frame = self.lisp.car(remaining_frames)?;
                    let rest = self.lisp.cdr(remaining_frames)?;
                    
                    // Get the before thunk from this frame: ((before . after) . parent)
                    let (before_after, _parent) = self.unpack2(frame)?;
                    let (before, _after) = self.unpack2(before_after)?;
                    
                    // Push this frame onto the current dynamic-wind chain
                    self.dynamic_wind_chain = self.lisp.cons(before_after, self.dynamic_wind_chain)?;
                    
                    // Push continuation for next wind-in step
                    let data2 = self.pack4(rest, return_val, target_chain, original_target)?;
                    self.push_cont(CONT_WIND_IN, data2, self.global_env)?;
                    
                    // Call the before thunk
                    self.apply_thunk(before, self.global_env)
                }
            }
            
            CONT_WITH_SYNTAX_BIND => {
                // val is the evaluated value for the current binding
                // Data format: ((current_name . rest_bindings) . (collected . (body . (env . existing))))
                let (remaining_bindings, collected_bindings, body, env, existing_pattern_bindings) = self.unpack5(data)?;
                
                // Extract current binding name from the packed data
                let current_name = self.lisp.car(remaining_bindings)?;
                let actual_remaining = self.lisp.cdr(remaining_bindings)?;
                
                // Add the binding to collected pattern bindings
                let binding_pair = self.lisp.cons(current_name, val)?;
                let new_collected = self.lisp.cons(binding_pair, collected_bindings)?;
                
                if self.lisp.get(actual_remaining)?.is_nil() {
                    // All bindings evaluated - now evaluate the body
                    // Build the final environment with #:pattern-bindings updated
                    
                    // Merge new bindings with existing pattern bindings
                    let mut final_pattern_bindings = existing_pattern_bindings;
                    let mut current = new_collected;
                    while let Value::Cons { .. } = self.lisp.get(current)? {
                        let pair = self.lisp.car(current)?;
                        final_pattern_bindings = self.lisp.cons(pair, final_pattern_bindings)?;
                        current = self.lisp.cdr(current)?;
                    }
                    
                    // Add the variables to the regular environment too
                    let mut extended_env = env;
                    current = new_collected;
                    while let Value::Cons { .. } = self.lisp.get(current)? {
                        let pair = self.lisp.car(current)?;
                        extended_env = self.lisp.cons(pair, extended_env)?;
                        current = self.lisp.cdr(current)?;
                    }
                    
                    // Update #:pattern-bindings in the environment
                    let key = self.lisp.symbol("#:pattern-bindings")?;
                    let bindings_pair = self.lisp.cons(key, final_pattern_bindings)?;
                    extended_env = self.lisp.cons(bindings_pair, extended_env)?;
                    
                    // Evaluate the body (which may be multiple expressions)
                    Ok(Some(self.step_eval_begin(body, extended_env)?))
                } else {
                    // More bindings to evaluate
                    let next_binding = self.lisp.car(actual_remaining)?;
                    let next_name = self.lisp.car(next_binding)?;
                    let next_val_expr = self.lisp.car(self.lisp.cdr(next_binding)?)?;
                    let rest_bindings = self.lisp.cdr(actual_remaining)?;
                    
                    // Pack data for continuation: (next_name . rest_bindings) as the remaining
                    let next_remaining = self.lisp.cons(next_name, rest_bindings)?;
                    let data = self.pack5(next_remaining, new_collected, body, env, existing_pattern_bindings)?;
                    self.push_cont(CONT_WITH_SYNTAX_BIND, data, env)?;
                    
                    // Evaluate the next binding value
                    Ok(Some(TrampolineState::Eval { expr: next_val_expr, env }))
                }
            }
            
            // Catch-all for unknown continuation types
            _ => {
                Err(self.make_error(ErrorKind::Generic, val)
                    .with_message("unknown continuation type"))
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
                        let nil = self.lisp.nil()?;
                        self.push_cont(CONT_QUASIQUOTE_UNQUOTE_WRAP, nil, env)?;
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
                    let nil = self.lisp.nil()?;
                    self.push_cont(CONT_QUASIQUOTE_NESTED_WRAP, nil, env)?;
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
                        // Data: (cdr . (depth_encoded . env))
                        let depth_encoded = Self::encode_usize(depth);
                        let data = self.pack3(cdr, depth_encoded, env)?;
                        self.push_cont(CONT_QUASIQUOTE_SPLICE, data, env)?;
                        return Ok(TrampolineState::Eval { expr: splice_expr, env });
                    }
                }
                
                // Recursively process car and cdr
                // Data: (cdr . (depth_encoded . env))
                let depth_encoded = Self::encode_usize(depth);
                let data = self.pack3(cdr, depth_encoded, env)?;
                self.push_cont(CONT_QUASIQUOTE_CAR, data, env)?;
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
            // Data: (remaining . env)
            let data = self.pack2(rest, env)?;
            self.push_cont(CONT_BEGIN_SEQ, data, env)?;
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
        // Data: (args_list_expr . env)
        let data = self.pack2(args_list_expr, env)?;
        self.push_cont(CONT_APPLY_FIRST, data, env)?;
        
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
        
        // Data: (remaining . (collected . env))
        let data = self.pack3(rest, nil, env)?;
        self.push_cont(CONT_VALUES_COLLECT, data, env)?;
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
        // Data: (consumer_expr . env)
        let data = self.pack2(consumer_expr, env)?;
        self.push_cont(CONT_CALL_WITH_VALUES_PRODUCER, data, env)?;
        
        // Evaluate producer first
        Ok(TrampolineState::Eval { expr: producer_expr, env })
    }
    
    /// Evaluate call-with-current-continuation (call/cc)
    /// 
    /// (call/cc proc) or (call-with-current-continuation proc)
    /// 
    /// Captures the current continuation as a first-class value and calls proc
    /// with that continuation as its only argument. If proc returns normally,
    /// that value becomes the result of call/cc. If the captured continuation
    /// is ever called with a value, that value immediately becomes the result
    /// of the call/cc, abandoning the current computation.
    pub(super) fn step_eval_call_cc(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        // Check that we have exactly one argument (the procedure)
        if self.lisp.get(args)?.is_nil() {
            return Err(self.make_error(ErrorKind::WrongArgCount, args)
                .with_message("call/cc requires exactly 1 argument"));
        }
        let proc_expr = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        if !self.lisp.get(rest)?.is_nil() {
            return Err(self.make_error(ErrorKind::WrongArgCount, args)
                .with_message("call/cc requires exactly 1 argument"));
        }
        
        // Capture the current continuation BEFORE evaluating the procedure
        // This is the continuation that will be restored when the captured
        // continuation is invoked
        let captured_continuation = self.capture_continuation(env)?;
        
        // Push continuation to apply proc to the captured continuation after proc is evaluated
        // Data: captured_continuation
        let data = self.pack1(captured_continuation)?;
        self.push_cont(CONT_CALL_CC_APPLY, data, env)?;
        
        // Evaluate the procedure expression
        Ok(TrampolineState::Eval { expr: proc_expr, env })
    }
    
    /// Capture the current continuation as a first-class value
    /// 
    /// With arena-based continuations, capture is O(1) - we just save the
    /// current_cont pointer as a Continuation value.
    fn capture_continuation(&self, env: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        // The current continuation is already an arena-based ContFrame chain
        let cont_chain = self.current_cont;
        
        // Create the continuation value with the current dynamic-wind chain
        let continuation = self.lisp.continuation(cont_chain, env, self.dynamic_wind_chain)?;
        
        Ok(continuation)
    }
    
    /// Restore a captured continuation
    /// 
    /// With arena-based continuations, restore is O(1) for the basic case.
    /// If dynamic-wind is involved, we need to call appropriate before/after thunks.
    fn restore_continuation(&mut self, continuation: ArenaIndex) -> Result<(), EvalError> {
        // Extract the cont_chain and dynamic-wind chain from the Continuation value
        let (cont_chain, _capture_env, captured_dw_chain) = self.lisp.continuation_parts(continuation)?;
        
        // Restore the continuation chain
        self.current_cont = cont_chain;
        
        // Restore the dynamic-wind chain (for now, simple replacement)
        // Full dynamic-wind handling with thunk execution is done via continuation types
        self.dynamic_wind_chain = captured_dw_chain;
        
        Ok(())
    }
    
    /// Evaluate lambda
    /// 
    /// Supports R7RS internal definitions: `define` forms at the start of the body
    /// are transformed to `letrec` semantics.
    /// 
    /// **Important**: Macro expansion happens here at lambda creation time (not at
    /// call time). This ensures side effects in macros execute once during expansion,
    /// not on every call to the lambda.
    pub(super) fn eval_lambda(&mut self, args: ArenaIndex, env: ArenaIndex) -> EvalResult {
        let params = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;
        
        // Check for internal defines and transform to letrec
        let body = self.transform_internal_defines(body_list)?;
        
        // Expand macros in the body at lambda creation time (not at call time)
        // This ensures macro side effects run during expansion, not during each call
        let expanded_body = self.expand(body)?;
        
        self.lisp.lambda(params, expanded_body, env).map_err(Into::into)
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
                // Data: name
                let data = self.pack1(first)?;
                self.push_cont(CONT_DEFINE_VALUE, data, env)?;
                Ok(TrampolineState::Eval { expr: value_expr, env })
            }
            // (define (name params...) body...) -> (define name (lambda (params...) body...))
            Value::Cons { .. } => {
                let name = self.lisp.car(first)?;
                let params = self.lisp.cdr(first)?;
                let body_list = rest;
                
                // Transform internal defines if needed
                let body = self.transform_internal_defines(body_list)?;
                
                // Expand macros in the body at definition time (not at call time)
                // This ensures macro side effects run during expansion, not during each call
                let expanded_body = self.expand(body)?;
                
                let lambda = self.lisp.lambda(params, expanded_body, env)?;
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
                // Data: (name . env)
                let data = self.pack2(name, env)?;
                self.push_cont(CONT_SET_VALUE, data, env)?;
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
        // Data: saved_macro_env
        let data = self.pack1(saved_macro_env)?;
        self.push_cont(CONT_LET_SYNTAX_BODY, data, env)?;
        
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
        // Data: (literals . (clauses . (env . pattern_bindings)))
        let data = self.pack4(literals, clauses, env, pattern_bindings)?;
        self.push_cont(CONT_SYNTAX_CASE_MATCH, data, env)?;

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
                    // Data: (output . (bindings . (literals . (remaining_clauses . (env . stx)))))
                    let data = self.pack6(output, bindings, literals, remaining_clauses, env, stx)?;
                    self.push_cont(CONT_SYNTAX_CASE_FENDER, data, fender_env)?;
                    
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
    /// 
    /// This adds pattern bindings to the environment in two ways:
    /// 1. Each binding is added directly for normal variable lookup
    /// 2. The full bindings alist is stored under `#:pattern-bindings` for
    ///    use by the `syntax` form for template transcription
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

        // Also store the full bindings alist under #:pattern-bindings
        // This allows the `syntax` form to retrieve just the pattern bindings
        // without picking up other environment bindings (like builtins)
        let key = self.lisp.symbol("#:pattern-bindings")?;
        let binding_pair = self.lisp.cons(key, bindings)?;
        result = self.lisp.cons(binding_pair, result)?;

        Ok(result)
    }

    // ========================================================================
    // syntax - Template Transcription
    // ========================================================================

    /// Evaluate (syntax template) - create syntax object from template
    ///
    /// This is similar to transcribe_template but operates at runtime
    /// using pattern bindings from the current environment.
    /// 
    /// The pattern bindings are stored under the special `#:pattern-bindings`
    /// key by `extend_env_with_bindings` when syntax-case matches.
    pub(super) fn step_eval_syntax(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> Result<TrampolineState, EvalError> {
        let template = self.lisp.car(args)?;

        // Get pattern bindings from the special key in the environment
        // This contains only the pattern variable bindings, not other env bindings
        let bindings = self.get_pattern_bindings_from_env(env)?;

        // Transcribe the template with pattern bindings
        let empty_renames = self.lisp.nil()?;
        let result = self.transcribe_template(template, bindings, empty_renames, self.global_env)?;

        Ok(TrampolineState::Return { val: result })
    }
    
    /// Evaluate (with-syntax ((pattern expr) ...) body ...)
    ///
    /// This binds pattern variables for use in syntax templates.
    /// Each expr is evaluated and bound to the corresponding pattern variable.
    /// The bindings are added to both the regular environment and the
    /// #:pattern-bindings for use by the (syntax ...) form.
    ///
    /// This is a special form (not a macro) because it needs to properly
    /// update the #:pattern-bindings mechanism used by syntax templates.
    pub(super) fn step_eval_with_syntax(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> Result<TrampolineState, EvalError> {
        let bindings_expr = self.lisp.car(args)?;
        let body = self.lisp.cdr(args)?;
        
        // Get existing pattern bindings
        let existing_pattern_bindings = self.get_pattern_bindings_from_env(env)?;
        
        // If no bindings, just evaluate the body
        if self.lisp.get(bindings_expr)?.is_nil() {
            return self.step_eval_begin(body, env);
        }
        
        // Start evaluating bindings
        let first_binding = self.lisp.car(bindings_expr)?;
        let rest_bindings = self.lisp.cdr(bindings_expr)?;
        
        let first_name = self.lisp.car(first_binding)?;
        let first_val_expr = self.lisp.car(self.lisp.cdr(first_binding)?)?;
        
        // Pack data for continuation:
        // (first_name . rest_bindings) - first_name paired with remaining bindings
        // collected_bindings - starts empty
        // body, env, existing_pattern_bindings
        let collected = self.lisp.nil()?;
        let remaining = self.lisp.cons(first_name, rest_bindings)?;
        let data = self.pack5(remaining, collected, body, env, existing_pattern_bindings)?;
        self.push_cont(CONT_WITH_SYNTAX_BIND, data, env)?;
        
        // Evaluate the first binding value
        Ok(TrampolineState::Eval { expr: first_val_expr, env })
    }
    
    // ========================================================================
    // dynamic-wind support
    // ========================================================================
    
    /// Evaluate dynamic-wind
    ///
    /// (dynamic-wind before body after)
    ///
    /// Calls the before thunk, then the body thunk, then the after thunk.
    /// Returns the value of the body thunk.
    ///
    /// When a continuation captured inside the body is invoked from outside,
    /// the after thunk is called before leaving this dynamic extent.
    /// When a continuation captured outside is invoked from inside the body,
    /// the after thunk is called before leaving, and if the continuation
    /// captured inside is invoked again, the before thunk is called to re-enter.
    pub(super) fn step_eval_dynamic_wind(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> Result<TrampolineState, EvalError> {
        // Parse arguments: (before body after)
        if self.lisp.get(args)?.is_nil() {
            return Err(self.make_error(ErrorKind::WrongArgCount, args)
                .with_message("dynamic-wind requires 3 arguments"));
        }
        let before_expr = self.lisp.car(args)?;
        let rest1 = self.lisp.cdr(args)?;
        
        if self.lisp.get(rest1)?.is_nil() {
            return Err(self.make_error(ErrorKind::WrongArgCount, args)
                .with_message("dynamic-wind requires 3 arguments"));
        }
        let body_expr = self.lisp.car(rest1)?;
        let rest2 = self.lisp.cdr(rest1)?;
        
        if self.lisp.get(rest2)?.is_nil() {
            return Err(self.make_error(ErrorKind::WrongArgCount, args)
                .with_message("dynamic-wind requires 3 arguments"));
        }
        let after_expr = self.lisp.car(rest2)?;
        let rest3 = self.lisp.cdr(rest2)?;
        
        if !self.lisp.get(rest3)?.is_nil() {
            return Err(self.make_error(ErrorKind::WrongArgCount, args)
                .with_message("dynamic-wind requires 3 arguments"));
        }
        
        // Save current dynamic-wind chain
        let saved_dw_chain = self.dynamic_wind_chain;
        
        // Push continuation for when before thunk is evaluated
        // Data: (body_expr . (after_expr . (env . saved_dw_chain)))
        let data = self.pack4(body_expr, after_expr, env, saved_dw_chain)?;
        self.push_cont(CONT_DYNAMIC_WIND_BEFORE, data, env)?;
        
        // Evaluate the before thunk
        Ok(TrampolineState::Eval { expr: before_expr, env })
    }
    
    /// Apply a thunk (zero-argument procedure)
    fn apply_thunk(
        &mut self,
        thunk: ArenaIndex,
        env: ArenaIndex,
    ) -> Result<Option<TrampolineState>, EvalError> {
        match self.lisp.get(thunk)? {
            Value::Lambda { .. } => {
                let (params, body, closure_env) = self.lisp.lambda_parts(thunk)?;
                
                // Check param count - should be exactly 0 for a thunk
                if !self.lisp.get(params)?.is_nil() {
                    return Err(self.make_error(ErrorKind::WrongArgCount, thunk)
                        .with_message("thunk must accept 0 arguments (expected a zero-argument procedure)"));
                }
                
                // Execute the lambda body
                Ok(Some(TrampolineState::Eval { expr: body, env: closure_env }))
            }
            Value::Continuation { .. } => {
                // Can't call a continuation as a thunk without an argument
                Err(self.make_error(ErrorKind::WrongArgCount, thunk)
                    .with_message("continuation requires 1 argument"))
            }
            _ => {
                // For other callable types, construct a call expression
                let nil = self.lisp.nil()?;
                let call_expr = self.lisp.cons(thunk, nil)?;
                self.push_frame(call_expr, thunk)?;
                let data = self.pack3(nil, env, call_expr)?;
                self.push_cont(CONT_APPLY_FORCED, data, env)?;
                Ok(Some(TrampolineState::Return { val: thunk }))
            }
        }
    }
    
    /// Check if two dynamic-wind chains are equal (by identity)
    fn dynamic_wind_chains_equal(
        &self,
        chain1: ArenaIndex,
        chain2: ArenaIndex,
    ) -> Result<bool, EvalError> {
        // Simple identity check for arena indices
        Ok(chain1 == chain2)
    }
    
    /// Compute the frames to wind out and wind in when transitioning between chains
    ///
    /// Returns (wind_out_frames, wind_in_frames) where:
    /// - wind_out_frames: list of frames to exit (call after thunks)
    /// - wind_in_frames: list of frames to enter (call before thunks)
    fn compute_wind_frames(
        &self,
        from_chain: ArenaIndex,
        to_chain: ArenaIndex,
    ) -> Result<(ArenaIndex, ArenaIndex), EvalError> {
        // Find common ancestor of the two chains
        // For simplicity, compute depth of each chain
        let from_depth = self.chain_depth(from_chain)?;
        let to_depth = self.chain_depth(to_chain)?;
        
        // Build list of frames to wind out (from current to common ancestor)
        let mut wind_out = self.lisp.nil()?;
        let mut from = from_chain;
        let mut from_d = from_depth;
        
        // Bring from_chain up to same depth as to_chain
        while from_d > to_depth {
            if !self.lisp.get(from)?.is_nil() {
                wind_out = self.lisp.cons(from, wind_out)?;
                from = self.lisp.cdr(from)?;
                from_d -= 1;
            } else {
                break;
            }
        }
        
        // Build list of frames to wind in (from common ancestor to target)
        let mut wind_in = self.lisp.nil()?;
        let mut to = to_chain;
        let mut to_d = to_depth;
        
        // Bring to_chain up to same depth as from_chain
        while to_d > from_d {
            if !self.lisp.get(to)?.is_nil() {
                // Prepend to wind_in list (we'll reverse later)
                wind_in = self.lisp.cons(to, wind_in)?;
                to = self.lisp.cdr(to)?;
                to_d -= 1;
            } else {
                break;
            }
        }
        
        // Now find common ancestor
        while from != to && !self.lisp.get(from)?.is_nil() && !self.lisp.get(to)?.is_nil() {
            wind_out = self.lisp.cons(from, wind_out)?;
            wind_in = self.lisp.cons(to, wind_in)?;
            from = self.lisp.cdr(from)?;
            to = self.lisp.cdr(to)?;
        }
        
        // Reverse wind_out (we want to process from innermost to outermost)
        wind_out = self.reverse_list(wind_out)?;
        
        // wind_in is already in the right order (from ancestor to target)
        // But we need to reverse it since we built it backwards
        wind_in = self.reverse_list(wind_in)?;
        
        Ok((wind_out, wind_in))
    }
    
    /// Get the depth of a dynamic-wind chain
    fn chain_depth(&self, mut chain: ArenaIndex) -> Result<usize, EvalError> {
        let mut depth = 0;
        while !self.lisp.get(chain)?.is_nil() {
            depth += 1;
            chain = self.lisp.cdr(chain)?;
        }
        Ok(depth)
    }
    
    /// Start the wind-out/wind-in transition
    fn start_wind_transition(
        &mut self,
        wind_out_frames: ArenaIndex,
        wind_in_frames: ArenaIndex,
        return_val: ArenaIndex,
        target_chain: ArenaIndex,
    ) -> Result<Option<TrampolineState>, EvalError> {
        if self.lisp.get(wind_out_frames)?.is_nil() {
            // No frames to wind out, start winding in
            self.start_wind_in(wind_in_frames, return_val, target_chain)
        } else {
            // Get the first frame to wind out
            let frame = self.lisp.car(wind_out_frames)?;
            let rest = self.lisp.cdr(wind_out_frames)?;
            
            // Get the after thunk from this frame
            // Frame format: ((before . after) . parent)
            let (before_after, _parent) = self.unpack2(frame)?;
            let (_before, after) = self.unpack2(before_after)?;
            
            // Pop this frame from the current dynamic-wind chain
            if !self.lisp.get(self.dynamic_wind_chain)?.is_nil() {
                self.dynamic_wind_chain = self.lisp.cdr(self.dynamic_wind_chain)?;
            }
            
            // Push continuation for next wind-out step
            let data = self.pack4(rest, return_val, target_chain, wind_in_frames)?;
            self.push_cont(CONT_WIND_OUT, data, self.global_env)?;
            
            // Call the after thunk
            self.apply_thunk(after, self.global_env)
        }
    }
    
    /// Start winding into the target chain
    fn start_wind_in(
        &mut self,
        wind_in_frames: ArenaIndex,
        return_val: ArenaIndex,
        target_chain: ArenaIndex,
    ) -> Result<Option<TrampolineState>, EvalError> {
        if self.lisp.get(wind_in_frames)?.is_nil() {
            // Done winding, set final chain and return
            self.dynamic_wind_chain = target_chain;
            Ok(Some(TrampolineState::Return { val: return_val }))
        } else {
            // Get the first frame to wind in
            let frame = self.lisp.car(wind_in_frames)?;
            let rest = self.lisp.cdr(wind_in_frames)?;
            
            // Get the before thunk from this frame
            // Frame format: ((before . after) . parent)
            let (before_after, _parent) = self.unpack2(frame)?;
            let (before, _after) = self.unpack2(before_after)?;
            
            // Push this frame's thunks onto the current dynamic-wind chain
            self.dynamic_wind_chain = self.lisp.cons(before_after, self.dynamic_wind_chain)?;
            
            // Push continuation for next wind-in step
            let data = self.pack4(rest, return_val, target_chain, target_chain)?;
            self.push_cont(CONT_WIND_IN, data, self.global_env)?;
            
            // Call the before thunk
            self.apply_thunk(before, self.global_env)
        }
    }
}
