//! Strict Lisp evaluator with first-class operatives (fexprs).
//!
//! Evaluates arena-allocated S-expressions in an environment using
//! call-by-value evaluation with tail-call optimization.
//!
//! All special forms and built-in functions are first-class operatives
//! bound in the global environment, following Shutt's vau calculus.
//! The evaluator's core application logic is a single path: look up
//! the operative, call it with unevaluated args and the current
//! environment, let the operative decide what to evaluate.

use grift_arena::{ArenaError, ArenaIndex, ArenaResult};

use crate::lisp::Lisp;
use crate::value::{BuiltinId, Value};

/// Convert a fallible closure into a `TailAction`: `Ok(())` → `Continue`,
/// `Err(e)` → `Return(Err(e))`.  Eliminates the repeated match boilerplate
/// in every TCO-aware operative.
macro_rules! tail_continue {
    ($body:expr) => {
        match $body {
            Ok(()) => TailAction::Continue,
            Err(e) => TailAction::Return(Err(e)),
        }
    };
}

/// Generate a type-predicate builtin method that checks the first arg
/// against a pattern.  All six predicates (`null?`, `pair?`, `number?`, …)
/// share the exact same shape; this macro captures it once.
macro_rules! type_predicate {
    ($name:ident, $pat:pat) => {
        fn $name(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
            let val = self.lisp.car(args)?;
            self.lisp.boolean(matches!(self.lisp.get(val)?, $pat))
        }
    };
}

/// Fold a variadic argument list over a checked arithmetic operation,
/// starting from `$init`.  Used by `(+ ...)` and `(* ...)`.
macro_rules! fold_numbers {
    ($self:ident, $args:ident, $init:expr, $op:ident) => {{
        let mut acc: isize = $init;
        let mut cur = $args;
        while !cur.is_nil() {
            let n = $self.lisp.get($self.lisp.car(cur)?)?.as_number()?;
            acc = acc.$op(n).ok_or(ArenaError::ArithmeticOverflow)?;
            cur = $self.lisp.cdr(cur)?;
        }
        $self.lisp.number(acc)
    }};
}

/// Generate a numeric comparison builtin method.
macro_rules! cmp_builtin {
    ($name:ident, $op:tt) => {
        fn $name(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
            let (a, b) = binary_nums!(self, args);
            self.lisp.boolean(a $op b)
        }
    };
}
macro_rules! binary_nums {
    ($self:ident, $args:ident) => {{
        let a = $self.lisp.get($self.lisp.car($args)?)?.as_number()?;
        let b = $self
            .lisp
            .get($self.lisp.car($self.lisp.cdr($args)?)?)?
            .as_number()?;
        (a, b)
    }};
}

/// Generate a pair-accessor builtin (`car` or `cdr`) that extracts a
/// component from the first argument.
macro_rules! pair_builtin {
    ($name:ident, $accessor:ident) => {
        fn $name(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
            let pair = self.lisp.car(args)?;
            self.lisp.$accessor(pair)
        }
    };
}

/// Non-tail operative: wrap a `(args, env) -> Result` as `TailAction::Return`.
#[inline]
fn non_tail(result: ArenaResult<ArenaIndex>) -> TailAction {
    TailAction::Return(result)
}

