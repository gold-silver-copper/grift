//! Builtin function implementations for the evaluator.
//!
//! Contains apply_builtin, apply_binary_builtin, and related helper functions
//! for numeric operations, comparisons, and string/char operations.

use grift_parser::{ArenaIndex, Value, Builtin, fsize};

use crate::error::{ErrorKind, EvalError, EvalResult};
use crate::continuation::{TrampolineState, ContType, is_binary_builtin, EnvRef, ExprRef};
use crate::helpers::{int_pow, equal_recursive, float_floor, float_ceil, float_truncate, float_round, float_sqrt, float_pow};
use crate::{
    extract_args, builtin_unary_pred, builtin_numeric_pred, builtin_rounding_op, builtin_div_op,
    builtin_char_to_int,
    binary_int_cmp, binary_int_op, binary_div_op,
};

use super::Evaluator;

impl<'a, const N: usize> Evaluator<'a, N> {
    pub(super) fn apply_builtin_with_args(&mut self, builtin: Builtin, args_expr: ArenaIndex, env: EnvRef, call_expr: ArenaIndex) 
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
                    // Data: (builtin_encoded . (second_arg . (call_expr . eval_env)))
                    let builtin_encoded = Self::encode_builtin(builtin);
                    self.cont(ContType::BinaryBuiltinFirst, env).data4(builtin_encoded, second_arg_expr, call_expr, env.0)?;
                    return Ok(Some(TrampolineState::Eval { expr: ExprRef(first_arg), env }));
                }
            }
        }
        
        // General case: collect args and apply
        // Data: (builtin_encoded . (remaining_args . (collected . (call_expr . eval_env))))
        let nil = self.lisp.nil()?;
        let builtin_encoded = Self::encode_builtin(builtin);
        self.cont(ContType::BuiltinForceArg, env).data5(builtin_encoded, rest_args, nil, call_expr, env.0)?;
        
        Ok(Some(TrampolineState::Eval { expr: ExprRef(first_arg), env }))
    }
    
    /// Reverse a list (used for BuiltinForceArg fallback path)
    pub(super) fn reverse_list(&self, mut list: ArenaIndex) -> EvalResult {
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
    pub(super) fn apply_builtin_trampolined(&mut self, builtin: Builtin, args: ArenaIndex, call_expr: ArenaIndex) 
        -> Result<TrampolineState, EvalError> 
    {
        match builtin {
            Builtin::Error => return self.apply_error_builtin(args, call_expr),
            Builtin::Load => return self.apply_load_builtin(args, call_expr),
            Builtin::VectorMap => return self.apply_vector_map(args, call_expr),
            Builtin::VectorForEach => return self.apply_vector_for_each(args, call_expr),
            Builtin::CallWithInputFile | Builtin::CallWithOutputFile =>
                return self.apply_call_with_file(builtin, args, call_expr),
            Builtin::CallWithPort => return self.apply_call_with_port(args, call_expr),
            Builtin::WithInputFromFile | Builtin::WithOutputToFile =>
                return self.apply_with_file(builtin, args, call_expr),
            Builtin::Values => {
                if let Value::Cons { .. } = self.lisp.get(args)? {
                    let rest = self.lisp.cdr(args)?;
                    if self.lisp.get(rest)?.is_nil() {
                        let single = self.lisp.car(args)?;
                        return Ok(TrampolineState::Return { val: single });
                    }
                }
                return Ok(TrampolineState::Return { val: args });
            }
            Builtin::Apply => return self.apply_apply_builtin(args, call_expr),
            Builtin::CallWithValues => return self.apply_call_with_values_builtin(args, call_expr),
            Builtin::CallCc | Builtin::CallWithCurrentContinuation =>
                return self.apply_call_cc_builtin(args, call_expr),
            Builtin::DynamicWind => return self.apply_dynamic_wind_builtin(args, call_expr),
            Builtin::WithExceptionHandler =>
                return self.apply_with_exception_handler_builtin(args, call_expr),
            Builtin::RaiseBuiltin => return self.apply_raise_builtin(args, call_expr, false),
            Builtin::RaiseContinuable => return self.apply_raise_builtin(args, call_expr, true),
            Builtin::EvalBuiltin => return self.apply_eval_builtin(args, call_expr),
            Builtin::EnvironmentBuiltin => return self.apply_environment_builtin(args, call_expr),
            _ => {}
        }
        
        // In strict evaluation, args are already evaluated values
        let result = self.apply_builtin(builtin, args, call_expr)?;
        Ok(TrampolineState::Return { val: result })
    }
    
    /// Create an R7RS error object and raise it through the exception handler chain.
    /// (error message obj ...) — R7RS §6.11
    pub(super) fn apply_error_builtin(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        // First arg is the message
        let msg = self.lisp.car(args)?;
        // Rest are irritants
        let irritants = self.lisp.cdr(args)?;
        // Type is Nil for standard (error msg ...) calls
        let nil = self.lisp.nil()?;
        let irritants_and_type = self.lisp.cons(irritants, nil)?;
        let error_obj = self.lisp.alloc(Value::ErrorObject { message: msg, irritants_and_type })?;
        
        // Raise through the exception handler chain
        match self.invoke_exception_handler(error_obj, false)? {
            Some(state) => Ok(state),
            None => {
                // No continuation to return to — shouldn't happen for raise
                Err(self.make_error(ErrorKind::UserError, call_expr))
            }
        }
    }
    
    /// Implement (load filename) — R7RS §6.13.
    /// Reads the file, parses all expressions, and evaluates them sequentially.
    fn apply_load_builtin(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let arg = self.lisp.car(args)?;
        let port = self.string_to_output_port(arg, call_expr)?;

        // Read file content and parse all expressions while holding the io borrow.
        // We must parse before releasing the borrow, since the content &str is tied to it.
        let forms = match &mut self.io {
            Some(io) => {
                let content = match io.read_file_from_string_port(port) {
                    Ok(s) => s,
                    Err(_) => return Err(self.make_error(ErrorKind::FileError, call_expr)),
                };
                let result = grift_parser::parse_all(self.lisp, content);
                io.close_port(port).ok();
                result?
            }
            None => return Err(self.make_error(ErrorKind::FileError, call_expr)),
        };

        // Evaluate each expression sequentially at the top level.
        // Use eval_for_macro to preserve the current continuation.
        let mut current = forms;
        let mut last_val = self.lisp.void_val()?;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let form = self.lisp.car(current)?;
            last_val = self.eval_for_macro(ExprRef(form), self.global_env)?;
            current = self.lisp.cdr(current)?;
        }

        Ok(TrampolineState::Return { val: last_val })
    }
    
    /// Apply vector-map: (vector-map proc vec1 vec2 ...)
    /// Uses VectorMapStep continuation for trampolined iteration.
    fn apply_vector_map(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let (proc, vecs_args, len) = self.validate_vector_args(args, call_expr)?;
        
        if len == 0 {
            let placeholder = self.lisp.number(0)?;
            let result = self.lisp.make_array(0, placeholder)?;
            return Ok(TrampolineState::Return { val: result });
        }
        
        let first_args = self.vector_map_build_args(vecs_args, 0, call_expr)?;
        let nil = self.lisp.nil()?;
        let index_enc = self.lisp.number(0)?;
        let len_enc = self.lisp.number(len as isize)?;
        let env = self.global_env.0;
        
        let d4 = self.lisp.cons(nil, call_expr)?;
        let d3 = self.lisp.cons(len_enc, d4)?;
        let d2 = self.lisp.cons(index_enc, d3)?;
        let d1 = self.lisp.cons(vecs_args, d2)?;
        let packed = self.lisp.cons(proc, d1)?;
        self.cont(ContType::VectorMapStep, EnvRef(env)).data1(packed)?;
        
        self.cont(ContType::ApplyDirect, EnvRef(env)).data3(first_args, env, call_expr)?;
        Ok(TrampolineState::Return { val: proc })
    }
    
    /// Apply vector-for-each: (vector-for-each proc vec1 vec2 ...)
    /// Uses VectorForEachStep continuation for trampolined iteration.
    fn apply_vector_for_each(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let (proc, vecs_args, len) = self.validate_vector_args(args, call_expr)?;
        
        if len == 0 {
            let void = self.lisp.void_val()?;
            return Ok(TrampolineState::Return { val: void });
        }
        
        let first_args = self.vector_map_build_args(vecs_args, 0, call_expr)?;
        let index_enc = self.lisp.number(0)?;
        let len_enc = self.lisp.number(len as isize)?;
        let env = self.global_env.0;
        
        let d3 = self.lisp.cons(len_enc, call_expr)?;
        let d2 = self.lisp.cons(index_enc, d3)?;
        let d1 = self.lisp.cons(vecs_args, d2)?;
        let packed = self.lisp.cons(proc, d1)?;
        self.cont(ContType::VectorForEachStep, EnvRef(env)).data1(packed)?;
        
        self.cont(ContType::ApplyDirect, EnvRef(env)).data3(first_args, env, call_expr)?;
        Ok(TrampolineState::Return { val: proc })
    }

    /// Implement (call-with-input-file string proc) and (call-with-output-file string proc).
    /// Opens the file, pushes a CallWithPortClose continuation, and applies proc to the port.
    fn apply_call_with_file(&mut self, builtin: Builtin, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let filename_val = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let proc = self.lisp.car(rest)?;
        let mode = if matches!(builtin, Builtin::CallWithInputFile) {
            grift_parser::FileOpenMode::TextInput
        } else {
            grift_parser::FileOpenMode::TextOutput
        };
        let str_port = self.string_to_output_port(filename_val, call_expr)?;

        let pid = match &mut self.io {
            Some(io) => {
                let result = io.open_file_from_string_port(str_port, mode);
                io.close_port(str_port).ok();
                result.map_err(|_| self.make_error(ErrorKind::FileError, call_expr))?
            }
            None => return Err(self.make_error(ErrorKind::FileError, call_expr)),
        };

        let port_val = self.lisp.port(pid)?;
        let port_id_enc = self.lisp.number(pid.0 as isize)?;
        let env = self.global_env.0;

        // Push continuation to close the port after proc returns
        self.cont(ContType::CallWithPortClose, EnvRef(env)).data1(port_id_enc)?;

        // Build args list: (port)
        let nil = self.lisp.nil()?;
        let args_list = self.lisp.cons(port_val, nil)?;

        // Apply proc to the port
        self.cont(ContType::ApplyDirect, EnvRef(env)).data3(args_list, env, call_expr)?;
        Ok(TrampolineState::Return { val: proc })
    }

    /// Implement (call-with-port port proc).
    /// Calls proc with port as argument, closes port when proc returns.
    fn apply_call_with_port(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let port_val = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let proc = self.lisp.car(rest)?;

        // Validate that the first arg is a port
        let pid = match self.lisp.get(port_val)? {
            Value::Port(pid) => pid,
            v => return Err(self.type_error(call_expr, "port", v.type_name())),
        };

        let port_id_enc = self.lisp.number(pid.0 as isize)?;
        let env = self.global_env.0;

        // Push continuation to close the port after proc returns
        self.cont(ContType::CallWithPortClose, EnvRef(env)).data1(port_id_enc)?;

        // Build args list: (port)
        let nil = self.lisp.nil()?;
        let args_list = self.lisp.cons(port_val, nil)?;

        // Apply proc to the port
        self.cont(ContType::ApplyDirect, EnvRef(env)).data3(args_list, env, call_expr)?;
        Ok(TrampolineState::Return { val: proc })
    }

    /// Implement (with-input-from-file string thunk) and (with-output-to-file string thunk).
    /// Opens the file, redirects the current port, calls the thunk, restores and closes.
    fn apply_with_file(&mut self, builtin: Builtin, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let filename_val = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let thunk = self.lisp.car(rest)?;
        let is_input = matches!(builtin, Builtin::WithInputFromFile);
        let mode = if is_input {
            grift_parser::FileOpenMode::TextInput
        } else {
            grift_parser::FileOpenMode::TextOutput
        };
        let str_port = self.string_to_output_port(filename_val, call_expr)?;

        let pid = match &mut self.io {
            Some(io) => {
                let result = io.open_file_from_string_port(str_port, mode);
                io.close_port(str_port).ok();
                result.map_err(|_| self.make_error(ErrorKind::FileError, call_expr))?
            }
            None => return Err(self.make_error(ErrorKind::FileError, call_expr)),
        };

        let saved_port = if is_input {
            self.current_input_port
        } else {
            self.current_output_port
        };

        // Redirect current port
        if is_input {
            self.current_input_port = pid;
        } else {
            self.current_output_port = pid;
        }

        let saved_enc = self.lisp.number(saved_port.0 as isize)?;
        let file_enc = self.lisp.number(pid.0 as isize)?;
        let env = self.global_env.0;

        // Push restore continuation
        let cont_type = if is_input {
            ContType::WithInputFromFileRestore
        } else {
            ContType::WithOutputToFileRestore
        };
        self.cont(cont_type, EnvRef(env)).data2(saved_enc, file_enc)?;

        // Call thunk with no args
        let nil = self.lisp.nil()?;
        self.cont(ContType::ApplyForced, EnvRef(env)).data3(nil, env, call_expr)?;
        Ok(TrampolineState::Return { val: thunk })
    }

    /// (apply proc arg ... args-list) — R7RS §6.4
    /// Apply procedure to arguments, with last arg being a list that gets spliced.
    fn apply_apply_builtin(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let func = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        // Collect fixed args and the trailing list
        // (apply f a b '(c d)) => apply f to (a b c d)
        let args_list = self.build_apply_args(rest, call_expr)?;
        let new_call = self.lisp.cons(func, args_list)?;
        self.push_frame(new_call, func)?;
        let env = self.global_env.0;
        self.cont(ContType::ApplyDirect, EnvRef(env)).data3(args_list, env, new_call)?;
        Ok(TrampolineState::Return { val: func })
    }

    /// Build the argument list for (apply proc arg ... args-list).
    /// The last element must be a list; preceding elements are prepended.
    fn build_apply_args(&self, rest: ArenaIndex, call_expr: ArenaIndex) -> EvalResult {
        match self.lisp.get(rest)? {
            Value::Nil => {
                // No args at all — error
                Err(self.make_error(ErrorKind::WrongArgCount, call_expr)
                    .with_message("apply requires at least 2 arguments"))
            }
            Value::Cons { .. } => {
                let head = self.lisp.car(rest)?;
                let tail = self.lisp.cdr(rest)?;
                if self.lisp.get(tail)?.is_nil() {
                    // Last element — must be a list; use it directly
                    Ok(head)
                } else {
                    // More elements follow — prepend this one
                    let rest_args = self.build_apply_args(tail, call_expr)?;
                    self.lisp.cons(head, rest_args).map_err(Into::into)
                }
            }
            _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
        }
    }

    /// (call-with-values producer consumer) — R7RS §6.10
    fn apply_call_with_values_builtin(&mut self, args: ArenaIndex, _call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let producer = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let consumer = self.lisp.car(rest)?;

        // Call producer with no args, then apply consumer to results.
        // Use CallWithValuesConsumer to hold the consumer until producer returns.
        // CallWithValuesConsumer will Eval the consumer value (self-evaluating for
        // lambdas/builtins), then CallWithValuesApply will apply it to producer results.
        let nil = self.lisp.nil()?;
        let producer_call = self.lisp.cons(producer, nil)?;
        let env = self.global_env.0;

        self.cont(ContType::CallWithValuesConsumer, EnvRef(env)).data2(consumer, env)?;

        // Apply producer with no args
        self.push_frame(producer_call, producer)?;
        self.cont(ContType::ApplyForced, EnvRef(env)).data3(nil, env, producer_call)?;
        Ok(TrampolineState::Return { val: producer })
    }

    /// (call/cc proc) or (call-with-current-continuation proc) — R7RS §6.10
    fn apply_call_cc_builtin(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let proc = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        if !self.lisp.get(rest)?.is_nil() {
            return Err(self.make_error(ErrorKind::WrongArgCount, call_expr)
                .with_message("call/cc requires exactly 1 argument"));
        }

        // Capture the current continuation
        let env = self.global_env.0;
        let captured_continuation = self.capture_continuation(env)?;

        // Apply proc to the captured continuation
        let nil = self.lisp.nil()?;
        let cont_args = self.lisp.cons(captured_continuation, nil)?;
        let call = self.lisp.cons(proc, cont_args)?;
        self.push_frame(call, proc)?;
        self.cont(ContType::ApplyDirect, EnvRef(env)).data3(cont_args, env, call)?;
        Ok(TrampolineState::Return { val: proc })
    }

    /// (dynamic-wind before body after) — R7RS §6.10
    fn apply_dynamic_wind_builtin(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let before = self.lisp.car(args)?;
        let rest1 = self.lisp.cdr(args)?;
        let body = self.lisp.car(rest1)?;
        let rest2 = self.lisp.cdr(rest1)?;
        let after = self.lisp.car(rest2)?;
        let rest3 = self.lisp.cdr(rest2)?;
        if !self.lisp.get(rest3)?.is_nil() {
            return Err(self.make_error(ErrorKind::WrongArgCount, call_expr)
                .with_message("dynamic-wind requires exactly 3 arguments"));
        }

        let saved_dw_chain = self.dynamic_wind_chain;
        let env = self.global_env.0;

        // Set up the continuation chain:
        // 1. Call before thunk
        // 2. When before returns → DynamicWindCallBody fires
        //    (expects val=body_thunk, but we need to feed body_thunk)
        //
        // Use DynamicWindCallBody which expects val = body thunk.
        // We chain: call before → ignore result → return body → DynamicWindCallBody
        //
        // Push DynamicWindCallBody (will receive body as val via BuiltinReturnValue)
        self.cont(ContType::DynamicWindCallBody, EnvRef(env))
            .data4(before, after, env, saved_dw_chain)?;

        // Push a continuation that discards before's return value and returns body
        self.cont(ContType::BuiltinReturnValue, EnvRef(env)).data1(body)?;

        // Call the before thunk
        match self.apply_thunk(before, EnvRef(env))? {
            Some(state) => Ok(state),
            None => Err(self.make_error(ErrorKind::NotAFunction, call_expr)
                .with_message("dynamic-wind: before must be a thunk")),
        }
    }

    /// (with-exception-handler handler thunk) — R7RS §6.11
    fn apply_with_exception_handler_builtin(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let handler = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let thunk = self.lisp.car(rest)?;

        // Both handler and thunk are already evaluated values.
        // Install handler and call thunk (same logic as WithExceptionHandlerCallThunk)
        let saved_chain = self.exception_handler_chain;
        self.exception_handler_chain = self.lisp.cons(handler, saved_chain)?;

        let true_val = self.lisp.true_val()?;
        let nil = self.lisp.nil()?;
        let global = self.global_env;
        self.cont(ContType::ExceptionHandlerFrame, global)
            .data4(nil, saved_chain, true_val, nil)?;

        match self.apply_thunk(thunk, self.global_env)? {
            Some(state) => Ok(state),
            None => Err(self.make_error(ErrorKind::NotAFunction, call_expr)
                .with_message("with-exception-handler: thunk must be a procedure")),
        }
    }

    /// (raise obj) or (raise-continuable obj) — R7RS §6.11
    fn apply_raise_builtin(&mut self, args: ArenaIndex, call_expr: ArenaIndex, continuable: bool)
        -> Result<TrampolineState, EvalError>
    {
        let obj = self.lisp.car(args)?;
        match self.invoke_exception_handler(obj, continuable)? {
            Some(state) => Ok(state),
            None => Err(self.make_error(ErrorKind::UserError, call_expr)),
        }
    }

    /// (eval expr) or (eval expr env) — R7RS §6.12
    /// Arguments are already evaluated when called as a builtin.
    fn apply_eval_builtin(&mut self, args: ArenaIndex, _call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let expr = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;

        if self.lisp.get(rest)?.is_nil() {
            // (eval expr) — evaluate expr in global env
            Ok(TrampolineState::Eval { expr: ExprRef(expr), env: self.global_env })
        } else {
            // (eval expr env-val) — evaluate expr in given environment
            let env_val = self.lisp.car(rest)?;
            match self.lisp.get(env_val)? {
                Value::Environment { env: target_env, .. } => {
                    Ok(TrampolineState::Eval { expr: ExprRef(expr), env: EnvRef(target_env) })
                }
                _ => Err(self.type_error(env_val, "environment", self.lisp.get(env_val)?.type_name())),
            }
        }
    }

    /// (environment import-set ...) — R7RS §6.12
    fn apply_environment_builtin(&mut self, args: ArenaIndex, _call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let nil = self.lisp.nil()?;
        let mut result_env = EnvRef(nil);

        let mut sets = args;
        while let Value::Cons { .. } = self.lisp.get(sets)? {
            let import_set = self.lisp.car(sets)?;
            sets = self.lisp.cdr(sets)?;
            result_env = self.import_library_into_env(import_set, result_env)?;
        }

        let env_val = self.lisp.alloc(Value::Environment { env: result_env.0, mutable: false })?;
        Ok(TrampolineState::Return { val: env_val })
    }
    
    /// Apply a builtin with already-evaluated arguments
    pub(super) fn apply_builtin(&mut self, builtin: Builtin, args: ArenaIndex, call_expr: ArenaIndex) 
        -> EvalResult 
    {
        match builtin {
            Builtin::Car | Builtin::Cdr => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Cons { .. } => {
                        if matches!(builtin, Builtin::Car) {
                            self.lisp.car(arg).map_err(Into::into)
                        } else {
                            self.lisp.cdr(arg).map_err(Into::into)
                        }
                    }
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
            // eq? and eqv? have identical semantics in this integer-only implementation
            Builtin::EqP | Builtin::EqvP => {
                extract_args!(self, args, a, b);
                self.eqv_compare(a, b)
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
            
            Builtin::Add => {
                // Handle zero-argument case (+) => 0
                if self.lisp.get(args)?.is_nil() {
                    return self.lisp.number(0).map_err(Into::into);
                }
                // Check if first argument is a BigNum - if so, use BigNum fold
                let first_idx = self.lisp.car(args)?;
                match self.lisp.get(first_idx)? {
                    Value::BigNum { .. } => {
                        // BigNum start: fold with addition
                        let (limbs, len, neg) = self.lisp.bignum_limbs(first_idx)?;
                        let mut acc = crate::bignum::BigNumBuf::zero();
                        acc.limbs[..len].copy_from_slice(&limbs[..len]);
                        acc.len = len; acc.negative = neg;
                        let mut current = self.lisp.cdr(args)?;
                        while let Value::Cons { .. } = self.lisp.get(current)? {
                            let elem = self.lisp.car(current)?;
                            match self.lisp.get(elem)? {
                                Value::Number(n) => {
                                    let b = crate::bignum::BigNumBuf::from_isize(n);
                                    acc = acc.add(&b);
                                }
                                Value::BigNum { .. } => {
                                    let (bl, blen, bneg) = self.lisp.bignum_limbs(elem)?;
                                    let mut bb = crate::bignum::BigNumBuf::zero();
                                    bb.limbs[..blen].copy_from_slice(&bl[..blen]);
                                    bb.len = blen; bb.negative = bneg;
                                    acc = acc.add(&bb);
                                }
                                Value::Float(f) => {
                                    let result = acc.to_f64() as fsize + f;
                                    return self.lisp.float(result).map_err(Into::into);
                                }
                                v => return Err(self.type_error(call_expr, "number", v.type_name())),
                            }
                            current = self.lisp.cdr(current)?;
                        }
                        match acc.to_isize() {
                            Some(n) => self.lisp.number(n).map_err(Into::into),
                            None => self.lisp.bignum_from_limbs(&acc.limbs[..acc.len], acc.negative).map_err(Into::into),
                        }
                    }
                    _ => {
                        self.numeric_fold(args, 0, 
                            |a, b| a.checked_add(b), 
                            |a, b| a + b,
                            call_expr)
                    }
                }
            }
            
            Builtin::Sub => {
                let first_idx = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                if self.lisp.get(rest)?.is_nil() {
                    // Unary minus
                    match self.lisp.get(first_idx)? {
                        Value::Number(n) => {
                            match n.checked_neg() {
                                Some(neg) => self.lisp.number(neg).map_err(Into::into),
                                None => {
                                    // isize::MIN negation overflows
                                    let buf = crate::bignum::BigNumBuf::from_isize(n).negate();
                                    self.lisp.bignum_from_limbs(&buf.limbs[..buf.len], buf.negative).map_err(Into::into)
                                }
                            }
                        }
                        Value::Float(f) => self.lisp.float(-f).map_err(Into::into),
                        Value::Rational { num, denom } => self.lisp.rational(-num, denom).map_err(Into::into),
                        Value::Complex { real, imag } => self.lisp.complex(-real, -imag).map_err(Into::into),
                        Value::BigNum { .. } => {
                            let (limbs, len, neg) = self.lisp.bignum_limbs(first_idx)?;
                            let mut buf = crate::bignum::BigNumBuf::zero();
                            buf.limbs[..len].copy_from_slice(&limbs[..len]);
                            buf.len = len; buf.negative = neg;
                            let result = buf.negate();
                            match result.to_isize() {
                                Some(n) => self.lisp.number(n).map_err(Into::into),
                                None => self.lisp.bignum_from_limbs(&result.limbs[..result.len], result.negative).map_err(Into::into),
                            }
                        }
                        v => Err(self.type_error(call_expr, "number", v.type_name())),
                    }
                } else {
                    match self.lisp.get(first_idx)? {
                        Value::Number(first) => {
                            self.numeric_fold(rest, first, 
                                |a, b| a.checked_sub(b),
                                |a, b| a - b,
                                call_expr)
                        }
                        Value::Float(first) => {
                            self.numeric_fold_float(rest, first, |a, b| a - b, call_expr)
                        }
                        Value::Rational { num, denom } => {
                            self.rational_fold_sub(rest, num, denom, call_expr)
                        }
                        Value::BigNum { .. } => {
                            // BigNum start: fold with subtraction
                            let (limbs, len, neg) = self.lisp.bignum_limbs(first_idx)?;
                            let mut acc = crate::bignum::BigNumBuf::zero();
                            acc.limbs[..len].copy_from_slice(&limbs[..len]);
                            acc.len = len; acc.negative = neg;
                            let mut current = rest;
                            while let Value::Cons { .. } = self.lisp.get(current)? {
                                let elem = self.lisp.car(current)?;
                                match self.lisp.get(elem)? {
                                    Value::Number(n) => {
                                        let b = crate::bignum::BigNumBuf::from_isize(n);
                                        acc = acc.sub(&b);
                                    }
                                    Value::BigNum { .. } => {
                                        let (bl, blen, bneg) = self.lisp.bignum_limbs(elem)?;
                                        let mut bb = crate::bignum::BigNumBuf::zero();
                                        bb.limbs[..blen].copy_from_slice(&bl[..blen]);
                                        bb.len = blen; bb.negative = bneg;
                                        acc = acc.sub(&bb);
                                    }
                                    Value::Float(f) => {
                                        let result = acc.to_f64() as fsize - f;
                                        return self.lisp.float(result).map_err(Into::into);
                                    }
                                    v => return Err(self.type_error(call_expr, "number", v.type_name())),
                                }
                                current = self.lisp.cdr(current)?;
                            }
                            match acc.to_isize() {
                                Some(n) => self.lisp.number(n).map_err(Into::into),
                                None => self.lisp.bignum_from_limbs(&acc.limbs[..acc.len], acc.negative).map_err(Into::into),
                            }
                        }
                        v => Err(self.type_error(call_expr, "number", v.type_name())),
                    }
                }
            }
            
            Builtin::Mul => {
                // Handle zero-argument case (*) => 1
                if self.lisp.get(args)?.is_nil() {
                    return self.lisp.number(1).map_err(Into::into);
                }
                // Check if first argument is a BigNum
                let first_idx = self.lisp.car(args)?;
                match self.lisp.get(first_idx)? {
                    Value::BigNum { .. } => {
                        // BigNum start: fold with multiplication
                        let (limbs, len, neg) = self.lisp.bignum_limbs(first_idx)?;
                        let mut acc = crate::bignum::BigNumBuf::zero();
                        acc.limbs[..len].copy_from_slice(&limbs[..len]);
                        acc.len = len; acc.negative = neg;
                        let mut current = self.lisp.cdr(args)?;
                        while let Value::Cons { .. } = self.lisp.get(current)? {
                            let elem = self.lisp.car(current)?;
                            match self.lisp.get(elem)? {
                                Value::Number(n) => {
                                    let b = crate::bignum::BigNumBuf::from_isize(n);
                                    acc = acc.mul(&b);
                                }
                                Value::BigNum { .. } => {
                                    let (bl, blen, bneg) = self.lisp.bignum_limbs(elem)?;
                                    let mut bb = crate::bignum::BigNumBuf::zero();
                                    bb.limbs[..blen].copy_from_slice(&bl[..blen]);
                                    bb.len = blen; bb.negative = bneg;
                                    acc = acc.mul(&bb);
                                }
                                Value::Float(f) => {
                                    let result = acc.to_f64() as fsize * f;
                                    return self.lisp.float(result).map_err(Into::into);
                                }
                                v => return Err(self.type_error(call_expr, "number", v.type_name())),
                            }
                            current = self.lisp.cdr(current)?;
                        }
                        match acc.to_isize() {
                            Some(n) => self.lisp.number(n).map_err(Into::into),
                            None => self.lisp.bignum_from_limbs(&acc.limbs[..acc.len], acc.negative).map_err(Into::into),
                        }
                    }
                    _ => self.numeric_fold_mul(args, call_expr),
                }
            }
            
            Builtin::Div => {
                // Division: (/ n) => 1/n, (/ n m ...) => n/m/...
                let first_idx = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                if self.lisp.get(rest)?.is_nil() {
                    // Unary reciprocal: (/ n) => 1/n
                    match self.lisp.get(first_idx)? {
                        Value::Number(n) => {
                            if n == 0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                            self.lisp.rational(1, n).map_err(Into::into)
                        }
                        Value::Float(f) => self.lisp.float(1.0 / f).map_err(Into::into),
                        Value::Rational { num, denom } => {
                            if num == 0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                            self.lisp.rational(denom, num).map_err(Into::into)
                        }
                        Value::Complex { real, imag } => {
                            // 1/(a+bi) = (a-bi)/(a²+b²)
                            let denom = real * real + imag * imag;
                            if denom == 0.0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                            self.lisp.complex(real / denom, -imag / denom).map_err(Into::into)
                        }
                        Value::BigNum { .. } => {
                            // 1/bignum — approximate as float
                            let f = self.bignum_to_f64(first_idx)?;
                            if f == 0.0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                            self.lisp.float(1.0 / f).map_err(Into::into)
                        }
                        v => Err(self.type_error(call_expr, "number", v.type_name())),
                    }
                } else {
                    match self.lisp.get(first_idx)? {
                        Value::Number(first) => {
                            self.rational_fold_div(rest, first, 1, call_expr)
                        }
                        Value::Float(first) => {
                            self.numeric_fold_float(rest, first, |a, b| a / b, call_expr)
                        }
                        Value::Rational { num, denom } => {
                            self.rational_fold_div(rest, num, denom, call_expr)
                        }
                        Value::BigNum { .. } => {
                            self.bignum_fold_div(first_idx, rest, call_expr)
                        }
                        v => Err(self.type_error(call_expr, "number", v.type_name())),
                    }
                }
            }
            
            // Division operations with zero check - generated by builtin_div_op! macro
            // Scheme modulo: result has the sign of the divisor
            Builtin::Modulo => builtin_div_op!(self, args, call_expr, |a, b| ((a % b) + b) % b),
            // Scheme remainder: result has the sign of the dividend
            Builtin::Remainder => builtin_div_op!(self, args, call_expr, |a, b| a % b),
            // Integer quotient (truncated towards zero)
            Builtin::Quotient => builtin_div_op!(self, args, call_expr, |a, b| a / b),
            
            Builtin::Expt => {
                // Exponentiation: (expt base power)
                let base_idx = self.lisp.car(args)?;
                let power_idx = self.lisp.car(self.lisp.cdr(args)?)?;
                let base_val = self.lisp.get(base_idx)?;
                let power_val = self.lisp.get(power_idx)?;
                
                match (base_val, power_val) {
                    (Value::Number(base), Value::Number(power)) => {
                        if power < 0 {
                            // Negative exponent produces float result
                            if base == 0 {
                                return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                            }
                            let fb = base as fsize;
                            let fp = power as fsize;
                            self.lisp.float(float_pow(fb, fp)).map_err(Into::into)
                        } else {
                            // Use BigNum exponentiation to avoid overflow
                            let result = bignum_pow(base, power as usize);
                            match result.to_isize() {
                                Some(n) => self.lisp.number(n).map_err(Into::into),
                                None => self.lisp.bignum_from_limbs(&result.limbs[..result.len], result.negative).map_err(Into::into),
                            }
                        }
                    }
                    (Value::BigNum { .. }, Value::Number(power)) => {
                        if power < 0 {
                            let (limbs, len, neg) = self.lisp.bignum_limbs(base_idx)?;
                            let mut buf = crate::bignum::BigNumBuf::zero();
                            buf.limbs[..len].copy_from_slice(&limbs[..len]);
                            buf.len = len;
                            buf.negative = neg;
                            let fb = buf.to_f64() as fsize;
                            let fp = power as fsize;
                            self.lisp.float(float_pow(fb, fp)).map_err(Into::into)
                        } else {
                            let (limbs, len, neg) = self.lisp.bignum_limbs(base_idx)?;
                            let mut base_big = crate::bignum::BigNumBuf::zero();
                            base_big.limbs[..len].copy_from_slice(&limbs[..len]);
                            base_big.len = len;
                            base_big.negative = neg;
                            let mut result = crate::bignum::BigNumBuf::from_isize(1);
                            let mut exp = power as usize;
                            let mut b = base_big;
                            while exp > 0 {
                                if exp % 2 == 1 {
                                    result = result.mul(&b);
                                }
                                exp /= 2;
                                if exp > 0 {
                                    b = b.mul(&b);
                                }
                            }
                            match result.to_isize() {
                                Some(n) => self.lisp.number(n).map_err(Into::into),
                                None => self.lisp.bignum_from_limbs(&result.limbs[..result.len], result.negative).map_err(Into::into),
                            }
                        }
                    }
                    _ => {
                        let base_f = match base_val {
                            Value::Number(n) => n as fsize,
                            Value::Float(f) => f,
                            Value::BigNum { .. } => {
                                let (limbs, len, neg) = self.lisp.bignum_limbs(base_idx)?;
                                let mut buf = crate::bignum::BigNumBuf::zero();
                                buf.limbs[..len].copy_from_slice(&limbs[..len]);
                                buf.len = len;
                                buf.negative = neg;
                                buf.to_f64() as fsize
                            }
                            v => return Err(self.type_error(call_expr, "number", v.type_name())),
                        };
                        let power_f = match power_val {
                            Value::Number(n) => n as fsize,
                            Value::Float(f) => f,
                            v => return Err(self.type_error(call_expr, "number", v.type_name())),
                        };
                        self.lisp.float(float_pow(base_f, power_f)).map_err(Into::into)
                    }
                }
            }
            
            Builtin::Sqrt => {
                // Square root - R7RS: returns complex for negative numbers
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(n) => {
                        if n < 0 {
                            // sqrt of negative integer → complex
                            let mag = float_sqrt((-n) as fsize);
                            return self.lisp.complex(0.0, mag).map_err(Into::into);
                        }
                        // Check for perfect square
                        let root = float_sqrt(n as fsize);
                        let iroot = root as isize;
                        if iroot * iroot == n {
                            self.lisp.number(iroot).map_err(Into::into)
                        } else {
                            self.lisp.float(root).map_err(Into::into)
                        }
                    }
                    Value::Float(f) => {
                        if f < 0.0 {
                            let mag = float_sqrt(-f);
                            self.lisp.complex(0.0, mag).map_err(Into::into)
                        } else {
                            self.lisp.float(float_sqrt(f)).map_err(Into::into)
                        }
                    }
                    Value::Complex { real, imag } => {
                        // sqrt of complex: use formula sqrt(r) * (cos(θ/2) + i*sin(θ/2))
                        let r = libm::sqrt((real as f64) * (real as f64) + (imag as f64) * (imag as f64));
                        // For the principal square root, use non-negative zero for imag
                        // to ensure atan2 returns π (not -π) for negative reals with -0.0 imag.
                        // This gives the conventional principal branch where im(sqrt(z)) >= 0
                        // when re(sqrt(z)) ≈ 0.
                        let imag_for_atan = if imag == 0.0 { 0.0_f64 } else { imag as f64 };
                        let theta = libm::atan2(imag_for_atan, real as f64);
                        let sqrt_r = libm::sqrt(r);
                        let half_theta = theta / 2.0;
                        let re = sqrt_r * libm::cos(half_theta);
                        let im = sqrt_r * libm::sin(half_theta);
                        // Clean up near-zero results from floating point imprecision
                        let re = if libm::fabs(re) < 1e-15 { 0.0 } else { re };
                        let im = if libm::fabs(im) < 1e-15 { 0.0 } else { im };
                        if im == 0.0 {
                            self.lisp.float(re as fsize).map_err(Into::into)
                        } else {
                            self.lisp.complex(re as fsize, im as fsize).map_err(Into::into)
                        }
                    }
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            // integer? is true for exact integers, floats that are whole numbers,
            // and complex with zero imaginary and integer real (R7RS §6.2.5)
            Builtin::Integerp => {
                builtin_unary_pred!(self, args, |v: Value| match v {
                    Value::Number(_) | Value::BigNum { .. } => true,
                    Value::Float(f) => f.is_finite() && f == (f as isize as fsize),
                    Value::Complex { real, imag } => imag == 0.0 && real.is_finite() && real == (real as isize as fsize),
                    _ => false,
                })
            }
            
            // exact? is true for integers and rationals (exact numbers)
            Builtin::Exactp => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(_) | Value::BigNum { .. } | Value::Rational { .. } => self.lisp.true_val().map_err(Into::into),
                    Value::Float(_) | Value::Complex { .. } => self.lisp.false_val().map_err(Into::into),
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::Inexactp => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(_) | Value::BigNum { .. } | Value::Rational { .. } => self.lisp.false_val().map_err(Into::into),
                    Value::Float(_) | Value::Complex { .. } => self.lisp.true_val().map_err(Into::into),
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::Finitep => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(_) | Value::Rational { .. } | Value::BigNum { .. } => self.lisp.true_val().map_err(Into::into),
                    Value::Float(f) => self.lisp.boolean(f.is_finite()).map_err(Into::into),
                    Value::Complex { real, imag } => self.lisp.boolean(real.is_finite() && imag.is_finite()).map_err(Into::into),
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::Infinitep => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(_) | Value::Rational { .. } | Value::BigNum { .. } => self.lisp.false_val().map_err(Into::into),
                    Value::Float(f) => self.lisp.boolean(f.is_infinite()).map_err(Into::into),
                    Value::Complex { real, imag } => self.lisp.boolean(real.is_infinite() || imag.is_infinite()).map_err(Into::into),
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::Nanp => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(_) | Value::Rational { .. } => self.lisp.false_val().map_err(Into::into),
                    Value::Float(f) => self.lisp.boolean(f.is_nan()).map_err(Into::into),
                    Value::Complex { real, imag } => self.lisp.boolean(real.is_nan() || imag.is_nan()).map_err(Into::into),
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            // Rounding operations - identity for integers, actual rounding for floats
            Builtin::Floor => builtin_rounding_op!(self, args, call_expr, float_floor),
            Builtin::Ceiling => builtin_rounding_op!(self, args, call_expr, float_ceil),
            Builtin::Truncate => builtin_rounding_op!(self, args, call_expr, float_truncate),
            Builtin::Round => builtin_rounding_op!(self, args, call_expr, float_round),
            
            // Exactness conversion (R7RS §6.2.6)
            Builtin::ExactToInexact | Builtin::Inexact => {
                let val = self.lisp.car(args)?;
                match self.lisp.get(val)? {
                    Value::Number(n) => self.lisp.float(n as fsize).map_err(Into::into),
                    Value::Float(f) => self.lisp.float(f).map_err(Into::into),
                    Value::Rational { num, denom } => self.lisp.float(num as fsize / denom as fsize).map_err(Into::into),
                    Value::Complex { real, imag } => self.lisp.complex(real, imag).map_err(Into::into),
                    Value::BigNum { .. } => {
                        let (limbs, len, neg) = self.lisp.bignum_limbs(val)?;
                        let mut buf = crate::bignum::BigNumBuf::zero();
                        buf.limbs[..len].copy_from_slice(&limbs[..len]);
                        buf.len = len; buf.negative = neg;
                        self.lisp.float(buf.to_f64() as fsize).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::InexactToExact | Builtin::Exact => {
                let val = self.lisp.car(args)?;
                match self.lisp.get(val)? {
                    Value::Number(n) => self.lisp.number(n).map_err(Into::into),
                    Value::Float(f) => {
                        if !f.is_finite() {
                            return Err(self.type_error(call_expr, "finite number", "infinite or nan"));
                        }
                        // Check if it's an integer
                        let trunc = libm::trunc(f as f64) as fsize;
                        if f == trunc && libm::fabs(f as f64) <= isize::MAX as f64 {
                            return self.lisp.number(f as isize).map_err(Into::into);
                        }
                        // Convert float to exact rational using continued fraction
                        let (num_f, denom_f) = float_to_rational(f as f64);
                        self.lisp.rational(num_f as isize, denom_f as isize).map_err(Into::into)
                    }
                    Value::Rational { num, denom } => self.lisp.rational(num, denom).map_err(Into::into),
                    Value::BigNum { .. } => Ok(val), // already exact
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::Lt => self.compare_numbers(args, |a, b| a < b, call_expr),
            Builtin::Gt => self.compare_numbers(args, |a, b| a > b, call_expr),
            Builtin::Le => self.compare_numbers(args, |a, b| a <= b, call_expr),
            Builtin::Ge => self.compare_numbers(args, |a, b| a >= b, call_expr),
            Builtin::NumEq => self.compare_numbers_eq(args, call_expr),
            
            Builtin::Display => {
                let val = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let has_port = !self.lisp.get(rest)?.is_nil();
                if has_port {
                    // (display obj port) - write to specific port via I/O provider
                    // Use display mode (no quotes around strings, chars as-is)
                    let pid = self.extract_output_port(rest, call_expr)?;
                    if let Some(ref mut io) = self.io {
                        let dv = grift_parser::DisplayValue::new_display(val, self.lisp);
                        let mut writer = IoPortWriter { io: &mut **io, port: pid, error: false };
                        use core::fmt::Write;
                        let _ = write!(writer, "{}", dv);
                    }
                } else if let Some(callback) = self.output_callback {
                    // (display obj) with callback - use callback for output capture
                    callback(self.lisp, val);
                } else if let Some(ref mut io) = self.io {
                    // (display obj) without callback - write to current output port
                    // Use display mode (no quotes around strings, chars as-is)
                    let pid = self.current_output_port;
                    let dv = grift_parser::DisplayValue::new_display(val, self.lisp);
                    let mut writer = IoPortWriter { io: &mut **io, port: pid, error: false };
                    use core::fmt::Write;
                    let _ = write!(writer, "{}", dv);
                }
                // Return void (unspecified value) per R7RS
                self.lisp.void_val().map_err(Into::into)
            }
            
            Builtin::Newline => {
                let rest = args;
                let has_port = !self.lisp.get(rest)?.is_nil();
                if has_port {
                    // (newline port) - write to specific port via I/O provider
                    let pid = self.extract_output_port(rest, call_expr)?;
                    if let Some(ref mut io) = self.io {
                        let _ = io.write_char(pid, '\n');
                    }
                } else if let Some(callback) = self.output_callback {
                    // (newline) with callback - use callback
                    callback(self.lisp, self.lisp.nil()?);
                } else if let Some(ref mut io) = self.io {
                    // (newline) without callback - write to current output port
                    let pid = self.current_output_port;
                    let _ = io.write_char(pid, '\n');
                }
                // Return void (unspecified value) per R7RS
                self.lisp.void_val().map_err(Into::into)
            }
            
            Builtin::Error => {
                // Fallback: if called directly without trampolining, create error object
                // and return as Rust error. Normal path goes through apply_error_builtin.
                let msg = self.lisp.car(args)?;
                let irritants = self.lisp.cdr(args)?;
                let nil = self.lisp.nil()?;
                let irritants_and_type = self.lisp.cons(irritants, nil)?;
                let error_obj = self.lisp.alloc(Value::ErrorObject { message: msg, irritants_and_type })?;
                Err(self.make_error(ErrorKind::UserError, error_obj))
            }
            
            Builtin::ErrorObjectP => {
                builtin_unary_pred!(self, args, |v: Value| matches!(v, Value::ErrorObject { .. }))
            }
            
            Builtin::ErrorObjectMessage => {
                // (error-object-message error-object) — R7RS §6.11
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::ErrorObject { message, .. } => Ok(message),
                    _ => Err(self.type_error(arg, "error-object", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::ErrorObjectIrritants | Builtin::ErrorObjectType => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::ErrorObject { irritants_and_type, .. } => {
                        if matches!(builtin, Builtin::ErrorObjectIrritants) {
                            self.lisp.car(irritants_and_type).map_err(Into::into)
                        } else {
                            self.lisp.cdr(irritants_and_type).map_err(Into::into)
                        }
                    }
                    _ => Err(self.type_error(arg, "error-object", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::SetCar | Builtin::SetCdr => {
                extract_args!(self, args, pair, value);
                match self.lisp.get(pair)? {
                    Value::Cons { .. } => {
                        if matches!(builtin, Builtin::SetCar) {
                            self.lisp.set_car(pair, value).map_err(Into::into)
                        } else {
                            self.lisp.set_cdr(pair, value).map_err(Into::into)
                        }
                    }
                    _ => Err(self.make_error(ErrorKind::NotAPair, call_expr)),
                }
            }
            
            // ============================================================
            // Vector operations (R7RS Section 6.8)
            // ============================================================
            
            Builtin::Vectorp => {
                builtin_unary_pred!(self, args, |v: Value| matches!(v, Value::Array { .. }))
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
                self.list_to_array(args, call_expr)
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
                // (vector->list vec [start [end]]) - convert vector to list
                let vec = self.lisp.car(args)?;
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
                        let rest = self.lisp.cdr(args)?;
                        let (start, end) = self.parse_range_args(rest, len, call_expr)?;
                        // Build list from end to front
                        let mut result = self.lisp.nil()?;
                        for i in (start..end).rev() {
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
                self.list_to_array(lst, call_expr)
            }
            
            Builtin::VectorFill => {
                // (vector-fill! vec fill [start [end]]) - fill vector with value
                let vec = self.lisp.car(args)?;
                let rest1 = self.lisp.cdr(args)?;
                let fill = self.lisp.car(rest1)?;
                let rest2 = self.lisp.cdr(rest1)?;
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
                        let (start, end) = self.parse_range_args(rest2, len, call_expr)?;
                        for i in start..end {
                            self.lisp.array_set(vec, i, fill)?;
                        }
                        // R7RS: returns unspecified, we return the vector
                        Ok(vec)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::VectorCopy => {
                // (vector-copy vec [start [end]]) - copy a vector
                let vec = self.lisp.car(args)?;
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
                        let rest = self.lisp.cdr(args)?;
                        let (start, end) = self.parse_range_args(rest, len, call_expr)?;
                        
                        let new_len = end - start;
                        let placeholder = self.lisp.number(0)?;
                        let new_vec = self.lisp.make_array(new_len, placeholder)?;
                        
                        for i in 0..new_len {
                            let elem = self.lisp.array_get(vec, start + i)?;
                            self.lisp.array_set(new_vec, i, elem)?;
                        }
                        
                        Ok(new_vec)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::VectorCopyTo => {
                // (vector-copy! to at from [start [end]]) - copy elements between vectors
                let to_vec = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let at_idx = self.lisp.car(rest)?;
                let rest2 = self.lisp.cdr(rest)?;
                let from_vec = self.lisp.car(rest2)?;
                let rest3 = self.lisp.cdr(rest2)?;
                
                let at = match self.lisp.get(at_idx)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                let (to_len, from_len) = match (self.lisp.get(to_vec)?, self.lisp.get(from_vec)?) {
                    (Value::Array { .. }, Value::Array { .. }) => {
                        (self.lisp.array_len(to_vec)?, self.lisp.array_len(from_vec)?)
                    }
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                let (start, end) = self.parse_range_args(rest3, from_len, call_expr)?;
                let count = end - start;
                if at + count > to_len {
                    return Err(self.make_error(ErrorKind::TypeError, call_expr));
                }
                
                // Use a temporary arena vector to handle overlapping ranges correctly.
                // array_get returns slot ArenaIndices; if source/dest overlap, slots may be
                // overwritten before being read. A temp vector stores independent copies.
                let placeholder = self.lisp.number(0)?;
                let temp_vec = self.lisp.make_array(count, placeholder)?;
                for i in 0..count {
                    let elem = self.lisp.array_get(from_vec, start + i)?;
                    self.lisp.array_set(temp_vec, i, elem)?;
                }
                for i in 0..count {
                    let elem = self.lisp.array_get(temp_vec, i)?;
                    self.lisp.array_set(to_vec, at + i, elem)?;
                }
                
                self.lisp.void_val().map_err(Into::into)
            }
            
            Builtin::VectorAppend => {
                // (vector-append vec ...) - concatenate vectors
                // Pass 1: count total elements and validate all args are vectors
                let mut total_len = 0;
                let mut current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            match self.lisp.get(car)? {
                                Value::Array { .. } => {
                                    total_len += self.lisp.array_len(car)?;
                                }
                                v => return Err(self.type_error(call_expr, "vector", v.type_name())),
                            }
                            current = self.lisp.cdr(current)?;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
                
                // Pass 2: create result array and copy elements
                let placeholder = self.lisp.number(0)?;
                let result = self.lisp.make_array(total_len, placeholder)?;
                let mut offset = 0;
                current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let len = self.lisp.array_len(car)?;
                            for i in 0..len {
                                self.lisp.array_set(result, offset + i, self.lisp.array_get(car, i)?)?;
                            }
                            offset += len;
                            current = self.lisp.cdr(current)?;
                        }
                        _ => break,
                    }
                }
                Ok(result)
            }
            
            Builtin::VectorMap | Builtin::VectorForEach => {
                // These are handled in apply_builtin_trampolined
                unreachable!("vector-map and vector-for-each are trampolined")
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
            
            Builtin::GcEnable | Builtin::GcDisable => {
                let enable = matches!(builtin, Builtin::GcEnable);
                self.lisp.arena().set_gc_enabled(enable);
                self.lisp.boolean(enable).map_err(Into::into)
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
            
            Builtin::CharEq | Builtin::CharLt | Builtin::CharGt
            | Builtin::CharLe | Builtin::CharGe
            | Builtin::CharCiEq | Builtin::CharCiLt | Builtin::CharCiGt
            | Builtin::CharCiLe | Builtin::CharCiGe => {
                let cmp_fn: fn(char, char) -> bool = match builtin {
                    Builtin::CharEq | Builtin::CharCiEq => |a, b| a == b,
                    Builtin::CharLt | Builtin::CharCiLt => |a, b| a < b,
                    Builtin::CharGt | Builtin::CharCiGt => |a, b| a > b,
                    Builtin::CharLe | Builtin::CharCiLe => |a, b| a <= b,
                    _ => |a, b| a >= b,
                };
                let case_insensitive = matches!(builtin,
                    Builtin::CharCiEq | Builtin::CharCiLt | Builtin::CharCiGt
                    | Builtin::CharCiLe | Builtin::CharCiGe);
                if case_insensitive {
                    self.char_chain_compare(args, |a, b| cmp_fn(grift_unicode::char_foldcase(a), grift_unicode::char_foldcase(b)), call_expr)
                } else {
                    self.char_chain_compare(args, cmp_fn, call_expr)
                }
            }
            
            Builtin::CharToInteger => builtin_char_to_int!(self, args, call_expr),
            
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
            
            Builtin::CharUpcase | Builtin::CharDowncase | Builtin::CharFoldcase => {
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                let result = match builtin {
                    Builtin::CharUpcase => grift_unicode::char_upcase(c),
                    Builtin::CharDowncase => grift_unicode::char_downcase(c),
                    _ => grift_unicode::char_foldcase(c),
                };
                self.lisp.char(result).map_err(Into::into)
            }
            
            // Character classification predicates
            Builtin::CharAlphabetic | Builtin::CharWhitespace
            | Builtin::CharUpperCase | Builtin::CharLowerCase => {
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                let result = match builtin {
                    Builtin::CharAlphabetic => grift_unicode::char_is_alphabetic(c),
                    Builtin::CharWhitespace => grift_unicode::char_is_whitespace(c),
                    Builtin::CharUpperCase => grift_unicode::char_is_uppercase(c),
                    _ => grift_unicode::char_is_lowercase(c),
                };
                self.lisp.boolean(result).map_err(Into::into)
            }
            
            Builtin::CharNumeric => {
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                let result = grift_unicode::char_is_numeric(c);
                self.lisp.boolean(result).map_err(Into::into)
            }
            
            Builtin::DigitValue => {
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                match grift_unicode::digit_value(c) {
                    Some(val) => self.lisp.number(val as isize).map_err(Into::into),
                    None => self.lisp.boolean(false).map_err(Into::into),
                }
            }
            
            
            Builtin::StringUpcase | Builtin::StringDowncase | Builtin::StringFoldcase => {
                let str_idx = self.lisp.car(args)?;
                let (len, data) = self.get_string(str_idx, call_expr)?;
                let case_map_char = |c: char| -> grift_unicode::CaseMapResult {
                    match builtin {
                        Builtin::StringUpcase => grift_unicode::full_upcase(c),
                        Builtin::StringDowncase => grift_unicode::full_downcase(c),
                        _ => grift_unicode::full_foldcase(c),
                    }
                };
                
                // First pass: compute total length after full case mapping
                let mut total_len = 0usize;
                for i in 0..len {
                    let slot = self.lisp.arena_index_at_offset(data, i)?;
                    if let Value::Char(c) = self.lisp.get(slot)? {
                        total_len += case_map_char(c).len();
                    } else {
                        return Err(self.make_error(ErrorKind::TypeError, call_expr));
                    }
                }
                
                // Re-read data after potential arena operations above
                let data = match self.lisp.get(str_idx)? {
                    Value::String { data, .. } => data,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                // Allocate result string with full mapped length
                let result = self.lisp.make_string(total_len, '\0')?;
                
                // Second pass: fill result with mapped characters
                let mut pos = 0;
                for i in 0..len {
                    let slot = self.lisp.arena_index_at_offset(data, i)?;
                    if let Value::Char(c) = self.lisp.get(slot)? {
                        let mapped = case_map_char(c);
                        for j in 0..mapped.len() {
                            if let Some(ch) = mapped.get(j) {
                                self.lisp.string_set(result, pos, ch)?;
                                pos += 1;
                            }
                        }
                    } else {
                        return Err(self.make_error(ErrorKind::TypeError, call_expr));
                    }
                }
                
                Ok(result)
            }
            
            Builtin::StringCiEq | Builtin::StringCiLt | Builtin::StringCiGt
            | Builtin::StringCiLe | Builtin::StringCiGe => {
                let cmp_fn: fn(core::cmp::Ordering) -> bool = match builtin {
                    Builtin::StringCiEq => |o| o == core::cmp::Ordering::Equal,
                    Builtin::StringCiLt => |o| o == core::cmp::Ordering::Less,
                    Builtin::StringCiGt => |o| o == core::cmp::Ordering::Greater,
                    Builtin::StringCiLe => |o| o != core::cmp::Ordering::Greater,
                    _ => |o| o != core::cmp::Ordering::Less,
                };
                self.string_ci_chain_compare(args, cmp_fn, call_expr)
            }
            
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
                
                // Allocate directly in the arena (no fixed-size stack buffer)
                self.lisp.make_string(k as usize, fill).map_err(Into::into)
            }
            
            Builtin::String => {
                // (string char ...) - Create string from characters
                // Collect chars into arena cons list (reversed), then build string
                let nil = self.lisp.nil()?;
                let mut collected = nil;
                let mut len: usize = 0;
                let mut current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            let c = self.get_char(car, call_expr)?;
                            let ch = self.lisp.alloc(Value::Char(c))?;
                            collected = self.lisp.cons(ch, collected)?;
                            len += 1;
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
                self.reversed_char_cons_to_string(collected, len)
            }
            
            Builtin::StringLength => {
                // (string-length string) - Get length
                let str_idx = self.lisp.car(args)?;
                let (len, _) = self.get_string(str_idx, call_expr)?;
                self.lisp.number(len as isize).map_err(Into::into)
            }
            
            Builtin::StringRef => {
                // (string-ref string k) - Get character at index
                extract_args!(self, args, str_idx, k_idx);
                let (len, data) = self.get_string(str_idx, call_expr)?;
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
            
            Builtin::StringSet => {
                // (string-set! string k char) - Set character at index
                extract_args!(self, args, str_idx, k_idx, char_arg);
                let (len, data) = self.get_string(str_idx, call_expr)?;
                let k = self.get_int(k_idx, call_expr)?;
                if k < 0 || (k as usize) >= len {
                    return Err(self.make_error(ErrorKind::TypeError, call_expr));
                }
                let c = self.get_char(char_arg, call_expr)?;
                // Copy-on-write: if the data is shared (interned), make a private copy
                if self.lisp.string_data_is_interned(data)? {
                    self.lisp.string_copy_data(str_idx)?;
                }
                // Re-read data after potential COW
                let current_data = match self.lisp.get(str_idx)? {
                    Value::String { data: d, .. } => d,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                // Characters start at data (no header with inline length)
                let char_slot = self.lisp.arena_index_at_offset(current_data, k as usize)?;
                self.lisp.set(char_slot, Value::Char(c))?;
                // Return unspecified value (we use the string itself)
                Ok(str_idx)
            }
            
            Builtin::StringEq | Builtin::StringLt | Builtin::StringGt
            | Builtin::StringLe | Builtin::StringGe => {
                let cmp_fn: fn(core::cmp::Ordering) -> bool = match builtin {
                    Builtin::StringEq => |o| o == core::cmp::Ordering::Equal,
                    Builtin::StringLt => |o| o == core::cmp::Ordering::Less,
                    Builtin::StringGt => |o| o == core::cmp::Ordering::Greater,
                    Builtin::StringLe => |o| o != core::cmp::Ordering::Greater,
                    _ => |o| o != core::cmp::Ordering::Less,
                };
                self.string_chain_compare(args, cmp_fn, call_expr)
            }
            
            Builtin::StringAppend => {
                // (string-append string ...) - Concatenate strings
                // First pass: compute total length
                let mut total_len: usize = 0;
                let mut current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            match self.lisp.get(car)? {
                                Value::String { len, .. } => {
                                    total_len += len;
                                }
                                v => return Err(self.type_error(call_expr, "string", v.type_name())),
                            }
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
                
                // Allocate result string
                let result = self.lisp.make_string(total_len, '\0')?;
                
                // Second pass: copy characters
                let mut pos = 0;
                current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            if let Value::String { len, data } = self.lisp.get(car)? {
                                for i in 0..len {
                                    let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                                    if let Value::Char(c) = self.lisp.get(char_slot)? {
                                        self.lisp.string_set(result, pos, c)?;
                                        pos += 1;
                                    }
                                }
                            }
                            current = cdr;
                        }
                        _ => break,
                    }
                }
                
                Ok(result)
            }
            
            Builtin::StringToList => {
                // (string->list string [start [end]]) - Convert string to list of characters
                let str_idx = self.lisp.car(args)?;
                let (len, data) = self.get_string(str_idx, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                let (start, end) = self.parse_range_args(rest, len, call_expr)?;
                let mut result = self.lisp.nil()?;
                // Build list from end to start
                for i in (start..end).rev() {
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
            
            Builtin::ListToString => {
                // (list->string list) - Convert list of characters to string
                // Two-pass: count elements, then allocate and fill
                let list = self.lisp.car(args)?;
                let len = self.lisp.list_len(list)?;
                let result = self.lisp.make_string(len, '\0')?;
                let mut current = list;
                for i in 0..len {
                    let car = self.lisp.car(current)?;
                    let c = self.get_char(car, call_expr)?;
                    self.lisp.string_set(result, i, c)?;
                    current = self.lisp.cdr(current)?;
                }
                Ok(result)
            }
            
            Builtin::Substring => {
                // (substring string start end) - Extract substring
                extract_args!(self, args, str_idx, start_idx, end_idx);
                let (len, data) = self.get_string(str_idx, call_expr)?;
                let start = self.get_int(start_idx, call_expr)?;
                let end = self.get_int(end_idx, call_expr)?;
                
                if start < 0 || end < 0 || (start as usize) > len || (end as usize) > len || start > end {
                    return Err(self.make_error(ErrorKind::TypeError, call_expr));
                }
                
                self.copy_string_range(data, start as usize, (end - start) as usize, call_expr)
            }
            
            Builtin::StringCopy => {
                // (string-copy string [start [end]]) - Copy a string
                let str_idx = self.lisp.car(args)?;
                let (len, data) = self.get_string(str_idx, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                let (start, end) = self.parse_range_args(rest, len, call_expr)?;
                self.copy_string_range(data, start, end - start, call_expr)
            }
            
            Builtin::StringCopyTo => {
                // (string-copy! to at from [start [end]]) - copy characters between strings
                extract_args!(self, args, to_str, at_idx, from_str);
                
                let at = match self.lisp.get(at_idx)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                let (to_len, _) = self.get_string(to_str, call_expr)?;
                let (from_len, from_data) = self.get_string(from_str, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                
                let (start, end) = self.parse_range_args(rest, from_len, call_expr)?;
                let count = end - start;
                if at + count > to_len {
                    return Err(self.make_error(ErrorKind::TypeError, call_expr));
                }
                
                // Copy-on-write: ensure destination is mutable
                if self.lisp.string_data_is_interned(match self.lisp.get(to_str)? {
                    Value::String { data, .. } => data,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                })? {
                    self.lisp.string_copy_data(to_str)?;
                }
                
                // Read source characters into arena cons list for overlapping safety
                let nil = self.lisp.nil()?;
                let mut collected = nil;
                for i in (0..count).rev() {
                    let char_slot = self.lisp.arena_index_at_offset(from_data, start + i)?;
                    match self.lisp.get(char_slot)? {
                        Value::Char(c) => {
                            let ch_val = self.lisp.char(c)?;
                            collected = self.lisp.cons(ch_val, collected)?;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                    }
                }
                
                // Write to destination from collected list
                let to_data = match self.lisp.get(to_str)? {
                    Value::String { data, .. } => data,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                let mut cursor = collected;
                for i in 0..count {
                    let ch_idx = self.lisp.car(cursor)?;
                    if let Value::Char(ch) = self.lisp.get(ch_idx)? {
                        let char_slot = self.lisp.arena_index_at_offset(to_data, at + i)?;
                        self.lisp.set(char_slot, Value::Char(ch))?;
                    }
                    cursor = self.lisp.cdr(cursor)?;
                }
                
                self.lisp.void_val().map_err(Into::into)
            }
            
            Builtin::StringFill => {
                // (string-fill! string fill [start [end]]) - fill string with character
                extract_args!(self, args, str_idx, fill_arg);
                let rest = self.lisp.cdr(args)?;
                
                let fill_char = self.get_char(fill_arg, call_expr)?;
                let (len, data) = self.get_string(str_idx, call_expr)?;
                let (start, end) = self.parse_range_args(rest, len, call_expr)?;
                
                // Copy-on-write: ensure string is mutable
                if self.lisp.string_data_is_interned(data)? {
                    self.lisp.string_copy_data(str_idx)?;
                }
                
                // Re-read data after potential COW
                let current_data = match self.lisp.get(str_idx)? {
                    Value::String { data: d, .. } => d,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                for i in start..end {
                    let char_slot = self.lisp.arena_index_at_offset(current_data, i)?;
                    self.lisp.set(char_slot, Value::Char(fill_char))?;
                }
                
                self.lisp.void_val().map_err(Into::into)
            }
            
            // ============================================================
            // Syntax-case support (R6RS Chapter 11)
            // ============================================================
            
            Builtin::Identifierp => {
                // (identifier? x) - Check if x is an identifier
                // An identifier is either a symbol or a syntax object wrapping a symbol
                let arg = self.lisp.car(args)?;
                let is_id = match self.lisp.get(arg)? {
                    Value::Symbol(_) => true,
                    Value::Syntax { expr, .. } => {
                        matches!(self.lisp.get(expr)?, Value::Symbol(_))
                    }
                    _ => false,
                };
                self.lisp.boolean(is_id).map_err(Into::into)
            }
            
            Builtin::BoundIdentifierEq => {
                // (bound-identifier=? id1 id2) - Check if two identifiers have same name and marks
                extract_args!(self, args, id1, id2);
                let result = self.bound_identifier_eq(id1, id2)?;
                self.lisp.boolean(result).map_err(Into::into)
            }
            
            Builtin::FreeIdentifierEq => {
                // (free-identifier=? id1 id2) - Check if two identifiers resolve to same binding
                extract_args!(self, args, id1, id2);
                let result = self.free_identifier_eq(id1, id2)?;
                self.lisp.boolean(result).map_err(Into::into)
            }
            
            Builtin::SyntaxToDatum | Builtin::SyntaxE | Builtin::SyntaxObjectToDatum => {
                // syntax->datum / syntax-e / syntax-object->datum - Strip syntax wrapper
                let stx = self.lisp.car(args)?;
                self.syntax_to_datum_recursive(stx)
            }
            
            Builtin::DatumToSyntax | Builtin::DatumToSyntaxObject => {
                // datum->syntax / datum->syntax-object - Wrap datum with syntax context
                extract_args!(self, args, template_id, datum);
                self.datum_to_syntax(template_id, datum)
            }
            
            Builtin::GenerateTemporaries => {
                // (generate-temporaries list) - Generate a list of fresh identifiers
                // For each element in the input list, generate a unique temporary identifier
                let input = self.lisp.car(args)?;
                self.generate_temporaries(input)
            }
            
            Builtin::SymbolToString => {
                // (symbol->string sym) - Convert symbol to string
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Symbol(chars) => {
                        // The symbol stores a reference to a String value
                        // We return that string directly
                        Ok(chars)
                    }
                    _ => Err(self.type_error(call_expr, "symbol", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::StringToSymbol => {
                // (string->symbol str) - Convert string to symbol
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::String { .. } => {
                        // Create/intern a symbol with this string
                        self.lisp.symbol_from_string(arg).map_err(Into::into)
                    }
                    _ => Err(self.type_error(call_expr, "string", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::NumberToString => {
                // (number->string num) or (number->string num radix)
                let arg = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let radix: u32 = if self.lisp.get(rest)?.is_nil() {
                    10
                } else {
                    let r = self.get_int(self.lisp.car(rest)?, call_expr)?;
                    match r {
                        2 | 8 | 10 | 16 => r as u32,
                        _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                    }
                };
                match self.lisp.get(arg)? {
                    Value::Number(n) => {
                        let mut buf = [0u8; 66]; // enough for binary i64 including sign
                        let mut pos = buf.len();
                        let negative = n < 0;
                        let mut val = n.unsigned_abs();
                        
                        if val == 0 {
                            pos -= 1;
                            buf[pos] = b'0';
                        } else {
                            while val > 0 {
                                pos -= 1;
                                let digit = (val % radix as usize) as u8;
                                buf[pos] = if digit < 10 { b'0' + digit } else { b'a' + digit - 10 };
                                val /= radix as usize;
                            }
                        }
                        
                        if negative {
                            pos -= 1;
                            buf[pos] = b'-';
                        }
                        
                        let len = buf.len() - pos;
                        let mut chars = ['\0'; 66];
                        for i in 0..len {
                            chars[i] = buf[pos + i] as char;
                        }
                        self.lisp.string_from_chars(&chars[..len]).map_err(Into::into)
                    }
                    Value::Float(f) => {
                        if radix != 10 {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        let mut chars = ['\0'; 32];
                        let len = format_float_to_chars(f, &mut chars);
                        self.lisp.string_from_chars(&chars[..len]).map_err(Into::into)
                    }
                    Value::Rational { num, denom } => {
                        if radix != 10 {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        // Format as "num/denom"
                        let mut chars = ['\0'; 42];
                        let mut pos = 0;
                        pos += format_isize_decimal(num, &mut chars[pos..]);
                        chars[pos] = '/';
                        pos += 1;
                        pos += format_isize_decimal(denom, &mut chars[pos..]);
                        self.lisp.string_from_chars(&chars[..pos]).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::StringToNumber => {
                // (string->number str) or (string->number str radix)
                let arg = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let explicit_radix: Option<u32> = if self.lisp.get(rest)?.is_nil() {
                    None
                } else {
                    let r = self.get_int(self.lisp.car(rest)?, call_expr)?;
                    match r {
                        2 | 8 | 10 | 16 => Some(r as u32),
                        _ => return self.lisp.false_val().map_err(Into::into),
                    }
                };
                match self.lisp.get(arg)? {
                    Value::String { len, data } => {
                        if len == 0 {
                            return self.lisp.false_val().map_err(Into::into);
                        }
                        let mut buf = [0u8; 128];
                        if len > buf.len() {
                            return self.lisp.false_val().map_err(Into::into);
                        }
                        for (i, slot) in buf.iter_mut().enumerate().take(len) {
                            let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                            match self.lisp.get(char_slot)? {
                                Value::Char(c) => {
                                    if !c.is_ascii() {
                                        return self.lisp.false_val().map_err(Into::into);
                                    }
                                    *slot = c as u8;
                                }
                                _ => return self.lisp.false_val().map_err(Into::into),
                            }
                        }
                        let s = core::str::from_utf8(&buf[..len]).unwrap_or("");
                        
                        // If explicit radix is provided and no prefix in string, add prefix
                        let prefixed: [u8; 130];
                        let to_parse = if let Some(r) = explicit_radix {
                            if !s.starts_with('#') {
                                let prefix = match r {
                                    2 => "#b",
                                    8 => "#o",
                                    16 => "#x",
                                    _ => "",
                                };
                                if !prefix.is_empty() {
                                    let pb = prefix.as_bytes();
                                    let sb = s.as_bytes();
                                    let total = pb.len() + sb.len();
                                    if total > 130 {
                                        return self.lisp.false_val().map_err(Into::into);
                                    }
                                    prefixed = {
                                        let mut arr = [0u8; 130];
                                        arr[..pb.len()].copy_from_slice(pb);
                                        arr[pb.len()..total].copy_from_slice(sb);
                                        arr
                                    };
                                    core::str::from_utf8(&prefixed[..total]).unwrap_or("")
                                } else {
                                    s
                                }
                            } else {
                                s
                            }
                        } else {
                            s
                        };
                        
                        // Use the parser/lexer to parse the number
                        match grift_parser::parse_single(self.lisp, to_parse) {
                            Ok(idx) => {
                                match self.lisp.get(idx)? {
                                    Value::Number(_) | Value::Float(_) | Value::Rational { .. } 
                                    | Value::Complex { .. } => Ok(idx),
                                    _ => self.lisp.false_val().map_err(Into::into),
                                }
                            }
                            Err(_) => self.lisp.false_val().map_err(Into::into),
                        }
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }

            // ================================================================
            // Port operations (R7RS §6.13)
            // ================================================================

            Builtin::Portp => {
                let arg = self.lisp.car(args)?;
                let is_port = matches!(self.lisp.get(arg)?, Value::Port(_));
                self.lisp.boolean(is_port).map_err(Into::into)
            }

            Builtin::InputPortp => self.port_predicate(args, |io, pid| io.is_input_port(pid)),

            Builtin::OutputPortp => self.port_predicate(args, |io, pid| io.is_output_port(pid)),

            Builtin::CurrentInputPort | Builtin::CurrentOutputPort | Builtin::CurrentErrorPort => {
                let pid = match builtin {
                    Builtin::CurrentInputPort => self.current_input_port,
                    Builtin::CurrentOutputPort => self.current_output_port,
                    _ => grift_parser::PortId::STDERR,
                };
                self.lisp.port(pid).map_err(Into::into)
            }

            Builtin::ClosePort | Builtin::CloseInputPort | Builtin::CloseOutputPort => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Port(pid) => {
                        if let Some(ref mut io) = self.io {
                            io.close_port(pid).map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        }
                        self.lisp.void_val().map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "port", v.type_name())),
                }
            }

            Builtin::ReadChar | Builtin::PeekChar => {
                let pid = self.extract_input_port(args, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        let result = if matches!(builtin, Builtin::ReadChar) {
                            io.read_char(pid)
                        } else {
                            io.peek_char(pid)
                        };
                        match result {
                            Ok(c) => self.lisp.alloc(Value::Char(c)).map_err(Into::into),
                            Err(grift_parser::IoErrorKind::Eof) => self.lisp.eof().map_err(Into::into),
                            Err(_) => Err(self.make_error(ErrorKind::Generic, call_expr)),
                        }
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::CharReadyp => {
                // (char-ready?) or (char-ready? port)
                let pid = self.extract_input_port(args, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        let ready = io.char_ready(pid).unwrap_or(false);
                        self.lisp.boolean(ready).map_err(Into::into)
                    }
                    None => self.lisp.boolean(false).map_err(Into::into),
                }
            }

            Builtin::WriteChar => {
                // (write-char char) or (write-char char port)
                let ch_arg = self.lisp.car(args)?;
                let c = match self.lisp.get(ch_arg)? {
                    Value::Char(c) => c,
                    v => return Err(self.type_error(call_expr, "char", v.type_name())),
                };
                let rest = self.lisp.cdr(args)?;
                let pid = self.extract_output_port(rest, call_expr)?;
                if let Some(ref mut io) = self.io {
                    io.write_char(pid, c).map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                } else if let Some(callback) = self.output_callback {
                    callback(self.lisp, ch_arg);
                }
                self.lisp.void_val().map_err(Into::into)
            }

            Builtin::WriteSimple => {
                // (write-simple obj [port]) - no datum labels
                let val = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let has_port = !self.lisp.get(rest)?.is_nil();
                if has_port {
                    let pid = self.extract_output_port(rest, call_expr)?;
                    if let Some(ref mut io) = self.io {
                        let dv = grift_parser::DisplayValue::new(val, self.lisp);
                        let mut writer = IoPortWriter { io: &mut **io, port: pid, error: false };
                        use core::fmt::Write;
                        let _ = write!(writer, "{}", dv);
                        if writer.error {
                            return Err(self.make_error(ErrorKind::Generic, call_expr));
                        }
                    }
                } else if let Some(callback) = self.output_callback {
                    callback(self.lisp, val);
                } else if let Some(ref mut io) = self.io {
                    let pid = self.current_output_port;
                    let dv = grift_parser::DisplayValue::new(val, self.lisp);
                    let mut writer = IoPortWriter { io: &mut **io, port: pid, error: false };
                    use core::fmt::Write;
                    let _ = write!(writer, "{}", dv);
                    if writer.error {
                        return Err(self.make_error(ErrorKind::Generic, call_expr));
                    }
                }
                self.lisp.void_val().map_err(Into::into)
            }

            Builtin::Write | Builtin::WriteShared => {
                // (write obj [port]) - datum labels for circular structures
                // (write-shared obj [port]) - datum labels for all shared structures
                let shared_mode = matches!(builtin, Builtin::WriteShared);
                let val = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let has_port = !self.lisp.get(rest)?.is_nil();
                if has_port {
                    let pid = self.extract_output_port(rest, call_expr)?;
                    if let Some(ref mut io) = self.io {
                        let mut writer = IoPortWriter { io: &mut **io, port: pid, error: false };
                        write_with_labels(self.lisp, val, &mut writer, shared_mode);
                        if writer.error {
                            return Err(self.make_error(ErrorKind::Generic, call_expr));
                        }
                    }
                } else if let Some(callback) = self.output_callback {
                    callback(self.lisp, val);
                } else if let Some(ref mut io) = self.io {
                    let pid = self.current_output_port;
                    let mut writer = IoPortWriter { io: &mut **io, port: pid, error: false };
                    write_with_labels(self.lisp, val, &mut writer, shared_mode);
                    if writer.error {
                        return Err(self.make_error(ErrorKind::Generic, call_expr));
                    }
                }
                self.lisp.void_val().map_err(Into::into)
            }

            Builtin::Read => {
                // (read) or (read port)
                let pid = self.extract_input_port(args, call_expr)?;
                // Reset datum label table for each top-level read
                self.read_labels = self.lisp.nil()?;
                self.apply_read_builtin(pid, call_expr)
            }

            Builtin::EofObject => {
                self.lisp.eof().map_err(Into::into)
            }

            Builtin::EofObjectp => {
                let arg = self.lisp.car(args)?;
                let is_eof = matches!(self.lisp.get(arg)?, Value::Eof);
                self.lisp.boolean(is_eof).map_err(Into::into)
            }

            Builtin::OpenInputString => {
                // (open-input-string str)
                let arg = self.lisp.car(args)?;
                let (len, data) = self.get_string(arg, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        // Write chars to a temp output string port, then convert
                        // to an input port. This avoids any fixed-size buffer.
                        let tmp = match io.open_output_string() {
                            Ok(p) => p,
                            Err(_) => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                        };
                        for i in 0..len {
                            let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                            if let Value::Char(c) = self.lisp.get(char_slot)? {
                                if io.write_char(tmp, c).is_err() {
                                    let _ = io.close_port(tmp);
                                    return Err(self.make_error(ErrorKind::Generic, call_expr));
                                }
                            }
                        }
                        let pid = match io.output_string_to_input_port(tmp) {
                            Ok(p) => p,
                            Err(_) => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                        };
                        self.lisp.port(pid).map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::OpenOutputString => {
                // (open-output-string)
                match &mut self.io {
                    Some(io) => {
                        let pid = io.open_output_string()
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        self.lisp.port(pid).map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::GetOutputString => {
                // (get-output-string port)
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Port(pid) => {
                        match &self.io {
                            Some(io) => {
                                let s = io.get_output_string(pid)
                                    .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                                self.lisp.string(s).map_err(Into::into)
                            }
                            None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                        }
                    }
                    v => Err(self.type_error(call_expr, "port", v.type_name())),
                }
            }

            Builtin::ReadLine => {
                // (read-line) or (read-line port)
                let pid = self.extract_input_port(args, call_expr)?;
                let io = match &mut self.io {
                    Some(io) => io,
                    None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                };
                // Collect chars into an arena cons list (reversed order)
                let nil = self.lisp.nil()?;
                let mut collected = nil;
                let mut count: usize = 0;
                let mut got_char = false;
                loop {
                    match io.read_char(pid) {
                        Ok('\n') => break,
                        Ok('\r') => {
                            // Handle \r\n: peek to consume \n if present
                            if let Ok('\n') = io.peek_char(pid) {
                                let _ = io.read_char(pid);
                            }
                            break;
                        }
                        Ok(c) => {
                            got_char = true;
                            let ch = self.lisp.alloc(Value::Char(c))?;
                            collected = self.lisp.cons(ch, collected)?;
                            count += 1;
                        }
                        Err(grift_parser::IoErrorKind::Eof) => {
                            if !got_char {
                                return self.lisp.eof().map_err(Into::into);
                            }
                            break;
                        }
                        Err(_) => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                    }
                }
                // Build string from reversed cons list of chars
                self.reversed_char_cons_to_string(collected, count)
            }

            Builtin::ReadString => {
                // (read-string k) or (read-string k port)
                let k_arg = self.lisp.car(args)?;
                let k = match self.lisp.get(k_arg)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    v => return Err(self.type_error(call_expr, "non-negative integer", v.type_name())),
                };
                let rest = self.lisp.cdr(args)?;
                let pid = self.extract_input_port(rest, call_expr)?;
                let io = match &mut self.io {
                    Some(io) => io,
                    None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                };
                // Collect chars into an arena cons list (reversed order)
                let nil = self.lisp.nil()?;
                let mut collected = nil;
                let mut chars_read: usize = 0;
                while chars_read < k {
                    match io.read_char(pid) {
                        Ok(c) => {
                            let ch = self.lisp.alloc(Value::Char(c))?;
                            collected = self.lisp.cons(ch, collected)?;
                            chars_read += 1;
                        }
                        Err(grift_parser::IoErrorKind::Eof) => break,
                        Err(_) => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                    }
                }
                if chars_read == 0 {
                    return self.lisp.eof().map_err(Into::into);
                }
                // Build string from reversed cons list of chars
                self.reversed_char_cons_to_string(collected, chars_read)
            }

            Builtin::TextualPortp => self.port_predicate(args, |io, pid| io.is_textual_port(pid)),

            Builtin::BinaryPortp => self.port_predicate(args, |io, pid| io.is_binary_port(pid)),

            Builtin::InputPortOpenp => self.port_predicate(args, |io, pid| io.is_input_port(pid) && io.is_port_open(pid)),

            Builtin::OutputPortOpenp => self.port_predicate(args, |io, pid| io.is_output_port(pid) && io.is_port_open(pid)),

            // ================================================================
            // File port operations (R7RS §6.13.2)
            // ================================================================

            Builtin::OpenInputFile => self.open_file_port(args, call_expr, grift_parser::FileOpenMode::TextInput),
            Builtin::OpenOutputFile => self.open_file_port(args, call_expr, grift_parser::FileOpenMode::TextOutput),
            Builtin::OpenBinaryInputFile => self.open_file_port(args, call_expr, grift_parser::FileOpenMode::BinaryInput),
            Builtin::OpenBinaryOutputFile => self.open_file_port(args, call_expr, grift_parser::FileOpenMode::BinaryOutput),

            // call-with-input-file/output-file and with-input-from-file/output-to-file
            // and call-with-port are handled in apply_builtin_trampolined
            Builtin::CallWithInputFile | Builtin::CallWithOutputFile
            | Builtin::WithInputFromFile | Builtin::WithOutputToFile
            | Builtin::CallWithPort => {
                Err(self.make_error(ErrorKind::Generic, call_expr))
            }

            // ================================================================
            // Binary I/O operations (R7RS §6.13.2)
            // ================================================================

            Builtin::ReadU8 | Builtin::PeekU8 => {
                // (read-u8) or (read-u8 port)
                let pid = self.extract_input_port(args, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        let result = if matches!(builtin, Builtin::ReadU8) {
                            io.read_u8(pid)
                        } else {
                            io.peek_u8(pid)
                        };
                        match result {
                            Ok(b) => self.lisp.number(b as isize).map_err(Into::into),
                            Err(grift_parser::IoErrorKind::Eof) => self.lisp.eof().map_err(Into::into),
                            Err(_) => Err(self.make_error(ErrorKind::Generic, call_expr)),
                        }
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::U8Readyp => {
                let pid = self.extract_input_port(args, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        let ready = io.u8_ready(pid).unwrap_or(false);
                        self.lisp.boolean(ready).map_err(Into::into)
                    }
                    None => self.lisp.boolean(false).map_err(Into::into),
                }
            }

            Builtin::WriteU8 => {
                // (write-u8 byte) or (write-u8 byte port)
                let byte_arg = self.lisp.car(args)?;
                let byte_val = match self.lisp.get(byte_arg)? {
                    Value::Number(n) if n >= 0 && n <= 255 => n as u8,
                    v => return Err(self.type_error(call_expr, "exact integer 0..255", v.type_name())),
                };
                let rest = self.lisp.cdr(args)?;
                let pid = self.extract_output_port(rest, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        io.write_u8(pid, byte_val)
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        self.lisp.void_val().map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::ReadBytevector => {
                // (read-bytevector k) or (read-bytevector k port)
                let k_arg = self.lisp.car(args)?;
                let k = match self.lisp.get(k_arg)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    v => return Err(self.type_error(call_expr, "non-negative integer", v.type_name())),
                };
                let rest = self.lisp.cdr(args)?;
                let pid = self.extract_input_port(rest, call_expr)?;
                let io = match &mut self.io {
                    Some(io) => io,
                    None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                };
                // Read byte-at-a-time into arena bytevector
                let bv = self.lisp.make_bytevector(k, 0)?;
                let mut n = 0;
                for i in 0..k {
                    match io.read_u8(pid) {
                        Ok(b) => {
                            self.lisp.bytevector_set(bv, i, b)?;
                            n += 1;
                        }
                        Err(grift_parser::IoErrorKind::Eof) => break,
                        Err(_) => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                    }
                }
                if n == 0 {
                    self.lisp.eof().map_err(Into::into)
                } else if n < k {
                    // Return a bytevector of the actual bytes read
                    let result = self.lisp.make_bytevector(n, 0)?;
                    for i in 0..n {
                        let elem = self.lisp.bytevector_get(bv, i)?;
                        let byte = match self.lisp.get(elem)? {
                            Value::Number(b) => b as u8,
                            _ => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                        };
                        self.lisp.bytevector_set(result, i, byte)?;
                    }
                    Ok(result)
                } else {
                    Ok(bv)
                }
            }

            Builtin::ReadBytevectorBang => {
                // (read-bytevector! bv) or (read-bytevector! bv port) or (read-bytevector! bv port start) or (read-bytevector! bv port start end)
                let bv_arg = self.lisp.car(args)?;
                let (bv_len, _) = self.get_bytevector(bv_arg, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                let (pid, start, end) = self.extract_port_and_range(rest, self.current_input_port, bv_len, call_expr)?;
                let io = match &mut self.io {
                    Some(io) => io,
                    None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                };
                let read_len = end.saturating_sub(start);
                // Read byte-at-a-time directly into bytevector
                let mut n = 0;
                for i in 0..read_len {
                    match io.read_u8(pid) {
                        Ok(b) => {
                            self.lisp.bytevector_set(bv_arg, start + i, b)?;
                            n += 1;
                        }
                        Err(grift_parser::IoErrorKind::Eof) => break,
                        Err(_) => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                    }
                }
                if n == 0 {
                    self.lisp.eof().map_err(Into::into)
                } else {
                    self.lisp.number(n as isize).map_err(Into::into)
                }
            }

            Builtin::WriteBytevector => {
                // (write-bytevector bv) or (write-bytevector bv port) or (write-bytevector bv port start) or (write-bytevector bv port start end)
                let bv_arg = self.lisp.car(args)?;
                let (bv_len, _) = self.get_bytevector(bv_arg, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                let (pid, start, end) = self.extract_port_and_range(rest, self.current_output_port, bv_len, call_expr)?;
                // Write byte-at-a-time directly from bytevector
                let write_len = end.saturating_sub(start);
                let io = match &mut self.io {
                    Some(io) => io,
                    None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                };
                for i in 0..write_len {
                    let byte_slot = self.lisp.bytevector_get(bv_arg, start + i)?;
                    match self.lisp.get(byte_slot)? {
                        Value::Number(n) => {
                            if io.write_u8(pid, n as u8).is_err() {
                                return Err(EvalError::new(ErrorKind::Generic).with_expr(call_expr));
                            }
                        }
                        _ => return Err(EvalError::new(ErrorKind::Generic).with_expr(call_expr)),
                    }
                }
                self.lisp.void_val().map_err(Into::into)
            }

            // ================================================================
            // Bytevector port operations (R7RS §6.13.2)
            // ================================================================

            Builtin::OpenInputBytevector => {
                // (open-input-bytevector bv)
                let arg = self.lisp.car(args)?;
                let (bv_len, _) = self.get_bytevector(arg, call_expr)?;
                // Write bytes to output bytevector port, then convert to input
                let io = match &mut self.io {
                    Some(io) => io,
                    None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                };
                let tmp_port = io.open_output_bytevector()
                    .map_err(|_| EvalError::new(ErrorKind::Generic).with_expr(call_expr))?;
                for i in 0..bv_len {
                    let byte_slot = self.lisp.bytevector_get(arg, i)?;
                    match self.lisp.get(byte_slot)? {
                        Value::Number(n) => {
                            if io.write_u8(tmp_port, n as u8).is_err() {
                                return Err(EvalError::new(ErrorKind::Generic).with_expr(call_expr));
                            }
                        }
                        _ => return Err(EvalError::new(ErrorKind::Generic).with_expr(call_expr)),
                    }
                }
                let pid = io.output_bytevector_to_input_port(tmp_port)
                    .map_err(|_| EvalError::new(ErrorKind::Generic).with_expr(call_expr))?;
                self.lisp.port(pid).map_err(Into::into)
            }

            Builtin::OpenOutputBytevector => {
                // (open-output-bytevector)
                match &mut self.io {
                    Some(io) => {
                        let pid = io.open_output_bytevector()
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        self.lisp.port(pid).map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::GetOutputBytevector => {
                // (get-output-bytevector port)
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Port(pid) => {
                        match &self.io {
                            Some(io) => {
                                let bytes = io.get_output_bytevector(pid)
                                    .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                                let len = bytes.len();
                                let bv = self.lisp.make_bytevector(len, 0)?;
                                for i in 0..len {
                                    self.lisp.bytevector_set(bv, i, bytes[i])?;
                                }
                                Ok(bv)
                            }
                            None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                        }
                    }
                    v => Err(self.type_error(call_expr, "port", v.type_name())),
                }
            }

            // ================================================================
            // Additional I/O operations (R7RS §6.13.2)
            // ================================================================

            Builtin::WriteStringPort => {
                // (write-string string) or (write-string string port) or (write-string string port start) or (write-string string port start end)
                let str_arg = self.lisp.car(args)?;
                let (str_len, str_data) = self.get_string(str_arg, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                let (pid, start, end) = self.extract_port_and_range(rest, self.current_output_port, str_len, call_expr)?;
                // Write char-at-a-time directly from string data
                let io = match &mut self.io {
                    Some(io) => io,
                    None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                };
                for i in start..end {
                    let char_slot = self.lisp.arena_index_at_offset(str_data, i)?;
                    if let Value::Char(c) = self.lisp.get(char_slot)? {
                        if io.write_char(pid, c).is_err() {
                            return Err(EvalError::new(ErrorKind::Generic).with_expr(call_expr));
                        }
                    }
                }
                self.lisp.void_val().map_err(Into::into)
            }

            Builtin::FlushOutputPort => {
                // (flush-output-port) or (flush-output-port port)
                let pid = self.extract_output_port(args, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        io.flush(pid)
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        self.lisp.void_val().map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            // ================================================================
            // File system operations (R7RS §6.13)
            // ================================================================

            Builtin::FileExistsP => {
                // (file-exists? filename)
                let arg = self.lisp.car(args)?;
                let str_port = self.string_to_output_port(arg, call_expr)?;
                let io = self.io.as_ref()
                    .ok_or_else(|| self.make_error(ErrorKind::Generic, call_expr))?;
                let exists = io.file_exists_from_string_port(str_port)
                    .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                // Close the temp port (need mut ref)
                if let Some(io) = &mut self.io { io.close_port(str_port).ok(); }
                self.lisp.boolean(exists).map_err(Into::into)
            }

            Builtin::DeleteFile => {
                // (delete-file filename)
                let arg = self.lisp.car(args)?;
                let str_port = self.string_to_output_port(arg, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        let result = io.delete_file_from_string_port(str_port);
                        io.close_port(str_port).ok();
                        result.map_err(|_| self.make_error(ErrorKind::FileError, call_expr))?;
                        self.lisp.void_val().map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::FileError, call_expr)),
                }
            }

            Builtin::Load => {
                // (load filename) — handled via trampolining in apply_builtin_trampolined
                // This fallback should not be reached; see apply_load_builtin.
                Err(self.make_error(ErrorKind::Generic, call_expr))
            }

            // ================================================================
            // Process / environment operations (R7RS §6.14)
            // ================================================================

            Builtin::CommandLine => {
                // (command-line) -> list of strings
                match &self.io {
                    Some(io) => {
                        let count = io.command_line_count()
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        // Build list in reverse then reverse it
                        let mut list = self.lisp.nil()?;
                        for i in (0..count).rev() {
                            let s = io.command_line_arg(i)
                                .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                            let str_val = self.lisp.string(s)?;
                            list = self.lisp.cons(str_val, list)?;
                        }
                        Ok(list)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::Exit | Builtin::EmergencyExit => {
                let code = self.extract_exit_code(args, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        let _ = if matches!(builtin, Builtin::Exit) {
                            io.exit_process(code)
                        } else {
                            io.emergency_exit_process(code)
                        };
                        self.lisp.void_val().map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::GetEnvironmentVariable => {
                // (get-environment-variable name) -> string or #f
                let arg = self.lisp.car(args)?;
                let str_port = self.string_to_output_port(arg, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        match io.get_env_var_from_string_port(str_port) {
                            Ok(Some(val)) => {
                                let result = self.lisp.string(val);
                                io.close_port(str_port).ok();
                                result.map_err(Into::into)
                            }
                            Ok(None) => {
                                io.close_port(str_port).ok();
                                self.lisp.false_val().map_err(Into::into)
                            }
                            Err(_) => {
                                io.close_port(str_port).ok();
                                Err(self.make_error(ErrorKind::Generic, call_expr))
                            }
                        }
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::GetEnvironmentVariables => {
                // (get-environment-variables) -> alist of (name . value)
                if !self.cached_environment_variables.is_nil() {
                    return Ok(self.cached_environment_variables);
                }
                // First, get count (requires &mut io)
                let count = match &mut self.io {
                    Some(io) => io.environment_variables_count()
                        .unwrap_or(0),
                    None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                };
                // Now build the alist by iterating (environment_variable_at uses &self)
                let mut list = self.lisp.nil()?;
                for i in (0..count).rev() {
                    let (name, value) = match &self.io {
                        Some(io) => io.environment_variable_at(i)
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?,
                        None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                    };
                    let name_str = self.lisp.string(name)?;
                    let value_str = self.lisp.string(value)?;
                    let pair = self.lisp.cons(name_str, value_str)?;
                    list = self.lisp.cons(pair, list)?;
                }
                self.cached_environment_variables = list;
                Ok(list)
            }

            Builtin::InteractionEnvironment => {
                // (interaction-environment) -> mutable environment object
                let env = self.global_env.0;
                self.lisp.alloc(Value::Environment { env, mutable: true }).map_err(Into::into)
            }

            Builtin::SchemeReportEnvironment | Builtin::NullEnvironment => {
                let version = self.get_int(self.lisp.car(args)?, call_expr)?;
                if version != 5 && version != 7 {
                    return Err(self.type_error(call_expr, "version 5 or 7", "unsupported version"));
                }
                let env = if matches!(builtin, Builtin::SchemeReportEnvironment) {
                    self.global_env.0
                } else {
                    self.lisp.nil()?
                };
                self.lisp.alloc(Value::Environment { env, mutable: false }).map_err(Into::into)
            }

            Builtin::Environmentp => {
                builtin_unary_pred!(self, args, |v: Value| matches!(v, Value::Environment { .. }))
            }

            // ================================================================
            // Bytevector operations (R7RS §6.9)
            // ================================================================

            Builtin::Bytevectorp => {
                // (bytevector? obj) - Check if value is a bytevector
                builtin_unary_pred!(self, args, |v: Value| v.is_bytevector())
            }

            Builtin::MakeBytevector => {
                // (make-bytevector k) or (make-bytevector k byte)
                let k = self.get_int(self.lisp.car(args)?, call_expr)?;
                if k < 0 {
                    return Err(self.type_error(call_expr, "non-negative integer", "negative integer"));
                }
                let rest = self.lisp.cdr(args)?;
                let fill: u8 = if self.lisp.get(rest)?.is_nil() {
                    0
                } else {
                    let byte_val = self.get_int(self.lisp.car(rest)?, call_expr)?;
                    if !(0..=255).contains(&byte_val) {
                        return Err(self.type_error(call_expr, "exact integer 0-255", "out of range"));
                    }
                    byte_val as u8
                };
                self.lisp.make_bytevector(k as usize, fill).map_err(Into::into)
            }

            Builtin::BytevectorLength => {
                // (bytevector-length bytevector) - Get length
                let bv = self.lisp.car(args)?;
                let (len, _) = self.get_bytevector(bv, call_expr)?;
                self.lisp.number(len as isize).map_err(Into::into)
            }

            Builtin::BytevectorU8Ref => {
                // (bytevector-u8-ref bytevector k) - Get byte at index
                extract_args!(self, args, bv, k_idx);
                self.get_bytevector(bv, call_expr)?;
                let k = self.get_int(k_idx, call_expr)?;
                if k < 0 {
                    return Err(self.type_error(call_expr, "non-negative integer", "negative integer"));
                }
                let elem = self.lisp.bytevector_get(bv, k as usize)?;
                Ok(elem)
            }

            Builtin::BytevectorU8Set => {
                // (bytevector-u8-set! bytevector k byte) - Set byte at index
                extract_args!(self, args, bv, k_idx, byte_idx);
                self.get_bytevector(bv, call_expr)?;
                let k = self.get_int(k_idx, call_expr)?;
                if k < 0 {
                    return Err(self.type_error(call_expr, "non-negative integer", "negative integer"));
                }
                let byte_val = self.get_int(byte_idx, call_expr)?;
                if !(0..=255).contains(&byte_val) {
                    return Err(self.type_error(call_expr, "exact integer 0-255", "out of range"));
                }
                self.lisp.bytevector_set(bv, k as usize, byte_val as u8)?;
                self.lisp.void_val().map_err(Into::into)
            }

            Builtin::BytevectorCopy => {
                // (bytevector-copy bytevector [start [end]]) - Copy a bytevector
                let bv = self.lisp.car(args)?;
                self.get_bytevector(bv, call_expr)?;
                let len = self.lisp.bytevector_len(bv)?;
                let rest = self.lisp.cdr(args)?;
                let (start, end) = self.parse_range_args(rest, len, call_expr)?;

                let new_len = end - start;
                let result = self.lisp.make_bytevector(new_len, 0)?;

                for i in 0..new_len {
                    let elem = self.lisp.bytevector_get(bv, start + i)?;
                    let byte = match self.lisp.get(elem)? {
                        Value::Number(n) => n as u8,
                        _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                    };
                    self.lisp.bytevector_set(result, i, byte)?;
                }
                Ok(result)
            }

            Builtin::BytevectorAppend => {
                // (bytevector-append bytevector ...) - Concatenate bytevectors
                // Two-pass: count total length, then allocate and copy directly
                let mut total_len = 0;
                let mut current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            match self.lisp.get(car)? {
                                Value::Bytevector { .. } => {
                                    total_len += self.lisp.bytevector_len(car)?;
                                }
                                v => return Err(self.type_error(call_expr, "bytevector", v.type_name())),
                            }
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }

                let result = self.lisp.make_bytevector(total_len, 0)?;
                let mut offset = 0;
                let mut current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            let len = self.lisp.bytevector_len(car)?;
                            for i in 0..len {
                                let elem = self.lisp.bytevector_get(car, i)?;
                                match self.lisp.get(elem)? {
                                    Value::Number(n) => {
                                        self.lisp.bytevector_set(result, offset, n as u8)?;
                                        offset += 1;
                                    }
                                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                                }
                            }
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
                Ok(result)
            }

            Builtin::Bytevector_ => {
                // (bytevector byte ...) - Create bytevector from given byte values
                // Two-pass: count args, then allocate and fill
                let len = self.lisp.list_len(args)?;

                let result = self.lisp.make_bytevector(len, 0)?;
                let mut current = args;
                for i in 0..len {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    let byte_val = self.get_int(car, call_expr)?;
                    if !(0..=255).contains(&byte_val) {
                        return Err(self.type_error(call_expr, "exact integer 0-255", "out of range"));
                    }
                    self.lisp.bytevector_set(result, i, byte_val as u8)?;
                    current = cdr;
                }
                Ok(result)
            }

            Builtin::BytevectorCopyBang => {
                // (bytevector-copy! to at from [start [end]])
                let to = self.lisp.car(args)?;
                let rest1 = self.lisp.cdr(args)?;
                let at_val = self.lisp.car(rest1)?;
                let rest2 = self.lisp.cdr(rest1)?;
                let from = self.lisp.car(rest2)?;
                let rest3 = self.lisp.cdr(rest2)?;

                // Validate types
                self.get_bytevector(to, call_expr)?;
                self.get_bytevector(from, call_expr)?;

                let at = self.get_int(at_val, call_expr)?;
                if at < 0 {
                    return Err(self.type_error(call_expr, "non-negative integer", "negative integer"));
                }
                let at = at as usize;

                let from_len = self.lisp.bytevector_len(from)?;
                let to_len = self.lisp.bytevector_len(to)?;
                let (start, end) = self.parse_range_args(rest3, from_len, call_expr)?;

                let copy_len = end - start;
                if at + copy_len > to_len {
                    return Err(self.make_error(ErrorKind::TypeError, call_expr));
                }

                // Copy bytes from `from[start..end]` to `to[at..at+copy_len]`
                // Handle overlapping regions by copying in the right direction
                if to == from && at > start {
                    // Copy backwards to handle forward overlap
                    for i in (0..copy_len).rev() {
                        let elem = self.lisp.bytevector_get(from, start + i)?;
                        let byte = match self.lisp.get(elem)? {
                            Value::Number(n) => n as u8,
                            _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                        };
                        self.lisp.bytevector_set(to, at + i, byte)?;
                    }
                } else {
                    for i in 0..copy_len {
                        let elem = self.lisp.bytevector_get(from, start + i)?;
                        let byte = match self.lisp.get(elem)? {
                            Value::Number(n) => n as u8,
                            _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                        };
                        self.lisp.bytevector_set(to, at + i, byte)?;
                    }
                }
                self.lisp.void_val().map_err(Into::into)
            }

            Builtin::Utf8ToString => {
                // (utf8->string bytevector [start [end]]) - Decode UTF-8 bytevector to string
                // Manual UTF-8 decoding from bytevector bytes, building a reversed cons list of chars
                let bv = self.lisp.car(args)?;
                self.get_bytevector(bv, call_expr)?;
                let len = self.lisp.bytevector_len(bv)?;
                let rest = self.lisp.cdr(args)?;
                let (start, end) = self.parse_range_args(rest, len, call_expr)?;

                let nil = self.lisp.nil()?;
                let mut collected = nil;
                let mut char_count = 0;
                let mut i = start;
                while i < end {
                    let b0 = self.get_bv_byte(bv, i)?;
                    let (ch, advance) = if b0 < 0x80 {
                        (b0 as u32, 1)
                    } else if b0 & 0xE0 == 0xC0 {
                        if i + 1 >= end { return Err(self.type_error(call_expr, "valid UTF-8", "invalid byte sequence")); }
                        let b1 = self.get_bv_byte(bv, i + 1)?;
                        if b1 & 0xC0 != 0x80 { return Err(self.type_error(call_expr, "valid UTF-8", "invalid byte sequence")); }
                        (((b0 as u32 & 0x1F) << 6) | (b1 as u32 & 0x3F), 2)
                    } else if b0 & 0xF0 == 0xE0 {
                        if i + 2 >= end { return Err(self.type_error(call_expr, "valid UTF-8", "invalid byte sequence")); }
                        let b1 = self.get_bv_byte(bv, i + 1)?;
                        let b2 = self.get_bv_byte(bv, i + 2)?;
                        if b1 & 0xC0 != 0x80 || b2 & 0xC0 != 0x80 { return Err(self.type_error(call_expr, "valid UTF-8", "invalid byte sequence")); }
                        (((b0 as u32 & 0x0F) << 12) | ((b1 as u32 & 0x3F) << 6) | (b2 as u32 & 0x3F), 3)
                    } else if b0 & 0xF8 == 0xF0 {
                        if i + 3 >= end { return Err(self.type_error(call_expr, "valid UTF-8", "invalid byte sequence")); }
                        let b1 = self.get_bv_byte(bv, i + 1)?;
                        let b2 = self.get_bv_byte(bv, i + 2)?;
                        let b3 = self.get_bv_byte(bv, i + 3)?;
                        if b1 & 0xC0 != 0x80 || b2 & 0xC0 != 0x80 || b3 & 0xC0 != 0x80 { return Err(self.type_error(call_expr, "valid UTF-8", "invalid byte sequence")); }
                        (((b0 as u32 & 0x07) << 18) | ((b1 as u32 & 0x3F) << 12) | ((b2 as u32 & 0x3F) << 6) | (b3 as u32 & 0x3F), 4)
                    } else {
                        return Err(self.type_error(call_expr, "valid UTF-8", "invalid byte sequence"));
                    };
                    let c = char::from_u32(ch)
                        .ok_or_else(|| self.type_error(call_expr, "valid UTF-8", "invalid byte sequence"))?;
                    let ch_val = self.lisp.char(c)?;
                    collected = self.lisp.cons(ch_val, collected)?;
                    char_count += 1;
                    i += advance;
                }
                self.reversed_char_cons_to_string(collected, char_count)
            }

            Builtin::StringToUtf8 => {
                // (string->utf8 string [start [end]]) - Encode string as UTF-8 bytevector
                let str_idx = self.lisp.car(args)?;
                let (len, data) = self.get_string(str_idx, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                let (start, end) = self.parse_range_args(rest, len, call_expr)?;

                // Two-pass: count total UTF-8 bytes needed, then allocate and fill
                let mut byte_len = 0;
                for i in start..end {
                    let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                    match self.lisp.get(char_slot)? {
                        Value::Char(c) => byte_len += c.len_utf8(),
                        _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                    }
                }

                let result = self.lisp.make_bytevector(byte_len, 0)?;
                let mut offset = 0;
                for i in start..end {
                    let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                    match self.lisp.get(char_slot)? {
                        Value::Char(c) => {
                            let mut tmp = [0u8; 4];
                            let encoded = c.encode_utf8(&mut tmp);
                            for &b in encoded.as_bytes() {
                                self.lisp.bytevector_set(result, offset, b)?;
                                offset += 1;
                            }
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                    }
                }
                Ok(result)
            }

            // ============================================================
            // Time procedures (R7RS §6.13.3)
            // ============================================================

            Builtin::CurrentSecond => {
                // (current-second) - Returns inexact seconds since epoch
                match &self.io {
                    Some(io) => {
                        let secs = io.current_second()
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        self.lisp.float(secs as fsize).map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::CurrentJiffy => {
                // (current-jiffy) - Returns exact integer jiffies
                self.time_i64_to_value(|io| io.current_jiffy(), call_expr)
            }

            Builtin::JiffiesPerSecond => {
                // (jiffies-per-second) - Returns exact integer
                self.time_i64_to_value(|io| io.jiffies_per_second(), call_expr)
            }

            // ============================================================
            // Error predicates (R7RS §6.11)
            // ============================================================

            Builtin::ReadErrorP | Builtin::FileErrorP => {
                let tag = if matches!(builtin, Builtin::ReadErrorP) { "read-error" } else { "file-error" };
                let arg = self.lisp.car(args)?;
                let matches = match self.lisp.get(arg)? {
                    Value::ErrorObject { irritants_and_type, .. } => {
                        let err_type = self.lisp.cdr(irritants_and_type)?;
                        self.lisp.symbol_matches(err_type, tag).unwrap_or(false)
                    }
                    _ => false,
                };
                self.lisp.boolean(matches).map_err(Into::into)
            }

            // ============================================================
            // Vector-String conversion (R7RS §6.8)
            // ============================================================

            Builtin::VectorToString => {
                // (vector->string vector [start [end]])
                let vec_idx = self.lisp.car(args)?;
                match self.lisp.get(vec_idx)? {
                    Value::Array { len, data } => {
                        let rest = self.lisp.cdr(args)?;
                        let (start, end) = self.parse_range_args(rest, len, call_expr)?;

                        const MAX_LEN: usize = 1024;
                        let count = end - start;
                        if count > MAX_LEN {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        let mut chars = ['\0'; MAX_LEN];
                        for i in 0..count {
                            let slot = self.lisp.arena_index_at_offset(data, start + i)?;
                            match self.lisp.get(slot)? {
                                Value::Char(c) => chars[i] = c,
                                _ => return Err(self.type_error(call_expr, "character", self.lisp.get(slot)?.type_name())),
                            }
                        }
                        self.lisp.string_from_chars(&chars[..count]).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "vector", v.type_name())),
                }
            }

            Builtin::StringToVector => {
                // (string->vector string [start [end]])
                let str_idx = self.lisp.car(args)?;
                let (len, data) = self.get_string(str_idx, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                let (start, end) = self.parse_range_args(rest, len, call_expr)?;

                let count = end - start;
                // Create a vector and fill with characters
                let default = self.lisp.char('\0')?;
                let result = self.lisp.make_array(count, default)?;
                for i in 0..count {
                    let char_slot = self.lisp.arena_index_at_offset(data, start + i)?;
                    let ch = self.lisp.get(char_slot)?;
                    match ch {
                        Value::Char(c) => {
                            let char_val = self.lisp.char(c)?;
                            self.lisp.array_set(result, i, char_val)?;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                    }
                }
                Ok(result)
            }

            // ================================================================
            // Transcendental functions (R7RS §6.2.6) — powered by libm
            // ================================================================

            Builtin::Exp => self.apply_unary_transcendental(args, call_expr, libm::exp),
            Builtin::Sin => self.apply_unary_transcendental(args, call_expr, libm::sin),
            Builtin::Cos => self.apply_unary_transcendental(args, call_expr, libm::cos),
            Builtin::Tan => self.apply_unary_transcendental(args, call_expr, libm::tan),
            Builtin::Asin => self.apply_unary_transcendental(args, call_expr, libm::asin),
            Builtin::Acos => self.apply_unary_transcendental(args, call_expr, libm::acos),

            Builtin::Log => {
                let z = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                if self.lisp.get(rest)?.is_nil() {
                    self.lisp.float(libm::log(z as f64) as fsize).map_err(Into::into)
                } else {
                    let base = self.get_num_as_fsize(self.lisp.car(rest)?, call_expr)?;
                    self.lisp.float((libm::log(z as f64) / libm::log(base as f64)) as fsize).map_err(Into::into)
                }
            }

            Builtin::Atan => {
                let y = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                if self.lisp.get(rest)?.is_nil() {
                    self.lisp.float(libm::atan(y as f64) as fsize).map_err(Into::into)
                } else {
                    let x = self.get_num_as_fsize(self.lisp.car(rest)?, call_expr)?;
                    self.lisp.float(libm::atan2(y as f64, x as f64) as fsize).map_err(Into::into)
                }
            }

            // ================================================================
            // Division procedures (R7RS §6.2.6)
            // ================================================================

            Builtin::FloorQuotient | Builtin::FloorRemainder
            | Builtin::TruncateQuotient | Builtin::TruncateRemainder
            | Builtin::CeilingQuotient | Builtin::CeilingRemainder
            | Builtin::RoundQuotient | Builtin::RoundRemainder => {
                let round_fn: fn(f64) -> f64 = match builtin {
                    Builtin::FloorQuotient | Builtin::FloorRemainder => libm::floor,
                    Builtin::CeilingQuotient | Builtin::CeilingRemainder => libm::ceil,
                    Builtin::RoundQuotient | Builtin::RoundRemainder => libm::rint,
                    _ => libm::trunc,
                };
                let (q, r) = self.division_op(args, call_expr, round_fn)?;
                let val = match builtin {
                    Builtin::FloorQuotient | Builtin::TruncateQuotient
                    | Builtin::CeilingQuotient | Builtin::RoundQuotient => q,
                    _ => r,
                };
                self.return_exact_if_both_exact(args, val, call_expr)
            }

            Builtin::FloorDiv => self.division_values(args, call_expr, libm::floor),
            Builtin::TruncateDiv => self.division_values(args, call_expr, libm::trunc),
            Builtin::CeilingDiv => self.division_values(args, call_expr, libm::ceil),
            Builtin::RoundDiv => self.division_values(args, call_expr, libm::rint),

            Builtin::EuclideanQuotient | Builtin::EuclideanRemainder => {
                let (q, r) = self.euclidean_division_op(args, call_expr)?;
                let val = if matches!(builtin, Builtin::EuclideanQuotient) { q } else { r };
                self.return_exact_if_both_exact(args, val, call_expr)
            }
            Builtin::EuclideanDiv => self.euclidean_division_values(args, call_expr),

            Builtin::BalancedQuotient | Builtin::BalancedRemainder => {
                let (q, r) = self.balanced_division_op(args, call_expr)?;
                let val = if matches!(builtin, Builtin::BalancedQuotient) { q } else { r };
                self.return_exact_if_both_exact(args, val, call_expr)
            }
            Builtin::BalancedDiv => self.balanced_division_values(args, call_expr),

            // ================================================================
            // Rational number operations (R7RS §6.2.6)
            // ================================================================

            Builtin::Numerator | Builtin::Denominator => {
                let is_num = matches!(builtin, Builtin::Numerator);
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(n) => {
                        self.lisp.number(if is_num { n } else { 1 }).map_err(Into::into)
                    }
                    Value::Float(f) => {
                        if !f.is_finite() {
                            return Err(self.type_error(call_expr, "finite number", "infinite or nan"));
                        }
                        let (num, den) = float_to_rational(f as f64);
                        self.lisp.float(if is_num { num } else { den } as fsize).map_err(Into::into)
                    }
                    Value::Rational { num, denom } => {
                        self.lisp.number(if is_num { num } else { denom }).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }

            Builtin::Rationalize => {
                let first_arg = self.lisp.car(args)?;
                let x = self.get_num_as_fsize(first_arg, call_expr)?;
                let tol = self.get_num_as_fsize(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                let (num, den) = rationalize_impl(x as f64, libm::fabs(tol as f64));
                // Check if both arguments are exact
                let first_exact = match self.lisp.get(first_arg)? {
                    Value::Number(_) | Value::Rational { .. } | Value::BigNum { .. } => true,
                    _ => false,
                };
                if first_exact {
                    // Return exact result
                    if den == 1.0 {
                        self.lisp.number(num as isize).map_err(Into::into)
                    } else {
                        self.lisp.rational(num as isize, den as isize).map_err(Into::into)
                    }
                } else {
                    self.lisp.float((num / den) as fsize).map_err(Into::into)
                }
            }

            // ================================================================
            // Exact integer square root (R7RS §6.2.6)
            // ================================================================

            Builtin::ExactIntegerSqrt => {
                let arg_idx = self.lisp.car(args)?;
                let arg_val = self.lisp.get(arg_idx)?;
                match arg_val {
                    Value::BigNum { .. } => {
                        // BigNum path
                        let (limbs, len, neg) = self.lisp.bignum_limbs(arg_idx)?;
                        if neg {
                            return Err(self.type_error(call_expr, "non-negative integer", "negative integer"));
                        }
                        let mut n_big = crate::bignum::BigNumBuf::zero();
                        n_big.limbs[..len].copy_from_slice(&limbs[..len]);
                        n_big.len = len;
                        let s = n_big.isqrt();
                        let s_sq = s.mul(&s);
                        let r = n_big.sub(&s_sq);
                        let sv = match s.to_isize() {
                            Some(n) => self.lisp.number(n)?,
                            None => self.lisp.bignum_from_limbs(&s.limbs[..s.len], s.negative)?,
                        };
                        let rv = match r.to_isize() {
                            Some(n) => self.lisp.number(n)?,
                            None => self.lisp.bignum_from_limbs(&r.limbs[..r.len], r.negative)?,
                        };
                        let nil = self.lisp.nil()?;
                        let tail = self.lisp.cons(rv, nil)?;
                        self.lisp.cons(sv, tail).map_err(Into::into)
                    }
                    Value::Number(n) => {
                        if n < 0 {
                            return Err(self.type_error(call_expr, "non-negative integer", "negative integer"));
                        }
                        let n_u = n as usize;
                        let s = isqrt(n_u);
                        let r = n_u - s * s;
                        let sv = self.lisp.number(s as isize)?;
                        let rv = self.lisp.number(r as isize)?;
                        let nil = self.lisp.nil()?;
                        let tail = self.lisp.cons(rv, nil)?;
                        self.lisp.cons(sv, tail).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "integer", v.type_name())),
                }
            }

            // ================================================================
            // Complex number operations (R7RS §6.2.6)
            // ================================================================

            Builtin::MakeRectangular => {
                let a = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
                let b = self.get_num_as_fsize(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                self.lisp.complex(a, b).map_err(Into::into)
            }

            Builtin::MakePolar => {
                let r = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
                let theta = self.get_num_as_fsize(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                let a = (r as f64) * libm::cos(theta as f64);
                let b = (r as f64) * libm::sin(theta as f64);
                if libm::fabs(b) < f64::EPSILON {
                    return self.lisp.float(a as fsize).map_err(Into::into);
                }
                self.lisp.complex(a as fsize, b as fsize).map_err(Into::into)
            }

            Builtin::RealPart => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(n) => self.lisp.number(n).map_err(Into::into),
                    Value::Float(f) => self.lisp.float(f).map_err(Into::into),
                    Value::Rational { num, denom } => self.lisp.rational(num, denom).map_err(Into::into),
                    Value::Complex { real, .. } => self.lisp.float(real).map_err(Into::into),
                    Value::Cons { .. } => {
                        // Check if tagged complex: (complex rect re im)
                        if self.is_complex_tagged(arg)? {
                            let cdr1 = self.lisp.cdr(arg)?; // (rect re im)
                            let cdr2 = self.lisp.cdr(cdr1)?; // (re im)
                            self.lisp.car(cdr2).map_err(Into::into)
                        } else {
                            Err(self.type_error(call_expr, "number", "pair"))
                        }
                    }
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }

            Builtin::ImagPart => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(_) | Value::Float(_) | Value::Rational { .. } => self.lisp.number(0).map_err(Into::into),
                    Value::Complex { imag, .. } => self.lisp.float(imag).map_err(Into::into),
                    Value::Cons { .. } => {
                        if self.is_complex_tagged(arg)? {
                            let cdr1 = self.lisp.cdr(arg)?; // (rect re im)
                            let cdr2 = self.lisp.cdr(cdr1)?; // (re im)
                            let cdr3 = self.lisp.cdr(cdr2)?; // (im)
                            self.lisp.car(cdr3).map_err(Into::into)
                        } else {
                            Err(self.type_error(call_expr, "number", "pair"))
                        }
                    }
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }

            Builtin::Magnitude => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(n) => {
                        let abs_n = if n < 0 { -n } else { n };
                        self.lisp.number(abs_n).map_err(Into::into)
                    }
                    Value::Float(f) => self.lisp.float(libm::fabs(f as f64) as fsize).map_err(Into::into),
                    Value::Rational { num, denom } => {
                        let f = num as fsize / denom as fsize;
                        self.lisp.float(libm::fabs(f as f64) as fsize).map_err(Into::into)
                    }
                    Value::Complex { real, imag } => {
                        let mag = libm::sqrt((real as f64) * (real as f64) + (imag as f64) * (imag as f64));
                        self.lisp.float(mag as fsize).map_err(Into::into)
                    }
                    Value::Cons { .. } => {
                        if self.is_complex_tagged(arg)? {
                            let re = self.complex_real_f(arg, call_expr)?;
                            let im = self.complex_imag_f(arg, call_expr)?;
                            let mag = libm::sqrt(re * re + im * im);
                            self.lisp.float(mag as fsize).map_err(Into::into)
                        } else {
                            Err(self.type_error(call_expr, "number", "pair"))
                        }
                    }
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }

            Builtin::Angle => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(n) => {
                        let angle = if n >= 0 { 0.0 } else { core::f64::consts::PI as fsize };
                        self.lisp.float(angle).map_err(Into::into)
                    }
                    Value::Float(f) => {
                        let angle = if f >= 0.0 { 0.0 } else { core::f64::consts::PI as fsize };
                        self.lisp.float(angle).map_err(Into::into)
                    }
                    Value::Rational { num, .. } => {
                        let angle = if num >= 0 { 0.0 } else { core::f64::consts::PI as fsize };
                        self.lisp.float(angle).map_err(Into::into)
                    }
                    Value::Complex { real, imag } => {
                        self.lisp.float(libm::atan2(imag as f64, real as f64) as fsize).map_err(Into::into)
                    }
                    Value::Cons { .. } => {
                        if self.is_complex_tagged(arg)? {
                            let re = self.complex_real_f(arg, call_expr)?;
                            let im = self.complex_imag_f(arg, call_expr)?;
                            self.lisp.float(libm::atan2(im, re) as fsize).map_err(Into::into)
                        } else {
                            Err(self.type_error(call_expr, "number", "pair"))
                        }
                    }
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }

            // These are handled in apply_builtin_trampolined (need continuation support)
            Builtin::Values | Builtin::CallWithValues | Builtin::Apply |
            Builtin::CallCc | Builtin::CallWithCurrentContinuation |
            Builtin::DynamicWind | Builtin::WithExceptionHandler |
            Builtin::RaiseBuiltin | Builtin::RaiseContinuable |
            Builtin::EvalBuiltin | Builtin::EnvironmentBuiltin => {
                unreachable!("handled in apply_builtin_trampolined")
            }
        }
    }
    pub(super) fn apply_binary_builtin(&mut self, builtin: Builtin, a: ArenaIndex, b: ArenaIndex, call_expr: ArenaIndex) 
        -> EvalResult 
    {
        match builtin {
            // Arithmetic with overflow check (mixed int/float support)
            Builtin::Add => binary_int_op!(self, a, b, call_expr, 
                |x: isize, y: isize| x.checked_add(y),
                |x: fsize, y: fsize| x + y),
            Builtin::Sub => binary_int_op!(self, a, b, call_expr, 
                |x: isize, y: isize| x.checked_sub(y),
                |x: fsize, y: fsize| x - y),
            Builtin::Mul => {
                let val_a = self.lisp.get(a)?;
                let val_b = self.lisp.get(b)?;
                match (val_a, val_b) {
                    (Value::Number(x), Value::Number(y)) => {
                        match x.checked_mul(y) {
                            Some(n) => self.lisp.number(n).map_err(Into::into),
                            None => {
                                // Overflow: produce BigNum instead of float
                                let bx = crate::bignum::BigNumBuf::from_isize(x);
                                let by = crate::bignum::BigNumBuf::from_isize(y);
                                let result = bx.mul(&by);
                                self.lisp.bignum_from_limbs(&result.limbs[..result.len], result.negative).map_err(Into::into)
                            }
                        }
                    }
                    // BigNum * Number
                    (Value::BigNum { .. }, Value::Number(y)) | (Value::Number(y), Value::BigNum { .. }) => {
                        let big_idx = if matches!(val_a, Value::BigNum { .. }) { a } else { b };
                        let (limbs, len, neg) = self.lisp.bignum_limbs(big_idx)?;
                        let mut buf = crate::bignum::BigNumBuf::zero();
                        buf.limbs[..len].copy_from_slice(&limbs[..len]);
                        buf.len = len;
                        buf.negative = neg;
                        let other = crate::bignum::BigNumBuf::from_isize(y);
                        let result = buf.mul(&other);
                        match result.to_isize() {
                            Some(n) => self.lisp.number(n).map_err(Into::into),
                            None => self.lisp.bignum_from_limbs(&result.limbs[..result.len], result.negative).map_err(Into::into),
                        }
                    }
                    // BigNum * BigNum
                    (Value::BigNum { .. }, Value::BigNum { .. }) => {
                        let (la, lena, nega) = self.lisp.bignum_limbs(a)?;
                        let (lb, lenb, negb) = self.lisp.bignum_limbs(b)?;
                        let mut ba = crate::bignum::BigNumBuf::zero();
                        ba.limbs[..lena].copy_from_slice(&la[..lena]);
                        ba.len = lena; ba.negative = nega;
                        let mut bb = crate::bignum::BigNumBuf::zero();
                        bb.limbs[..lenb].copy_from_slice(&lb[..lenb]);
                        bb.len = lenb; bb.negative = negb;
                        let result = ba.mul(&bb);
                        match result.to_isize() {
                            Some(n) => self.lisp.number(n).map_err(Into::into),
                            None => self.lisp.bignum_from_limbs(&result.limbs[..result.len], result.negative).map_err(Into::into),
                        }
                    }
                    // BigNum * Float or Float * BigNum
                    (Value::BigNum { .. }, Value::Float(y)) => {
                        let (limbs, len, neg) = self.lisp.bignum_limbs(a)?;
                        let mut buf = crate::bignum::BigNumBuf::zero();
                        buf.limbs[..len].copy_from_slice(&limbs[..len]);
                        buf.len = len; buf.negative = neg;
                        self.lisp.float(buf.to_f64() as fsize * y).map_err(Into::into)
                    }
                    (Value::Float(x), Value::BigNum { .. }) => {
                        let (limbs, len, neg) = self.lisp.bignum_limbs(b)?;
                        let mut buf = crate::bignum::BigNumBuf::zero();
                        buf.limbs[..len].copy_from_slice(&limbs[..len]);
                        buf.len = len; buf.negative = neg;
                        self.lisp.float(x * buf.to_f64() as fsize).map_err(Into::into)
                    }
                    (Value::Number(x), Value::Float(y)) => self.lisp.float(x as fsize * y).map_err(Into::into),
                    (Value::Float(x), Value::Number(y)) => self.lisp.float(x * y as fsize).map_err(Into::into),
                    (Value::Float(x), Value::Float(y)) => self.lisp.float(x * y).map_err(Into::into),
                    (Value::Rational { num, denom }, Value::Number(y)) => {
                        match num.checked_mul(y) {
                            Some(nn) => self.lisp.rational(nn, denom).map_err(Into::into),
                            None => self.lisp.float((num as fsize / denom as fsize) * y as fsize).map_err(Into::into),
                        }
                    }
                    (Value::Number(x), Value::Rational { num, denom }) => {
                        match x.checked_mul(num) {
                            Some(nn) => self.lisp.rational(nn, denom).map_err(Into::into),
                            None => self.lisp.float(x as fsize * (num as fsize / denom as fsize)).map_err(Into::into),
                        }
                    }
                    (Value::Rational { num: n1, denom: d1 }, Value::Rational { num: n2, denom: d2 }) => {
                        match (n1.checked_mul(n2), d1.checked_mul(d2)) {
                            (Some(nn), Some(nd)) => self.lisp.rational(nn, nd).map_err(Into::into),
                            _ => self.lisp.float((n1 as fsize / d1 as fsize) * (n2 as fsize / d2 as fsize)).map_err(Into::into),
                        }
                    }
                    (Value::Rational { num, denom }, Value::Float(y)) => {
                        self.lisp.float((num as fsize / denom as fsize) * y).map_err(Into::into)
                    }
                    (Value::Float(x), Value::Rational { num, denom }) => {
                        self.lisp.float(x * (num as fsize / denom as fsize)).map_err(Into::into)
                    }
                    (v, _) if !v.is_number() => Err(self.type_error(call_expr, "number", v.type_name())),
                    (_, v) => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            // Division: exact integer division produces rationals
            Builtin::Div => {
                let val_a = self.lisp.get(a)?;
                let val_b = self.lisp.get(b)?;
                match (val_a, val_b) {
                    (Value::Number(x), Value::Number(y)) => {
                        if y == 0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                        self.lisp.rational(x, y).map_err(Into::into)
                    }
                    (Value::Number(x), Value::Float(y)) => {
                        self.lisp.float(x as fsize / y).map_err(Into::into)
                    }
                    (Value::Float(x), Value::Number(y)) => {
                        self.lisp.float(x / y as fsize).map_err(Into::into)
                    }
                    (Value::Float(x), Value::Float(y)) => {
                        self.lisp.float(x / y).map_err(Into::into)
                    }
                    (Value::Rational { num: n1, denom: d1 }, Value::Number(y)) => {
                        if y == 0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                        match d1.checked_mul(y) {
                            Some(new_d) => self.lisp.rational(n1, new_d).map_err(Into::into),
                            None => self.lisp.float(n1 as fsize / (d1 as fsize * y as fsize)).map_err(Into::into),
                        }
                    }
                    (Value::Number(x), Value::Rational { num: n2, denom: d2 }) => {
                        if n2 == 0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                        match x.checked_mul(d2) {
                            Some(new_n) => self.lisp.rational(new_n, n2).map_err(Into::into),
                            None => self.lisp.float(x as fsize * d2 as fsize / n2 as fsize).map_err(Into::into),
                        }
                    }
                    (Value::Rational { num: n1, denom: d1 }, Value::Rational { num: n2, denom: d2 }) => {
                        if n2 == 0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                        match (n1.checked_mul(d2), d1.checked_mul(n2)) {
                            (Some(new_num), Some(new_denom)) => self.lisp.rational(new_num, new_denom).map_err(Into::into),
                            _ => self.lisp.float((n1 as fsize * d2 as fsize) / (d1 as fsize * n2 as fsize)).map_err(Into::into),
                        }
                    }
                    (Value::Rational { num, denom }, Value::Float(y)) => {
                        self.lisp.float(num as fsize / denom as fsize / y).map_err(Into::into)
                    }
                    (Value::Float(x), Value::Rational { num, denom }) => {
                        self.lisp.float(x * denom as fsize / num as fsize).map_err(Into::into)
                    }
                    // BigNum / BigNum
                    (Value::BigNum { .. }, Value::BigNum { .. }) => {
                        let nil = self.lisp.nil()?;
                        let rest = self.lisp.cons(b, nil)?;
                        self.bignum_fold_div(a, rest, call_expr)
                    }
                    // BigNum / Number
                    (Value::BigNum { .. }, Value::Number(y)) => {
                        if y == 0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                        let nil = self.lisp.nil()?;
                        let b_idx = self.lisp.number(y)?;
                        let rest = self.lisp.cons(b_idx, nil)?;
                        self.bignum_fold_div(a, rest, call_expr)
                    }
                    // Number / BigNum
                    (Value::Number(_), Value::BigNum { .. }) => {
                        let b_buf = self.load_bignum_buf(b)?;
                        if b_buf.is_zero() { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                        // Small number / large number → rational (likely 0/n or small fraction)
                        let nil = self.lisp.nil()?;
                        let rest = self.lisp.cons(b, nil)?;
                        // Convert a to bignum and use bignum_fold_div
                        let a_val = match val_a { Value::Number(x) => x, _ => 0 };
                        let a_buf = crate::bignum::BigNumBuf::from_isize(a_val);
                        let a_big = self.lisp.bignum_from_limbs(&a_buf.limbs[..a_buf.len], a_buf.negative)?;
                        self.bignum_fold_div(a_big, rest, call_expr)
                    }
                    // BigNum / Float or Float / BigNum
                    (Value::BigNum { .. }, Value::Float(y)) => {
                        let x = self.bignum_to_f64(a)?;
                        self.lisp.float((x as fsize) / y).map_err(Into::into)
                    }
                    (Value::Float(x), Value::BigNum { .. }) => {
                        let y = self.bignum_to_f64(b)?;
                        self.lisp.float(x / (y as fsize)).map_err(Into::into)
                    }
                    (v, _) if !v.is_number() => Err(self.type_error(call_expr, "number", v.type_name())),
                    (_, v) => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            Builtin::Modulo => binary_div_op!(self, a, b, call_expr, 
                |x, y| ((x % y) + y) % y,
                |x: fsize, y: fsize| ((x % y) + y) % y),
            Builtin::Remainder => binary_div_op!(self, a, b, call_expr, 
                |x, y| x % y,
                |x: fsize, y: fsize| x % y),
            
            // Comparisons
            Builtin::Lt => binary_int_cmp!(self, a, b, call_expr, |x, y| x < y),
            Builtin::Gt => binary_int_cmp!(self, a, b, call_expr, |x, y| x > y),
            Builtin::Le => binary_int_cmp!(self, a, b, call_expr, |x, y| x <= y),
            Builtin::Ge => binary_int_cmp!(self, a, b, call_expr, |x, y| x >= y),
            Builtin::NumEq => {
                // Use precision-aware equality check
                let rest = self.lisp.cons(b, self.lisp.nil()?)?;
                let args = self.lisp.cons(a, rest)?;
                self.compare_numbers_eq(args, call_expr)
            }
            Builtin::EqP | Builtin::EqvP => self.eqv_compare(a, b),
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
    pub(super) fn get_int(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<isize, EvalError> {
        match self.lisp.get(idx)? {
            Value::Number(n) => Ok(n),
            Value::Float(f) => {
                // Accept floats that represent exact integers
                if f.is_finite() && f == (f as isize as fsize) {
                    Ok(f as isize)
                } else {
                    Err(self.type_error(call_expr, "integer", "inexact number"))
                }
            }
            v => Err(self.type_error(call_expr, "integer", v.type_name())),
        }
    }
    
    /// Apply a unary transcendental function (e.g. sin, cos, exp) to a single argument.
    fn apply_unary_transcendental(&self, args: ArenaIndex, call_expr: ArenaIndex, f: fn(f64) -> f64) -> EvalResult {
        let x = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
        self.lisp.float(f(x as f64) as fsize).map_err(Into::into)
    }

    /// Build a string from a reversed cons list of Char values.
    /// `collected` is a reversed cons list and `count` is the number of elements.
    fn reversed_char_cons_to_string(&self, collected: ArenaIndex, count: usize) -> EvalResult {
        let result = self.lisp.make_string(count, '\0')?;
        let mut cursor = collected;
        let mut i = count;
        while let Value::Cons { .. } = self.lisp.get(cursor)? {
            i -= 1;
            let ch = self.lisp.car(cursor)?;
            match self.lisp.get(ch)? {
                Value::Char(c) => self.lisp.string_set(result, i, c)?,
                _ => {} // caller guarantees only Char values
            }
            cursor = self.lisp.cdr(cursor)?;
        }
        Ok(result)
    }

    /// Copy a range of characters from string data into a new string.
    fn copy_string_range(&self, data: ArenaIndex, start: usize, count: usize, call_expr: ArenaIndex) -> EvalResult {
        let result = self.lisp.make_string(count, '\0')?;
        for i in 0..count {
            let char_slot = self.lisp.arena_index_at_offset(data, start + i)?;
            match self.lisp.get(char_slot)? {
                Value::Char(c) => self.lisp.string_set(result, i, c)?,
                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
            }
        }
        Ok(result)
    }

    /// Extract a single byte from a bytevector at the given index.
    fn get_bv_byte(&self, bv: ArenaIndex, index: usize) -> Result<u8, EvalError> {
        let elem = self.lisp.bytevector_get(bv, index)?;
        match self.lisp.get(elem)? {
            Value::Number(n) => Ok(n as u8),
            _ => Err(EvalError::new(ErrorKind::TypeError)),
        }
    }

    /// Get number as fsize from already-evaluated value
    pub(super) fn get_num_as_fsize(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<fsize, EvalError> {
        match self.lisp.get(idx)? {
            Value::Number(n) => Ok(n as fsize),
            Value::Float(f) => Ok(f),
            Value::Rational { num, denom } => Ok(num as fsize / denom as fsize),
            Value::Complex { .. } => Err(self.type_error(call_expr, "real number", "complex")),
            Value::BigNum { .. } => {
                let (limbs, len, neg) = self.lisp.bignum_limbs(idx)?;
                let mut buf = crate::bignum::BigNumBuf::zero();
                buf.limbs[..len].copy_from_slice(&limbs[..len]);
                buf.len = len; buf.negative = neg;
                Ok(buf.to_f64() as fsize)
            }
            v => Err(self.type_error(call_expr, "number", v.type_name())),
        }
    }
    
    /// Get character from already-evaluated value
    pub(super) fn get_char(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<char, EvalError> {
        match self.lisp.get(idx)? {
            Value::Char(c) => Ok(c),
            v => Err(self.type_error(call_expr, "char", v.type_name())),
        }
    }

    /// Get string length and data from already-evaluated value
    pub(super) fn get_string(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<(usize, ArenaIndex), EvalError> {
        match self.lisp.get(idx)? {
            Value::String { len, data } => Ok((len, data)),
            v => Err(self.type_error(call_expr, "string", v.type_name())),
        }
    }

    /// Get bytevector length and data from already-evaluated value
    pub(super) fn get_bytevector(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<(usize, ArenaIndex), EvalError> {
        match self.lisp.get(idx)? {
            Value::Bytevector { len, data } => Ok((len, data)),
            v => Err(self.type_error(call_expr, "bytevector", v.type_name())),
        }
    }
    
    /// Check if a value is a tagged complex number: (complex rect re im)
    fn is_complex_tagged(&self, idx: ArenaIndex) -> Result<bool, EvalError> {
        match self.lisp.get(idx)? {
            Value::Cons { .. } => {
                let car = self.lisp.car(idx)?;
                if let Value::Symbol(_) = self.lisp.get(car)? {
                    self.lisp.symbol_matches(car, "complex").map_err(Into::into)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }

    /// Extract real part of a tagged complex number as f64
    fn complex_real_f(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<f64, EvalError> {
        let cdr1 = self.lisp.cdr(idx)?; // (rect re im)
        let cdr2 = self.lisp.cdr(cdr1)?; // (re im)
        let re_idx = self.lisp.car(cdr2)?;
        Ok(self.get_num_as_fsize(re_idx, call_expr)? as f64)
    }

    /// Extract imaginary part of a tagged complex number as f64
    fn complex_imag_f(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<f64, EvalError> {
        let cdr1 = self.lisp.cdr(idx)?; // (rect re im)
        let cdr2 = self.lisp.cdr(cdr1)?; // (re im)
        let cdr3 = self.lisp.cdr(cdr2)?; // (im)
        let im_idx = self.lisp.car(cdr3)?;
        Ok(self.get_num_as_fsize(im_idx, call_expr)? as f64)
    }

    /// Return an exact integer if both args are exact, otherwise a float
    fn return_exact_if_both_exact(&self, args: ArenaIndex, val: fsize, _call_expr: ArenaIndex) -> EvalResult {
        let a = self.lisp.car(args)?;
        let b_list = self.lisp.cdr(args)?;
        let b = self.lisp.car(b_list)?;
        let both_exact = matches!(self.lisp.get(a)?, Value::Number(_) | Value::BigNum { .. } | Value::Rational { .. })
            && matches!(self.lisp.get(b)?, Value::Number(_) | Value::BigNum { .. } | Value::Rational { .. });
        if both_exact {
            // Check if the value fits in isize
            let f = val as f64;
            if f >= isize::MIN as f64 && f <= isize::MAX as f64 {
                self.lisp.number(val as isize).map_err(Into::into)
            } else {
                // Value too large for isize, produce BigNum
                match crate::bignum::BigNumBuf::from_f64(f) {
                    Some(buf) => {
                        match buf.to_isize() {
                            Some(n) => self.lisp.number(n).map_err(Into::into),
                            None => self.lisp.bignum_from_limbs(&buf.limbs[..buf.len], buf.negative).map_err(Into::into),
                        }
                    }
                    None => self.lisp.float(val).map_err(Into::into),
                }
            }
        } else {
            self.lisp.float(val).map_err(Into::into)
        }
    }

    /// Scheme eq?/eqv? comparison of two values
    fn eqv_compare(&self, a: ArenaIndex, b: ArenaIndex) -> EvalResult {
        let val_a = self.lisp.get(a)?;
        let val_b = self.lisp.get(b)?;
        
        let eq = match (val_a, val_b) {
            (Value::Nil, Value::Nil) => true,
            (Value::True, Value::True) => true,
            (Value::False, Value::False) => true,
            (Value::Number(x), Value::Number(y)) => x == y,
            (Value::Float(x), Value::Float(y)) => x == y || (x.is_nan() && y.is_nan()),
            (Value::Rational { num: n1, denom: d1 }, Value::Rational { num: n2, denom: d2 }) => n1 == n2 && d1 == d2,
            (Value::Complex { real: r1, imag: i1 }, Value::Complex { real: r2, imag: i2 }) => {
                (r1 == r2 || (r1.is_nan() && r2.is_nan())) && (i1 == i2 || (i1.is_nan() && i2.is_nan()))
            }
            (Value::Char(x), Value::Char(y)) => x == y,
            (Value::Symbol(_), Value::Symbol(_)) => self.lisp.symbol_eq(a, b)?,
            (Value::String { len: la, data: da }, Value::String { len: lb, data: db }) => {
                a == b || (la == lb && da == db)
            }
            _ => a == b,
        };
        
        self.lisp.boolean(eq).map_err(Into::into)
    }
    
    /// Numeric fold with already-evaluated args, supporting mixed int/float arithmetic.
    /// If any argument is a float, the result is a float.
    pub(super) fn numeric_fold<F, G>(&self, args: ArenaIndex, init: isize, int_f: F, float_f: G, call_expr: ArenaIndex) -> EvalResult
    where 
        F: Fn(isize, isize) -> Option<isize>,
        G: Fn(fsize, fsize) -> fsize,
    {
        let mut acc_int: isize = init;
        let mut acc_float: fsize = init as fsize;
        // Track accumulator state: 0=int, 1=rational, 2=float
        let mut state: u8 = 0;
        let mut acc_num: isize = init;
        let mut acc_denom: isize = 1;
        let mut current = args;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    return match state {
                        2 => self.lisp.float(acc_float).map_err(Into::into),
                        1 => self.lisp.rational(acc_num, acc_denom).map_err(Into::into),
                        _ => self.lisp.number(acc_int).map_err(Into::into),
                    };
                }
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    match self.lisp.get(car)? {
                        Value::Number(n) => {
                            match state {
                                2 => { acc_float = float_f(acc_float, n as fsize); }
                                1 => {
                                    // rational + int: (acc_num/acc_denom) op (n/1)
                                    // For add: (acc_num + n*acc_denom) / acc_denom
                                    // For mul: (acc_num * n) / acc_denom
                                    let n_num = n.checked_mul(acc_denom);
                                    match n_num.and_then(|nn| int_f(acc_num, nn)) {
                                        Some(r) => { acc_num = r; }
                                        None => {
                                            acc_float = float_f(acc_num as fsize / acc_denom as fsize, n as fsize);
                                            state = 2;
                                        }
                                    }
                                }
                                _ => {
                                    match int_f(acc_int, n) {
                                        Some(r) => acc_int = r,
                                        None => {
                                            acc_float = float_f(acc_int as fsize, n as fsize);
                                            state = 2;
                                        }
                                    }
                                }
                            }
                        }
                        Value::Float(f) => {
                            match state {
                                2 => { acc_float = float_f(acc_float, f); }
                                1 => {
                                    acc_float = float_f(acc_num as fsize / acc_denom as fsize, f);
                                    state = 2;
                                }
                                _ => {
                                    acc_float = float_f(acc_int as fsize, f);
                                    state = 2;
                                }
                            }
                        }
                        Value::Rational { num, denom } => {
                            match state {
                                2 => {
                                    acc_float = float_f(acc_float, num as fsize / denom as fsize);
                                }
                                1 => {
                                    // rational op rational: (a/b) op (n/d)
                                    // For add: (a*d + n*b) / (b*d)
                                    // For mul: (a*n) / (b*d) 
                                    let new_denom = acc_denom.checked_mul(denom);
                                    let a_scaled = acc_num.checked_mul(denom);
                                    let n_scaled = num.checked_mul(acc_denom);
                                    match (new_denom, a_scaled, n_scaled) {
                                        (Some(nd), Some(as_), Some(ns)) => {
                                            match int_f(as_, ns) {
                                                Some(new_num) => {
                                                    acc_num = new_num;
                                                    acc_denom = nd;
                                                }
                                                None => {
                                                    acc_float = float_f(acc_num as fsize / acc_denom as fsize, num as fsize / denom as fsize);
                                                    state = 2;
                                                }
                                            }
                                        }
                                        _ => {
                                            acc_float = float_f(acc_num as fsize / acc_denom as fsize, num as fsize / denom as fsize);
                                            state = 2;
                                        }
                                    }
                                }
                                _ => {
                                    // int -> rational: promote to rational
                                    // (acc_int/1) op (num/denom) 
                                    let a_scaled = acc_int.checked_mul(denom);
                                    match a_scaled.and_then(|as_| int_f(as_, num)) {
                                        Some(new_num) => {
                                            acc_num = new_num;
                                            acc_denom = denom;
                                            state = 1;
                                        }
                                        None => {
                                            acc_float = float_f(acc_int as fsize, num as fsize / denom as fsize);
                                            state = 2;
                                        }
                                    }
                                }
                            }
                        }
                        Value::BigNum { .. } => {
                            let (limbs, len, neg) = self.lisp.bignum_limbs(car)?;
                            let mut buf = crate::bignum::BigNumBuf::zero();
                            buf.limbs[..len].copy_from_slice(&limbs[..len]);
                            buf.len = len; buf.negative = neg;
                            let big_f = buf.to_f64() as fsize;
                            match state {
                                2 => { acc_float = float_f(acc_float, big_f); }
                                1 => {
                                    acc_float = float_f(acc_num as fsize / acc_denom as fsize, big_f);
                                    state = 2;
                                }
                                _ => {
                                    acc_float = float_f(acc_int as fsize, big_f);
                                    state = 2;
                                }
                            }
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Compare two numbers (supports mixed int/float)
    pub(super) fn compare_numbers<F>(&self, args: ArenaIndex, cmp: F, call_expr: ArenaIndex) -> EvalResult
    where F: Fn(fsize, fsize) -> bool
    {
        let mut prev = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
        let mut current = self.lisp.cdr(args)?;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.lisp.true_val().map_err(Into::into),
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let next = self.get_num_as_fsize(car, call_expr)?;
                    if !cmp(prev, next) {
                        return self.lisp.false_val().map_err(Into::into);
                    }
                    prev = next;
                    current = self.lisp.cdr(current)?;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }

    /// Numeric equality that respects exact/inexact precision boundaries.
    /// When comparing an exact integer with an inexact float, checks that the
    /// float can exactly represent the integer value (round-trip check).
    pub(super) fn compare_numbers_eq(&self, args: ArenaIndex, call_expr: ArenaIndex) -> EvalResult {
        let first = self.lisp.car(args)?;
        let mut prev_idx = first;
        let mut current = self.lisp.cdr(args)?;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.lisp.true_val().map_err(Into::into),
                Value::Cons { .. } => {
                    let next_idx = self.lisp.car(current)?;
                    if !self.nums_equal(prev_idx, next_idx, call_expr)? {
                        return self.lisp.false_val().map_err(Into::into);
                    }
                    prev_idx = next_idx;
                    current = self.lisp.cdr(current)?;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }

    /// Compare two numeric values for equality, respecting exact/inexact precision.
    fn nums_equal(&self, a: ArenaIndex, b: ArenaIndex, _call_expr: ArenaIndex) -> Result<bool, EvalError> {
        let va = self.lisp.get(a)?;
        let vb = self.lisp.get(b)?;
        match (va, vb) {
            (Value::Number(x), Value::Number(y)) => Ok(x == y),
            (Value::Float(x), Value::Float(y)) => Ok(x == y),
            (Value::Number(n), Value::Float(f)) | (Value::Float(f), Value::Number(n)) => {
                // Round-trip: convert int→float→int to check exact representability
                let n_as_f = n as fsize;
                Ok(n_as_f == f && n_as_f as isize == n)
            }
            (Value::Rational { num: n1, denom: d1 }, Value::Rational { num: n2, denom: d2 }) => {
                // Cross-multiply to avoid float conversion
                Ok((n1 as i128) * (d2 as i128) == (n2 as i128) * (d1 as i128))
            }
            (Value::Complex { real: r1, imag: i1 }, Value::Complex { real: r2, imag: i2 }) => {
                Ok(r1 == r2 && i1 == i2)
            }
            // Compare real number with complex: equal iff imaginary part is 0
            (Value::Complex { real, imag }, _) => {
                if imag != 0.0 { return Ok(false); }
                // Compare real part with b
                let real_idx = self.lisp.float(real)?;
                self.nums_equal(real_idx, b, _call_expr)
            }
            (_, Value::Complex { real, imag }) => {
                if imag != 0.0 { return Ok(false); }
                let real_idx = self.lisp.float(real)?;
                self.nums_equal(a, real_idx, _call_expr)
            }
            // BigNum comparisons
            (Value::BigNum { .. }, Value::BigNum { .. }) => {
                let (la, lena, nega) = self.lisp.bignum_limbs(a)?;
                let (lb, lenb, negb) = self.lisp.bignum_limbs(b)?;
                if lena != lenb || nega != negb { return Ok(false); }
                Ok(la[..lena] == lb[..lenb])
            }
            (Value::BigNum { .. }, Value::Number(n)) | (Value::Number(n), Value::BigNum { .. }) => {
                let big_idx = if matches!(va, Value::BigNum { .. }) { a } else { b };
                let (limbs, len, neg) = self.lisp.bignum_limbs(big_idx)?;
                let mut buf = crate::bignum::BigNumBuf::zero();
                buf.limbs[..len].copy_from_slice(&limbs[..len]);
                buf.len = len; buf.negative = neg;
                match buf.to_isize() {
                    Some(v) => Ok(v == n),
                    None => Ok(false),
                }
            }
            (Value::BigNum { .. }, Value::Float(f)) | (Value::Float(f), Value::BigNum { .. }) => {
                // For transitive =: convert float to exact BigNum and compare exactly.
                // This avoids precision loss from BigNum→f64 conversion.
                let big_idx = if matches!(va, Value::BigNum { .. }) { a } else { b };
                let (limbs, len, neg) = self.lisp.bignum_limbs(big_idx)?;
                match crate::bignum::BigNumBuf::from_f64(f as f64) {
                    Some(float_as_big) => {
                        let mut buf = crate::bignum::BigNumBuf::zero();
                        buf.limbs[..len].copy_from_slice(&limbs[..len]);
                        buf.len = len; buf.negative = neg;
                        Ok(buf.cmp(&float_as_big) == core::cmp::Ordering::Equal)
                    }
                    None => Ok(false), // NaN, Infinity, or non-integer float
                }
            }
            _ => {
                // Fall back to fsize comparison for rational vs int/float
                let fa = match va {
                    Value::Number(n) => n as fsize,
                    Value::Float(f) => f,
                    Value::Rational { num, denom } => num as fsize / denom as fsize,
                    Value::BigNum { .. } => {
                        let (limbs, len, neg) = self.lisp.bignum_limbs(a)?;
                        let mut buf = crate::bignum::BigNumBuf::zero();
                        buf.limbs[..len].copy_from_slice(&limbs[..len]);
                        buf.len = len; buf.negative = neg;
                        buf.to_f64() as fsize
                    }
                    _ => return Ok(false),
                };
                let fb = match vb {
                    Value::Number(n) => n as fsize,
                    Value::Float(f) => f,
                    Value::Rational { num, denom } => num as fsize / denom as fsize,
                    Value::BigNum { .. } => {
                        let (limbs, len, neg) = self.lisp.bignum_limbs(b)?;
                        let mut buf = crate::bignum::BigNumBuf::zero();
                        buf.limbs[..len].copy_from_slice(&limbs[..len]);
                        buf.len = len; buf.negative = neg;
                        buf.to_f64() as fsize
                    }
                    _ => return Ok(false),
                };
                Ok(fa == fb)
            }
        }
    }
    
    /// Numeric fold that always produces a float result (starts with float accumulator)
    pub(super) fn numeric_fold_float<G>(&self, args: ArenaIndex, init: fsize, float_f: G, call_expr: ArenaIndex) -> EvalResult
    where 
        G: Fn(fsize, fsize) -> fsize,
    {
        let mut acc = init;
        let mut current = args;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.lisp.float(acc).map_err(Into::into),
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    let n = self.get_num_as_fsize(car, call_expr)?;
                    acc = float_f(acc, n);
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }

    /// Fold multiplication over args with proper rational support.
    /// (* a b c) keeps exact rational results when possible.
    fn numeric_fold_mul(&self, args: ArenaIndex, call_expr: ArenaIndex) -> EvalResult {
        // state: 0=int, 1=rational, 2=float
        let mut state: u8 = 0;
        let mut acc_int: isize = 1;
        let mut acc_num: isize = 1;
        let mut acc_denom: isize = 1;
        let mut acc_float: fsize = 1.0;
        let mut current = args;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    return match state {
                        2 => self.lisp.float(acc_float).map_err(Into::into),
                        1 => self.lisp.rational(acc_num, acc_denom).map_err(Into::into),
                        _ => self.lisp.number(acc_int).map_err(Into::into),
                    };
                }
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    match self.lisp.get(car)? {
                        Value::Number(n) => {
                            match state {
                                2 => { acc_float *= n as fsize; }
                                1 => {
                                    match acc_num.checked_mul(n) {
                                        Some(r) => { acc_num = r; }
                                        None => {
                                            acc_float = (acc_num as fsize / acc_denom as fsize) * n as fsize;
                                            state = 2;
                                        }
                                    }
                                }
                                _ => {
                                    match acc_int.checked_mul(n) {
                                        Some(r) => { acc_int = r; }
                                        None => {
                                            acc_float = acc_int as fsize * n as fsize;
                                            state = 2;
                                        }
                                    }
                                }
                            }
                        }
                        Value::Float(f) => {
                            acc_float = match state {
                                2 => acc_float * f,
                                1 => (acc_num as fsize / acc_denom as fsize) * f,
                                _ => acc_int as fsize * f,
                            };
                            state = 2;
                        }
                        Value::Rational { num, denom } => {
                            match state {
                                2 => {
                                    acc_float *= num as fsize / denom as fsize;
                                }
                                1 => {
                                    // (a/b) * (n/d) = (a*n) / (b*d)
                                    match (acc_num.checked_mul(num), acc_denom.checked_mul(denom)) {
                                        (Some(nn), Some(nd)) => {
                                            acc_num = nn;
                                            acc_denom = nd;
                                        }
                                        _ => {
                                            acc_float = (acc_num as fsize / acc_denom as fsize) * (num as fsize / denom as fsize);
                                            state = 2;
                                        }
                                    }
                                }
                                _ => {
                                    // int * rational: (acc_int * num) / denom
                                    match acc_int.checked_mul(num) {
                                        Some(nn) => {
                                            acc_num = nn;
                                            acc_denom = denom;
                                            state = 1;
                                        }
                                        None => {
                                            acc_float = acc_int as fsize * (num as fsize / denom as fsize);
                                            state = 2;
                                        }
                                    }
                                }
                            }
                        }
                        Value::BigNum { .. } => {
                            let (limbs, len, neg) = self.lisp.bignum_limbs(car)?;
                            let mut buf = crate::bignum::BigNumBuf::zero();
                            buf.limbs[..len].copy_from_slice(&limbs[..len]);
                            buf.len = len; buf.negative = neg;
                            let big_f = buf.to_f64() as fsize;
                            match state {
                                2 => { acc_float *= big_f; }
                                1 => {
                                    acc_float = (acc_num as fsize / acc_denom as fsize) * big_f;
                                    state = 2;
                                }
                                _ => {
                                    acc_float = acc_int as fsize * big_f;
                                    state = 2;
                                }
                            }
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }

    /// Fold division over remaining args, accumulating as rational num/denom.
    /// Produces exact rational results for integer division chains like (/ 3 4 5) => 3/20.
    fn rational_fold_div(&self, args: ArenaIndex, init_num: isize, init_denom: isize, call_expr: ArenaIndex) -> EvalResult {
        let mut num = init_num;
        let mut denom = init_denom;
        let mut is_float = false;
        let mut acc_float: fsize = 0.0;
        let mut current = args;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    return if is_float {
                        self.lisp.float(acc_float).map_err(Into::into)
                    } else {
                        self.lisp.rational(num, denom).map_err(Into::into)
                    };
                }
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    match self.lisp.get(car)? {
                        Value::Number(n) => {
                            if n == 0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                            if is_float {
                                acc_float /= n as fsize;
                            } else {
                                denom = denom.checked_mul(n).unwrap_or_else(|| {
                                    is_float = true;
                                    acc_float = num as fsize / (denom as fsize * n as fsize);
                                    1
                                });
                            }
                        }
                        Value::Float(f) => {
                            if !is_float {
                                acc_float = num as fsize / denom as fsize;
                                is_float = true;
                            }
                            acc_float /= f;
                        }
                        Value::Rational { num: n2, denom: d2 } => {
                            if n2 == 0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                            if is_float {
                                acc_float /= n2 as fsize / d2 as fsize;
                            } else {
                                // a/b / (n2/d2) = a*d2 / (b*n2)
                                num = num.checked_mul(d2).unwrap_or_else(|| {
                                    is_float = true;
                                    acc_float = (num as fsize * d2 as fsize) / (denom as fsize * n2 as fsize);
                                    1
                                });
                                if !is_float {
                                    denom = denom.checked_mul(n2).unwrap_or_else(|| {
                                        is_float = true;
                                        acc_float = num as fsize / (denom as fsize * n2 as fsize);
                                        1
                                    });
                                }
                            }
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }

    /// Fold subtraction over remaining args with rational accumulator.
    fn rational_fold_sub(&self, args: ArenaIndex, init_num: isize, init_denom: isize, call_expr: ArenaIndex) -> EvalResult {
        let mut num = init_num;
        let mut denom = init_denom;
        let mut is_float = false;
        let mut acc_float: fsize = 0.0;
        let mut current = args;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    return if is_float {
                        self.lisp.float(acc_float).map_err(Into::into)
                    } else {
                        self.lisp.rational(num, denom).map_err(Into::into)
                    };
                }
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    match self.lisp.get(car)? {
                        Value::Number(n) => {
                            if is_float {
                                acc_float -= n as fsize;
                            } else {
                                // a/b - n = (a - n*b)/b
                                match denom.checked_mul(n).and_then(|nd| num.checked_sub(nd)) {
                                    Some(r) => num = r,
                                    None => {
                                        acc_float = num as fsize / denom as fsize - n as fsize;
                                        is_float = true;
                                    }
                                }
                            }
                        }
                        Value::Float(f) => {
                            if !is_float {
                                acc_float = num as fsize / denom as fsize;
                                is_float = true;
                            }
                            acc_float -= f;
                        }
                        Value::Rational { num: n2, denom: d2 } => {
                            if is_float {
                                acc_float -= n2 as fsize / d2 as fsize;
                            } else {
                                // a/b - n2/d2 = (a*d2 - n2*b) / (b*d2)
                                match denom.checked_mul(d2) {
                                    Some(new_denom) => {
                                        match num.checked_mul(d2).and_then(|ad| n2.checked_mul(denom).and_then(|nb| ad.checked_sub(nb))) {
                                            Some(new_num) => {
                                                num = new_num;
                                                denom = new_denom;
                                            }
                                            None => {
                                                acc_float = num as fsize / denom as fsize - n2 as fsize / d2 as fsize;
                                                is_float = true;
                                            }
                                        }
                                    }
                                    None => {
                                        acc_float = num as fsize / denom as fsize - n2 as fsize / d2 as fsize;
                                        is_float = true;
                                    }
                                }
                            }
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Helper for character chain comparisons (char=?, char<?, etc.)
    pub(super) fn char_chain_compare<F>(&self, args: ArenaIndex, compare_fn: F, call_expr: ArenaIndex) -> EvalResult
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
    
    /// Helper for string chain comparisons (case-sensitive)
    pub(super) fn string_chain_compare<F>(&self, args: ArenaIndex, compare_fn: F, call_expr: ArenaIndex) -> EvalResult
    where
        F: Fn(core::cmp::Ordering) -> bool
    {
        self.string_chain_compare_with(args, compare_fn, call_expr, false)
    }
    
    /// Compare two strings lexicographically, with optional case-insensitive comparison.
    /// When case-insensitive, uses full Unicode case folding (e.g., ß → ss) so that
    /// "Straße" and "STRASSE" compare as equal.
    pub(super) fn compare_strings_with(&self, a: ArenaIndex, b: ArenaIndex, call_expr: ArenaIndex, case_insensitive: bool) -> Result<core::cmp::Ordering, EvalError> {
        if !case_insensitive {
            // Case-sensitive: simple character-by-character comparison
            let (len_a, data_a) = self.get_string(a, call_expr)?;
            let (len_b, data_b) = self.get_string(b, call_expr)?;
            
            let min_len = len_a.min(len_b);
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
                match char_a.cmp(&char_b) {
                    core::cmp::Ordering::Equal => {}
                    ord => return Ok(ord),
                }
            }
            Ok(len_a.cmp(&len_b))
        } else {
            // Case-insensitive comparison
            let (len_a, data_a) = self.get_string(a, call_expr)?;
            let (len_b, data_b) = self.get_string(b, call_expr)?;
            
            {
                // Use full Unicode case folding. Full folding can expand characters
                // (e.g. ß → ss), so we produce folded chars on the fly and compare them.
                
                // State for iterating folded chars from string A
                let mut src_a = 0usize;
                let mut buf_a = grift_unicode::CaseMapResult::default();
                let mut buf_a_pos = 0usize;
                
                // State for iterating folded chars from string B
                let mut src_b = 0usize;
                let mut buf_b = grift_unicode::CaseMapResult::default();
                let mut buf_b_pos = 0usize;
                
                loop {
                    // Refill buffer A if needed
                    while buf_a_pos >= buf_a.len() && src_a < len_a {
                        let slot = self.lisp.arena_index_at_offset(data_a, src_a)?;
                        if let Value::Char(c) = self.lisp.get(slot)? {
                            buf_a = grift_unicode::full_foldcase(c);
                            buf_a_pos = 0;
                            src_a += 1;
                        } else {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                    }
                    
                    // Refill buffer B if needed
                    while buf_b_pos >= buf_b.len() && src_b < len_b {
                        let slot = self.lisp.arena_index_at_offset(data_b, src_b)?;
                        if let Value::Char(c) = self.lisp.get(slot)? {
                            buf_b = grift_unicode::full_foldcase(c);
                            buf_b_pos = 0;
                            src_b += 1;
                        } else {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                    }
                    
                    let has_a = buf_a_pos < buf_a.len();
                    let has_b = buf_b_pos < buf_b.len();
                    
                    match (has_a, has_b) {
                        (false, false) => return Ok(core::cmp::Ordering::Equal),
                        (true, false) => return Ok(core::cmp::Ordering::Greater),
                        (false, true) => return Ok(core::cmp::Ordering::Less),
                        (true, true) => {
                            let ca = buf_a.get(buf_a_pos).unwrap_or('\0');
                            let cb = buf_b.get(buf_b_pos).unwrap_or('\0');
                            buf_a_pos += 1;
                            buf_b_pos += 1;
                            match ca.cmp(&cb) {
                                core::cmp::Ordering::Equal => {}
                                ord => return Ok(ord),
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Helper for case-insensitive string chain comparisons
    pub(super) fn string_ci_chain_compare<F>(&self, args: ArenaIndex, compare_fn: F, call_expr: ArenaIndex) -> EvalResult
    where
        F: Fn(core::cmp::Ordering) -> bool
    {
        self.string_chain_compare_with(args, compare_fn, call_expr, true)
    }
    
    /// Unified helper for string chain comparisons with optional case-insensitivity
    fn string_chain_compare_with<F>(&self, args: ArenaIndex, compare_fn: F, call_expr: ArenaIndex, case_insensitive: bool) -> EvalResult
    where
        F: Fn(core::cmp::Ordering) -> bool
    {
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
                    let ordering = self.compare_strings_with(prev_idx, car, call_expr, case_insensitive)?;
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
    
    /// Convert a proper list to an array (vector).
    /// Used by both `(vector obj ...)` and `(list->vector lst)`.
    pub(super) fn list_to_array(&self, list: ArenaIndex, call_expr: ArenaIndex) -> EvalResult {
        // Count elements
        let mut count = 0usize;
        let mut current = list;
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
        
        // Create vector and fill
        let placeholder = self.lisp.number(0)?;
        let vec = self.lisp.make_array(count, placeholder)?;
        current = list;
        for i in 0..count {
            let val = self.lisp.car(current)?;
            self.lisp.array_set(vec, i, val)?;
            current = self.lisp.cdr(current)?;
        }
        Ok(vec)
    }

    /// Validate vector arguments for vector-map/vector-for-each.
    /// Returns (proc, vecs_args, len) where len is the minimum length across all vectors.
    fn validate_vector_args(&self, args: ArenaIndex, call_expr: ArenaIndex) -> Result<(ArenaIndex, ArenaIndex, usize), EvalError> {
        let proc = self.lisp.car(args)?;
        let vecs_args = self.lisp.cdr(args)?;
        
        let first_vec = self.lisp.car(vecs_args)?;
        let mut len = match self.lisp.get(first_vec)? {
            Value::Array { .. } => self.lisp.array_len(first_vec)?,
            v => return Err(self.type_error(call_expr, "vector", v.type_name())),
        };
        
        let mut current = self.lisp.cdr(vecs_args)?;
        while !self.lisp.get(current)?.is_nil() {
            let vec = self.lisp.car(current)?;
            match self.lisp.get(vec)? {
                Value::Array { .. } => {
                    let vlen = self.lisp.array_len(vec)?;
                    if vlen < len {
                        len = vlen;
                    }
                }
                v => return Err(self.type_error(call_expr, "vector", v.type_name())),
            }
            current = self.lisp.cdr(current)?;
        }
        
        Ok((proc, vecs_args, len))
    }

    /// Parse optional start/end range arguments from a list.
    /// If no arguments: returns (0, default_len).
    /// If start only: returns (start, default_len).
    /// If start and end: returns (start, end).
    /// Validates start >= 0, end >= 0, start <= end, both <= max_len.
    fn parse_range_args(&self, rest: ArenaIndex, max_len: usize, call_expr: ArenaIndex) -> Result<(usize, usize), EvalError> {
        let (start, end) = if self.lisp.get(rest)?.is_nil() {
            (0usize, max_len)
        } else {
            let start_val = self.lisp.car(rest)?;
            let s = self.get_int(start_val, call_expr)?;
            if s < 0 { return Err(self.make_error(ErrorKind::TypeError, call_expr)); }
            let rest2 = self.lisp.cdr(rest)?;
            let e = if self.lisp.get(rest2)?.is_nil() {
                max_len
            } else {
                let end_val = self.lisp.car(rest2)?;
                let ev = self.get_int(end_val, call_expr)?;
                if ev < 0 { return Err(self.make_error(ErrorKind::TypeError, call_expr)); }
                ev as usize
            };
            (s as usize, e)
        };
        if start > max_len || end > max_len || start > end {
            return Err(self.make_error(ErrorKind::TypeError, call_expr));
        }
        Ok((start, end))
    }

    /// Helper for port predicates: extract first arg, check if it's a port,
    /// and apply the predicate function against the IoProvider.
    fn port_predicate<F>(&self, args: ArenaIndex, check: F) -> EvalResult
    where
        F: FnOnce(&dyn grift_parser::IoProvider, grift_parser::PortId) -> bool,
    {
        let arg = self.lisp.car(args)?;
        let result = match self.lisp.get(arg)? {
            Value::Port(pid) => {
                if let Some(ref io) = self.io { check(&**io, pid) } else { false }
            }
            _ => false,
        };
        self.lisp.boolean(result).map_err(Into::into)
    }

    /// Extract an input port from optional arguments.
    /// Returns STDIN if no argument is provided.
    fn extract_input_port(&self, args: ArenaIndex, call_expr: ArenaIndex) -> Result<grift_parser::PortId, EvalError> {
        if self.lisp.get(args)?.is_nil() {
            Ok(self.current_input_port)
        } else {
            let port_arg = self.lisp.car(args)?;
            match self.lisp.get(port_arg)? {
                Value::Port(pid) => Ok(pid),
                v => Err(self.type_error(call_expr, "port", v.type_name())),
            }
        }
    }

    /// Extract an output port from optional arguments (rest of args list).
    /// Returns current output port if no argument is provided.
    fn extract_output_port(&self, rest: ArenaIndex, call_expr: ArenaIndex) -> Result<grift_parser::PortId, EvalError> {
        if self.lisp.get(rest)?.is_nil() {
            Ok(self.current_output_port)
        } else {
            let port_arg = self.lisp.car(rest)?;
            match self.lisp.get(port_arg)? {
                Value::Port(pid) => Ok(pid),
                v => Err(self.type_error(call_expr, "port", v.type_name())),
            }
        }
    }

    /// Convert an i64 time value (from IoProvider) to a Lisp value.
    /// Tries exact integer first, falls back to float.
    fn time_i64_to_value(
        &self,
        getter: impl FnOnce(&dyn grift_parser::IoProvider) -> Result<i64, grift_parser::IoErrorKind>,
        call_expr: ArenaIndex,
    ) -> EvalResult {
        match &self.io {
            Some(io) => {
                let val = getter(&**io)
                    .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                match isize::try_from(val) {
                    Ok(n) => self.lisp.number(n).map_err(Into::into),
                    Err(_) => self.lisp.float(val as fsize).map_err(Into::into),
                }
            }
            None => Err(self.make_error(ErrorKind::Generic, call_expr)),
        }
    }

    /// Compute a floor or truncate division operation.
    /// `round_fn` is either `libm::floor` or `libm::trunc`.
    fn division_op(
        &self,
        args: ArenaIndex,
        call_expr: ArenaIndex,
        round_fn: fn(f64) -> f64,
    ) -> Result<(fsize, fsize), EvalError> {
        let n_idx = self.lisp.car(args)?;
        let d_idx = self.lisp.car(self.lisp.cdr(args)?)?;
        let n_val = self.lisp.get(n_idx)?;
        let d_val = self.lisp.get(d_idx)?;

        // Check for BigNum arguments requiring exact BigNum division
        let has_bignum = matches!(n_val, Value::BigNum { .. }) || matches!(d_val, Value::BigNum { .. });
        if has_bignum {
            // Convert both to BigNumBuf
            let n_big = match n_val {
                Value::BigNum { .. } => self.load_bignum_buf(n_idx)?,
                Value::Number(n) => crate::bignum::BigNumBuf::from_isize(n),
                _ => {
                    let f = self.get_num_as_fsize(n_idx, call_expr)?;
                    return self.division_op_float(f, d_idx, call_expr, round_fn);
                }
            };
            let d_big = match d_val {
                Value::BigNum { .. } => self.load_bignum_buf(d_idx)?,
                Value::Number(n) => crate::bignum::BigNumBuf::from_isize(n),
                _ => {
                    let n_f = n_big.to_f64() as fsize;
                    let d_f = self.get_num_as_fsize(d_idx, call_expr)?;
                    if d_f == 0.0 {
                        return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                    }
                    let q = round_fn(n_f as f64 / d_f as f64);
                    let r = n_f as f64 - d_f as f64 * q;
                    return Ok((q as fsize, r as fsize));
                }
            };
            if d_big.is_zero() {
                return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
            }

            // Compute BigNum truncated division
            let (q_trunc, r_trunc) = n_big.divmod(&d_big);

            if r_trunc.is_zero() {
                // Exact division
                let q_f = q_trunc.to_f64() as fsize;
                return Ok((q_f, 0.0));
            }

            // We have n = q_trunc * d + r_trunc
            // The exact quotient is q_trunc + r_trunc/d
            // Apply rounding to decide final quotient
            let q_trunc_f = q_trunc.to_f64();
            let r_f = r_trunc.to_f64();
            let d_f = d_big.to_f64();
            let frac = r_f / d_f;
            let exact_q = q_trunc_f + frac;
            let rounded_q = round_fn(exact_q);

            // Compute remainder: r = n - d * rounded_q
            // But we need to be more precise: if rounded_q == q_trunc_f, then r = r_trunc
            // If rounded_q == q_trunc_f + 1, then r = r_trunc - d
            // If rounded_q == q_trunc_f - 1, then r = r_trunc + d
            let q_diff = rounded_q - q_trunc_f;
            let r_result = if q_diff == 0.0 {
                r_trunc.to_f64()
            } else if q_diff == 1.0 {
                r_trunc.to_f64() - d_big.to_f64()
            } else if q_diff == -1.0 {
                r_trunc.to_f64() + d_big.to_f64()
            } else {
                // General case
                let n_f = n_big.to_f64();
                n_f - d_f * rounded_q
            };

            Ok((rounded_q as fsize, r_result as fsize))
        } else {
            let n = self.get_num_as_fsize(n_idx, call_expr)?;
            let d = self.get_num_as_fsize(d_idx, call_expr)?;
            if d == 0.0 {
                return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
            }
            let q = round_fn((n as f64) / (d as f64));
            let r = n as f64 - d as f64 * q;
            Ok((q as fsize, r as fsize))
        }
    }

    /// Helper for division_op with float arguments
    fn division_op_float(
        &self,
        n: fsize,
        d_idx: ArenaIndex,
        call_expr: ArenaIndex,
        round_fn: fn(f64) -> f64,
    ) -> Result<(fsize, fsize), EvalError> {
        let d = self.get_num_as_fsize(d_idx, call_expr)?;
        if d == 0.0 {
            return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
        }
        let q = round_fn((n as f64) / (d as f64));
        let r = n as f64 - d as f64 * q;
        Ok((q as fsize, r as fsize))
    }

    /// Compute floor/truncate division and return both quotient and remainder as a two-element list.
    fn division_values(
        &self,
        args: ArenaIndex,
        call_expr: ArenaIndex,
        round_fn: fn(f64) -> f64,
    ) -> Result<ArenaIndex, EvalError> {
        let (q, r) = self.division_op(args, call_expr, round_fn)?;
        let qv = self.return_exact_if_both_exact(args, q, call_expr)?;
        let rv = self.return_exact_if_both_exact(args, r, call_expr)?;
        let nil = self.lisp.nil()?;
        let tail = self.lisp.cons(rv, nil)?;
        self.lisp.cons(qv, tail).map_err(Into::into)
    }

    /// Euclidean division: remainder is always non-negative.
    /// q = floor(n/d) if d > 0, ceil(n/d) if d < 0.
    fn euclidean_division_op(
        &self,
        args: ArenaIndex,
        call_expr: ArenaIndex,
    ) -> Result<(fsize, fsize), EvalError> {
        let n = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
        let d = self.get_num_as_fsize(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
        if d == 0.0 {
            return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
        }
        let q = if d > 0.0 {
            libm::floor((n as f64) / (d as f64))
        } else {
            libm::ceil((n as f64) / (d as f64))
        };
        let r = n as f64 - d as f64 * q;
        Ok((q as fsize, r as fsize))
    }

    /// Euclidean division returning both quotient and remainder as a two-element list.
    fn euclidean_division_values(
        &self,
        args: ArenaIndex,
        call_expr: ArenaIndex,
    ) -> Result<ArenaIndex, EvalError> {
        let (q, r) = self.euclidean_division_op(args, call_expr)?;
        let qv = self.return_exact_if_both_exact(args, q, call_expr)?;
        let rv = self.return_exact_if_both_exact(args, r, call_expr)?;
        let nil = self.lisp.nil()?;
        let tail = self.lisp.cons(rv, nil)?;
        self.lisp.cons(qv, tail).map_err(Into::into)
    }

    /// Balanced division: rounds to nearest, ties give negative remainder.
    /// At half-way: ceil(n/d) when d>0, floor(n/d) when d<0.
    fn balanced_division_op(
        &self,
        args: ArenaIndex,
        call_expr: ArenaIndex,
    ) -> Result<(fsize, fsize), EvalError> {
        let n = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
        let d = self.get_num_as_fsize(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
        if d == 0.0 {
            return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
        }
        let exact_q = (n as f64) / (d as f64);
        let tq = libm::trunc(exact_q);
        let r_trunc = n as f64 - d as f64 * tq;
        let abs_r = libm::fabs(r_trunc);
        let abs_d_half = libm::fabs(d as f64) / 2.0;
        let q = if abs_r > abs_d_half {
            // Remainder too large, need to adjust
            if (d > 0.0) == (r_trunc > 0.0) { tq + 1.0 } else { tq - 1.0 }
        } else if abs_r == abs_d_half {
            // Exactly half: ceil when d>0, floor when d<0
            if d > 0.0 { libm::ceil(exact_q) } else { libm::floor(exact_q) }
        } else {
            tq
        };
        let r = n as f64 - d as f64 * q;
        Ok((q as fsize, r as fsize))
    }

    /// Balanced division returning both quotient and remainder as a two-element list.
    fn balanced_division_values(
        &self,
        args: ArenaIndex,
        call_expr: ArenaIndex,
    ) -> Result<ArenaIndex, EvalError> {
        let (q, r) = self.balanced_division_op(args, call_expr)?;
        let qv = self.return_exact_if_both_exact(args, q, call_expr)?;
        let rv = self.return_exact_if_both_exact(args, r, call_expr)?;
        let nil = self.lisp.nil()?;
        let tail = self.lisp.cons(rv, nil)?;
        self.lisp.cons(qv, tail).map_err(Into::into)
    }

    /// Convert a BigNum arena value to f64.
    fn bignum_to_f64(&self, idx: ArenaIndex) -> Result<f64, EvalError> {
        let (limbs, len, neg) = self.lisp.bignum_limbs(idx)?;
        let mut buf = crate::bignum::BigNumBuf::zero();
        buf.limbs[..len].copy_from_slice(&limbs[..len]);
        buf.len = len;
        buf.negative = neg;
        Ok(buf.to_f64())
    }

    /// Load a BigNumBuf from an arena value.
    fn load_bignum_buf(&self, idx: ArenaIndex) -> Result<crate::bignum::BigNumBuf, EvalError> {
        let (limbs, len, neg) = self.lisp.bignum_limbs(idx)?;
        let mut buf = crate::bignum::BigNumBuf::zero();
        buf.limbs[..len].copy_from_slice(&limbs[..len]);
        buf.len = len;
        buf.negative = neg;
        Ok(buf)
    }

    /// Store a BigNumBuf result, converting to isize if possible.
    fn store_bignum_result(&self, buf: &crate::bignum::BigNumBuf) -> EvalResult {
        match buf.to_isize() {
            Some(n) => self.lisp.number(n).map_err(Into::into),
            None => self.lisp.bignum_from_limbs(&buf.limbs[..buf.len], buf.negative).map_err(Into::into),
        }
    }

    /// Round a tagged BigNum ratio (%bignum-ratio quotient remainder denom).
    /// The ratio represents `quotient + remainder/denom`.
    /// `round_fn` is the standard rounding function (floor, ceil, trunc, rint).
    pub fn round_bignum_ratio(&self, tagged: ArenaIndex, round_fn: fn(fsize) -> fsize) -> EvalResult {
        // Check tag
        let tag = self.lisp.car(tagged)?;
        if !self.lisp.symbol_matches(tag, "%bignum-ratio")? {
            // Not a tagged ratio, fall back to type error
            return Err(self.type_error(tagged, "number", "list"));
        }
        let rest = self.lisp.cdr(tagged)?;
        let q_idx = self.lisp.car(rest)?;
        let rest2 = self.lisp.cdr(rest)?;
        let r_idx = self.lisp.car(rest2)?;
        let rest3 = self.lisp.cdr(rest2)?;
        let d_idx = self.lisp.car(rest3)?;

        let r = match self.lisp.get(r_idx)? {
            Value::Number(n) => n,
            _ => return Err(self.type_error(tagged, "number", "?")),
        };
        let d = match self.lisp.get(d_idx)? {
            Value::Number(n) => n,
            _ => return Err(self.type_error(tagged, "number", "?")),
        };

        if r == 0 || d == 0 {
            // No fractional part, return quotient as-is
            return Ok(q_idx);
        }

        // Compute the rounding adjustment using the fractional part r/d
        let frac = r as fsize / d as fsize;
        let abs_frac = if frac < 0.0 { -frac } else { frac };

        // For banker's rounding (rint), when |frac| == 0.5 we need to check
        // the parity of the quotient, not just round the fraction alone.
        let adjustment = if abs_frac == 0.5 {
            // Detect banker's rounding by testing round_fn(0.5)==0 AND round_fn(1.5)==2
            let is_bankers = round_fn(0.5 as fsize) == 0.0 && round_fn(1.5 as fsize) == 2.0;
            if is_bankers {
                // This is banker's rounding (rint): round 0.5 to nearest even
                // We need to check if quotient is odd or even
                let q_is_odd = match self.lisp.get(q_idx)? {
                    Value::Number(n) => n % 2 != 0,
                    Value::BigNum { .. } => {
                        let buf = self.load_bignum_buf(q_idx)?;
                        buf.limbs[0] % 2 != 0
                    }
                    _ => false,
                };
                if q_is_odd {
                    // q is odd, round away from q (toward even q+1 or q-1)
                    if frac > 0.0 { 1isize } else { -1isize }
                } else {
                    // q is even, stay at q
                    0isize
                }
            } else {
                // Not banker's rounding, use normal round_fn
                round_fn(frac) as isize
            }
        } else {
            round_fn(frac) as isize
        };

        if adjustment == 0 {
            // No adjustment needed
            Ok(q_idx)
        } else {
            // Add adjustment to quotient
            match self.lisp.get(q_idx)? {
                Value::Number(n) => {
                    match n.checked_add(adjustment) {
                        Some(result) => self.lisp.number(result).map_err(Into::into),
                        None => {
                            let buf = crate::bignum::BigNumBuf::from_isize(n)
                                .add(&crate::bignum::BigNumBuf::from_isize(adjustment));
                            self.store_bignum_result(&buf)
                        }
                    }
                }
                Value::BigNum { .. } => {
                    let q_buf = self.load_bignum_buf(q_idx)?;
                    let adj_buf = crate::bignum::BigNumBuf::from_isize(adjustment);
                    let result = q_buf.add(&adj_buf);
                    self.store_bignum_result(&result)
                }
                _ => Err(self.type_error(tagged, "number", "?")),
            }
        }
    }

    /// BigNum division fold: (/ bignum divisor ...) → rational or integer.
    /// Tracks numerator and denominator as BigNumBufs, simplifies via GCD.
    fn bignum_fold_div(&self, first_idx: ArenaIndex, rest: ArenaIndex, call_expr: ArenaIndex) -> EvalResult {
        let mut num_big = self.load_bignum_buf(first_idx)?;
        let mut denom_big = crate::bignum::BigNumBuf::from_isize(1);

        let mut current = rest;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let elem = self.lisp.car(current)?;
            match self.lisp.get(elem)? {
                Value::Number(n) => {
                    if n == 0 { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                    let b = crate::bignum::BigNumBuf::from_isize(n);
                    denom_big = denom_big.mul(&b);
                }
                Value::BigNum { .. } => {
                    let b = self.load_bignum_buf(elem)?;
                    if b.is_zero() { return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)); }
                    denom_big = denom_big.mul(&b);
                }
                Value::Float(f) => {
                    // Fall back to float arithmetic
                    let result = num_big.to_f64() / denom_big.to_f64() / f as f64;
                    return self.lisp.float(result as fsize).map_err(Into::into);
                }
                _ => return Err(self.type_error(call_expr, "number", "?")),
            }
            current = self.lisp.cdr(current)?;
        }

        // Simplify num_big/denom_big via GCD
        let g = num_big.gcd(&denom_big);
        if !g.is_zero() && !(g.len == 1 && g.limbs[0] == 1) {
            let (num_simplified, _) = num_big.divmod(&g);
            let (denom_simplified, _) = denom_big.divmod(&g);
            num_big = num_simplified;
            denom_big = denom_simplified;
        }

        // Normalize sign: ensure denominator is positive
        if denom_big.negative {
            num_big = num_big.negate();
            denom_big = denom_big.negate();
        }

        // Try to convert to isize rational
        match (num_big.to_isize(), denom_big.to_isize()) {
            (Some(n), Some(d)) => {
                if d == 1 {
                    self.lisp.number(n).map_err(Into::into)
                } else {
                    self.lisp.rational(n, d).map_err(Into::into)
                }
            }
            (None, Some(d)) if d == 1 => {
                // Numerator is BigNum, denominator is 1 → return BigNum
                self.store_bignum_result(&num_big)
            }
            (None, Some(d)) => {
                // Numerator is BigNum, denominator fits in isize
                // Use BigNum divmod for precision
                let denom_buf = crate::bignum::BigNumBuf::from_isize(d);
                let (q_buf, r_buf) = num_big.divmod(&denom_buf);
                if r_buf.is_zero() {
                    // Exact division
                    self.store_bignum_result(&q_buf)
                } else {
                    // Non-exact: store as tagged BigNum ratio
                    // (%bignum-ratio quotient remainder denom)
                    // This allows rounding operations to compute exact results
                    let tag = self.lisp.symbol("%bignum-ratio")?;
                    let q_val = self.store_bignum_result(&q_buf)?;
                    let r_isize = r_buf.to_isize().unwrap_or(0);
                    let r_val = self.lisp.number(r_isize)?;
                    let d_val = self.lisp.number(d)?;
                    let nil = self.lisp.nil()?;
                    let l3 = self.lisp.cons(d_val, nil)?;
                    let l2 = self.lisp.cons(r_val, l3)?;
                    let l1 = self.lisp.cons(q_val, l2)?;
                    self.lisp.cons(tag, l1).map_err(Into::into)
                }
            }
            _ => {
                // Both are BigNums too large for isize
                let (q_buf, r_buf) = num_big.divmod(&denom_big);
                if r_buf.is_zero() {
                    self.store_bignum_result(&q_buf)
                } else {
                    let result = num_big.to_f64() / denom_big.to_f64();
                    self.lisp.float(result as fsize).map_err(Into::into)
                }
            }
        }
    }

    /// Extract an optional port and start/end range from argument list.
    /// `default_port` is used when no port argument is provided.
    fn extract_port_and_range(
        &self,
        rest: ArenaIndex,
        default_port: grift_parser::PortId,
        max_len: usize,
        call_expr: ArenaIndex,
    ) -> Result<(grift_parser::PortId, usize, usize), EvalError> {
        if self.lisp.get(rest)?.is_nil() {
            return Ok((default_port, 0, max_len));
        }
        let port_arg = self.lisp.car(rest)?;
        let pid = match self.lisp.get(port_arg)? {
            Value::Port(pid) => pid,
            v => return Err(self.type_error(call_expr, "port", v.type_name())),
        };
        let rest2 = self.lisp.cdr(rest)?;
        if self.lisp.get(rest2)?.is_nil() {
            return Ok((pid, 0, max_len));
        }
        let start = self.get_int(self.lisp.car(rest2)?, call_expr)? as usize;
        let rest3 = self.lisp.cdr(rest2)?;
        let end = if self.lisp.get(rest3)?.is_nil() {
            max_len
        } else {
            self.get_int(self.lisp.car(rest3)?, call_expr)? as usize
        };
        Ok((pid, start, end))
    }

    /// Implement (read) by reading characters from a port and parsing.
    fn apply_read_builtin(&mut self, pid: grift_parser::PortId, call_expr: ArenaIndex) -> EvalResult {
        let tmp = match self.io.as_mut() {
            Some(io) => match io.open_output_string() {
                Ok(p) => p,
                Err(_) => return Err(self.make_error(ErrorKind::Generic, call_expr)),
            },
            None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
        };

        let mut paren_depth: i32 = 0;
        let mut in_string = false;
        let mut in_bar_sym = false;
        let mut escape = false;
        let mut got_token = false;
        let mut fold_case = false;

        // Helper macros for IO operations that re-borrow self.io each time
        macro_rules! io_read {
            () => {{
                self.io.as_mut().unwrap().read_char(pid)
            }};
        }
        macro_rules! io_peek {
            () => {{
                self.io.as_mut().unwrap().peek_char(pid)
            }};
        }
        macro_rules! io_write {
            ($c:expr) => {{
                let _ = self.io.as_mut().unwrap().write_char(tmp, $c);
            }};
        }
        macro_rules! io_close_tmp {
            () => {{
                if let Some(io) = self.io.as_mut() { let _ = io.close_port(tmp); }
            }};
        }

        // Skip leading whitespace and comments
        loop {
            match io_read!() {
                Ok(c) => {
                    if c == ';' {
                        // Line comment
                        loop {
                            match io_read!() {
                                Ok('\n') | Ok('\r') => break,
                                Ok(_) => {}
                                Err(_) => break,
                            }
                        }
                        continue;
                    }
                    if c == '#' {
                        match io_peek!() {
                            Ok('|') => {
                                let _ = io_read!();
                                let mut depth = 1i32;
                                let mut prev = ' ';
                                while depth > 0 {
                                    match io_read!() {
                                        Ok(bc) => {
                                            if prev == '#' && bc == '|' { depth += 1; }
                                            if prev == '|' && bc == '#' { depth -= 1; }
                                            prev = bc;
                                        }
                                        Err(_) => break,
                                    }
                                }
                                continue;
                            }
                            Ok(';') => {
                                let _ = io_read!();
                                self.apply_read_builtin(pid, call_expr)?;
                                continue;
                            }
                            Ok('(') => {
                                let _ = io_read!();
                                io_write!('#');
                                io_write!('(');
                                paren_depth += 1;
                                break;
                            }
                            Ok('u') => {
                                let _ = io_read!();
                                io_write!('#');
                                io_write!('u');
                                match io_peek!() {
                                    Ok('8') => {
                                        let _ = io_read!();
                                        io_write!('8');
                                        match io_peek!() {
                                            Ok('(') => {
                                                let _ = io_read!();
                                                io_write!('(');
                                                paren_depth += 1;
                                                break;
                                            }
                                            _ => { got_token = true; break; }
                                        }
                                    }
                                    _ => { got_token = true; break; }
                                }
                            }
                            Ok('!') => {
                                // #!fold-case or #!no-fold-case directive
                                let _ = io_read!(); // consume '!'
                                let mut directive = [0u8; 16];
                                let mut dlen = 0;
                                loop {
                                    match io_peek!() {
                                        Ok(c) if c.is_alphanumeric() || c == '-' => {
                                            let _ = io_read!();
                                            if dlen < 16 { directive[dlen] = c.to_ascii_lowercase() as u8; dlen += 1; }
                                        }
                                        _ => break,
                                    }
                                }
                                if &directive[..dlen] == b"fold-case" {
                                    fold_case = true;
                                } else if &directive[..dlen] == b"no-fold-case" {
                                    fold_case = false;
                                }
                                // Skip whitespace after directive
                                loop {
                                    match io_peek!() {
                                        Ok(c) if c.is_whitespace() => {
                                            let _ = io_read!();
                                        }
                                        _ => break,
                                    }
                                }
                                continue;
                            }
                            Ok(c2) if c2.is_ascii_digit() => {
                                // Datum label: #n=<datum> or #n#
                                // Write the complete datum label syntax into the
                                // buffer and let the parser handle it.
                                io_write!('#');
                                let _ = io_read!(); // consume first digit
                                io_write!(c2);
                                // Read remaining digits
                                loop {
                                    match io_peek!() {
                                        Ok(d) if d.is_ascii_digit() => {
                                            let _ = io_read!();
                                            io_write!(d);
                                        }
                                        _ => break,
                                    }
                                }
                                match io_peek!() {
                                    Ok('=') => {
                                        // #n=<datum> — write '=' and continue to read
                                        // the labeled datum into the same buffer
                                        let _ = io_read!();
                                        io_write!('=');
                                        // Check what follows
                                        match io_peek!() {
                                            Ok('(') | Ok('[') => {
                                                // List datum — include it in the buffer
                                                let _ = io_read!();
                                                io_write!('(');
                                                paren_depth += 1;
                                                break;
                                            }
                                            Ok('#') => {
                                                // Could be #(, #t, #f, #\, etc.
                                                // Let the outer loop re-process from the #
                                                // by not consuming it; break with got_token
                                                // No — we need to continue reading.
                                                // Just break and let the got_token path
                                                // read the next atom, or re-enter the loop.
                                                // Actually, we can't re-enter because we 
                                                // already have content in the buffer.
                                                // Write '#' and process
                                                let _ = io_read!();
                                                io_write!('#');
                                                // Read the rest of the hash token inline
                                                match io_peek!() {
                                                    Ok('(') => {
                                                        let _ = io_read!();
                                                        io_write!('(');
                                                        paren_depth += 1;
                                                        break;
                                                    }
                                                    _ => {
                                                        got_token = true;
                                                        break;
                                                    }
                                                }
                                            }
                                            Ok('"') => {
                                                // String datum
                                                let _ = io_read!();
                                                io_write!('"');
                                                in_string = true;
                                                break;
                                            }
                                            _ => {
                                                // Atom datum (number, symbol, etc.)
                                                got_token = true;
                                                break;
                                            }
                                        }
                                    }
                                    Ok('#') => {
                                        // #n# — datum reference, just write '#'
                                        let _ = io_read!();
                                        io_write!('#');
                                        got_token = true;
                                        break;
                                    }
                                    _ => {
                                        // Not a valid datum label
                                        got_token = true;
                                        break;
                                    }
                                }
                            }
                            _ => {
                                io_write!('#');
                                got_token = true;
                                break;
                            }
                        }
                    } else if !c.is_whitespace() {
                        let wc = if fold_case && c.is_ascii_alphabetic() { c.to_ascii_lowercase() } else { c };
                        io_write!(wc);
                        if c == '(' || c == '[' {
                            paren_depth += 1;
                        } else if c == '"' {
                            in_string = true;
                        } else if c == '\'' || c == '`' {
                            // Quote/quasiquote prefix
                            io_close_tmp!();
                            let inner = self.apply_read_builtin(pid, call_expr)?;
                            let sym = if c == '\'' {
                                self.lisp.symbol("quote")?
                            } else {
                                self.lisp.symbol("quasiquote")?
                            };
                            let nil = self.lisp.nil()?;
                            let tail = self.lisp.cons(inner, nil)?;
                            return self.lisp.cons(sym, tail).map_err(Into::into);
                        } else if c == ',' {
                            io_close_tmp!();
                            let is_splicing = match io_peek!() {
                                Ok('@') => { let _ = io_read!(); true }
                                _ => false,
                            };
                            let inner = self.apply_read_builtin(pid, call_expr)?;
                            let sym = if is_splicing {
                                self.lisp.symbol("unquote-splicing")?
                            } else {
                                self.lisp.symbol("unquote")?
                            };
                            let nil = self.lisp.nil()?;
                            let tail = self.lisp.cons(inner, nil)?;
                            return self.lisp.cons(sym, tail).map_err(Into::into);
                        } else if c == '|' {
                            in_bar_sym = true;
                        } else if paren_depth == 0 {
                            got_token = true;
                        }
                        break;
                    }
                }
                Err(grift_parser::IoErrorKind::Eof) => {
                    io_close_tmp!();
                    return self.lisp.eof().map_err(Into::into);
                }
                Err(_) => {
                    io_close_tmp!();
                    return Err(self.make_error(ErrorKind::Generic, call_expr));
                }
            }
        }

        // Read remaining characters
        if paren_depth > 0 || in_string {
            loop {
                match io_read!() {
                    Ok(c) => {
                        if in_string {
                            io_write!(c);
                            if escape {
                                escape = false;
                            } else if c == '\\' {
                                escape = true;
                            } else if c == '"' {
                                in_string = false;
                                if paren_depth == 0 { break; }
                            }
                        } else if c == ';' {
                            loop {
                                match io_read!() {
                                    Ok('\n') | Ok('\r') => break,
                                    Ok(_) => {}
                                    Err(_) => break,
                                }
                            }
                        } else if c == '#' {
                            match io_peek!() {
                                Ok('|') => {
                                    let _ = io_read!();
                                    let mut depth = 1i32;
                                    let mut prev = ' ';
                                    while depth > 0 {
                                        match io_read!() {
                                            Ok(bc) => {
                                                if prev == '#' && bc == '|' { depth += 1; }
                                                if prev == '|' && bc == '#' { depth -= 1; }
                                                prev = bc;
                                            }
                                            Err(_) => break,
                                        }
                                    }
                                }
                                Ok(';') => {
                                    let _ = io_read!();
                                    // Datum comment: skip next datum.
                                    // Propagate errors (e.g., #;. is invalid)
                                    self.apply_read_builtin(pid, call_expr)?;
                                }
                                Ok('(') => {
                                    let _ = io_read!();
                                    io_write!('#');
                                    io_write!('(');
                                    paren_depth += 1;
                                }
                                Ok('u') => {
                                    let _ = io_read!();
                                    io_write!('#');
                                    io_write!('u');
                                    match io_peek!() {
                                        Ok('8') => {
                                            let _ = io_read!();
                                            io_write!('8');
                                            match io_peek!() {
                                                Ok('(') => {
                                                    let _ = io_read!();
                                                    io_write!('(');
                                                    paren_depth += 1;
                                                }
                                                _ => {}
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {
                                    io_write!(c);
                                }
                            }
                        } else {
                            io_write!(c);
                            if c == '(' || c == '[' {
                                paren_depth += 1;
                            } else if c == ')' || c == ']' {
                                paren_depth -= 1;
                                if paren_depth == 0 { break; }
                            } else if c == '"' {
                                in_string = true;
                            }
                        }
                    }
                    Err(grift_parser::IoErrorKind::Eof) => break,
                    Err(_) => {
                        io_close_tmp!();
                        return Err(self.make_error(ErrorKind::Generic, call_expr));
                    }
                }
            }
        } else if got_token || in_bar_sym {
            loop {
                match io_peek!() {
                    Ok(c) if !in_bar_sym && (c.is_whitespace() || c == '(' || c == ')' || c == '[' || c == ']' || c == '"' || c == ';') => break,
                    Ok(c) => {
                        let _ = io_read!();
                        let wc = if fold_case && !in_bar_sym && c.is_ascii_alphabetic() { c.to_ascii_lowercase() } else { c };
                        io_write!(wc);
                        if c == '|' { in_bar_sym = !in_bar_sym; }
                        if !in_bar_sym && !got_token { got_token = true; break; }
                    }
                    Err(_) => break,
                }
            }
            // If we just closed a bar symbol and there's more to read
            if got_token && !in_bar_sym {
                // Continue reading any suffix after the closing |
                loop {
                    match io_peek!() {
                        Ok(c) if c.is_whitespace() || c == '(' || c == ')' || c == '[' || c == ']' || c == '"' || c == ';' => break,
                        Ok(c) => {
                            let _ = io_read!();
                            let wc = if fold_case && c.is_ascii_alphabetic() { c.to_ascii_lowercase() } else { c };
                            io_write!(wc);
                        }
                        Err(_) => break,
                    }
                }
            }
        }

        // Parse the accumulated expression
        let result = {
            let s = match self.io.as_ref().and_then(|io| io.get_output_string(tmp).ok()) {
                Some(s) => s,
                None => {
                    io_close_tmp!();
                    return Err(self.make_error(ErrorKind::Generic, call_expr));
                }
            };
            if s.is_empty() {
                io_close_tmp!();
                return self.lisp.eof().map_err(Into::into);
            }
            grift_parser::parse(self.lisp, s)
        };

        io_close_tmp!();
        match result {
            Ok(expr) => Ok(expr),
            Err(e) => Err(EvalError::from_parse_error(e, call_expr)),
        }
    }

    /// Write the characters of a Scheme string into an output string port.
    /// Returns the PortId of the output string port containing the string content.
    /// The caller is responsible for closing the port after use.
    pub(super) fn string_to_output_port(&mut self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<grift_parser::PortId, EvalError> {
        let (len, data) = self.get_string(idx, call_expr)?;
        let io = match &mut self.io {
            Some(io) => io,
            None => return Err(EvalError::new(ErrorKind::Generic).with_expr(call_expr)),
        };
        let port = match io.open_output_string() {
            Ok(p) => p,
            Err(_) => return Err(EvalError::new(ErrorKind::Generic).with_expr(call_expr)),
        };
        for i in 0..len {
            let char_slot = self.lisp.arena_index_at_offset(data, i)?;
            if let Value::Char(c) = self.lisp.get(char_slot)? {
                if io.write_char(port, c).is_err() {
                    return Err(EvalError::new(ErrorKind::Generic).with_expr(call_expr));
                }
            }
        }
        Ok(port)
    }

    /// Open a file port using the provided file open mode.
    fn open_file_port(
        &mut self,
        args: ArenaIndex,
        call_expr: ArenaIndex,
        mode: grift_parser::FileOpenMode,
    ) -> EvalResult {
        let arg = self.lisp.car(args)?;
        let port = self.string_to_output_port(arg, call_expr)?;
        let io = match &mut self.io {
            Some(io) => io,
            None => return Err(EvalError::new(ErrorKind::FileError).with_expr(call_expr)),
        };
        let pid = match io.open_file_from_string_port(port, mode) {
            Ok(p) => p,
            Err(_) => {
                io.close_port(port).ok();
                return Err(EvalError::new(ErrorKind::FileError).with_expr(call_expr));
            }
        };
        io.close_port(port).ok();
        self.lisp.port(pid).map_err(Into::into)
    }

    /// Extract an exit code from optional arguments.
    /// - No args or #t: 0
    /// - #f: 1
    /// - integer: use directly
    fn extract_exit_code(&self, args: ArenaIndex, call_expr: ArenaIndex) -> Result<i32, EvalError> {
        if self.lisp.get(args)?.is_nil() {
            return Ok(0);
        }
        let arg = self.lisp.car(args)?;
        match self.lisp.get(arg)? {
            Value::True => Ok(0),
            Value::False => Ok(1),
            Value::Number(n) => Ok(n as i32),
            v => Err(self.type_error(call_expr, "integer or boolean", v.type_name())),
        }
    }
}

/// Adapter to write through an [`IoProvider`] port via [`core::fmt::Write`].
struct IoPortWriter<'a> {
    io: &'a mut dyn grift_parser::IoProvider,
    port: grift_parser::PortId,
    error: bool,
}

impl core::fmt::Write for IoPortWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.io.write_str(self.port, s).is_err() {
            self.error = true;
            Err(core::fmt::Error)
        } else {
            Ok(())
        }
    }
}

// ============================================================================
// Write with datum labels (R7RS §6.13.3)
// ============================================================================

/// Maximum number of trackable cons cells for shared/circular detection.
/// When exceeded, additional nodes are silently ignored (no labels emitted).
const MAX_TRACKED_NODES: usize = 256;

/// Maximum write depth to prevent stack overflow on deep structures.
const MAX_WRITE_DEPTH: usize = 100;

/// Maximum list elements to write before truncating.
const MAX_WRITE_LIST_ELEMENTS: usize = 100;

/// Entry in the node tracker. Nodes not present in the tracker are unseen.
/// `count` tracks how many times a node has been visited during scanning.
/// After label assignment, `label >= 0` means this node gets a datum label.
#[derive(Clone, Copy)]
struct TrackerEntry {
    raw: usize,
    count: u16,
    label: i16, // -1 = no label, >= 0 = assigned label number
    emitted: bool,
}

/// Fixed-size tracker for detecting shared/circular cons cells.
struct NodeTracker {
    entries: [TrackerEntry; MAX_TRACKED_NODES],
    len: usize,
}

impl NodeTracker {
    fn new() -> Self {
        NodeTracker {
            entries: [TrackerEntry { raw: 0, count: 0, label: -1, emitted: false }; MAX_TRACKED_NODES],
            len: 0,
        }
    }

    /// Find index of entry for given ArenaIndex, or None.
    fn find(&self, idx: ArenaIndex) -> Option<usize> {
        let raw = idx.raw();
        for i in 0..self.len {
            if self.entries[i].raw == raw {
                return Some(i);
            }
        }
        None
    }

    /// Record a visit to a node. Returns the count after this visit.
    fn visit(&mut self, idx: ArenaIndex) -> u16 {
        let raw = idx.raw();
        if let Some(i) = self.find(idx) {
            if self.entries[i].count < u16::MAX {
                self.entries[i].count += 1;
            }
            return self.entries[i].count;
        }
        if self.len < MAX_TRACKED_NODES {
            self.entries[self.len] = TrackerEntry { raw, count: 1, label: -1, emitted: false };
            self.len += 1;
        }
        1
    }

    /// Mark a node as needing a datum label.
    fn mark_labeled(&mut self, idx: ArenaIndex, label: i16) {
        if let Some(i) = self.find(idx) {
            self.entries[i].label = label;
        }
    }

    /// Get the label for a node, or -1 if none.
    fn get_label(&self, idx: ArenaIndex) -> i16 {
        if let Some(i) = self.find(idx) {
            self.entries[i].label
        } else {
            -1
        }
    }

    /// Check if this node's label has been emitted (first occurrence written).
    fn is_emitted(&self, idx: ArenaIndex) -> bool {
        if let Some(i) = self.find(idx) {
            self.entries[i].emitted
        } else {
            false
        }
    }

    /// Mark a node's label as emitted.
    fn set_emitted(&mut self, idx: ArenaIndex) {
        if let Some(i) = self.find(idx) {
            self.entries[i].emitted = true;
        }
    }
}

/// Phase 1 for `write-shared`: count all cons cell occurrences.
fn scan_shared<const N: usize>(
    lisp: &grift_parser::Lisp<N>,
    idx: ArenaIndex,
    tracker: &mut NodeTracker,
) {
    match lisp.get(idx) {
        Ok(Value::Cons { .. }) => {
            let count = tracker.visit(idx);
            if count > 1 {
                return; // already visited, don't recurse again
            }
            if let Ok((car, cdr)) = lisp.car_cdr(idx) {
                scan_shared(lisp, car, tracker);
                scan_shared(lisp, cdr, tracker);
            }
        }
        Ok(Value::Array { len, .. }) => {
            for i in 0..len {
                if let Ok(elem) = lisp.array_get(idx, i) {
                    scan_shared(lisp, elem, tracker);
                }
            }
        }
        _ => {}
    }
}

/// Phase 1 for `write`: detect circular cons cells using DFS with "in-progress" marking.
/// Uses a separate stack-based tracker for the "currently on stack" set.
fn scan_circular<const N: usize>(
    lisp: &grift_parser::Lisp<N>,
    idx: ArenaIndex,
    tracker: &mut NodeTracker,
    on_stack: &mut [usize; MAX_TRACKED_NODES],
    on_stack_len: &mut usize,
) {
    match lisp.get(idx) {
        Ok(Value::Cons { .. }) => {
            let raw = idx.raw();
            // Check if on current DFS stack (circular)
            for i in 0..*on_stack_len {
                if on_stack[i] == raw {
                    // Found cycle - mark this node
                    tracker.visit(idx);
                    tracker.visit(idx); // count=2 to flag as shared
                    return;
                }
            }
            // Check if already fully visited
            if let Some(i) = tracker.find(idx) {
                if tracker.entries[i].count > 0 {
                    return; // already explored this subtree
                }
            }
            // Mark as visited and push onto stack
            tracker.visit(idx);
            if *on_stack_len < MAX_TRACKED_NODES {
                on_stack[*on_stack_len] = raw;
                *on_stack_len += 1;
            }
            if let Ok((car, cdr)) = lisp.car_cdr(idx) {
                scan_circular(lisp, car, tracker, on_stack, on_stack_len);
                scan_circular(lisp, cdr, tracker, on_stack, on_stack_len);
            }
            // Pop from stack
            if *on_stack_len > 0 {
                *on_stack_len -= 1;
            }
        }
        Ok(Value::Array { len, .. }) => {
            for i in 0..len {
                if let Ok(elem) = lisp.array_get(idx, i) {
                    scan_circular(lisp, elem, tracker, on_stack, on_stack_len);
                }
            }
        }
        _ => {}
    }
}

/// Phase 2: assign datum labels to nodes that need them (count > 1).
fn assign_labels(tracker: &mut NodeTracker) {
    let mut next_label: i16 = 0;
    for i in 0..tracker.len {
        if tracker.entries[i].count > 1 {
            tracker.entries[i].label = next_label;
            next_label += 1;
        }
    }
}

/// Phase 3: write value with datum labels.
fn write_value_labeled<const N: usize, W: core::fmt::Write>(
    lisp: &grift_parser::Lisp<N>,
    idx: ArenaIndex,
    w: &mut W,
    tracker: &mut NodeTracker,
    depth: usize,
) {
    if depth > MAX_WRITE_DEPTH {
        let _ = w.write_str("...");
        return;
    }
    match lisp.get(idx) {
        Ok(Value::Cons { .. }) => {
            let label = tracker.get_label(idx);
            if label >= 0 {
                if tracker.is_emitted(idx) {
                    // Back-reference
                    let _ = w.write_str("#");
                    write_usize(w, label as usize);
                    let _ = w.write_str("#");
                    return;
                }
                // First occurrence - emit definition
                tracker.set_emitted(idx);
                let _ = w.write_str("#");
                write_usize(w, label as usize);
                let _ = w.write_str("=");
            }
            let _ = w.write_str("(");
            write_list_labeled(lisp, idx, w, tracker, depth + 1);
            let _ = w.write_str(")");
        }
        Ok(Value::Array { len, .. }) => {
            let _ = w.write_str("#(");
            for i in 0..len {
                if i > 0 {
                    let _ = w.write_str(" ");
                }
                if let Ok(elem) = lisp.array_get(idx, i) {
                    write_value_labeled(lisp, elem, w, tracker, depth + 1);
                }
            }
            let _ = w.write_str(")");
        }
        _ => {
            // Non-composite values: delegate to DisplayValue
            let dv = grift_parser::DisplayValue::new(idx, lisp);
            let _ = core::fmt::write(w, format_args!("{}", dv));
        }
    }
}

/// Write list contents with datum label awareness.
fn write_list_labeled<const N: usize, W: core::fmt::Write>(
    lisp: &grift_parser::Lisp<N>,
    mut idx: ArenaIndex,
    w: &mut W,
    tracker: &mut NodeTracker,
    depth: usize,
) {
    let mut first = true;
    let mut count = 0;
    loop {
        if count > MAX_WRITE_LIST_ELEMENTS {
            let _ = w.write_str(" ...");
            return;
        }
        match lisp.get(idx) {
            Ok(Value::Nil) => return,
            Ok(Value::Cons { .. }) => {
                if !first {
                    // Check if this cdr cons has a label (shared/circular reference)
                    let label = tracker.get_label(idx);
                    if label >= 0 {
                        if tracker.is_emitted(idx) {
                            // Back-reference to already-emitted cons
                            let _ = w.write_str(" . ");
                            let _ = w.write_str("#");
                            write_usize(w, label as usize);
                            let _ = w.write_str("#");
                            return;
                        }
                        // First occurrence in cdr position - emit with label
                        let _ = w.write_str(" . ");
                        tracker.set_emitted(idx);
                        let _ = w.write_str("#");
                        write_usize(w, label as usize);
                        let _ = w.write_str("=");
                        let _ = w.write_str("(");
                        write_list_labeled(lisp, idx, w, tracker, depth);
                        let _ = w.write_str(")");
                        return;
                    }
                    let _ = w.write_str(" ");
                }
                first = false;
                if let Ok((car, cdr)) = lisp.car_cdr(idx) {
                    write_value_labeled(lisp, car, w, tracker, depth);
                    idx = cdr;
                } else {
                    return;
                }
                count += 1;
            }
            Ok(_) => {
                // Improper list
                let _ = w.write_str(" . ");
                write_value_labeled(lisp, idx, w, tracker, depth);
                return;
            }
            Err(_) => {
                let _ = w.write_str(" . #<error>");
                return;
            }
        }
    }
}

/// Write a usize as decimal digits (20-digit buffer suffices for u64::MAX).
fn write_usize<W: core::fmt::Write>(w: &mut W, mut n: usize) {
    if n == 0 {
        let _ = w.write_str("0");
        return;
    }
    let mut digits = [0u8; 20];
    let mut len = 0;
    while n > 0 {
        digits[len] = (n % 10) as u8;
        n /= 10;
        len += 1;
    }
    for i in (0..len).rev() {
        let _ = w.write_str(match digits[i] {
            0 => "0", 1 => "1", 2 => "2", 3 => "3", 4 => "4",
            5 => "5", 6 => "6", 7 => "7", 8 => "8", _ => "9",
        });
    }
}

/// Top-level: write a value with datum labels for shared/circular structures.
fn write_with_labels<const N: usize, W: core::fmt::Write>(
    lisp: &grift_parser::Lisp<N>,
    idx: ArenaIndex,
    w: &mut W,
    shared_mode: bool,
) {
    let mut tracker = NodeTracker::new();
    // Phase 1: scan for shared/circular structures
    if shared_mode {
        scan_shared(lisp, idx, &mut tracker);
    } else {
        let mut on_stack = [0usize; MAX_TRACKED_NODES];
        let mut on_stack_len = 0usize;
        scan_circular(lisp, idx, &mut tracker, &mut on_stack, &mut on_stack_len);
    }
    // Phase 2: assign labels to nodes seen more than once
    assign_labels(&mut tracker);
    // Phase 3: write with labels
    write_value_labeled(lisp, idx, w, &mut tracker, 0);
}

// ============================================================================
// Float formatting and parsing helpers (no_std compatible)
// ============================================================================

/// Format a float value into a char buffer, returning the number of chars written.
/// Uses core::fmt::Write for no_std-compatible formatting.
fn format_float_to_chars(f: fsize, chars: &mut [char; 32]) -> usize {
    use core::fmt::Write;
    
    // Special values
    if f.is_nan() {
        let s = b"+nan.0";
        for (i, &b) in s.iter().enumerate() { chars[i] = b as char; }
        return s.len();
    }
    if f.is_infinite() {
        let s = if f > 0.0 { b"+inf.0" as &[u8] } else { b"-inf.0" as &[u8] };
        for (i, &b) in s.iter().enumerate() { chars[i] = b as char; }
        return s.len();
    }
    
    // Use a stack buffer to format via core::fmt
    let mut buf = [0u8; 32];
    let mut writer = BufWriter { buf: &mut buf, pos: 0 };
    
    // Check if this is a whole number
    let is_whole = f.is_finite() && f == (f as isize as fsize);
    if is_whole {
        let _ = write!(writer, "{:.1}", f);
    } else {
        let _ = write!(writer, "{}", f);
    }
    
    let len = writer.pos.min(32);
    for i in 0..len {
        chars[i] = buf[i] as char;
    }
    len
}

/// Minimal byte-buffer writer for core::fmt::Write
struct BufWriter<'a> {
    buf: &'a mut [u8; 32],
    pos: usize,
}

impl core::fmt::Write for BufWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &b in s.as_bytes() {
            if self.pos < 32 {
                self.buf[self.pos] = b;
                self.pos += 1;
            }
        }
        Ok(())
    }
}

/// Parse a float from a string in no_std. Handles decimal notation and exponents.
fn parse_float_no_std(s: &str) -> Option<fsize> {
    let bytes = s.as_bytes();
    if bytes.is_empty() { return None; }
    
    let mut pos = 0;
    let negative = if bytes[pos] == b'-' { pos += 1; true } 
                   else if bytes[pos] == b'+' { pos += 1; false }
                   else { false };
    
    // Parse integer part
    let mut result: fsize = 0.0;
    let mut has_digits = false;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        result = result * 10.0 + (bytes[pos] - b'0') as fsize;
        pos += 1;
        has_digits = true;
    }
    
    // Parse fractional part
    let mut has_dot = false;
    if pos < bytes.len() && bytes[pos] == b'.' {
        has_dot = true;
        pos += 1;
        let mut frac_scale: fsize = 0.1;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            result += (bytes[pos] - b'0') as fsize * frac_scale;
            frac_scale *= 0.1;
            pos += 1;
            has_digits = true;
        }
    }
    
    if !has_digits { return None; }
    
    // Parse exponent
    if pos < bytes.len() && (bytes[pos] == b'e' || bytes[pos] == b'E') {
        pos += 1;
        let exp_neg = if pos < bytes.len() && bytes[pos] == b'-' { pos += 1; true }
                      else if pos < bytes.len() && bytes[pos] == b'+' { pos += 1; false }
                      else { false };
        let mut exp: i32 = 0;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            exp = exp.saturating_mul(10).saturating_add((bytes[pos] - b'0') as i32);
            pos += 1;
        }
        if exp_neg { exp = -exp; }
        // Multiply by 10^exp
        if exp >= 0 {
            for _ in 0..exp { result *= 10.0; }
        } else {
            for _ in 0..(-exp) { result /= 10.0; }
        }
    }
    
    // Must have consumed all input and must have been a float (dot or exponent)
    if pos != bytes.len() { return None; }
    if !has_dot && pos == bytes.len() {
        // This would have parsed as integer, don't convert
        // Unless there was an exponent
    }
    
    if negative { result = -result; }
    Some(result)
}

/// Parse an integer from a string with the given radix (2, 8, 10, 16).
fn parse_int_radix(s: &str, radix: u32) -> Option<isize> {
    let bytes = s.as_bytes();
    if bytes.is_empty() { return None; }
    
    let mut pos = 0;
    let negative = if bytes[pos] == b'-' { pos += 1; true }
                   else if bytes[pos] == b'+' { pos += 1; false }
                   else { false };
    
    if pos >= bytes.len() { return None; }
    
    let mut result: isize = 0;
    let mut has_digits = false;
    while pos < bytes.len() {
        let digit = match bytes[pos] {
            b'0'..=b'9' => (bytes[pos] - b'0') as u32,
            b'a'..=b'f' => (bytes[pos] - b'a') as u32 + 10,
            b'A'..=b'F' => (bytes[pos] - b'A') as u32 + 10,
            _ => return None,
        };
        if digit >= radix { return None; }
        result = result.checked_mul(radix as isize)?.checked_add(digit as isize)?;
        pos += 1;
        has_digits = true;
    }
    
    if !has_digits { return None; }
    if negative { result = -result; }
    Some(result)
}

/// Convert a float to a rational number (numerator, denominator) using continued fractions.
fn float_to_rational(x: f64) -> (f64, f64) {
    if x == 0.0 { return (0.0, 1.0); }
    
    let negative = x < 0.0;
    let x = libm::fabs(x);
    
    // Check if it's already a whole number
    let rounded = libm::floor(x);
    if x == rounded {
        let n = if negative { -rounded } else { rounded };
        return (n, 1.0);
    }
    
    // Continued fraction approximation with limited iterations
    let mut p0: f64 = 0.0;
    let mut q0: f64 = 1.0;
    let mut p1: f64 = 1.0;
    let mut q1: f64 = 0.0;
    let mut val = x;
    
    for _ in 0..64 {
        let a = libm::floor(val);
        let p2 = a * p1 + p0;
        let q2 = a * q1 + q0;
        
        // Check if we've found an exact representation
        if q2 > 1e15 { break; }
        
        p0 = p1; q0 = q1;
        p1 = p2; q1 = q2;
        
        let remainder = val - a;
        if libm::fabs(remainder) < 1e-15 { break; }
        if libm::fabs(p1 / q1 - x) < 1e-15 { break; }
        
        val = 1.0 / remainder;
    }
    
    if negative { (-p1, q1) } else { (p1, q1) }
}

/// Find the simplest rational number within a tolerance of x.
/// Uses the Stern-Brocot tree / mediant approach.
fn rationalize_impl(x: f64, tol: f64) -> (f64, f64) {
    if tol >= libm::fabs(x) {
        return (0.0, 1.0);
    }
    
    let negative = x < 0.0;
    let x = libm::fabs(x);
    let lo = x - tol;
    let hi = x + tol;
    
    // Use Stern-Brocot tree to find simplest fraction in [lo, hi]
    let mut lo_p: f64 = 0.0;
    let mut lo_q: f64 = 1.0;
    let mut hi_p: f64 = 1.0;
    let mut hi_q: f64 = 0.0;
    
    for _ in 0..100 {
        let med_p = lo_p + hi_p;
        let med_q = lo_q + hi_q;
        
        if med_q > 1e15 { break; }
        
        let med = med_p / med_q;
        
        if med < lo {
            lo_p = med_p;
            lo_q = med_q;
        } else if med > hi {
            hi_p = med_p;
            hi_q = med_q;
        } else {
            // Found a fraction in range
            let result_p = if negative { -med_p } else { med_p };
            return (result_p, med_q);
        }
    }
    
    // Fallback
    let result_p = if negative { -lo_p } else { lo_p };
    (result_p, lo_q)
}

/// Integer square root using Newton's method.
fn isqrt(n: usize) -> usize {
    if n == 0 { return 0; }
    if n == 1 { return 1; }
    
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Format an isize as a decimal string into `buf`, returning the number of chars written.
///
/// The caller must ensure `buf` is large enough to hold the formatted number
/// (at most 20 digits plus an optional sign character).
fn format_isize_decimal(n: isize, buf: &mut [char]) -> usize {
    let negative = n < 0;
    let mut val = n.unsigned_abs();
    let mut digits = [0u8; 20];
    let mut dlen = 0;
    if val == 0 {
        digits[0] = b'0';
        dlen = 1;
    } else {
        while val > 0 {
            digits[dlen] = b'0' + (val % 10) as u8;
            dlen += 1;
            val /= 10;
        }
    }
    let mut pos = 0;
    if negative {
        buf[pos] = '-';
        pos += 1;
    }
    for i in (0..dlen).rev() {
        buf[pos] = digits[i] as char;
        pos += 1;
    }
    pos
}

/// BigNum exponentiation: base^power using BigNumBuf.
fn bignum_pow(base: isize, power: usize) -> crate::bignum::BigNumBuf {
    use crate::bignum::BigNumBuf;
    if power == 0 {
        return BigNumBuf::from_isize(1);
    }
    let mut result = BigNumBuf::from_isize(1);
    let mut b = BigNumBuf::from_isize(base);
    let mut exp = power;
    while exp > 0 {
        if exp % 2 == 1 {
            result = result.mul(&b);
        }
        exp /= 2;
        if exp > 0 {
            b = b.mul(&b);
        }
    }
    result
}
