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
/// Each variant stores a `data_start` offset into the evaluator's data stack.
/// The number of elements is fixed per variant (encoded in the variant type).
/// Use the corresponding pack_*/unpack_* methods in Evaluator to create and access data.
#[derive(Clone, Copy, Debug)]
pub enum Cont {
    /// We're done - return the value
    Done,

    /// After evaluating function, decide builtin vs lambda
    /// Stack data: [args_expr, env, call_expr] (3 elements)
    ApplyForced { data_start: usize },

    /// After evaluating condition, choose branch
    /// Stack data: [then_expr, else_expr, env] (3 elements)
    IfBranch { data_start: usize },

    /// After evaluating argument for builtin (variadic ops like +)
    /// Stack data: [builtin_as_u8, remaining_args, collected, call_expr, eval_env] (5 elements)
    /// Note: builtin stored as u8 discriminant to avoid arena allocation
    BuiltinForceArg { data_start: usize },

    /// OPTIMIZED: After evaluating first arg of binary builtin, evaluate second arg
    /// Stack data: [builtin_as_u8, second_arg, call_expr, eval_env] (4 elements)
    BinaryBuiltinFirst { data_start: usize },

    /// OPTIMIZED: After evaluating both args of binary builtin, apply
    /// Stack data: [builtin_as_u8, first_val, call_expr] (3 elements)
    BinaryBuiltinSecond { data_start: usize },

    /// After evaluating first lambda arg, bind it to param
    /// Stack data: [param] (1 element)
    LambdaFirstBind { data_start: usize },

    /// After binding a lambda arg, continue with remaining args
    /// Stack data: [remaining_exprs, eval_env, remaining_params, body, new_env, call_expr] (6 elements)
    LambdaBindArg { data_start: usize },

    /// After evaluating a let binding value, extend env and continue with remaining bindings
    /// Stack data: [remaining_bindings, new_env, original_env, body, name] (5 elements)
    LetBinding { data_start: usize },

    /// After evaluating a let* binding value, extend env and continue
    /// Stack data: [remaining_bindings, new_env, body, name] (4 elements)
    LetStarBinding { data_start: usize },

    /// After evaluating a letrec init expression, set! the variable and continue
    /// Stack data: [remaining_bindings, new_env, body, name] (4 elements)
    LetrecInit { data_start: usize },

    /// After evaluating test in when, decide whether to run body
    /// Stack data: [body, env] (2 elements)
    When { data_start: usize },

    /// After evaluating test in unless, decide whether to run body
    /// Stack data: [body, env] (2 elements)
    Unless { data_start: usize },

    /// After evaluating expr in eval special form
    /// Stack data: [env] (1 element)
    EvalExpr { data_start: usize },

    /// After evaluating cond test clause
    /// Stack data: [then_exprs, remaining_clauses, env] (3 elements)
    CondTest { data_start: usize },

    /// Processing and short-circuit evaluation
    /// Stack data: [remaining, env] (2 elements)
    And { data_start: usize },

    /// Processing or short-circuit evaluation
    /// Stack data: [remaining, env] (2 elements)
    Or { data_start: usize },

    /// Processing begin expressions (non-tail)
    /// Stack data: [remaining, env] (2 elements)
    BeginSeq { data_start: usize },

    // ========================================================================
    // Continuation types for fully trampolined evaluation
    // ========================================================================

    /// After evaluating key for case, check clauses
    /// Stack data: [clauses, env] (2 elements)
    CaseKey { data_start: usize },

    /// After evaluating a do init expression, bind and continue with remaining bindings
    /// Stack data: [remaining_bindings, var_steps, test_clause, body, loop_env, original_env, current_var] (7 elements)
    DoInit { data_start: usize },

    /// After evaluating do test, decide to exit or continue
    /// Stack data: [var_steps, test_clause, body, loop_env] (4 elements)
    DoTestResult { data_start: usize },

