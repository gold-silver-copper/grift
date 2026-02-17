//! Strict Lisp evaluator with Kernel-style operative/applicative semantics.
//!
//! Evaluates arena-allocated S-expressions in an environment using
//! call-by-value evaluation with tail-call optimization.
//!
//! Following Shutt's vau calculus (Kernel language), the combiner system
//! is unified: the **operative** is the sole primitive, and the
//! **applicative** is a derived wrapper that evaluates arguments before
//! delegating to the wrapped combiner.

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
/// against a pattern.
macro_rules! type_predicate {
    ($name:ident, $pat:pat) => {
        fn $name(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
            let val = self.lisp.car(args)?;
            self.lisp.boolean(matches!(self.lisp.get(val)?, $pat))
        }
    };
}

/// Fold a variadic argument list over a checked arithmetic operation,
/// starting from `$init`.
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
        let b = $self.lisp.get($self.lisp.cadr($args)?)?.as_number()?;
        (a, b)
    }};
}

/// Generate a pair-accessor builtin (`car` or `cdr`).
macro_rules! pair_builtin {
    ($name:ident, $accessor:ident) => {
        fn $name(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
            let pair = self.lisp.car(args)?;
            self.lisp.$accessor(pair)
        }
    };
}

/// TCO control flow for operatives.
enum TailAction {
    /// Return this value immediately (non-tail position result).
    Return(ArenaResult<ArenaIndex>),
    /// expr and env have been updated; re-enter the eval loop.
    Continue,
}

impl TailAction {
    /// Wrap a non-tail result as `TailAction::Return`.
    #[inline]
    fn non_tail(result: ArenaResult<ArenaIndex>) -> Self {
        TailAction::Return(result)
    }
}

// ============================================================================
// Unified builtin registration
// ============================================================================

/// Generate all builtin infrastructure from a single declarative table:
/// BuiltinId constants, `init_builtins` registration, and dispatch methods.
macro_rules! define_builtins {
    (
        operatives { $( $op_name:literal => $op_id:ident => $op_method:ident, )* }
        applicatives { $( $bi_name:literal => $bi_id:ident => $bi_method:ident, )* }
    ) => {
        // — ID constants (single shared u8 space) —
        define_builtins!(@ids 0u8; $($op_id,)* $($bi_id,)*);

        impl<'a, const N: usize> Evaluator<'a, N> {
            /// Register all builtins in the global environment.
            fn init_builtins(&mut self) {
                $( self.bind_builtin($op_name, $op_id, false); )*
                $( self.bind_builtin($bi_name, $bi_id, true); )*
                self.bind_self_evaluating("#ignore");
            }

            /// Dispatch an operative builtin (receives unevaluated args + caller env).
            #[allow(non_upper_case_globals)]
            fn apply_operative_builtin(
                &mut self,
                id: BuiltinId,
                args: ArenaIndex,
                expr: &mut ArenaIndex,
                env: &mut ArenaIndex,
            ) -> TailAction {
                match id {
                    $( $op_id => self.$op_method(args, expr, env), )*
                    _ => TailAction::Return(Err(ArenaError::NotCallable)),
                }
            }

            /// Dispatch an applicative builtin (receives already-evaluated args).
            #[allow(non_upper_case_globals)]
            fn apply_builtin_pure(
                &mut self,
                id: BuiltinId,
                args: ArenaIndex,
            ) -> ArenaResult<ArenaIndex> {
                match id {
                    $( $bi_id => self.$bi_method(args), )*
                    _ => Err(ArenaError::NotCallable),
                }
            }
        }
    };
    // Recursive ID assignment
    (@ids $id:expr; ) => {};
    (@ids $id:expr; $head:ident, $($rest:ident,)*) => {
        #[allow(non_upper_case_globals)]
        const $head: BuiltinId = BuiltinId($id);
        define_builtins!(@ids $id + 1u8; $($rest,)*);
    };
}

