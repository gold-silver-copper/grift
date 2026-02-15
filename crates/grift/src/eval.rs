//! Lisp evaluator with call-by-need (lazy) semantics.
//!
//! Evaluates arena-allocated S-expressions in an environment using
//! call-by-need evaluation with memoization and tail-call optimization.

use grift_arena::{ArenaIndex, ArenaError, ArenaResult};

use crate::lisp::Lisp;
use crate::value::Value;

/// Declare all built-in functions and special forms in one place.
///
/// **Built-in functions** (section `builtins { ... }`) are registered in the
/// global environment as `Value::Builtin(id)` with auto-assigned sequential
/// IDs.  Their arguments are evaluated and forced before the handler is called.
///
/// **Special forms** (section `special_forms { ... }`) are matched by symbol
/// name during evaluation and receive their arguments *unevaluated*.
macro_rules! define_builtins {
    (
        builtins {
            $( $bname:literal => $bmethod:ident ),* $(,)?
        }
        special_forms {
            $( $sname:literal => $smethod:ident ),* $(,)?
        }
    ) => {
        // Generate unique constant IDs for each builtin.
        define_builtins!(@consts 0u8, $( $bname, $bmethod; )* );

        impl<'a, const N: usize> Evaluator<'a, N> {
            /// Register all built-in functions in the global environment.
            fn init_builtins(&mut self) {
                $(
                    if let (Ok(sym), Ok(val)) = (
                        self.lisp.symbol($bname),
                        self.lisp.arena.alloc(Value::Builtin(
                            define_builtins!(@const_name $bmethod)
                        )),
                    ) {
                        if let Ok(new_env) = env_bind(self.lisp, self.global_env, sym, val) {
                            self.global_env = new_env;
                        }
                    }
                )*
            }

            /// Apply a built-in function.
            #[allow(non_upper_case_globals)]
            fn apply_builtin(&mut self, id: u8, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
                match id {
                    $( define_builtins!(@const_name $bmethod) => self.$bmethod(args), )*
                    _ => Err(ArenaError::InvalidIndex),
                }
            }

            /// Try to dispatch a special form by symbol name (TCO-aware).
            ///
            /// Returns `Some(TailAction)` if `car` matched a special form,
            /// `None` otherwise (fall through to function application).
            fn try_special_form_tco(
                &mut self,
                car: ArenaIndex,
                cdr: ArenaIndex,
                expr: &mut ArenaIndex,
                env: &mut ArenaIndex,
            ) -> Option<TailAction> {
                $(
                    if self.lisp.symbol_name_eq(car, $sname) {
                        return Some(self.$smethod(cdr, expr, env));
                    }
                )*
                None
            }
        }
    };

    // Generate const declarations recursively with incrementing IDs.
    (@consts $id:expr, $name:literal, $method:ident; $( $rest_name:literal, $rest_method:ident; )* ) => {
        define_builtins!(@make_const $method, $id);
        define_builtins!(@consts $id + 1u8, $( $rest_name, $rest_method; )* );
    };
    (@consts $id:expr, ) => {};

    // Generate a single const with a name derived from the method name.
    (@make_const $method:ident, $id:expr) => {
        #[allow(non_upper_case_globals)]
        const $method: u8 = $id;
    };

    // Reference a const by method name.
    (@const_name $method:ident) => { $method };
}

// Invoke the macro to generate `init_builtins`, `apply_builtin`,
// and `try_special_form_tco`.
define_builtins! {
    builtins {
        "+"        => builtin_add,
        "-"        => builtin_sub,
        "*"        => builtin_mul,
        "/"        => builtin_div,
        "="        => builtin_eq,
        "<"        => builtin_lt,
        ">"        => builtin_gt,
        "<="       => builtin_le,
        ">="       => builtin_ge,
        "car"      => builtin_car,
        "cdr"      => builtin_cdr,
        "list"     => builtin_list,
        "null?"    => builtin_nullp,
        "not"      => builtin_not,
        "pair?"    => builtin_pairp,
        "number?"  => builtin_numberp,
        "symbol?"  => builtin_symbolp,
        "boolean?" => builtin_booleanp,
    }
    special_forms {
        "quote"  => eval_quote,
        "if"     => eval_if,
        "define" => eval_define,
        "set!"   => eval_set,
        "lambda" => eval_lambda,
        "begin"  => eval_begin,
        "cond"   => eval_cond,
        "and"    => eval_and,
        "or"     => eval_or,
        "let"    => eval_let,
        "cons"   => eval_cons,
    }
}

