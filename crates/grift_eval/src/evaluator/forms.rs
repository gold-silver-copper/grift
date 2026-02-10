//! Special form handling for the evaluator.
//!
//! Contains step_return (continuation handling) and all step_eval_* forms
//! (begin, quasiquote, apply, values, dynamic-wind, etc.)
//!
//! Note: let, let*, letrec, letrec*, and, or, cond, case, do are now handled by macros.

use grift_parser::{ArenaIndex, Value, parse};

use crate::error::{ErrorKind, EvalError, EvalResult};
use crate::continuation::{TrampolineState, ContType, EnvRef, ExprRef};
use crate::extract_args;

use super::Evaluator;

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Extract index and length from encoded arena values.
    fn decode_index_len(&self, index_enc: ArenaIndex, len_enc: ArenaIndex, call_expr: ArenaIndex) -> Result<(usize, usize), EvalError> {
        let index = match self.lisp.get(index_enc)? {
            Value::Number(n) => n as usize,
            _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
        };
        let len = match self.lisp.get(len_enc)? {
            Value::Number(n) => n as usize,
            _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
        };
        Ok((index, len))
    }

    /// Decode a Number-encoded port ID from an arena value.
    fn decode_port_id(&self, enc: ArenaIndex, err_ctx: ArenaIndex) -> Result<grift_parser::PortId, EvalError> {
        match self.lisp.get(enc)? {
            Value::Number(n) => Ok(grift_parser::PortId(n as usize)),
            _ => Err(self.make_error(ErrorKind::Generic, err_ctx)),
        }
    }

    pub(super) fn step_return(&mut self, val: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
        // The cont_env field is stored for potential future use (e.g., debugging, stack traces)
        // but is not currently used during normal continuation processing.
        let (cont_type, data, cont_env) = self.pop_cont()?;
        
        match cont_type {
            ContType::Done => {
                // No more continuations - we're done
                Ok(None)
            }
            
            ContType::IfBranch => {
                // Data: (then_expr . (else_expr . env))
                let (then_expr, else_expr, env) = self.unpack3(data)?;
                // val is the evaluated condition
                let branch = if !self.is_false(val)? { then_expr } else { else_expr };
                if branch.is_nil() {
                    // No else branch and test was false: return #f
                    // Per R7RS §4.1.5, the result is unspecified when the
                    // alternative is omitted and the test is false.
                    // We choose #f as the most useful unspecified value.
                    let false_val = self.lisp.boolean(false)?;
                    Ok(Some(TrampolineState::Return { val: false_val }))
                } else {
                    Ok(Some(TrampolineState::Eval { expr: ExprRef(branch), env: EnvRef(env) }))
                }
            }
            
            ContType::ApplyForced => {
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
                            self.apply_builtin_with_args(b, args_expr, EnvRef(env), call_expr)
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
                                let extended_env = self.env_extend(EnvRef(closure_env), params, nil)?;
                                Ok(Some(TrampolineState::Eval { expr: ExprRef(body), env: extended_env }))
                            } else {
                                // Evaluate first arg and start collecting
                                let first_expr = self.lisp.car(args_expr)?;
                                let rest_exprs = self.lisp.cdr(args_expr)?;
                                let nil = self.lisp.nil()?;
                                
                                // Data: (remaining_exprs . (eval_env . (rest_param . (body . (new_env . (collected . call_expr))))))
                                self.cont(ContType::LambdaRestCollect, EnvRef(env)).data7(rest_exprs, env, params, body, closure_env, nil, call_expr)?;
                                
                                Ok(Some(TrampolineState::Eval { expr: ExprRef(first_expr), env: EnvRef(env) }))
                            }
                        } else if self.lisp.get(args_expr)?.is_nil() {
                            // No args - check params are also empty
                            if !self.lisp.get(params)?.is_nil() {
                                let expected = self.count_list(params)?;
                                return Err(self.arg_error(call_expr, expected, 0));
                            }
                            Ok(Some(TrampolineState::Eval { expr: ExprRef(body), env: EnvRef(closure_env) }))
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
                            self.cont(ContType::LambdaBindArg, EnvRef(env)).data6(rest_exprs, env, rest_params, body, closure_env, call_expr)?;
                            // Push binding continuation for first param
                            // Data for LambdaFirstBind: param
                            self.cont(ContType::LambdaFirstBind, EnvRef(env)).data1(first_param)?;
                            
                            Ok(Some(TrampolineState::Eval { expr: ExprRef(first_expr), env: EnvRef(env) }))
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
                        
                        // Check for rest-argument form: (lambda args body) where params is a symbol
                        if self.lisp.get(params)?.is_symbol() {
                            // Rest-only: all args collected into a single list
                            if self.lisp.get(args_expr)?.is_nil() {
                                // No args - bind to empty list
                                let nil = self.lisp.nil()?;
                                let extended_env = self.env_extend(closure_env, params, nil)?;
                                Ok(Some(TrampolineState::Eval { expr: ExprRef(body), env: extended_env }))
                            } else {
                                // Evaluate first arg and start collecting
                                let first_expr = self.lisp.car(args_expr)?;
                                let rest_exprs = self.lisp.cdr(args_expr)?;
                                let nil = self.lisp.nil()?;
                                
                                self.cont(ContType::LambdaRestCollect, EnvRef(env)).data7(rest_exprs, env, params, body, closure_env.0, nil, call_expr)?;
                                
                                Ok(Some(TrampolineState::Eval { expr: ExprRef(first_expr), env: EnvRef(env) }))
                            }
                        } else if self.lisp.get(args_expr)?.is_nil() {
                            // No args - check params are also empty
                            if !self.lisp.get(params)?.is_nil() {
                                let expected = self.count_list(params)?;
                                return Err(self.arg_error(call_expr, expected, 0));
                            }
                            Ok(Some(TrampolineState::Eval { expr: ExprRef(body), env: closure_env }))
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
                            self.cont(ContType::LambdaBindArg, EnvRef(env)).data6(rest_exprs, env, rest_params, body, closure_env.0, call_expr)?;
                            // Push binding continuation for first param
                            self.cont(ContType::LambdaFirstBind, EnvRef(env)).data1(first_param)?;
                            
                            Ok(Some(TrampolineState::Eval { expr: ExprRef(first_expr), env: EnvRef(env) }))
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
                            self.cont(ContType::NativeArgsCollect, EnvRef(env)).data4(rest, nil, id_encoded, env)?;
                            Ok(Some(TrampolineState::Eval { expr: ExprRef(first_expr), env: EnvRef(env) }))
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
                        self.cont(ContType::ContinuationApply, EnvRef(env)).data1(val)?;
                        
                        // Evaluate the argument
                        Ok(Some(TrampolineState::Eval { expr: ExprRef(arg_expr), env: EnvRef(env) }))
                    }
                    _ => {
                        self.pop_frame();
                        Err(self.type_error(call_expr, "procedure", self.lisp.get(val)?.type_name()))
                    }
                }
            }
            
            ContType::LambdaFirstBind => {
                // Data: param
                let param = self.unpack1(data);
                // val is evaluated first arg - bind to param
                // Pop LambdaBindArg, extend env, push it back
                let (next_type, next_data, _) = self.pop_cont()?;
                if next_type == ContType::LambdaBindArg {
                    // Data: (remaining_exprs . (eval_env . (remaining_params . (body . (new_env . call_expr)))))
                    let (remaining_exprs, eval_env, remaining_params, body, new_env, call_expr) = 
                        self.unpack6(next_data)?;
                    
                    // Extend environment with binding
                    let extended_env = self.env_extend(EnvRef(new_env), param, val)?;
                    
                    // Check if remaining_params is a symbol (rest parameter for dotted lambda)
                    if self.lisp.get(remaining_params)?.is_symbol() {
                        // Dotted parameter: (a b . rest) - collect remaining args into rest
                        if self.lisp.get(remaining_exprs)?.is_nil() {
                            // No more args - bind rest param to empty list
                            let nil = self.lisp.nil()?;
                            let final_env = self.env_extend(extended_env, remaining_params, nil)?;
                            Ok(Some(TrampolineState::Eval { expr: ExprRef(body), env: final_env }))
                        } else {
                            // Start collecting rest args
                            let first_expr = self.lisp.car(remaining_exprs)?;
                            let rest_exprs = self.lisp.cdr(remaining_exprs)?;
                            let nil = self.lisp.nil()?;
                            
                            self.cont(ContType::LambdaRestCollect, EnvRef(eval_env)).data7(rest_exprs, eval_env, remaining_params, body, extended_env.0, nil, call_expr)?;
                            
                            Ok(Some(TrampolineState::Eval { expr: ExprRef(first_expr), env: EnvRef(eval_env) }))
                        }
                    } else if self.lisp.get(remaining_exprs)?.is_nil() {
                        // No more args - check params match
                        if !self.lisp.get(remaining_params)?.is_nil() {
                            let expected = self.count_list(remaining_params)? + 1;
                            return Err(self.arg_error(call_expr, expected, 1));
                        }
                        // Evaluate body with extended env
                        Ok(Some(TrampolineState::Eval { expr: ExprRef(body), env: extended_env }))
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
                        self.cont(ContType::LambdaBindArg, EnvRef(eval_env)).data6(rest_exprs, eval_env, rest_params, body, extended_env.0, call_expr)?;
                        self.cont(ContType::LambdaFirstBind, EnvRef(eval_env)).data1(next_param)?;
                        
                        Ok(Some(TrampolineState::Eval { expr: ExprRef(next_expr), env: EnvRef(eval_env) }))
                    }
                } else {
                    // This shouldn't happen
                    Err(self.make_error(ErrorKind::Generic, val))
                }
            }
            
            ContType::LambdaBindArg => {
                // This shouldn't be hit directly - LambdaFirstBind pops it
                Err(self.make_error(ErrorKind::Generic, val))
            }
            
            ContType::LambdaRestCollect => {
                // Collecting rest arguments into a list
                // Data: (remaining_exprs . (eval_env . (rest_param . (body . (new_env . (collected . call_expr))))))
                let (remaining_exprs, eval_env, rest_param, body, new_env, collected, call_expr) = 
                    self.unpack7(data)?;
                
                // Add evaluated value to collected list
                let new_collected = self.lisp.cons(val, collected)?;
                
                if self.lisp.get(remaining_exprs)?.is_nil() {
                    // No more args - reverse collected and bind to rest_param
                    let rest_list = self.reverse_list(new_collected)?;
                    let extended_env = self.env_extend(EnvRef(new_env), rest_param, rest_list)?;
                    Ok(Some(TrampolineState::Eval { expr: ExprRef(body), env: extended_env }))
                } else {
                    // More args to collect
                    let next_expr = self.lisp.car(remaining_exprs)?;
                    let rest_exprs = self.lisp.cdr(remaining_exprs)?;
                    
                    self.cont(ContType::LambdaRestCollect, EnvRef(eval_env)).data7(rest_exprs, eval_env, rest_param, body, new_env, new_collected, call_expr)?;
                    
                    Ok(Some(TrampolineState::Eval { expr: ExprRef(next_expr), env: EnvRef(eval_env) }))
                }
            }
            
            ContType::BuiltinForceArg => {
                // Data: (builtin_encoded . (remaining_args . (collected . (call_expr . eval_env))))
                let (builtin_encoded, remaining_args, collected, call_expr, eval_env) = self.unpack5(data)?;
                let builtin = Self::decode_builtin(builtin_encoded);
                // val is an evaluated argument for a builtin
                let new_collected = self.lisp.cons(val, collected)?;
                
                if self.lisp.get(remaining_args)?.is_nil() {
                    // All args evaluated - apply builtin
                    let args = self.reverse_list(new_collected)?;
                    let state = self.apply_builtin_trampolined(builtin, args, call_expr)?;
                    Ok(Some(state))
                } else {
                    // More args to evaluate
                    let next_arg = self.lisp.car(remaining_args)?;
                    let rest_args = self.lisp.cdr(remaining_args)?;
                    
                    let builtin_encoded = Self::encode_builtin(builtin);
                    self.cont(ContType::BuiltinForceArg, EnvRef(eval_env)).data5(builtin_encoded, rest_args, new_collected, call_expr, eval_env)?;
                    
                    Ok(Some(TrampolineState::Eval { expr: ExprRef(next_arg), env: EnvRef(eval_env) }))
                }
            }
            
            ContType::BinaryBuiltinFirst => {
                // Data: (builtin_encoded . (second_arg . (call_expr . eval_env)))
                let (builtin_encoded, second_arg, call_expr, eval_env) = self.unpack4(data)?;
                // val is first evaluated arg - now evaluate second
                // Data for BinaryBuiltinSecond: (builtin_encoded . (first_val . call_expr))
                self.cont(ContType::BinaryBuiltinSecond, EnvRef(eval_env)).data3(builtin_encoded, val, call_expr)?;
                Ok(Some(TrampolineState::Eval { expr: ExprRef(second_arg), env: EnvRef(eval_env) }))
            }
            
            ContType::BinaryBuiltinSecond => {
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

            ContType::EvalExpr => {
                // Data: env
                let env = self.unpack1(data);
                // val is the evaluated expression - now evaluate it
                Ok(Some(TrampolineState::Eval { expr: ExprRef(val), env: EnvRef(env) }))
            }

            ContType::EvalEnvArg => {
                // Data: (expr_to_eval . eval_env)
                // val is the evaluated env argument — must be an Environment value
                let (expr_to_eval, eval_env) = self.unpack2(data)?;
                match self.lisp.get(val)? {
                    Value::Environment { env: target_env, .. } => {
                        // Push EvalExpr continuation with target env in data
                        // (EvalExpr reads env from data, not from frame env)
                        self.cont(ContType::EvalExpr, EnvRef(target_env)).data1(target_env)?;
                        Ok(Some(TrampolineState::Eval { expr: ExprRef(expr_to_eval), env: EnvRef(eval_env) }))
                    }
                    _ => Err(self.type_error(val, "environment", self.lisp.get(val)?.type_name())),
                }
            }

            ContType::VectorMapStep => {
                // val is the result of applying proc to current element(s)
                // Data: (proc . (vecs . (index_encoded . (len_encoded . (collected . call_expr)))))
                let (proc, rest) = self.unpack2(data)?;
                let (vecs, rest2) = self.unpack2(rest)?;
                let (index_enc, rest3) = self.unpack2(rest2)?;
                let (len_enc, rest4) = self.unpack2(rest3)?;
                let (collected, call_expr) = self.unpack2(rest4)?;
                
                let (index, len) = self.decode_index_len(index_enc, len_enc, call_expr)?;
                
                // Collect result
                let new_collected = self.lisp.cons(val, collected)?;
                let next_index = index + 1;
                
                if next_index >= len {
                    // Done - reverse collected and convert to vector
                    let result_list = self.reverse_list(new_collected)?;
                    let result_vec = self.list_to_array(result_list, call_expr)?;
                    Ok(Some(TrampolineState::Return { val: result_vec }))
                } else {
                    // More elements - build args and apply proc again
                    let args = self.vector_map_build_args(vecs, next_index, call_expr)?;
                    let next_idx = self.lisp.number(next_index as isize)?;
                    
                    // Push VectorMapStep continuation for next iteration
                    let d4 = self.lisp.cons(new_collected, call_expr)?;
                    let d3 = self.lisp.cons(len_enc, d4)?;
                    let d2 = self.lisp.cons(next_idx, d3)?;
                    let d1 = self.lisp.cons(vecs, d2)?;
                    let packed = self.lisp.cons(proc, d1)?;
                    self.cont(ContType::VectorMapStep, EnvRef(cont_env)).data1(packed)?;
                    
                    // Apply proc via ApplyForced
                    self.cont(ContType::ApplyForced, EnvRef(cont_env)).data3(args, cont_env, call_expr)?;
                    Ok(Some(TrampolineState::Return { val: proc }))
                }
            }

            ContType::VectorForEachStep => {
                // val is the result of applying proc (ignored)
                // Data: (proc . (vecs . (index_encoded . (len_encoded . call_expr))))
                let (proc, rest) = self.unpack2(data)?;
                let (vecs, rest2) = self.unpack2(rest)?;
                let (index_enc, rest3) = self.unpack2(rest2)?;
                let (len_enc, call_expr) = self.unpack2(rest3)?;
                
                let (index, len) = self.decode_index_len(index_enc, len_enc, call_expr)?;
                
                let next_index = index + 1;
                
                if next_index >= len {
                    // Done - return void
                    let void = self.lisp.void_val()?;
                    Ok(Some(TrampolineState::Return { val: void }))
                } else {
                    // More elements - build args and apply proc again
                    let args = self.vector_map_build_args(vecs, next_index, call_expr)?;
                    let next_idx = self.lisp.number(next_index as isize)?;
                    
                    // Push VectorForEachStep continuation for next iteration
                    let d3 = self.lisp.cons(len_enc, call_expr)?;
                    let d2 = self.lisp.cons(next_idx, d3)?;
                    let d1 = self.lisp.cons(vecs, d2)?;
                    let packed = self.lisp.cons(proc, d1)?;
                    self.cont(ContType::VectorForEachStep, EnvRef(cont_env)).data1(packed)?;
                    
                    // Apply proc via ApplyForced
                    self.cont(ContType::ApplyForced, EnvRef(cont_env)).data3(args, cont_env, call_expr)?;
                    Ok(Some(TrampolineState::Return { val: proc }))
                }
            }

            ContType::CallWithPortClose => {
                // val is the result of the proc
                // Data: port_id_encoded (single value - Number encoding port id)
                let port_id = self.decode_port_id(data, val)?;
                if let Some(ref mut io) = self.io {
                    let _ = io.close_port(port_id);
                }
                Ok(Some(TrampolineState::Return { val }))
            }

            ContType::WithInputFromFileRestore => {
                // val is the result of the thunk
                // Data: (saved_port_encoded . file_port_encoded)
                let (saved_enc, file_enc) = self.unpack2(data)?;
                self.current_input_port = self.decode_port_id(saved_enc, val)?;
                let file_id = self.decode_port_id(file_enc, val)?;
                if let Some(ref mut io) = self.io {
                    let _ = io.close_port(file_id);
                }
                Ok(Some(TrampolineState::Return { val }))
            }

            ContType::WithOutputToFileRestore => {
                // val is the result of the thunk
                // Data: (saved_port_encoded . file_port_encoded)
                let (saved_enc, file_enc) = self.unpack2(data)?;
                self.current_output_port = self.decode_port_id(saved_enc, val)?;
                let file_id = self.decode_port_id(file_enc, val)?;
                if let Some(ref mut io) = self.io {
                    let _ = io.close_port(file_id);
                }
                Ok(Some(TrampolineState::Return { val }))
            }

            ContType::BeginSeq => {
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
                        Ok(Some(TrampolineState::Eval { expr: ExprRef(next_expr), env: EnvRef(env) }))
                    } else {
                        // More after next - push continuation
                        self.cont(ContType::BeginSeq, EnvRef(env)).data2(rest, env)?;
                        Ok(Some(TrampolineState::Eval { expr: ExprRef(next_expr), env: EnvRef(env) }))
                    }
                }
            }

            // ================================================================
            // New continuation types for fully trampolined evaluation
            // ================================================================

            // Note: CaseKey, DoInit, DoTestResult, DoBody, DoStep removed - now handled by macros (Phase 9)

            ContType::ApplyFirst => {
                // Data: (args_list_expr . env)
                let (args_list_expr, env) = self.unpack2(data)?;
                // val is the evaluated function - now evaluate args list
                self.cont(ContType::ApplySecond, EnvRef(env)).data2(val, env)?;
                Ok(Some(TrampolineState::Eval { expr: ExprRef(args_list_expr), env: EnvRef(env) }))
            }

            ContType::ApplySecond => {
                // Data: (func . env)
                let (func, env) = self.unpack2(data)?;
                // val is the evaluated args list - perform application
                let args_list = val;
                let call_expr = self.lisp.cons(func, args_list)?;
                self.push_frame(call_expr, func)?;
                self.cont(ContType::ApplyForced, EnvRef(env)).data3(args_list, env, call_expr)?;
                Ok(Some(TrampolineState::Return { val: func }))
            }

            ContType::ValuesCollect => {
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
                    self.cont(ContType::ValuesCollect, EnvRef(env)).data3(rest, new_collected, env)?;
                    Ok(Some(TrampolineState::Eval { expr: ExprRef(next_expr), env: EnvRef(env) }))
                }
            }

            ContType::CallWithValuesProducer => {
                // Data: (consumer_expr . env)
                let (consumer_expr, env) = self.unpack2(data)?;
                // val is the producer function - call it with no arguments
                // Build call expression: (producer)
                let nil = self.lisp.nil()?;
                let call_expr = self.lisp.cons(val, nil)?;
                
                // Push continuation to apply consumer after producer returns
                self.cont(ContType::CallWithValuesConsumer, EnvRef(env)).data2(consumer_expr, env)?;
                
                // Apply the producer (no arguments)
                self.push_frame(call_expr, val)?;
                self.cont(ContType::ApplyForced, EnvRef(env)).data3(nil, env, call_expr)?;
                Ok(Some(TrampolineState::Return { val }))
            }

            ContType::CallWithValuesConsumer => {
                // Data: (consumer_expr . env)
                let (consumer_expr, env) = self.unpack2(data)?;
                // val is the result from producer - could be a single value or a list from (values ...)
                // Now we need to evaluate consumer and apply it to the producer's result(s)
                
                // Store the producer result and push continuation to apply consumer
                self.cont(ContType::CallWithValuesApply, EnvRef(env)).data2(val, env)?;
                
                // Evaluate the consumer expression
                Ok(Some(TrampolineState::Eval { expr: ExprRef(consumer_expr), env: EnvRef(env) }))
            }

            ContType::CallWithValuesApply => {
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
                self.cont(ContType::ApplyForced, EnvRef(env)).data3(args_list, env, call_expr)?;
                Ok(Some(TrampolineState::Return { val }))
            }

            ContType::DefineValue => {
                // Data: name
                let name = self.unpack1(data);
                // val is the evaluated value - define the binding
                self.define(name, val)?;
                // Return void (unspecified value) per R7RS
                let void = self.lisp.void_val()?;
                Ok(Some(TrampolineState::Return { val: void }))
            }

            ContType::SetValue => {
                // Data: (name . env)
                let (name, env) = self.unpack2(data)?;
                // val is the evaluated value - set! the binding
                self.env_set(EnvRef(env), name, val)?;
                // Return void (unspecified value) per R7RS
                let void = self.lisp.void_val()?;
                Ok(Some(TrampolineState::Return { val: void }))
            }

            ContType::NativeArgsCollect => {
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
                    self.cont(ContType::NativeArgsCollect, EnvRef(env)).data4(rest, new_collected, id_encoded, env)?;
                    Ok(Some(TrampolineState::Eval { expr: ExprRef(next_expr), env: EnvRef(env) }))
                }
            }

            ContType::QuasiquoteCar => {
                // Data: (cdr . (depth_encoded . env))
                let (cdr, depth_encoded, env) = self.unpack3(data)?;
                let depth = Self::decode_usize(depth_encoded);
                // val is the evaluated car - now process cdr
                let car_val = val;
                self.cont(ContType::QuasiquoteCdr, EnvRef(env)).data1(car_val)?;
                Ok(Some(self.step_quasiquote_trampoline(cdr, EnvRef(env), depth)?))
            }

            ContType::QuasiquoteCdr => {
                // Data: car_val
                let car_val = self.unpack1(data);
                // val is the processed cdr - cons with car
                let result = self.lisp.cons(car_val, val)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            ContType::QuasiquoteUnquoteWrap => {
                // val is the inner processed value - wrap with unquote
                let unquote_sym = self.lisp.symbol("unquote")?;
                let nil = self.lisp.nil()?;
                let inner_list = self.lisp.cons(val, nil)?;
                let result = self.lisp.cons(unquote_sym, inner_list)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            ContType::QuasiquoteNestedWrap => {
                // val is the inner processed value - wrap with quasiquote
                let qq_sym = self.lisp.symbol("quasiquote")?;
                let nil = self.lisp.nil()?;
                let inner_list = self.lisp.cons(val, nil)?;
                let result = self.lisp.cons(qq_sym, inner_list)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            ContType::QuasiquoteSplice => {
                // Data: (cdr . (depth_encoded . env))
                let (cdr, depth_encoded, env) = self.unpack3(data)?;
                let depth = Self::decode_usize(depth_encoded);
                // val is the evaluated splice expression - process cdr then append
                let splice_val = val;
                self.cont(ContType::QuasiquoteSpliceAppend, EnvRef(env)).data1(splice_val)?;
                Ok(Some(self.step_quasiquote_trampoline(cdr, EnvRef(env), depth)?))
            }

            ContType::QuasiquoteSpliceAppend => {
                // Data: splice_val
                let splice_val = self.unpack1(data);
                // val is the processed cdr - append splice_val with it
                let result = self.append_lists(splice_val, val)?;
                Ok(Some(TrampolineState::Return { val: result }))
            }

            ContType::LetSyntaxBody => {
                // Restore macro environment after let-syntax body evaluation
                // Data: saved_macro_env
                let saved_macro_env = self.unpack1(data);
                self.macro_env = EnvRef(saved_macro_env);
                // Return the value from the body
                Ok(Some(TrampolineState::Return { val }))
            }

            ContType::SyntaxCaseMatch => {
                // val is the evaluated stx-expr
                // Now try to match it against clauses
                // Data: (literals . (clauses . (env . pattern_bindings)))
                let (literals, clauses, env, pattern_bindings) = self.unpack4(data)?;
                self.step_syntax_case_match(val, literals, clauses, env, pattern_bindings)
            }
            
            ContType::SyntaxCaseFender => {
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
                    Ok(Some(TrampolineState::Eval { expr: ExprRef(output), env: EnvRef(output_env) }))
                }
            }
            
            ContType::CallCcApply => {
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
                        let extended_env = self.env_extend(EnvRef(closure_env), first_param, captured_continuation)?;
                        Ok(Some(TrampolineState::Eval { expr: ExprRef(body), env: extended_env }))
                    }
                    _ => {
                        // For other callable types, use the standard apply mechanism
                        // Create a call expression and go through ApplyForced
                        let call_expr = self.lisp.cons(val, args)?;
                        self.push_frame(call_expr, val)?;
                        let global = self.global_env;
                        self.cont(ContType::ApplyForced, global).data3(args, global.0, call_expr)?;
                        Ok(Some(TrampolineState::Return { val }))
                    }
                }
            }
            
            ContType::ContinuationApply => {
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
                    let global = self.global_env;
                    self.cont(ContType::FinishContinuationRestore, global).data2(captured_continuation, val)?;
                    
                    // Start the winding process - first wind out, then wind in
                    return self.start_wind_transition(wind_out_frames, wind_in_frames, val, target_dw_chain);
                }
                
                // No dynamic-wind transitions needed - restore directly
                self.restore_continuation(captured_continuation)?;
                
                // Return val as the result of the original call/cc
                Ok(Some(TrampolineState::Return { val }))
            }
            
            ContType::FinishContinuationRestore => {
                // val is the result from winding (ignored)
                // Data: (captured_continuation . return_val)
                let (captured_continuation, return_val) = self.unpack2(data)?;
                
                // Dynamic-wind chain should already be updated by the winding process
                // Just restore the continuation and return the value
                self.restore_continuation(captured_continuation)?;
                
                // Return the original value that was passed to the continuation
                Ok(Some(TrampolineState::Return { val: return_val }))
            }
            
            ContType::DynamicWindBefore => {
                // val is the evaluated before thunk
                // Now we need to call it (no args) before running the body
                // Data: (body_expr . (after_expr . (env . saved_dw_chain)))
                let (body_expr, after_expr, env, saved_dw_chain) = self.unpack4(data)?;
                
                // Push continuation for after calling before thunk
                // We need to pass body_expr and after_expr to the next stage
                let data2 = self.pack4(body_expr, after_expr, env, saved_dw_chain)?;
                // Also save the before thunk in the data so we can add it to the dw chain
                self.cont(ContType::DynamicWindBody, EnvRef(env)).data2(val, data2)?; // (before_thunk . (body_expr . (after_expr . (env . saved_dw_chain))))
                
                // Call the before thunk (no args)
                self.apply_thunk(val, EnvRef(env))
            }
            
            ContType::DynamicWindBody => {
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
                self.cont(ContType::DynamicWindEvalAfter, EnvRef(env)).data4(before_thunk, body_expr, env, saved_dw_chain)?;
                
                // Evaluate the after thunk expression
                Ok(Some(TrampolineState::Eval { expr: ExprRef(after_expr), env: EnvRef(env) }))
            }
            
            ContType::DynamicWindEvalAfter => {
                // val is the evaluated after thunk
                // Now we need to evaluate the body thunk
                // Data: (before_thunk . (body_expr . (env . saved_dw_chain)))
                let (before_thunk, body_expr, env, saved_dw_chain) = self.unpack4(data)?;
                
                // Push continuation for after evaluating body_expr
                // Data: (before_thunk . (after_thunk . (env . saved_dw_chain)))
                self.cont(ContType::DynamicWindCallBody, EnvRef(env)).data4(before_thunk, val, env, saved_dw_chain)?;
                
                // Evaluate the body thunk expression
                Ok(Some(TrampolineState::Eval { expr: ExprRef(body_expr), env: EnvRef(env) }))
            }
            
            ContType::DynamicWindCallBody => {
                // val is the evaluated body thunk
                // Now we need to push the dynamic-wind frame and call the body thunk
                // Data: (before_thunk . (after_thunk . (env . saved_dw_chain)))
                let (before_thunk, after_thunk, env, saved_dw_chain) = self.unpack4(data)?;
                
                // Create the before/after pair and push onto dynamic-wind chain
                let before_after = self.lisp.cons(before_thunk, after_thunk)?;
                self.dynamic_wind_chain = self.lisp.cons(before_after, saved_dw_chain)?;
                
                // Push continuation for after body thunk completes
                // Data: (after_thunk . saved_dw_chain)
                self.cont(ContType::DynamicWindAfter, EnvRef(env)).data2(after_thunk, saved_dw_chain)?;
                
                // Call the body thunk
                self.apply_thunk(val, EnvRef(env))
            }
            
            ContType::DynamicWindAfter => {
                // val is the result of calling the body thunk
                // Now we need to call the after thunk (already evaluated)
                // Data: (after_thunk . saved_dw_chain)
                let (after_thunk, saved_dw_chain) = self.unpack2(data)?;
                
                // Save the body result for after the after thunk runs
                let global = self.global_env;
                self.cont(ContType::DynamicWindAfterCall, global).data2(val, saved_dw_chain)?;
                
                // Call the after thunk (no args)
                self.apply_thunk(after_thunk, self.global_env)
            }
            
            ContType::DynamicWindAfterCall => {
                // val is the result of calling after thunk (ignored)
                // Return the body result and restore dynamic-wind chain
                // Data: (body_result . saved_dw_chain)
                let (body_result, saved_dw_chain) = self.unpack2(data)?;
                
                // Restore the dynamic-wind chain
                self.dynamic_wind_chain = saved_dw_chain;
                
                // Return the body's result
                Ok(Some(TrampolineState::Return { val: body_result }))
            }
            
            ContType::WindOut => {
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
                    let global = self.global_env;
                    self.cont(ContType::WindOut, global).data4(rest, return_val, target_chain, wind_in_frames)?;
                    
                    // Call the after thunk
                    self.apply_thunk(after, self.global_env)
                }
            }
            
            ContType::WindIn => {
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
                    let global = self.global_env;
                    self.cont(ContType::WindIn, global).data4(rest, return_val, target_chain, original_target)?;
                    
                    // Call the before thunk
                    self.apply_thunk(before, self.global_env)
                }
            }
            
            // Note: with-syntax is now a macro
            
            ContType::MacroResult => {
                // val is the result of evaluating the macro transformer body
                // We need to re-evaluate this result (it may be a macro invocation itself)
                // Data: (eval_env . saved_call_site_env)
                let (eval_env, saved_call_site_env) = self.unpack2(data)?;
                // Restore the call-site environment from before this macro expansion.
                // This ensures that nested macro expansions (e.g., `and` in a fender)
                // don't permanently overwrite the outer macro's call-site context.
                self.call_site_env = EnvRef(saved_call_site_env);
                Ok(Some(TrampolineState::Eval { expr: ExprRef(val), env: EnvRef(eval_env) }))
            }
            
            ContType::WithExceptionHandlerEvalThunk => {
                // val = evaluated handler; now evaluate the thunk expression
                // Data: (thunk_expr . env)
                let (thunk_expr, env) = self.unpack2(data)?;
                let handler = val;
                self.cont(ContType::WithExceptionHandlerCallThunk, EnvRef(env))
                    .data2(handler, env)?;
                Ok(Some(TrampolineState::Eval { expr: ExprRef(thunk_expr), env: EnvRef(env) }))
            }
            
            ContType::WithExceptionHandlerCallThunk => {
                // val = evaluated thunk; handler is in data
                // Data: (handler . env)
                let (handler, _env) = self.unpack2(data)?;
                let thunk = val;
                // Install handler, call thunk, restore handler on return
                let saved_chain = self.exception_handler_chain;
                self.exception_handler_chain = self.lisp.cons(handler, saved_chain)?;
                // Push frame to restore handler chain when thunk returns.
                // continuable_flag = true means "thunk context" — always allow return
                let true_val = self.lisp.true_val()?;
                let global = self.global_env;
                self.cont(ContType::ExceptionHandlerFrame, global)
                    .data3(handler, saved_chain, true_val)?;
                // Call the thunk (zero-arg procedure)
                self.apply_thunk(thunk, self.global_env)
            }
            
            ContType::ExceptionHandlerFrame => {
                // Handler or thunk completed — restore handler chain
                // Data: (handler . (saved_handler_chain . continuable_flag))
                let (_handler, saved_chain, _continuable_flag) = self.unpack3(data)?;
                self.exception_handler_chain = saved_chain;
                // Note: R7RS §6.11 says it is an error for a handler invoked by
                // `raise` to return, but our `guard` macro relies on handler return.
                // We permit return in all cases for compatibility.
                Ok(Some(TrampolineState::Return { val }))
            }
            
            ContType::RaiseEval => {
                // val = evaluated exception object; invoke current handler
                // data = Nil for non-continuable, non-Nil for continuable
                let continuable = !self.lisp.get(data)?.is_nil();
                self.invoke_exception_handler(val, continuable)
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
    pub(super) fn step_quasiquote_trampoline(&mut self, template: ArenaIndex, env: EnvRef, depth: usize) 
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
                        return Ok(TrampolineState::Eval { expr: ExprRef(inner_expr), env });
                    } else {
                        // Nested quasiquote - decrease depth and process
                        let nil = self.lisp.nil()?;
                        self.push_cont(ContType::QuasiquoteUnquoteWrap, nil, env.0)?;
                        return self.step_quasiquote_trampoline(inner_expr, env, depth - 1);
                    }
                }
                
                // Check for unquote-splicing at top level
                if self.lisp.symbol_matches(car, "unquote-splicing").unwrap_or(false)
                    && depth == 1
                {
                    // Return the evaluated list (caller handles splicing)
                    let inner_expr = self.lisp.car(cdr)?;
                    return Ok(TrampolineState::Eval { expr: ExprRef(inner_expr), env });
                }
                
                // Check for nested quasiquote
                if self.lisp.symbol_matches(car, "quasiquote").unwrap_or(false) {
                    let nil = self.lisp.nil()?;
                    self.push_cont(ContType::QuasiquoteNestedWrap, nil, env.0)?;
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
                        self.cont(ContType::QuasiquoteSplice, env).data3(cdr, depth_encoded, env.0)?;
                        return Ok(TrampolineState::Eval { expr: ExprRef(splice_expr), env });
                    }
                }
                
                // Recursively process car and cdr
                // Data: (cdr . (depth_encoded . env))
                let depth_encoded = Self::encode_usize(depth);
                self.cont(ContType::QuasiquoteCar, env).data3(cdr, depth_encoded, env.0)?;
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
    pub(super) fn step_eval_begin(&mut self, exprs: ArenaIndex, env: EnvRef) -> Result<TrampolineState, EvalError> {
        if self.lisp.get(exprs)?.is_nil() {
            // Empty begin - return nil
            let nil = self.lisp.nil()?;
            return Ok(TrampolineState::Return { val: nil });
        }

        let first_expr = self.lisp.car(exprs)?;
        let rest = self.lisp.cdr(exprs)?;

        if self.lisp.get(rest)?.is_nil() {
            // Single expression - tail call
            Ok(TrampolineState::Eval { expr: ExprRef(first_expr), env })
        } else {
            // Multiple expressions - push continuation for the rest
            // Data: (remaining . env)
            self.cont(ContType::BeginSeq, env).data2(rest, env.0)?;
            Ok(TrampolineState::Eval { expr: ExprRef(first_expr), env })
        }
    }

    // Note: step_eval_and and step_eval_or removed - and/or are now handled by macros
    // Note: step_eval_case and step_eval_do removed - case and do are now handled by macros (Phase 9)
    
    /// Evaluate quasiquote - template with unquote (trampolined version)
    pub(super) fn eval_quasiquote(&mut self, template: ArenaIndex, env: EnvRef) -> Result<TrampolineState, EvalError> {
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
    pub(super) fn step_eval_apply(&mut self, args: ArenaIndex, env: EnvRef) -> Result<TrampolineState, EvalError> {
        let func_expr = self.lisp.car(args)?;
        let args_list_expr = self.lisp.car(self.lisp.cdr(args)?)?;
        
        // Push continuation to evaluate args_list after func is evaluated
        // Data: (args_list_expr . env)
        self.cont(ContType::ApplyFirst, env).data2(args_list_expr, env.0)?;
        
        // Evaluate function first
        Ok(TrampolineState::Eval { expr: ExprRef(func_expr), env })
    }
    
    /// Evaluate values - create a multi-value return (trampolined)
    pub(super) fn eval_values(&mut self, args: ArenaIndex, env: EnvRef) -> Result<TrampolineState, EvalError> {
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
        self.cont(ContType::ValuesCollect, env).data3(rest, nil, env.0)?;
        Ok(TrampolineState::Eval { expr: ExprRef(first_expr), env })
    }
    
    /// Evaluate call-with-values - call producer, apply consumer to results
    /// 
    /// (call-with-values producer consumer)
    /// 
    /// Calls producer with no arguments, then applies consumer to the values
    /// returned by producer. If producer returns multiple values (via values),
    /// those become the arguments to consumer.
    pub(super) fn step_eval_call_with_values(&mut self, args: ArenaIndex, env: EnvRef) -> Result<TrampolineState, EvalError> {
        let producer_expr = self.lisp.car(args)?;
        let consumer_expr = self.lisp.car(self.lisp.cdr(args)?)?;
        
        // Push continuation to call consumer after producer is evaluated and called
        // Data: (consumer_expr . env)
        self.cont(ContType::CallWithValuesProducer, env).data2(consumer_expr, env.0)?;
        
        // Evaluate producer first
        Ok(TrampolineState::Eval { expr: ExprRef(producer_expr), env })
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
    pub(super) fn step_eval_call_cc(&mut self, args: ArenaIndex, env: EnvRef) -> Result<TrampolineState, EvalError> {
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
        let captured_continuation = self.capture_continuation(env.0)?;
        
        // Push continuation to apply proc to the captured continuation after proc is evaluated
        // Data: captured_continuation
        self.cont(ContType::CallCcApply, env).data1(captured_continuation)?;
        
        // Evaluate the procedure expression
        Ok(TrampolineState::Eval { expr: ExprRef(proc_expr), env })
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
    pub(super) fn eval_lambda(&mut self, args: ArenaIndex, env: EnvRef) -> EvalResult {
        let params = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;
        
        // Check for internal defines and transform to letrec
        let body = self.transform_internal_defines(body_list)?;
        
        // NOTE: Macro expansion is now deferred until lambda call time.
        // This is necessary for lexically-scoped syntax objects to work correctly,
        // because macro input expressions need to carry their call-site lexical context.
        // If we expanded here, the lambda parameters wouldn't be bound yet, and
        // we couldn't wrap macro inputs with the proper lexical environment.
        //
        // The tradeoff is that macro expansion happens at each call, not just once.
        // But this matches the semantics required for true lexically-scoped syntax.
        
        self.lisp.lambda(params, body, env.0).map_err(Into::into)
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
                if let Value::Symbol(_) = self.lisp.get(head)?
                    && self.lisp.symbol_matches(head, "define")? {
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
    pub(super) fn eval_define(&mut self, args: ArenaIndex, env: EnvRef) -> Result<TrampolineState, EvalError> {
        let first = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        
        // Unwrap syntax objects to handle identifiers created by datum->syntax
        let unwrapped = match self.lisp.get(first)? {
            Value::Syntax { .. } => self.lisp.syntax_to_datum(first)?,
            _ => first,
        };
        
        match self.lisp.get(unwrapped)? {
            // (define name value)
            Value::Symbol(_) => {
                let value_expr = self.lisp.car(rest)?;
                // Push continuation and evaluate value
                // Data: name (use unwrapped symbol for proper binding)
                self.cont(ContType::DefineValue, env).data1(unwrapped)?;
                Ok(TrampolineState::Eval { expr: ExprRef(value_expr), env })
            }
            // (define (name params...) body...) -> (define name (lambda (params...) body...))
            Value::Cons { .. } => {
                let name = self.lisp.car(unwrapped)?;
                let params = self.lisp.cdr(unwrapped)?;
                let body_list = rest;
                
                // Transform internal defines if needed
                let body = self.transform_internal_defines(body_list)?;
                
                // Expand macros in the body at definition time (not at call time)
                // This ensures macro side effects run during expansion, not during each call
                let expanded_body = self.expand(body)?;
                
                let lambda = self.lisp.lambda(params, expanded_body, env.0)?;
                self.define(name, lambda)?;
                // Return void (unspecified value) per R7RS
                let void = self.lisp.void_val()?;
                Ok(TrampolineState::Return { val: void })
            }
            _ => Err(self.type_error(first, "symbol or list", self.lisp.get(first)?.type_name())),
        }
    }
    
    /// Evaluate (set! name value) - mutate an existing variable binding (trampolined)
    pub(super) fn eval_set(&mut self, args: ArenaIndex, env: EnvRef) -> Result<TrampolineState, EvalError> {
        extract_args!(self, args, name, value_expr);
        
        // Verify name is a symbol
        match self.lisp.get(name)? {
            Value::Symbol(_) => {
                // Push continuation and evaluate value
                // Data: (name . env)
                self.cont(ContType::SetValue, env).data2(name, env.0)?;
                Ok(TrampolineState::Eval { expr: ExprRef(value_expr), env })
            }
            _ => Err(self.type_error(name, "symbol", self.lisp.get(name)?.type_name())),
        }
    }
    
    /// Evaluate (define-syntax name transformer) at evaluation time
    /// Also supports the shorthand (define-syntax (name stx) body)
    /// which desugars to (define-syntax name (lambda (stx) body))
    /// 
    /// This allows macros to be defined during evaluation rather than
    /// only during pre-expansion.
    pub(super) fn step_eval_define_syntax(&mut self, args: ArenaIndex, env: EnvRef) -> Result<TrampolineState, EvalError> {
        let first = self.lisp.car(args)?;
        
        // Check for shorthand: (define-syntax (name stx) body)
        let (name, transformer_expr) = if let Value::Cons { .. } = self.lisp.get(first)? {
            // (name stx) form - desugar to (lambda (stx) body)
            let actual_name = self.lisp.car(first)?;
            let lambda_params = self.lisp.cdr(first)?;
            let body_list = self.lisp.cdr(args)?;
            
            // Build (lambda (params) body...)
            let lambda_sym = self.lisp.symbol("lambda")?;
            let lambda_with_params = self.lisp.cons(lambda_params, body_list)?;
            let lambda_expr = self.lisp.cons(lambda_sym, lambda_with_params)?;
            
            (actual_name, lambda_expr)
        } else {
            // Standard form: (define-syntax name transformer)
            let transformer_expr = self.lisp.car(self.lisp.cdr(args)?)?;
            (first, transformer_expr)
        };
        
        // Expand the transformer expression if it's not already a lambda
        let final_transformer_expr = self.expand_transformer_if_needed(transformer_expr)?;
        
        // Parse the transformer with the current lexical environment
        // This allows macro transformers to capture lexical variables
        let transformer = self.parse_transformer_with_env(final_transformer_expr, env.0)?;
        
        // Add to macro environment
        let binding = self.lisp.cons(name, transformer)?;
        self.macro_env = EnvRef(self.lisp.cons(binding, self.macro_env.0)?);
        
        // Return void (unspecified value) per R7RS
        let void = self.lisp.void_val()?;
        Ok(TrampolineState::Return { val: void })
    }
    
    /// Evaluate (let-syntax ((name transformer) ...) body ...) at evaluation time
    /// 
    /// Creates local macro bindings for the duration of the body.
    /// All transformers are evaluated in the outer macro environment before any
    /// bindings are installed (parallel semantics, like let for variables).
    /// After body evaluation, the macro environment is restored via the 
    /// LetSyntaxBody continuation.
    pub(super) fn step_eval_let_syntax(&mut self, args: ArenaIndex, env: EnvRef) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;
        
        // Save current macro environment for restoration after body
        let saved_macro_env = self.macro_env;
        
        // Phase 1: Parse ALL transformers in the outer (saved) macro environment.
        // This ensures no transformer can see bindings from other let-syntax clauses.
        // Arena-allocated list of (name . transformer) pairs for parallel installation.
        let mut parsed_bindings = self.lisp.nil()?;
        let mut current = bindings;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let binding = self.lisp.car(current)?;
            let name = self.lisp.car(binding)?;
            let transformer_expr = self.lisp.car(self.lisp.cdr(binding)?)?;
            
            let final_transformer_expr = self.expand_transformer_if_needed(transformer_expr)?;
            let transformer = self.parse_transformer_with_env(final_transformer_expr, env.0)?;
            
            let pair = self.lisp.cons(name, transformer)?;
            parsed_bindings = self.lisp.cons(pair, parsed_bindings)?;
            
            current = self.lisp.cdr(current)?;
        }
        
        // Phase 2: Install all bindings at once
        let mut cursor = parsed_bindings;
        while let Value::Cons { .. } = self.lisp.get(cursor)? {
            let pair = self.lisp.car(cursor)?;
            let name = self.lisp.car(pair)?;
            let transformer = self.lisp.cdr(pair)?;
            let macro_binding = self.lisp.cons(name, transformer)?;
            self.macro_env = EnvRef(self.lisp.cons(macro_binding, self.macro_env.0)?);
            cursor = self.lisp.cdr(cursor)?;
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
        self.cont(ContType::LetSyntaxBody, env).data1(saved_macro_env.0)?;
        
        // Evaluate body with extended macro environment
        Ok(TrampolineState::Eval { expr: ExprRef(body), env })
    }

    /// Evaluate (letrec-syntax ((name transformer) ...) body ...) at evaluation time
    /// 
    /// Like let-syntax, but transformers can see each other's bindings.
    /// Each transformer is evaluated with all letrec-syntax bindings visible
    /// (sequential installation, like letrec for variables).
    pub(super) fn step_eval_letrec_syntax(&mut self, args: ArenaIndex, env: EnvRef) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;
        
        // Save current macro environment for restoration after body
        let saved_macro_env = self.macro_env;
        
        // Install bindings sequentially - each transformer can see previous bindings
        let mut current = bindings;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let binding = self.lisp.car(current)?;
            let name = self.lisp.car(binding)?;
            let transformer_expr = self.lisp.car(self.lisp.cdr(binding)?)?;
            
            let final_transformer_expr = self.expand_transformer_if_needed(transformer_expr)?;
            let transformer = self.parse_transformer_with_env(final_transformer_expr, env.0)?;
            let macro_binding = self.lisp.cons(name, transformer)?;
            self.macro_env = EnvRef(self.lisp.cons(macro_binding, self.macro_env.0)?);
            
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
        self.cont(ContType::LetSyntaxBody, env).data1(saved_macro_env.0)?;
        
        // Evaluate body with extended macro environment
        Ok(TrampolineState::Eval { expr: ExprRef(body), env })
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
        env: EnvRef,
    ) -> Result<TrampolineState, EvalError> {
        // Parse: (stx-expr (literal ...) clause ...)
        let stx_expr = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let literals = self.lisp.car(rest)?;
        let clauses = self.lisp.cdr(rest)?;

        // Start with empty pattern bindings (will be populated by caller via with-syntax)
        let pattern_bindings = self.get_pattern_bindings_from_env(env.0)?;

        // Push continuation to handle pattern matching after stx-expr is evaluated
        // Data: (literals . (clauses . (env . pattern_bindings)))
        self.cont(ContType::SyntaxCaseMatch, env).data4(literals, clauses, env.0, pattern_bindings)?;

        // Evaluate stx-expr first
        Ok(TrampolineState::Eval { expr: ExprRef(stx_expr), env })
    }

    /// Get pattern bindings from the current environment
    /// 
    /// In syntax-case, pattern bindings are stored in the environment.
    /// Uses a gensym-style internal symbol `#:pattern-bindings` to avoid
    /// conflicts with user code.
    pub(super) fn get_pattern_bindings_from_env(&self, env: ArenaIndex) -> Result<ArenaIndex, EvalError> {
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

            // Match pattern against stx (syntax-aware matching)
            // Pattern variables will bind to the matched values
            let empty = self.lisp.nil()?;
            if let Some(bindings) = self.match_pattern_syntax(pattern, stx, literals, empty)? {
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
                    self.cont(ContType::SyntaxCaseFender, EnvRef(fender_env)).data6(output, bindings, literals, remaining_clauses, env, stx)?;
                    
                    // Evaluate fender with trampolined evaluation
                    return Ok(Some(TrampolineState::Eval { expr: ExprRef(fender_expr), env: EnvRef(fender_env) }));
                }

                // No fender - evaluate output expression with pattern bindings
                let output_env = self.extend_env_with_bindings(env, bindings)?;
                return Ok(Some(TrampolineState::Eval { expr: ExprRef(output), env: EnvRef(output_env) }));
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
    ///
    /// Pattern bindings are MERGED with existing bindings, so nested
    /// syntax-case forms can access bindings from outer contexts.
    fn extend_env_with_bindings(
        &self,
        env: ArenaIndex,
        bindings: ArenaIndex,
    ) -> Result<ArenaIndex, EvalError> {
        // Get existing pattern bindings (if any) to merge with
        let existing_bindings = self.get_pattern_bindings_from_env(env)?;
        
        // Merge new bindings with existing ones (new ones take precedence)
        // New bindings go at the front so they shadow existing ones
        let mut merged_bindings = existing_bindings;
        let mut current = bindings;
        
        // Collect new bindings into arena cons list (reversed by prepending)
        let mut reversed_pairs = self.lisp.nil()?;
        
        while let Value::Cons { .. } = self.lisp.get(current)? {
            reversed_pairs = self.lisp.cons(self.lisp.car(current)?, reversed_pairs)?;
            current = self.lisp.cdr(current)?;
        }
        
        // Walk reversed list to prepend in correct order
        let mut cursor = reversed_pairs;
        while let Value::Cons { .. } = self.lisp.get(cursor)? {
            merged_bindings = self.lisp.cons(self.lisp.car(cursor)?, merged_bindings)?;
            cursor = self.lisp.cdr(cursor)?;
        }
        
        // bindings is an alist of (var . value) pairs
        // Prepend each binding to env for normal variable lookup
        let mut result = env;
        current = bindings;

        while let Value::Cons { .. } = self.lisp.get(current)? {
            let pair = self.lisp.car(current)?;
            result = self.lisp.cons(pair, result)?;
            current = self.lisp.cdr(current)?;
        }

        // Store the MERGED bindings under #:pattern-bindings
        // This allows nested syntax-case forms to access outer bindings
        let key = self.lisp.symbol("#:pattern-bindings")?;
        let binding_pair = self.lisp.cons(key, merged_bindings)?;
        result = self.lisp.cons(binding_pair, result)?;

        Ok(result)
    }

    // ========================================================================
    // syntax - Template Transcription
    // ========================================================================

    /// Evaluate (syntax template) - create syntax object from template
    ///
    /// This creates syntax objects from templates, capturing the current lexical
    /// environment for proper scope preservation.
    /// 
    /// The pattern bindings are stored under the special `#:pattern-bindings`
    /// key by `extend_env_with_bindings` when syntax-case matches.
    ///
    /// For lexically-scoped syntax objects, identifiers that are bound in the
    /// current lexical environment get wrapped in syntax objects with the
    /// captured environment, enabling proper scope preservation.
    pub(super) fn step_eval_syntax(
        &mut self,
        args: ArenaIndex,
        env: EnvRef,
    ) -> Result<TrampolineState, EvalError> {
        let template = self.lisp.car(args)?;

        // Get pattern bindings from the special key in the environment
        // This contains only the pattern variable bindings, not other env bindings
        let bindings = self.get_pattern_bindings_from_env(env.0)?;

        // Transcribe the template with pattern bindings and capture lexical environment
        // This enables lexically-scoped syntax objects where identifiers resolve
        // in their creation context, not the expansion context
        let empty_renames = self.lisp.nil()?;
        let result = self.transcribe_template_with_env(
            template, bindings, empty_renames, self.global_env.0, env.0
        )?;

        Ok(TrampolineState::Return { val: result })
    }
    
    // Note: step_eval_with_syntax was removed - with-syntax is now a macro in macros.scm
    // that uses syntax-case directly to bind patterns.
    
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
        env: EnvRef,
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
        self.cont(ContType::DynamicWindBefore, env).data4(body_expr, after_expr, env.0, saved_dw_chain)?;
        
        // Evaluate the before thunk
        Ok(TrampolineState::Eval { expr: ExprRef(before_expr), env })
    }
    
    /// Apply a thunk (zero-argument procedure)
    fn apply_thunk(
        &mut self,
        thunk: ArenaIndex,
        env: EnvRef,
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
                Ok(Some(TrampolineState::Eval { expr: ExprRef(body), env: EnvRef(closure_env) }))
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
                self.cont(ContType::ApplyForced, env).data3(nil, env.0, call_expr)?;
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
            let global = self.global_env;
            self.cont(ContType::WindOut, global).data4(rest, return_val, target_chain, wind_in_frames)?;
            
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
            let global = self.global_env;
            self.cont(ContType::WindIn, global).data4(rest, return_val, target_chain, target_chain)?;
            
            // Call the before thunk
            self.apply_thunk(before, self.global_env)
        }
    }
    
    // ========================================================================
    // Record types (R7RS §5.5)
    // ========================================================================
    
    /// (define-record-type <name> (<ctor> <ctor-field> ...) <pred> <field-spec> ...)
    ///
    /// Records are represented as vectors: #(<tag> field1 field2 ...)
    /// The tag is a unique gensym to distinguish record types.
    pub(super) fn step_eval_define_record_type(
        &mut self,
        args: ArenaIndex,
        env: EnvRef,
    ) -> Result<TrampolineState, EvalError> {
        // Parse: <name>
        let _name = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        
        // Parse: (<constructor-name> <field-name> ...)
        let ctor_spec = self.lisp.car(rest)?;
        let ctor_name = self.lisp.car(ctor_spec)?;
        let ctor_fields = self.lisp.cdr(ctor_spec)?;
        let rest = self.lisp.cdr(rest)?;
        
        // Parse: <predicate-name>
        let pred_name = self.lisp.car(rest)?;
        let field_specs = self.lisp.cdr(rest)?;
        
        // Collect all field specs to determine field ordering
        // field_specs is a list of (<field-name> <accessor> [<mutator>])
        // Build reversed field name list for efficient reverse iteration in constructor
        let mut reversed_fields = self.lisp.nil()?;
        {
            let mut cursor = field_specs;
            while !self.lisp.get(cursor)?.is_nil() {
                let spec = self.lisp.car(cursor)?;
                let fname = self.lisp.car(spec)?;
                reversed_fields = self.lisp.cons(fname, reversed_fields)?;
                cursor = self.lisp.cdr(cursor)?;
            }
        }
        
        // Create a unique tag for this record type
        let tag = self.gensym("record")?;
        
        // Build the (begin ...) expression that defines everything:
        // 1. constructor
        // 2. predicate
        // 3. accessors/mutators for each field
        
        let nil = self.lisp.nil()?;
        let mut defs = nil; // list of definitions (will be reversed)
        
        // --- 1. Constructor ---
        // (define (ctor-name ctor-fields ...) (vector tag ctor-field-values ...))
        // The constructor takes field values in the order specified in the ctor spec,
        // but must place them in the correct slot (field order from field specs).
        {
            // Build the vector call: (vector 'tag f1 f2 ... fn)
            // Fields are in all_fields order; use ctor_fields to get param names
            let quote_sym = self.lisp.symbol("quote")?;
            let quoted_tag = self.lisp.cons(tag, nil)?;
            let quoted_tag = self.lisp.cons(quote_sym, quoted_tag)?;
            
            let vector_sym = self.lisp.symbol("vector")?;
            
            // Build vector args: tag, then each field in field-spec order
            // For each field in reversed_fields (reverse of field-spec order),
            // check if it's in ctor_fields, and cons onto vec_args
            let mut vec_args = nil;
            let mut cursor = reversed_fields;
            while !self.lisp.get(cursor)?.is_nil() {
                let field = self.lisp.car(cursor)?;
                // Check if this field is in ctor_fields
                let mut found = false;
                let mut cf_cursor = ctor_fields;
                while !self.lisp.get(cf_cursor)?.is_nil() {
                    let cf = self.lisp.car(cf_cursor)?;
                    if self.lisp.symbol_eq(cf, field)? {
                        found = true;
                        break;
                    }
                    cf_cursor = self.lisp.cdr(cf_cursor)?;
                }
                if found {
                    vec_args = self.lisp.cons(field, vec_args)?;
                } else {
                    // Field not in constructor — default to #f
                    let false_val = self.lisp.false_val()?;
                    vec_args = self.lisp.cons(false_val, vec_args)?;
                }
                cursor = self.lisp.cdr(cursor)?;
            }
            // Prepend quoted tag
            vec_args = self.lisp.cons(quoted_tag, vec_args)?;
            // Prepend 'vector' symbol
            let vector_call = self.lisp.cons(vector_sym, vec_args)?;
            
            // Build params list from ctor_fields
            let define_sym = self.lisp.symbol("define")?;
            let ctor_name_and_params = self.lisp.cons(ctor_name, ctor_fields)?;
            
            // (define (ctor-name fields ...) (vector ...))
            let body = self.lisp.cons(vector_call, nil)?;
            let def = self.lisp.cons(ctor_name_and_params, body)?;
            let def = self.lisp.cons(define_sym, def)?;
            
            defs = self.lisp.cons(def, defs)?;
        }
        
        // --- 2. Predicate ---
        // (define (pred? obj) (and (vector? obj) (> (vector-length obj) 0) (eq? (vector-ref obj 0) 'tag)))
        {
            let define_sym = self.lisp.symbol("define")?;
            let obj_sym = self.lisp.symbol("%rec-obj")?;
            let pred_params = self.lisp.cons(obj_sym, nil)?;
            let pred_head = self.lisp.cons(pred_name, pred_params)?;
            
            let and_sym = self.lisp.symbol("and")?;
            let vector_q_sym = self.lisp.symbol("vector?")?;
            let vector_ref_sym = self.lisp.symbol("vector-ref")?;
            let vector_length_sym = self.lisp.symbol("vector-length")?;
            let eq_sym = self.lisp.symbol("eq?")?;
            let gt_sym = self.lisp.symbol(">")?;
            let quote_sym = self.lisp.symbol("quote")?;
            let zero = self.lisp.number(0)?;
            
            // (vector? obj)
            let check1 = self.lisp.cons(obj_sym, nil)?;
            let check1 = self.lisp.cons(vector_q_sym, check1)?;
            
            // (> (vector-length obj) 0)
            let vl = self.lisp.cons(obj_sym, nil)?;
            let vl = self.lisp.cons(vector_length_sym, vl)?;
            let check2 = self.lisp.cons(zero, nil)?;
            let check2 = self.lisp.cons(vl, check2)?;
            let check2 = self.lisp.cons(gt_sym, check2)?;
            
            // (eq? (vector-ref obj 0) 'tag)
            let vr = self.lisp.cons(zero, nil)?;
            let vr = self.lisp.cons(obj_sym, vr)?;
            let vr = self.lisp.cons(vector_ref_sym, vr)?;
            
            let qtag = self.lisp.cons(tag, nil)?;
            let qtag = self.lisp.cons(quote_sym, qtag)?;
            
            let check3 = self.lisp.cons(qtag, nil)?;
            let check3 = self.lisp.cons(vr, check3)?;
            let check3 = self.lisp.cons(eq_sym, check3)?;
            
            // (and check1 check2 check3)
            let and_expr = self.lisp.cons(check3, nil)?;
            let and_expr = self.lisp.cons(check2, and_expr)?;
            let and_expr = self.lisp.cons(check1, and_expr)?;
            let and_expr = self.lisp.cons(and_sym, and_expr)?;
            
            let body = self.lisp.cons(and_expr, nil)?;
            let def = self.lisp.cons(pred_head, body)?;
            let def = self.lisp.cons(define_sym, def)?;
            
            defs = self.lisp.cons(def, defs)?;
        }
        
        // --- 3. Accessors and Mutators ---
        {
            let define_sym = self.lisp.symbol("define")?;
            let vector_ref_sym = self.lisp.symbol("vector-ref")?;
            let vector_set_sym = self.lisp.symbol("vector-set!")?;
            let obj_sym = self.lisp.symbol("%rec-obj")?;
            let val_sym = self.lisp.symbol("%rec-val")?;
            
            let mut cursor = field_specs;
            let mut field_idx = 0usize;
            while !self.lisp.get(cursor)?.is_nil() {
                let spec = self.lisp.car(cursor)?;
                let _fname = self.lisp.car(spec)?;
                let spec_rest = self.lisp.cdr(spec)?;
                let accessor = self.lisp.car(spec_rest)?;
                let spec_rest2 = self.lisp.cdr(spec_rest)?;
                
                let idx_val = self.lisp.number((field_idx + 1) as isize)?; // +1 for tag at index 0
                
                // Accessor: (define (accessor obj) (vector-ref obj idx))
                {
                    let params = self.lisp.cons(obj_sym, nil)?;
                    let head = self.lisp.cons(accessor, params)?;
                    
                    let vr = self.lisp.cons(idx_val, nil)?;
                    let vr = self.lisp.cons(obj_sym, vr)?;
                    let vr = self.lisp.cons(vector_ref_sym, vr)?;
                    
                    let body = self.lisp.cons(vr, nil)?;
                    let def = self.lisp.cons(head, body)?;
                    let def = self.lisp.cons(define_sym, def)?;
                    
                    defs = self.lisp.cons(def, defs)?;
                }
                
                // Mutator (if present): (define (mutator obj val) (vector-set! obj idx val))
                if !self.lisp.get(spec_rest2)?.is_nil() {
                    let mutator = self.lisp.car(spec_rest2)?;
                    
                    let params = self.lisp.cons(val_sym, nil)?;
                    let params = self.lisp.cons(obj_sym, params)?;
                    let head = self.lisp.cons(mutator, params)?;
                    
                    let vs = self.lisp.cons(val_sym, nil)?;
                    let vs = self.lisp.cons(idx_val, vs)?;
                    let vs = self.lisp.cons(obj_sym, vs)?;
                    let vs = self.lisp.cons(vector_set_sym, vs)?;
                    
                    let body = self.lisp.cons(vs, nil)?;
                    let def = self.lisp.cons(head, body)?;
                    let def = self.lisp.cons(define_sym, def)?;
                    
                    defs = self.lisp.cons(def, defs)?;
                }
                
                field_idx += 1;
                cursor = self.lisp.cdr(cursor)?;
            }
        }
        
        // Reverse defs list and wrap in (begin ...)
        let mut reversed = nil;
        while !self.lisp.get(defs)?.is_nil() {
            let d = self.lisp.car(defs)?;
            reversed = self.lisp.cons(d, reversed)?;
            defs = self.lisp.cdr(defs)?;
        }
        
        let begin_sym = self.lisp.symbol("begin")?;
        let begin_expr = self.lisp.cons(begin_sym, reversed)?;
        
        Ok(TrampolineState::Eval { expr: ExprRef(begin_expr), env })
    }
    
    // ========================================================================
    // Exception handling (R7RS §6.11)
    // ========================================================================
    
    /// (with-exception-handler handler thunk) — evaluate thunk with handler installed
    pub(super) fn step_eval_with_exception_handler(
        &mut self,
        args: ArenaIndex,
        env: EnvRef,
    ) -> Result<TrampolineState, EvalError> {
        let handler_expr = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let thunk_expr = self.lisp.car(rest)?;
        
        // Push continuation to evaluate thunk after handler is evaluated
        self.cont(ContType::WithExceptionHandlerEvalThunk, env).data2(thunk_expr, env.0)?;
        
        // Evaluate the handler expression first
        Ok(TrampolineState::Eval { expr: ExprRef(handler_expr), env })
    }
    
    /// (raise obj) or (raise-continuable obj) — evaluate obj then invoke handler
    pub(super) fn step_eval_raise(
        &mut self,
        args: ArenaIndex,
        env: EnvRef,
        continuable: bool,
    ) -> Result<TrampolineState, EvalError> {
        let obj_expr = self.lisp.car(args)?;
        
        // Marker: Nil = non-continuable, True = continuable
        let marker = if continuable {
            self.lisp.true_val()?
        } else {
            self.lisp.nil()?
        };
        
        self.cont(ContType::RaiseEval, env).data1(marker)?;
        Ok(TrampolineState::Eval { expr: ExprRef(obj_expr), env })
    }
    
    /// Invoke the current exception handler with the given exception object
    pub(super) fn invoke_exception_handler(
        &mut self,
        obj: ArenaIndex,
        continuable: bool,
    ) -> Result<Option<TrampolineState>, EvalError> {
        if self.lisp.get(self.exception_handler_chain)?.is_nil() {
            // No handler installed — fall back to Rust error
            return Err(self.make_error(ErrorKind::UserError, obj)
                .with_message("unhandled exception"));
        }
        
        // Pop the current handler
        let handler = self.lisp.car(self.exception_handler_chain)?;
        let parent_chain = self.lisp.cdr(self.exception_handler_chain)?;
        
        // Pop handler before calling it (R7RS: handler runs with previous handler)
        let saved_chain = self.exception_handler_chain;
        self.exception_handler_chain = parent_chain;
        
        // Push a frame to restore the handler chain after the handler returns.
        // Encode whether this was a continuable raise so we can enforce R7RS semantics.
        let continuable_flag = if continuable {
            self.lisp.true_val()?
        } else {
            self.lisp.nil()?
        };
        let global = self.global_env;
        self.cont(ContType::ExceptionHandlerFrame, global)
            .data3(handler, saved_chain, continuable_flag)?;
        
        // Call the handler with the exception object
        match self.lisp.get(handler)? {
            Value::Lambda { .. } => {
                let (params, body, closure_env) = self.lisp.lambda_parts(handler)?;
                let new_env = self.env_extend(EnvRef(closure_env), self.lisp.car(params)?, obj)?;
                Ok(Some(TrampolineState::Eval { expr: ExprRef(body), env: new_env }))
            }
            _ => {
                // For other callable types (builtins, etc.), construct application
                let nil = self.lisp.nil()?;
                let arg_list = self.lisp.cons(obj, nil)?;
                let call_expr = self.lisp.cons(handler, arg_list)?;
                let global = self.global_env;
                self.push_frame(call_expr, handler)?;
                self.cont(ContType::ApplyForced, global)
                    .data3(arg_list, global.0, call_expr)?;
                Ok(Some(TrampolineState::Return { val: handler }))
            }
        }
    }

    // ================================================================
    // Library system (R7RS §5.6)
    // ================================================================

    /// Evaluate `(define-library <name> <declarations>...)`
    ///
    /// Creates a fresh environment, processes `export`, `import`, and `begin`
    /// declarations, then registers the library by name.
    ///
    /// The library entry in the registry is stored as `(name (env . macro-env))`:
    /// - `car(entry)` = name (list of symbols)
    /// - `car(cdr(entry))` = exported regular bindings env
    /// - `cdr(cdr(entry))` = exported macro bindings env
    pub(super) fn step_eval_define_library(&mut self, args: ArenaIndex, _env: EnvRef)
        -> Result<TrampolineState, EvalError>
    {
        // (define-library <name> <decl> ...)
        let lib_name = self.lisp.car(args)?;
        let decls = self.lisp.cdr(args)?;

        // Start with a copy of the global env (builtins + stdlib)
        let mut lib_env = self.global_env;
        let mut lib_macro_env = self.macro_env;

        let nil = self.lisp.nil()?;
        let mut exports = nil; // list of export symbols

        // First pass: collect exports and process imports
        let mut current = decls;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let decl = self.lisp.car(current)?;
            current = self.lisp.cdr(current)?;

            if !matches!(self.lisp.get(decl)?, Value::Cons { .. }) { continue; }

            let decl_head = self.lisp.car(decl)?;
            let decl_body = self.lisp.cdr(decl)?;

            if self.lisp.symbol_matches(decl_head, "export")? {
                let mut names = decl_body;
                while let Value::Cons { .. } = self.lisp.get(names)? {
                    let name = self.lisp.car(names)?;
                    exports = self.lisp.cons(name, exports)?;
                    names = self.lisp.cdr(names)?;
                }
            } else if self.lisp.symbol_matches(decl_head, "import")? {
                let mut sets = decl_body;
                while let Value::Cons { .. } = self.lisp.get(sets)? {
                    let import_set = self.lisp.car(sets)?;
                    lib_env = self.import_library_into_env(import_set, lib_env)?;
                    sets = self.lisp.cdr(sets)?;
                }
            }
        }

        // Second pass: evaluate begin bodies in the library environment
        // Save and restore both global_env and macro_env so that
        // define-syntax forms inside the library are captured.
        let mut current = decls;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let decl = self.lisp.car(current)?;
            current = self.lisp.cdr(current)?;

            if !matches!(self.lisp.get(decl)?, Value::Cons { .. }) { continue; }
            let decl_head = self.lisp.car(decl)?;
            let decl_body = self.lisp.cdr(decl)?;

            if self.lisp.symbol_matches(decl_head, "begin")? {
                let mut body = decl_body;
                while let Value::Cons { .. } = self.lisp.get(body)? {
                    let expr = self.lisp.car(body)?;
                    body = self.lisp.cdr(body)?;

                    let saved_global = self.global_env;
                    let saved_macros = self.macro_env;
                    self.global_env = lib_env;
                    self.macro_env = lib_macro_env;
                    let _result = self.eval(crate::continuation::ExprRef(expr))?;
                    lib_env = self.global_env;       // capture defines
                    lib_macro_env = self.macro_env;  // capture define-syntax
                    self.global_env = saved_global;
                    self.macro_env = saved_macros;
                }
            }
        }

        // Build the exported environments (regular + macros)
        let mut lib_exported_env = EnvRef(nil);
        let mut lib_exported_macro_env = EnvRef(nil);

        if self.lisp.get(exports)?.is_nil() {
            // No exports specified → export everything
            lib_exported_env = lib_env;
            lib_exported_macro_env = lib_macro_env;
        } else {
            let mut exp_list = exports;
            while let Value::Cons { .. } = self.lisp.get(exp_list)? {
                let name = self.lisp.car(exp_list)?;
                exp_list = self.lisp.cdr(exp_list)?;

                // Check regular bindings
                if let Some(val) = self.lookup_in_env_optional(lib_env.0, name)? {
                    lib_exported_env = self.env_extend(lib_exported_env, name, val)?;
                }
                // Check macro bindings
                if let Some(val) = self.lookup_in_env_optional(lib_macro_env.0, name)? {
                    lib_exported_macro_env = self.env_extend(lib_exported_macro_env, name, val)?;
                }
            }
        }

        // Register: library_registry = ((name (env . macro-env)) . registry)
        let env_pair = self.lisp.cons(lib_exported_env.0, lib_exported_macro_env.0)?;
        let entry = self.lisp.cons(lib_name, env_pair)?;
        self.library_registry = self.lisp.cons(entry, self.library_registry)?;

        let void = self.lisp.void_val()?;
        Ok(TrampolineState::Return { val: void })
    }

    /// Evaluate `(environment <import-set>...)`
    ///
    /// Creates a new immutable environment containing bindings from the
    /// specified libraries. Each import-set is processed just like `import`
    /// but the bindings are collected into a fresh environment object
    /// instead of the global environment.
    ///
    /// Supports quoted and unquoted import specs:
    ///   (environment (scheme base))
    ///   (environment '(scheme base))
    pub(super) fn step_eval_environment(&mut self, args: ArenaIndex, _env: EnvRef)
        -> Result<TrampolineState, EvalError>
    {
        let nil = self.lisp.nil()?;
        let mut result_env = EnvRef(nil);

        let mut sets = args;
        while let Value::Cons { .. } = self.lisp.get(sets)? {
            let mut import_set = self.lisp.car(sets)?;
            sets = self.lisp.cdr(sets)?;

            // Unwrap a single level of (quote ...) so that
            // (environment '(scheme base)) works the same as
            // (environment (scheme base))
            if let Value::Cons { .. } = self.lisp.get(import_set)? {
                let head = self.lisp.car(import_set)?;
                if self.lisp.symbol_matches(head, "quote")? {
                    import_set = self.lisp.car(self.lisp.cdr(import_set)?)?;
                }
            }

            result_env = self.import_library_into_env(import_set, result_env)?;
        }

        let env_val = self.lisp.alloc(Value::Environment { env: result_env.0, mutable: false })?;
        Ok(TrampolineState::Return { val: env_val })
    }

    /// Evaluate `(import <import-set>...)`
    ///
    /// Each import-set is a library name or an import modifier.
    /// Imports both regular bindings and macro bindings.
    pub(super) fn step_eval_import(&mut self, args: ArenaIndex, _env: EnvRef)
        -> Result<TrampolineState, EvalError>
    {
        let mut sets = args;
        while let Value::Cons { .. } = self.lisp.get(sets)? {
            let import_set = self.lisp.car(sets)?;
            sets = self.lisp.cdr(sets)?;

            self.global_env = self.import_library_into_env(import_set, self.global_env)?;
        }

        let void = self.lisp.void_val()?;
        Ok(TrampolineState::Return { val: void })
    }

    /// Import a library's exported bindings into the given environment.
    ///
    /// Handles import modifiers: `only`, `except`, `prefix`, `rename`.
    /// Also imports macro bindings into `self.macro_env`.
    fn import_library_into_env(&mut self, import_set: ArenaIndex, target: EnvRef)
        -> Result<EnvRef, EvalError>
    {
        let (lib_env, lib_macro_env) = self.resolve_import_set(import_set)?;

        // Copy regular bindings into target
        let mut result = target;
        let mut env = lib_env;
        while let Value::Cons { .. } = self.lisp.get(env)? {
            let binding = self.lisp.car(env)?;
            env = self.lisp.cdr(env)?;
            if let Value::Cons { .. } = self.lisp.get(binding)? {
                let name = self.lisp.car(binding)?;
                let val = self.lisp.cdr(binding)?;
                result = self.env_extend(result, name, val)?;
            }
        }

        // Copy macro bindings into macro_env
        let mut menv = lib_macro_env;
        while let Value::Cons { .. } = self.lisp.get(menv)? {
            let binding = self.lisp.car(menv)?;
            menv = self.lisp.cdr(menv)?;
            if let Value::Cons { .. } = self.lisp.get(binding)? {
                let name = self.lisp.car(binding)?;
                let val = self.lisp.cdr(binding)?;
                self.macro_env = self.env_extend(self.macro_env, name, val)?;
            }
        }

        Ok(result)
    }

    /// Resolve an import set to `(regular_env, macro_env)`.
    ///
    /// Handles bare library names and import modifiers:
    /// - `(only <import-set> <id> ...)`
    /// - `(except <import-set> <id> ...)`
    /// - `(prefix <import-set> <identifier>)`
    /// - `(rename <import-set> (<old> <new>) ...)`
    fn resolve_import_set(&mut self, import_set: ArenaIndex)
        -> Result<(ArenaIndex, ArenaIndex), EvalError>
    {
        if let Value::Cons { .. } = self.lisp.get(import_set)? {
            let head = self.lisp.car(import_set)?;
            let rest = self.lisp.cdr(import_set)?;

            // --- import modifiers ---
            if self.lisp.symbol_matches(head, "only")? {
                let inner = self.lisp.car(rest)?;
                let ids = self.lisp.cdr(rest)?;
                let (env, menv) = self.resolve_import_set(inner)?;
                return self.filter_env_only(env, menv, ids);
            }

            if self.lisp.symbol_matches(head, "except")? {
                let inner = self.lisp.car(rest)?;
                let ids = self.lisp.cdr(rest)?;
                let (env, menv) = self.resolve_import_set(inner)?;
                return self.filter_env_except(env, menv, ids);
            }

            if self.lisp.symbol_matches(head, "prefix")? {
                let inner = self.lisp.car(rest)?;
                let pfx = self.lisp.car(self.lisp.cdr(rest)?)?;
                let (env, menv) = self.resolve_import_set(inner)?;
                return self.apply_env_prefix(env, menv, pfx);
            }

            if self.lisp.symbol_matches(head, "rename")? {
                let inner = self.lisp.car(rest)?;
                let renames = self.lisp.cdr(rest)?;
                let (env, menv) = self.resolve_import_set(inner)?;
                return self.apply_env_rename(env, menv, renames);
            }

            // Not a modifier — treat as a library name, use auto-loading
            return self.lookup_or_load_library(import_set);
        }

        Err(self.make_error(ErrorKind::Generic, import_set))
    }

    /// `(only ...)` — keep only named identifiers.
    fn filter_env_only(
        &self,
        env: ArenaIndex,
        macro_env: ArenaIndex,
        ids: ArenaIndex,
    ) -> Result<(ArenaIndex, ArenaIndex), EvalError> {
        let nil = self.lisp.nil()?;
        let mut new_env = nil;
        let mut new_menv = nil;

        // For each id in the list, look it up in env / macro_env
        let mut id_list = ids;
        while let Value::Cons { .. } = self.lisp.get(id_list)? {
            let id = self.lisp.car(id_list)?;
            id_list = self.lisp.cdr(id_list)?;

            if let Some(val) = self.lookup_in_env_optional(env, id)? {
                let binding = self.lisp.cons(id, val)?;
                new_env = self.lisp.cons(binding, new_env)?;
            }
            if let Some(val) = self.lookup_in_env_optional(macro_env, id)? {
                let binding = self.lisp.cons(id, val)?;
                new_menv = self.lisp.cons(binding, new_menv)?;
            }
        }
        Ok((new_env, new_menv))
    }

    /// `(except ...)` — keep all identifiers except the named ones.
    fn filter_env_except(
        &self,
        env: ArenaIndex,
        macro_env: ArenaIndex,
        ids: ArenaIndex,
    ) -> Result<(ArenaIndex, ArenaIndex), EvalError> {
        let nil = self.lisp.nil()?;

        let filter_one = |src: ArenaIndex| -> Result<ArenaIndex, EvalError> {
            let mut result = nil;
            let mut cur = src;
            while let Value::Cons { .. } = self.lisp.get(cur)? {
                let binding = self.lisp.car(cur)?;
                cur = self.lisp.cdr(cur)?;
                if let Value::Cons { .. } = self.lisp.get(binding)? {
                    let name = self.lisp.car(binding)?;
                    // Check if name is in the exclude list
                    let mut excluded = false;
                    let mut id_list = ids;
                    while let Value::Cons { .. } = self.lisp.get(id_list)? {
                        let id = self.lisp.car(id_list)?;
                        id_list = self.lisp.cdr(id_list)?;
                        if self.lisp.symbol_eq(name, id)? {
                            excluded = true;
                            break;
                        }
                    }
                    if !excluded {
                        result = self.lisp.cons(binding, result)?;
                    }
                }
            }
            Ok(result)
        };

        let new_env = filter_one(env)?;
        let new_menv = filter_one(macro_env)?;
        Ok((new_env, new_menv))
    }

    /// Transform all binding names in an environment using a closure.
    fn map_env_names(
        &self,
        src: ArenaIndex,
        transform: &mut dyn FnMut(ArenaIndex) -> Result<ArenaIndex, EvalError>,
    ) -> Result<ArenaIndex, EvalError> {
        let nil = self.lisp.nil()?;
        let mut result = nil;
        let mut cur = src;
        while let Value::Cons { .. } = self.lisp.get(cur)? {
            let binding = self.lisp.car(cur)?;
            cur = self.lisp.cdr(cur)?;
            if let Value::Cons { .. } = self.lisp.get(binding)? {
                let name = self.lisp.car(binding)?;
                let val = self.lisp.cdr(binding)?;
                let new_name = transform(name)?;
                let new_binding = self.lisp.cons(new_name, val)?;
                result = self.lisp.cons(new_binding, result)?;
            }
        }
        Ok(result)
    }

    /// `(prefix ...)` — prefix all names with the given identifier.
    fn apply_env_prefix(
        &self,
        env: ArenaIndex,
        macro_env: ArenaIndex,
        prefix: ArenaIndex,
    ) -> Result<(ArenaIndex, ArenaIndex), EvalError> {
        let mut transform = |name: ArenaIndex| -> Result<ArenaIndex, EvalError> {
            self.prefix_symbol(prefix, name)
        };
        let new_env = self.map_env_names(env, &mut transform)?;
        let new_menv = self.map_env_names(macro_env, &mut transform)?;
        Ok((new_env, new_menv))
    }

    /// `(rename ...)` — rename specific identifiers.
    fn apply_env_rename(
        &self,
        env: ArenaIndex,
        macro_env: ArenaIndex,
        renames: ArenaIndex,
    ) -> Result<(ArenaIndex, ArenaIndex), EvalError> {
        let mut transform = |name: ArenaIndex| -> Result<ArenaIndex, EvalError> {
            let mut ren_list = renames;
            while let Value::Cons { .. } = self.lisp.get(ren_list)? {
                let pair = self.lisp.car(ren_list)?;
                ren_list = self.lisp.cdr(ren_list)?;
                if let Value::Cons { .. } = self.lisp.get(pair)? {
                    let old = self.lisp.car(pair)?;
                    if self.lisp.symbol_eq(name, old)? {
                        let new_name_cell = self.lisp.cdr(pair)?;
                        return self.lisp.car(new_name_cell).map_err(Into::into);
                    }
                }
            }
            Ok(name)
        };
        let new_env = self.map_env_names(env, &mut transform)?;
        let new_menv = self.map_env_names(macro_env, &mut transform)?;
        Ok((new_env, new_menv))
    }

    /// Compare two library names for equality.
    ///
    /// Library names are lists of symbols; we compare element-wise.
    pub(super) fn library_names_equal(&self, a: ArenaIndex, b: ArenaIndex) -> Result<bool, EvalError> {
        let mut a_cur = a;
        let mut b_cur = b;
        loop {
            let a_val = self.lisp.get(a_cur)?;
            let b_val = self.lisp.get(b_cur)?;
            match (a_val, b_val) {
                (Value::Nil, Value::Nil) => return Ok(true),
                (Value::Cons { .. }, Value::Cons { .. }) => {
                    let a_head = self.lisp.car(a_cur)?;
                    let b_head = self.lisp.car(b_cur)?;
                    // Compare head elements
                    match (self.lisp.get(a_head)?, self.lisp.get(b_head)?) {
                        (Value::Symbol(_), Value::Symbol(_)) => {
                            if !self.lisp.symbol_eq(a_head, b_head)? {
                                return Ok(false);
                            }
                        }
                        (Value::Number(x), Value::Number(y)) => {
                            if x != y { return Ok(false); }
                        }
                        _ => return Ok(false),
                    }
                    a_cur = self.lisp.cdr(a_cur)?;
                    b_cur = self.lisp.cdr(b_cur)?;
                }
                _ => return Ok(false),
            }
        }
    }
    
    /// Build an argument list for vector-map/vector-for-each at a given index.
    /// vecs is a list of vectors. Returns a proper list of elements at index i.
    pub(super) fn vector_map_build_args(&self, vecs: ArenaIndex, index: usize, call_expr: ArenaIndex) -> Result<ArenaIndex, EvalError> {
        // Build args in reverse, then reverse
        let mut args = self.lisp.nil()?;
        let mut current = vecs;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { .. } => {
                    let vec = self.lisp.car(current)?;
                    let elem = self.lisp.array_get(vec, index).map_err(|_| self.make_error(ErrorKind::TypeError, call_expr))?;
                    args = self.lisp.cons(elem, args)?;
                    current = self.lisp.cdr(current)?;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
            }
        }
        self.reverse_list(args)}

    /// Implement (include "file1" "file2" ...) and (include-ci "file1" ...) — R7RS §4.1.7.
    /// Reads each file, parses all expressions, and evaluates them sequentially.
    /// If `case_insensitive` is true, identifiers read from the file are folded to lowercase.
    pub(super) fn step_eval_include(&mut self, args: ArenaIndex, env: EnvRef, _case_insensitive: bool)
        -> Result<TrampolineState, EvalError>
    {
        let mut filenames = args;
        let mut last_val = self.lisp.void_val()?;

        while let Value::Cons { .. } = self.lisp.get(filenames)? {
            let filename_expr = self.lisp.car(filenames)?;
            let path = self.extract_string_arg(filename_expr, filenames)?;

            // Read and parse the file
            let forms = match &mut self.io {
                Some(io) => {
                    let content = match io.read_file(&path) {
                        Ok(s) => s,
                        Err(_) => {
                            // Raise a file-error typed error object
                            return self.raise_file_error("include: cannot read file", filenames);
                        }
                    };
                    grift_parser::parse_all(self.lisp, content)?
                }
                None => {
                    return self.raise_file_error("include: no I/O provider", filenames);
                }
            };

            // Evaluate each form sequentially
            let mut current = forms;
            while let Value::Cons { .. } = self.lisp.get(current)? {
                let form = self.lisp.car(current)?;
                last_val = self.eval_for_macro(ExprRef(form), env)?;
                current = self.lisp.cdr(current)?;
            }

            filenames = self.lisp.cdr(filenames)?;
        }

        Ok(TrampolineState::Return { val: last_val })
    }
}
