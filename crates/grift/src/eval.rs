//! Lisp evaluator.
//!
//! Evaluates arena-allocated S-expressions in an environment.

use grift_arena::{ArenaIndex, ArenaError, ArenaResult};

use crate::lisp::Lisp;
use crate::value::Value;

// Built-in function IDs
const BUILTIN_ADD: u8 = 0;
const BUILTIN_SUB: u8 = 1;
const BUILTIN_MUL: u8 = 2;
const BUILTIN_DIV: u8 = 3;
const BUILTIN_EQ: u8 = 4;
const BUILTIN_LT: u8 = 5;
const BUILTIN_GT: u8 = 6;
const BUILTIN_CONS: u8 = 7;
const BUILTIN_CAR: u8 = 8;
const BUILTIN_CDR: u8 = 9;
const BUILTIN_LIST: u8 = 10;
const BUILTIN_NULLP: u8 = 11;
const BUILTIN_NOT: u8 = 12;
const BUILTIN_PAIRP: u8 = 13;
const BUILTIN_NUMBERP: u8 = 14;
const BUILTIN_SYMBOLP: u8 = 15;
const BUILTIN_BOOLEANP: u8 = 16;
const BUILTIN_LE: u8 = 17;
const BUILTIN_GE: u8 = 18;

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

    /// Register all built-in functions in the global environment.
    fn init_builtins(&mut self) {
        let builtins: &[(&str, u8)] = &[
            ("+", BUILTIN_ADD),
            ("-", BUILTIN_SUB),
            ("*", BUILTIN_MUL),
            ("/", BUILTIN_DIV),
            ("=", BUILTIN_EQ),
            ("<", BUILTIN_LT),
            (">", BUILTIN_GT),
            ("<=", BUILTIN_LE),
            (">=", BUILTIN_GE),
            ("cons", BUILTIN_CONS),
            ("car", BUILTIN_CAR),
            ("cdr", BUILTIN_CDR),
            ("list", BUILTIN_LIST),
            ("null?", BUILTIN_NULLP),
            ("not", BUILTIN_NOT),
            ("pair?", BUILTIN_PAIRP),
            ("number?", BUILTIN_NUMBERP),
            ("symbol?", BUILTIN_SYMBOLP),
            ("boolean?", BUILTIN_BOOLEANP),
        ];

        for &(name, id) in builtins {
            if let (Ok(sym), Ok(val)) = (
                self.lisp.symbol(name),
                self.lisp.arena.alloc(Value::Builtin(id)),
            ) {
                if let Ok(new_env) = env_bind(self.lisp, self.global_env, sym, val) {
                    self.global_env = new_env;
                }
            }
        }
    }

    /// Evaluate an expression in an environment.
    pub fn eval(&mut self, expr: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let val = self.lisp.get(expr)?;

        match val {
            // Self-evaluating values
            Value::Nil | Value::True | Value::False | Value::Number(_)
            | Value::Char(_) | Value::String { .. }
            | Value::Builtin(_) | Value::Lambda { .. } => Ok(expr),

            // Symbol → look up in environment
            Value::Symbol(_) => env_lookup(self.lisp, env, expr),

            // List → special form or function application
            Value::Cons { car, cdr } => {
                let head = self.lisp.get(car)?;

                // Check for special forms
                if let Value::Symbol(_) = head {
                    if self.lisp.symbol_name_eq(car, "quote") {
                        return self.lisp.car(cdr);
                    }
                    if self.lisp.symbol_name_eq(car, "if") {
                        return self.eval_if(cdr, env);
                    }
                    if self.lisp.symbol_name_eq(car, "define") {
                        return self.eval_define(cdr, env);
                    }
                    if self.lisp.symbol_name_eq(car, "set!") {
                        return self.eval_set(cdr, env);
                    }
                    if self.lisp.symbol_name_eq(car, "lambda") {
                        return self.eval_lambda(cdr, env);
                    }
                    if self.lisp.symbol_name_eq(car, "begin") {
                        return self.eval_begin(cdr, env);
                    }
                    if self.lisp.symbol_name_eq(car, "cond") {
                        return self.eval_cond(cdr, env);
                    }
                    if self.lisp.symbol_name_eq(car, "and") {
                        return self.eval_and(cdr, env);
                    }
                    if self.lisp.symbol_name_eq(car, "or") {
                        return self.eval_or(cdr, env);
                    }
                    if self.lisp.symbol_name_eq(car, "let") {
                        return self.eval_let(cdr, env);
                    }
                }

                // Function application
                let func = self.eval(car, env)?;
                let args = self.eval_list(cdr, env)?;
                self.apply(func, args)
            }
        }
    }

    /// Evaluate `(if test then else)`.
    fn eval_if(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let test = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        let then_expr = self.lisp.car(rest)?;

        let test_val = self.eval(test, env)?;
        let is_false = matches!(self.lisp.get(test_val)?, Value::False);

        if !is_false {
            self.eval(then_expr, env)
        } else {
            let else_rest = self.lisp.cdr(rest)?;
            if else_rest.is_nil() {
                self.lisp.nil()
            } else {
                let else_expr = self.lisp.car(else_rest)?;
                self.eval(else_expr, env)
            }
        }
    }

    /// Evaluate `(define name expr)` or `(define (name params...) body)`.
    fn eval_define(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let first = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;

        match self.lisp.get(first)? {
            Value::Symbol(_) => {
                // (define name expr)
                let expr = self.lisp.car(rest)?;
                let val = self.eval(expr, env)?;
                self.global_env = env_bind(self.lisp, self.global_env, first, val)?;
                Ok(val)
            }
            Value::Cons { car: name, cdr: params } => {
                // (define (name params...) body...)
                let body = self.wrap_begin(rest)?;
                let lam = self.lisp.lambda(params, body, env)?;
                self.global_env = env_bind(self.lisp, self.global_env, name, lam)?;
                Ok(lam)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    /// Evaluate `(set! name expr)`.
    fn eval_set(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let name = self.lisp.car(args)?;
        let expr = self.lisp.car(self.lisp.cdr(args)?)?;
        let val = self.eval(expr, env)?;

        // Try local env first, then global
        if env_set(self.lisp, env, name, val).is_ok() {
            return self.lisp.nil();
        }
        env_set(self.lisp, self.global_env, name, val)?;
        self.lisp.nil()
    }

    /// Evaluate `(lambda (params...) body...)`.
    fn eval_lambda(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let params = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;
        let body = self.wrap_begin(body_list)?;
        self.lisp.lambda(params, body, env)
    }

    /// Evaluate `(begin expr1 expr2 ...)`.
    fn eval_begin(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let mut cur = args;
        let mut result = self.lisp.nil()?;
        while !cur.is_nil() {
            let expr = self.lisp.car(cur)?;
            result = self.eval(expr, env)?;
            cur = self.lisp.cdr(cur)?;
        }
        Ok(result)
    }

    /// Evaluate `(cond (test expr) ...)`.
    fn eval_cond(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let mut cur = args;
        while !cur.is_nil() {
            let clause = self.lisp.car(cur)?;
            let test = self.lisp.car(clause)?;
            let body = self.lisp.cdr(clause)?;

            // Check for `else` clause
            let is_else = self.lisp.symbol_name_eq(test, "else");
            if is_else {
                return self.eval_begin(body, env);
            }

            let test_val = self.eval(test, env)?;
            if !matches!(self.lisp.get(test_val)?, Value::False) {
                return self.eval_begin(body, env);
            }
            cur = self.lisp.cdr(cur)?;
        }
        self.lisp.nil()
    }

    /// Evaluate `(and expr1 expr2 ...)`.
    fn eval_and(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let mut cur = args;
        let mut result = self.lisp.boolean(true)?;
        while !cur.is_nil() {
            let expr = self.lisp.car(cur)?;
            result = self.eval(expr, env)?;
            if matches!(self.lisp.get(result)?, Value::False) {
                return Ok(result);
            }
            cur = self.lisp.cdr(cur)?;
        }
        Ok(result)
    }

    /// Evaluate `(or expr1 expr2 ...)`.
    fn eval_or(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let mut cur = args;
        while !cur.is_nil() {
            let expr = self.lisp.car(cur)?;
            let result = self.eval(expr, env)?;
            if !matches!(self.lisp.get(result)?, Value::False) {
                return Ok(result);
            }
            cur = self.lisp.cdr(cur)?;
        }
        self.lisp.boolean(false)
    }

    /// Evaluate `(let ((name val) ...) body...)`.
    fn eval_let(&mut self, args: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let bindings = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;

        let mut local_env = env;
        let mut cur = bindings;
        while !cur.is_nil() {
            let binding = self.lisp.car(cur)?;
            let name = self.lisp.car(binding)?;
            let val_expr = self.lisp.car(self.lisp.cdr(binding)?)?;
            let val = self.eval(val_expr, env)?;
            local_env = env_bind(self.lisp, local_env, name, val)?;
            cur = self.lisp.cdr(cur)?;
        }

        self.eval_begin(body_list, local_env)
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

    /// Evaluate each element in a list, returning a new list of results.
    fn eval_list(&mut self, list: ArenaIndex, env: ArenaIndex) -> ArenaResult<ArenaIndex> {
        if list.is_nil() {
            return self.lisp.nil();
        }
        let car = self.lisp.car(list)?;
        let cdr = self.lisp.cdr(list)?;
        let eval_car = self.eval(car, env)?;
        let eval_cdr = self.eval_list(cdr, env)?;
        self.lisp.cons(eval_car, eval_cdr)
    }

    /// Apply a function to arguments.
    fn apply(&mut self, func: ArenaIndex, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match self.lisp.get(func)? {
            Value::Builtin(id) => self.apply_builtin(id, args),
            Value::Lambda { .. } => {
                let (params, body, closure_env) = self.lisp.lambda_parts(func)?;
                let local_env = self.bind_params(params, args, closure_env)?;
                self.eval(body, local_env)
            }
            _ => Err(ArenaError::InvalidIndex),
        }
    }

    /// Bind parameter names to argument values in a new environment.
    fn bind_params(
        &self,
        params: ArenaIndex,
        args: ArenaIndex,
        env: ArenaIndex,
    ) -> ArenaResult<ArenaIndex> {
        let mut p = params;
        let mut a = args;
        let mut local_env = env;

        while !p.is_nil() {
            match self.lisp.get(p)? {
                Value::Cons { car: param, cdr: rest } => {
                    if a.is_nil() {
                        return Err(ArenaError::InvalidIndex);
                    }
                    let arg = self.lisp.car(a)?;
                    local_env = env_bind(self.lisp, local_env, param, arg)?;
                    p = rest;
                    a = self.lisp.cdr(a)?;
                }
                Value::Symbol(_) => {
                    // Rest parameter: bind remaining args as a list
                    local_env = env_bind(self.lisp, local_env, p, a)?;
                    return Ok(local_env);
                }
                _ => return Err(ArenaError::InvalidIndex),
            }
        }

        Ok(local_env)
    }

    /// Apply a built-in function.
    fn apply_builtin(&mut self, id: u8, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        match id {
            BUILTIN_ADD => self.builtin_add(args),
            BUILTIN_SUB => self.builtin_sub(args),
            BUILTIN_MUL => self.builtin_mul(args),
            BUILTIN_DIV => self.builtin_div(args),
            BUILTIN_EQ => self.builtin_eq(args),
            BUILTIN_LT => self.builtin_cmp(args, |a, b| a < b),
            BUILTIN_GT => self.builtin_cmp(args, |a, b| a > b),
            BUILTIN_LE => self.builtin_cmp(args, |a, b| a <= b),
            BUILTIN_GE => self.builtin_cmp(args, |a, b| a >= b),
            BUILTIN_CONS => self.builtin_cons(args),
            BUILTIN_CAR => self.builtin_car(args),
            BUILTIN_CDR => self.builtin_cdr(args),
            BUILTIN_LIST => Ok(args),
            BUILTIN_NULLP => self.builtin_nullp(args),
            BUILTIN_NOT => self.builtin_not(args),
            BUILTIN_PAIRP => self.builtin_pairp(args),
            BUILTIN_NUMBERP => self.builtin_numberp(args),
            BUILTIN_SYMBOLP => self.builtin_symbolp(args),
            BUILTIN_BOOLEANP => self.builtin_booleanp(args),
            _ => Err(ArenaError::InvalidIndex),
        }
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

    // ========================================================================
    // Pair / list built-ins
    // ========================================================================

    fn builtin_cons(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let a = self.lisp.car(args)?;
        let b = self.lisp.car(self.lisp.cdr(args)?)?;
        self.lisp.cons(a, b)
    }

    fn builtin_car(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let pair = self.lisp.car(args)?;
        self.lisp.car(pair)
    }

    fn builtin_cdr(&self, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        let pair = self.lisp.car(args)?;
        self.lisp.cdr(pair)
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