/// TCO control flow for special forms.
enum TailAction {
    /// Return this value immediately (non-tail position result).
    Return(ArenaResult<ArenaIndex>),
    /// expr and env have been updated; re-enter the eval loop.
    Continue,
}

/// The evaluator state.
pub(crate) struct Evaluator<'a, const N: usize> {
    lisp: &'a Lisp<N>,
    pub global_env: ArenaIndex,
}

/// Bind a name to a value in an environment, returning the new environment.
fn env_bind<const N: usize>(
    lisp: &Lisp<N>,
    env: ArenaIndex,
    name: ArenaIndex,
    val: ArenaIndex,
) -> ArenaResult<ArenaIndex> {
    let pair = lisp.cons(name, val)?;
    lisp.cons(pair, env)
}

/// Look up a name in an environment.
fn env_lookup<const N: usize>(
    lisp: &Lisp<N>,
    env: ArenaIndex,
    name: ArenaIndex,
) -> ArenaResult<ArenaIndex> {
    let mut cur = env;
    while !cur.is_nil() {
        let binding = lisp.car(cur)?;
        let key = lisp.car(binding)?;
        if key == name {
            return lisp.cdr(binding);
        }
        cur = lisp.cdr(cur)?;
    }
    Err(ArenaError::InvalidIndex)
}