/// Register all operatives (builtins and special forms) in the global
/// environment as `Value::Builtin(id)` with auto-assigned sequential IDs.
///
/// Every operative receives its arguments *unevaluated* along with the
/// caller's environment. Applicative operatives (like `+`, `car`) evaluate
/// their arguments internally; special-form operatives (like `if`, `define`)
/// work directly with unevaluated forms.
macro_rules! define_operatives {
    ( $( $name:literal => $method:ident ),* $(,)? ) => {
        // Generate unique constant IDs for each operative.
        define_operatives!(@consts 0u8; $( $method, )*);

        impl<'a, const N: usize> Evaluator<'a, N> {
            /// Register all operatives in the global environment.
            fn init_builtins(&mut self) {
                $(
                    if let (Ok(sym), Ok(val)) = (
                        self.lisp.symbol($name),
                        self.lisp.arena.alloc(Value::Builtin($method)),
                    ) {
                        if let Ok(new_env) = env_bind(self.lisp, self.global_env, sym, val) {
                            self.global_env = new_env;
                        }
                    }
                )*
            }

            /// Apply a built-in operative (TCO-aware).
            ///
            /// The operative receives unevaluated arguments and the caller's
            /// environment. It may evaluate arguments as needed and can return
            /// `TailAction::Continue` for tail-call optimization.
            #[allow(non_upper_case_globals)]
            fn apply_operative(
                &mut self,
                id: BuiltinId,
                args: ArenaIndex,
                expr: &mut ArenaIndex,
                env: &mut ArenaIndex,
            ) -> TailAction {
                match id {
                    $( $method => self.$method(args, expr, env), )*
                    _ => TailAction::Return(Err(ArenaError::NotCallable)),
                }
            }
        }
    };

    // Generate const declarations: base case.
    (@consts $id:expr; ) => {};
    // Generate const declarations: recursive case.
    (@consts $id:expr; $method:ident, $( $rest:ident, )*) => {
        #[allow(non_upper_case_globals)]
        const $method: BuiltinId = BuiltinId($id);
        define_operatives!(@consts $id + 1u8; $( $rest, )*);
    };
}

// Register all operatives — both applicative builtins and special forms
// are unified as first-class operatives in the global environment.
define_operatives! {
    // Special forms (receive unevaluated args)
    "quote"    => op_quote,
    "if"       => op_if,
    "define"   => op_define,
    "lambda"   => op_lambda,
    "begin"    => op_begin,
    "cond"     => op_cond,
    "and"      => op_and,
    "or"       => op_or,
    "let"      => op_let,
    "vau"      => op_vau,
    "eval"     => op_eval,
    // Applicative builtins (evaluate args before applying)
    "cons"     => op_cons,
    "+"        => op_add,
    "-"        => op_sub,
    "*"        => op_mul,
    "/"        => op_div,
    "="        => op_eq,
    "<"        => op_lt,
    ">"        => op_gt,
    "<="       => op_le,
    ">="       => op_ge,
    "car"      => op_car,
    "cdr"      => op_cdr,
    "list"     => op_list,
    "null?"    => op_nullp,
    "not"      => op_not,
    "pair?"    => op_pairp,
    "number?"  => op_numberp,
    "symbol?"  => op_symbolp,
    "boolean?" => op_booleanp,
}

/// TCO control flow for operatives.
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
    /// Shadow stack of GC roots stored as a linked list of cons cells in
    /// the arena.  Each entry is `(root_value . rest)`, with `ArenaIndex::NIL`
    /// as the empty list.
    gc_roots: ArenaIndex,
}

/// Look up a name in an environment association list.
/// Returns `Err(UnboundVariable)` when the name is not found.
#[inline]
fn env_lookup<const N: usize>(
    lisp: &Lisp<N>,
    env: ArenaIndex,
    name: ArenaIndex,
) -> ArenaResult<ArenaIndex> {
    let mut cur = env;
    while !cur.is_nil() {
        let binding = lisp.car(cur)?;
        if lisp.car(binding)? == name {
            return lisp.cdr(binding);
        }
        cur = lisp.cdr(cur)?;
    }
    Err(ArenaError::UnboundVariable)
}

