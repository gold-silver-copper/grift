//! Continuation types for trampolined evaluation.
//!
//! All continuation variants (except Done, QuasiquoteUnquoteWrap, and QuasiquoteNestedWrap)
//! store their data as a single ArenaIndex pointing to contiguous Ref slots in the arena.
//! This is much more efficient than cons-lists (e.g., 3 values = 3 slots vs 9 slots for cons).

use grift_parser::{ArenaIndex, Builtin};

/// Maximum continuation stack depth
pub const MAX_CONT_DEPTH: usize = 1024;

/// Continuation - what to do after a computation completes
///
/// Each variant stores its data as an ArenaIndex pointing to contiguous Ref slots in the arena.
/// Use the corresponding pack_*/unpack_* methods in Evaluator to create and access data.
#[derive(Clone, Copy, Debug)]
pub enum Cont {
    /// We're done - return the value
    Done,

    /// After evaluating function, decide builtin vs lambda
    /// data: [args_expr, env, call_expr]
    ApplyForced { data: ArenaIndex },

    /// After evaluating condition, choose branch
    /// data: [then_expr, else_expr, env]
    IfBranch { data: ArenaIndex },

    /// After evaluating argument for builtin (variadic ops like +)
    /// data: [builtin_val, remaining_args, collected, call_expr, eval_env]
    BuiltinForceArg { data: ArenaIndex },

    /// OPTIMIZED: After evaluating first arg of binary builtin, evaluate second arg
    /// data: [builtin_val, second_arg, call_expr, eval_env]
    BinaryBuiltinFirst { data: ArenaIndex },

    /// OPTIMIZED: After evaluating both args of binary builtin, apply
    /// data: [builtin_val, first_val, call_expr]
    BinaryBuiltinSecond { data: ArenaIndex },

    /// After evaluating first lambda arg, bind it to param
    /// data: [param]
    LambdaFirstBind { data: ArenaIndex },

    /// After binding a lambda arg, continue with remaining args
    /// data: [remaining_exprs, eval_env, remaining_params, body, new_env, call_expr]
    LambdaBindArg { data: ArenaIndex },

    /// After evaluating a let binding value, extend env and continue with remaining bindings
    /// data: [remaining_bindings, new_env, original_env, body, name]
    LetBinding { data: ArenaIndex },

    /// After evaluating a let* binding value, extend env and continue
    /// data: [remaining_bindings, new_env, body, name]
    LetStarBinding { data: ArenaIndex },

    /// After evaluating a letrec init expression, set! the variable and continue
    /// data: [remaining_bindings, new_env, body, name]
    LetrecInit { data: ArenaIndex },

    /// After evaluating test in when, decide whether to run body
    /// data: [body, env]
    When { data: ArenaIndex },

    /// After evaluating test in unless, decide whether to run body
    /// data: [body, env]
    Unless { data: ArenaIndex },

    /// After evaluating expr in eval special form
    /// data: [env]
    EvalExpr { data: ArenaIndex },

    /// After evaluating cond test clause
    /// data: [then_exprs, remaining_clauses, env]
    CondTest { data: ArenaIndex },

    /// Processing and short-circuit evaluation
    /// data: [remaining, env]
    And { data: ArenaIndex },

    /// Processing or short-circuit evaluation
    /// data: [remaining, env]
    Or { data: ArenaIndex },

    /// Processing begin expressions (non-tail)
    /// data: [remaining, env]
    BeginSeq { data: ArenaIndex },

    // ========================================================================
    // Continuation types for fully trampolined evaluation
    // (Replacing eval_preserving_stack)
    // ========================================================================

    /// After evaluating key for case, check clauses
    /// data: [clauses, env]
    CaseKey { data: ArenaIndex },

    /// After evaluating a do init expression, bind and continue with remaining bindings
    /// data: [remaining_bindings, var_steps, test_clause, body, loop_env, original_env, current_var]
    DoInit { data: ArenaIndex },

    /// After evaluating do test, decide to exit or continue
    /// data: [var_steps, test_clause, body, loop_env]
    DoTestResult { data: ArenaIndex },

    /// Evaluate body expressions in do loop (for side effects)
    /// data: [remaining_body, var_steps, test_clause, body, loop_env]
    DoBody { data: ArenaIndex },

    /// Evaluate step expressions in do loop
    /// data: [remaining_steps, collected_vals, var_steps, test_clause, body, loop_env, current_var]
    DoStep { data: ArenaIndex },

    /// After evaluating first arg for apply, evaluate second arg (args list)
    /// data: [args_list_expr, env]
    ApplyFirst { data: ArenaIndex },

    /// After evaluating both args for apply, perform the application
    /// data: [func, env]
    ApplySecond { data: ArenaIndex },

    /// Evaluate expressions for values, collecting results
    /// data: [remaining, collected, env]
    ValuesCollect { data: ArenaIndex },

    /// After evaluating value for define
    /// data: [name]
    DefineValue { data: ArenaIndex },

    /// After evaluating value for set!
    /// data: [name, env]
    SetValue { data: ArenaIndex },

    /// Evaluate arguments for native function call
    /// data: [remaining, collected, id_val, env]
    NativeArgsCollect { data: ArenaIndex },

    /// After evaluating car in quasiquote, evaluate cdr
    /// data: [cdr, depth_val, env]
    QuasiquoteCar { data: ArenaIndex },

    /// After evaluating cdr in quasiquote, cons with car
    /// data: [car_val]
    QuasiquoteCdr { data: ArenaIndex },

    /// After evaluating unquote in quasiquote at depth > 1, wrap with unquote symbol
    QuasiquoteUnquoteWrap,

    /// After evaluating inner in nested quasiquote, wrap with quasiquote symbol
    QuasiquoteNestedWrap,

    /// After evaluating unquote-splicing, append with rest
    /// data: [cdr, depth_val, env]
    QuasiquoteSplice { data: ArenaIndex },

    /// After evaluating cdr for splice, append with splice value
    /// data: [splice_val]
    QuasiquoteSpliceAppend { data: ArenaIndex },
}

/// Trampoline state - what we're currently doing
#[derive(Clone, Copy, Debug)]
pub enum TrampolineState {
    /// Evaluate expression in environment
    Eval { expr: ArenaIndex, env: ArenaIndex },
    /// Return a value to the continuation
    Return { val: ArenaIndex },
}

/// Check if a builtin is a binary operation (exactly 2 args, optimized path)
pub fn is_binary_builtin(builtin: Builtin) -> bool {
    matches!(builtin, 
        Builtin::Add | Builtin::Sub | Builtin::Mul | Builtin::Div | Builtin::Modulo | Builtin::Remainder |
        Builtin::Lt | Builtin::Gt | Builtin::Le | Builtin::Ge | Builtin::NumEq |
        Builtin::EqP | Builtin::EqvP | Builtin::Cons
    )
}