/// Set a binding in an environment (mutate existing binding).
fn env_set<const N: usize>(
    lisp: &Lisp<N>,
    env: ArenaIndex,
    name: ArenaIndex,
    val: ArenaIndex,
) -> ArenaResult<()> {
    let mut cur = env;
    while !cur.is_nil() {
        let binding = lisp.car(cur)?;
        let key = lisp.car(binding)?;
        if key == name {
            // Mutate the cdr of the binding pair
            lisp.arena.set(
                binding,
                Value::Cons {
                    car: key,
                    cdr: val,
                },
            )?;
            return Ok(());
        }
        cur = lisp.cdr(cur)?;
    }
    Err(ArenaError::InvalidIndex)
}

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Create a new evaluator with built-in functions bound in the global environment.
    pub fn new(lisp: &'a Lisp<N>) -> Self {
        let env = ArenaIndex::NIL;
        let mut eval = Evaluator {
            lisp,
            global_env: env,
        };
        eval.init_builtins();
        eval
    }

    /// Force a value to Weak Head Normal Form (WHNF), memoizing the result.
    pub fn force(&mut self, mut idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        loop {
            match self.lisp.get(idx)? {
                // WHNF — already evaluated, return immediately
                Value::Nil | Value::True | Value::False | Value::Number(_)
                | Value::Char(_) | Value::String { .. }
                | Value::Cons { .. } | Value::Lambda { .. }
                | Value::Builtin(_) | Value::Symbol(_) => {
                    return Ok(idx);
                }

                // Indirection — follow the pointer.
                Value::Indirection(target) => {
                    idx = target;
                    continue;
                }

                // Thunk — force it via the black-hole protocol.
                Value::Thunk { expr, env } => {
                    let thunk_idx = idx;
                    // Step 1: Black-hole the cell (cycle detection).
                    self.lisp.arena.set(thunk_idx, Value::BlackHole)?;

                    // Step 2: Evaluate the expression to WHNF.
                    let result = self.eval(expr, env)?;

                    // Step 3: Follow indirections in result to detect cycles.
                    let mut final_result = result;
                    loop {
                        match self.lisp.get(final_result)? {
                            Value::Indirection(target) => {
                                final_result = target;
                            }
                            Value::BlackHole => {
                                // The result transitively points to a
                                // black-holed thunk — circular evaluation.
                                return Err(ArenaError::InvalidIndex);
                            }
                            _ => break,
                        }
                    }

                    // Step 4: Memoize — overwrite the cell with an indirection.
                    self.lisp.arena.set(thunk_idx, Value::Indirection(final_result))?;

                    // Continue the loop to return the final WHNF value.
                    idx = final_result;
                    continue;
                }

                // Circular dependency detected.
                Value::BlackHole => {
                    return Err(ArenaError::InvalidIndex);
                }
            }
        }
    }

    /// Evaluate an expression in an environment (with TCO).
    pub fn eval(&mut self, mut expr: ArenaIndex, mut env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        loop {
            let val = self.lisp.get(expr)?;

            match val {
                // Self-evaluating values
                Value::Nil | Value::True | Value::False | Value::Number(_)
                | Value::Char(_) | Value::String { .. }
                | Value::Builtin(_) | Value::Lambda { .. } => {
                    return Ok(expr);
                }

                // Thunk encountered as a bare expression — force it.
                Value::Thunk { .. } => {
                    return self.force(expr);
                }

                // Indirection — follow it (part of the eval loop, no stack growth)
                Value::Indirection(target) => {
                    expr = target;
                    continue;
                }

                // Black hole — circular evaluation
                Value::BlackHole => {
                    return Err(ArenaError::InvalidIndex);
                }

                // Symbol → look up in local env, then global env
                Value::Symbol(_) => {
                    let binding = if let Ok(b) = env_lookup(self.lisp, env, expr) {
                        b
                    } else {
                        env_lookup(self.lisp, self.global_env, expr)?
                    };
                    // Check for BlackHole (circular evaluation).
                    // Other value types (Thunk, Indirection, WHNF) are
                    // returned as-is — the caller decides when to force.
                    if matches!(self.lisp.get(binding)?, Value::BlackHole) {
                        return Err(ArenaError::InvalidIndex);
                    }
                    return Ok(binding);
                }

                // List → special form or function application
                Value::Cons { car, cdr } => {
                    // Check for special forms
                    if matches!(self.lisp.get(car)?, Value::Symbol(_)) {
                        if let Some(action) = self.try_special_form_tco(car, cdr, &mut expr, &mut env) {
                            match action {
                                TailAction::Return(val) => return val,
                                TailAction::Continue => continue,
                            }
                        }
                    }

                    // Function application (call-by-need):
                    // Step 1: Evaluate the operator to WHNF.
                    let func_idx = self.eval(car, env)?;
                    let func_whnf = self.force(func_idx)?;

                    match self.lisp.get(func_whnf)? {
                        // Builtin — strict in all arguments.
                        Value::Builtin(id) => {
                            let args = self.force_args(cdr, env)?;
                            return self.apply_builtin(id, args);
                        }

                        // Lambda — lazy in arguments (call-by-need).
                        Value::Lambda { .. } => {
                            let (params, body, closed_env) = self.lisp.lambda_parts(func_whnf)?;
                            env = self.bind_args_lazy(closed_env, params, cdr, env)?;
                            expr = body;
                            continue; // ← TCO: no Rust stack frame
                        }

                        _ => return Err(ArenaError::InvalidIndex),
                    }
                }
            }
        }
    }

    /// Bind parameters to argument thunks (call-by-need).
    fn bind_args_lazy(
        &mut self,
        mut fn_env: ArenaIndex,
        mut params: ArenaIndex,
        mut arg_exprs: ArenaIndex,
        call_env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        while !params.is_nil() && !arg_exprs.is_nil() {
            match self.lisp.get(params)? {
                Value::Cons { car: param, cdr: rest } => {
                    let arg_expr = self.lisp.car(arg_exprs)?;

                    // Allocate a thunk in the arena.
                    let thunk = self.lisp.arena.alloc(Value::Thunk {
                        expr: arg_expr,
                        env: call_env,
                    })?;

                    fn_env = env_bind(self.lisp, fn_env, param, thunk)?;

                    params = rest;
                    arg_exprs = self.lisp.cdr(arg_exprs)?;
                }
                Value::Symbol(_) => {
                    // Rest parameter: bind remaining args as a thunk-wrapped list
                    // We need to evaluate the rest args lazily.
                    // Build a list of thunks for the remaining args.
                    let rest_thunks = self.make_thunk_list(arg_exprs, call_env)?;
                    fn_env = env_bind(self.lisp, fn_env, params, rest_thunks)?;
                    return Ok(fn_env);
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
        }
        Ok(fn_env)
    }

    /// Build a list of thunks from a list of expressions.
    fn make_thunk_list(
        &self,
        mut exprs: ArenaIndex,
        call_env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        if exprs.is_nil() {
            return self.lisp.nil();
        }
        let arg_expr = self.lisp.car(exprs)?;
        let thunk = self.lisp.arena.alloc(Value::Thunk {
            expr: arg_expr,
            env: call_env,
        })?;
        exprs = self.lisp.cdr(exprs)?;
        let rest = self.make_thunk_list(exprs, call_env)?;
        self.lisp.cons(thunk, rest)
    }

    /// Evaluate and force all arguments in a list (for strict builtins).
    fn force_args(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        if args.is_nil() {
            return self.lisp.nil();
        }
        let head_expr = self.lisp.car(args)?;
        let head_val = self.eval(head_expr, env)?;
        let head_forced = self.force(head_val)?;

        let tail = self.lisp.cdr(args)?;
        let tail_forced = self.force_args(tail, env)?;

        self.lisp.cons(head_forced, tail_forced)
    }

    // ========================================================================
    // Special forms (TCO-aware)
    // ========================================================================

    /// `(quote expr)` — return the expression unevaluated.
    fn eval_quote(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        _env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::Return(self.lisp.car(args))
    }

    /// `(if test then else)` — test is strict, branches are tail positions.
    fn eval_if(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        let result = (|| -> ArenaResult<()> {
            let test_expr = self.lisp.car(args)?;
            let rest = self.lisp.cdr(args)?;

            // Non-tail: evaluate and force the test
            let test_val = self.eval(test_expr, *env)?;
            let test_forced = self.force(test_val)?;
            let is_false = matches!(self.lisp.get(test_forced)?, Value::False);

            if !is_false {
                *expr = self.lisp.car(rest)?;
            } else {
                let else_rest = self.lisp.cdr(rest)?;
                *expr = if else_rest.is_nil() {
                    self.lisp.nil()?
                } else {
                    self.lisp.car(else_rest)?
                };
            }
            Ok(())
        })();

        match result {
            Ok(()) => TailAction::Continue,
            Err(e) => TailAction::Return(Err(e)),
        }
    }

    /// `(define name expr)` or `(define (name params...) body)`.
    fn eval_define(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::Return(self.eval_define_inner(args, *env))
    }

    fn eval_define_inner(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let first = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;

        match self.lisp.get(first)? {
            Value::Symbol(_) => {
                let val_expr = self.lisp.car(rest)?;
                // Create thunk for the RHS
                let thunk = self.lisp.arena.alloc(Value::Thunk {
                    expr: val_expr,
                    env,
                })?;
                self.global_env = env_bind(self.lisp, self.global_env, first, thunk)?;
                Ok(thunk)
            }
            Value::Cons { car: name, cdr: params } => {
                // Function shorthand — lambda is already WHNF, no thunk needed
                let body = self.wrap_begin(rest)?;
                let lam = self.lisp.lambda(params, body, env)?;
                self.global_env = env_bind(self.lisp, self.global_env, name, lam)?;
                Ok(lam)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    /// `(set! name expr)`.
    fn eval_set(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::Return(self.eval_set_inner(args, *env))
    }

    fn eval_set_inner(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let name = self.lisp.car(args)?;
        let expr = self.lisp.car(self.lisp.cdr(args)?)?;
        let val = self.eval(expr, env)?;
        let forced = self.force(val)?;

        // Try local env first, then global
        if env_set(self.lisp, env, name, forced).is_ok() {
            return self.lisp.nil();
        }
        env_set(self.lisp, self.global_env, name, forced)?;
        self.lisp.nil()
    }

    /// `(lambda (params...) body...)`.
    fn eval_lambda(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::Return(self.eval_lambda_inner(args, *env))
    }

    fn eval_lambda_inner(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let params = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;
        let body = self.wrap_begin(body_list)?;
        self.lisp.lambda(params, body, env)
    }

    /// `(begin expr1 expr2 ...)` — all but last are non-tail, last is tail.
    fn eval_begin(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        let result = (|| -> ArenaResult<()> {
            let mut cur = args;
            while !cur.is_nil() {
                let next = self.lisp.cdr(cur)?;
                if next.is_nil() {
                    // Last expression — tail position
                    *expr = self.lisp.car(cur)?;
                    return Ok(());
                }
                // Non-last — evaluate (non-tail) and discard result
                let e = self.lisp.car(cur)?;
                self.eval(e, *env)?;
                cur = next;
            }
            *expr = self.lisp.nil()?;
            Ok(())
        })();

        match result {
            Ok(()) => TailAction::Continue,
            Err(e) => TailAction::Return(Err(e)),
        }
    }

    /// `(cond (test expr) ...)` — tests are strict, result is tail.
    fn eval_cond(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::Return(self.eval_cond_inner(args, expr, env))
    }

    fn eval_cond_inner(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let mut cur = args;
        while !cur.is_nil() {
            let clause = self.lisp.car(cur)?;
            let test = self.lisp.car(clause)?;
            let body = self.lisp.cdr(clause)?;

            // Check for `else` clause
            if self.lisp.symbol_name_eq(test, "else") {
                return self.eval_begin_inner(body, *env);
            }

            let test_val = self.eval(test, *env)?;
            let test_forced = self.force(test_val)?;
            if !matches!(self.lisp.get(test_forced)?, Value::False) {
                return self.eval_begin_inner(body, *env);
            }
            cur = self.lisp.cdr(cur)?;
        }
        self.lisp.nil()
    }

    /// Helper: evaluate a begin body without TCO (for use within cond).
    fn eval_begin_inner(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let mut cur = args;
        let mut result = self.lisp.nil()?;
        while !cur.is_nil() {
            let e = self.lisp.car(cur)?;
            result = self.eval(e, env)?;
            cur = self.lisp.cdr(cur)?;
        }
        Ok(result)
    }

    /// `(and expr1 expr2 ...)` — strict on tests, last is tail.
    fn eval_and(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        let result = (|| -> ArenaResult<()> {
            let mut cur = args;
            while !cur.is_nil() {
                let next = self.lisp.cdr(cur)?;
                if next.is_nil() {
                    // Last expression — tail position
                    *expr = self.lisp.car(cur)?;
                    return Ok(());
                }
                let e = self.lisp.car(cur)?;
                let result = self.eval(e, *env)?;
                let forced = self.force(result)?;
                if matches!(self.lisp.get(forced)?, Value::False) {
                    // Short-circuit: need to return false directly
                    // We set expr to the false value (self-evaluating)
                    *expr = forced;
                    return Ok(());
                }
                cur = next;
            }
            // (and) with no args → #t
            *expr = self.lisp.boolean(true)?;
            Ok(())
        })();

        match result {
            Ok(()) => TailAction::Continue,
            Err(e) => TailAction::Return(Err(e)),
        }
    }

    /// `(or expr1 expr2 ...)` — strict on tests, last is tail.
    fn eval_or(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        let result = (|| -> ArenaResult<()> {
            let mut cur = args;
            while !cur.is_nil() {
                let next = self.lisp.cdr(cur)?;
                if next.is_nil() {
                    // Last expression — tail position
                    *expr = self.lisp.car(cur)?;
                    return Ok(());
                }
                let e = self.lisp.car(cur)?;
                let result = self.eval(e, *env)?;
                let forced = self.force(result)?;
                if !matches!(self.lisp.get(forced)?, Value::False) {
                    // Short-circuit: return the truthy value
                    *expr = forced;
                    return Ok(());
                }
                cur = next;
            }
            // (or) with no args → #f
            *expr = self.lisp.boolean(false)?;
            Ok(())
        })();

        match result {
            Ok(()) => TailAction::Continue,
            Err(e) => TailAction::Return(Err(e)),
        }
    }

    /// `(let ((name val) ...) body...)` — bindings are lazy, body is tail.
    fn eval_let(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        let result = (|| -> ArenaResult<()> {
            let bindings = self.lisp.car(args)?;
            let body_list = self.lisp.cdr(args)?;

            let mut local_env = *env;
            let mut cur = bindings;
            while !cur.is_nil() {
                let binding = self.lisp.car(cur)?;
                let name = self.lisp.car(binding)?;
                let val_expr = self.lisp.car(self.lisp.cdr(binding)?)?;

                // LAZY: wrap in a thunk instead of evaluating
                let thunk = self.lisp.arena.alloc(Value::Thunk {
                    expr: val_expr,
                    env: *env,
                })?;
                local_env = env_bind(self.lisp, local_env, name, thunk)?;

                cur = self.lisp.cdr(cur)?;
            }

            *env = local_env;
            let body = self.wrap_begin(body_list)?;
            *expr = body;
            Ok(())
        })();

        match result {
            Ok(()) => TailAction::Continue,
            Err(e) => TailAction::Return(Err(e)),
        }
    }

    /// `(cons a b)` — lazy cons: does NOT evaluate arguments.
    fn eval_cons(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::Return(self.eval_cons_inner(args, *env))
    }

    fn eval_cons_inner(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let a_expr = self.lisp.car(args)?;
        let b_expr = self.lisp.car(self.lisp.cdr(args)?)?;

        // Create thunks for both arguments
        let a_thunk = self.lisp.arena.alloc(Value::Thunk {
            expr: a_expr,
            env,
        })?;
        let b_thunk = self.lisp.arena.alloc(Value::Thunk {
            expr: b_expr,
            env,
        })?;

        self.lisp.cons(a_thunk, b_thunk)
    }

    /// Wrap a list of expressions in a `begin` form if there are multiple,
    /// or return the single expression if there's only one.
    fn wrap_begin(&self, exprs: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if exprs.is_nil() {
            return self.lisp.nil();
        }
        let rest = self.lisp.cdr(exprs)?;
        if rest.is_nil() {
            // Single expression, no need to wrap
            return self.lisp.car(exprs);
        }
        // Multiple expressions, wrap in begin
        let begin_sym = self.lisp.symbol("begin")?;
        self.lisp.cons(begin_sym, exprs)
    }

    // ========================================================================
    // Arithmetic built-ins
    // ========================================================================

    /// `(+ ...)` — variadic addition.
    fn builtin_add(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let mut sum: isize = 0;
        let mut cur = args;
        while !cur.is_nil() {
            let val = self.lisp.car(cur)?;
            match self.lisp.get(val)? {
                Value::Number(n) => {
                    sum = sum.checked_add(n).ok_or(ArenaError::InvalidIndex)?;
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
            cur = self.lisp.cdr(cur)?;
        }
        self.lisp.number(sum)
    }

    /// `(- a b ...)` — subtraction. With one arg, negates.
    fn builtin_sub(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if args.is_nil() {
            return Err(ArenaError::InvalidIndex);
        }
        let first_idx = self.lisp.car(args)?;
        let first = match self.lisp.get(first_idx)? {
            Value::Number(n) => n,
            _ => return Err(ArenaError::InvalidIndex),
        };
        let rest = self.lisp.cdr(args)?;
        if rest.is_nil() {
            // Unary minus
            return self.lisp.number(-first);
        }
        let mut result = first;
        let mut cur = rest;
        while !cur.is_nil() {
            let val = self.lisp.car(cur)?;
            match self.lisp.get(val)? {
                Value::Number(n) => {
                    result = result.checked_sub(n).ok_or(ArenaError::InvalidIndex)?;
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
            cur = self.lisp.cdr(cur)?;
        }
        self.lisp.number(result)
    }

    /// `(* ...)` — variadic multiplication.
    fn builtin_mul(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let mut product: isize = 1;
        let mut cur = args;
        while !cur.is_nil() {
            let val = self.lisp.car(cur)?;
            match self.lisp.get(val)? {
                Value::Number(n) => {
                    product = product.checked_mul(n).ok_or(ArenaError::InvalidIndex)?;
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
            cur = self.lisp.cdr(cur)?;
        }
        self.lisp.number(product)
    }

    /// `(/ a b)` — integer division.
    fn builtin_div(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let a_idx = self.lisp.car(args)?;
        let b_idx = self.lisp.car(self.lisp.cdr(args)?)?;
        let a = match self.lisp.get(a_idx)? {
            Value::Number(n) => n,
            _ => return Err(ArenaError::InvalidIndex),
        };
        let b = match self.lisp.get(b_idx)? {
            Value::Number(n) => n,
            _ => return Err(ArenaError::InvalidIndex),
        };
        if b == 0 {
            return Err(ArenaError::InvalidIndex);
        }
        self.lisp.number(a / b)
    }

    /// `(= a b)` — numeric equality.
    fn builtin_eq(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let a_idx = self.lisp.car(args)?;
        let b_idx = self.lisp.car(self.lisp.cdr(args)?)?;
        let a = match self.lisp.get(a_idx)? {
            Value::Number(n) => n,
            _ => return Err(ArenaError::InvalidIndex),
        };
        let b = match self.lisp.get(b_idx)? {
            Value::Number(n) => n,
            _ => return Err(ArenaError::InvalidIndex),
        };
        self.lisp.boolean(a == b)
    }

    /// Numeric comparison with a given comparator.
    fn builtin_cmp(
        &self,
        args: ArenaIndex,
        cmp: fn(isize, isize) -> bool,
    ) -> ArenaResult<ArenaIndex> {
        let a_idx = self.lisp.car(args)?;
        let b_idx = self.lisp.car(self.lisp.cdr(args)?)?;
        let a = match self.lisp.get(a_idx)? {
            Value::Number(n) => n,
            _ => return Err(ArenaError::InvalidIndex),
        };
        let b = match self.lisp.get(b_idx)? {
            Value::Number(n) => n,
            _ => return Err(ArenaError::InvalidIndex),
        };
        self.lisp.boolean(cmp(a, b))
    }

    /// `(< a b)` — less than.
    fn builtin_lt(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.builtin_cmp(args, |a, b| a < b)
    }

    /// `(> a b)` — greater than.
    fn builtin_gt(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.builtin_cmp(args, |a, b| a > b)
    }

    /// `(<= a b)` — less than or equal.
    fn builtin_le(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.builtin_cmp(args, |a, b| a <= b)
    }

    /// `(>= a b)` — greater than or equal.
    fn builtin_ge(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.builtin_cmp(args, |a, b| a >= b)
    }

    // ========================================================================
    // Pair / list built-ins
    // ========================================================================

    /// `(list ...)` — return args as-is (already forced into a list).
    fn builtin_list(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        Ok(args)
    }

    fn builtin_car(&mut self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let pair = self.lisp.car(args)?;
        let car = self.lisp.car(pair)?;
        // Force through thunks/indirections
        self.force(car)
    }

    fn builtin_cdr(&mut self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let pair = self.lisp.car(args)?;
        let cdr = self.lisp.cdr(pair)?;
        // Force through thunks/indirections
        self.force(cdr)
    }

    // ========================================================================
    // Type predicate built-ins
    // ========================================================================

    fn builtin_nullp(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.lisp.car(args)?;
        self.lisp.boolean(matches!(self.lisp.get(val)?, Value::Nil))
    }

    fn builtin_not(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.lisp.car(args)?;
        self.lisp.boolean(matches!(self.lisp.get(val)?, Value::False))
    }

    fn builtin_pairp(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.lisp.car(args)?;
        self.lisp
            .boolean(matches!(self.lisp.get(val)?, Value::Cons { .. }))
    }

    fn builtin_numberp(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.lisp.car(args)?;
        self.lisp
            .boolean(matches!(self.lisp.get(val)?, Value::Number(_)))
    }

    fn builtin_symbolp(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.lisp.car(args)?;
        self.lisp
            .boolean(matches!(self.lisp.get(val)?, Value::Symbol(_)))
    }

    fn builtin_booleanp(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.lisp.car(args)?;
        self.lisp
            .boolean(matches!(self.lisp.get(val)?, Value::True | Value::False))
    }
}