/// Bind a name to a value in an environment, returning the new environment.
#[inline]
fn env_bind<const N: usize>(
    lisp: &Lisp<N>,
    env: ArenaIndex,
    name: ArenaIndex,
    val: ArenaIndex,
) -> ArenaResult<ArenaIndex> {
    let pair = lisp.cons(name, val)?;
    lisp.cons(pair, env)
}

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Create a new evaluator with operatives bound in the global environment.
    pub fn new(lisp: &'a Lisp<N>) -> Self {
        let mut eval = Evaluator {
            lisp,
            global_env: ArenaIndex::NIL,
            gc_roots: ArenaIndex::NIL,
        };
        eval.init_builtins();
        eval
    }

    /// Push a value onto the GC root stack so it survives collection.
    #[inline]
    fn push_root(&mut self, idx: ArenaIndex) {
        if let Ok(new_roots) = self.lisp.cons(idx, self.gc_roots) {
            self.gc_roots = new_roots;
        } else {
            debug_assert!(false, "GC root push failed: arena out of memory");
        }
    }

    /// Pop `n` values from the GC root stack.
    #[inline]
    fn pop_roots(&mut self, n: usize) {
        for _ in 0..n {
            debug_assert!(!self.gc_roots.is_nil(), "GC root stack underflow");
            match self.lisp.cdr(self.gc_roots) {
                Ok(rest) => self.gc_roots = rest,
                Err(_) => break,
            }
        }
    }

    /// Trigger garbage collection using all known live roots.
    fn collect_garbage(&self, expr: ArenaIndex, env: ArenaIndex) {
        self.lisp
            .arena
            .collect_garbage(&[expr, env, self.global_env, self.gc_roots]);
    }

    /// Check arena memory pressure and collect garbage if needed.
    #[inline]
    fn maybe_collect(&self, expr: ArenaIndex, env: ArenaIndex) {
        let len = self.lisp.arena.len();
        let cap = self.lisp.arena.capacity();
        if len > cap * 3 / 4 {
            self.collect_garbage(expr, env);
        }
    }

    /// Evaluate an expression in an environment (with TCO).
    ///
    /// The core application logic is a single path: look up the operative,
    /// call it with unevaluated args and the current environment, let the
    /// operative decide what to evaluate.
    pub fn eval(&mut self, mut expr: ArenaIndex, mut env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        loop {
            self.maybe_collect(expr, env);

            let val = self.lisp.get(expr)?;

            // Self-evaluating: literals, closures, operatives, builtins.
            if val.is_self_evaluating() {
                return Ok(expr);
            }

            match val {
                // Symbol → look up in local env, then global env.
                Value::Symbol(_) => {
                    return env_lookup(self.lisp, env, expr)
                        .or_else(|_| env_lookup(self.lisp, self.global_env, expr));
                }

                // List → operative/function application.
                Value::Cons { car, cdr } => {
                    self.push_root(cdr);
                    self.push_root(env);

                    // Evaluate the operator position.
                    let func_val = self.eval(car, env)?;

                    match self.lisp.get(func_val)? {
                        // Built-in operative: pass unevaluated args + env.
                        Value::Builtin(id) => {
                            let action = self.apply_operative(id, cdr, &mut expr, &mut env);
                            self.pop_roots(2);
                            match action {
                                TailAction::Return(val) => return val,
                                TailAction::Continue => continue,
                            }
                        }
                        // Lambda (applicative): evaluate args, then apply.
                        Value::Lambda { .. } => {
                            let (params, body, closed_env) =
                                self.lisp.lambda_parts(func_val)?;
                            env = self.bind_args(closed_env, params, cdr, env)?;
                            expr = body;
                            self.pop_roots(2);
                            continue; // ← TCO
                        }
                        // Vau (operative / fexpr): bind unevaluated args + caller env.
                        Value::Vau { .. } => {
                            let (params, env_param, body, closed_env) =
                                self.lisp.vau_parts(func_val)?;
                            // Bind the unevaluated argument list to params.
                            let mut vau_env = self.bind_vau_params(closed_env, params, cdr)?;
                            // Bind the caller's environment to env_param (unless #ignore).
                            if !env_param.is_nil() {
                                vau_env = env_bind(self.lisp, vau_env, env_param, env)?;
                            }
                            env = vau_env;
                            expr = body;
                            self.pop_roots(2);
                            continue; // ← TCO
                        }
                        _ => {
                            self.pop_roots(2);
                            return Err(ArenaError::NotCallable);
                        }
                    }
                }

                _ => unreachable!(),
            }
        }
    }

    /// Bind vau parameters to unevaluated argument expressions.
    ///
    /// Unlike `bind_args` (which evaluates each argument), this binds the
    /// raw argument expressions directly. A symbol parameter binds all
    /// remaining args as a list (rest parameter).
    fn bind_vau_params(
        &self,
        mut vau_env: ArenaIndex,
        mut params: ArenaIndex,
        mut arg_exprs: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        while !params.is_nil() && !arg_exprs.is_nil() {
            match self.lisp.get(params)? {
                Value::Cons {
                    car: param,
                    cdr: rest,
                } => {
                    let arg_expr = self.lisp.car(arg_exprs)?;
                    vau_env = env_bind(self.lisp, vau_env, param, arg_expr)?;
                    params = rest;
                    arg_exprs = self.lisp.cdr(arg_exprs)?;
                }
                Value::Symbol(_) => {
                    // Rest parameter: bind remaining unevaluated args as a list.
                    vau_env = env_bind(self.lisp, vau_env, params, arg_exprs)?;
                    return Ok(vau_env);
                }
                _ => return Err(ArenaError::TypeError),
            }
        }
        Ok(vau_env)
    }

    /// Bind parameters to evaluated argument values (call-by-value).
    fn bind_args(
        &mut self,
        mut fn_env: ArenaIndex,
        mut params: ArenaIndex,
        mut arg_exprs: ArenaIndex,
        call_env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        while !params.is_nil() && !arg_exprs.is_nil() {
            match self.lisp.get(params)? {
                Value::Cons {
                    car: param,
                    cdr: rest,
                } => {
                    let arg_expr = self.lisp.car(arg_exprs)?;
                    let arg_val = self.eval(arg_expr, call_env)?;
                    fn_env = env_bind(self.lisp, fn_env, param, arg_val)?;

                    params = rest;
                    arg_exprs = self.lisp.cdr(arg_exprs)?;
                }
                Value::Symbol(_) => {
                    // Rest parameter: evaluate and bind remaining args as a list.
                    let rest_vals = self.eval_args(arg_exprs, call_env)?;
                    fn_env = env_bind(self.lisp, fn_env, params, rest_vals)?;
                    return Ok(fn_env);
                }
                _ => return Err(ArenaError::TypeError),
            }
        }
        Ok(fn_env)
    }

    /// Evaluate all arguments in a list (for applicative builtins).
    fn eval_args(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if args.is_nil() {
            return self.lisp.nil();
        }
        self.push_root(args);
        self.push_root(env);

        let head_expr = self.lisp.car(args)?;
        let head_val = self.eval(head_expr, env)?;

        self.push_root(head_val);

        let tail = self.lisp.cdr(args)?;
        let tail_vals = self.eval_args(tail, env)?;

        self.pop_roots(3);

        self.lisp.cons(head_val, tail_vals)
    }

    // ================================================================
    // Special-form operatives (receive unevaluated args)
    // ================================================================

    /// `(quote expr)` — return the expression unevaluated.
    fn op_quote(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        _env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::Return(self.lisp.car(args))
    }

    /// `(if test then else)` — test is strict, branches are tail positions.
    fn op_if(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        tail_continue!((|| -> ArenaResult<()> {
            let test_expr = self.lisp.car(args)?;
            let rest = self.lisp.cdr(args)?;

            let test_val = self.eval(test_expr, *env)?;

            if self.lisp.get(test_val)?.is_truthy() {
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
        })())
    }

    /// `(define name expr)` or `(define (name params...) body)`.
    fn op_define(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail((|| {
            let first = self.lisp.car(args)?;
            let rest = self.lisp.cdr(args)?;

            match self.lisp.get(first)? {
                Value::Symbol(_) => {
                    let val_expr = self.lisp.car(rest)?;
                    let val = self.eval(val_expr, *env)?;
                    self.global_env = env_bind(self.lisp, self.global_env, first, val)?;
                    Ok(val)
                }
                Value::Cons {
                    car: name,
                    cdr: params,
                } => {
                    let body = self.wrap_begin(rest)?;
                    let lam = self.lisp.lambda(params, body, *env)?;
                    self.global_env = env_bind(self.lisp, self.global_env, name, lam)?;
                    Ok(lam)
                }
                _ => Err(ArenaError::TypeError),
            }
        })())
    }

    /// `(lambda (params...) body...)`.
    fn op_lambda(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail((|| {
            let params = self.lisp.car(args)?;
            let body = self.wrap_begin(self.lisp.cdr(args)?)?;
            self.lisp.lambda(params, body, *env)
        })())
    }

    /// `(begin expr1 expr2 ...)` — all but last are non-tail, last is tail.
    fn op_begin(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        tail_continue!((|| -> ArenaResult<()> {
            let mut cur = args;
            while !cur.is_nil() {
                let next = self.lisp.cdr(cur)?;
                if next.is_nil() {
                    *expr = self.lisp.car(cur)?;
                    return Ok(());
                }
                let e = self.lisp.car(cur)?;
                self.eval(e, *env)?;
                cur = next;
            }
            *expr = self.lisp.nil()?;
            Ok(())
        })())
    }

    /// `(cond (test expr...) ...)` — tests are strict, last body expr is tail.
    fn op_cond(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        tail_continue!((|| -> ArenaResult<()> {
            let mut cur = args;
            while !cur.is_nil() {
                let clause = self.lisp.car(cur)?;
                let test = self.lisp.car(clause)?;
                let body = self.lisp.cdr(clause)?;

                let matched = self.lisp.symbol_name_eq(test, "else") || {
                    let test_val = self.eval(test, *env)?;
                    self.lisp.get(test_val)?.is_truthy()
                };

                if matched {
                    *expr = self.wrap_begin(body)?;
                    return Ok(());
                }
                cur = self.lisp.cdr(cur)?;
            }
            *expr = self.lisp.nil()?;
            Ok(())
        })())
    }

    /// `(and expr1 expr2 ...)` — strict on tests, last is tail.
    fn op_and(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.eval_short_circuit(args, expr, env, true)
    }

    /// `(or expr1 expr2 ...)` — strict on tests, last is tail.
    fn op_or(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.eval_short_circuit(args, expr, env, false)
    }

    /// Shared `and`/`or` implementation.
    fn eval_short_circuit(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
        continue_while_truthy: bool,
    ) -> TailAction {
        tail_continue!((|| -> ArenaResult<()> {
            let mut cur = args;
            while !cur.is_nil() {
                let next = self.lisp.cdr(cur)?;
                let e = self.lisp.car(cur)?;
                let val = self.eval(e, *env)?;
                let b = self.lisp.get(val)?.as_bool()?;
                if b != continue_while_truthy {
                    *expr = val;
                    return Ok(());
                }
                cur = next;
            }
            *expr = self.lisp.boolean(continue_while_truthy)?;
            Ok(())
        })())
    }

    /// `(let ((name val) ...) body...)` — bindings are strict, body is tail.
    fn op_let(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        tail_continue!((|| -> ArenaResult<()> {
            let bindings = self.lisp.car(args)?;
            let body_list = self.lisp.cdr(args)?;

            let mut local_env = *env;
            let mut cur = bindings;
            while !cur.is_nil() {
                let binding = self.lisp.car(cur)?;
                let name = self.lisp.car(binding)?;
                let val_expr = self.lisp.car(self.lisp.cdr(binding)?)?;
                let val = self.eval(val_expr, *env)?;
                local_env = env_bind(self.lisp, local_env, name, val)?;
                cur = self.lisp.cdr(cur)?;
            }

            *env = local_env;
            *expr = self.wrap_begin(body_list)?;
            Ok(())
        })())
    }

    /// `(vau params env-param body)` — create a fexpr (operative).
    ///
    /// The resulting operative, when called, receives its arguments
    /// unevaluated and the caller's environment bound to `env-param`.
    fn op_vau(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail((|| {
            let params = self.lisp.car(args)?;
            let rest = self.lisp.cdr(args)?;
            let env_param = self.lisp.car(rest)?;
            let body_list = self.lisp.cdr(rest)?;
            let body = self.wrap_begin(body_list)?;
            // If env_param is the symbol `#ignore`, use NIL to signal "no binding".
            let ep = if self.lisp.symbol_name_eq(env_param, "#ignore") {
                ArenaIndex::NIL
            } else {
                env_param
            };
            self.lisp.vau(params, ep, body, *env)
        })())
    }

    /// `(eval expr env)` — evaluate an expression in a given environment,
    /// or `(eval expr)` — evaluate in the current environment.
    fn op_eval(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail((|| {
            let expr_arg = self.lisp.car(args)?;
            let expr_val = self.eval(expr_arg, *env)?;
            let rest = self.lisp.cdr(args)?;
            if rest.is_nil() {
                self.eval(expr_val, *env)
            } else {
                let env_arg = self.lisp.car(rest)?;
                let env_val = self.eval(env_arg, *env)?;
                self.eval(expr_val, env_val)
            }
        })())
    }

    // ================================================================
    // Applicative operatives (evaluate args, then apply pure function)
    // ================================================================

    /// `(cons a b)` — strict cons: evaluates both arguments.
    fn op_cons(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail((|| {
            let a_expr = self.lisp.car(args)?;
            let a_val = self.eval(a_expr, *env)?;
            self.push_root(a_val);
            self.push_root(*env);
            let b_expr = self.lisp.car(self.lisp.cdr(args)?)?;
            let b_val = self.eval(b_expr, *env)?;
            self.pop_roots(2);
            self.lisp.cons(a_val, b_val)
        })())
    }

    /// Helper: evaluate args then apply a pure function.
    fn applicative(
        &mut self,
        args: ArenaIndex,
        env: &mut ArenaIndex,
        f: fn(&Self, ArenaIndex) -> ArenaResult<ArenaIndex>,
    ) -> TailAction {
        non_tail((|| {
            let evaled = self.eval_args(args, *env)?;
            f(self, evaled)
        })())
    }

    // — Arithmetic —

    fn op_add(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_add)
    }

    fn op_sub(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_sub)
    }

    fn op_mul(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_mul)
    }

    fn op_div(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_div)
    }

    fn op_eq(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_eq)
    }

    fn op_lt(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_lt)
    }

    fn op_gt(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_gt)
    }

    fn op_le(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_le)
    }

    fn op_ge(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_ge)
    }

    fn op_car(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_car)
    }

    fn op_cdr(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_cdr)
    }

    fn op_list(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_list)
    }

    fn op_nullp(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_nullp)
    }

    fn op_not(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_not)
    }

    fn op_pairp(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_pairp)
    }

    fn op_numberp(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_numberp)
    }

    fn op_symbolp(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_symbolp)
    }

    fn op_booleanp(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.applicative(args, env, Self::builtin_booleanp)
    }

    // ================================================================
    // Utility methods
    // ================================================================

    /// Wrap a list of expressions in a `begin` form if there are multiple,
    /// or return the single expression if there's only one.
    fn wrap_begin(&self, exprs: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if exprs.is_nil() {
            return self.lisp.nil();
        }
        let rest = self.lisp.cdr(exprs)?;
        if rest.is_nil() {
            return self.lisp.car(exprs);
        }
        let begin_sym = self.lisp.symbol("begin")?;
        self.lisp.cons(begin_sym, exprs)
    }

    // ================================================================
    // Pure builtin implementations (operate on evaluated args)
    // ================================================================

    /// `(+ ...)` — variadic addition.
    fn builtin_add(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        fold_numbers!(self, args, 0, checked_add)
    }

    /// `(- a b ...)` — subtraction. With one arg, negates.
    fn builtin_sub(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if args.is_nil() {
            return Err(ArenaError::InvalidArgument);
        }
        let first = self.lisp.get(self.lisp.car(args)?)?.as_number()?;
        let rest = self.lisp.cdr(args)?;
        if rest.is_nil() {
            return self
                .lisp
                .number(first.checked_neg().ok_or(ArenaError::ArithmeticOverflow)?);
        }
        fold_numbers!(self, rest, first, checked_sub)
    }

    /// `(* ...)` — variadic multiplication.
    fn builtin_mul(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        fold_numbers!(self, args, 1, checked_mul)
    }

    /// `(/ a b)` — integer division.
    fn builtin_div(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let (a, b) = binary_nums!(self, args);
        if b == 0 {
            return Err(ArenaError::DivisionByZero);
        }
        self.lisp.number(a / b)
    }

    cmp_builtin!(builtin_eq, ==);
    cmp_builtin!(builtin_lt, <);
    cmp_builtin!(builtin_gt, >);
    cmp_builtin!(builtin_le, <=);
    cmp_builtin!(builtin_ge, >=);

    // — Pair / list built-ins —

    /// `(list ...)` — return args as-is (already evaluated).
    fn builtin_list(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        Ok(args)
    }

    pair_builtin!(builtin_car, car);
    pair_builtin!(builtin_cdr, cdr);

    // — Type predicate built-ins —

    type_predicate!(builtin_nullp, Value::Nil);
    type_predicate!(builtin_not, Value::Boolean(false));
    type_predicate!(builtin_pairp, Value::Cons { .. });
    type_predicate!(builtin_numberp, Value::Number(_));
    type_predicate!(builtin_symbolp, Value::Symbol(_));
    type_predicate!(builtin_booleanp, Value::Boolean(_));
}