    /// Evaluate body expressions in do loop (for side effects)
    /// Stack data: [remaining_body, var_steps, test_clause, body, loop_env] (5 elements)
    DoBody { data_start: usize },

    /// Evaluate step expressions in do loop
    /// Stack data: [remaining_steps, collected_vals, var_steps, test_clause, body, loop_env, current_var] (7 elements)
    DoStep { data_start: usize },

    /// After evaluating first arg for apply, evaluate second arg (args list)
    /// Stack data: [args_list_expr, env] (2 elements)
    ApplyFirst { data_start: usize },

    /// After evaluating both args for apply, perform the application
    /// Stack data: [func, env] (2 elements)
    ApplySecond { data_start: usize },

    /// Evaluate expressions for values, collecting results
    /// Stack data: [remaining, collected, env] (3 elements)
    ValuesCollect { data_start: usize },

    /// After evaluating value for define
    /// Stack data: [name] (1 element)
    DefineValue { data_start: usize },

    /// After evaluating value for set!
    /// Stack data: [name, env] (2 elements)
    SetValue { data_start: usize },

    /// Evaluate arguments for native function call
    /// Stack data: [remaining, collected, id_as_usize, env] (4 elements)
    /// Note: id stored as raw usize bits in ArenaIndex
    NativeArgsCollect { data_start: usize },

    /// After evaluating car in quasiquote, evaluate cdr
    /// Stack data: [cdr, depth_as_usize, env] (3 elements)
    /// Note: depth stored as raw usize bits in ArenaIndex
    QuasiquoteCar { data_start: usize },

    /// After evaluating cdr in quasiquote, cons with car
    /// Stack data: [car_val] (1 element)
    QuasiquoteCdr { data_start: usize },

    /// After evaluating unquote in quasiquote at depth > 1, wrap with unquote symbol
    QuasiquoteUnquoteWrap,

    /// After evaluating inner in nested quasiquote, wrap with quasiquote symbol
    QuasiquoteNestedWrap,

    /// After evaluating unquote-splicing, append with rest
    /// Stack data: [cdr, depth_as_usize, env] (3 elements)
    QuasiquoteSplice { data_start: usize },

    /// After evaluating cdr for splice, append with splice value
    /// Stack data: [splice_val] (1 element)
    QuasiquoteSpliceAppend { data_start: usize },
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
            Cont::LambdaFirstBind { .. } => 1,
            Cont::EvalExpr { .. } => 1,
            Cont::DefineValue { .. } => 1,
            Cont::QuasiquoteCdr { .. } => 1,
            Cont::QuasiquoteSpliceAppend { .. } => 1,
            Cont::When { .. } => 2,
            Cont::Unless { .. } => 2,
            Cont::And { .. } => 2,
            Cont::Or { .. } => 2,
            Cont::BeginSeq { .. } => 2,
            Cont::CaseKey { .. } => 2,
            Cont::ApplyFirst { .. } => 2,
            Cont::ApplySecond { .. } => 2,
            Cont::SetValue { .. } => 2,
            Cont::ApplyForced { .. } => 3,
            Cont::IfBranch { .. } => 3,
            Cont::BinaryBuiltinSecond { .. } => 3,
            Cont::CondTest { .. } => 3,
            Cont::ValuesCollect { .. } => 3,
            Cont::QuasiquoteCar { .. } => 3,
            Cont::QuasiquoteSplice { .. } => 3,
            Cont::BinaryBuiltinFirst { .. } => 4,
            Cont::LetStarBinding { .. } => 4,
            Cont::LetrecInit { .. } => 4,
            Cont::DoTestResult { .. } => 4,
            Cont::NativeArgsCollect { .. } => 4,
            Cont::BuiltinForceArg { .. } => 5,
            Cont::LetBinding { .. } => 5,
            Cont::DoBody { .. } => 5,
            Cont::LambdaBindArg { .. } => 6,
            Cont::DoInit { .. } => 7,
            Cont::DoStep { .. } => 7,
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
