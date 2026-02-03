//! Continuation types for trampolined evaluation.
//!
//! Continuation data is stored in a separate data stack (not the arena).
//! Each Cont variant stores a `data_start` offset into the evaluator's data stack.
//! The number of elements is fixed per variant type.

use grift_parser::Builtin;

/// Maximum continuation stack depth
pub const MAX_CONT_DEPTH: usize = 1024;

/// Maximum data stack size (stores ArenaIndex values for continuation data)
/// Each continuation needs at most 7 values, so this is plenty.
pub const MAX_DATA_STACK: usize = MAX_CONT_DEPTH * 8;

/// Continuation - what to do after a computation completes
///
/// Each variant wraps a `usize` offset into the evaluator's data stack.
/// The number of elements is fixed per variant (encoded in the variant type).
/// Use the corresponding pack_*/unpack_* methods in Evaluator to create and access data.
#[derive(Clone, Copy, Debug)]
pub enum Cont {
    /// We're done - return the value
    Done,

    /// After evaluating function, decide builtin vs lambda
    /// Stack data: [args_expr, env, call_expr] (3 elements)
    ApplyForced(usize),

    /// After evaluating condition, choose branch
    /// Stack data: [then_expr, else_expr, env] (3 elements)
    IfBranch(usize),

    /// After evaluating argument for builtin (variadic ops like +)
    /// Stack data: [builtin_encoded, remaining_args, collected, call_expr, eval_env] (5 elements)
    /// Note: builtin stored as usize discriminant to avoid arena allocation
    BuiltinForceArg(usize),

    /// OPTIMIZED: After evaluating first arg of binary builtin, evaluate second arg
    /// Stack data: [builtin_encoded, second_arg, call_expr, eval_env] (4 elements)
    BinaryBuiltinFirst(usize),

    /// OPTIMIZED: After evaluating both args of binary builtin, apply
    /// Stack data: [builtin_encoded, first_val, call_expr] (3 elements)
    BinaryBuiltinSecond(usize),

    /// After evaluating first lambda arg, bind it to param
    /// Stack data: [param] (1 element)
    LambdaFirstBind(usize),

    /// After binding a lambda arg, continue with remaining args
    /// Stack data: [remaining_exprs, eval_env, remaining_params, body, new_env, call_expr] (6 elements)
    LambdaBindArg(usize),

    /// Collecting rest arguments for rest-parameter lambda
    /// Stack data: [remaining_exprs, eval_env, rest_param, body, new_env, collected, call_expr] (7 elements)
    LambdaRestCollect(usize),

    // Note: LetBinding, LetStarBinding, LetrecInit removed - now handled by macros
    // Note: When, Unless, CondTest, And, Or removed - now handled by macros

    /// After evaluating expr in eval special form
    /// Stack data: [env] (1 element)
    EvalExpr(usize),

    /// Processing begin expressions (non-tail)
    /// Stack data: [remaining, env] (2 elements)
    BeginSeq(usize),

    // ========================================================================
    // Continuation types for fully trampolined evaluation
    // ========================================================================

    // Note: CaseKey, DoInit, DoTestResult, DoBody, DoStep removed - now handled by macros (Phase 9)

    /// After evaluating first arg for apply, evaluate second arg (args list)
    /// Stack data: [args_list_expr, env] (2 elements)
    ApplyFirst(usize),

    /// After evaluating both args for apply, perform the application
    /// Stack data: [func, env] (2 elements)
    ApplySecond(usize),

    /// Evaluate expressions for values, collecting results
    /// Stack data: [remaining, collected, env] (3 elements)
    ValuesCollect(usize),

    /// After evaluating value for define
    /// Stack data: [name] (1 element)
    DefineValue(usize),

    /// After evaluating value for set!
    /// Stack data: [name, env] (2 elements)
    SetValue(usize),

    /// Evaluate arguments for native function call
    /// Stack data: [remaining, collected, id_as_usize, env] (4 elements)
    /// Note: id stored as raw usize bits in ArenaIndex
    NativeArgsCollect(usize),

