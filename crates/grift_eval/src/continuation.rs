//! Continuation types for arena-based trampolined evaluation.
//!
//! Continuations are stored in the arena as a linked list of `ContFrame` values.
//! Each frame stores a continuation type (as usize), associated data, and a
//! reference to the parent continuation.
//!
//! This design enables O(1) capture for call/cc (just save the current pointer)
//! and natural structure sharing between continuations.

use grift_parser::{ArenaIndex, Builtin};

// ============================================================================
// GC Root Tracking
// ============================================================================

/// Trait for types that contain GC roots.
///
/// Implementors should call `tracer` for each [`ArenaIndex`] that represents
/// a live GC root. This ensures the garbage collector does not collect
/// reachable objects.
///
/// By centralizing root enumeration in this trait, adding a new
/// [`ArenaIndex`] field to a type will produce a compile-time reminder
/// (or at least a single, obvious place) to update root tracking, rather
/// than requiring updates in every GC call-site.
pub trait GcRoots {
    /// Call `tracer` once for every [`ArenaIndex`] that is a live GC root.
    fn trace_roots(&self, tracer: &mut dyn FnMut(ArenaIndex));
}

// ============================================================================
// Continuation Type Enum
// ============================================================================
//
// The `ContType` enum identifies the continuation type stored in each ContFrame.
// Using an enum provides exhaustiveness checking in `step_return()` and makes
// adding new continuation types compiler-checked.
//
// The data field of each ContFrame contains continuation-specific data
// encoded as cons cells in the arena.

