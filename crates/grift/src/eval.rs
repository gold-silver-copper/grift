//! Strict Lisp evaluator with Kernel-style operative/applicative semantics.
//!
//! Evaluates arena-allocated S-expressions in an environment using
//! call-by-value evaluation with tail-call optimization (TCO).
//!
//! Following Shutt's vau calculus (Kernel language), the combiner system
//! is unified: the **operative** is the sole primitive, and the
//! **applicative** is a derived wrapper that evaluates arguments before
//! delegating to the wrapped combiner.
//!
//! ## Tail-Call Optimization
//!
//! The evaluator uses a *trampoline* loop: each iteration calls
//! [`eval_step`](Lisp::eval_step), which either returns a final value
//! (`Ok(Some(v))`), signals a tail call by mutating `expr`/`env` and
//! returning `Ok(None)`, or propagates an error. This allows unbounded
//! tail recursion without growing the Rust call stack.
//!
//! ## GC Integration
//!
//! Garbage collection is **demand-driven**: it is triggered only when an
//! allocation returns [`ArenaError::OutOfMemory`]. On OOM the evaluator
//! restores the GC root stack, runs a collection cycle, and retries.
//! If the collection frees no memory, the OOM error propagates.
//!
//! ## Builtin Registration
//!
//! All builtins are declared in a single `define_builtins!` invocation
//! which generates:
//! - `BuiltinId` constants (a shared `u8` namespace)
//! - `init_builtins` — binds each builtin in the ground environment
//! - `apply_operative_builtin` / `apply_builtin_pure` — dispatch tables

use crate::arena::{ArenaError, ArenaIndex, ArenaResult, GcStats};

use crate::lisp::{ArenaWriter, Lisp};
use crate::parse::SliceSource;
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
        #[doc = "Return `#t` iff every evaluated argument matches the expected"]
        #[doc = "runtime pattern for this predicate; otherwise return `#f`."]
        fn $name(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
            let mut cur = args;
            while !cur.is_nil() {
                let val = self.car(cur)?;
                if !matches!(self.get(val)?, $pat) {
                    return Ok(ArenaIndex::FALSE);
                }
                cur = self.cdr(cur)?;
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
            let n = $self.get($self.car(cur)?)?.as_number()?;
            acc = acc.$op(n).ok_or(ArenaError::ArithmeticOverflow)?;
            cur = $self.cdr(cur)?;
        }
        $self.number(acc)
    }};
}

/// Generate a numeric comparison builtin method.
macro_rules! cmp_builtin {
    ($name:ident, $op:tt) => {
        #[doc = "Compare two evaluated numeric arguments and return a boolean"]
        #[doc = "singleton representing the result."]
        fn $name(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
            let a = self.get(self.car(args)?)?.as_number()?;
            let b = self.get(self.cadr(args)?)?.as_number()?;
            Ok(ArenaIndex::from_bool(a $op b))
        }
    };
}

