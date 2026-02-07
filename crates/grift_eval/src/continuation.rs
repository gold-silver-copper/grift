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
// Continuation Type Constants
// ============================================================================
//
// These constants identify the continuation type stored in each ContFrame.
// The data field of each ContFrame contains continuation-specific data
// encoded as cons cells in the arena.

/// We're done - return the value (no data)
pub const CONT_DONE: usize = 0;

/// After evaluating function, decide builtin vs lambda
/// Data: (args_expr . (env . call_expr))
pub const CONT_APPLY_FORCED: usize = 1;

/// After evaluating condition, choose branch
/// Data: (then_expr . (else_expr . env))
pub const CONT_IF_BRANCH: usize = 2;

/// After evaluating argument for builtin (variadic ops like +)
/// Data: (builtin_encoded . (remaining_args . (collected . (call_expr . eval_env))))
pub const CONT_BUILTIN_FORCE_ARG: usize = 3;

/// After evaluating first arg of binary builtin, evaluate second arg
/// Data: (builtin_encoded . (second_arg . (call_expr . eval_env)))
pub const CONT_BINARY_BUILTIN_FIRST: usize = 4;

/// After evaluating both args of binary builtin, apply
/// Data: (builtin_encoded . (first_val . call_expr))
pub const CONT_BINARY_BUILTIN_SECOND: usize = 5;

/// After evaluating first lambda arg, bind it to param
/// Data: param (single value)
pub const CONT_LAMBDA_FIRST_BIND: usize = 6;

/// After binding a lambda arg, continue with remaining args
/// Data: (remaining_exprs . (eval_env . (remaining_params . (body . (new_env . call_expr)))))
pub const CONT_LAMBDA_BIND_ARG: usize = 7;

/// Collecting rest arguments for rest-parameter lambda
/// Data: (remaining_exprs . (eval_env . (rest_param . (body . (new_env . (collected . call_expr))))))
pub const CONT_LAMBDA_REST_COLLECT: usize = 8;

/// After evaluating expr in eval special form
/// Data: env (single value)
pub const CONT_EVAL_EXPR: usize = 9;

/// Processing begin expressions (non-tail)
/// Data: (remaining . env)
pub const CONT_BEGIN_SEQ: usize = 10;

/// After evaluating first arg for apply, evaluate second arg (args list)
/// Data: (args_list_expr . env)
pub const CONT_APPLY_FIRST: usize = 11;

/// After evaluating both args for apply, perform the application
/// Data: (func . env)
pub const CONT_APPLY_SECOND: usize = 12;

/// Evaluate expressions for values, collecting results
/// Data: (remaining . (collected . env))
pub const CONT_VALUES_COLLECT: usize = 13;

/// After evaluating value for define
/// Data: name (single value)
pub const CONT_DEFINE_VALUE: usize = 14;

/// After evaluating value for set!
/// Data: (name . env)
pub const CONT_SET_VALUE: usize = 15;

/// Evaluate arguments for native function call
/// Data: (remaining . (collected . (id_encoded . env)))
pub const CONT_NATIVE_ARGS_COLLECT: usize = 16;

/// After evaluating car in quasiquote, evaluate cdr
/// Data: (cdr . (depth_encoded . env))
pub const CONT_QUASIQUOTE_CAR: usize = 17;

/// After evaluating cdr in quasiquote, cons with car
/// Data: car_val (single value)
pub const CONT_QUASIQUOTE_CDR: usize = 18;

/// After evaluating unquote in quasiquote at depth > 1, wrap with unquote symbol
/// Data: Nil (no data)
pub const CONT_QUASIQUOTE_UNQUOTE_WRAP: usize = 19;

/// After evaluating inner in nested quasiquote, wrap with quasiquote symbol
/// Data: Nil (no data)
pub const CONT_QUASIQUOTE_NESTED_WRAP: usize = 20;

/// After evaluating unquote-splicing, append with rest
/// Data: (cdr . (depth_encoded . env))
pub const CONT_QUASIQUOTE_SPLICE: usize = 21;