/// Continuation types for the arena-based trampoline.
///
/// Each variant corresponds to a specific point in the evaluation where
/// a continuation is captured. The `#[repr(usize)]` ensures stable
/// discriminants for arena serialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ContType {
    /// We're done - return the value (no data)
    Done = 0,

    /// After evaluating function, decide builtin vs lambda
    /// Data: (args_expr . (env . call_expr))
    ApplyForced = 1,

    /// After evaluating condition, choose branch
    /// Data: (then_expr . (else_expr . env))
    IfBranch = 2,

    /// After evaluating argument for builtin (variadic ops like +)
    /// Data: (builtin_encoded . (remaining_args . (collected . (call_expr . eval_env))))
    BuiltinForceArg = 3,

    /// After evaluating first arg of binary builtin, evaluate second arg
    /// Data: (builtin_encoded . (second_arg . (call_expr . eval_env)))
    BinaryBuiltinFirst = 4,

    /// After evaluating both args of binary builtin, apply
    /// Data: (builtin_encoded . (first_val . call_expr))
    BinaryBuiltinSecond = 5,

    /// After evaluating first lambda arg, bind it to param
    /// Data: param (single value)
    LambdaFirstBind = 6,

    /// After binding a lambda arg, continue with remaining args
    /// Data: (remaining_exprs . (eval_env . (remaining_params . (body . (new_env . call_expr)))))
    LambdaBindArg = 7,

    /// Collecting rest arguments for rest-parameter lambda
    /// Data: (remaining_exprs . (eval_env . (rest_param . (body . (new_env . (collected . call_expr))))))
    LambdaRestCollect = 8,

    /// After evaluating expr in eval special form
    /// Data: env (single value)
    EvalExpr = 9,

    /// Processing begin expressions (non-tail)
    /// Data: (remaining . env)
    BeginSeq = 10,

    /// After evaluating first arg for apply, evaluate second arg (args list)
    /// Data: (args_list_expr . env)
    ApplyFirst = 11,

    /// After evaluating both args for apply, perform the application
    /// Data: (func . env)
    ApplySecond = 12,

    /// Evaluate expressions for values, collecting results
    /// Data: (remaining . (collected . env))
    ValuesCollect = 13,

    /// After evaluating value for define
    /// Data: name (single value)
    DefineValue = 14,

    /// After evaluating value for set!
    /// Data: (name . env)
    SetValue = 15,

    /// Evaluate arguments for native function call
    /// Data: (remaining . (collected . (id_encoded . env)))
    NativeArgsCollect = 16,

    /// After evaluating car in quasiquote, evaluate cdr
    /// Data: (cdr . (depth_encoded . env))
    QuasiquoteCar = 17,

    /// After evaluating cdr in quasiquote, cons with car
    /// Data: car_val (single value)
    QuasiquoteCdr = 18,

    /// After evaluating unquote in quasiquote at depth > 1, wrap with unquote symbol
    /// Data: Nil (no data)
    QuasiquoteUnquoteWrap = 19,

    /// After evaluating inner in nested quasiquote, wrap with quasiquote symbol
    /// Data: Nil (no data)
    QuasiquoteNestedWrap = 20,

    /// After evaluating unquote-splicing, append with rest
    /// Data: (cdr . (depth_encoded . env))
    QuasiquoteSplice = 21,

    /// After evaluating cdr for splice, append with splice value
    /// Data: splice_val (single value)
    QuasiquoteSpliceAppend = 22,

    /// After evaluating let-syntax body, restore macro environment
    /// Data: saved_macro_env (single value)
    LetSyntaxBody = 23,

    /// After evaluating producer for call-with-values, evaluate consumer
    /// Data: (consumer_expr . env)
    CallWithValuesProducer = 24,

    /// After calling producer, evaluate consumer
    /// Data: (consumer_expr . env)
    CallWithValuesConsumer = 25,

    /// After evaluating consumer, apply it to producer result
    /// Data: (producer_result . env)
    CallWithValuesApply = 26,

    /// After evaluating stx-expr in syntax-case, try pattern matching
    /// Data: (literals . (clauses . (env . pattern_bindings)))
    SyntaxCaseMatch = 27,

    /// After evaluating fender in syntax-case, decide to use this clause or continue
    /// Data: (output . (bindings . (literals . (remaining_clauses . (env . stx)))))
    SyntaxCaseFender = 28,

    /// After evaluating the procedure argument of call/cc, apply it to the captured continuation
    /// Data: captured_continuation (single value)
    CallCcApply = 29,

    /// After evaluating the argument to a captured continuation, restore and return
    /// Data: captured_continuation (single value)
    ContinuationApply = 30,

    /// After evaluating before thunk in dynamic-wind, call it (no args)
    /// Data: (body . (after . (env . saved_dw_chain)))
    DynamicWindBefore = 31,

    /// After calling before thunk, evaluate and call body thunk
    /// Data: (after . (env . saved_dw_chain))
    DynamicWindBody = 32,

    /// After calling body thunk, evaluate and call after thunk
    /// Data: (body_result . saved_dw_chain)
    DynamicWindAfter = 33,

    /// After evaluating after thunk, call it (no args) and return body result
    /// Data: (body_result . saved_dw_chain)
    DynamicWindAfterCall = 34,

    /// Executing wind-in thunks (before thunks) during continuation restoration
    /// Data: (remaining_frames . (return_val . (target_chain . original_target_chain)))
    WindIn = 35,

    /// Executing wind-out thunks (after thunks) during continuation restoration
    /// Data: (remaining_frames . (return_val . (target_chain . original_target_chain)))
    WindOut = 36,

    /// After evaluating after_expr in dynamic-wind, evaluate and call body
    /// Data: (before_thunk . (body_expr . (env . saved_dw_chain)))
    DynamicWindEvalAfter = 37,

    /// After evaluating body_expr in dynamic-wind, call body thunk
    /// Data: (after_thunk . (env . saved_dw_chain))
    DynamicWindCallBody = 38,

    /// After winding out/in completes, finish restoring continuation
    /// Data: (captured_continuation . return_val)
    FinishContinuationRestore = 39,

    /// After evaluating macro transformer body, re-expand the result
    /// Data: env (single value - the environment to continue evaluation in)
    ///
    /// This continuation enables iterative macro expansion without Rust stack recursion.
    /// When a macro invocation is encountered, we push this continuation and evaluate
    /// the transformer body. When the body returns, this continuation re-evaluates
    /// the expanded result (which may itself be a macro invocation).
    MacroResult = 40,
}