define_builtins! {
    operatives {
        "quote"  => op_quote  => op_quote,
        "if"     => op_if     => op_if,
        "define!" => op_define => op_define,
        "lambda" => op_lambda => op_lambda,
        "begin"  => op_begin  => op_begin,
        "cond"   => op_cond   => op_cond,
        "and"    => op_and    => op_and,
        "or"     => op_or     => op_or,
        "let"    => op_let    => op_let,
        "vau"    => op_vau    => op_vau,
    }
    applicatives {
        "cons"   => bi_cons   => builtin_cons,
        "+"      => bi_add    => builtin_add,
        "-"      => bi_sub    => builtin_sub,
        "*"      => bi_mul    => builtin_mul,
        "/"      => bi_div    => builtin_div,
        "="      => bi_eq     => builtin_eq,
        "<"      => bi_lt     => builtin_lt,
        ">"      => bi_gt     => builtin_gt,
        "<="     => bi_le     => builtin_le,
        ">="     => bi_ge     => builtin_ge,
        "car"    => bi_car    => builtin_car,
        "cdr"    => bi_cdr    => builtin_cdr,
        "list"   => bi_list   => builtin_list,
        "null?"  => bi_nullp  => builtin_nullp,
        "not"    => bi_not    => builtin_not,
        "pair?"  => bi_pairp  => builtin_pairp,
        "number?" => bi_numberp => builtin_numberp,
        "symbol?" => bi_symbolp => builtin_symbolp,
        "boolean?" => bi_booleanp => builtin_booleanp,
        "eval"   => bi_eval   => builtin_eval,
        "wrap"   => bi_wrap   => builtin_wrap,
        "unwrap" => bi_unwrap => builtin_unwrap,
        "operative?" => bi_operativep => builtin_operativep,
        "applicative?" => bi_applicativep => builtin_applicativep,
        "make-environment" => bi_make_env => builtin_make_env,
        "make-empty-environment" => bi_make_empty_env => builtin_make_empty_env,
        "environment?" => bi_environmentp => builtin_environmentp,
    }
}

