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

use crate::io::IoProvider;
use crate::lisp::Lisp;
use crate::value::{BuiltinId, Value};

/// Maximum size of a file path extracted from a CharPair chain (bytes).
const MAX_PATH_LEN: usize = 256;

/// Maximum size of a string extracted from a CharPair chain for
/// `raw-read-string` / `raw-write-file` (bytes).
const MAX_STRING_BUF_LEN: usize = 4096;

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

        impl<const N: usize, IO: IoProvider> Lisp<N, IO> {
            /// Register all builtins in the ground environment.
            pub(crate) fn init_builtins(&self) {
                $( self.bind_builtin($op_name, $op_id, false); )*
                $( self.bind_builtin($bi_name, $bi_id, true); )*
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
        "set!"   => op_set    => op_set,
        "lambda" => op_lambda => op_lambda,
        "begin"  => op_begin  => op_begin,
        "cond"   => op_cond   => op_cond,
        "and"    => op_and    => op_and,
        "or"     => op_or     => op_or,
        "let"    => op_let    => op_let,
        "vau"    => op_vau    => op_vau,
        "current-environment" => op_current_env => op_current_env,
        "load"   => op_load   => op_load,
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
        "raw-display"  => bi_raw_display  => builtin_raw_display,
        "raw-write"    => bi_raw_write    => builtin_raw_write,
        "raw-write-stdout"  => bi_raw_write_stdout  => builtin_raw_write_stdout,
        "raw-write-stderr"  => bi_raw_write_stderr  => builtin_raw_write_stderr,
        "raw-read"          => bi_raw_read          => builtin_raw_read,
        "raw-read-char"     => bi_raw_read_char     => builtin_raw_read_char,
        "raw-peek-char"     => bi_raw_peek_char     => builtin_raw_peek_char,
        "raw-read-file"     => bi_raw_read_file     => builtin_raw_read_file,
        "raw-write-file"    => bi_raw_write_file    => builtin_raw_write_file,
        "raw-file-exists?"  => bi_raw_file_existsp  => builtin_raw_file_existsp,
        "raw-delete-file"   => bi_raw_delete_file   => builtin_raw_delete_file,
        "raw-read-string"   => bi_raw_read_string   => builtin_raw_read_string,
        "raw-display-to-string" => bi_raw_display_to_string => builtin_raw_display_to_string,
        "raw-write-to-string"   => bi_raw_write_to_string   => builtin_raw_write_to_string,

    }
}

impl<const N: usize, IO: IoProvider> Lisp<N, IO> {
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

    /// Trigger garbage collection unconditionally (ignores gc_enabled flag).
    /// Used by the `gc-collect` builtin for explicit manual collection.
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
                            Value::Builtin(id) => {
                                Ok(Some(self.apply_builtin_pure(id, evaled_args)?))
                            }
                            Value::Applicative(_) => {
                                Ok(Some(self.apply_combiner(inner, evaled_args, *env)?))
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

    /// Apply a combiner to a list of already-evaluated arguments.
    ///
    /// Used for double-wrapped applicatives and internal dispatch.
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
        let (params, env_param, body, closed_env) = self.vau_parts(func)?;
        let op_env = self.make_child_env(closed_env)?;
        self.match_ptree(params, args, op_env)?;
        if !env_param.is_nil() {
            self.env_define(op_env, env_param, caller_env)?;
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
        self.reverse_list(reversed)
    }

    /// Reverse a singly-linked cons-list in place by swapping cdr pointers.
    #[inline]
    fn reverse_list(&self, mut list: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let mut prev = ArenaIndex::NIL;
        while !list.is_nil() {
            let Value::Cons { car, cdr } = self.get(list)? else {
                return Err(ArenaError::TypeError);
            };
            self.arena.set(list, Value::Cons { car, cdr: prev })?;
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

    /// `($define! definiend expression)` — Kernel §4.9.1.
    ///
    /// Evaluates `expression` in the dynamic environment and matches `definiend`
    /// (a formal parameter tree) to the result, binding symbols in the dynamic
    /// environment.  Returns `#inert`.
    ///
    /// Function shorthand: `(define! (fn name params...) body...)` desugars to
    /// `(define! name (lambda (params...) body...))`.  The `fn` marker is a
    /// syntactic keyword recognized only in this position — it is never
    /// evaluated or looked up as a variable binding.
    ///
    /// Per Kernel §3.2, mutation of the ground environment or its ancestors
    /// is forbidden.
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

            // Function shorthand: (define! (fn name params...) body...)
            if let Value::Cons { car, cdr } = self.get(definiend)? {
                if self.symbol_name_eq(car, "fn") {
                    let name = self.car(cdr)?;
                    let params = self.cdr(cdr)?;
                    let body_list = self.cdr(args)?;
                    let body = self.wrap_begin(body_list)?;
                    let func = self.lambda(params, body, *env)?;
                    self.env_define(*env, name, func)?;
                    return Ok(ArenaIndex::INERT);
                }
            }

            // Regular define! with ptree matching
            self.validate_ptree(definiend)?;
            let val_expr = self.cadr(args)?;
            let val = self.eval_expr(val_expr, *env)?;
            self.match_ptree(definiend, val, *env)?;
            Ok(ArenaIndex::INERT)
        })
    }