/// After evaluating cdr for splice, append with splice value
/// Data: splice_val (single value)
pub const CONT_QUASIQUOTE_SPLICE_APPEND: usize = 22;

/// After evaluating let-syntax body, restore macro environment
/// Data: saved_macro_env (single value)
pub const CONT_LET_SYNTAX_BODY: usize = 23;

/// After evaluating producer for call-with-values, evaluate consumer
/// Data: (consumer_expr . env)
pub const CONT_CALL_WITH_VALUES_PRODUCER: usize = 24;

/// After calling producer, evaluate consumer
/// Data: (consumer_expr . env)
pub const CONT_CALL_WITH_VALUES_CONSUMER: usize = 25;

/// After evaluating consumer, apply it to producer result
/// Data: (producer_result . env)
pub const CONT_CALL_WITH_VALUES_APPLY: usize = 26;

/// After evaluating stx-expr in syntax-case, try pattern matching
/// Data: (literals . (clauses . (env . pattern_bindings)))
pub const CONT_SYNTAX_CASE_MATCH: usize = 27;

/// After evaluating fender in syntax-case, decide to use this clause or continue
/// Data: (output . (bindings . (literals . (remaining_clauses . (env . stx)))))
pub const CONT_SYNTAX_CASE_FENDER: usize = 28;

/// After evaluating the procedure argument of call/cc, apply it to the captured continuation
/// Data: captured_continuation (single value)
pub const CONT_CALL_CC_APPLY: usize = 29;

/// After evaluating the argument to a captured continuation, restore and return
/// Data: captured_continuation (single value)
pub const CONT_CONTINUATION_APPLY: usize = 30;

/// After evaluating before thunk in dynamic-wind, call it (no args)
/// Data: (body . (after . (env . saved_dw_chain)))
pub const CONT_DYNAMIC_WIND_BEFORE: usize = 31;

/// After calling before thunk, evaluate and call body thunk
/// Data: (after . (env . saved_dw_chain))
pub const CONT_DYNAMIC_WIND_BODY: usize = 32;

/// After calling body thunk, evaluate and call after thunk
/// Data: (body_result . saved_dw_chain)
pub const CONT_DYNAMIC_WIND_AFTER: usize = 33;

/// After evaluating after thunk, call it (no args) and return body result
/// Data: (body_result . saved_dw_chain)
pub const CONT_DYNAMIC_WIND_AFTER_CALL: usize = 34;

/// Executing wind-in thunks (before thunks) during continuation restoration
/// Data: (remaining_frames . (return_val . (target_chain . original_target_chain)))
pub const CONT_WIND_IN: usize = 35;

/// Executing wind-out thunks (after thunks) during continuation restoration  
/// Data: (remaining_frames . (return_val . (target_chain . original_target_chain)))
pub const CONT_WIND_OUT: usize = 36;

/// After evaluating after_expr in dynamic-wind, evaluate and call body
/// Data: (before_thunk . (body_expr . (env . saved_dw_chain)))
pub const CONT_DYNAMIC_WIND_EVAL_AFTER: usize = 37;

/// After evaluating body_expr in dynamic-wind, call body thunk
/// Data: (after_thunk . (env . saved_dw_chain))
pub const CONT_DYNAMIC_WIND_CALL_BODY: usize = 38;

/// After winding out/in completes, finish restoring continuation
/// Data: (captured_continuation . return_val)
pub const CONT_FINISH_CONTINUATION_RESTORE: usize = 39;

// Note: CONT_WITH_SYNTAX_BIND (40) was removed - with-syntax is now a macro

/// After evaluating macro transformer body, re-expand the result
/// Data: env (single value - the environment to continue evaluation in)
/// 
/// This continuation enables iterative macro expansion without Rust stack recursion.
/// When a macro invocation is encountered, we push this continuation and evaluate
/// the transformer body. When the body returns, this continuation re-evaluates
/// the expanded result (which may itself be a macro invocation).
pub const CONT_MACRO_RESULT: usize = 40;

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
