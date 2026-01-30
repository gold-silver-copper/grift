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
    LambdaBindArg { remaining_exprs: ArenaIndex, eval_env: ArenaIndex,
                    remaining_params: ArenaIndex, body: ArenaIndex,
                    new_env: ArenaIndex, call_expr: ArenaIndex },

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

    /// After evaluating test in when/unless, decide whether to run body
    /// is_when: true for when, false for unless
    WhenUnless { body: ArenaIndex, env: ArenaIndex, is_when: bool },

    /// After evaluating expr in eval special form
    EvalExpr { env: ArenaIndex },

    /// After evaluating cond test clause
    CondTest { then_exprs: ArenaIndex, remaining_clauses: ArenaIndex, env: ArenaIndex },

    /// Processing and/or short-circuit evaluation
    /// remaining: remaining expressions to evaluate
    /// is_and: true for and, false for or
    AndOr { remaining: ArenaIndex, env: ArenaIndex, is_and: bool },

    /// Processing begin expressions (non-tail)
    BeginSeq { remaining: ArenaIndex, env: ArenaIndex },
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