    /// `($set! exp1 formals exp2)` — Kernel §6.8.1.
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

            // Validate formals parameter tree and collect seen symbols.
            let seen_syms = self.validate_ptree(params)?;

            // Validate env-param: must be a symbol or #ignore.
            let ep = match self.get(env_param)? {
                Value::Ignore => ArenaIndex::NIL,
                Value::Symbol(_) => {
                    // env-param symbol must not also occur in formals.
                    if self.list_contains(seen_syms, env_param) {
                        return Err(ArenaError::InvalidArgument);
                    }
                    env_param
                }
                _ => return Err(ArenaError::TypeError),
            };

            self.vau(params, ep, body, *env)
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
        match self.get(ptree)? {
            Value::Ignore => Ok(seen_syms),
            Value::Symbol(_) => {
                if self.list_contains(seen_syms, ptree) {
                    return Err(ArenaError::InvalidArgument);
                }
                self.cons(ptree, seen_syms)
            }
            Value::Cons {
                car: ptree_car,
                cdr: ptree_cdr,
            } => {
                // Cycle detection: this pair must not have been visited.
                if self.list_contains(visited, ptree) {
                    return Err(ArenaError::Cyclic);
                }
                let new_visited = self.cons(ptree, visited)?;
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
        self.reverse_list(reversed)
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
        if let Value::CharPair { ch, cdr } = self.get(a)? {
            if cdr.is_nil() {
                return self.arena.alloc(Value::CharPair { ch, cdr: b });
            }
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
    /// For immutable, encapsulated types (booleans, nil, inert, symbols),
    /// eq? is determined by value. For mutable/constructed objects (pairs,
    /// environments), eq? compares arena identity.
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

    /// `(wrap combiner)` — wrap an operative into an applicative.
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

    // ================================================================
    // I/O builtins (portless model)
    // ================================================================

    /// `(raw-display obj)` — write human-readable output to stdout.
    fn builtin_raw_display(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.car(args)?;
        {
            let mut w = crate::io::StdoutWriter {
                io: &self.io,
                error: None,
            };
            let _ = self.display_value(val, &mut w);
        }
        Ok(val)
    }

    /// `(raw-write obj)` — write machine-readable output to stdout.
    fn builtin_raw_write(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.car(args)?;
        {
            let mut w = crate::io::StdoutWriter {
                io: &self.io,
                error: None,
            };
            let _ = self.write_value(val, &mut w);
        }
        Ok(val)
    }

    /// `(raw-write-stdout str)` — write a CharPair chain to stdout.
    fn builtin_raw_write_stdout(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let str_idx = self.car(args)?;
        self.write_chars_to_stdout(str_idx)?;
        Ok(ArenaIndex::INERT)
    }

    /// `(raw-write-stderr str)` — write a CharPair chain to stderr.
    fn builtin_raw_write_stderr(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let str_idx = self.car(args)?;
        self.write_chars_to_stderr(str_idx)?;
        Ok(ArenaIndex::INERT)
    }

    /// Walk a CharPair chain and write each character to stdout via IoProvider.
    fn write_chars_to_stdout(&self, mut idx: ArenaIndex) -> ArenaResult<()> {
        while !idx.is_nil() {
            let Value::CharPair { ch, cdr } = self.get(idx)? else {
                return Err(ArenaError::TypeError);
            };
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            let _ = self.io.borrow_mut().write_stdout(s);
            idx = cdr;
        }
        Ok(())
    }

    /// Walk a CharPair chain and write each character to stderr via IoProvider.
    fn write_chars_to_stderr(&self, mut idx: ArenaIndex) -> ArenaResult<()> {
        while !idx.is_nil() {
            let Value::CharPair { ch, cdr } = self.get(idx)? else {
                return Err(ArenaError::TypeError);
            };
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            let _ = self.io.borrow_mut().write_stderr(s);
            idx = cdr;
        }
        Ok(())
    }

    /// `(raw-read)` — read one s-expression from stdin.
    /// Returns NIL at end-of-input.
    fn builtin_raw_read(&self, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        self.read_from_stdin()
    }

    /// `(raw-read-char)` — read one character from stdin.
    /// Returns a single CharPair or NIL at EOF.
    fn builtin_raw_read_char(&self, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.io.borrow_mut().read_stdin_char() {
            Ok(c) => self.char_val(c),
            Err(crate::io::IoErrorKind::Eof) => Ok(ArenaIndex::NIL),
            Err(_) => Err(ArenaError::IoError),
        }
    }

    /// `(raw-peek-char)` — peek at next character on stdin.
    /// Returns a single CharPair or NIL at EOF.
    fn builtin_raw_peek_char(&self, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.io.borrow_mut().peek_stdin_char() {
            Ok(c) => self.char_val(c),
            Err(crate::io::IoErrorKind::Eof) => Ok(ArenaIndex::NIL),
            Err(_) => Err(ArenaError::IoError),
        }
    }

    /// `(raw-read-file path)` — read entire file into a CharPair chain.
    fn builtin_raw_read_file(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let path_idx = self.car(args)?;
        let mut buf = [0u8; MAX_PATH_LEN];
        let len = self.collect_string(path_idx, &mut buf)?;
        let path = core::str::from_utf8(&buf[..len]).map_err(|_| ArenaError::InvalidArgument)?;
        let mut io = self.io.borrow_mut();
        // SAFETY: We need the borrow to last long enough to get the &str,
        // then we allocate from it. This is fine because alloc_string
        // doesn't touch io.
        let contents = io.read_file(path).map_err(|_| ArenaError::IoError)?;
        self.alloc_string(contents)
    }

    /// `(raw-write-file path str)` — write a string to a file.
    fn builtin_raw_write_file(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let path_idx = self.car(args)?;
        let rest = self.cdr(args)?;
        let str_idx = self.car(rest)?;
        let mut path_buf = [0u8; MAX_PATH_LEN];
        let path_len = self.collect_string(path_idx, &mut path_buf)?;
        let path = core::str::from_utf8(&path_buf[..path_len]).map_err(|_| ArenaError::InvalidArgument)?;
        let mut str_buf = [0u8; MAX_STRING_BUF_LEN];
        let str_len = self.collect_string(str_idx, &mut str_buf)?;
        let content = core::str::from_utf8(&str_buf[..str_len]).map_err(|_| ArenaError::InvalidArgument)?;
        self.io.borrow_mut().write_file(path, content).map_err(|_| ArenaError::IoError)?;
        Ok(ArenaIndex::INERT)
    }

    /// `(raw-file-exists? path)` — check if a file exists.
    fn builtin_raw_file_existsp(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let path_idx = self.car(args)?;
        let mut buf = [0u8; MAX_PATH_LEN];
        let len = self.collect_string(path_idx, &mut buf)?;
        let path = core::str::from_utf8(&buf[..len]).map_err(|_| ArenaError::InvalidArgument)?;
        match self.io.borrow().file_exists(path) {
            Ok(exists) => Ok(ArenaIndex::from_bool(exists)),
            Err(_) => Err(ArenaError::IoError),
        }
    }

    /// `(raw-delete-file path)` — delete a file.
    fn builtin_raw_delete_file(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let path_idx = self.car(args)?;
        let mut buf = [0u8; MAX_PATH_LEN];
        let len = self.collect_string(path_idx, &mut buf)?;
        let path = core::str::from_utf8(&buf[..len]).map_err(|_| ArenaError::InvalidArgument)?;
        self.io.borrow_mut().delete_file(path).map_err(|_| ArenaError::IoError)?;
        Ok(ArenaIndex::INERT)
    }

    /// `(raw-read-string str)` — parse one s-expression from a string.
    /// Returns the parsed value or NIL if the string is empty.
    fn builtin_raw_read_string(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let str_idx = self.car(args)?;
        let mut buf = [0u8; MAX_STRING_BUF_LEN];
        let len = self.collect_string(str_idx, &mut buf)?;
        if len == 0 {
            return Ok(ArenaIndex::NIL);
        }
        let s = core::str::from_utf8(&buf[..len]).map_err(|_| ArenaError::InvalidArgument)?;
        let mut parser = crate::parse::Parser::new(s);
        if parser.has_more() {
            parser.parse(self)
        } else {
            Ok(ArenaIndex::NIL)
        }
    }

    /// `(raw-display-to-string obj)` — display a value to a string (CharPair chain).
    /// Uses display_value (no quotes on strings).
    fn builtin_raw_display_to_string(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.car(args)?;
        let mut buf = [0u8; MAX_STRING_BUF_LEN];
        let mut pos = 0;
        {
            struct BufWriter<'a> {
                buf: &'a mut [u8],
                pos: &'a mut usize,
            }
            impl core::fmt::Write for BufWriter<'_> {
                fn write_str(&mut self, s: &str) -> core::fmt::Result {
                    let bytes = s.as_bytes();
                    if *self.pos + bytes.len() > self.buf.len() {
                        return Err(core::fmt::Error);
                    }
                    self.buf[*self.pos..*self.pos + bytes.len()].copy_from_slice(bytes);
                    *self.pos += bytes.len();
                    Ok(())
                }
            }
            let mut w = BufWriter { buf: &mut buf, pos: &mut pos };
            let _ = self.display_value(val, &mut w);
        }
        let s = core::str::from_utf8(&buf[..pos]).map_err(|_| ArenaError::InvalidArgument)?;
        self.alloc_string(s)
    }

    /// `(raw-write-to-string obj)` — write a value to a string (CharPair chain).
    /// Uses write_value (machine-readable, with quotes on strings).
    fn builtin_raw_write_to_string(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.car(args)?;
        let mut buf = [0u8; MAX_STRING_BUF_LEN];
        let mut pos = 0;
        {
            struct BufWriter<'a> {
                buf: &'a mut [u8],
                pos: &'a mut usize,
            }
            impl core::fmt::Write for BufWriter<'_> {
                fn write_str(&mut self, s: &str) -> core::fmt::Result {
                    let bytes = s.as_bytes();
                    if *self.pos + bytes.len() > self.buf.len() {
                        return Err(core::fmt::Error);
                    }
                    self.buf[*self.pos..*self.pos + bytes.len()].copy_from_slice(bytes);
                    *self.pos += bytes.len();
                    Ok(())
                }
            }
            let mut w = BufWriter { buf: &mut buf, pos: &mut pos };
            let _ = self.write_value(val, &mut w);
        }
        let s = core::str::from_utf8(&buf[..pos]).map_err(|_| ArenaError::InvalidArgument)?;
        self.alloc_string(s)
    }

    /// `(error msg)` — signal an error.
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

    // ================================================================
    // Stdin reader (portless)
    // ================================================================

    /// Read one s-expression from stdin using the IoProvider's `read_stdin_char`.
    ///
    /// Returns the parsed arena value or NIL at EOF.
    fn read_from_stdin(&self) -> ArenaResult<ArenaIndex> {
        // Skip whitespace and comments
        loop {
            let ch = match self.io.borrow_mut().read_stdin_char() {
                Ok(c) => c,
                Err(crate::io::IoErrorKind::Eof) => return Ok(ArenaIndex::NIL),
                Err(_) => return Err(ArenaError::IoError),
            };
            match ch {
                ' ' | '\t' | '\n' | '\r' => continue,
                ';' => {
                    // Skip to end of line
                    loop {
                        match self.io.borrow_mut().read_stdin_char() {
                            Ok('\n') | Err(crate::io::IoErrorKind::Eof) => break,
                            Ok(_) => continue,
                            Err(_) => return Err(ArenaError::IoError),
                        }
                    }
                }
                _ => {
                    let mut buf = [0u8; MAX_STRING_BUF_LEN];
                    let pos = ch.len_utf8();
                    ch.encode_utf8(&mut buf[..]);
                    return self.read_expr_from_stdin(&mut buf, pos);
                }
            }
        }
    }

    /// Finish reading an s-expression from stdin after the first character has been consumed.
    fn read_expr_from_stdin(
        &self,
        buf: &mut [u8; MAX_STRING_BUF_LEN],
        initial_pos: usize,
    ) -> ArenaResult<ArenaIndex> {
        let mut pos = initial_pos;
        let first = buf[0];

        match first {
            b'\'' => {
                let inner = self.read_from_stdin()?;
                let quote_sym = self.symbol("quote")?;
                let inner_list = self.cons(inner, ArenaIndex::NIL)?;
                self.cons(quote_sym, inner_list)
            }
            b'"' => {
                // String literal: read until closing quote, processing escapes.
                pos = 0;
                loop {
                    let ch = match self.io.borrow_mut().read_stdin_char() {
                        Ok(c) => c,
                        Err(_) => return Err(ArenaError::ParseError),
                    };
                    if ch == '"' {
                        break;
                    }
                    let actual = if ch == '\\' {
                        let esc = match self.io.borrow_mut().read_stdin_char() {
                            Ok(c) => c,
                            Err(_) => return Err(ArenaError::ParseError),
                        };
                        match esc {
                            'n' => '\n',
                            't' => '\t',
                            'r' => '\r',
                            '\\' => '\\',
                            '"' => '"',
                            _ => return Err(ArenaError::InvalidArgument),
                        }
                    } else {
                        ch
                    };
                    let len = actual.len_utf8();
                    if pos + len > buf.len() {
                        return Err(ArenaError::InvalidArgument);
                    }
                    actual.encode_utf8(&mut buf[pos..]);
                    pos += len;
                }
                let s = core::str::from_utf8(&buf[..pos]).map_err(|_| ArenaError::ParseError)?;
                self.alloc_string(s)
            }
            b'(' => {
                self.read_list_from_stdin()
            }
            b')' => Err(ArenaError::ParseError),
            _ => {
                // Atom: read until delimiter
                loop {
                    let ch = match self.io.borrow_mut().peek_stdin_char() {
                        Ok(c) => c,
                        Err(crate::io::IoErrorKind::Eof) => break,
                        Err(_) => break,
                    };
                    match ch {
                        ' ' | '\t' | '\n' | '\r' | '(' | ')' | '"' | ';' => break,
                        _ => {
                            let _ = self.io.borrow_mut().read_stdin_char();
                            let len = ch.len_utf8();
                            if pos + len > buf.len() {
                                return Err(ArenaError::InvalidArgument);
                            }
                            ch.encode_utf8(&mut buf[pos..]);
                            pos += len;
                        }
                    }
                }
                let s = core::str::from_utf8(&buf[..pos]).map_err(|_| ArenaError::ParseError)?;
                match s {
                    "#t" | "#true" => Ok(ArenaIndex::TRUE),
                    "#f" | "#false" => Ok(ArenaIndex::FALSE),
                    "#inert" => Ok(ArenaIndex::INERT),
                    "#ignore" => Ok(ArenaIndex::IGNORE),
                    _ => {
                        if let Some(n) = crate::parse::parse_integer(s) {
                            self.number(n)
                        } else {
                            self.symbol(s)
                        }
                    }
                }
            }
        }
    }

    /// Read a list from stdin: elements until ')'.
    fn read_list_from_stdin(&self) -> ArenaResult<ArenaIndex> {
        // Skip whitespace
        loop {
            let ch = match self.io.borrow_mut().peek_stdin_char() {
                Ok(c) => c,
                Err(crate::io::IoErrorKind::Eof) => return Err(ArenaError::ParseError),
                Err(_) => return Err(ArenaError::IoError),
            };
            match ch {
                ' ' | '\t' | '\n' | '\r' => { let _ = self.io.borrow_mut().read_stdin_char(); }
                ';' => {
                    let _ = self.io.borrow_mut().read_stdin_char();
                    loop {
                        match self.io.borrow_mut().read_stdin_char() {
                            Ok('\n') | Err(crate::io::IoErrorKind::Eof) => break,
                            Ok(_) => continue,
                            Err(_) => return Err(ArenaError::IoError),
                        }
                    }
                }
                ')' => {
                    let _ = self.io.borrow_mut().read_stdin_char();
                    return Ok(ArenaIndex::NIL);
                }
                '.' => {
                    let _ = self.io.borrow_mut().read_stdin_char();
                    let next = match self.io.borrow_mut().peek_stdin_char() {
                        Ok(c) => c,
                        Err(_) => return Err(ArenaError::ParseError),
                    };
                    if matches!(next, ' ' | '\t' | '\n' | '\r' | ')') {
                        // Dotted pair
                        let cdr = self.read_from_stdin()?;
                        loop {
                            let c = match self.io.borrow_mut().peek_stdin_char() {
                                Ok(c) => c,
                                Err(_) => return Err(ArenaError::ParseError),
                            };
                            match c {
                                ' ' | '\t' | '\n' | '\r' => { let _ = self.io.borrow_mut().read_stdin_char(); }
                                ')' => { let _ = self.io.borrow_mut().read_stdin_char(); return Ok(cdr); }
                                _ => return Err(ArenaError::ParseError),
                            }
                        }
                    } else {
                        // Symbol starting with '.'
                        let mut buf = [0u8; MAX_STRING_BUF_LEN];
                        buf[0] = b'.';
                        let expr = self.read_expr_from_stdin(&mut buf, 1)?;
                        let rest = self.read_list_from_stdin()?;
                        return self.cons(expr, rest);
                    }
                }
                _ => break,
            }
        }
        let car = self.read_from_stdin()?;
        let cdr = self.read_list_from_stdin()?;
        self.cons(car, cdr)
    }

    // ================================================================
    // Load operative
    // ================================================================

    /// `(load path)` — load and evaluate a file in the caller's environment.
    ///
    /// Evaluates its argument to get a path string, reads the file via
    /// `IoProvider::read_file`, parses all expressions, and evaluates
    /// each in the caller's environment.
    fn op_load(
        &self,
        args: ArenaIndex,
        _expr: &mut ArenaIndex,
        env: &mut ArenaIndex,
    ) -> TailAction {
        non_tail!({
            let path_expr = self.car(args)?;
            let path_val = self.eval_expr(path_expr, *env)?;
            self.load_file(path_val, *env)
        })
    }

    /// Load and evaluate expressions from a file path (CharPair chain).
    fn load_file(
        &self,
        path_val: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let mut buf = [0u8; MAX_PATH_LEN];
        let len = self.collect_string(path_val, &mut buf)?;
        let path = core::str::from_utf8(&buf[..len]).map_err(|_| ArenaError::InvalidArgument)?;

        // Read the file via IoProvider.
        // We need to hold the borrow to access the string, parse all expressions
        // into arena cells, then drop the borrow and evaluate.
        let parsed_exprs = {
            let mut io = self.io.borrow_mut();
            let contents = io.read_file(path).map_err(|_| ArenaError::IoError)?;
            let mut parser = crate::parse::Parser::new(contents);
            let mut list = ArenaIndex::NIL;
            while parser.has_more() {
                let e = parser.parse(self)?;
                list = self.cons(e, list)?;
            }
            // Reverse so expressions are in order
            self.reverse_list(list)?
        };

        // Now evaluate each expression in the caller's env
        let mut result = ArenaIndex::INERT;
        let mut cur = parsed_exprs;
        while !cur.is_nil() {
            let e = self.car(cur)?;
            result = self.eval_expr(e, env)?;
            cur = self.cdr(cur)?;
        }
        Ok(result)
    }
}
