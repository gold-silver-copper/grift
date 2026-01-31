//! Special form evaluation helpers.
//!
//! This module contains the step_eval_* and step_return_* helpers for
//! special forms like let, let*, letrec, cond, case, do, quasiquote, etc.

use grift_parser::{ArenaIndex, Value, Builtin, StdLib, parse};

use crate::error::{ErrorKind, EvalError, EvalResult};
use crate::continuation::{Cont, TrampolineState};
use crate::helpers::case_matches;
use super::Evaluator;

impl<'a, const N: usize> Evaluator<'a, N> {
    // ========================================================================
    // Cond
    // ========================================================================
    
    /// Evaluate cond using continuations
    pub(super) fn step_eval_cond(&mut self, clauses: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        if self.lisp.get(clauses)?.is_nil() {
            // No clauses - return unspecified (nil)
            let nil = self.lisp.nil()?;
            return Ok(TrampolineState::Return { val: nil });
        }

        let clause = self.lisp.car(clauses)?;
        let rest_clauses = self.lisp.cdr(clauses)?;
        let test = self.lisp.car(clause)?;
        let then_exprs = self.lisp.cdr(clause)?;

        // Check for else clause
        if self.lisp.symbol_matches(test, "else")? {
            if self.lisp.get(then_exprs)?.is_nil() {
                let nil = self.lisp.nil()?;
                return Ok(TrampolineState::Return { val: nil });
            }
            let begin = self.lisp.symbol("begin")?;
            let new_expr = self.lisp.cons(begin, then_exprs)?;
            return Ok(TrampolineState::Eval { expr: new_expr, env });
        }

        // Push continuation for after evaluating test
        let data_start = self.pack_cond_test(then_exprs, rest_clauses, env)?;
        self.push_cont(Cont::CondTest(data_start))?;

        // Evaluate the test
        Ok(TrampolineState::Eval { expr: test, env })
    }
    
    /// Helper for cond continuation - process remaining clauses
    pub(super) fn step_eval_cond_cont(&mut self, clauses: ArenaIndex, env: ArenaIndex) -> Result<Option<TrampolineState>, EvalError> {
        if self.lisp.get(clauses)?.is_nil() {
            // No more clauses - return unspecified (nil)
            let nil = self.lisp.nil()?;
            return Ok(Some(TrampolineState::Return { val: nil }));
        }

        let clause = self.lisp.car(clauses)?;
        let rest_clauses = self.lisp.cdr(clauses)?;
        let test = self.lisp.car(clause)?;
        let then_exprs = self.lisp.cdr(clause)?;

        // Check for else clause
        if self.lisp.symbol_matches(test, "else")? {
            if self.lisp.get(then_exprs)?.is_nil() {
                let nil = self.lisp.nil()?;
                return Ok(Some(TrampolineState::Return { val: nil }));
            }
            let begin = self.lisp.symbol("begin")?;
            let new_expr = self.lisp.cons(begin, then_exprs)?;
            return Ok(Some(TrampolineState::Eval { expr: new_expr, env }));
        }

        // Push continuation for after evaluating test
        let data_start = self.pack_cond_test(then_exprs, rest_clauses, env)?;
        self.push_cont(Cont::CondTest(data_start))?;

        // Evaluate the test
        Ok(Some(TrampolineState::Eval { expr: test, env }))
    }
    
    pub(super) fn step_return_cond_test(&mut self, val: ArenaIndex, data_start: usize) -> Result<Option<TrampolineState>, EvalError> {
        let (then_exprs, remaining_clauses, env) = self.unpack_cond_test(data_start);
        // val is the evaluated test
        if !self.is_false(val)? {
            // Test passed - evaluate body expressions
            if self.lisp.get(then_exprs)?.is_nil() {
                // No body - return the test value itself (cond => behavior)
                Ok(Some(TrampolineState::Return { val }))
            } else {
                // Evaluate body as begin
                let begin = self.lisp.symbol("begin")?;
                let new_expr = self.lisp.cons(begin, then_exprs)?;
                Ok(Some(TrampolineState::Eval { expr: new_expr, env }))
            }
        } else {
            // Test failed - try remaining clauses
            self.step_eval_cond_cont(remaining_clauses, env)
        }
    }
    
