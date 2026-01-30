//! Continuation types for trampolined evaluation.

use grift_parser::{ArenaIndex, Builtin};

/// Maximum continuation stack depth
pub const MAX_CONT_DEPTH: usize = 1024;

/// Continuation - what to do after a computation completes
#[derive(Clone, Copy, Debug)]
pub enum Cont {
    /// We're done - return the value
    Done,

    /// After evaluating function, decide builtin vs lambda
    ApplyForced { args_expr: ArenaIndex, env: ArenaIndex, call_expr: ArenaIndex },

    /// After evaluating condition, choose branch
    IfBranch { then_expr: ArenaIndex, else_expr: ArenaIndex, env: ArenaIndex },

    /// After evaluating argument for builtin (variadic ops like +)
    BuiltinForceArg { builtin: Builtin, remaining_args: ArenaIndex,
                      collected: ArenaIndex, call_expr: ArenaIndex, eval_env: ArenaIndex },

    /// OPTIMIZED: After evaluating first arg of binary builtin, evaluate second arg
    BinaryBuiltinFirst { builtin: Builtin, second_arg: ArenaIndex, call_expr: ArenaIndex, eval_env: ArenaIndex },

    /// OPTIMIZED: After evaluating both args of binary builtin, apply
    BinaryBuiltinSecond { builtin: Builtin, first_val: ArenaIndex, call_expr: ArenaIndex },

    /// After evaluating first lambda arg, bind it to param
    LambdaFirstBind { param: ArenaIndex },

    /// After binding a lambda arg, continue with remaining args
    /// data: ArenaIndex pointing to a cons-list in the arena containing:
    ///       (remaining_exprs . (eval_env . (remaining_params . (body . (new_env . (call_expr . nil))))))
    LambdaBindArg { data: ArenaIndex },

    /// After evaluating a let binding value, extend env and continue with remaining bindings
    /// remaining_bindings: remaining ((name value-expr) ...) to process
    /// new_env: environment being built with bindings
    /// original_env: environment for evaluating value expressions (for let, not let*)
    /// body: body expression to evaluate after all bindings
    LetBinding { remaining_bindings: ArenaIndex, new_env: ArenaIndex,
                 original_env: ArenaIndex, body: ArenaIndex, name: ArenaIndex },

    /// After evaluating a let* binding value, extend env and continue
    /// For let*, we use new_env for both extending AND evaluating
    LetStarBinding { remaining_bindings: ArenaIndex, new_env: ArenaIndex,
                     body: ArenaIndex, name: ArenaIndex },

    /// After evaluating a letrec init expression, set! the variable and continue
    /// remaining_bindings: remaining ((name init-expr) ...) to process
    /// new_env: environment with all names bound (initially to nil)
    /// body: body expression to evaluate after all inits
    LetrecInit { remaining_bindings: ArenaIndex, new_env: ArenaIndex,
                 body: ArenaIndex, name: ArenaIndex },

    /// After evaluating test in when, decide whether to run body
    When { body: ArenaIndex, env: ArenaIndex },

    /// After evaluating test in unless, decide whether to run body
    Unless { body: ArenaIndex, env: ArenaIndex },

    /// After evaluating expr in eval special form
    EvalExpr { env: ArenaIndex },

    /// After evaluating cond test clause
    CondTest { then_exprs: ArenaIndex, remaining_clauses: ArenaIndex, env: ArenaIndex },

    /// Processing and short-circuit evaluation
    /// remaining: remaining expressions to evaluate
    And { remaining: ArenaIndex, env: ArenaIndex },

    /// Processing or short-circuit evaluation
    /// remaining: remaining expressions to evaluate
    Or { remaining: ArenaIndex, env: ArenaIndex },

    /// Processing begin expressions (non-tail)
    BeginSeq { remaining: ArenaIndex, env: ArenaIndex },

    // ========================================================================
    // Continuation types for fully trampolined evaluation
    // (Replacing eval_preserving_stack)
    // ========================================================================

    /// After evaluating key for case, check clauses
    CaseKey { clauses: ArenaIndex, env: ArenaIndex },

    /// After evaluating a do init expression, bind and continue with remaining bindings
    /// remaining_bindings: remaining ((var init step) ...) to process
    /// var_steps: list of (var . step) pairs for iteration
    /// test_clause: (test result ...)
    /// body: body expressions
    /// loop_env: environment being built
    /// original_env: environment for evaluating init expressions
    DoInit { remaining_bindings: ArenaIndex, var_steps: ArenaIndex, test_clause: ArenaIndex,
             body: ArenaIndex, loop_env: ArenaIndex, original_env: ArenaIndex, current_var: ArenaIndex },

    /// After evaluating do test, decide to exit or continue
    DoTestResult { var_steps: ArenaIndex, test_clause: ArenaIndex, body: ArenaIndex, loop_env: ArenaIndex },

    /// Evaluate body expressions in do loop (for side effects)
    DoBody { remaining_body: ArenaIndex, var_steps: ArenaIndex, test_clause: ArenaIndex, 
             body: ArenaIndex, loop_env: ArenaIndex },

    /// Evaluate step expressions in do loop
    /// remaining_steps: remaining (var . step) pairs to evaluate
    /// collected_vals: list of evaluated (var . val) pairs
    DoStep { remaining_steps: ArenaIndex, collected_vals: ArenaIndex, var_steps: ArenaIndex, 
             test_clause: ArenaIndex, body: ArenaIndex, loop_env: ArenaIndex, current_var: ArenaIndex },

    /// After evaluating first arg for apply, evaluate second arg (args list)
    ApplyFirst { args_list_expr: ArenaIndex, env: ArenaIndex },

    /// After evaluating both args for apply, perform the application
    ApplySecond { func: ArenaIndex, env: ArenaIndex },

    /// Evaluate expressions for values, collecting results
    ValuesCollect { remaining: ArenaIndex, collected: ArenaIndex, env: ArenaIndex },

    /// After evaluating value for define
    DefineValue { name: ArenaIndex },

    /// After evaluating value for set!
    SetValue { name: ArenaIndex, env: ArenaIndex },

    /// Evaluate arguments for native function call
    NativeArgsCollect { remaining: ArenaIndex, collected: ArenaIndex, id: usize, env: ArenaIndex },

    /// After evaluating car in quasiquote, evaluate cdr
    QuasiquoteCar { cdr: ArenaIndex, depth: u8, env: ArenaIndex },

    /// After evaluating cdr in quasiquote, cons with car
    QuasiquoteCdr { car_val: ArenaIndex },

    /// After evaluating unquote in quasiquote at depth > 1, wrap with unquote symbol
    QuasiquoteUnquoteWrap,

    /// After evaluating inner in nested quasiquote, wrap with quasiquote symbol
    QuasiquoteNestedWrap,

    /// After evaluating unquote-splicing, append with rest
    QuasiquoteSplice { cdr: ArenaIndex, depth: u8, env: ArenaIndex },

    /// After evaluating cdr for splice, append with splice value
    QuasiquoteSpliceAppend { splice_val: ArenaIndex },
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
