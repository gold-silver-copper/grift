//! Continuation type enum for arena-based trampolined evaluation.
//!
//! Each variant corresponds to a specific point in the evaluation where
//! a continuation is captured.

/// Continuation types for the arena-based trampoline.
///
/// Each variant corresponds to a specific point in the evaluation where
/// a continuation is captured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContType {
    /// We're done - return the value (no data)
    Done,

    /// After evaluating function, decide builtin vs lambda
    /// Data: (args_expr . (env . call_expr))
    ApplyForced,

    /// After evaluating condition, choose branch
    /// Data: (then_expr . (else_expr . env))
    IfBranch,

    /// After evaluating argument for builtin (variadic ops like +)
    /// Data: (builtin_encoded . (remaining_args . (collected . (call_expr . eval_env))))
    BuiltinForceArg,

    /// After evaluating first arg of binary builtin, evaluate second arg
    /// Data: (builtin_encoded . (second_arg . (call_expr . eval_env)))
    BinaryBuiltinFirst,

    /// After evaluating both args of binary builtin, apply
    /// Data: (builtin_encoded . (first_val . call_expr))
    BinaryBuiltinSecond,

    /// After evaluating first lambda arg, bind it to param
    /// Data: param (single value)
    LambdaFirstBind,

    /// After binding a lambda arg, continue with remaining args
    /// Data: (remaining_exprs . (eval_env . (remaining_params . (body . (new_env . call_expr)))))
    LambdaBindArg,

    /// Collecting rest arguments for rest-parameter lambda
    /// Data: (remaining_exprs . (eval_env . (rest_param . (body . (new_env . (collected . call_expr))))))
    LambdaRestCollect,

    /// After evaluating expr in eval special form
    /// Data: env (single value)
    EvalExpr,

    /// Processing begin expressions (non-tail)
    /// Data: (remaining . env)
    BeginSeq,

    /// After evaluating first arg for apply, evaluate second arg (args list)
    /// Data: (args_list_expr . env)
    ApplyFirst,

    /// After evaluating both args for apply, perform the application
    /// Data: (func . env)
    ApplySecond,

    /// Evaluate expressions for values, collecting results
    /// Data: (remaining . (collected . env))
    ValuesCollect,

    /// After evaluating value for define
    /// Data: name (single value)
    DefineValue,

    /// After evaluating value for set!
    /// Data: (name . env)
    SetValue,

    /// Evaluate arguments for native function call
    /// Data: (remaining . (collected . (id_encoded . env)))
    NativeArgsCollect,

    /// After evaluating car in quasiquote, evaluate cdr
    /// Data: (cdr . (depth_encoded . env))
    QuasiquoteCar,

    /// After evaluating cdr in quasiquote, cons with car
    /// Data: car_val (single value)
    QuasiquoteCdr,

    /// After evaluating unquote in quasiquote at depth > 1, wrap with unquote symbol
    /// Data: Nil (no data)
    QuasiquoteUnquoteWrap,

    /// After evaluating inner in nested quasiquote, wrap with quasiquote symbol
    /// Data: Nil (no data)
    QuasiquoteNestedWrap,

    /// After evaluating unquote-splicing, append with rest
    /// Data: (cdr . (depth_encoded . env))
    QuasiquoteSplice,

    /// After evaluating cdr for splice, append with splice value
    /// Data: splice_val (single value)
    QuasiquoteSpliceAppend,

    /// After evaluating let-syntax body, restore macro environment
    /// Data: saved_macro_env (single value)
    LetSyntaxBody,

    /// After evaluating producer for call-with-values, evaluate consumer
    /// Data: (consumer_expr . env)
    CallWithValuesProducer,

    /// After calling producer, evaluate consumer
    /// Data: (consumer_expr . env)
    CallWithValuesConsumer,

    /// After evaluating consumer, apply it to producer result
    /// Data: (producer_result . env)
    CallWithValuesApply,

    /// After evaluating stx-expr in syntax-case, try pattern matching
    /// Data: (literals . (clauses . (env . pattern_bindings)))
    SyntaxCaseMatch,

    /// After evaluating fender in syntax-case, decide to use this clause or continue
    /// Data: (output . (bindings . (literals . (remaining_clauses . (env . stx)))))
    SyntaxCaseFender,

    /// After evaluating the procedure argument of call/cc, apply it to the captured continuation
    /// Data: captured_continuation (single value)
    CallCcApply,

    /// After evaluating the argument to a captured continuation, restore and return
    /// Data: captured_continuation (single value)
    ContinuationApply,

    /// After evaluating before thunk in dynamic-wind, call it (no args)
    /// Data: (body . (after . (env . saved_dw_chain)))
    DynamicWindBefore,

    /// After calling before thunk, evaluate and call body thunk
    /// Data: (after . (env . saved_dw_chain))
    DynamicWindBody,

    /// After calling body thunk, evaluate and call after thunk
    /// Data: (body_result . saved_dw_chain)
    DynamicWindAfter,

    /// After evaluating after thunk, call it (no args) and return body result
    /// Data: (body_result . saved_dw_chain)
    DynamicWindAfterCall,

    /// Executing wind-in thunks (before thunks) during continuation restoration
    /// Data: (remaining_frames . (return_val . (target_chain . original_target_chain)))
    WindIn,

    /// Executing wind-out thunks (after thunks) during continuation restoration
    /// Data: (remaining_frames . (return_val . (target_chain . original_target_chain)))
    WindOut,

    /// After evaluating after_expr in dynamic-wind, evaluate and call body
    /// Data: (before_thunk . (body_expr . (env . saved_dw_chain)))
    DynamicWindEvalAfter,

    /// After evaluating body_expr in dynamic-wind, call body thunk
    /// Data: (after_thunk . (env . saved_dw_chain))
    DynamicWindCallBody,

    /// After winding out/in completes, finish restoring continuation
    /// Data: (captured_continuation . return_val)
    FinishContinuationRestore,

    /// After evaluating macro transformer body, re-expand the result
    /// Data: env (single value - the environment to continue evaluation in)
    ///
    /// This continuation enables iterative macro expansion without Rust stack recursion.
    /// When a macro invocation is encountered, we push this continuation and evaluate
    /// the transformer body. When the body returns, this continuation re-evaluates
    /// the expanded result (which may itself be a macro invocation).
    MacroResult,

    /// After evaluating handler-expr in with-exception-handler, evaluate thunk-expr
    /// Data: (thunk_expr . env)
    WithExceptionHandlerEvalThunk,

    /// After evaluating thunk-expr in with-exception-handler, call thunk with handler installed
    /// Data: (handler . env)
    WithExceptionHandlerCallThunk,

    /// Installed exception handler frame — marks the dynamic extent
    /// Data: (handler . saved_handler_chain)
    ExceptionHandlerFrame,

    /// After evaluating raise argument, invoke exception handler
    /// Data: Nil (no data — continuable flag encoded as env marker)
    RaiseEval,

    /// After evaluating env argument in 2-arg eval, extract env and evaluate expression
    /// Data: (expr_to_eval . eval_env) — expr still unevaluated, eval_env is the
    /// environment in which expr_to_eval should be evaluated
    EvalEnvArg,

    /// vector-map: After applying proc to current element, collect and continue
    /// Data: (proc . (vecs . (index_encoded . (len_encoded . (collected . call_expr)))))
    VectorMapStep,

    /// vector-for-each: After applying proc to current element, continue
    /// Data: (proc . (vecs . (index_encoded . (len_encoded . call_expr))))
    VectorForEachStep,

    /// call-with-input-file / call-with-output-file: After proc returns, close the port
    /// Data: port_id_encoded (single value - Number encoding port id)
    CallWithPortClose,

    /// with-input-from-file: After thunk returns, restore previous input port and close file port
    /// Data: (saved_port_encoded . file_port_encoded)
    WithInputFromFileRestore,

    /// with-output-to-file: After thunk returns, restore previous output port and close file port
    /// Data: (saved_port_encoded . file_port_encoded)
    WithOutputToFileRestore,

    /// Discard the incoming value and return a saved value instead.
    /// Used by first-class builtins (e.g., dynamic-wind) to bridge
    /// between continuation steps.
    /// Data: saved_value (single value)
    BuiltinReturnValue,

    /// Apply a function with pre-evaluated arguments (no re-evaluation).
    /// Used by apply, call-with-values, and other contexts where arguments
    /// are already evaluated values.
    /// Data: (args_list . (env . call_expr))
    ApplyDirect,
}