/// Generate a pair-accessor builtin (`car` or `cdr`).
macro_rules! pair_builtin {
    ($name:ident, $accessor:ident) => {
        #[doc = "Apply the corresponding pair/string accessor to the first"]
        #[doc = "evaluated argument."]
        fn $name(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
            let pair = self.car_char(args)?;
            self.$accessor(pair)
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

        impl<const N: usize> Lisp<N> {
            /// Register all builtins in the ground environment.
            pub(crate) fn init_builtins(&self) {
                $( self.bind_builtin($op_name, $op_id, false); )*
                $( self.bind_builtin($bi_name, $bi_id, true); )*
            }

            /// True if this builtin id names an operative builtin.
            #[allow(non_upper_case_globals)]
            fn is_operative_builtin(&self, id: BuiltinId) -> bool {
                matches!(id, $( $op_id )|*)
            }

            /// Dispatch an operative builtin (receives unevaluated args + caller env).
            #[allow(non_upper_case_globals)]
            fn apply_operative_builtin(
                &self,
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
                &self,
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
        "fn!"    => op_fn_define => op_fn_define,
        "set!"   => op_set    => op_set,
        "lambda" => op_lambda => op_lambda,
        "begin"  => op_begin  => op_begin,
        "cond"   => op_cond   => op_cond,
        "and"    => op_and    => op_and,
        "or"     => op_or     => op_or,
        "let"    => op_let    => op_let,
        "vau"    => op_vau    => op_vau,
        "current-environment" => op_current_env => op_current_env,
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
        "error"    => bi_error    => builtin_error,
        "apply"    => bi_apply    => builtin_apply,
        "raw-read-string"   => bi_raw_read_string   => builtin_raw_read_string,
        "raw-display-to-string" => bi_raw_display_to_string => builtin_raw_display_to_string,
        "raw-write-to-string"   => bi_raw_write_to_string   => builtin_raw_write_to_string,

    }
}

impl<const N: usize> Lisp<N> {
    /// Bind a builtin in the ground environment. If `wrap` is true, wraps it as an applicative.
    fn bind_builtin(&self, name: &str, id: BuiltinId, wrap: bool) {
        let Ok(sym) = self.symbol(name) else {
            return;
        };
        let Ok(mut val) = self.arena.alloc(id.into()) else {
            return;
        };
        if wrap {
            let Ok(wrapped) = self.wrap(val) else {
                return;
            };
            val = wrapped;
        }
        let _ = self.env_define(ArenaIndex::GROUND_ENV, sym, val);
    }

    // — GC root stack management (stored at ArenaIndex::GC_ROOTS) —

    /// Read the current head of the GC root stack.
    #[inline]
    fn gc_roots_head(&self) -> ArenaIndex {
        match self.arena.get(ArenaIndex::GC_ROOTS) {
            Ok(Value::Cons { car, .. }) => car,
            _ => ArenaIndex::NIL,
        }
    }

    /// Set the head of the GC root stack.
    #[inline]
    fn set_gc_roots_head(&self, head: ArenaIndex) {
        let _ = self.arena.set(
            ArenaIndex::GC_ROOTS,
            Value::Cons {
                car: head,
                cdr: ArenaIndex::NIL,
            },
        );
    }

    /// Push a value onto the GC root stack so it survives collection.
    #[inline]
    fn push_root(&self, idx: ArenaIndex) -> ArenaResult<()> {
        let old_head = self.gc_roots_head();
        let new_node = self.cons(idx, old_head)?;
        self.set_gc_roots_head(new_node);
        Ok(())
    }

    /// Pop `n` values from the GC root stack.
    #[inline]
    fn pop_roots(&self, n: usize) {
        for _ in 0..n {
            let head = self.gc_roots_head();
            debug_assert!(!head.is_nil(), "GC root stack underflow");
            match self.cdr(head) {
                Ok(rest) => self.set_gc_roots_head(rest),
                Err(_) => break,
            }
        }
    }

    /// Trigger garbage collection using all known live roots. Used for OOM collections.
    #[cold]
    fn eval_collect_garbage(&self, expr: ArenaIndex, env: ArenaIndex) -> GcStats {
        self.collect_with_roots(&[expr, env])
    }

    /// Trigger garbage collection unconditionally.
    ///
    /// This is used by the `gc-collect` builtin, which should force a
    /// collection cycle even when the evaluator is not currently handling OOM.
    #[cold]
    fn eval_collect_garbage_unconditional(&self) -> GcStats {
        self.collect_with_roots(&[])
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
    pub(crate) fn eval_expr(
        &self,
        mut expr: ArenaIndex,
        mut env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        loop {
            let saved_gc_roots = self.gc_roots_head();
            match self.eval_step(&mut expr, &mut env) {
                Ok(Some(result)) => return Ok(result),
                Ok(None) => continue,
                Err(ArenaError::OutOfMemory) => {
                    self.set_gc_roots_head(saved_gc_roots);
                    let stats = self.eval_collect_garbage(expr, env);
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
        &self,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> Result<Option<ArenaIndex>, ArenaError> {
        match self.get(*expr)? {
            Value::Symbol(_) => Ok(Some(self.env_lookup(*env, *expr)?)),

            Value::Cons { car, cdr } => {
                self.push_root(cdr)?;
                self.push_root(*env)?;

                let func_val = self.eval_expr(car, *env)?;

                match self.get(func_val)? {
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

                        let inner_val = self.get(inner)?;
                        self.pop_roots(3);

                        match inner_val {
                            Value::Operative { .. } => {
                                let (body, op_env) =
                                    self.invoke_operative(inner, evaled_args, *env)?;
                                *env = op_env;
                                *expr = body;
                                Ok(None)
                            }
                            Value::Builtin(id) => Ok(Some(
                                self.apply_builtin_value(id, evaled_args, *env)?,
                            )),
                            Value::Applicative(_) => {
                                Ok(Some(self.apply_combiner(inner, evaled_args, *env)?))
                            }
                            Value::Prelude(prelude) => {
                                let real = self.eval_prelude_source(prelude)?;
                                let (body, op_env) =
                                    self.invoke_operative(real, evaled_args, *env)?;
                                *env = op_env;
                                *expr = body;
                                Ok(None)
                            }
                            Value::Native(f) => {
                                Ok(Some(self.call_native(f, evaled_args)?))
                            }
                            _ => Err(ArenaError::NotCallable),
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

    /// Drive a builtin tail action to a concrete result.
    fn resolve_tail_action(
        &self,
        action: TailAction,
        expr: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        match action {
            TailAction::Return(result) => result,
            TailAction::Continue => self.eval_expr(expr, env),
        }
    }

    /// Invoke a builtin using the calling convention implied by its id.
    fn apply_builtin_value(
        &self,
        id: BuiltinId,
        args: ArenaIndex,
        caller_env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        if self.is_operative_builtin(id) {
            let mut expr = ArenaIndex::NIL;
            let mut env = caller_env;
            let action = self.apply_operative_builtin(id, args, &mut expr, &mut env);
            self.resolve_tail_action(action, expr, env)
        } else {
            self.apply_builtin_pure(id, args)
        }
    }

    /// Apply a combiner to an operand object.
    ///
    /// Applicatives expect `evaled_args` to contain evaluated arguments.
    /// Operatives receive it unchanged as their raw operand object.
    fn apply_combiner(
        &self,
        combiner: ArenaIndex,
        evaled_args: ArenaIndex,
        caller_env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        match self.get(combiner)? {
            Value::Operative { .. } => {
                let (body, op_env) = self.invoke_operative(combiner, evaled_args, caller_env)?;
                self.eval_expr(body, op_env)
            }
            Value::Builtin(id) => self.apply_builtin_value(id, evaled_args, caller_env),
            Value::Native(f) => self.call_native(f, evaled_args),
            Value::Applicative(inner) => self.apply_combiner(inner, evaled_args, caller_env),
            Value::Prelude(prelude) => {
                let real = self.eval_prelude_source(prelude)?;
                let (body, op_env) = self.invoke_operative(real, evaled_args, caller_env)?;
                self.eval_expr(body, op_env)
            }
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
        let (params, env_param, body, closed_env) = self.vau_parts(func)?;
        let op_env = self.make_child_env(closed_env)?;
        self.match_ptree(params, args, op_env)?;
        if !env_param.is_nil() {
            self.env_define(op_env, env_param, caller_env)?;
        }
        Ok((body, op_env))
    }

    /// Parse a prelude lambda source and evaluate it to get the underlying operative.
    ///
    /// Called on demand each time a `Prelude` function is invoked.
    /// The source is a lambda expression (e.g. `(lambda (x) (+ x 1))`)
    /// which evaluates to an Applicative(Operative). We unwrap to get
    /// the inner Operative for direct invocation.
    fn eval_prelude_source(&self, prelude: crate::prelude::Prelude) -> ArenaResult<ArenaIndex> {
        let mut src = SliceSource::new(prelude.source());
        let lambda_expr = self.parse_expr(&mut src)?;
        let app = self.eval_expr(lambda_expr, ArenaIndex::GLOBAL_ENV)?;
        // lambda returns Applicative(Operative) — unwrap to get the operative
        self.unwrap_applicative(app)
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
    fn match_ptree(&self, ptree: ArenaIndex, obj: ArenaIndex, env: ArenaIndex) -> ArenaResult<()> {
        if ptree.is_nil() {
            if obj.is_nil() {
                return Ok(());
            }
            return Err(ArenaError::TypeError);
        }
        match self.get(ptree)? {
            Value::Ignore => Ok(()),
            Value::Symbol(_) => self.env_define(env, ptree, obj),
            Value::Cons {
                car: ptree_car,
                cdr: ptree_cdr,
            } => {
                let (obj_car, obj_cdr) = self.get(obj)?.as_cons()?;
                self.match_ptree(ptree_car, obj_car, env)?;
                self.match_ptree(ptree_cdr, obj_cdr, env)
            }
            _ => Err(ArenaError::TypeError),
        }
    }

    /// Evaluate all arguments in a list (iterative with in-place reversal).
    fn eval_args(&self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        // Build result in reverse order to avoid O(n) Rust stack depth.
        let mut cur = args;
        let mut reversed = ArenaIndex::NIL;
        while !cur.is_nil() {
            let head_expr = self.car(cur)?;
            self.push_root(reversed)?;
            self.push_root(env)?;
            self.push_root(cur)?;
            let head_val = self.eval_expr(head_expr, env)?;
            self.pop_roots(3);
            reversed = self.cons(head_val, reversed)?;
            cur = self.cdr(cur)?;
        }
        // Reverse in place — all cons cells are freshly allocated by us.
        self.reverse_chain(reversed)
    }

    // ================================================================
    // Operative builtin implementations (receive unevaluated args)
    // ================================================================

    /// `(quote expr)` — return the expression unevaluated.
    fn op_quote(
        &self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        _env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::Return(self.car(args))
    }

    /// `(if test then else)` — test is strict, branches are tail positions.
    fn op_if(&self, args: ArenaIndex, expr: &mut ArenaIndex, env: &mut ArenaIndex) -> TailAction {
        tail_continue!({
            let test_expr = self.car(args)?;
            let rest = self.cdr(args)?;
            let test_val = self.eval_expr(test_expr, *env)?;
            *expr = if self.get(test_val)?.as_bool()? {
                self.car(rest)?
            } else {
                let else_rest = self.cdr(rest)?;
                if else_rest.is_nil() {
                    ArenaIndex::NIL
                } else {
                    self.car(else_rest)?
                }
            };
            Ok(())
        })
    }

    /// `(define! definiend expression)` — Kernel §4.9.1.
    ///
    /// Evaluates `expression` in the dynamic environment and matches `definiend`
    /// (a formal parameter tree) to the result, binding symbols in the dynamic
    /// environment.  Returns `#inert`.
    ///
    /// It is an error to define a variable that already has a binding in the
    /// current environment frame. Use `set!` to update an existing binding.
    ///
    /// User code never receives a direct handle to the ground environment, so
    /// this operative only mutates the current dynamic frame.
    fn op_define(
        &self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail!({
            // Protect the ground environment from mutation.
            debug_assert!(*env != ArenaIndex::GROUND_ENV, "define! in ground env");

            let definiend = self.car(args)?;

            // Regular define! with ptree matching
            self.validate_ptree(definiend)?;
            let val_expr = self.cadr(args)?;
            let val = self.eval_expr(val_expr, *env)?;
            self.match_ptree(definiend, val, *env)?;
            Ok(ArenaIndex::INERT)
        })
    }

    /// `(fn! name params body...)` — define a named function.
    ///
    /// Syntactic sugar for `(define! name (lambda params (begin body...)))`.
    /// `name` is a symbol, `params` is a formal parameter tree, and `body`
    /// is one or more body expressions.
    ///
    /// Returns `#inert`.
    fn op_fn_define(
        &self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail!({
            debug_assert!(*env != ArenaIndex::GROUND_ENV, "fn! in ground env");

            let name = self.car(args)?;
            let rest = self.cdr(args)?;
            let params = self.car(rest)?;
            let body_list = self.cdr(rest)?;
            let body = self.wrap_begin(body_list)?;
            let func = self.lambda(params, body, *env)?;
            self.env_define(*env, name, func)?;
            Ok(ArenaIndex::INERT)
        })
    }

    /// `(set! exp1 formals exp2)` — Kernel §6.8.1.
    ///
    /// Evaluates `exp1` and `exp2` in the dynamic environment; call the
    /// results `env` and `obj`.  If `env` is not an environment, an error
    /// is signaled.  Then the operative updates an existing binding of
    /// `formals` (a single symbol) in environment `env`.  The symbol must
    /// already exist in `env`'s own frame — parents are not walked, and a
    /// new binding is never created.  Returns `#inert`.
    fn op_set(&self, args: ArenaIndex, _expr: &mut ArenaIndex, env: &mut ArenaIndex) -> TailAction {
        non_tail!({
            let env_expr = self.car(args)?;
            let rest = self.cdr(args)?;
            let definiend = self.car(rest)?;
            let val_expr = self.cadr(rest)?;

            let target_env = self.eval_expr(env_expr, *env)?;
            // Validate target is an environment.
            if !matches!(self.get(target_env)?, Value::Environment { .. }) {
                return Err(ArenaError::TypeError);
            }

            debug_assert!(
                target_env != ArenaIndex::GROUND_ENV,
                "define! in ground env"
            );

            let val = self.eval_expr(val_expr, *env)?;
            // set! only supports a single symbol formal.
            if !matches!(self.get(definiend)?, Value::Symbol(_)) {
                return Err(ArenaError::TypeError);
            }
            self.env_set(target_env, definiend, val)?;
            Ok(ArenaIndex::INERT)
        })
    }

    /// `(lambda (params...) body...)`.
    /// Derived: `lambda = wrap(vau(params, #ignore, body))`.
    fn op_lambda(
        &self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail!({
            let params = self.car(args)?;
            let body = self.wrap_begin(self.cdr(args)?)?;
            self.lambda(params, body, *env)
        })
    }

    /// `(begin expr1 expr2 ...)` — all but last are non-tail, last is tail.
    fn op_begin(
        &self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        tail_continue!({
            let mut cur = args;
            while !cur.is_nil() {
                let next = self.cdr(cur)?;
                if next.is_nil() {
                    *expr = self.car(cur)?;
                    return Ok(());
                }
                let e = self.car(cur)?;
                self.eval_expr(e, *env)?;
                cur = next;
            }
            *expr = ArenaIndex::NIL;
            Ok(())
        })
    }

    /// `(cond (test expr...) ...)` — tests are strict, last body expr is tail.
    fn op_cond(&self, args: ArenaIndex, expr: &mut ArenaIndex, env: &mut ArenaIndex) -> TailAction {
        tail_continue!({
            let mut cur = args;
            while !cur.is_nil() {
                let clause = self.car(cur)?;
                let test = self.car(clause)?;
                let body = self.cdr(clause)?;

                let matched = self.symbol_name_eq(test, "else") || {
                    let test_val = self.eval_expr(test, *env)?;
                    self.get(test_val)?.as_bool()?
                };

                if matched {
                    *expr = self.wrap_begin(body)?;
                    return Ok(());
                }
                cur = self.cdr(cur)?;
            }
            *expr = ArenaIndex::NIL;
            Ok(())
        })
    }

    /// `(and expr1 expr2 ...)` — strict on tests, last is tail.
    fn op_and(&self, args: ArenaIndex, expr: &mut ArenaIndex, env: &mut ArenaIndex) -> TailAction {
        self.eval_short_circuit(args, expr, env, true)
    }

    /// `(or expr1 expr2 ...)` — strict on tests, last is tail.
    fn op_or(&self, args: ArenaIndex, expr: &mut ArenaIndex, env: &mut ArenaIndex) -> TailAction {
        self.eval_short_circuit(args, expr, env, false)
    }

    /// Shared `and`/`or` implementation.
    fn eval_short_circuit(
        &self,
        args: ArenaIndex,
        expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
        continue_while_truthy: bool,
    ) -> TailAction {
        tail_continue!({
            if args.is_nil() {
                return Err(ArenaError::InvalidArgument);
            }
            let second = self.cdr(args)?;
            if second.is_nil() {
                return Err(ArenaError::InvalidArgument);
            }

            let mut cur = args;
            while !cur.is_nil() {
                let next = self.cdr(cur)?;
                let e = self.car(cur)?;
                let val = self.eval_expr(e, *env)?;
                let b = self.get(val)?.as_bool()?;
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
    ///
    /// Named let: `(let name ((param init) ...) body...)` desugars to a
    /// recursive function `name` with params bound to evaluated inits.
    fn op_let(&self, args: ArenaIndex, expr: &mut ArenaIndex, env: &mut ArenaIndex) -> TailAction {
        tail_continue!({
            let first = self.car(args)?;

            if matches!(self.get(first)?, Value::Symbol(_)) {
                // Named let
                let name = first;
                let rest = self.cdr(args)?;
                let bindings = self.car(rest)?;
                let body_list = self.cdr(rest)?;

                let params = self.map_list(bindings, Self::car)?;
                let inits = self.map_list(bindings, Self::cadr)?;

                let local_env = self.make_child_env(*env)?;
                self.push_root(local_env)?;

                // Evaluate init values in the OUTER env
                let evaled_inits = self.eval_args(inits, *env)?;
                self.push_root(evaled_inits)?;

                // Create the recursive function in the local env
                let body = self.wrap_begin(body_list)?;
                let func = self.lambda(params, body, local_env)?;
                self.env_define(local_env, name, func)?;

                self.pop_roots(2);

                // Invoke the operative directly with pre-evaluated init values,
                // bypassing the normal eval loop.  The previous approach built
                // `(name val1 val2 …)` and fed it back through `eval_expr`,
                // which would re-evaluate the already-evaluated values.  That
                // is harmless for self-evaluating types (numbers, booleans) but
                // breaks when an init value is a list—`eval` would interpret
                // the list as a function call, producing a NotCallable error.
                let inner = self.unwrap_applicative(func)?;
                let (body_expr, op_env) = self.invoke_operative(inner, evaled_inits, local_env)?;
                *env = op_env;
                *expr = body_expr;
                Ok(())
            } else {
                // Regular let
                let bindings = first;
                let body_list = self.cdr(args)?;

                let local_env = self.make_child_env(*env)?;
                self.push_root(local_env)?;
                let mut cur = bindings;
                while !cur.is_nil() {
                    let binding = self.car(cur)?;
                    let name = self.car(binding)?;
                    let val_expr = self.cadr(binding)?;
                    let val = self.eval_expr(val_expr, *env)?;
                    self.env_define(local_env, name, val)?;
                    cur = self.cdr(cur)?;
                }
                self.pop_roots(1);

                *env = local_env;
                *expr = self.wrap_begin(body_list)?;
                Ok(())
            }
        })
    }

    /// `(vau params env-param body)` — create a fexpr (operative).
    ///
    /// Per the Kernel spec (§4.10.3):
    /// - `params` must be a valid formal parameter tree.
    /// - `env-param` must be either a symbol or `#ignore`.
    /// - If `env-param` is a symbol that also occurs in `params`, an error
    ///   is signaled.
    fn op_vau(&self, args: ArenaIndex, _expr: &mut ArenaIndex, env: &mut ArenaIndex) -> TailAction {
        non_tail!({
            let params = self.car(args)?;
            let env_param = self.cadr(args)?;
            let body_list = self.cdr(self.cdr(args)?)?;
            let body = self.wrap_begin(body_list)?;
            self.vau(params, env_param, body, *env)
        })
    }

    /// `(current-environment)` — return the caller's environment.
    fn op_current_env(
        &self,
        _args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        TailAction::Return(Ok(*env))
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
        let rest = self.cdr(exprs)?;
        if rest.is_nil() {
            return self.car(exprs);
        }
        let begin_sym = self.symbol("begin")?;
        self.cons(begin_sym, exprs)
    }

    /// Map a function over a cons-list, building a new list of results.
    fn map_list(
        &self,
        list: ArenaIndex,
        f: impl Fn(&Self, ArenaIndex) -> ArenaResult<ArenaIndex>,
    ) -> ArenaResult<ArenaIndex> {
        let mut cur = list;
        let mut reversed = ArenaIndex::NIL;
        while !cur.is_nil() {
            let head = self.car(cur)?;
            let val = f(self, head)?;
            reversed = self.cons(val, reversed)?;
            cur = self.cdr(cur)?;
        }
        self.reverse_chain(reversed)
    }

    // ================================================================
    // Applicative builtin implementations (operate on evaluated args)
    // ================================================================

    /// `(cons a b)` — cons cell construction.
    /// When `a` is a single-character string (CharPair with cdr=NIL),
    /// produces a CharPair node instead, so `(cons (car "h") "ello")` → `"hello"`.
    fn builtin_cons(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let a = self.car_char(args)?;
        let b = self.cadr_char(args)?;
        if let Value::CharPair { ch, cdr } = self.get(a)?
            && cdr.is_nil()
        {
            return self.arena.alloc(Value::CharPair { ch, cdr: b });
        }
        self.cons(a, b)
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
        let first = self.get(self.car(args)?)?.as_number()?;
        let rest = self.cdr(args)?;
        if rest.is_nil() {
            return self.number(first.checked_neg().ok_or(ArenaError::ArithmeticOverflow)?);
        }
        fold_numbers!(self, rest, first, checked_sub)
    }

    /// `(* ...)` — variadic multiplication.
    fn builtin_mul(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        fold_numbers!(self, args, 1, checked_mul)
    }

    /// `(/ a b)` — integer division.
    fn builtin_div(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let a = self.get(self.car(args)?)?.as_number()?;
        let b = self.get(self.cadr(args)?)?.as_number()?;
        if b == 0 {
            return Err(ArenaError::DivisionByZero);
        }
        self.number(a / b)
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

    pair_builtin!(builtin_car, car_char);
    pair_builtin!(builtin_cdr, cdr_char);

    // — Type predicate built-ins —

    type_predicate!(builtin_nullp, Value::Nil);
    type_predicate!(builtin_pairp, Value::Cons { .. } | Value::CharPair { .. });
    type_predicate!(builtin_numberp, Value::Number(_));
    type_predicate!(builtin_symbolp, Value::Symbol(_));
    type_predicate!(builtin_booleanp, Value::Boolean(_));
    type_predicate!(builtin_inertp, Value::Inert);

    /// `(not boolean)` — boolean negation (requires exactly one boolean arg).
    fn builtin_not(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.car(args)?;
        let b = self.get(val)?.as_bool()?;
        Ok(ArenaIndex::from_bool(!b))
    }

    /// `(eq? object1 object2)` — identity predicate (§4.2.1).
    ///
    /// Returns `#t` iff the two objects are effectively the same object.
    /// Identity is index-based first; if the indices differ, the helper falls
    /// back to `Value::is_immutable()` to decide whether direct value
    /// comparison is permitted.
    fn builtin_eqp(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let a = self.car(args)?;
        let b = self.cadr(args)?;
        Ok(ArenaIndex::from_bool(self.is_eq(a, b)?))
    }

    /// `(equal? object1 object2)` — structural equality predicate (§4.3.1).
    ///
    /// Returns `#t` iff the two objects "look" the same as long as nothing
    /// is mutated. Weaker than eq?; equal? returns true whenever eq? would.
    fn builtin_equalp(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let a = self.car(args)?;
        let b = self.cadr(args)?;
        Ok(ArenaIndex::from_bool(self.is_equal(a, b)?))
    }

    /// Core `eq?` logic: identity comparison.
    fn is_eq(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<bool> {
        // Same arena slot ⇒ same object.
        if a == b {
            return Ok(true);
        }
        let (va, vb) = (self.get(a)?, self.get(b)?);
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
        let va = self.get(a)?;
        let vb = self.get(b)?;
        match (va, vb) {
            // Pairs: structural comparison of car and cdr.
            (Value::Cons { car: a1, cdr: a2 }, Value::Cons { car: b1, cdr: b2 }) => {
                Ok(self.is_equal(a1, b1)? && self.is_equal(a2, b2)?)
            }
            // Strings: compare character-by-character.
            (Value::CharPair { .. }, Value::CharPair { .. }) => self.strings_equal(a, b),
            // Environments: eq? only (identity-based).
            // Different environments are never equal? unless eq?.
            (Value::Environment { .. }, Value::Environment { .. }) => Ok(false),
            // Different types or non-matching values.
            _ => Ok(false),
        }
    }

    // — Kernel combiners —

    /// `(eval expr env)` — evaluate expression in given environment.
    fn builtin_eval(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let expr_val = self.car(args)?;
        let rest = self.cdr(args)?;
        let env_val = if rest.is_nil() {
            ArenaIndex::GLOBAL_ENV
        } else {
            self.car(rest)?
        };
        self.eval_expr(expr_val, env_val)
    }

    /// `(wrap combiner)` — wrap a combiner into an applicative.
    fn builtin_wrap(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let combiner = self.car(args)?;
        self.wrap(combiner)
    }

    /// `(unwrap applicative)` — extract the underlying combiner.
    fn builtin_unwrap(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let app = self.car(args)?;
        self.unwrap_applicative(app)
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
            let v = self.car(cur)?;
            if !matches!(self.get(v)?, Value::Environment { .. }) {
                return Err(ArenaError::TypeError);
            }
            cur = self.cdr(cur)?;
        }
        // Copy the parents list so it's independent of the original.
        let parents = self.copy_list(args)?;
        self.make_env(parents)
    }

    /// `(make-empty-environment)` — always creates a parentless environment.
    fn builtin_make_empty_env(&self, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.make_env(ArenaIndex::NIL)
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
        let stats = self.eval_collect_garbage_unconditional();
        self.number(stats.collected as isize)
    }

    /// Copy a cons-list into fresh cons cells (iterative).
    fn copy_list(&self, list: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.map_list(list, |_, h| Ok(h))
    }

    /// `(raw-read-string str)` — parse one s-expression from a string.
    /// Returns the parsed value or NIL if the string is empty.
    fn builtin_raw_read_string(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let str_idx = self.car(args)?;
        if str_idx.is_nil() {
            return Ok(ArenaIndex::NIL);
        }
        let mut src = crate::parse::ChainSource::new(&self.arena, str_idx);
        self.parse_expr(&mut src)
    }

    /// Format a value to an arena CharPair chain. Shared by
    /// `raw-display-to-string` and `raw-write-to-string`.
    fn fmt_to_string(&self, args: ArenaIndex, display: bool) -> ArenaResult<ArenaIndex> {
        let val = self.car(args)?;
        let mut w = ArenaWriter::new(self);
        let _ = self.fmt_value(val, &mut w, display);
        w.finish()
    }

    /// `(raw-display-to-string obj)` — display a value to a string (CharPair chain).
    fn builtin_raw_display_to_string(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.fmt_to_string(args, true)
    }

    /// `(raw-write-to-string obj)` — write a value to a string (CharPair chain).
    fn builtin_raw_write_to_string(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.fmt_to_string(args, false)
    }

    /// `(error msg)` — currently signal `InvalidArgument` unconditionally.
    ///
    /// The argument is accepted for future compatibility but is not yet
    /// threaded into a richer error payload.
    fn builtin_error(&self, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        Err(ArenaError::InvalidArgument)
    }

    /// `(apply combiner args)` — apply a combiner to a list of arguments.
    fn builtin_apply(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let combiner = self.car(args)?;
        let rest1 = self.cdr(args)?;
        let arg_list = self.car(rest1)?;
        let rest2 = self.cdr(rest1)?;
        let env = if rest2.is_nil() {
            ArenaIndex::GLOBAL_ENV
        } else {
            self.car(rest2)?
        };
        self.apply_combiner(combiner, arg_list, env)
    }
}