    /// After evaluating car in quasiquote, evaluate cdr
    /// Stack data: [cdr, depth_as_usize, env] (3 elements)
    /// Note: depth stored as raw usize bits in ArenaIndex
    QuasiquoteCar(usize),

    /// After evaluating cdr in quasiquote, cons with car
    /// Stack data: [car_val] (1 element)
    QuasiquoteCdr(usize),

    /// After evaluating unquote in quasiquote at depth > 1, wrap with unquote symbol
    QuasiquoteUnquoteWrap,

    /// After evaluating inner in nested quasiquote, wrap with quasiquote symbol
    QuasiquoteNestedWrap,

    /// After evaluating unquote-splicing, append with rest
    /// Stack data: [cdr, depth_as_usize, env] (3 elements)
    QuasiquoteSplice(usize),

    /// After evaluating cdr for splice, append with splice value
    /// Stack data: [splice_val] (1 element)
    QuasiquoteSpliceAppend(usize),

    /// After evaluating let-syntax body, restore macro environment
    /// Stack data: [saved_macro_env] (1 element)
    LetSyntaxBody(usize),

    /// After evaluating producer for call-with-values, evaluate consumer
    /// Stack data: [consumer_expr, env] (2 elements)
    CallWithValuesProducer(usize),

    /// After calling producer, evaluate consumer
    /// Stack data: [consumer_expr, env] (2 elements)
    CallWithValuesConsumer(usize),

    /// After evaluating consumer, apply it to producer result
    /// Stack data: [producer_result, env] (2 elements)
    CallWithValuesApply(usize),

    /// After evaluating stx-expr in syntax-case, try pattern matching
    /// Stack data: [literals, clauses, env, pattern_bindings] (4 elements)
    SyntaxCaseMatch(usize),
}

impl Cont {
    /// Get the number of data stack elements this continuation uses.
    /// Used for restoring the data stack when popping.
    #[inline]
    pub const fn data_len(&self) -> usize {
        match self {
            Cont::Done => 0,
            Cont::QuasiquoteUnquoteWrap => 0,
            Cont::QuasiquoteNestedWrap => 0,
            Cont::LambdaFirstBind(_) => 1,
            Cont::EvalExpr(_) => 1,
            Cont::DefineValue(_) => 1,
            Cont::QuasiquoteCdr(_) => 1,
            Cont::QuasiquoteSpliceAppend(_) => 1,
            Cont::LetSyntaxBody(_) => 1,
            Cont::BeginSeq(_) => 2,
            // Note: CaseKey removed
            Cont::ApplyFirst(_) => 2,
            Cont::ApplySecond(_) => 2,
            Cont::SetValue(_) => 2,
            Cont::CallWithValuesProducer(_) => 2,
            Cont::CallWithValuesConsumer(_) => 2,
            Cont::CallWithValuesApply(_) => 2,
            Cont::ApplyForced(_) => 3,
            Cont::IfBranch(_) => 3,
            Cont::BinaryBuiltinSecond(_) => 3,
            Cont::ValuesCollect(_) => 3,
            Cont::QuasiquoteCar(_) => 3,
            Cont::QuasiquoteSplice(_) => 3,
            Cont::BinaryBuiltinFirst(_) => 4,
            // Note: DoTestResult removed
            Cont::NativeArgsCollect(_) => 4,
            Cont::SyntaxCaseMatch(_) => 4,
            Cont::BuiltinForceArg(_) => 5,
            // Note: DoBody removed
            Cont::LambdaBindArg(_) => 6,
            Cont::LambdaRestCollect(_) => 7,
            // Note: DoInit, DoStep removed
        }
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

/// Check if a builtin is a binary operation (exactly 2 args, optimized path)
pub fn is_binary_builtin(builtin: Builtin) -> bool {
    matches!(builtin, 
        Builtin::Add | Builtin::Sub | Builtin::Mul | Builtin::Div | Builtin::Modulo | Builtin::Remainder |
        Builtin::Lt | Builtin::Gt | Builtin::Le | Builtin::Ge | Builtin::NumEq |
        Builtin::EqP | Builtin::EqvP | Builtin::Cons
    )
}
