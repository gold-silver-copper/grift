//! Strict Lisp evaluator with call-by-value semantics.
//!
//! Evaluates arena-allocated S-expressions in an environment using
//! call-by-value evaluation with tail-call optimization.
//! The language semantics are referentially transparent — there is no
//! `set!` or other mutation visible to Lisp programs.

use grift_arena::{ArenaError, ArenaIndex, ArenaResult};

use crate::lisp::Lisp;
use crate::value::{BuiltinId, Value};

/// Convert a fallible closure into a `TailAction`: `Ok(())` → `Continue`,
/// `Err(e)` → `Return(Err(e))`.  Eliminates the repeated match boilerplate
/// in every TCO-aware special form.
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

/// Non-tail special form: wrap a `(args, env) -> Result` method as a
/// TCO `TailAction::Return`.
#[inline]
fn non_tail(result: ArenaResult<ArenaIndex>) -> TailAction {
    TailAction::Return(result)
}
///
/// **Built-in functions** (section `builtins { ... }`) are registered in the
/// global environment as `Value::Builtin(id)` with auto-assigned sequential
/// IDs.  Their arguments are evaluated before the handler is called.
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
        // Generate unique constant IDs for each builtin using index counting.
        define_builtins!(@consts 0u8; $( $bmethod, )*);

        impl<'a, const N: usize> Evaluator<'a, N> {
            /// Register all built-in functions in the global environment.
            fn init_builtins(&mut self) {
                $(
                    if let (Ok(sym), Ok(val)) = (
                        self.lisp.symbol($bname),
                        self.lisp.arena.alloc(Value::Builtin($bmethod)),
                    ) {
                        if let Ok(new_env) = env_bind(self.lisp, self.global_env, sym, val) {
                            self.global_env = new_env;
                        }
                    }
                )*
            }

            /// Apply a built-in function.
            #[allow(non_upper_case_globals)]
            fn apply_builtin(&mut self, id: BuiltinId, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
                match id {
                    $( $bmethod => self.$bmethod(args), )*
                    _ => Err(ArenaError::NotCallable),
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

    // Generate const declarations: base case.
    (@consts $id:expr; ) => {};
    // Generate const declarations: recursive case.
    (@consts $id:expr; $method:ident, $( $rest:ident, )*) => {
        #[allow(non_upper_case_globals)]
        const $method: BuiltinId = BuiltinId($id);
        define_builtins!(@consts $id + 1u8; $( $rest, )*);
    };
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
    /// Shadow stack of GC roots stored as a linked list of cons cells in
    /// the arena.  Each entry is `(root_value . rest)`, with `ArenaIndex::NIL`
    /// as the empty list.  This replaces the former fixed-size array, so all
    /// allocation lives inside the arena.
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
    /// Create a new evaluator with built-in functions bound in the global environment.
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
    ///
    /// The root stack is a linked list of cons cells in the arena:
    /// each entry is `(value . rest)`.
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
    ///
    /// The roots include the global environment, the current expression
    /// and environment, and the GC root linked list (which the arena's
    /// mark phase will trace through automatically).
    fn collect_garbage(&self, expr: ArenaIndex, env: ArenaIndex) {
        self.lisp
            .arena
            .collect_garbage(&[expr, env, self.global_env, self.gc_roots]);
    }

    /// Check arena memory pressure and collect garbage if needed.
    ///
    /// Triggers GC when the arena is more than 75% full, using the
    /// provided expression and environment as additional GC roots
    /// alongside the global environment.
    #[inline]
    fn maybe_collect(&self, expr: ArenaIndex, env: ArenaIndex) {
        let len = self.lisp.arena.len();
        let cap = self.lisp.arena.capacity();
        if len > cap * 3 / 4 {
            self.collect_garbage(expr, env);
        }
    }

    /// Evaluate an expression in an environment (with TCO).
    pub fn eval(&mut self, mut expr: ArenaIndex, mut env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        loop {
            // Collect garbage when the arena is under memory pressure.
            self.maybe_collect(expr, env);

            let val = self.lisp.get(expr)?;

            // Self-evaluating: literals, closures, builtins.
            if val.is_self_evaluating() {
                return Ok(expr);
            }

            match val {
                // Symbol → look up in local env, then global env.
                Value::Symbol(_) => {
                    return env_lookup(self.lisp, env, expr)
                        .or_else(|_| env_lookup(self.lisp, self.global_env, expr));
                }

                // List → special form or function application.
                Value::Cons { car, cdr } => {
                    self.push_root(cdr);
                    self.push_root(env);

                    // Check for special forms.
                    if matches!(self.lisp.get(car)?, Value::Symbol(_)) {
                        if let Some(action) =
                            self.try_special_form_tco(car, cdr, &mut expr, &mut env)
                        {
                            self.pop_roots(2);
                            match action {
                                TailAction::Return(val) => return val,
                                TailAction::Continue => continue,
                            }
                        }
                    }

                    // Function application (call-by-value).
                    let func_val = self.eval(car, env)?;

                    match self.lisp.get(func_val)? {
                        Value::Builtin(id) => {
                            let args = self.eval_args(cdr, env)?;
                            self.pop_roots(2);
                            return self.apply_builtin(id, args);
                        }
                        Value::Lambda { .. } => {
                            let (params, body, closed_env) = self.lisp.lambda_parts(func_val)?;
                            env = self.bind_args(closed_env, params, cdr, env)?;
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

    /// Evaluate all arguments in a list (for strict builtins and rest params).
    fn eval_args(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if args.is_nil() {
            return self.lisp.nil();
        }
        // Protect `args` and `env` across recursive eval calls.
        self.push_root(args);
        self.push_root(env);

        let head_expr = self.lisp.car(args)?;
        let head_val = self.eval(head_expr, env)?;

        // Protect `head_val` across the recursive eval_args call.
        self.push_root(head_val);

        let tail = self.lisp.cdr(args)?;
        let tail_vals = self.eval_args(tail, env)?;

        self.pop_roots(3); // head_val, env, args

        self.lisp.cons(head_val, tail_vals)
    }

    // — Special forms (TCO-aware) —

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
    fn eval_define(
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
    fn eval_lambda(
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
    fn eval_begin(
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
    fn eval_cond(
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
    fn eval_and(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.eval_short_circuit(args, expr, env, true)
    }

    /// `(or expr1 expr2 ...)` — strict on tests, last is tail.
    fn eval_or(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        self.eval_short_circuit(args, expr, env, false)
    }

    /// Shared `and`/`or` implementation.
    ///
    /// `continue_while_truthy`: when `true` (and), continues while tests are
    /// truthy and short-circuits on the first falsy value; defaults to `#t`.
    /// When `false` (or), continues while tests are falsy and short-circuits
    /// on the first truthy value; defaults to `#f`.
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
    fn eval_let(
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

    /// `(cons a b)` — strict cons: evaluates both arguments.
    fn eval_cons(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail((|| {
            let a_expr = self.lisp.car(args)?;
            let a_val = self.eval(a_expr, *env)?;
            self.push_root(a_val);
            let b_expr = self.lisp.car(self.lisp.cdr(args)?)?;
            let b_val = self.eval(b_expr, *env)?;
            self.pop_roots(1);
            self.lisp.cons(a_val, b_val)
        })())
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

    // — Arithmetic built-ins —

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

    /// `(list ...)` — return args as-is (already forced into a list).
    fn builtin_list(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        Ok(args)
    }

    // `(car pair)` / `(cdr pair)` — extract and force a pair component.
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
