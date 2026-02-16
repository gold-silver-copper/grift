//! Lisp evaluator with call-by-need (lazy) semantics.
//!
//! Evaluates arena-allocated S-expressions in an environment using
//! call-by-need evaluation with memoization and tail-call optimization.

use grift_arena::{ArenaIndex, ArenaError, ArenaResult};

use crate::lisp::Lisp;
use crate::value::{Value, BuiltinId};

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
            acc = acc.$op(n).ok_or(ArenaError::InvalidIndex)?;
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
        let b = $self.lisp.get($self.lisp.car($self.lisp.cdr($args)?)?)?.as_number()?;
        (a, b)
    }};
}

/// Generate a non-tail special form that delegates to a `&mut self` inner
/// method.  Four special forms (`define`, `set!`, `lambda`, `cons`) share
/// the same pattern: accept the TCO `(expr, env)` pair but immediately
/// return a result via `TailAction::Return` without modifying them.
/// This macro captures that pattern, accepting an optional doc comment.
macro_rules! delegate_special_form {
    ($(#[doc = $doc:expr])* $vis:vis fn $name:ident -> $inner:ident) => {
        $(#[doc = $doc])*
        $vis fn $name(
            &mut self,
            args: ArenaIndex,
            _expr: &mut ArenaIndex,
            env: &mut ArenaIndex,
        ) -> TailAction {
            TailAction::Return(self.$inner(args, *env))
        }
    };
}
///
/// **Built-in functions** (section `builtins { ... }`) are registered in the
/// global environment as `Value::Builtin(id)` with auto-assigned sequential
/// IDs.  Their arguments are evaluated and forced to WHNF before the handler
/// is called (strict evaluation).
///
/// **Special forms** (section `special_forms { ... }`) are matched by symbol
/// name during evaluation and receive their arguments *unevaluated*.
/// Some special forms (e.g., `cons`) implement lazy semantics by wrapping
/// arguments in thunks.
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
            fn apply_builtin(&mut self, id: BuiltinId, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
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
        const $method: BuiltinId = BuiltinId($id);
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
    /// Shadow stack of GC roots stored as a linked list of cons cells in
    /// the arena.  Each entry is `(root_value . rest)`, with `ArenaIndex::NIL`
    /// as the empty list.  This replaces the former fixed-size array, so all
    /// allocation lives inside the arena.
    gc_roots: ArenaIndex,
}

/// Walk an environment association list, calling `f` on each (key, binding)
/// pair until `f` returns `Some(R)`.  Returns `Err(InvalidIndex)` when the
/// name is not found.
fn env_scan<const N: usize, R, F>(
    lisp: &Lisp<N>,
    env: ArenaIndex,
    name: ArenaIndex,
    mut f: F,
) -> ArenaResult<R>
where
    F: FnMut(ArenaIndex) -> ArenaResult<R>,
{
    let mut cur = env;
    while !cur.is_nil() {
        let binding = lisp.car(cur)?;
        if lisp.car(binding)? == name {
            return f(binding);
        }
        cur = lisp.cdr(cur)?;
    }
    Err(ArenaError::InvalidIndex)
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
    env_scan(lisp, env, name, |binding| lisp.cdr(binding))
}

/// Set a binding in an environment (mutate existing binding).
fn env_set<const N: usize>(
    lisp: &Lisp<N>,
    env: ArenaIndex,
    name: ArenaIndex,
    val: ArenaIndex,
) -> ArenaResult<()> {
    env_scan(lisp, env, name, |binding| {
        let key = lisp.car(binding)?;
        lisp.arena.set(binding, Value::Cons { car: key, cdr: val })
    })
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
            if self.gc_roots.is_nil() {
                break;
            }
            let rest = self.lisp.cdr(self.gc_roots);
            debug_assert!(rest.is_ok(), "GC root list corrupted during pop");
            if let Ok(rest) = rest {
                self.gc_roots = rest;
            } else {
                break;
            }
        }
    }

    /// Force a value to Weak Head Normal Form (WHNF), memoizing the result.
    pub fn force(&mut self, mut idx: ArenaIndex) -> ArenaResult<ArenaIndex> {
        loop {
            let val = self.lisp.get(idx)?;

            if val.is_whnf() {
                return Ok(idx);
            }

            match val {
                // Indirection — follow the pointer.
                Value::Indirection(target) => idx = target,

                // Thunk — force it via the black-hole protocol.
                Value::Thunk { expr, env } => {
                    let thunk_idx = idx;
                    self.lisp.arena.set(thunk_idx, Value::BlackHole)?;
                    self.push_root(thunk_idx);

                    let result = self.eval(expr, env)?;
                    self.pop_roots(1);

                    // Follow indirections, detecting cycles via black holes.
                    let mut target = result;
                    loop {
                        match self.lisp.get(target)? {
                            Value::Indirection(t) => target = t,
                            Value::BlackHole => return Err(ArenaError::InvalidIndex),
                            _ => break,
                        }
                    }

                    // Memoize: overwrite thunk cell with indirection.
                    self.lisp.arena.set(thunk_idx, Value::Indirection(target))?;
                    idx = target;
                }

                // Circular dependency detected.
                Value::BlackHole => return Err(ArenaError::InvalidIndex),

                _ => unreachable!(),
            }
        }
    }

    /// Trigger garbage collection using all known live roots.
    ///
    /// The roots include the global environment, the current expression
    /// and environment, and the GC root linked list (which the arena's
    /// mark phase will trace through automatically).
    fn collect_garbage(&self, expr: ArenaIndex, env: ArenaIndex) {
        self.lisp.arena.collect_garbage(&[
            expr, env, self.global_env, self.gc_roots,
        ]);
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

    /// Evaluate an expression and immediately force the result to WHNF.
    #[inline]
    fn eval_force(&mut self, expr: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.eval(expr, env)?;
        self.force(val)
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
                // Thunk encountered as a bare expression — force it.
                Value::Thunk { .. } => return self.force(expr),

                // Indirection — follow it (no stack growth).
                Value::Indirection(target) => { expr = target; continue; }

                // Black hole — circular evaluation.
                Value::BlackHole => return Err(ArenaError::InvalidIndex),

                // Symbol → look up in local env, then global env.
                Value::Symbol(_) => {
                    let binding = env_lookup(self.lisp, env, expr)
                        .or_else(|_| env_lookup(self.lisp, self.global_env, expr))?;
                    if matches!(self.lisp.get(binding)?, Value::BlackHole) {
                        return Err(ArenaError::InvalidIndex);
                    }
                    return Ok(binding);
                }

                // List → special form or function application.
                Value::Cons { car, cdr } => {
                    self.push_root(cdr);
                    self.push_root(env);

                    // Check for special forms.
                    if matches!(self.lisp.get(car)?, Value::Symbol(_)) {
                        if let Some(action) = self.try_special_form_tco(car, cdr, &mut expr, &mut env) {
                            self.pop_roots(2);
                            match action {
                                TailAction::Return(val) => return val,
                                TailAction::Continue => continue,
                            }
                        }
                    }

                    // Function application (call-by-need).
                    let func_whnf = self.eval_force(car, env)?;

                    match self.lisp.get(func_whnf)? {
                        Value::Builtin(id) => {
                            let args = self.force_args(cdr, env)?;
                            self.pop_roots(2);
                            return self.apply_builtin(id, args);
                        }
                        Value::Lambda { .. } => {
                            let (params, body, closed_env) = self.lisp.lambda_parts(func_whnf)?;
                            env = self.bind_args_lazy(closed_env, params, cdr, env)?;
                            expr = body;
                            self.pop_roots(2);
                            continue; // ← TCO
                        }
                        _ => {
                            self.pop_roots(2);
                            return Err(ArenaError::InvalidIndex);
                        }
                    }
                }

                _ => unreachable!(),
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
                    let thunk = self.lisp.thunk(arg_expr, call_env)?;
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

    /// Build a list of thunks from a list of expressions (iterative, O(n)).
    fn make_thunk_list(
        &self,
        exprs: ArenaIndex,
        call_env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let mut reversed = ArenaIndex::NIL;
        let mut cur = exprs;
        while !cur.is_nil() {
            let thunk = self.lisp.thunk(self.lisp.car(cur)?, call_env)?;
            reversed = self.lisp.cons(thunk, reversed)?;
            cur = self.lisp.cdr(cur)?;
        }
        // Reverse to restore original order.
        let mut result = ArenaIndex::NIL;
        while !reversed.is_nil() {
            let head = self.lisp.car(reversed)?;
            result = self.lisp.cons(head, result)?;
            reversed = self.lisp.cdr(reversed)?;
        }
        Ok(result)
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
        // Protect `args` and `env` across recursive eval/force calls.
        self.push_root(args);
        self.push_root(env);

        let head_expr = self.lisp.car(args)?;
        let head_forced = self.eval_force(head_expr, env)?;

        // Protect `head_forced` across the recursive force_args call.
        self.push_root(head_forced);

        let tail = self.lisp.cdr(args)?;
        let tail_forced = self.force_args(tail, env)?;

        self.pop_roots(3); // head_forced, env, args

        self.lisp.cons(head_forced, tail_forced)
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

            let test_forced = self.eval_force(test_expr, *env)?;

            if self.lisp.get(test_forced)?.is_truthy() {
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

    delegate_special_form! {
        /// `(define name expr)` or `(define (name params...) body)`.
        fn eval_define -> eval_define_inner
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
                let thunk = self.lisp.thunk(val_expr, env)?;
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

    delegate_special_form! {
        /// `(set! name expr)`.
        fn eval_set -> eval_set_inner
    }

    fn eval_set_inner(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let name = self.lisp.car(args)?;
        let expr = self.lisp.car(self.lisp.cdr(args)?)?;
        let forced = self.eval_force(expr, env)?;
        env_set(self.lisp, env, name, forced)
            .or_else(|_| env_set(self.lisp, self.global_env, name, forced))?;
        self.lisp.nil()
    }

    delegate_special_form! {
        /// `(lambda (params...) body...)`.
        fn eval_lambda -> eval_lambda_inner
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
                    let test_forced = self.eval_force(test, *env)?;
                    self.lisp.get(test_forced)?.is_truthy()
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

    /// Shared implementation for `and`/`or` short-circuit evaluation.
    ///
    /// When `short_on_truthy` is `true`, behaves as `and` (short-circuits on
    /// falsy, defaults to `#t`).  When `false`, behaves as `or` (short-circuits
    /// on truthy, defaults to `#f`).
    fn eval_short_circuit(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
        short_on_truthy: bool,
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
                let forced = self.eval_force(e, *env)?;
                if self.lisp.get(forced)?.is_truthy() != short_on_truthy {
                    *expr = forced;
                    return Ok(());
                }
                cur = next;
            }
            *expr = self.lisp.boolean(short_on_truthy)?;
            Ok(())
        })())
    }

    /// `(let ((name val) ...) body...)` — bindings are lazy, body is tail.
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
                let thunk = self.lisp.thunk(val_expr, *env)?;
                local_env = env_bind(self.lisp, local_env, name, thunk)?;
                cur = self.lisp.cdr(cur)?;
            }

            *env = local_env;
            *expr = self.wrap_begin(body_list)?;
            Ok(())
        })())
    }

    delegate_special_form! {
        /// `(cons a b)` — lazy cons: does NOT evaluate arguments.
        fn eval_cons -> eval_cons_inner
    }

    fn eval_cons_inner(
        &mut self,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let a_expr = self.lisp.car(args)?;
        let b_expr = self.lisp.car(self.lisp.cdr(args)?)?;
        let a_thunk = self.lisp.thunk(a_expr, env)?;
        let b_thunk = self.lisp.thunk(b_expr, env)?;
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

    // — Arithmetic built-ins —

    /// `(+ ...)` — variadic addition.
    fn builtin_add(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        fold_numbers!(self, args, 0, checked_add)
    }

    /// `(- a b ...)` — subtraction. With one arg, negates.
    fn builtin_sub(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if args.is_nil() {
            return Err(ArenaError::InvalidIndex);
        }
        let first = self.lisp.get(self.lisp.car(args)?)?.as_number()?;
        let rest = self.lisp.cdr(args)?;
        if rest.is_nil() {
            return self.lisp.number(first.checked_neg().ok_or(ArenaError::InvalidIndex)?);
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
            return Err(ArenaError::InvalidIndex);
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

    /// Extract a component from the first argument (a pair) and force it.
    fn pair_accessor(&mut self, args: ArenaIndex, f: fn(&Lisp<N>, ArenaIndex) -> ArenaResult<ArenaIndex>) -> ArenaResult<ArenaIndex> {
        let pair = self.lisp.car(args)?;
        self.force(f(self.lisp, pair)?)
    }

    /// `(car pair)` — extract and force the car of a pair.
    fn builtin_car(&mut self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.pair_accessor(args, Lisp::car)
    }

    /// `(cdr pair)` — extract and force the cdr of a pair.
    fn builtin_cdr(&mut self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.pair_accessor(args, Lisp::cdr)
    }

    // — Type predicate built-ins —

    type_predicate!(builtin_nullp, Value::Nil);
    type_predicate!(builtin_not, Value::False);
    type_predicate!(builtin_pairp, Value::Cons { .. });
    type_predicate!(builtin_numberp, Value::Number(_));
    type_predicate!(builtin_symbolp, Value::Symbol(_));
    type_predicate!(builtin_booleanp, Value::True | Value::False);
}
