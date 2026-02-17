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
        let b = $self
            .lisp
            .get($self.lisp.car($self.lisp.cdr($args)?)?)?
            .as_number()?;
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

/// Non-tail operative: wrap a `(args, env) -> Result` as `TailAction::Return`.
#[inline]
fn non_tail(result: ArenaResult<ArenaIndex>) -> TailAction {
    TailAction::Return(result)
}

// ============================================================================
// Builtin ID generation
// ============================================================================

/// Assign unique `BuiltinId` constants to all builtins.
macro_rules! assign_builtin_ids {
    ( $( $method:ident ),* $(,)? ) => {
        assign_builtin_ids!(@consts 0u8; $( $method, )*);
    };
    (@consts $id:expr; ) => {};
    (@consts $id:expr; $method:ident, $( $rest:ident, )*) => {
        #[allow(non_upper_case_globals)]
        const $method: BuiltinId = BuiltinId($id);
        assign_builtin_ids!(@consts $id + 1u8; $( $rest, )*);
    };
}

// All builtin IDs — operative builtins and applicative builtins share
// a single ID space since they all go through `Value::Builtin`.
assign_builtin_ids!(
    // Operative builtins (receive unevaluated args)
    op_quote,
    op_if,
    op_define,
    op_lambda,
    op_begin,
    op_cond,
    op_and,
    op_or,
    op_let,
    op_vau,
    // Applicative builtins (receive pre-evaluated args via Applicative wrapper)
    bi_cons,
    bi_add,
    bi_sub,
    bi_mul,
    bi_div,
    bi_eq,
    bi_lt,
    bi_gt,
    bi_le,
    bi_ge,
    bi_car,
    bi_cdr,
    bi_list,
    bi_nullp,
    bi_not,
    bi_pairp,
    bi_numberp,
    bi_symbolp,
    bi_booleanp,
    bi_eval,
    bi_wrap,
    bi_unwrap,
    bi_operativep,
    bi_applicativep,
);

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
    /// the arena.
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

    /// Register all builtins in the global environment.
    ///
    /// Operative builtins are bound bare as `Value::Builtin`.
    /// Applicative builtins are wrapped as `Value::Applicative(Value::Builtin)`.
    fn init_builtins(&mut self) {
        // Operative builtins — bound bare (receive unevaluated args + caller env)
        self.bind_operative("quote", op_quote);
        self.bind_operative("if", op_if);
        self.bind_operative("define", op_define);
        self.bind_operative("lambda", op_lambda);
        self.bind_operative("begin", op_begin);
        self.bind_operative("cond", op_cond);
        self.bind_operative("and", op_and);
        self.bind_operative("or", op_or);
        self.bind_operative("let", op_let);
        self.bind_operative("vau", op_vau);

        // Applicative builtins — wrapped (receive evaluated args)
        self.bind_applicative("cons", bi_cons);
        self.bind_applicative("+", bi_add);
        self.bind_applicative("-", bi_sub);
        self.bind_applicative("*", bi_mul);
        self.bind_applicative("/", bi_div);
        self.bind_applicative("=", bi_eq);
        self.bind_applicative("<", bi_lt);
        self.bind_applicative(">", bi_gt);
        self.bind_applicative("<=", bi_le);
        self.bind_applicative(">=", bi_ge);
        self.bind_applicative("car", bi_car);
        self.bind_applicative("cdr", bi_cdr);
        self.bind_applicative("list", bi_list);
        self.bind_applicative("null?", bi_nullp);
        self.bind_applicative("not", bi_not);
        self.bind_applicative("pair?", bi_pairp);
        self.bind_applicative("number?", bi_numberp);
        self.bind_applicative("symbol?", bi_symbolp);
        self.bind_applicative("boolean?", bi_booleanp);
        self.bind_applicative("eval", bi_eval);
        self.bind_applicative("wrap", bi_wrap);
        self.bind_applicative("unwrap", bi_unwrap);
        self.bind_applicative("operative?", bi_operativep);
        self.bind_applicative("applicative?", bi_applicativep);

        // Self-evaluating constants
        self.bind_self_evaluating("#ignore");
    }

    /// Bind an operative builtin as bare `Value::Builtin` in the environment.
    fn bind_operative(&mut self, name: &str, id: BuiltinId) {
        if let (Ok(sym), Ok(val)) = (
            self.lisp.symbol(name),
            self.lisp.arena.alloc(Value::Builtin(id)),
        ) {
            if let Ok(new_env) = env_bind(self.lisp, self.global_env, sym, val) {
                self.global_env = new_env;
            }
        }
    }

    /// Bind an applicative builtin as `Value::Applicative(Value::Builtin)`.
    fn bind_applicative(&mut self, name: &str, id: BuiltinId) {
        if let (Ok(sym), Ok(prim)) = (
            self.lisp.symbol(name),
            self.lisp.arena.alloc(Value::Builtin(id)),
        ) {
            if let Ok(wrapped) = self.lisp.arena.alloc(Value::Applicative(prim)) {
                if let Ok(new_env) = env_bind(self.lisp, self.global_env, sym, wrapped) {
                    self.global_env = new_env;
                }
            }
        }
    }

    /// Bind a symbol to itself so it evaluates to its own identity.
    fn bind_self_evaluating(&mut self, name: &str) {
        if let Ok(sym) = self.lisp.symbol(name) {
            if let Ok(new_env) = env_bind(self.lisp, self.global_env, sym, sym) {
                self.global_env = new_env;
            }
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
                    return env_lookup(self.lisp, env, expr)
                        .or_else(|_| env_lookup(self.lisp, self.global_env, expr));
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
                            let (params, env_param, body, closed_env) =
                                self.lisp.vau_parts(func_val)?;
                            let mut op_env = self.bind_vau_params(closed_env, params, cdr)?;
                            if !env_param.is_nil() {
                                op_env = env_bind(self.lisp, op_env, env_param, env)?;
                            }
                            env = op_env;
                            expr = body;
                            self.pop_roots(2);
                            continue;
                        }

                        Value::Applicative(inner) => {
                            let evaled_args = self.eval_args(cdr, env)?;
                            self.push_root(evaled_args);

                            match self.lisp.get(inner)? {
                                Value::Operative { .. } => {
                                    let (params, env_param, body, closed_env) =
                                        self.lisp.vau_parts(inner)?;
                                    let mut op_env =
                                        self.bind_vau_params(closed_env, params, evaled_args)?;
                                    if !env_param.is_nil() {
                                        op_env = env_bind(self.lisp, op_env, env_param, env)?;
                                    }
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
                let (params, env_param, body, closed_env) = self.lisp.vau_parts(combiner)?;
                let mut op_env = self.bind_vau_params(closed_env, params, evaled_args)?;
                if !env_param.is_nil() {
                    op_env = env_bind(self.lisp, op_env, env_param, caller_env)?;
                }
                self.eval(body, op_env)
            }
            Value::Builtin(id) => self.apply_builtin_pure(id, evaled_args),
            Value::Applicative(inner) => self.apply_combiner(inner, evaled_args, caller_env),
            _ => Err(ArenaError::NotCallable),
        }
    }

    /// Bind vau parameters to unevaluated argument expressions.
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
                    vau_env = env_bind(self.lisp, vau_env, params, arg_exprs)?;
                    return Ok(vau_env);
                }
                _ => return Err(ArenaError::TypeError),
            }
        }
        Ok(vau_env)
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
    // Operative builtin dispatch (TCO-aware, receives unevaluated args)
    // ================================================================

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
            op_quote => self.op_quote(args, expr, env),
            op_if => self.op_if(args, expr, env),
            op_define => self.op_define(args, expr, env),
            op_lambda => self.op_lambda(args, expr, env),
            op_begin => self.op_begin(args, expr, env),
            op_cond => self.op_cond(args, expr, env),
            op_and => self.op_and(args, expr, env),
            op_or => self.op_or(args, expr, env),
            op_let => self.op_let(args, expr, env),
            op_vau => self.op_vau(args, expr, env),
            _ => TailAction::Return(Err(ArenaError::NotCallable)),
        }
    }

    // ================================================================
    // Applicative builtin dispatch (receives pre-evaluated args)
    // ================================================================

    /// Dispatch an applicative builtin (receives already-evaluated args).
    #[allow(non_upper_case_globals)]
    fn apply_builtin_pure(&mut self, id: BuiltinId, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match id {
            bi_cons => self.builtin_cons(args),
            bi_add => self.builtin_add(args),
            bi_sub => self.builtin_sub(args),
            bi_mul => self.builtin_mul(args),
            bi_div => self.builtin_div(args),
            bi_eq => self.builtin_eq(args),
            bi_lt => self.builtin_lt(args),
            bi_gt => self.builtin_gt(args),
            bi_le => self.builtin_le(args),
            bi_ge => self.builtin_ge(args),
            bi_car => self.builtin_car(args),
            bi_cdr => self.builtin_cdr(args),
            bi_list => self.builtin_list(args),
            bi_nullp => self.builtin_nullp(args),
            bi_not => self.builtin_not(args),
            bi_pairp => self.builtin_pairp(args),
            bi_numberp => self.builtin_numberp(args),
            bi_symbolp => self.builtin_symbolp(args),
            bi_booleanp => self.builtin_booleanp(args),
            bi_eval => self.builtin_eval(args),
            bi_wrap => self.builtin_wrap(args),
            bi_unwrap => self.builtin_unwrap(args),
            bi_operativep => self.builtin_operativep(args),
            bi_applicativep => self.builtin_applicativep(args),
            _ => Err(ArenaError::NotCallable),
        }
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
    /// Derived: `lambda = wrap(vau(params, #ignore, body))`.
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
        let b = self.lisp.car(self.lisp.cdr(args)?)?;
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
        let rest = self.lisp.cdr(args)?;
        if rest.is_nil() {
            self.eval(expr_val, self.global_env)
        } else {
            let env_val = self.lisp.car(rest)?;
            self.eval(expr_val, env_val)
        }
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

    /// `(operative? x)` — #t if x is an operative (Operative or bare Builtin).
    fn builtin_operativep(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.lisp.car(args)?;
        self.lisp.boolean(matches!(
            self.lisp.get(val)?,
            Value::Operative { .. } | Value::Builtin(_)
        ))
    }

    /// `(applicative? x)` — #t if x is an applicative.
    fn builtin_applicativep(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.lisp.car(args)?;
        self.lisp
            .boolean(matches!(self.lisp.get(val)?, Value::Applicative(_)))
    }
}
