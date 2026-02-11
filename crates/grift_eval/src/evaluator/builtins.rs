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

/// Collects characters written through `core::fmt::Write` into a small fixed buffer.
/// Used to capture the output of ICU4X full case mapping operations (fold, uppercase, lowercase).
/// The Unicode standard guarantees no single character expands to more than 3 characters
/// under any case mapping operation.
struct CaseMapCollector {
    chars: [char; 3],
    len: usize,
}

impl CaseMapCollector {
    fn new() -> Self {
        CaseMapCollector {
            chars: ['\0'; 3],
            len: 0,
        }
    }
}

impl core::fmt::Write for CaseMapCollector {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            if self.len < 3 {
                self.chars[self.len] = c;
                self.len += 1;
            }
        }
        Ok(())
    }
}

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
        // Special handling for error - creates error object and raises through exception system
        if matches!(builtin, Builtin::Error) {
            return self.apply_error_builtin(args, call_expr);
        }
        
        // Special handling for load - reads file and evaluates all expressions
        if matches!(builtin, Builtin::Load) {
            return self.apply_load_builtin(args, call_expr);
        }
        
        // Special handling for vector-map - needs to apply proc via trampolining
        if matches!(builtin, Builtin::VectorMap) {
            return self.apply_vector_map(args, call_expr);
        }
        
        // Special handling for vector-for-each - needs to apply proc via trampolining
        if matches!(builtin, Builtin::VectorForEach) {
            return self.apply_vector_for_each(args, call_expr);
        }

        // call-with-input-file / call-with-output-file — open port, apply proc, close port
        if matches!(builtin, Builtin::CallWithInputFile | Builtin::CallWithOutputFile) {
            return self.apply_call_with_file(builtin, args, call_expr);
        }

        // call-with-port — apply proc to port, close port when done
        if matches!(builtin, Builtin::CallWithPort) {
            return self.apply_call_with_port(args, call_expr);
        }

        // with-input-from-file / with-output-to-file — redirect current port, call thunk, restore
        if matches!(builtin, Builtin::WithInputFromFile | Builtin::WithOutputToFile) {
            return self.apply_with_file(builtin, args, call_expr);
        }

        // First-class procedure builtins (R7RS requires these to be values)
        if matches!(builtin, Builtin::Values) {
            // (values v ...) — return args list as multi-value result
            return Ok(TrampolineState::Return { val: args });
        }
        if matches!(builtin, Builtin::Apply) {
            return self.apply_apply_builtin(args, call_expr);
        }
        if matches!(builtin, Builtin::CallWithValues) {
            return self.apply_call_with_values_builtin(args, call_expr);
        }
        if matches!(builtin, Builtin::CallCc | Builtin::CallWithCurrentContinuation) {
            return self.apply_call_cc_builtin(args, call_expr);
        }
        if matches!(builtin, Builtin::DynamicWind) {
            return self.apply_dynamic_wind_builtin(args, call_expr);
        }
        if matches!(builtin, Builtin::WithExceptionHandler) {
            return self.apply_with_exception_handler_builtin(args, call_expr);
        }
        if matches!(builtin, Builtin::RaiseBuiltin) {
            return self.apply_raise_builtin(args, call_expr, false);
        }
        if matches!(builtin, Builtin::RaiseContinuable) {
            return self.apply_raise_builtin(args, call_expr, true);
        }
        if matches!(builtin, Builtin::EvalBuiltin) {
            return self.apply_eval_builtin(args, call_expr);
        }
        if matches!(builtin, Builtin::EnvironmentBuiltin) {
            return self.apply_environment_builtin(args, call_expr);
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
        
        self.cont(ContType::ApplyForced, EnvRef(env)).data3(first_args, env, call_expr)?;
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
        
        self.cont(ContType::ApplyForced, EnvRef(env)).data3(first_args, env, call_expr)?;
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
        self.cont(ContType::ApplyForced, EnvRef(env)).data3(args_list, env, call_expr)?;
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
        self.cont(ContType::ApplyForced, EnvRef(env)).data3(args_list, env, call_expr)?;
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
        self.cont(ContType::ApplyForced, EnvRef(env)).data3(args_list, env, new_call)?;
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
        self.cont(ContType::ApplyForced, EnvRef(env)).data3(cont_args, env, call)?;
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
        let global = self.global_env;
        self.cont(ContType::ExceptionHandlerFrame, global)
            .data3(handler, saved_chain, true_val)?;

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
    fn apply_environment_builtin(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
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
        let _ = call_expr; // suppress unused warning
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
            
            Builtin::Add => self.numeric_fold(args, 0, 
                |a, b| a.checked_add(b), 
                |a, b| a + b,
                call_expr),
            
            Builtin::Sub => {
                let first_idx = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                if self.lisp.get(rest)?.is_nil() {
                    // Unary minus
                    match self.lisp.get(first_idx)? {
                        Value::Number(n) => self.lisp.number(-n).map_err(Into::into),
                        Value::Float(f) => self.lisp.float(-f).map_err(Into::into),
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
                            // Start with float accumulator
                            self.numeric_fold_float(rest, first, |a, b| a - b, call_expr)
                        }
                        v => Err(self.type_error(call_expr, "number", v.type_name())),
                    }
                }
            }
            
            Builtin::Mul => self.numeric_fold(args, 1, 
                |a, b| a.checked_mul(b),
                |a, b| a * b,
                call_expr),
            
            Builtin::Div => {
                // Division: (/ n) => 1/n, (/ n m ...) => n/m/...
                let first_idx = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                match self.lisp.get(first_idx)? {
                    Value::Number(first) => {
                        self.numeric_fold(rest, first, 
                            |a, b| if b == 0 { None } else { a.checked_div(b) },
                            |a, b| a / b,
                            call_expr)
                    }
                    Value::Float(first) => {
                        self.numeric_fold_float(rest, first, |a, b| a / b, call_expr)
                    }
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
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
                            let result = int_pow(base, power as usize);
                            self.lisp.number(result).map_err(Into::into)
                        }
                    }
                    _ => {
                        let base_f = match base_val {
                            Value::Number(n) => n as fsize,
                            Value::Float(f) => f,
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
                // Square root - always returns float for non-perfect squares
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(n) => {
                        if n < 0 {
                            return Err(self.type_error(call_expr, "non-negative number", "negative integer"));
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
                        self.lisp.float(float_sqrt(f)).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            // integer? is true for exact integers and floats that are whole numbers
            Builtin::Integerp => {
                builtin_unary_pred!(self, args, |v: Value| match v {
                    Value::Number(_) => true,
                    Value::Float(f) => f.is_finite() && f == (f as isize as fsize),
                    _ => false,
                })
            }
            
            // exact? is true for integers (exact numbers)
            Builtin::Exactp => builtin_numeric_pred!(self, args, call_expr, |_n| true, |_f| false),
            
            Builtin::Inexactp => builtin_numeric_pred!(self, args, call_expr, |_n| false, |_f| true),
            
            Builtin::Finitep => builtin_numeric_pred!(self, args, call_expr, |_n| true, |f| f.is_finite()),
            
            Builtin::Infinitep => builtin_numeric_pred!(self, args, call_expr, |_n| false, |f| f.is_infinite()),
            
            Builtin::Nanp => builtin_numeric_pred!(self, args, call_expr, |_n| false, |f| f.is_nan()),
            
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
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::InexactToExact | Builtin::Exact => {
                let val = self.lisp.car(args)?;
                match self.lisp.get(val)? {
                    Value::Number(n) => self.lisp.number(n).map_err(Into::into),
                    Value::Float(f) => {
                        if f.is_finite() {
                            self.lisp.number(f as isize).map_err(Into::into)
                        } else {
                            Err(self.type_error(call_expr, "finite number", "infinite or nan"))
                        }
                    }
                    Value::Rational { num, denom } => self.lisp.rational(num, denom).map_err(Into::into),
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::Lt => self.compare_numbers(args, |a, b| a < b, call_expr),
            Builtin::Gt => self.compare_numbers(args, |a, b| a > b, call_expr),
            Builtin::Le => self.compare_numbers(args, |a, b| a <= b, call_expr),
            Builtin::Ge => self.compare_numbers(args, |a, b| a >= b, call_expr),
            Builtin::NumEq => self.compare_numbers(args, |a, b| a == b, call_expr),
            
            Builtin::Display => {
                let val = self.lisp.car(args)?;
                // Call output callback if set
                if let Some(callback) = self.output_callback {
                    callback(self.lisp, val);
                }
                // Return void (unspecified value) per R7RS
                self.lisp.void_val().map_err(Into::into)
            }
            
            Builtin::Newline => {
                // Call output callback with nil (marker for newline)
                if let Some(callback) = self.output_callback {
                    callback(self.lisp, self.lisp.nil()?);
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
                // (vector->list vec) - convert vector to list
                let vec = self.lisp.car(args)?;
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
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
                self.list_to_array(lst, call_expr)
            }
            
            Builtin::VectorFill => {
                // (vector-fill! vec fill) - fill vector with value
                extract_args!(self, args, vec, fill);
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
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
                // CaseMapper::new() is a const fn returning a reference to static compiled data (zero-cost)
                let cm = icu_casemap::CaseMapper::new();
                let result = match builtin {
                    Builtin::CharUpcase => cm.simple_uppercase(c),
                    Builtin::CharDowncase => cm.simple_lowercase(c),
                    _ => cm.simple_fold(c),
                };
                self.lisp.char(result).map_err(Into::into)
            }
            
            // Character classification predicates — all use icu4x CodePointSetData/CodePointMapData.
            // These ::new() calls are const fns returning references to static compiled data (zero-cost).
            Builtin::CharAlphabetic => {
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                self.lisp.boolean(icu_properties::CodePointSetData::new::<icu_properties::props::Alphabetic>().contains(c)).map_err(Into::into)
            }
            
            Builtin::CharNumeric => {
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                let gc = icu_properties::CodePointMapData::<icu_properties::props::GeneralCategory>::new().get(c);
                self.lisp.boolean(gc == icu_properties::props::GeneralCategory::DecimalNumber).map_err(Into::into)
            }
            
            Builtin::CharWhitespace => {
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                self.lisp.boolean(icu_properties::CodePointSetData::new::<icu_properties::props::WhiteSpace>().contains(c)).map_err(Into::into)
            }
            
            Builtin::CharUpperCase => {
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                self.lisp.boolean(icu_properties::CodePointSetData::new::<icu_properties::props::Uppercase>().contains(c)).map_err(Into::into)
            }
            
            Builtin::CharLowerCase => {
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                self.lisp.boolean(icu_properties::CodePointSetData::new::<icu_properties::props::Lowercase>().contains(c)).map_err(Into::into)
            }
            
            Builtin::DigitValue => {
                let c = self.get_char(self.lisp.car(args)?, call_expr)?;
                let map = icu_properties::CodePointMapData::<icu_properties::props::GeneralCategory>::new();
                if map.get(c) == icu_properties::props::GeneralCategory::DecimalNumber {
                    // Walk back to find the '0' digit of this block
                    let cp = c as u32;
                    let mut zero = cp;
                    while zero > 0 {
                        let prev = zero - 1;
                        if let Some(prev_char) = char::from_u32(prev) {
                            if map.get(prev_char) == icu_properties::props::GeneralCategory::DecimalNumber {
                                zero = prev;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    self.lisp.number((cp - zero) as isize).map_err(Into::into)
                } else {
                    self.lisp.boolean(false).map_err(Into::into)
                }
            }
            
            Builtin::CharCiEq | Builtin::CharCiLt | Builtin::CharCiGt
            | Builtin::CharCiLe | Builtin::CharCiGe => {
                // CaseMapper::new() is a const fn returning a reference to static compiled data (zero-cost)
                let cm = icu_casemap::CaseMapper::new();
                let cmp_fn: fn(char, char) -> bool = match builtin {
                    Builtin::CharCiEq => |a, b| a == b,
                    Builtin::CharCiLt => |a, b| a < b,
                    Builtin::CharCiGt => |a, b| a > b,
                    Builtin::CharCiLe => |a, b| a <= b,
                    _ => |a, b| a >= b,
                };
                self.char_chain_compare(args, |a, b| cmp_fn(cm.simple_fold(a), cm.simple_fold(b)), call_expr)
            }
            
            Builtin::StringUpcase | Builtin::StringDowncase | Builtin::StringFoldcase => {
                let str_idx = self.lisp.car(args)?;
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let cm = icu_casemap::CaseMapper::new();
                        let default_langid = icu_locale_core::LanguageIdentifier::UNKNOWN;
                        
                        // First pass: compute total length after full case mapping
                        // (full mapping can expand characters, e.g. ß → ss, ß → SS)
                        let mut total_len = 0usize;
                        for i in 0..len {
                            let slot = self.lisp.arena_index_at_offset(data, i)?;
                            if let Value::Char(c) = self.lisp.get(slot)? {
                                let mut utf8_buf = [0u8; 4];
                                let s = c.encode_utf8(&mut utf8_buf);
                                let mut collector = CaseMapCollector::new();
                                match builtin {
                                    Builtin::StringUpcase => { use writeable::Writeable; let _ = cm.uppercase(s, &default_langid).write_to(&mut collector); }
                                    Builtin::StringDowncase => { use writeable::Writeable; let _ = cm.lowercase(s, &default_langid).write_to(&mut collector); }
                                    _ => { use writeable::Writeable; let _ = cm.fold(s).write_to(&mut collector); }
                                }
                                total_len += collector.len;
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
                                let mut utf8_buf = [0u8; 4];
                                let s = c.encode_utf8(&mut utf8_buf);
                                let mut collector = CaseMapCollector::new();
                                match builtin {
                                    Builtin::StringUpcase => { use writeable::Writeable; let _ = cm.uppercase(s, &default_langid).write_to(&mut collector); }
                                    Builtin::StringDowncase => { use writeable::Writeable; let _ = cm.lowercase(s, &default_langid).write_to(&mut collector); }
                                    _ => { use writeable::Writeable; let _ = cm.fold(s).write_to(&mut collector); }
                                }
                                for j in 0..collector.len {
                                    self.lisp.string_set(result, pos, collector.chars[j])?;
                                    pos += 1;
                                }
                            } else {
                                return Err(self.make_error(ErrorKind::TypeError, call_expr));
                            }
                        }
                        
                        Ok(result)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringCiEq => {
                self.string_ci_chain_compare(args, |ordering| ordering == core::cmp::Ordering::Equal, call_expr)
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
                match self.lisp.get(str_idx)? {
                    Value::String { .. } => {
                        let len = self.lisp.string_len(str_idx)?;
                        self.lisp.number(len as isize).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringRef => {
                // (string-ref string k) - Get character at index
                extract_args!(self, args, str_idx, k_idx);
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
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
                    Value::String { len, data } => {
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
                // (string->list string) - Convert string to list of characters
                let str_idx = self.lisp.car(args)?;
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let mut result = self.lisp.nil()?;
                        // Build list from end to start
                        for i in (0..len).rev() {
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
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
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
                let str_idx = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let start_idx = self.lisp.car(rest)?;
                let rest2 = self.lisp.cdr(rest)?;
                let end_idx = self.lisp.car(rest2)?;
                
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let start = self.get_int(start_idx, call_expr)?;
                        let end = self.get_int(end_idx, call_expr)?;
                        
                        if start < 0 || end < 0 || (start as usize) > len || (end as usize) > len || start > end {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        
                        self.copy_string_range(data, start as usize, (end - start) as usize, call_expr)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringCopy => {
                // (string-copy string [start [end]]) - Copy a string
                let str_idx = self.lisp.car(args)?;
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let rest = self.lisp.cdr(args)?;
                        let (start, end) = self.parse_range_args(rest, len, call_expr)?;
                        self.copy_string_range(data, start, end - start, call_expr)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringCopyTo => {
                // (string-copy! to at from [start [end]]) - copy characters between strings
                let to_str = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let at_idx = self.lisp.car(rest)?;
                let rest2 = self.lisp.cdr(rest)?;
                let from_str = self.lisp.car(rest2)?;
                let rest3 = self.lisp.cdr(rest2)?;
                
                let at = match self.lisp.get(at_idx)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                let (to_len, _to_data, from_len, from_data) = match (self.lisp.get(to_str)?, self.lisp.get(from_str)?) {
                    (Value::String { len: tl, data: td }, Value::String { len: fl, data: fd }) => (tl, td, fl, fd),
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                let (start, end) = self.parse_range_args(rest3, from_len, call_expr)?;
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
                let str_idx = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let fill_arg = self.lisp.car(rest)?;
                let rest2 = self.lisp.cdr(rest)?;
                
                let fill_char = self.get_char(fill_arg, call_expr)?;
                
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let (start, end) = self.parse_range_args(rest2, len, call_expr)?;
                        
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
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringCiLt => {
                // (string-ci<? string1 string2 ...) - Case-insensitive less than
                self.string_ci_chain_compare(args, |ordering| ordering == core::cmp::Ordering::Less, call_expr)
            }
            
            Builtin::StringCiGt => {
                // (string-ci>? string1 string2 ...) - Case-insensitive greater than
                self.string_ci_chain_compare(args, |ordering| ordering == core::cmp::Ordering::Greater, call_expr)
            }
            
            Builtin::StringCiLe => {
                // (string-ci<=? string1 string2 ...) - Case-insensitive less than or equal
                self.string_ci_chain_compare(args, |ordering| ordering != core::cmp::Ordering::Greater, call_expr)
            }
            
            Builtin::StringCiGe => {
                // (string-ci>=? string1 string2 ...) - Case-insensitive greater than or equal
                self.string_ci_chain_compare(args, |ordering| ordering != core::cmp::Ordering::Less, call_expr)
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
                        // Format numerator
                        let negative = num < 0;
                        let mut val = num.unsigned_abs();
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
                        if negative {
                            chars[pos] = '-';
                            pos += 1;
                        }
                        for i in (0..dlen).rev() {
                            chars[pos] = digits[i] as char;
                            pos += 1;
                        }
                        chars[pos] = '/';
                        pos += 1;
                        // Format denominator
                        let mut val = denom.unsigned_abs();
                        dlen = 0;
                        while val > 0 {
                            digits[dlen] = b'0' + (val % 10) as u8;
                            dlen += 1;
                            val /= 10;
                        }
                        for i in (0..dlen).rev() {
                            chars[pos] = digits[i] as char;
                            pos += 1;
                        }
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
                        let mut buf = [0u8; 66];
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
                        
                        // Check for special float constants
                        match s {
                            "+inf.0" => return self.lisp.float(fsize::INFINITY).map_err(Into::into),
                            "-inf.0" => return self.lisp.float(fsize::NEG_INFINITY).map_err(Into::into),
                            "+nan.0" | "-nan.0" => return self.lisp.float(fsize::NAN).map_err(Into::into),
                            _ => {}
                        }
                        
                        // Detect prefix notation: #b, #o, #d, #x
                        let (num_str, radix) = if s.len() >= 2 && s.as_bytes()[0] == b'#' {
                            let prefix_radix = match s.as_bytes()[1] {
                                b'b' | b'B' => Some(2u32),
                                b'o' | b'O' => Some(8u32),
                                b'd' | b'D' => Some(10u32),
                                b'x' | b'X' => Some(16u32),
                                _ => None,
                            };
                            match prefix_radix {
                                Some(r) => (&s[2..], r),
                                None => return self.lisp.false_val().map_err(Into::into),
                            }
                        } else {
                            (s, explicit_radix.unwrap_or(10))
                        };
                        
                        if num_str.is_empty() {
                            return self.lisp.false_val().map_err(Into::into);
                        }
                        
                        if radix == 10 {
                            // Try integer parse first, then float
                            if let Ok(n) = num_str.parse::<isize>() {
                                return self.lisp.number(n).map_err(Into::into);
                            }
                            if let Some(f) = parse_float_no_std(num_str) {
                                return self.lisp.float(f).map_err(Into::into);
                            }
                            self.lisp.false_val().map_err(Into::into)
                        } else {
                            // Non-decimal radix: parse integer only
                            match parse_int_radix(num_str, radix) {
                                Some(n) => self.lisp.number(n).map_err(Into::into),
                                None => self.lisp.false_val().map_err(Into::into),
                            }
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

            Builtin::Write | Builtin::WriteShared | Builtin::WriteSimple => {
                // (write obj [port]) / (write-shared obj [port]) / (write-simple obj [port])
                // In this implementation, all behave identically since we don't
                // have circular structures.
                let val = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let pid = self.extract_output_port(rest, call_expr)?;
                if let Some(ref mut io) = self.io {
                    let dv = grift_parser::DisplayValue::new(val, self.lisp);
                    let mut writer = IoPortWriter { io: &mut **io, port: pid, error: false };
                    use core::fmt::Write;
                    let _ = write!(writer, "{}", dv);
                    if writer.error {
                        return Err(self.make_error(ErrorKind::Generic, call_expr));
                    }
                } else if let Some(callback) = self.output_callback {
                    callback(self.lisp, val);
                }
                self.lisp.void_val().map_err(Into::into)
            }

            Builtin::Read => {
                // (read) or (read port)
                let pid = self.extract_input_port(args, call_expr)?;
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
                match self.lisp.get(arg)? {
                    Value::String { len, data } => {
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
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
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
                let bv_len = match self.lisp.get(bv_arg)? {
                    Value::Bytevector { len, .. } => len,
                    v => return Err(self.type_error(call_expr, "bytevector", v.type_name())),
                };
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
                let bv_len = match self.lisp.get(bv_arg)? {
                    Value::Bytevector { len, .. } => len,
                    v => return Err(self.type_error(call_expr, "bytevector", v.type_name())),
                };
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
                let bv_len = match self.lisp.get(arg)? {
                    Value::Bytevector { len, .. } => len,
                    v => return Err(self.type_error(call_expr, "bytevector", v.type_name())),
                };
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
                let (str_len, str_data) = match self.lisp.get(str_arg)? {
                    Value::String { len, data } => (len, data),
                    v => return Err(self.type_error(call_expr, "string", v.type_name())),
                };
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
                Ok(list)
            }

            Builtin::InteractionEnvironment => {
                // (interaction-environment) -> mutable environment object
                let env = self.global_env.0;
                self.lisp.alloc(Value::Environment { env, mutable: true }).map_err(Into::into)
            }

            Builtin::SchemeReportEnvironment => {
                // (scheme-report-environment version)
                // Returns an environment corresponding to R^version RS (e.g., R5RS)
                let version = self.get_int(self.lisp.car(args)?, call_expr)?;
                if version != 5 && version != 7 {
                    return Err(self.type_error(call_expr, "version 5 or 7", "unsupported version"));
                }
                // Return the global environment as immutable (read-only snapshot)
                let env = self.global_env.0;
                self.lisp.alloc(Value::Environment { env, mutable: false }).map_err(Into::into)
            }

            Builtin::NullEnvironment => {
                // (null-environment version)
                // Returns a minimal environment with only syntax (no procedure bindings)
                let version = self.get_int(self.lisp.car(args)?, call_expr)?;
                if version != 5 && version != 7 {
                    return Err(self.type_error(call_expr, "version 5 or 7", "unsupported version"));
                }
                // Return an empty environment (immutable)
                let nil = self.lisp.nil()?;
                self.lisp.alloc(Value::Environment { env: nil, mutable: false }).map_err(Into::into)
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
                match self.lisp.get(bv)? {
                    Value::Bytevector { .. } => {
                        let len = self.lisp.bytevector_len(bv)?;
                        self.lisp.number(len as isize).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "bytevector", v.type_name())),
                }
            }

            Builtin::BytevectorU8Ref => {
                // (bytevector-u8-ref bytevector k) - Get byte at index
                extract_args!(self, args, bv, k_idx);
                match self.lisp.get(bv)? {
                    Value::Bytevector { .. } => {
                        let k = self.get_int(k_idx, call_expr)?;
                        if k < 0 {
                            return Err(self.type_error(call_expr, "non-negative integer", "negative integer"));
                        }
                        let elem = self.lisp.bytevector_get(bv, k as usize)?;
                        Ok(elem)
                    }
                    v => Err(self.type_error(call_expr, "bytevector", v.type_name())),
                }
            }

            Builtin::BytevectorU8Set => {
                // (bytevector-u8-set! bytevector k byte) - Set byte at index
                let bv = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let k_idx = self.lisp.car(rest)?;
                let rest2 = self.lisp.cdr(rest)?;
                let byte_idx = self.lisp.car(rest2)?;

                match self.lisp.get(bv)? {
                    Value::Bytevector { .. } => {
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
                    v => Err(self.type_error(call_expr, "bytevector", v.type_name())),
                }
            }

            Builtin::BytevectorCopy => {
                // (bytevector-copy bytevector [start [end]]) - Copy a bytevector
                let bv = self.lisp.car(args)?;
                match self.lisp.get(bv)? {
                    Value::Bytevector { .. } => {
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
                    v => Err(self.type_error(call_expr, "bytevector", v.type_name())),
                }
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
                match self.lisp.get(to)? {
                    Value::Bytevector { .. } => {}
                    v => return Err(self.type_error(call_expr, "bytevector", v.type_name())),
                }
                match self.lisp.get(from)? {
                    Value::Bytevector { .. } => {}
                    v => return Err(self.type_error(call_expr, "bytevector", v.type_name())),
                }

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
                for i in 0..copy_len {
                    let elem = self.lisp.bytevector_get(from, start + i)?;
                    let byte = match self.lisp.get(elem)? {
                        Value::Number(n) => n as u8,
                        _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                    };
                    self.lisp.bytevector_set(to, at + i, byte)?;
                }
                self.lisp.void_val().map_err(Into::into)
            }

            Builtin::Utf8ToString => {
                // (utf8->string bytevector [start [end]]) - Decode UTF-8 bytevector to string
                // Manual UTF-8 decoding from bytevector bytes, building a reversed cons list of chars
                let bv = self.lisp.car(args)?;
                match self.lisp.get(bv)? {
                    Value::Bytevector { .. } => {
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
                    v => Err(self.type_error(call_expr, "bytevector", v.type_name())),
                }
            }

            Builtin::StringToUtf8 => {
                // (string->utf8 string [start [end]]) - Encode string as UTF-8 bytevector
                let str_idx = self.lisp.car(args)?;
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
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
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
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
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
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
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
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

            Builtin::FloorQuotient => {
                let (q, _) = self.division_op(args, call_expr, libm::floor)?;
                self.return_exact_if_both_exact(args, q, call_expr)
            }

            Builtin::FloorRemainder => {
                let (_, r) = self.division_op(args, call_expr, libm::floor)?;
                self.return_exact_if_both_exact(args, r, call_expr)
            }

            Builtin::FloorDiv => self.division_values(args, call_expr, libm::floor),

            Builtin::TruncateQuotient => {
                let (q, _) = self.division_op(args, call_expr, libm::trunc)?;
                self.return_exact_if_both_exact(args, q, call_expr)
            }

            Builtin::TruncateRemainder => {
                let (_, r) = self.division_op(args, call_expr, libm::trunc)?;
                self.return_exact_if_both_exact(args, r, call_expr)
            }

            Builtin::TruncateDiv => self.division_values(args, call_expr, libm::trunc),

            // ================================================================
            // Rational number operations (R7RS §6.2.6)
            // ================================================================

            Builtin::Numerator => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(n) => self.lisp.number(n).map_err(Into::into),
                    Value::Float(f) => {
                        if !f.is_finite() {
                            return Err(self.type_error(call_expr, "finite number", "infinite or nan"));
                        }
                        let (num, _den) = float_to_rational(f as f64);
                        self.lisp.float(num as fsize).map_err(Into::into)
                    }
                    Value::Rational { num, .. } => self.lisp.number(num).map_err(Into::into),
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }

            Builtin::Denominator => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(_) => self.lisp.number(1).map_err(Into::into),
                    Value::Float(f) => {
                        if !f.is_finite() {
                            return Err(self.type_error(call_expr, "finite number", "infinite or nan"));
                        }
                        let (_num, den) = float_to_rational(f as f64);
                        self.lisp.float(den as fsize).map_err(Into::into)
                    }
                    Value::Rational { denom, .. } => self.lisp.number(denom).map_err(Into::into),
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }

            Builtin::Rationalize => {
                let x = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
                let tol = self.get_num_as_fsize(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                let (num, den) = rationalize_impl(x as f64, libm::fabs(tol as f64));
                if den == 1.0 {
                    let both_exact = matches!(self.lisp.get(self.lisp.car(args)?)?, Value::Number(_));
                    if both_exact {
                        self.lisp.number(num as isize).map_err(Into::into)
                    } else {
                        self.lisp.float(num as fsize).map_err(Into::into)
                    }
                } else {
                    self.lisp.float((num / den) as fsize).map_err(Into::into)
                }
            }

            // ================================================================
            // Exact integer square root (R7RS §6.2.6)
            // ================================================================

            Builtin::ExactIntegerSqrt => {
                let n = self.get_int(self.lisp.car(args)?, call_expr)?;
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

            // ================================================================
            // Complex number operations (R7RS §6.2.6)
            // ================================================================

            Builtin::MakeRectangular => {
                let a = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
                let b = self.get_num_as_fsize(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                if b == 0.0 {
                    return self.lisp.float(a).map_err(Into::into);
                }
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
                        if n >= 0 { self.lisp.float(0.0).map_err(Into::into) }
                        else { self.lisp.float(core::f64::consts::PI as fsize).map_err(Into::into) }
                    }
                    Value::Float(f) => {
                        if f >= 0.0 { self.lisp.float(0.0).map_err(Into::into) }
                        else { self.lisp.float(core::f64::consts::PI as fsize).map_err(Into::into) }
                    }
                    Value::Rational { num, .. } => {
                        if num >= 0 { self.lisp.float(0.0).map_err(Into::into) }
                        else { self.lisp.float(core::f64::consts::PI as fsize).map_err(Into::into) }
                    }
                    Value::Complex { real, imag } => {
                        let angle = libm::atan2(imag as f64, real as f64);
                        self.lisp.float(angle as fsize).map_err(Into::into)
                    }
                    Value::Cons { .. } => {
                        if self.is_complex_tagged(arg)? {
                            let re = self.complex_real_f(arg, call_expr)?;
                            let im = self.complex_imag_f(arg, call_expr)?;
                            let angle = libm::atan2(im, re);
                            self.lisp.float(angle as fsize).map_err(Into::into)
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
            Builtin::Mul => binary_int_op!(self, a, b, call_expr, 
                |x: isize, y: isize| x.checked_mul(y),
                |x: fsize, y: fsize| x * y),
            
            // Division operations with zero check
            Builtin::Div => binary_div_op!(self, a, b, call_expr, 
                |x, y| x / y,
                |x: fsize, y: fsize| x / y),
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
            Builtin::NumEq => binary_int_cmp!(self, a, b, call_expr, |x, y| x == y),
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
        let both_exact = matches!(self.lisp.get(a)?, Value::Number(_))
            && matches!(self.lisp.get(b)?, Value::Number(_));
        if both_exact {
            self.lisp.number(val as isize).map_err(Into::into)
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
            (Value::Float(x), Value::Float(y)) => x == y,
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
        let mut is_float = false;
        let mut current = args;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    return if is_float {
                        self.lisp.float(acc_float).map_err(Into::into)
                    } else {
                        self.lisp.number(acc_int).map_err(Into::into)
                    };
                }
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    match self.lisp.get(car)? {
                        Value::Number(n) => {
                            if is_float {
                                acc_float = float_f(acc_float, n as fsize);
                            } else {
                                match int_f(acc_int, n) {
                                    Some(r) => acc_int = r,
                                    None => return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)),
                                }
                            }
                        }
                        Value::Float(f) => {
                            if !is_float {
                                // Promote accumulator to float
                                acc_float = acc_int as fsize;
                                is_float = true;
                            }
                            acc_float = float_f(acc_float, f);
                        }
                        Value::Rational { num, denom } => {
                            // Promote to float for mixed arithmetic
                            if !is_float {
                                acc_float = acc_int as fsize;
                                is_float = true;
                            }
                            acc_float = float_f(acc_float, num as fsize / denom as fsize);
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
        let a = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
        let b = self.get_num_as_fsize(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
        self.lisp.boolean(cmp(a, b)).map_err(Into::into)
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
            let (len_a, data_a) = match self.lisp.get(a)? {
                Value::String { len, data } => (len, data),
                v => return Err(self.type_error(call_expr, "string", v.type_name())),
            };
            let (len_b, data_b) = match self.lisp.get(b)? {
                Value::String { len, data } => (len, data),
                v => return Err(self.type_error(call_expr, "string", v.type_name())),
            };
            
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
            // Case-insensitive: use full Unicode case folding via iterators.
            // Full folding can expand characters (e.g. ß → ss), so we produce
            // folded chars on the fly from each source string and compare them.
            let (len_a, data_a) = match self.lisp.get(a)? {
                Value::String { len, data } => (len, data),
                v => return Err(self.type_error(call_expr, "string", v.type_name())),
            };
            let (len_b, data_b) = match self.lisp.get(b)? {
                Value::String { len, data } => (len, data),
                v => return Err(self.type_error(call_expr, "string", v.type_name())),
            };
            
            let cm = icu_casemap::CaseMapper::new();
            
            // State for iterating folded chars from string A
            let mut src_a = 0usize;    // next source char index in string A
            let mut buf_a = CaseMapCollector::new();
            let mut buf_a_pos = 0usize; // position within buf_a
            
            // State for iterating folded chars from string B
            let mut src_b = 0usize;
            let mut buf_b = CaseMapCollector::new();
            let mut buf_b_pos = 0usize;
            
            loop {
                // Refill buffer A if needed
                while buf_a_pos >= buf_a.len && src_a < len_a {
                    let slot = self.lisp.arena_index_at_offset(data_a, src_a)?;
                    if let Value::Char(c) = self.lisp.get(slot)? {
                        let mut utf8_buf = [0u8; 4];
                        let s = c.encode_utf8(&mut utf8_buf);
                        buf_a = CaseMapCollector::new();
                        { use writeable::Writeable; let _ = cm.fold(s).write_to(&mut buf_a); }
                        buf_a_pos = 0;
                        src_a += 1;
                    } else {
                        return Err(self.make_error(ErrorKind::TypeError, call_expr));
                    }
                }
                
                // Refill buffer B if needed
                while buf_b_pos >= buf_b.len && src_b < len_b {
                    let slot = self.lisp.arena_index_at_offset(data_b, src_b)?;
                    if let Value::Char(c) = self.lisp.get(slot)? {
                        let mut utf8_buf = [0u8; 4];
                        let s = c.encode_utf8(&mut utf8_buf);
                        buf_b = CaseMapCollector::new();
                        { use writeable::Writeable; let _ = cm.fold(s).write_to(&mut buf_b); }
                        buf_b_pos = 0;
                        src_b += 1;
                    } else {
                        return Err(self.make_error(ErrorKind::TypeError, call_expr));
                    }
                }
                
                let has_a = buf_a_pos < buf_a.len;
                let has_b = buf_b_pos < buf_b.len;
                
                match (has_a, has_b) {
                    (false, false) => return Ok(core::cmp::Ordering::Equal),
                    (true, false) => return Ok(core::cmp::Ordering::Greater),
                    (false, true) => return Ok(core::cmp::Ordering::Less),
                    (true, true) => {
                        let ca = buf_a.chars[buf_a_pos];
                        let cb = buf_b.chars[buf_b_pos];
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
    /// Returns (proc, vecs_args, len) after validating all vectors have the same length.
    fn validate_vector_args(&self, args: ArenaIndex, call_expr: ArenaIndex) -> Result<(ArenaIndex, ArenaIndex, usize), EvalError> {
        let proc = self.lisp.car(args)?;
        let vecs_args = self.lisp.cdr(args)?;
        
        let first_vec = self.lisp.car(vecs_args)?;
        let len = match self.lisp.get(first_vec)? {
            Value::Array { .. } => self.lisp.array_len(first_vec)?,
            v => return Err(self.type_error(call_expr, "vector", v.type_name())),
        };
        
        let mut current = self.lisp.cdr(vecs_args)?;
        while !self.lisp.get(current)?.is_nil() {
            let vec = self.lisp.car(current)?;
            match self.lisp.get(vec)? {
                Value::Array { .. } => {
                    let vlen = self.lisp.array_len(vec)?;
                    if vlen != len {
                        return Err(self.make_error(ErrorKind::TypeError, call_expr));
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
        let n = self.get_num_as_fsize(self.lisp.car(args)?, call_expr)?;
        let d = self.get_num_as_fsize(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
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
        let io = match &mut self.io {
            Some(io) => io,
            None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
        };

        // Open a temp output string port to accumulate chars without a fixed-size limit
        let tmp = match io.open_output_string() {
            Ok(p) => p,
            Err(_) => return Err(self.make_error(ErrorKind::Generic, call_expr)),
        };

        let mut paren_depth: i32 = 0;
        let mut in_string = false;
        let mut escape = false;
        let mut got_token = false;

        // Skip leading whitespace
        loop {
            match io.read_char(pid) {
                Ok(c) => {
                    if !c.is_whitespace() {
                        if io.write_char(tmp, c).is_err() {
                            let _ = io.close_port(tmp);
                            return Err(self.make_error(ErrorKind::Generic, call_expr));
                        }

                        if c == '(' || c == '[' {
                            paren_depth += 1;
                        } else if c == '"' {
                            in_string = true;
                        } else if paren_depth == 0 {
                            got_token = true;
                        }
                        break;
                    }
                }
                Err(grift_parser::IoErrorKind::Eof) => {
                    let _ = io.close_port(tmp);
                    return self.lisp.eof().map_err(Into::into);
                }
                Err(_) => {
                    let _ = io.close_port(tmp);
                    return Err(self.make_error(ErrorKind::Generic, call_expr));
                }
            }
        }

        // Read remaining characters to form a complete expression
        if paren_depth > 0 || in_string {
            loop {
                match io.read_char(pid) {
                    Ok(c) => {
                        if io.write_char(tmp, c).is_err() {
                            let _ = io.close_port(tmp);
                            return Err(self.make_error(ErrorKind::Generic, call_expr));
                        }

                        if in_string {
                            if escape {
                                escape = false;
                            } else if c == '\\' {
                                escape = true;
                            } else if c == '"' {
                                in_string = false;
                                if paren_depth == 0 { break; }
                            }
                        } else if c == '(' || c == '[' {
                            paren_depth += 1;
                        } else if c == ')' || c == ']' {
                            paren_depth -= 1;
                            if paren_depth == 0 { break; }
                        } else if c == '"' {
                            in_string = true;
                        }
                    }
                    Err(grift_parser::IoErrorKind::Eof) => break,
                    Err(_) => {
                        let _ = io.close_port(tmp);
                        return Err(self.make_error(ErrorKind::Generic, call_expr));
                    }
                }
            }
        } else if got_token {
            // Continue reading the token (number, symbol, etc.)
            loop {
                match io.peek_char(pid) {
                    Ok(c) if c.is_whitespace() || c == '(' || c == ')' || c == '[' || c == ']' => break,
                    Ok(c) => {
                        let _ = io.read_char(pid);
                        if io.write_char(tmp, c).is_err() { break; }
                    }
                    Err(_) => break,
                }
            }
        }

        // Parse the accumulated expression from the temp port
        let result = {
            let s = match io.get_output_string(tmp) {
                Ok(s) => s,
                Err(_) => {
                    let _ = io.close_port(tmp);
                    return Err(self.make_error(ErrorKind::Generic, call_expr));
                }
            };
            if s.is_empty() {
                let _ = io.close_port(tmp);
                return self.lisp.eof().map_err(Into::into);
            }
            grift_parser::parse(self.lisp, s)
        };

        let _ = io.close_port(tmp);
        match result {
            Ok(expr) => Ok(expr),
            Err(e) => Err(EvalError::from(e)),
        }
    }

    /// Write the characters of a Scheme string into an output string port.
    /// Returns the PortId of the output string port containing the string content.
    /// The caller is responsible for closing the port after use.
    pub(super) fn string_to_output_port(&mut self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<grift_parser::PortId, EvalError> {
        let (len, data) = match self.lisp.get(idx)? {
            Value::String { len, data } => (len, data),
            v => return Err(self.type_error(call_expr, "string", v.type_name())),
        };
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