impl ContType {
    /// Convert from a `usize` discriminant (as stored in the arena).
    ///
    /// Returns `None` if the value does not correspond to a valid variant.
    pub fn from_usize(n: usize) -> Option<ContType> {
        match n {
            0 => Some(ContType::Done),
            1 => Some(ContType::ApplyForced),
            2 => Some(ContType::IfBranch),
            3 => Some(ContType::BuiltinForceArg),
            4 => Some(ContType::BinaryBuiltinFirst),
            5 => Some(ContType::BinaryBuiltinSecond),
            6 => Some(ContType::LambdaFirstBind),
            7 => Some(ContType::LambdaBindArg),
            8 => Some(ContType::LambdaRestCollect),
            9 => Some(ContType::EvalExpr),
            10 => Some(ContType::BeginSeq),
            11 => Some(ContType::ApplyFirst),
            12 => Some(ContType::ApplySecond),
            13 => Some(ContType::ValuesCollect),
            14 => Some(ContType::DefineValue),
            15 => Some(ContType::SetValue),
            16 => Some(ContType::NativeArgsCollect),
            17 => Some(ContType::QuasiquoteCar),
            18 => Some(ContType::QuasiquoteCdr),
            19 => Some(ContType::QuasiquoteUnquoteWrap),
            20 => Some(ContType::QuasiquoteNestedWrap),
            21 => Some(ContType::QuasiquoteSplice),
            22 => Some(ContType::QuasiquoteSpliceAppend),
            23 => Some(ContType::LetSyntaxBody),
            24 => Some(ContType::CallWithValuesProducer),
            25 => Some(ContType::CallWithValuesConsumer),
            26 => Some(ContType::CallWithValuesApply),
            27 => Some(ContType::SyntaxCaseMatch),
            28 => Some(ContType::SyntaxCaseFender),
            29 => Some(ContType::CallCcApply),
            30 => Some(ContType::ContinuationApply),
            31 => Some(ContType::DynamicWindBefore),
            32 => Some(ContType::DynamicWindBody),
            33 => Some(ContType::DynamicWindAfter),
            34 => Some(ContType::DynamicWindAfterCall),
            35 => Some(ContType::WindIn),
            36 => Some(ContType::WindOut),
            37 => Some(ContType::DynamicWindEvalAfter),
            38 => Some(ContType::DynamicWindCallBody),
            39 => Some(ContType::FinishContinuationRestore),
            40 => Some(ContType::MacroResult),
            _ => None,
        }
    }

    /// Convert to `usize` discriminant for arena storage.
    #[inline]
    pub const fn as_usize(self) -> usize {
        self as usize
    }
}

/// Trampoline state - what we're currently doing
#[derive(Clone, Copy, Debug)]
pub enum TrampolineState {
    /// Evaluate expression in environment
    Eval { expr: grift_parser::ArenaIndex, env: grift_parser::ArenaIndex },
    /// Return a value to the continuation
    Return { val: grift_parser::ArenaIndex },
}

impl GcRoots for TrampolineState {
    fn trace_roots(&self, tracer: &mut dyn FnMut(ArenaIndex)) {
        match self {
            TrampolineState::Eval { expr, env } => {
                tracer(*expr);
                tracer(*env);
            }
            TrampolineState::Return { val } => {
                tracer(*val);
            }
        }
    }
}

/// Check if a builtin is a binary operation (exactly 2 args, optimized path)
pub fn is_binary_builtin(builtin: Builtin) -> bool {
    matches!(builtin, 
        Builtin::Add | Builtin::Sub | Builtin::Mul | Builtin::Div | Builtin::Modulo | Builtin::Remainder |
        Builtin::Lt | Builtin::Gt | Builtin::Le | Builtin::Ge | Builtin::NumEq |
        Builtin::EqP | Builtin::EqvP | Builtin::Cons
    )
}
