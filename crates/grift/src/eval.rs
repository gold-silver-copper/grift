//! Strict Lisp evaluator with Kernel-style operative/applicative semantics.
//!
//! Evaluates arena-allocated S-expressions in an environment using
//! call-by-value evaluation with tail-call optimization.
//!
//! Following Shutt's vau calculus (Kernel language), the combiner system
//! is unified: the **operative** is the sole primitive, and the
//! **applicative** is a derived wrapper that evaluates arguments before
//! delegating to the wrapped combiner.

use grift_arena::{ArenaError, ArenaIndex, ArenaResult, GcStats};

use crate::lisp::Lisp;
use crate::value::{BuiltinId, Value};

/// Convert a fallible block into a `TailAction`: `Ok(())` → `Continue`,
/// `Err(e)` → `Return(Err(e))`.  Wraps the body in an IIFE so `?` and
/// early `return` work naturally inside operatives.
macro_rules! tail_continue {
    ($body:expr) => {
        match (|| -> ArenaResult<()> { $body })() {
            Ok(()) => TailAction::Continue,
            Err(e) => TailAction::Return(Err(e)),
        }
    };
}

/// Wrap a fallible closure result as `TailAction::Return` (non-tail position).
macro_rules! non_tail {
    ($body:expr) => {
        TailAction::Return((|| $body)())
    };
}

/// Generate a variadic type-predicate builtin method.
/// `(pred? . objects)` returns `#t` iff every object matches the pattern.
macro_rules! type_predicate {
    ($name:ident, $pat:pat) => {
        fn $name(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
            let mut cur = args;
            while !cur.is_nil() {
                let val = self.lisp.car(cur)?;
                if !matches!(self.lisp.get(val)?, $pat) {
                    return Ok(ArenaIndex::FALSE);
                }
                cur = self.lisp.cdr(cur)?;
            }
            Ok(ArenaIndex::TRUE)
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
            let a = self.lisp.get(self.lisp.car(args)?)?.as_number()?;
            let b = self.lisp.get(self.lisp.cadr(args)?)?.as_number()?;
            Ok(ArenaIndex::from_bool(a $op b))
        }
    };
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
        "set!"   => op_set    => op_set,
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
        "inert?"  => bi_inertp  => builtin_inertp,
        "ignore?" => bi_ignorep => builtin_ignorep,
        "eq?"    => bi_eqp    => builtin_eqp,
        "equal?" => bi_equalp => builtin_equalp,
        "eval"   => bi_eval   => builtin_eval,
        "wrap"   => bi_wrap   => builtin_wrap,
        "unwrap" => bi_unwrap => builtin_unwrap,
        "operative?" => bi_operativep => builtin_operativep,
        "applicative?" => bi_applicativep => builtin_applicativep,
        "make-environment" => bi_make_env => builtin_make_env,
        "make-empty-environment" => bi_make_empty_env => builtin_make_empty_env,
        "environment?" => bi_environmentp => builtin_environmentp,
        "gc-collect" => bi_gc_collect => builtin_gc_collect,
        "gc-enable"  => bi_gc_enable  => builtin_gc_enable,
        "gc-disable" => bi_gc_disable => builtin_gc_disable,
        "gc-enabled?" => bi_gc_enabledp => builtin_gc_enabledp,
    }
}