    pub(super) fn pack_cond_test(&mut self, then_exprs: ArenaIndex, remaining_clauses: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[then_exprs, remaining_clauses, env])
    }
    
    pub(super) fn unpack_cond_test(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex) {
        self.read_data3(data_start)
    }
    
    // ========================================================================
    // Let
    // ========================================================================
    
    /// Evaluate let using continuations (no Rust recursion)
    pub(super) fn step_eval_let(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;

        // Build body expression
        let body = if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
            self.lisp.car(body_list)?
        } else {
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body_list)?
        };

        // If no bindings, just evaluate body
        if self.lisp.get(bindings)?.is_nil() {
            return Ok(TrampolineState::Eval { expr: body, env });
        }

        // Get first binding
        let first_binding = self.lisp.car(bindings)?;
        let rest_bindings = self.lisp.cdr(bindings)?;
        let name = self.lisp.car(first_binding)?;
        let value_expr = self.lisp.car(self.lisp.cdr(first_binding)?)?;

        // Push continuation for after evaluating this binding
        let data_start = self.pack_let_binding(rest_bindings, env, env, body, name)?;
        self.push_cont(Cont::LetBinding(data_start))?;

        // Evaluate the first value expression in the original environment
        Ok(TrampolineState::Eval { expr: value_expr, env })
    }
    
    pub(super) fn step_return_let_binding(&mut self, val: ArenaIndex, data_start: usize) -> Result<Option<TrampolineState>, EvalError> {
        let (remaining_bindings, new_env, original_env, body, name) = self.unpack_let_binding(data_start);
        // val is the evaluated value for the current binding
        // Extend environment with the binding
        let extended_env = self.env_extend(new_env, name, val)?;

        // Check for more bindings
        if self.lisp.get(remaining_bindings)?.is_nil() {
            // All bindings done - evaluate body in extended environment
            Ok(Some(TrampolineState::Eval { expr: body, env: extended_env }))
        } else {
            // More bindings - get next binding
            let next_binding = self.lisp.car(remaining_bindings)?;
            let rest_bindings = self.lisp.cdr(remaining_bindings)?;
            let next_name = self.lisp.car(next_binding)?;
            let next_value_expr = self.lisp.car(self.lisp.cdr(next_binding)?)?;

            // Push continuation for after evaluating this binding
            let data_start = self.pack_let_binding(rest_bindings, extended_env, original_env, body, next_name)?;
            self.push_cont(Cont::LetBinding(data_start))?;

            // Evaluate the value expression in the ORIGINAL environment
            Ok(Some(TrampolineState::Eval { expr: next_value_expr, env: original_env }))
        }
    }
    
    pub(super) fn pack_let_binding(&mut self, remaining_bindings: ArenaIndex, new_env: ArenaIndex, 
                                  original_env: ArenaIndex, body: ArenaIndex, name: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[remaining_bindings, new_env, original_env, body, name])
    }
    
    pub(super) fn unpack_let_binding(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        self.read_data5(data_start)
    }
    
    // ========================================================================
    // Let*
    // ========================================================================
    
    /// Evaluate let* using continuations (no Rust recursion)
    pub(super) fn step_eval_let_star(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;

        // Build body expression
        let body = if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
            self.lisp.car(body_list)?
        } else {
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body_list)?
        };

        // If no bindings, just evaluate body
        if self.lisp.get(bindings)?.is_nil() {
            return Ok(TrampolineState::Eval { expr: body, env });
        }

        // Get first binding
        let first_binding = self.lisp.car(bindings)?;
        let rest_bindings = self.lisp.cdr(bindings)?;
        let name = self.lisp.car(first_binding)?;
        let value_expr = self.lisp.car(self.lisp.cdr(first_binding)?)?;

        // Push continuation for after evaluating this binding
        let data_start = self.pack_let_star_binding(rest_bindings, env, body, name)?;
        self.push_cont(Cont::LetStarBinding(data_start))?;

        // Evaluate the first value expression
        Ok(TrampolineState::Eval { expr: value_expr, env })
    }
    
    pub(super) fn step_return_let_star_binding(&mut self, val: ArenaIndex, data_start: usize) -> Result<Option<TrampolineState>, EvalError> {
        let (remaining_bindings, new_env, body, name) = self.unpack_let_star_binding(data_start);
        // val is the evaluated value for the current binding
        // Extend environment with the binding
        let extended_env = self.env_extend(new_env, name, val)?;

        // Check for more bindings
        if self.lisp.get(remaining_bindings)?.is_nil() {
            // All bindings done - evaluate body in extended environment
            Ok(Some(TrampolineState::Eval { expr: body, env: extended_env }))
        } else {
            // More bindings - get next binding
            let next_binding = self.lisp.car(remaining_bindings)?;
            let rest_bindings = self.lisp.cdr(remaining_bindings)?;
            let next_name = self.lisp.car(next_binding)?;
            let next_value_expr = self.lisp.car(self.lisp.cdr(next_binding)?)?;

            // Push continuation for after evaluating this binding
            let data_start = self.pack_let_star_binding(rest_bindings, extended_env, body, next_name)?;
            self.push_cont(Cont::LetStarBinding(data_start))?;

            // Evaluate the value expression in the NEW (extended) environment
            Ok(Some(TrampolineState::Eval { expr: next_value_expr, env: extended_env }))
        }
    }
    
    pub(super) fn pack_let_star_binding(&mut self, remaining_bindings: ArenaIndex, new_env: ArenaIndex, 
                                        body: ArenaIndex, name: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[remaining_bindings, new_env, body, name])
    }
    
    pub(super) fn unpack_let_star_binding(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        self.read_data4(data_start)
    }
    
    // ========================================================================
    // Letrec
    // ========================================================================
    
    /// Evaluate letrec/letrec* using continuations (no Rust recursion)
    pub(super) fn step_eval_letrec(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let bindings = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;

        // Build body expression
        let body = if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
            self.lisp.car(body_list)?
        } else {
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body_list)?
        };

        // If no bindings, just evaluate body
        if self.lisp.get(bindings)?.is_nil() {
            return Ok(TrampolineState::Eval { expr: body, env });
        }

        // First, create environment with all names bound to nil
        let mut new_env = env;
        let mut current = bindings;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { .. } => {
                    let binding = self.lisp.car(current)?;
                    let rest = self.lisp.cdr(current)?;
                    let name = self.lisp.car(binding)?;
                    let undefined = self.lisp.nil()?;
                    new_env = self.env_extend(new_env, name, undefined)?;
                    current = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, bindings)),
            }
        }

        // Now start evaluating init expressions
        let first_binding = self.lisp.car(bindings)?;
        let rest_bindings = self.lisp.cdr(bindings)?;
        let name = self.lisp.car(first_binding)?;
        let init_expr = self.lisp.car(self.lisp.cdr(first_binding)?)?;

        // Push continuation for after evaluating this init
        let data_start = self.pack_letrec_init(rest_bindings, new_env, body, name)?;
        self.push_cont(Cont::LetrecInit(data_start))?;

        // Evaluate the init expression in the new environment (where all names are visible)
        Ok(TrampolineState::Eval { expr: init_expr, env: new_env })
    }
    
    pub(super) fn step_return_letrec_init(&mut self, val: ArenaIndex, data_start: usize) -> Result<Option<TrampolineState>, EvalError> {
        let (remaining_bindings, new_env, body, name) = self.unpack_letrec_init(data_start);
        // val is the evaluated init expression - set! the variable
        self.env_set(new_env, name, val)?;

        // Check for more bindings
        if self.lisp.get(remaining_bindings)?.is_nil() {
            // All inits done - evaluate body
            Ok(Some(TrampolineState::Eval { expr: body, env: new_env }))
        } else {
            // More bindings - get next binding
            let next_binding = self.lisp.car(remaining_bindings)?;
            let rest_bindings = self.lisp.cdr(remaining_bindings)?;
            let next_name = self.lisp.car(next_binding)?;
            let next_init_expr = self.lisp.car(self.lisp.cdr(next_binding)?)?;

            // Push continuation for after evaluating this init
            let data_start = self.pack_letrec_init(rest_bindings, new_env, body, next_name)?;
            self.push_cont(Cont::LetrecInit(data_start))?;

            // Evaluate the init expression in the letrec environment
            Ok(Some(TrampolineState::Eval { expr: next_init_expr, env: new_env }))
        }
    }
    
    pub(super) fn pack_letrec_init(&mut self, remaining_bindings: ArenaIndex, new_env: ArenaIndex, 
                                   body: ArenaIndex, name: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[remaining_bindings, new_env, body, name])
    }
    
    pub(super) fn unpack_letrec_init(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        self.read_data4(data_start)
    }
    
    // ========================================================================
    // Begin
    // ========================================================================
    
    /// Evaluate begin using continuations (no Rust recursion)
    pub(super) fn step_eval_begin(&mut self, exprs: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        if self.lisp.get(exprs)?.is_nil() {
            // Empty begin - return nil
            let nil = self.lisp.nil()?;
            return Ok(TrampolineState::Return { val: nil });
        }

        let first_expr = self.lisp.car(exprs)?;
        let rest = self.lisp.cdr(exprs)?;

        if self.lisp.get(rest)?.is_nil() {
            // Single expression - tail call
            Ok(TrampolineState::Eval { expr: first_expr, env })
        } else {
            // Multiple expressions - push continuation for the rest
            let data_start = self.pack_begin_seq(rest, env)?;
            self.push_cont(Cont::BeginSeq(data_start))?;
            Ok(TrampolineState::Eval { expr: first_expr, env })
        }
    }
    
    // ========================================================================
    // And / Or
    // ========================================================================
    
    /// Evaluate and using continuations (no Rust recursion)
    pub(super) fn step_eval_and(&mut self, exprs: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        if self.lisp.get(exprs)?.is_nil() {
            // (and) with no args returns #t
            let true_val = self.lisp.boolean(true)?;
            return Ok(TrampolineState::Return { val: true_val });
        }

        let first_expr = self.lisp.car(exprs)?;
        let rest = self.lisp.cdr(exprs)?;

        if self.lisp.get(rest)?.is_nil() {
            // Single expression - evaluate it (its value is the result)
            Ok(TrampolineState::Eval { expr: first_expr, env })
        } else {
            // Multiple expressions - push continuation
            let data_start = self.pack_and(rest, env)?;
            self.push_cont(Cont::And(data_start))?;
            Ok(TrampolineState::Eval { expr: first_expr, env })
        }
    }

    /// Evaluate or using continuations (no Rust recursion)
    pub(super) fn step_eval_or(&mut self, exprs: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        if self.lisp.get(exprs)?.is_nil() {
            // (or) with no args returns #f
            let false_val = self.lisp.boolean(false)?;
            return Ok(TrampolineState::Return { val: false_val });
        }

        let first_expr = self.lisp.car(exprs)?;
        let rest = self.lisp.cdr(exprs)?;

        if self.lisp.get(rest)?.is_nil() {
            // Single expression - evaluate it (its value is the result)
            Ok(TrampolineState::Eval { expr: first_expr, env })
        } else {
            // Multiple expressions - push continuation
            let data_start = self.pack_or(rest, env)?;
            self.push_cont(Cont::Or(data_start))?;
            Ok(TrampolineState::Eval { expr: first_expr, env })
        }
    }
    
    // ========================================================================
    // Case
    // ========================================================================
    
    /// Evaluate case - pattern matching
    pub(super) fn step_eval_case(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let key_expr = self.lisp.car(args)?;
        let clauses = self.lisp.cdr(args)?;
        
        // Push continuation to check clauses after key is evaluated
        let data_start = self.pack_case_key(clauses, env)?;
        self.push_cont(Cont::CaseKey(data_start))?;
        
        // Evaluate key
        Ok(TrampolineState::Eval { expr: key_expr, env })
    }
    
    /// Helper for case - check clauses after key is evaluated
    pub(super) fn step_return_case_key(&mut self, key: ArenaIndex, clauses: ArenaIndex, env: ArenaIndex) 
        -> Result<Option<TrampolineState>, EvalError> 
    {
        // Check each clause
        let mut current = clauses;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    // No match found, return nil
                    let nil = self.lisp.nil()?;
                    return Ok(Some(TrampolineState::Return { val: nil }));
                }
                Value::Cons { .. } => {
                    let clause = self.lisp.car(current)?;
                    let rest = self.lisp.cdr(current)?;
                    let datums = self.lisp.car(clause)?;
                    let body = self.lisp.cdr(clause)?;
                    
                    // Check for 'else' clause
                    if self.lisp.symbol_matches(datums, "else").unwrap_or(false) {
                        let state = self.step_eval_begin(body, env)?;
                        return Ok(Some(state));
                    }
                    
                    // Check if key matches any datum
                    if case_matches(self.lisp, key, datums)? {
                        let state = self.step_eval_begin(body, env)?;
                        return Ok(Some(state));
                    }
                    
                    current = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, clauses)),
            }
        }
    }
    
    pub(super) fn pack_case_key(&mut self, clauses: ArenaIndex, env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[clauses, env])
    }
    
    pub(super) fn unpack_case_key(&self, data_start: usize) -> (ArenaIndex, ArenaIndex) {
        self.read_data2(data_start)
    }
    
    // ========================================================================
    // Do
    // ========================================================================
    
    /// Evaluate do - iteration construct
    pub(super) fn step_eval_do(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        // Parse (do ((var init step) ...) (test result ...) body ...)
        let bindings = self.lisp.car(args)?;
        let test_clause = self.lisp.car(self.lisp.cdr(args)?)?;
        let body = self.lisp.cdr(self.lisp.cdr(args)?)?;
        
        // Collect var-step pairs for iteration
        let var_steps = self.collect_do_steps(bindings)?;
        
        // Start evaluating init expressions
        if self.lisp.get(bindings)?.is_nil() {
            // No bindings - go straight to test
            let data_start = self.pack_do_test_result(var_steps, test_clause, body, env)?;
            self.push_cont(Cont::DoTestResult(data_start))?;
            let test = self.lisp.car(test_clause)?;
            Ok(TrampolineState::Eval { expr: test, env })
        } else {
            // Start with first binding
            let first_binding = self.lisp.car(bindings)?;
            let rest_bindings = self.lisp.cdr(bindings)?;
            let var = self.lisp.car(first_binding)?;
            let init = self.lisp.car(self.lisp.cdr(first_binding)?)?;
            
            let data_start = self.pack_do_init(rest_bindings, var_steps, test_clause, body, env, env, var)?;
            self.push_cont(Cont::DoInit(data_start))?;
            Ok(TrampolineState::Eval { expr: init, env })
        }
    }
    
    /// Collect (var . step) pairs from do bindings
    pub(super) fn collect_do_steps(&self, mut bindings: ArenaIndex) -> EvalResult {
        let mut result = self.lisp.nil()?;
        
        loop {
            match self.lisp.get(bindings)? {
                Value::Nil => return Ok(result),
                Value::Cons { .. } => {
                    let binding = self.lisp.car(bindings)?;
                    let rest = self.lisp.cdr(bindings)?;
                    let var = self.lisp.car(binding)?;
                    let binding_cdr = self.lisp.cdr(binding)?;
                    let init_and_step = self.lisp.cdr(binding_cdr)?;
                    
                    // Step is optional - if present, it's the car of init_and_step
                    let step = if self.lisp.get(init_and_step)?.is_nil() {
                        var // No step means use the variable itself
                    } else {
                        self.lisp.car(init_and_step)?
                    };
                    
                    let pair = self.lisp.cons(var, step)?;
                    result = self.lisp.cons(pair, result)?;
                    bindings = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, bindings)),
            }
        }
    }
    
    /// Helper for do - continue processing init bindings after one is evaluated
    pub(super) fn step_return_do_init(&mut self, remaining_bindings: ArenaIndex, var_steps: ArenaIndex, 
                                     test_clause: ArenaIndex, body: ArenaIndex, 
                                     loop_env: ArenaIndex, original_env: ArenaIndex) 
        -> Result<Option<TrampolineState>, EvalError> 
    {
        if self.lisp.get(remaining_bindings)?.is_nil() {
            // All inits done - evaluate test
            let data_start = self.pack_do_test_result(var_steps, test_clause, body, loop_env)?;
            self.push_cont(Cont::DoTestResult(data_start))?;
            let test = self.lisp.car(test_clause)?;
            Ok(Some(TrampolineState::Eval { expr: test, env: loop_env }))
        } else {
            // More inits
            let next_binding = self.lisp.car(remaining_bindings)?;
            let rest_bindings = self.lisp.cdr(remaining_bindings)?;
            let var = self.lisp.car(next_binding)?;
            let init = self.lisp.car(self.lisp.cdr(next_binding)?)?;
            
            let data_start = self.pack_do_init(rest_bindings, var_steps, test_clause, body, loop_env, original_env, var)?;
            self.push_cont(Cont::DoInit(data_start))?;
            Ok(Some(TrampolineState::Eval { expr: init, env: original_env }))
        }
    }
    
    pub(super) fn step_return_do_test_result(&mut self, val: ArenaIndex, data_start: usize) -> Result<Option<TrampolineState>, EvalError> {
        let (var_steps, test_clause, body, loop_env) = self.unpack_do_test_result(data_start);
        // val is the evaluated test result
        if !self.is_false(val)? {
            // Test passed - evaluate result expressions
            let result_exprs = self.lisp.cdr(test_clause)?;
            if self.lisp.get(result_exprs)?.is_nil() {
                Ok(Some(TrampolineState::Return { val }))
            } else {
                let state = self.step_eval_begin(result_exprs, loop_env)?;
                Ok(Some(state))
            }
        } else {
            // Test failed - evaluate body for side effects, then steps
            if self.lisp.get(body)?.is_nil() {
                // No body - go straight to steps
                self.step_do_start_steps(var_steps, test_clause, body, loop_env)
            } else {
                // Evaluate body expressions
                let first_body = self.lisp.car(body)?;
                let rest_body = self.lisp.cdr(body)?;
                let data_start = self.pack_do_body(rest_body, var_steps, test_clause, body, loop_env)?;
                self.push_cont(Cont::DoBody(data_start))?;
                Ok(Some(TrampolineState::Eval { expr: first_body, env: loop_env }))
            }
        }
    }
    
    pub(super) fn step_return_do_body(&mut self, _val: ArenaIndex, data_start: usize) -> Result<Option<TrampolineState>, EvalError> {
        let (remaining_body, var_steps, test_clause, body, loop_env) = self.unpack_do_body(data_start);
        // val is discarded (body evaluated for side effects)
        if self.lisp.get(remaining_body)?.is_nil() {
            // Body done - start evaluating step expressions
            self.step_do_start_steps(var_steps, test_clause, body, loop_env)
        } else {
            // More body expressions
            let next_body = self.lisp.car(remaining_body)?;
            let rest_body = self.lisp.cdr(remaining_body)?;
            let data_start = self.pack_do_body(rest_body, var_steps, test_clause, body, loop_env)?;
            self.push_cont(Cont::DoBody(data_start))?;
            Ok(Some(TrampolineState::Eval { expr: next_body, env: loop_env }))
        }
    }
    
    pub(super) fn step_return_do_step(&mut self, val: ArenaIndex, data_start: usize) -> Result<Option<TrampolineState>, EvalError> {
        let (remaining_steps, collected_vals, var_steps, test_clause, body, loop_env, current_var) = self.unpack_do_step(data_start);
        // val is the evaluated step expression - collect and continue
        let new_collected = self.lisp.cons(current_var, val)?;
        let new_collected = self.lisp.cons(new_collected, collected_vals)?;
        
        if self.lisp.get(remaining_steps)?.is_nil() {
            // All steps evaluated - update environment and loop
            let new_env = self.apply_do_step_values(loop_env, new_collected)?;
            // Continue to next iteration - evaluate test
            let data_start = self.pack_do_test_result(var_steps, test_clause, body, new_env)?;
            self.push_cont(Cont::DoTestResult(data_start))?;
            let test = self.lisp.car(test_clause)?;
            Ok(Some(TrampolineState::Eval { expr: test, env: new_env }))
        } else {
            // More steps to evaluate
            let next_pair = self.lisp.car(remaining_steps)?;
            let rest_steps = self.lisp.cdr(remaining_steps)?;
            let next_var = self.lisp.car(next_pair)?;
            let next_step = self.lisp.cdr(next_pair)?;
            
            let data_start = self.pack_do_step(rest_steps, new_collected, var_steps, test_clause, body, loop_env, next_var)?;
            self.push_cont(Cont::DoStep(data_start))?;
            Ok(Some(TrampolineState::Eval { expr: next_step, env: loop_env }))
        }
    }
    
    /// Start evaluating step expressions
    pub(super) fn step_do_start_steps(&mut self, var_steps: ArenaIndex, test_clause: ArenaIndex, body: ArenaIndex, loop_env: ArenaIndex) 
        -> Result<Option<TrampolineState>, EvalError> 
    {
        if self.lisp.get(var_steps)?.is_nil() {
            // No steps - go straight to test
            let data_start = self.pack_do_test_result(var_steps, test_clause, body, loop_env)?;
            self.push_cont(Cont::DoTestResult(data_start))?;
            let test = self.lisp.car(test_clause)?;
            Ok(Some(TrampolineState::Eval { expr: test, env: loop_env }))
        } else {
            // Start evaluating first step
            let first_pair = self.lisp.car(var_steps)?;
            let rest_pairs = self.lisp.cdr(var_steps)?;
            let var = self.lisp.car(first_pair)?;
            let step_expr = self.lisp.cdr(first_pair)?;
            
            let nil = self.lisp.nil()?;
            let data_start = self.pack_do_step(rest_pairs, nil, var_steps, test_clause, body, loop_env, var)?;
            self.push_cont(Cont::DoStep(data_start))?;
            Ok(Some(TrampolineState::Eval { expr: step_expr, env: loop_env }))
        }
    }
    
    /// Apply collected step values to update environment
    pub(super) fn apply_do_step_values(&self, mut env: ArenaIndex, mut collected: ArenaIndex) -> EvalResult {
        // collected is a list of (var . val) pairs
        loop {
            match self.lisp.get(collected)? {
                Value::Nil => return Ok(env),
                Value::Cons { .. } => {
                    let pair = self.lisp.car(collected)?;
                    let rest = self.lisp.cdr(collected)?;
                    let var = self.lisp.car(pair)?;
                    let val = self.lisp.cdr(pair)?;
                    self.env_set(env, var, val)?;
                    collected = rest;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, collected)),
            }
        }
    }
    
    // Do pack/unpack helpers
    pub(super) fn pack_do_init(&mut self, remaining_bindings: ArenaIndex, var_steps: ArenaIndex, test_clause: ArenaIndex,
                              body: ArenaIndex, loop_env: ArenaIndex, original_env: ArenaIndex, current_var: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[remaining_bindings, var_steps, test_clause, body, loop_env, original_env, current_var])
    }
    
    pub(super) fn unpack_do_init(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        self.read_data7(data_start)
    }
    
    pub(super) fn pack_do_test_result(&mut self, var_steps: ArenaIndex, test_clause: ArenaIndex, body: ArenaIndex, loop_env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[var_steps, test_clause, body, loop_env])
    }
    
    pub(super) fn unpack_do_test_result(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        self.read_data4(data_start)
    }
    
    pub(super) fn pack_do_body(&mut self, remaining_body: ArenaIndex, var_steps: ArenaIndex, test_clause: ArenaIndex, body: ArenaIndex, loop_env: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[remaining_body, var_steps, test_clause, body, loop_env])
    }
    
    pub(super) fn unpack_do_body(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        self.read_data5(data_start)
    }
    
    pub(super) fn pack_do_step(&mut self, remaining_steps: ArenaIndex, collected_vals: ArenaIndex, var_steps: ArenaIndex, 
                               test_clause: ArenaIndex, body: ArenaIndex, loop_env: ArenaIndex, current_var: ArenaIndex) -> Result<usize, EvalError> {
        self.push_data(&[remaining_steps, collected_vals, var_steps, test_clause, body, loop_env, current_var])
    }
    
    pub(super) fn unpack_do_step(&self, data_start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        self.read_data7(data_start)
    }
    
    // ========================================================================
    // Apply
    // ========================================================================
    
    /// Evaluate apply - apply function to list of arguments
    pub(super) fn step_eval_apply(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let func_expr = self.lisp.car(args)?;
        let args_list_expr = self.lisp.car(self.lisp.cdr(args)?)?;
        
        // Push continuation for after evaluating func
        let data_start = self.pack_apply_first(args_list_expr, env)?;
        self.push_cont(Cont::ApplyFirst(data_start))?;
        
        // Evaluate function expression
        Ok(TrampolineState::Eval { expr: func_expr, env })
    }
    
    // ========================================================================
    // Values
    // ========================================================================
    
    /// Evaluate values - create a multi-value return (trampolined)
    pub(super) fn eval_values(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        if self.lisp.get(args)?.is_nil() {
            // No values - return nil
            let nil = self.lisp.nil()?;
            return Ok(TrampolineState::Return { val: nil });
        }
        
        let first_expr = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        
        if self.lisp.get(rest)?.is_nil() {
            // Single value - just evaluate it
            Ok(TrampolineState::Eval { expr: first_expr, env })
        } else {
            // Multiple values - push continuation
            let nil = self.lisp.nil()?;
            let data_start = self.pack_values_collect(rest, nil, env)?;
            self.push_cont(Cont::ValuesCollect(data_start))?;
            Ok(TrampolineState::Eval { expr: first_expr, env })
        }
    }
    
    // ========================================================================
    // Lambda
    // ========================================================================
    
    /// Evaluate lambda
    pub(super) fn eval_lambda(&mut self, args: ArenaIndex, env: ArenaIndex) -> EvalResult {
        let params = self.lisp.car(args)?;
        let body_list = self.lisp.cdr(args)?;
        
        // Support multi-expression body
        let body = if self.lisp.get(self.lisp.cdr(body_list)?)?.is_nil() {
            self.lisp.car(body_list)?
        } else {
            let begin = self.lisp.symbol("begin")?;
            self.lisp.cons(begin, body_list)?
        };
        
        self.lisp.lambda(params, body, env).map_err(Into::into)
    }
    
    // ========================================================================
    // Define
    // ========================================================================
    
    /// Evaluate define (trampolined)
    pub(super) fn eval_define(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let first = self.lisp.car(args)?;
        
        // Check if function shorthand: (define (name params...) body)
        if let Value::Cons { .. } = self.lisp.get(first)? {
            let name = self.lisp.car(first)?;
            let params = self.lisp.cdr(first)?;
            let body = self.lisp.cdr(args)?;
            
            // Build lambda: (lambda params body)
            let lambda_body = if self.lisp.get(self.lisp.cdr(body)?)?.is_nil() {
                self.lisp.car(body)?
            } else {
                let begin = self.lisp.symbol("begin")?;
                self.lisp.cons(begin, body)?
            };
            
            let lambda_val = self.lisp.lambda(params, lambda_body, env)?;
            self.define(name, lambda_val)?;
            return Ok(TrampolineState::Return { val: name });
        }
        
        // Simple define: (define name expr)
        let name = first;
        let expr = self.lisp.car(self.lisp.cdr(args)?)?;
        
        // Push continuation for after evaluating the expression
        let data_start = self.pack_define_value(name)?;
        self.push_cont(Cont::DefineValue(data_start))?;
        
        Ok(TrampolineState::Eval { expr, env })
    }
    
    // ========================================================================
    // Set!
    // ========================================================================
    
    /// Evaluate (set! name value) - mutate an existing variable binding (trampolined)
    pub(super) fn eval_set(&mut self, args: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        let name = self.lisp.car(args)?;
        let value_expr = self.lisp.car(self.lisp.cdr(args)?)?;
        
        // Push continuation for after evaluating the value
        let data_start = self.pack_set_value(name, env)?;
        self.push_cont(Cont::SetValue(data_start))?;
        
        Ok(TrampolineState::Eval { expr: value_expr, env })
    }
    
    // ========================================================================
    // Quasiquote
    // ========================================================================
    
    /// Evaluate quasiquote - template with unquote (trampolined version)
    pub(super) fn eval_quasiquote(&mut self, template: ArenaIndex, env: ArenaIndex) -> Result<TrampolineState, EvalError> {
        self.step_quasiquote_trampoline(template, env, 1)
    }
    
    /// Trampolined quasiquote processing
    pub(super) fn step_quasiquote_trampoline(&mut self, template: ArenaIndex, env: ArenaIndex, depth: usize) 
        -> Result<TrampolineState, EvalError> 
    {
        match self.lisp.get(template)? {
            Value::Cons { .. } => {
                let car = self.lisp.car(template)?;
                let cdr = self.lisp.cdr(template)?;
                
                // Check for unquote
                if self.lisp.symbol_matches(car, "unquote")? {
                    let inner = self.lisp.car(cdr)?;
                    if depth == 1 {
                        // Depth 1: Evaluate and return result
                        return Ok(TrampolineState::Eval { expr: inner, env });
                    } else {
                        // Nested: Decrement depth and process inner
                        self.push_cont(Cont::QuasiquoteUnquoteWrap)?;
                        return self.step_quasiquote_trampoline(inner, env, depth - 1);
                    }
                }
                
                // Check for unquote-splicing
                if self.lisp.symbol_matches(car, "unquote-splicing")? {
                    let inner = self.lisp.car(cdr)?;
                    if depth == 1 {
                        // Depth 1: Evaluate splice and continue with cdr
                        let data_start = self.pack_quasiquote_splice(cdr, depth, env)?;
                        self.push_cont(Cont::QuasiquoteSplice(data_start))?;
                        return Ok(TrampolineState::Eval { expr: inner, env });
                    } else {
                        // Nested: Just process normally
                    }
                }
                
                // Check for nested quasiquote
                if self.lisp.symbol_matches(car, "quasiquote")? {
                    let inner = self.lisp.car(cdr)?;
                    self.push_cont(Cont::QuasiquoteNestedWrap)?;
                    return self.step_quasiquote_trampoline(inner, env, depth + 1);
                }
                
                // Regular cons: process car, then cdr
                let data_start = self.pack_quasiquote_car(cdr, depth, env)?;
                self.push_cont(Cont::QuasiquoteCar(data_start))?;
                self.step_quasiquote_trampoline(car, env, depth)
            }
            _ => {
                // Atoms are returned as-is
                Ok(TrampolineState::Return { val: template })
            }
        }
    }
}