/// The evaluator state.
pub(crate) struct Evaluator<'a, const N: usize> {
    lisp: &'a Lisp<N>,
    pub global_env: ArenaIndex,
    /// Shadow stack of GC roots stored as a linked list of cons cells in
    /// the arena.
    gc_roots: ArenaIndex,
}

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Create a new evaluator with operatives bound in the global environment.
    pub fn new(lisp: &'a Lisp<N>) -> Self {
        let global_env = lisp
            .make_root_env()
            .expect("failed to allocate global environment");
        let mut eval = Evaluator {
            lisp,
            global_env,
            gc_roots: ArenaIndex::NIL,
        };
        eval.init_builtins();
        eval
    }

    /// Bind a builtin in the environment. If `wrap` is true, wraps it as an applicative.
    fn bind_builtin(&mut self, name: &str, id: BuiltinId, wrap: bool) {
        let Ok(sym) = self.lisp.symbol(name) else { return };
        let Ok(val) = self.lisp.arena.alloc(id.into()) else { return };
        let val = if wrap {
            let Ok(wrapped) = self.lisp.arena.alloc(Value::Applicative(val)) else { return };
            wrapped
        } else {
            val
        };
        let _ = self.lisp.env_define(self.global_env, sym, val);
    }

    /// Bind a symbol to itself so it evaluates to its own identity.
    fn bind_self_evaluating(&mut self, name: &str) {
        if let Ok(sym) = self.lisp.symbol(name) {
            let _ = self.lisp.env_define(self.global_env, sym, sym);
        }
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
    /// Kernel-style dispatch: three combiner types —
    /// `Operative` (compound fexpr), `Applicative` (wrapper that evals args),
    /// and `Builtin` (primitive operative).
    pub fn eval(&mut self, mut expr: ArenaIndex, mut env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        loop {
            self.maybe_collect(expr, env);

            let val = self.lisp.get(expr)?;

            if val.is_self_evaluating() {
                return Ok(expr);
            }

            match val {
                Value::Symbol(_) => {
                    return self.lisp.env_lookup(env, expr);
                }

                Value::Cons { car, cdr } => {
                    self.push_root(cdr);
                    self.push_root(env);

                    let func_val = self.eval(car, env)?;

                    match self.lisp.get(func_val)? {
                        Value::Builtin(id) => {
                            let action = self.apply_operative_builtin(id, cdr, &mut expr, &mut env);
                            self.pop_roots(2);
                            match action {
                                TailAction::Return(val) => return val,
                                TailAction::Continue => continue,
                            }
                        }

                        Value::Operative { .. } => {
                            let (body, op_env) = self.invoke_operative(func_val, cdr, env)?;
                            self.pop_roots(2);
                            env = op_env;
                            expr = body;
                            continue;
                        }

                        Value::Applicative(inner) => {
                            let evaled_args = self.eval_args(cdr, env)?;
                            self.push_root(evaled_args);

                            match self.lisp.get(inner)? {
                                Value::Operative { .. } => {
                                    let (body, op_env) =
                                        self.invoke_operative(inner, evaled_args, env)?;
                                    self.pop_roots(3);
                                    env = op_env;
                                    expr = body;
                                    continue;
                                }
                                Value::Builtin(id) => {
                                    self.pop_roots(3);
                                    return self.apply_builtin_pure(id, evaled_args);
                                }
                                Value::Applicative(_) => {
                                    self.pop_roots(3);
                                    return self.apply_combiner(inner, evaled_args, env);
                                }
                                _ => {
                                    self.pop_roots(3);
                                    return Err(ArenaError::NotCallable);
                                }
                            }
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

    /// Apply a combiner to a list of already-evaluated arguments.
    ///
    /// Used for double-wrapped applicatives and internal dispatch.
    fn apply_combiner(
        &mut self,
        combiner: ArenaIndex,
        evaled_args: ArenaIndex,
        caller_env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        match self.lisp.get(combiner)? {
            Value::Operative { .. } => {
                let (body, op_env) = self.invoke_operative(combiner, evaled_args, caller_env)?;
                self.eval(body, op_env)
            }
            Value::Builtin(id) => self.apply_builtin_pure(id, evaled_args),
            Value::Applicative(inner) => self.apply_combiner(inner, evaled_args, caller_env),
            _ => Err(ArenaError::NotCallable),
        }
    }

    /// Common operative invocation: destructure, bind params, optionally bind caller env.
    /// Returns `(body, operative_env)` for tail-call or direct eval.
    fn invoke_operative(
        &self,
        func: ArenaIndex,
        args: ArenaIndex,
        caller_env: ArenaIndex,
    ) -> ArenaResult<(ArenaIndex, ArenaIndex)> {
        let (params, env_param, body, closed_env) = self.lisp.vau_parts(func)?;
        let op_env = self.bind_vau_params(closed_env, params, args)?;
        if !env_param.is_nil() {
            self.lisp.env_define(op_env, env_param, caller_env)?;
        }
        Ok((body, op_env))
    }

    /// Bind vau parameters to unevaluated argument expressions.
    /// Creates a child environment of the closed-over environment.
    fn bind_vau_params(
        &self,
        closed_env: ArenaIndex,
        mut params: ArenaIndex,
        mut arg_exprs: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let child_env = self.lisp.make_child_env(closed_env)?;
        while !params.is_nil() && !arg_exprs.is_nil() {
            match self.lisp.get(params)? {
                Value::Cons {
                    car: param,
                    cdr: rest,
                } => {
                    let arg_expr = self.lisp.car(arg_exprs)?;
                    self.lisp.env_define(child_env, param, arg_expr)?;
                    params = rest;
                    arg_exprs = self.lisp.cdr(arg_exprs)?;
                }
                Value::Symbol(_) => {
                    self.lisp.env_define(child_env, params, arg_exprs)?;
                    return Ok(child_env);
                }
                _ => return Err(ArenaError::TypeError),
            }
        }
        Ok(child_env)
    }

    /// Evaluate all arguments in a list.
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
    // Operative builtin implementations (receive unevaluated args)
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
            *expr = if self.lisp.get(test_val)?.is_truthy() {
                self.lisp.car(rest)?
            } else {
                self.lisp.cdr(rest)
                    .and_then(|r| if r.is_nil() { self.lisp.nil() } else { self.lisp.car(r) })?
            };
            Ok(())
        })())
    }

    /// `(define! name expr)` or `(define! (name params...) body)`.
    fn op_define(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::non_tail((|| {
            let first = self.lisp.car(args)?;
            let rest = self.lisp.cdr(args)?;

            match self.lisp.get(first)? {
                Value::Symbol(_) => {
                    let val_expr = self.lisp.car(rest)?;
                    let val = self.eval(val_expr, *env)?;
                    self.lisp.env_define(*env, first, val)?;
                    Ok(val)
                }
                Value::Cons {
                    car: name,
                    cdr: params,
                } => {
                    let body = self.wrap_begin(rest)?;
                    let lam = self.lisp.lambda(params, body, *env)?;
                    self.lisp.env_define(*env, name, lam)?;
                    Ok(lam)
                }
                _ => Err(ArenaError::TypeError),
            }
        })())
    }

    /// `(lambda (params...) body...)`.
    /// Derived: `lambda = wrap(vau(params, #ignore, body))`.
    fn op_lambda(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::non_tail((|| {
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

            let local_env = self.lisp.make_child_env(*env)?;
            self.push_root(local_env);
            let mut cur = bindings;
            while !cur.is_nil() {
                let binding = self.lisp.car(cur)?;
                let name = self.lisp.car(binding)?;
                let val_expr = self.lisp.cadr(binding)?;
                let val = self.eval(val_expr, *env)?;
                self.lisp.env_define(local_env, name, val)?;
                cur = self.lisp.cdr(cur)?;
            }
            self.pop_roots(1);

            *env = local_env;
            *expr = self.wrap_begin(body_list)?;
            Ok(())
        })())
    }

    /// `(vau params env-param body)` — create a fexpr (operative).
    fn op_vau(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::non_tail((|| {
            let params = self.lisp.car(args)?;
            let env_param = self.lisp.cadr(args)?;
            let body_list = self.lisp.cdr(self.lisp.cdr(args)?)?;
            let body = self.wrap_begin(body_list)?;
            let ep = if self.lisp.symbol_name_eq(env_param, "#ignore") {
                ArenaIndex::NIL
            } else {
                env_param
            };
            self.lisp.vau(params, ep, body, *env)
        })())
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
    // Applicative builtin implementations (operate on evaluated args)
    // ================================================================

    /// `(cons a b)` — cons cell construction.
    fn builtin_cons(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let a = self.lisp.car(args)?;
        let b = self.lisp.cadr(args)?;
        self.lisp.cons(a, b)
    }

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

    // — Kernel combiners —

    /// `(eval expr env)` — evaluate expression in given environment.
    fn builtin_eval(&mut self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let expr_val = self.lisp.car(args)?;
        let env_val = self.lisp.cdr(args)
            .and_then(|rest| if rest.is_nil() { Ok(self.global_env) } else { self.lisp.car(rest) })?;
        self.eval(expr_val, env_val)
    }

    /// `(wrap combiner)` — wrap an operative into an applicative.
    fn builtin_wrap(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let combiner = self.lisp.car(args)?;
        self.lisp.wrap(combiner)
    }

    /// `(unwrap applicative)` — extract the underlying combiner.
    fn builtin_unwrap(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let app = self.lisp.car(args)?;
        self.lisp.unwrap_applicative(app)
    }

    type_predicate!(builtin_operativep, Value::Operative { .. } | Value::Builtin(_));
    type_predicate!(builtin_applicativep, Value::Applicative(_));

    /// `(make-environment [parent])`.
    fn builtin_make_env(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let parent = if args.is_nil() { ArenaIndex::NIL } else { self.lisp.car(args)? };
        self.lisp.make_child_env(parent)
    }

    /// `(make-empty-environment)` — always creates a parentless environment.
    fn builtin_make_empty_env(&self, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.lisp.make_child_env(ArenaIndex::NIL)
    }

    type_predicate!(builtin_environmentp, Value::Environment { .. });
}