/// The evaluator state.
pub(crate) struct Evaluator<'a, const N: usize> {
    lisp: &'a Lisp<N>,
    /// The ground environment containing all builtins. This environment
    /// is immutable per Kernel §3.2: programs cannot capture or mutate
    /// any improper ancestor of the ground environment.
    ground_env: ArenaIndex,
    /// The standard environment — a child of the ground environment —
    /// where top-level expressions are evaluated (Kernel §3.2).
    pub global_env: ArenaIndex,
    /// Shadow stack of GC roots stored as a linked list of cons cells in
    /// the arena.
    gc_roots: ArenaIndex,
}

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Create a new evaluator with operatives bound in the ground environment.
    /// The global_env is a standard environment (child of ground).
    pub fn new(lisp: &'a Lisp<N>) -> ArenaResult<Self> {
        let ground_env = lisp.make_env(ArenaIndex::NIL)?;
        let mut eval = Evaluator {
            lisp,
            ground_env,
            global_env: ArenaIndex::NIL,
            gc_roots: ArenaIndex::NIL,
        };
        eval.init_builtins();
        // Create the standard environment as a child of the ground environment.
        eval.global_env = lisp.make_child_env(ground_env)?;
        Ok(eval)
    }

    /// Bind a builtin in the ground environment. If `wrap` is true, wraps it as an applicative.
    fn bind_builtin(&mut self, name: &str, id: BuiltinId, wrap: bool) {
        let Ok(sym) = self.lisp.symbol(name) else {
            return;
        };
        let Ok(mut val) = self.lisp.arena.alloc(id.into()) else {
            return;
        };
        if wrap {
            let Ok(wrapped) = self.lisp.wrap(val) else {
                return;
            };
            val = wrapped;
        }
        let _ = self.lisp.env_define(self.ground_env, sym, val);
    }

    /// Push a value onto the GC root stack so it survives collection.
    #[inline]
    fn push_root(&mut self, idx: ArenaIndex) -> ArenaResult<()> {
        let new_roots = self.lisp.cons(idx, self.gc_roots)?;
        self.gc_roots = new_roots;
        Ok(())
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
    #[cold]
    fn collect_garbage(&self, expr: ArenaIndex, env: ArenaIndex) -> GcStats {
        self.lisp.arena.collect_garbage(&[
            expr, env, self.ground_env, self.global_env, self.gc_roots,
            ArenaIndex::TRUE, ArenaIndex::FALSE,
            ArenaIndex::INERT, ArenaIndex::IGNORE,
        ])
    }

    /// Trigger garbage collection unconditionally (ignores gc_enabled flag).
    /// Used by the `gc-collect` builtin for explicit manual collection.
    #[cold]
    fn collect_garbage_unconditional(&self) -> GcStats {
        self.lisp.arena.collect_garbage_unconditional(&[
            self.ground_env, self.global_env, self.gc_roots,
            ArenaIndex::TRUE, ArenaIndex::FALSE,
            ArenaIndex::INERT, ArenaIndex::IGNORE,
        ])
    }

    /// Evaluate an expression in an environment (with TCO).
    ///
    /// Kernel-style dispatch: three combiner types —
    /// `Operative` (compound fexpr), `Applicative` (wrapper that evals args),
    /// and `Builtin` (primitive operative).
    ///
    /// Garbage collection is triggered only on allocation failure (OOM):
    /// when any operation returns `OutOfMemory`, the evaluator restores
    /// the GC root stack, collects garbage, and retries.
    pub fn eval(&mut self, mut expr: ArenaIndex, mut env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        loop {
            let saved_gc_roots = self.gc_roots;
            match self.eval_step(&mut expr, &mut env) {
                Ok(Some(result)) => return Ok(result),
                Ok(None) => continue,
                Err(ArenaError::OutOfMemory) => {
                    self.gc_roots = saved_gc_roots;
                    let stats = self.collect_garbage(expr, env);
                    if !stats.did_collect() {
                        return Err(ArenaError::OutOfMemory);
                    }
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// One step of the eval trampoline. Returns:
    /// - `Ok(Some(value))` — evaluation complete, return this value.
    /// - `Ok(None)` — TCO: `expr`/`env` have been updated, re-enter the loop.
    /// - `Err(e)` — error (including OOM).
    fn eval_step(
        &mut self,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> Result<Option<ArenaIndex>, ArenaError> {
        match self.lisp.get(*expr)? {
            Value::Symbol(_) => {
                Ok(Some(self.lisp.env_lookup(*env, *expr)?))
            }

            Value::Cons { car, cdr } => {
                self.push_root(cdr)?;
                self.push_root(*env)?;

                let func_val = self.eval(car, *env)?;

                match self.lisp.get(func_val)? {
                    Value::Builtin(id) => {
                        let action = self.apply_operative_builtin(id, cdr, expr, env);
                        self.pop_roots(2);
                        match action {
                            TailAction::Return(val) => val.map(Some),
                            TailAction::Continue => Ok(None),
                        }
                    }

                    Value::Operative { .. } => {
                        let (body, op_env) = self.invoke_operative(func_val, cdr, *env)?;
                        self.pop_roots(2);
                        *env = op_env;
                        *expr = body;
                        Ok(None)
                    }

                    Value::Applicative(inner) => {
                        let evaled_args = self.eval_args(cdr, *env)?;
                        self.push_root(evaled_args)?;

                        let inner_val = self.lisp.get(inner)?;
                        self.pop_roots(3);

                        match inner_val {
                            Value::Operative { .. } => {
                                let (body, op_env) =
                                    self.invoke_operative(inner, evaled_args, *env)?;
                                *env = op_env;
                                *expr = body;
                                Ok(None)
                            }
                            Value::Builtin(id) => {
                                Ok(Some(self.apply_builtin_pure(id, evaled_args)?))
                            }
                            Value::Applicative(_) => {
                                Ok(Some(self.apply_combiner(inner, evaled_args, *env)?))
                            }
                            _ => {
                                Err(ArenaError::NotCallable)
                            }
                        }
                    }

                    _ => {
                        self.pop_roots(2);
                        Err(ArenaError::NotCallable)
                    }
                }
            }

            _ => Ok(Some(*expr)), // self-evaluating: literals, closures, builtins,
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
    #[inline]
    fn invoke_operative(
        &self,
        func: ArenaIndex,
        args: ArenaIndex,
        caller_env: ArenaIndex,
    ) -> ArenaResult<(ArenaIndex, ArenaIndex)> {
        let (params, env_param, body, closed_env) = self.lisp.vau_parts(func)?;
        let op_env = self.lisp.make_child_env(closed_env)?;
        self.match_ptree(params, args, op_env)?;
        if !env_param.is_nil() {
            self.lisp.env_define(op_env, env_param, caller_env)?;
        }
        Ok((body, op_env))
    }

    /// Recursively match a formal parameter tree `ptree` against a value `obj`
    /// in environment `env`, binding each symbol in `ptree` to the
    /// corresponding part of `obj`.
    ///
    /// Per the Kernel spec (§4.9.1):
    /// - If ptree is a symbol, bind it to obj.
    /// - If ptree is `#ignore`, do nothing.
    /// - If ptree is nil, obj must be nil (else error).
    /// - If ptree is a pair, obj must be a pair; match car/cdr recursively.
    fn match_ptree(
        &self,
        ptree: ArenaIndex,
        obj: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<()> {
        if ptree.is_nil() {
            if obj.is_nil() {
                return Ok(());
            }
            return Err(ArenaError::TypeError);
        }
        match self.lisp.get(ptree)? {
            Value::Ignore => Ok(()),
            Value::Symbol(_) => {
                self.lisp.env_define(env, ptree, obj)
            }
            Value::Cons {
                car: ptree_car,
                cdr: ptree_cdr,
            } => {
                let (obj_car, obj_cdr) = self.lisp.get(obj)?.as_cons()?;
                self.match_ptree(ptree_car, obj_car, env)?;
                self.match_ptree(ptree_cdr, obj_cdr, env)
            }
            _ => Err(ArenaError::TypeError),
        }
    }

    /// Evaluate all arguments in a list (iterative with in-place reversal).
    fn eval_args(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        // Build result in reverse order to avoid O(n) Rust stack depth.
        let mut cur = args;
        let mut reversed = ArenaIndex::NIL;
        while !cur.is_nil() {
            let head_expr = self.lisp.car(cur)?;
            self.push_root(reversed)?;
            self.push_root(env)?;
            self.push_root(cur)?;
            let head_val = self.eval(head_expr, env)?;
            self.pop_roots(3);
            reversed = self.lisp.cons(head_val, reversed)?;
            cur = self.lisp.cdr(cur)?;
        }
        // Reverse in place — all cons cells are freshly allocated by us.
        self.reverse_list(reversed)
    }

    /// Reverse a singly-linked cons-list in place by swapping cdr pointers.
    #[inline]
    fn reverse_list(&self, mut list: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let mut prev = ArenaIndex::NIL;
        while !list.is_nil() {
            let Value::Cons { car, cdr } = self.lisp.get(list)? else {
                return Err(ArenaError::TypeError);
            };
            self.lisp.arena.set(list, Value::Cons { car, cdr: prev })?;
            prev = list;
            list = cdr;
        }
        Ok(prev)
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
        tail_continue!({
            let test_expr = self.lisp.car(args)?;
            let rest = self.lisp.cdr(args)?;
            let test_val = self.eval(test_expr, *env)?;
            *expr = if self.lisp.get(test_val)?.as_bool()? {
                self.lisp.car(rest)?
            } else {
                let else_rest = self.lisp.cdr(rest)?;
                if else_rest.is_nil() {
                    ArenaIndex::NIL
                } else {
                    self.lisp.car(else_rest)?
                }
            };
            Ok(())
        })
    }

    /// `($define! definiend expression)` — Kernel §4.9.1.
    ///
    /// Evaluates `expression` in the dynamic environment and matches `definiend`
    /// (a formal parameter tree) to the result, binding symbols in the dynamic
    /// environment.  Returns `#inert`.
    ///
    /// Per Kernel §3.2, mutation of the ground environment or its ancestors
    /// is forbidden.
    fn op_define(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail!({
            // Protect the ground environment from mutation.
            if *env == self.ground_env {
                return Err(ArenaError::ImmutableEnvironment);
            }
            let definiend = self.lisp.car(args)?;
            self.validate_ptree(definiend)?;
            let val_expr = self.lisp.cadr(args)?;
            let val = self.eval(val_expr, *env)?;
            self.match_ptree(definiend, val, *env)?;
            Ok(ArenaIndex::INERT)
        })
    }

    /// `($set! exp1 formals exp2)` — Kernel §6.8.1.
    ///
    /// Evaluates `exp1` and `exp2` in the dynamic environment; call the
    /// results `env` and `obj`.  If `env` is not an environment, an error
    /// is signaled.  Then the operative matches `formals` to `obj` in
    /// environment `env` — i.e., the symbols of `formals` are bound in
    /// `env` to the corresponding parts of `obj` (exactly as `$define!`
    /// would).  Returns `#inert`.
    fn op_set(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail!({
            let env_expr = self.lisp.car(args)?;
            let rest = self.lisp.cdr(args)?;
            let definiend = self.lisp.car(rest)?;
            let val_expr = self.lisp.cadr(rest)?;

            let target_env = self.eval(env_expr, *env)?;
            // Validate target is an environment.
            if !matches!(self.lisp.get(target_env)?, Value::Environment { .. }) {
                return Err(ArenaError::TypeError);
            }
            // Protect the ground environment from mutation (§3.2).
            if target_env == self.ground_env {
                return Err(ArenaError::ImmutableEnvironment);
            }

            self.validate_ptree(definiend)?;
            let val = self.eval(val_expr, *env)?;
            self.match_ptree(definiend, val, target_env)?;
            Ok(ArenaIndex::INERT)
        })
    }

    /// `(lambda (params...) body...)`.
    /// Derived: `lambda = wrap(vau(params, #ignore, body))`.
    fn op_lambda(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail!({
            let params = self.lisp.car(args)?;
            let body = self.wrap_begin(self.lisp.cdr(args)?)?;
            self.lisp.lambda(params, body, *env)
        })
    }

    /// `(begin expr1 expr2 ...)` — all but last are non-tail, last is tail.
    fn op_begin(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        tail_continue!({
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
            *expr = ArenaIndex::NIL;
            Ok(())
        })
    }

    /// `(cond (test expr...) ...)` — tests are strict, last body expr is tail.
    fn op_cond(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        tail_continue!({
            let mut cur = args;
            while !cur.is_nil() {
                let clause = self.lisp.car(cur)?;
                let test = self.lisp.car(clause)?;
                let body = self.lisp.cdr(clause)?;

                let matched = self.lisp.symbol_name_eq(test, "else") || {
                    let test_val = self.eval(test, *env)?;
                    self.lisp.get(test_val)?.as_bool()?
                };

                if matched {
                    *expr = self.wrap_begin(body)?;
                    return Ok(());
                }
                cur = self.lisp.cdr(cur)?;
            }
            *expr = ArenaIndex::NIL;
            Ok(())
        })
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
        tail_continue!({
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
            *expr = ArenaIndex::from_bool(continue_while_truthy);
            Ok(())
        })
    }

    /// `(let ((name val) ...) body...)` — bindings are strict, body is tail.
    fn op_let(
        &mut self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        tail_continue!({
            let bindings = self.lisp.car(args)?;
            let body_list = self.lisp.cdr(args)?;

            let local_env = self.lisp.make_child_env(*env)?;
            self.push_root(local_env)?;
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
        })
    }

    /// `(vau params env-param body)` — create a fexpr (operative).
    ///
    /// Per the Kernel spec (§4.10.3):
    /// - `params` must be a valid formal parameter tree.
    /// - `env-param` must be either a symbol or `#ignore`.
    /// - If `env-param` is a symbol that also occurs in `params`, an error
    ///   is signaled.
    fn op_vau(
        &mut self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail!({
            let params = self.lisp.car(args)?;
            let env_param = self.lisp.cadr(args)?;
            let body_list = self.lisp.cdr(self.lisp.cdr(args)?)?;
            let body = self.wrap_begin(body_list)?;

            // Validate formals parameter tree and collect seen symbols.
            let seen_syms = self.validate_ptree(params)?;

            // Validate env-param: must be a symbol or #ignore.
            let ep = match self.lisp.get(env_param)? {
                Value::Ignore => ArenaIndex::NIL,
                Value::Symbol(_) => {
                    // env-param symbol must not also occur in formals.
                    if self.lisp.list_contains(seen_syms, env_param) {
                        return Err(ArenaError::InvalidArgument);
                    }
                    env_param
                }
                _ => return Err(ArenaError::TypeError),
            };

            self.lisp.vau(params, ep, body, *env)
        })
    }

    /// Validate that `ptree` is a well-formed formal parameter tree.
    ///
    /// Per the Kernel spec (§4.9.1), a valid ptree is:
    /// - A symbol or `#ignore`.
    /// - Nil (the empty list).
    /// - A pair whose car and cdr are both valid ptrees.
    ///
    /// Additionally, the tree must be acyclic and no symbol may occur
    /// more than once.
    ///
    /// Returns the cons-list of symbols found in the tree (for use by
    /// callers that need to check membership, e.g. `op_vau`).
    fn validate_ptree(&self, ptree: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.validate_ptree_inner(ptree, ArenaIndex::NIL, ArenaIndex::NIL)
    }

    /// Recursive helper for `validate_ptree`.
    ///
    /// `visited` is a cons-list of pair nodes already traversed (cycle
    /// detection).  `seen_syms` is a cons-list of symbol indices already
    /// encountered (duplicate detection).  Returns the updated
    /// `seen_syms` list on success.
    fn validate_ptree_inner(
        &self,
        ptree: ArenaIndex,
        visited: ArenaIndex,
        seen_syms: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        if ptree.is_nil() {
            return Ok(seen_syms);
        }
        match self.lisp.get(ptree)? {
            Value::Ignore => Ok(seen_syms),
            Value::Symbol(_) => {
                if self.lisp.list_contains(seen_syms, ptree) {
                    return Err(ArenaError::InvalidArgument);
                }
                self.lisp.cons(ptree, seen_syms)
            }
            Value::Cons {
                car: ptree_car,
                cdr: ptree_cdr,
            } => {
                // Cycle detection: this pair must not have been visited.
                if self.lisp.list_contains(visited, ptree) {
                    return Err(ArenaError::Cyclic);
                }
                let new_visited = self.lisp.cons(ptree, visited)?;
                let seen_syms = self.validate_ptree_inner(ptree_car, new_visited, seen_syms)?;
                self.validate_ptree_inner(ptree_cdr, new_visited, seen_syms)
            }
            _ => Err(ArenaError::TypeError),
        }
    }

    // ================================================================
    // Utility methods
    // ================================================================

    /// Wrap a list of expressions in a `begin` form if there are multiple,
    /// or return the single expression if there's only one.
    #[inline]
    fn wrap_begin(&self, exprs: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if exprs.is_nil() {
            return Ok(ArenaIndex::NIL);
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
        let a = self.lisp.get(self.lisp.car(args)?)?.as_number()?;
        let b = self.lisp.get(self.lisp.cadr(args)?)?.as_number()?;
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
    type_predicate!(builtin_pairp, Value::Cons { .. });
    type_predicate!(builtin_numberp, Value::Number(_));
    type_predicate!(builtin_symbolp, Value::Symbol(_));
    type_predicate!(builtin_booleanp, Value::Boolean(_));
    type_predicate!(builtin_inertp, Value::Inert);

    /// `(not boolean)` — boolean negation (requires exactly one boolean arg).
    fn builtin_not(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.lisp.car(args)?;
        let b = self.lisp.get(val)?.as_bool()?;
        Ok(ArenaIndex::from_bool(!b))
    }

    /// `(eq? object1 object2)` — identity predicate (§4.2.1).
    ///
    /// Returns `#t` iff the two objects are effectively the same object.
    /// For immutable, encapsulated types (booleans, nil, inert, symbols),
    /// eq? is determined by value. For mutable/constructed objects (pairs,
    /// environments), eq? compares arena identity.
    fn builtin_eqp(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let a = self.lisp.car(args)?;
        let b = self.lisp.cadr(args)?;
        Ok(ArenaIndex::from_bool(self.is_eq(a, b)?))
    }

    /// `(equal? object1 object2)` — structural equality predicate (§4.3.1).
    ///
    /// Returns `#t` iff the two objects "look" the same as long as nothing
    /// is mutated. Weaker than eq?; equal? returns true whenever eq? would.
    fn builtin_equalp(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let a = self.lisp.car(args)?;
        let b = self.lisp.cadr(args)?;
        Ok(ArenaIndex::from_bool(self.is_equal(a, b)?))
    }

    /// Core `eq?` logic: identity comparison.
    fn is_eq(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        // Same arena slot ⇒ same object.
        if a == b {
            return Ok(true);
        }
        let (va, vb) = (self.lisp.get(a)?, self.lisp.get(b)?);
        // Immutable encapsulated types: eq? is value-based.
        // Mutable/constructed types (pairs, environments, operatives,
        // applicatives, strings): eq? is arena identity only (checked above).
        Ok(va.is_immutable() && va == vb)
    }

    /// Core `equal?` logic: structural equality.
    fn is_equal(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        // eq? ⇒ equal? (Rule 2).
        if self.is_eq(a, b)? {
            return Ok(true);
        }
        let va = self.lisp.get(a)?;
        let vb = self.lisp.get(b)?;
        match (va, vb) {
            // Pairs: structural comparison of car and cdr.
            (Value::Cons { car: a1, cdr: a2 }, Value::Cons { car: b1, cdr: b2 }) => {
                Ok(self.is_equal(a1, b1)? && self.is_equal(a2, b2)?)
            }
            // Strings: compare character-by-character.
            (Value::String { data: da }, Value::String { data: db }) => {
                self.lisp.strings_equal(da, db)
            }
            // Environments: eq? only (identity-based).
            // Different environments are never equal? unless eq?.
            (Value::Environment { .. }, Value::Environment { .. }) => Ok(false),
            // Different types or non-matching values.
            _ => Ok(false),
        }
    }

    // — Kernel combiners —

    /// `(eval expr env)` — evaluate expression in given environment.
    fn builtin_eval(&mut self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let expr_val = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let env_val = if rest.is_nil() {
            self.global_env
        } else {
            self.lisp.car(rest)?
        };
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

    type_predicate!(
        builtin_operativep,
        Value::Operative { .. } | Value::Builtin(_)
    );
    type_predicate!(builtin_applicativep, Value::Applicative(_));

    /// `(make-environment . environments)` — create a new environment with
    /// zero or more parent environments (Kernel §4.8.4).
    ///
    /// The parents list is copied so that subsequent mutation of the
    /// argument list does not affect the constructed environment.
    fn builtin_make_env(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        // Validate all arguments are environments.
        let mut cur = args;
        while !cur.is_nil() {
            let v = self.lisp.car(cur)?;
            if !matches!(self.lisp.get(v)?, Value::Environment { .. }) {
                return Err(ArenaError::TypeError);
            }
            cur = self.lisp.cdr(cur)?;
        }
        // Copy the parents list so it's independent of the original.
        let parents = self.copy_list(args)?;
        self.lisp.make_env(parents)
    }

    /// `(make-empty-environment)` — always creates a parentless environment.
    fn builtin_make_empty_env(&self, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.lisp.make_env(ArenaIndex::NIL)
    }

    type_predicate!(builtin_environmentp, Value::Environment { .. });
    type_predicate!(builtin_ignorep, Value::Ignore);

    // ================================================================
    // GC control builtins
    // ================================================================

    /// `(gc-collect)` — manually trigger garbage collection.
    /// Returns the number of objects collected. Always runs unconditionally
    /// (ignores the gc-enabled flag), since the user explicitly requested it.
    fn builtin_gc_collect(&self, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let stats = self.collect_garbage_unconditional();
        self.lisp.number(stats.collected as isize)
    }

    /// `(gc-enable)` — enable automatic garbage collection on OOM.
    fn builtin_gc_enable(&self, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.lisp.arena.set_gc_enabled(true);
        Ok(ArenaIndex::INERT)
    }

    /// `(gc-disable)` — disable automatic garbage collection.
    /// When disabled, OOM errors propagate immediately without attempting GC.
    /// Manual `(gc-collect)` still works regardless.
    fn builtin_gc_disable(&self, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.lisp.arena.set_gc_enabled(false);
        Ok(ArenaIndex::INERT)
    }

    /// `(gc-enabled?)` — check if automatic GC is enabled.
    fn builtin_gc_enabledp(&self, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        Ok(ArenaIndex::from_bool(self.lisp.arena.is_gc_enabled()))
    }

    /// Copy a cons-list into fresh cons cells (iterative).
    fn copy_list(&self, list: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let mut cur = list;
        let mut reversed = ArenaIndex::NIL;
        while !cur.is_nil() {
            let head = self.lisp.car(cur)?;
            reversed = self.lisp.cons(head, reversed)?;
            cur = self.lisp.cdr(cur)?;
        }
        self.reverse_list(reversed)
    }
}
