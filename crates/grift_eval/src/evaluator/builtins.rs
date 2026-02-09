//! Builtin function implementations for the evaluator.
//!
//! Contains apply_builtin, apply_binary_builtin, and related helper functions
//! for numeric operations, comparisons, and string/char operations.

use grift_parser::{ArenaIndex, Value, Builtin};

use crate::error::{ErrorKind, EvalError, EvalResult};
use crate::continuation::{TrampolineState, ContType, is_binary_builtin, EnvRef, ExprRef};
use crate::helpers::{gcd_helper, int_pow, equal_recursive};
use crate::{
    extract_args, builtin_unary_pred, builtin_numeric_pred, builtin_int_identity, builtin_div_op,
    builtin_unary_int, builtin_char_to_int, builtin_char_transform,
    binary_int_cmp, binary_int_op, binary_div_op,
};

use super::Evaluator;

impl<'a, const N: usize> Evaluator<'a, N> {
    pub(super) fn apply_builtin_with_args(&mut self, builtin: Builtin, args_expr: ArenaIndex, env: EnvRef, call_expr: ArenaIndex) 
        -> Result<Option<TrampolineState>, EvalError> 
    {
        // Get first arg expression
        let first_arg = self.lisp.car(args_expr)?;
        let rest_args = self.lisp.cdr(args_expr)?;
        
        // Check for binary builtins optimization
        if is_binary_builtin(builtin) {
            // Check if exactly 2 args
            if !self.lisp.get(rest_args)?.is_nil() {
                let second_arg_expr = self.lisp.car(rest_args)?;
                let third_check = self.lisp.cdr(rest_args)?;
                if self.lisp.get(third_check)?.is_nil() {
                    // Exactly 2 args - use optimized binary path
                    // Evaluate second arg expr (store for later), then evaluate first
                    // Data: (builtin_encoded . (second_arg . (call_expr . eval_env)))
                    let builtin_encoded = Self::encode_builtin(builtin);
                    self.cont(ContType::BinaryBuiltinFirst, env).data4(builtin_encoded, second_arg_expr, call_expr, env.0)?;
                    return Ok(Some(TrampolineState::Eval { expr: ExprRef(first_arg), env }));
                }
            }
        }
        
        // General case: collect args and apply
        // Data: (builtin_encoded . (remaining_args . (collected . (call_expr . eval_env))))
        let nil = self.lisp.nil()?;
        let builtin_encoded = Self::encode_builtin(builtin);
        self.cont(ContType::BuiltinForceArg, env).data5(builtin_encoded, rest_args, nil, call_expr, env.0)?;
        
        Ok(Some(TrampolineState::Eval { expr: ExprRef(first_arg), env }))
    }
    
    /// Reverse a list (used for BuiltinForceArg fallback path)
    pub(super) fn reverse_list(&self, mut list: ArenaIndex) -> EvalResult {
        let mut result = self.lisp.nil()?;
        loop {
            match self.lisp.get(list)? {
                Value::Nil => return Ok(result),
                Value::Cons { .. } => {
                    let car = self.lisp.car(list)?;
                    let cdr = self.lisp.cdr(list)?;
                    result = self.lisp.cons(car, result)?;
                    list = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, list)),
            }
        }
    }
    
    /// Apply a builtin function (trampolined version)
    /// Arguments are already evaluated in strict mode
    pub(super) fn apply_builtin_trampolined(&mut self, builtin: Builtin, args: ArenaIndex, call_expr: ArenaIndex) 
        -> Result<TrampolineState, EvalError> 
    {
        // Special handling for error - creates error object and raises through exception system
        if matches!(builtin, Builtin::Error) {
            return self.apply_error_builtin(args, call_expr);
        }
        
        // Special handling for load - reads file and evaluates all expressions
        if matches!(builtin, Builtin::Load) {
            return self.apply_load_builtin(args, call_expr);
        }
        
        // In strict evaluation, args are already evaluated values
        let result = self.apply_builtin(builtin, args, call_expr)?;
        Ok(TrampolineState::Return { val: result })
    }
    
    /// Create an R7RS error object and raise it through the exception handler chain.
    /// (error message obj ...) — R7RS §6.11
    pub(super) fn apply_error_builtin(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        // First arg is the message
        let msg = self.lisp.car(args)?;
        // Rest are irritants
        let irritants = self.lisp.cdr(args)?;
        // Type is Nil for standard (error msg ...) calls
        let nil = self.lisp.nil()?;
        let irritants_and_type = self.lisp.cons(irritants, nil)?;
        let error_obj = self.lisp.alloc(Value::ErrorObject { message: msg, irritants_and_type })?;
        
        // Raise through the exception handler chain
        match self.invoke_exception_handler(error_obj, false)? {
            Some(state) => Ok(state),
            None => {
                // No continuation to return to — shouldn't happen for raise
                Err(self.make_error(ErrorKind::UserError, call_expr))
            }
        }
    }
    
    /// Implement (load filename) — R7RS §6.13.
    /// Reads the file, parses all expressions, and evaluates them sequentially.
    fn apply_load_builtin(&mut self, args: ArenaIndex, call_expr: ArenaIndex)
        -> Result<TrampolineState, EvalError>
    {
        let arg = self.lisp.car(args)?;
        let path = self.extract_string_arg(arg, call_expr)?;

        // Read file content and parse all expressions while holding the io borrow.
        // We must parse before releasing the borrow, since the content &str is tied to it.
        let forms = match &mut self.io {
            Some(io) => {
                let content = match io.read_file(&path) {
                    Ok(s) => s,
                    Err(_) => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                };
                grift_parser::parse_all(self.lisp, content)?
            }
            None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
        };

        // Evaluate each expression sequentially
        let mut current = forms;
        let mut last_val = self.lisp.void_val()?;
        while let Value::Cons { .. } = self.lisp.get(current)? {
            let form = self.lisp.car(current)?;
            last_val = self.eval(ExprRef(form))?;
            current = self.lisp.cdr(current)?;
        }

        Ok(TrampolineState::Return { val: last_val })
    }
    
    /// Apply a builtin with already-evaluated arguments
    pub(super) fn apply_builtin(&mut self, builtin: Builtin, args: ArenaIndex, call_expr: ArenaIndex) 
        -> EvalResult 
    {
        match builtin {
            Builtin::Car => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Cons { .. } => self.lisp.car(arg).map_err(Into::into),
                    // Scheme R7RS: car of empty list is an error
                    Value::Nil => Err(self.type_error(call_expr, "pair", "null")),
                    _ => Err(self.type_error(call_expr, "pair", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::Cdr => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Cons { .. } => self.lisp.cdr(arg).map_err(Into::into),
                    // Scheme R7RS: cdr of empty list is an error
                    Value::Nil => Err(self.type_error(call_expr, "pair", "null")),
                    _ => Err(self.type_error(call_expr, "pair", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::Cons => {
                extract_args!(self, args, a, b);
                self.lisp.cons(a, b).map_err(Into::into)
            }
            
            Builtin::List => Ok(args),
            
            // Scheme-compliant equality predicates
            // eq? and eqv? have identical semantics in this integer-only implementation
            Builtin::EqP | Builtin::EqvP => {
                extract_args!(self, args, a, b);
                
                let val_a = self.lisp.get(a)?;
                let val_b = self.lisp.get(b)?;
                
                let eq = match (val_a, val_b) {
                    (Value::Nil, Value::Nil) => true,
                    (Value::True, Value::True) => true,
                    (Value::False, Value::False) => true,
                    (Value::Number(x), Value::Number(y)) => x == y,
                    (Value::Char(x), Value::Char(y)) => x == y,
                    (Value::Symbol(_), Value::Symbol(_)) => self.lisp.symbol_eq(a, b)?,
                    (Value::String { len: la, data: da }, Value::String { len: lb, data: db }) => {
                        a == b || (la == lb && da == db)
                    }
                    _ => a == b,
                };
                
                self.lisp.boolean(eq).map_err(Into::into)
            }
            
            Builtin::EqualP => {
                // equal? - tests structural equality recursively
                let result = equal_recursive(self.lisp, self.lisp.car(args)?, self.lisp.car(self.lisp.cdr(args)?)?)?;
                self.lisp.boolean(result).map_err(Into::into)
            }
            
            Builtin::Null => builtin_unary_pred!(self, args, |v: Value| v.is_nil()),
            
            Builtin::Pairp => builtin_unary_pred!(self, args, |v: Value| v.is_cons()),
            
            Builtin::Numberp => builtin_unary_pred!(self, args, |v: Value| v.is_number()),
            
            Builtin::Booleanp => builtin_unary_pred!(self, args, |v: Value| v.is_boolean()),
            
            Builtin::Procedurep => builtin_unary_pred!(self, args, |v: Value| v.is_procedure()),
            
            Builtin::Symbolp => builtin_unary_pred!(self, args, |v: Value| v.is_symbol()),
            
            Builtin::Not => builtin_unary_pred!(self, args, |v: Value| v.is_false()),
            
            Builtin::Add => self.numeric_fold(args, 0, 
                |a, b| a.checked_add(b), 
                call_expr),
            
            Builtin::Sub => {
                let first = self.get_int(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                if self.lisp.get(rest)?.is_nil() {
                    // Unary minus
                    self.lisp.number(-first).map_err(Into::into)
                } else {
                    self.numeric_fold(rest, first, 
                        |a, b| a.checked_sub(b), 
                        call_expr)
                }
            }
            
            Builtin::Mul => self.numeric_fold(args, 1, 
                |a, b| a.checked_mul(b), 
                call_expr),
            
            Builtin::Div => {
                // Integer division
                let first = self.get_int(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold(rest, first, 
                    |a, b| if b == 0 { None } else { a.checked_div(b) },
                    call_expr)
            }
            
            // Division operations with zero check - generated by builtin_div_op! macro
            // Scheme modulo: result has the sign of the divisor
            Builtin::Modulo => builtin_div_op!(self, args, call_expr, |a, b| ((a % b) + b) % b),
            // Scheme remainder: result has the sign of the dividend
            Builtin::Remainder => builtin_div_op!(self, args, call_expr, |a, b| a % b),
            // Integer quotient (truncated towards zero)
            Builtin::Quotient => builtin_div_op!(self, args, call_expr, |a, b| a / b),
            
            Builtin::Abs => builtin_unary_int!(self, args, call_expr, |n: isize| n.abs()),
            
            Builtin::Max => {
                // Maximum of one or more numbers
                let first = self.get_int(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold(rest, first, 
                    |a, b| Some(if a > b { a } else { b }),
                    call_expr)
            }
            
            Builtin::Min => {
                // Minimum of one or more numbers
                let first = self.get_int(self.lisp.car(args)?, call_expr)?;
                let rest = self.lisp.cdr(args)?;
                self.numeric_fold(rest, first,
                    |a, b| Some(if a < b { a } else { b }),
                    call_expr)
            }
            
            Builtin::Gcd => {
                // Greatest common divisor (integers only)
                // gcd() with no args returns 0, gcd(n) returns |n|
                if self.lisp.get(args)?.is_nil() {
                    return self.lisp.number(0).map_err(Into::into);
                }
                let first = self.get_int(self.lisp.car(args)?, call_expr)?.abs();
                let rest = self.lisp.cdr(args)?;
                let mut acc = first;
                let mut current = rest;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => return self.lisp.number(acc).map_err(Into::into),
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            let n = self.get_int(car, call_expr)?.abs();
                            acc = gcd_helper(acc, n);
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
            }
            
            Builtin::Lcm => {
                // Least common multiple (integers only)
                // lcm() with no args returns 1, lcm(n) returns |n|
                if self.lisp.get(args)?.is_nil() {
                    return self.lisp.number(1).map_err(Into::into);
                }
                let first = self.get_int(self.lisp.car(args)?, call_expr)?.abs();
                let rest = self.lisp.cdr(args)?;
                let mut acc = first;
                let mut current = rest;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => return self.lisp.number(acc).map_err(Into::into),
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            let b_abs = self.get_int(car, call_expr)?.abs();
                            if acc == 0 || b_abs == 0 {
                                acc = 0;
                            } else {
                                let g = gcd_helper(acc, b_abs);
                                acc = (acc / g).saturating_mul(b_abs);
                            }
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
            }
            
            Builtin::Expt => {
                // Exponentiation: (expt base power)
                let base = self.get_int(self.lisp.car(args)?, call_expr)?;
                let power = self.get_int(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
                
                if power < 0 {
                    // Negative integer exponent - error for integer-only mode
                    if base == 0 {
                        return Err(self.make_error(ErrorKind::DivisionByZero, call_expr));
                    }
                    // Return 0 for |base| > 1, 1 for |base| == 1
                    let result = if base.abs() == 1 { 1 } else { 0 };
                    self.lisp.number(result).map_err(Into::into)
                } else {
                    let result = int_pow(base, power as usize);
                    self.lisp.number(result).map_err(Into::into)
                }
            }
            
            Builtin::Square => builtin_unary_int!(self, args, call_expr, |n: isize| n.saturating_mul(n)),
            
            // Numeric predicates - generated by builtin_numeric_pred! macro
            Builtin::Zerop => builtin_numeric_pred!(self, args, call_expr, |n| n == 0),
            Builtin::Positivep => builtin_numeric_pred!(self, args, call_expr, |n| n > 0),
            Builtin::Negativep => builtin_numeric_pred!(self, args, call_expr, |n| n < 0),
            Builtin::Oddp => builtin_numeric_pred!(self, args, call_expr, |n| n % 2 != 0),
            Builtin::Evenp => builtin_numeric_pred!(self, args, call_expr, |n| n % 2 == 0),
            
            // All numbers are integers and exact in this implementation
            Builtin::Integerp | Builtin::Exactp | Builtin::ExactIntegerp => {
                builtin_unary_pred!(self, args, |v: Value| matches!(v, Value::Number(_)))
            }
            
            Builtin::Inexactp => {
                // No inexact numbers - always false for valid numbers, error for non-numbers
                let val = self.lisp.car(args)?;
                match self.lisp.get(val)? {
                    Value::Number(_) => self.lisp.boolean(false).map_err(Into::into),
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            // Rounding operations (identity for integers)
            Builtin::Floor | Builtin::Ceiling | Builtin::Truncate | Builtin::Round => {
                builtin_int_identity!(self, args, call_expr)
            }
            
            Builtin::Lt => self.compare_numbers(args, |a, b| a < b, call_expr),
            Builtin::Gt => self.compare_numbers(args, |a, b| a > b, call_expr),
            Builtin::Le => self.compare_numbers(args, |a, b| a <= b, call_expr),
            Builtin::Ge => self.compare_numbers(args, |a, b| a >= b, call_expr),
            Builtin::NumEq => self.compare_numbers(args, |a, b| a == b, call_expr),
            
            Builtin::Display => {
                let val = self.lisp.car(args)?;
                // Call output callback if set
                if let Some(callback) = self.output_callback {
                    callback(self.lisp, val);
                }
                // Return void (unspecified value) per R7RS
                self.lisp.void_val().map_err(Into::into)
            }
            
            Builtin::Newline => {
                // Call output callback with nil (marker for newline)
                if let Some(callback) = self.output_callback {
                    callback(self.lisp, self.lisp.nil()?);
                }
                // Return void (unspecified value) per R7RS
                self.lisp.void_val().map_err(Into::into)
            }
            
            Builtin::Error => {
                // Fallback: if called directly without trampolining, create error object
                // and return as Rust error. Normal path goes through apply_error_builtin.
                let msg = self.lisp.car(args)?;
                let irritants = self.lisp.cdr(args)?;
                let nil = self.lisp.nil()?;
                let irritants_and_type = self.lisp.cons(irritants, nil)?;
                let error_obj = self.lisp.alloc(Value::ErrorObject { message: msg, irritants_and_type })?;
                Err(self.make_error(ErrorKind::UserError, error_obj))
            }
            
            Builtin::ErrorObjectP => {
                builtin_unary_pred!(self, args, |v: Value| matches!(v, Value::ErrorObject { .. }))
            }
            
            Builtin::ErrorObjectMessage => {
                // (error-object-message error-object) — R7RS §6.11
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::ErrorObject { message, .. } => Ok(message),
                    _ => Err(self.type_error(arg, "error-object", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::ErrorObjectIrritants => {
                // (error-object-irritants error-object) — R7RS §6.11
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::ErrorObject { irritants_and_type, .. } => {
                        self.lisp.car(irritants_and_type).map_err(Into::into)
                    }
                    _ => Err(self.type_error(arg, "error-object", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::ErrorObjectType => {
                // (error-object-type error-object) — R7RS §6.11
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::ErrorObject { irritants_and_type, .. } => {
                        self.lisp.cdr(irritants_and_type).map_err(Into::into)
                    }
                    _ => Err(self.type_error(arg, "error-object", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::SetCar => {
                // (set-car! pair value) - mutate the car of a cons cell
                extract_args!(self, args, pair, value);
                
                // Verify it's a pair
                match self.lisp.get(pair)? {
                    Value::Cons { .. } => {
                        self.lisp.set_car(pair, value).map_err(Into::into)
                    }
                    _ => Err(self.make_error(ErrorKind::NotAPair, call_expr)),
                }
            }
            
            Builtin::SetCdr => {
                // (set-cdr! pair value) - mutate the cdr of a cons cell
                extract_args!(self, args, pair, value);
                
                // Verify it's a pair
                match self.lisp.get(pair)? {
                    Value::Cons { .. } => {
                        self.lisp.set_cdr(pair, value).map_err(Into::into)
                    }
                    _ => Err(self.make_error(ErrorKind::NotAPair, call_expr)),
                }
            }
            
            // ============================================================
            // Vector operations (R7RS Section 6.8)
            // ============================================================
            
            Builtin::Vectorp => {
                builtin_unary_pred!(self, args, |v: Value| matches!(v, Value::Array { .. }))
            }
            
            Builtin::MakeVector => {
                // (make-vector k) or (make-vector k fill) - create a vector
                let len_val = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                
                let len = match self.lisp.get(len_val)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                // Default fill is 0 (unspecified in R7RS, we use 0)
                let fill = if self.lisp.get(rest)?.is_nil() {
                    self.lisp.number(0)?
                } else {
                    self.lisp.car(rest)?
                };
                
                self.lisp.make_array(len, fill).map_err(Into::into)
            }
            
            Builtin::Vector => {
                // (vector obj ...) - create vector from arguments
                self.list_to_array(args, call_expr)
            }
            
            Builtin::VectorLength => {
                // (vector-length vec) - get length of vector
                let vec = self.lisp.car(args)?;
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
                        self.lisp.number(len as isize).map_err(Into::into)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::VectorRef => {
                // (vector-ref vec k) - get element at index k
                extract_args!(self, args, vec, index_val);
                
                let index = match self.lisp.get(index_val)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        self.lisp.array_get(vec, index).map_err(Into::into)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::VectorSet => {
                // (vector-set! vec k obj) - set element at index k
                extract_args!(self, args, vec, index_val, value);
                
                let index = match self.lisp.get(index_val)? {
                    Value::Number(n) if n >= 0 => n as usize,
                    _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                };
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        self.lisp.array_set(vec, index, value)?;
                        // R7RS: returns unspecified, we return the vector
                        Ok(vec)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::VectorToList => {
                // (vector->list vec) - convert vector to list
                let vec = self.lisp.car(args)?;
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
                        // Build list from end to front
                        let mut result = self.lisp.nil()?;
                        for i in (0..len).rev() {
                            let elem = self.lisp.array_get(vec, i)?;
                            result = self.lisp.cons(elem, result)?;
                        }
                        Ok(result)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::ListToVector => {
                // (list->vector lst) - convert list to vector
                let lst = self.lisp.car(args)?;
                self.list_to_array(lst, call_expr)
            }
            
            Builtin::VectorFill => {
                // (vector-fill! vec fill) - fill vector with value
                extract_args!(self, args, vec, fill);
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
                        for i in 0..len {
                            self.lisp.array_set(vec, i, fill)?;
                        }
                        // R7RS: returns unspecified, we return the vector
                        Ok(vec)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::VectorCopy => {
                // (vector-copy vec) - copy a vector
                let vec = self.lisp.car(args)?;
                
                match self.lisp.get(vec)? {
                    Value::Array { .. } => {
                        let len = self.lisp.array_len(vec)?;
                        // Create new vector with same length
                        let placeholder = self.lisp.number(0)?;
                        let new_vec = self.lisp.make_array(len, placeholder)?;
                        
                        // Copy elements
                        for i in 0..len {
                            let elem = self.lisp.array_get(vec, i)?;
                            self.lisp.array_set(new_vec, i, elem)?;
                        }
                        
                        Ok(new_vec)
                    }
                    _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                }
            }
            
            Builtin::Gc => {
                // (gc) - Manually trigger garbage collection
                // Returns a list: (marked collected total-before)
                // Built right-to-left since cons prepends
                let stats = self.gc();
                let marked = self.lisp.number(stats.marked as isize)?;
                let collected = self.lisp.number(stats.collected as isize)?;
                let total_before = self.lisp.number(stats.total_before as isize)?;
                let nil = self.lisp.nil()?;
                let list = self.lisp.cons(total_before, nil)?;
                let list = self.lisp.cons(collected, list)?;
                let list = self.lisp.cons(marked, list)?;
                Ok(list)
            }
            
            Builtin::GcEnable => {
                // (gc-enable) - Enable automatic garbage collection
                self.lisp.arena().set_gc_enabled(true);
                self.lisp.true_val().map_err(Into::into)
            }
            
            Builtin::GcDisable => {
                // (gc-disable) - Disable automatic garbage collection
                self.lisp.arena().set_gc_enabled(false);
                self.lisp.false_val().map_err(Into::into)
            }
            
            Builtin::GcEnabledP => {
                // (gc-enabled?) - Check if GC is enabled
                let enabled = self.lisp.arena().is_gc_enabled();
                self.lisp.boolean(enabled).map_err(Into::into)
            }
            
            Builtin::ArenaStats => {
                // (arena-stats) - Get arena statistics
                // Returns a list: (capacity allocated free usage-percent)
                let stats = self.lisp.stats();
                let capacity = self.lisp.number(stats.capacity as isize)?;
                let allocated = self.lisp.number(stats.allocated as isize)?;
                let free = self.lisp.number(stats.free as isize)?;
                let usage = self.lisp.number(stats.usage_percent() as isize)?;
                let nil = self.lisp.nil()?;
                let list = self.lisp.cons(usage, nil)?;
                let list = self.lisp.cons(free, list)?;
                let list = self.lisp.cons(allocated, list)?;
                let list = self.lisp.cons(capacity, list)?;
                Ok(list)
            }
            
            // ============================================================
            // Character operations (R7RS Section 6.6)
            // ============================================================
            
            Builtin::Charp => {
                // (char? obj) - Check if value is a character
                builtin_unary_pred!(self, args, |v: Value| matches!(v, Value::Char(_)))
            }
            
            Builtin::CharEq => {
                // (char=? char1 char2 ...) - Character equality
                self.char_chain_compare(args, |a, b| a == b, call_expr)
            }
            
            Builtin::CharLt => {
                // (char<? char1 char2 ...) - Monotonically increasing
                self.char_chain_compare(args, |a, b| a < b, call_expr)
            }
            
            Builtin::CharGt => {
                // (char>? char1 char2 ...) - Monotonically decreasing
                self.char_chain_compare(args, |a, b| a > b, call_expr)
            }
            
            Builtin::CharLe => {
                // (char<=? char1 char2 ...) - Monotonically non-decreasing
                self.char_chain_compare(args, |a, b| a <= b, call_expr)
            }
            
            Builtin::CharGe => {
                // (char>=? char1 char2 ...) - Monotonically non-increasing
                self.char_chain_compare(args, |a, b| a >= b, call_expr)
            }
            
            Builtin::CharToInteger => builtin_char_to_int!(self, args, call_expr),
            
            Builtin::IntegerToChar => {
                // (integer->char n) - Convert Unicode code point to char
                let n = self.get_int(self.lisp.car(args)?, call_expr)?;
                if n < 0 {
                    return Err(self.type_error(call_expr, "non-negative integer", "negative integer"));
                }
                match char::from_u32(n as u32) {
                    Some(c) => self.lisp.char(c).map_err(Into::into),
                    None => Err(self.type_error(call_expr, "valid Unicode code point", "invalid code point")),
                }
            }
            
            Builtin::CharUpcase => builtin_char_transform!(self, args, call_expr, 'a', 'z', b'a', b'A'),
            Builtin::CharDowncase => builtin_char_transform!(self, args, call_expr, 'A', 'Z', b'A', b'a'),
            
            // ============================================================
            // String operations (R7RS Section 6.7)
            // ============================================================
            
            Builtin::Stringp => {
                // (string? obj) - Check if value is a string
                builtin_unary_pred!(self, args, |v: Value| matches!(v, Value::String { .. }))
            }
            
            Builtin::MakeString => {
                // (make-string k) or (make-string k char)
                let k = self.get_int(self.lisp.car(args)?, call_expr)?;
                if k < 0 {
                    return Err(self.type_error(call_expr, "non-negative integer", "negative integer"));
                }
                let rest = self.lisp.cdr(args)?;
                let fill = if self.lisp.get(rest)?.is_nil() {
                    ' ' // Default fill character
                } else {
                    self.get_char(self.lisp.car(rest)?, call_expr)?
                };
                
                // Create string of k characters
                const MAX_MAKE_STRING_LEN: usize = 1024;
                let len = k as usize;
                if len > MAX_MAKE_STRING_LEN {
                    return Err(self.make_error(ErrorKind::TypeError, call_expr));
                }
                let mut chars = ['\0'; MAX_MAKE_STRING_LEN];
                chars[..len].fill(fill);
                self.lisp.string_from_chars(&chars[..len]).map_err(Into::into)
            }
            
            Builtin::String => {
                // (string char ...) - Create string from characters
                const MAX_STRING_LEN: usize = 1024;
                let mut chars = ['\0'; MAX_STRING_LEN];
                let mut len = 0;
                let mut current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            if len >= MAX_STRING_LEN {
                                return Err(self.make_error(ErrorKind::TypeError, call_expr));
                            }
                            chars[len] = self.get_char(car, call_expr)?;
                            len += 1;
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
                self.lisp.string_from_chars(&chars[..len]).map_err(Into::into)
            }
            
            Builtin::StringLength => {
                // (string-length string) - Get length
                let str_idx = self.lisp.car(args)?;
                match self.lisp.get(str_idx)? {
                    Value::String { .. } => {
                        let len = self.lisp.string_len(str_idx)?;
                        self.lisp.number(len as isize).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringRef => {
                // (string-ref string k) - Get character at index
                extract_args!(self, args, str_idx, k_idx);
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let k = self.get_int(k_idx, call_expr)?;
                        if k < 0 || (k as usize) >= len {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        // Characters start at data (no header with inline length)
                        let char_slot = self.lisp.arena_index_at_offset(data, k as usize)?;
                        match self.lisp.get(char_slot)? {
                            Value::Char(c) => self.lisp.char(c).map_err(Into::into),
                            _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
                        }
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringSet => {
                // (string-set! string k char) - Set character at index
                let str_idx = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let k_idx = self.lisp.car(rest)?;
                let rest2 = self.lisp.cdr(rest)?;
                let char_arg = self.lisp.car(rest2)?;
                
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let k = self.get_int(k_idx, call_expr)?;
                        if k < 0 || (k as usize) >= len {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        let c = self.get_char(char_arg, call_expr)?;
                        // Copy-on-write: if the data is shared (interned), make a private copy
                        if self.lisp.string_data_is_interned(data)? {
                            self.lisp.string_copy_data(str_idx)?;
                        }
                        // Re-read data after potential COW
                        let current_data = match self.lisp.get(str_idx)? {
                            Value::String { data: d, .. } => d,
                            _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                        };
                        // Characters start at data (no header with inline length)
                        let char_slot = self.lisp.arena_index_at_offset(current_data, k as usize)?;
                        self.lisp.set(char_slot, Value::Char(c))?;
                        // Return unspecified value (we use the string itself)
                        Ok(str_idx)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringEq => {
                // (string=? string1 string2 ...) - String equality
                self.string_chain_compare(args, |ordering| ordering == core::cmp::Ordering::Equal, call_expr)
            }
            
            Builtin::StringLt => {
                // (string<? string1 string2 ...) - Monotonically increasing
                self.string_chain_compare(args, |ordering| ordering == core::cmp::Ordering::Less, call_expr)
            }
            
            Builtin::StringGt => {
                // (string>? string1 string2 ...) - Monotonically decreasing
                self.string_chain_compare(args, |ordering| ordering == core::cmp::Ordering::Greater, call_expr)
            }
            
            Builtin::StringLe => {
                // (string<=? string1 string2 ...) - Monotonically non-decreasing
                self.string_chain_compare(args, |ordering| ordering != core::cmp::Ordering::Greater, call_expr)
            }
            
            Builtin::StringGe => {
                // (string>=? string1 string2 ...) - Monotonically non-increasing
                self.string_chain_compare(args, |ordering| ordering != core::cmp::Ordering::Less, call_expr)
            }
            
            Builtin::StringAppend => {
                // (string-append string ...) - Concatenate strings
                const MAX_TOTAL_LEN: usize = 4096;
                let mut chars = ['\0'; MAX_TOTAL_LEN];
                let mut total_len = 0;
                
                let mut current = args;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            match self.lisp.get(car)? {
                                Value::String { len, data } => {
                                    if total_len + len > MAX_TOTAL_LEN {
                                        return Err(self.make_error(ErrorKind::TypeError, call_expr));
                                    }
                                    for i in 0..len {
                                        // Characters start at data (no header with inline length)
                                        let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                                        match self.lisp.get(char_slot)? {
                                            Value::Char(c) => {
                                                chars[total_len] = c;
                                                total_len += 1;
                                            }
                                            _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                                        }
                                    }
                                }
                                v => return Err(self.type_error(call_expr, "string", v.type_name())),
                            }
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
                
                self.lisp.string_from_chars(&chars[..total_len]).map_err(Into::into)
            }
            
            Builtin::StringToList => {
                // (string->list string) - Convert string to list of characters
                let str_idx = self.lisp.car(args)?;
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let mut result = self.lisp.nil()?;
                        // Build list from end to start
                        for i in (0..len).rev() {
                            // Characters start at data (no header with inline length)
                            let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                            match self.lisp.get(char_slot)? {
                                Value::Char(c) => {
                                    let char_val = self.lisp.char(c)?;
                                    result = self.lisp.cons(char_val, result)?;
                                }
                                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                            }
                        }
                        Ok(result)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::ListToString => {
                // (list->string list) - Convert list of characters to string
                const MAX_STRING_LEN: usize = 1024;
                let mut chars = ['\0'; MAX_STRING_LEN];
                let mut len = 0;
                
                let mut current = self.lisp.car(args)?;
                loop {
                    match self.lisp.get(current)? {
                        Value::Nil => break,
                        Value::Cons { .. } => {
                            let car = self.lisp.car(current)?;
                            let cdr = self.lisp.cdr(current)?;
                            if len >= MAX_STRING_LEN {
                                return Err(self.make_error(ErrorKind::TypeError, call_expr));
                            }
                            chars[len] = self.get_char(car, call_expr)?;
                            len += 1;
                            current = cdr;
                        }
                        _ => return Err(self.make_error(ErrorKind::TypeError, current)),
                    }
                }
                
                self.lisp.string_from_chars(&chars[..len]).map_err(Into::into)
            }
            
            Builtin::Substring => {
                // (substring string start end) - Extract substring
                let str_idx = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let start_idx = self.lisp.car(rest)?;
                let rest2 = self.lisp.cdr(rest)?;
                let end_idx = self.lisp.car(rest2)?;
                
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        let start = self.get_int(start_idx, call_expr)?;
                        let end = self.get_int(end_idx, call_expr)?;
                        
                        if start < 0 || end < 0 || (start as usize) > len || (end as usize) > len || start > end {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        
                        let sub_len = (end - start) as usize;
                        const MAX_SUBSTRING_LEN: usize = 1024;
                        let mut chars = ['\0'; MAX_SUBSTRING_LEN];
                        if sub_len > MAX_SUBSTRING_LEN {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        
                        for i in 0..sub_len {
                            // Characters start at data (no header with inline length)
                            let char_slot = self.lisp.arena_index_at_offset(data, (start as usize) + i)?;
                            match self.lisp.get(char_slot)? {
                                Value::Char(c) => chars[i] = c,
                                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                            }
                        }
                        
                        self.lisp.string_from_chars(&chars[..sub_len]).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            Builtin::StringCopy => {
                // (string-copy string) - Copy a string
                let str_idx = self.lisp.car(args)?;
                match self.lisp.get(str_idx)? {
                    Value::String { len, data } => {
                        const MAX_STRING_LEN: usize = 1024;
                        let mut chars = ['\0'; MAX_STRING_LEN];
                        if len > MAX_STRING_LEN {
                            return Err(self.make_error(ErrorKind::TypeError, call_expr));
                        }
                        
                        for i in 0..len {
                            // Characters start at data (no header with inline length)
                            let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                            match self.lisp.get(char_slot)? {
                                Value::Char(c) => chars[i] = c,
                                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
                            }
                        }
                        
                        self.lisp.string_from_chars(&chars[..len]).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }
            
            // ============================================================
            // Syntax-case support (R6RS Chapter 11)
            // ============================================================
            
            Builtin::Identifierp => {
                // (identifier? x) - Check if x is an identifier
                // An identifier is either a symbol or a syntax object wrapping a symbol
                let arg = self.lisp.car(args)?;
                let is_id = match self.lisp.get(arg)? {
                    Value::Symbol(_) => true,
                    Value::Syntax { expr, .. } => {
                        matches!(self.lisp.get(expr)?, Value::Symbol(_))
                    }
                    _ => false,
                };
                self.lisp.boolean(is_id).map_err(Into::into)
            }
            
            Builtin::BoundIdentifierEq => {
                // (bound-identifier=? id1 id2) - Check if two identifiers have same name and marks
                extract_args!(self, args, id1, id2);
                let result = self.bound_identifier_eq(id1, id2)?;
                self.lisp.boolean(result).map_err(Into::into)
            }
            
            Builtin::FreeIdentifierEq => {
                // (free-identifier=? id1 id2) - Check if two identifiers resolve to same binding
                extract_args!(self, args, id1, id2);
                let result = self.free_identifier_eq(id1, id2)?;
                self.lisp.boolean(result).map_err(Into::into)
            }
            
            Builtin::SyntaxToDatum | Builtin::SyntaxE | Builtin::SyntaxObjectToDatum => {
                // syntax->datum / syntax-e / syntax-object->datum - Strip syntax wrapper
                let stx = self.lisp.car(args)?;
                self.syntax_to_datum_recursive(stx)
            }
            
            Builtin::DatumToSyntax | Builtin::DatumToSyntaxObject => {
                // datum->syntax / datum->syntax-object - Wrap datum with syntax context
                extract_args!(self, args, template_id, datum);
                self.datum_to_syntax(template_id, datum)
            }
            
            Builtin::GenerateTemporaries => {
                // (generate-temporaries list) - Generate a list of fresh identifiers
                // For each element in the input list, generate a unique temporary identifier
                let input = self.lisp.car(args)?;
                self.generate_temporaries(input)
            }
            
            Builtin::SymbolToString => {
                // (symbol->string sym) - Convert symbol to string
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Symbol(chars) => {
                        // The symbol stores a reference to a String value
                        // We return that string directly
                        Ok(chars)
                    }
                    _ => Err(self.type_error(call_expr, "symbol", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::StringToSymbol => {
                // (string->symbol str) - Convert string to symbol
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::String { .. } => {
                        // Create/intern a symbol with this string
                        self.lisp.symbol_from_string(arg).map_err(Into::into)
                    }
                    _ => Err(self.type_error(call_expr, "string", self.lisp.get(arg)?.type_name())),
                }
            }
            
            Builtin::NumberToString => {
                // (number->string num) - Convert number to string
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Number(n) => {
                        // Convert integer to string using stack buffer (no_std compatible)
                        let mut buf = [0u8; 21]; // enough for i64 including sign
                        let mut pos = buf.len();
                        let negative = n < 0;
                        let mut val = if negative { 
                            // Handle isize::MIN by working with unsigned
                            (n as isize).unsigned_abs()
                        } else { 
                            n as usize 
                        };
                        
                        if val == 0 {
                            pos -= 1;
                            buf[pos] = b'0';
                        } else {
                            while val > 0 {
                                pos -= 1;
                                buf[pos] = b'0' + (val % 10) as u8;
                                val /= 10;
                            }
                        }
                        
                        if negative {
                            pos -= 1;
                            buf[pos] = b'-';
                        }
                        
                        let len = buf.len() - pos;
                        let mut chars = ['\0'; 21];
                        for i in 0..len {
                            chars[i] = buf[pos + i] as char;
                        }
                        self.lisp.string_from_chars(&chars[..len]).map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "number", v.type_name())),
                }
            }
            
            Builtin::StringToNumber => {
                // (string->number str) - Convert string to number, or #f if invalid
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::String { len, data } => {
                        if len == 0 {
                            return self.lisp.false_val().map_err(Into::into);
                        }
                        // Read chars into stack buffer
                        let mut buf = [0u8; 21];
                        if len > buf.len() {
                            return self.lisp.false_val().map_err(Into::into);
                        }
                        for i in 0..len {
                            let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                            match self.lisp.get(char_slot)? {
                                Value::Char(c) => {
                                    if !c.is_ascii() {
                                        return self.lisp.false_val().map_err(Into::into);
                                    }
                                    buf[i] = c as u8;
                                }
                                _ => return self.lisp.false_val().map_err(Into::into),
                            }
                        }
                        // Parse the number
                        let s = core::str::from_utf8(&buf[..len]).unwrap_or("");
                        match s.parse::<isize>() {
                            Ok(n) => self.lisp.number(n).map_err(Into::into),
                            Err(_) => self.lisp.false_val().map_err(Into::into),
                        }
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }

            // ================================================================
            // Port operations (R7RS §6.13)
            // ================================================================

            Builtin::Portp => {
                let arg = self.lisp.car(args)?;
                let is_port = matches!(self.lisp.get(arg)?, Value::Port(_));
                self.lisp.boolean(is_port).map_err(Into::into)
            }

            Builtin::InputPortp => {
                let arg = self.lisp.car(args)?;
                let result = match self.lisp.get(arg)? {
                    Value::Port(pid) => {
                        if let Some(ref io) = self.io { io.is_input_port(pid) } else { false }
                    }
                    _ => false,
                };
                self.lisp.boolean(result).map_err(Into::into)
            }

            Builtin::OutputPortp => {
                let arg = self.lisp.car(args)?;
                let result = match self.lisp.get(arg)? {
                    Value::Port(pid) => {
                        if let Some(ref io) = self.io { io.is_output_port(pid) } else { false }
                    }
                    _ => false,
                };
                self.lisp.boolean(result).map_err(Into::into)
            }

            Builtin::CurrentInputPort => {
                self.lisp.port(grift_parser::PortId::STDIN).map_err(Into::into)
            }

            Builtin::CurrentOutputPort => {
                self.lisp.port(grift_parser::PortId::STDOUT).map_err(Into::into)
            }

            Builtin::CurrentErrorPort => {
                self.lisp.port(grift_parser::PortId::STDERR).map_err(Into::into)
            }

            Builtin::ClosePort | Builtin::CloseInputPort | Builtin::CloseOutputPort => {
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Port(pid) => {
                        if let Some(ref mut io) = self.io {
                            io.close_port(pid).map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        }
                        self.lisp.void_val().map_err(Into::into)
                    }
                    v => Err(self.type_error(call_expr, "port", v.type_name())),
                }
            }

            Builtin::ReadChar => {
                // (read-char) or (read-char port)
                let pid = self.extract_input_port(args, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        match io.read_char(pid) {
                            Ok(c) => self.lisp.alloc(Value::Char(c)).map_err(Into::into),
                            Err(grift_parser::IoErrorKind::Eof) => self.lisp.eof().map_err(Into::into),
                            Err(_) => Err(self.make_error(ErrorKind::Generic, call_expr)),
                        }
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::PeekChar => {
                // (peek-char) or (peek-char port)
                let pid = self.extract_input_port(args, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        match io.peek_char(pid) {
                            Ok(c) => self.lisp.alloc(Value::Char(c)).map_err(Into::into),
                            Err(grift_parser::IoErrorKind::Eof) => self.lisp.eof().map_err(Into::into),
                            Err(_) => Err(self.make_error(ErrorKind::Generic, call_expr)),
                        }
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::CharReadyp => {
                // (char-ready?) or (char-ready? port)
                let pid = self.extract_input_port(args, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        let ready = io.char_ready(pid).unwrap_or(false);
                        self.lisp.boolean(ready).map_err(Into::into)
                    }
                    None => self.lisp.boolean(false).map_err(Into::into),
                }
            }

            Builtin::WriteChar => {
                // (write-char char) or (write-char char port)
                let ch_arg = self.lisp.car(args)?;
                let c = match self.lisp.get(ch_arg)? {
                    Value::Char(c) => c,
                    v => return Err(self.type_error(call_expr, "char", v.type_name())),
                };
                let rest = self.lisp.cdr(args)?;
                let pid = if self.lisp.get(rest)?.is_nil() {
                    grift_parser::PortId::STDOUT
                } else {
                    let port_arg = self.lisp.car(rest)?;
                    match self.lisp.get(port_arg)? {
                        Value::Port(pid) => pid,
                        v => return Err(self.type_error(call_expr, "port", v.type_name())),
                    }
                };
                if let Some(ref mut io) = self.io {
                    io.write_char(pid, c).map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                } else if let Some(callback) = self.output_callback {
                    callback(self.lisp, ch_arg);
                }
                self.lisp.void_val().map_err(Into::into)
            }

            Builtin::Write => {
                // (write obj) or (write obj port)
                let val = self.lisp.car(args)?;
                let rest = self.lisp.cdr(args)?;
                let pid = if self.lisp.get(rest)?.is_nil() {
                    grift_parser::PortId::STDOUT
                } else {
                    let port_arg = self.lisp.car(rest)?;
                    match self.lisp.get(port_arg)? {
                        Value::Port(pid) => pid,
                        v => return Err(self.type_error(call_expr, "port", v.type_name())),
                    }
                };
                if let Some(ref mut io) = self.io {
                    // Use DisplayValue (write mode with quotes) through a fmt::Write adapter
                    let dv = grift_parser::DisplayValue::new(val, self.lisp);
                    let mut writer = IoPortWriter { io: &mut **io, port: pid, error: false };
                    use core::fmt::Write;
                    let _ = write!(writer, "{}", dv);
                    if writer.error {
                        return Err(self.make_error(ErrorKind::Generic, call_expr));
                    }
                } else if let Some(callback) = self.output_callback {
                    callback(self.lisp, val);
                }
                self.lisp.void_val().map_err(Into::into)
            }

            Builtin::Read => {
                // (read) or (read port)
                let pid = self.extract_input_port(args, call_expr)?;
                self.apply_read_builtin(pid, call_expr)
            }

            Builtin::EofObject => {
                self.lisp.eof().map_err(Into::into)
            }

            Builtin::EofObjectp => {
                let arg = self.lisp.car(args)?;
                let is_eof = matches!(self.lisp.get(arg)?, Value::Eof);
                self.lisp.boolean(is_eof).map_err(Into::into)
            }

            Builtin::OpenInputString => {
                // (open-input-string str)
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::String { len, data } => {
                        // Extract string content into stack buffer
                        let mut buf = [0u8; 1024];
                        let mut byte_len = 0;
                        for i in 0..len {
                            let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                            if let Value::Char(c) = self.lisp.get(char_slot)? {
                                let enc_len = c.len_utf8();
                                if byte_len + enc_len > buf.len() { break; }
                                c.encode_utf8(&mut buf[byte_len..]);
                                byte_len += enc_len;
                            }
                        }
                        let s = core::str::from_utf8(&buf[..byte_len])
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        match &mut self.io {
                            Some(io) => {
                                let pid = io.open_input_string(s)
                                    .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                                self.lisp.port(pid).map_err(Into::into)
                            }
                            None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                        }
                    }
                    v => Err(self.type_error(call_expr, "string", v.type_name())),
                }
            }

            Builtin::OpenOutputString => {
                // (open-output-string)
                match &mut self.io {
                    Some(io) => {
                        let pid = io.open_output_string()
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        self.lisp.port(pid).map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::GetOutputString => {
                // (get-output-string port)
                let arg = self.lisp.car(args)?;
                match self.lisp.get(arg)? {
                    Value::Port(pid) => {
                        match &self.io {
                            Some(io) => {
                                let s = io.get_output_string(pid)
                                    .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                                self.lisp.string(s).map_err(Into::into)
                            }
                            None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                        }
                    }
                    v => Err(self.type_error(call_expr, "port", v.type_name())),
                }
            }

            // ================================================================
            // File system operations (R7RS §6.13)
            // ================================================================

            Builtin::FileExistsP => {
                // (file-exists? filename)
                let arg = self.lisp.car(args)?;
                let path = self.extract_string_arg(arg, call_expr)?;
                match &self.io {
                    Some(io) => {
                        let exists = io.file_exists(&path)
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        self.lisp.boolean(exists).map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::DeleteFile => {
                // (delete-file filename)
                let arg = self.lisp.car(args)?;
                let path = self.extract_string_arg(arg, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        io.delete_file(&path)
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        self.lisp.void_val().map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::Load => {
                // (load filename) — handled via trampolining in apply_builtin_trampolined
                // This fallback should not be reached; see apply_load_builtin.
                Err(self.make_error(ErrorKind::Generic, call_expr))
            }

            // ================================================================
            // Process / environment operations (R7RS §6.14)
            // ================================================================

            Builtin::CommandLine => {
                // (command-line) -> list of strings
                match &self.io {
                    Some(io) => {
                        let count = io.command_line_count()
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                        // Build list in reverse then reverse it
                        let mut list = self.lisp.nil()?;
                        for i in (0..count).rev() {
                            let s = io.command_line_arg(i)
                                .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;
                            let str_val = self.lisp.string(s)?;
                            list = self.lisp.cons(str_val, list)?;
                        }
                        Ok(list)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::Exit => {
                // (exit) or (exit obj)
                let code = self.extract_exit_code(args, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        let _ = io.exit_process(code);
                        // If the io provider doesn't actually exit (e.g. in tests),
                        // return void
                        self.lisp.void_val().map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::EmergencyExit => {
                // (emergency-exit) or (emergency-exit obj)
                let code = self.extract_exit_code(args, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        let _ = io.emergency_exit_process(code);
                        self.lisp.void_val().map_err(Into::into)
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::GetEnvironmentVariable => {
                // (get-environment-variable name) -> string or #f
                let arg = self.lisp.car(args)?;
                let name = self.extract_string_arg(arg, call_expr)?;
                match &mut self.io {
                    Some(io) => {
                        match io.get_environment_variable(&name) {
                            Ok(Some(val)) => self.lisp.string(val).map_err(Into::into),
                            Ok(None) => self.lisp.false_val().map_err(Into::into),
                            Err(_) => Err(self.make_error(ErrorKind::Generic, call_expr)),
                        }
                    }
                    None => Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }

            Builtin::GetEnvironmentVariables => {
                // (get-environment-variables) -> alist of (name . value)
                // First, get count (requires &mut io)
                let count = match &mut self.io {
                    Some(io) => io.environment_variables_count()
                        .unwrap_or(0),
                    None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                };
                // Now build the alist by iterating (environment_variable_at uses &self)
                let mut list = self.lisp.nil()?;
                for i in (0..count).rev() {
                    let (name, value) = match &self.io {
                        Some(io) => io.environment_variable_at(i)
                            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?,
                        None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                    };
                    let name_str = self.lisp.string(name)?;
                    let value_str = self.lisp.string(value)?;
                    let pair = self.lisp.cons(name_str, value_str)?;
                    list = self.lisp.cons(pair, list)?;
                }
                Ok(list)
            }
        }
    }
    
    /// OPTIMIZED: Apply binary builtin directly without list allocation
    pub(super) fn apply_binary_builtin(&mut self, builtin: Builtin, a: ArenaIndex, b: ArenaIndex, call_expr: ArenaIndex) 
        -> EvalResult 
    {
        match builtin {
            // Arithmetic with overflow check
            Builtin::Add => binary_int_op!(self, a, b, call_expr, |x: isize, y: isize| x.checked_add(y)),
            Builtin::Sub => binary_int_op!(self, a, b, call_expr, |x: isize, y: isize| x.checked_sub(y)),
            Builtin::Mul => binary_int_op!(self, a, b, call_expr, |x: isize, y: isize| x.checked_mul(y)),
            
            // Division operations with zero check
            Builtin::Div => binary_div_op!(self, a, b, call_expr, |x, y| x / y),
            Builtin::Modulo => binary_div_op!(self, a, b, call_expr, |x, y| ((x % y) + y) % y),
            Builtin::Remainder => binary_div_op!(self, a, b, call_expr, |x, y| x % y),
            
            // Comparisons
            Builtin::Lt => binary_int_cmp!(self, a, b, call_expr, |x, y| x < y),
            Builtin::Gt => binary_int_cmp!(self, a, b, call_expr, |x, y| x > y),
            Builtin::Le => binary_int_cmp!(self, a, b, call_expr, |x, y| x <= y),
            Builtin::Ge => binary_int_cmp!(self, a, b, call_expr, |x, y| x >= y),
            Builtin::NumEq => binary_int_cmp!(self, a, b, call_expr, |x, y| x == y),
            Builtin::EqP | Builtin::EqvP => {
                let val_a = self.lisp.get(a)?;
                let val_b = self.lisp.get(b)?;
                
                let eq = match (val_a, val_b) {
                    (Value::Nil, Value::Nil) => true,
                    (Value::True, Value::True) => true,
                    (Value::False, Value::False) => true,
                    (Value::Number(x), Value::Number(y)) => x == y,
                    (Value::Char(x), Value::Char(y)) => x == y,
                    (Value::Symbol(_), Value::Symbol(_)) => self.lisp.symbol_eq(a, b)?,
                    (Value::String { len: la, data: da }, Value::String { len: lb, data: db }) => {
                        a == b || (la == lb && da == db)
                    }
                    _ => a == b,
                };
                
                self.lisp.boolean(eq).map_err(Into::into)
            }
            Builtin::Cons => {
                self.lisp.cons(a, b).map_err(Into::into)
            }
            // For other builtins, fall back to list-based approach
            _ => {
                let rest = self.lisp.cons(b, self.lisp.nil()?)?;
                let args = self.lisp.cons(a, rest)?;
                self.apply_builtin(builtin, args, call_expr)
            }
        }
    }
    
    /// Get integer from already-evaluated value
    pub(super) fn get_int(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<isize, EvalError> {
        match self.lisp.get(idx)? {
            Value::Number(n) => Ok(n),
            v => Err(self.type_error(call_expr, "integer", v.type_name())),
        }
    }
    
    /// Get character from already-evaluated value
    pub(super) fn get_char(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<char, EvalError> {
        match self.lisp.get(idx)? {
            Value::Char(c) => Ok(c),
            v => Err(self.type_error(call_expr, "char", v.type_name())),
        }
    }
    
    /// Numeric fold with already-evaluated integer args
    pub(super) fn numeric_fold<F>(&self, args: ArenaIndex, init: isize, int_f: F, call_expr: ArenaIndex) -> EvalResult
    where 
        F: Fn(isize, isize) -> Option<isize>,
    {
        let mut acc = init;
        let mut current = args;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.lisp.number(acc).map_err(Into::into),
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    let n = self.get_int(car, call_expr)?;
                    acc = match int_f(acc, n) {
                        Some(r) => r,
                        None => return Err(self.make_error(ErrorKind::DivisionByZero, call_expr)),
                    };
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Compare two integer numbers
    pub(super) fn compare_numbers<F>(&self, args: ArenaIndex, cmp: F, call_expr: ArenaIndex) -> EvalResult
    where F: Fn(isize, isize) -> bool
    {
        let a = self.get_int(self.lisp.car(args)?, call_expr)?;
        let b = self.get_int(self.lisp.car(self.lisp.cdr(args)?)?, call_expr)?;
        self.lisp.boolean(cmp(a, b)).map_err(Into::into)
    }
    
    /// Helper for character chain comparisons (char=?, char<?, etc.)
    pub(super) fn char_chain_compare<F>(&self, args: ArenaIndex, compare_fn: F, call_expr: ArenaIndex) -> EvalResult
    where
        F: Fn(char, char) -> bool
    {
        // Need at least 2 arguments
        let first = self.get_char(self.lisp.car(args)?, call_expr)?;
        let rest = self.lisp.cdr(args)?;
        
        if self.lisp.get(rest)?.is_nil() {
            return Err(self.type_error(call_expr, "at least 2 arguments", "1 argument"));
        }
        
        let mut prev = first;
        let mut current = rest;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.lisp.true_val().map_err(Into::into),
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    let c = self.get_char(car, call_expr)?;
                    if !compare_fn(prev, c) {
                        return self.lisp.false_val().map_err(Into::into);
                    }
                    prev = c;
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Helper for string chain comparisons (string=?, string<?, etc.)
    pub(super) fn string_chain_compare<F>(&self, args: ArenaIndex, compare_fn: F, call_expr: ArenaIndex) -> EvalResult
    where
        F: Fn(core::cmp::Ordering) -> bool
    {
        // Need at least 2 arguments
        let first_idx = self.lisp.car(args)?;
        let rest = self.lisp.cdr(args)?;
        
        if self.lisp.get(rest)?.is_nil() {
            return Err(self.type_error(call_expr, "at least 2 arguments", "1 argument"));
        }
        
        let mut prev_idx = first_idx;
        let mut current = rest;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => return self.lisp.true_val().map_err(Into::into),
                Value::Cons { .. } => {
                    let car = self.lisp.car(current)?;
                    let cdr = self.lisp.cdr(current)?;
                    let ordering = self.compare_strings(prev_idx, car, call_expr)?;
                    if !compare_fn(ordering) {
                        return self.lisp.false_val().map_err(Into::into);
                    }
                    prev_idx = car;
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, current)),
            }
        }
    }
    
    /// Compare two strings lexicographically
    pub(super) fn compare_strings(&self, a: ArenaIndex, b: ArenaIndex, call_expr: ArenaIndex) -> Result<core::cmp::Ordering, EvalError> {
        let (len_a, data_a) = match self.lisp.get(a)? {
            Value::String { len, data } => (len, data),
            v => return Err(self.type_error(call_expr, "string", v.type_name())),
        };
        let (len_b, data_b) = match self.lisp.get(b)? {
            Value::String { len, data } => (len, data),
            v => return Err(self.type_error(call_expr, "string", v.type_name())),
        };
        
        let min_len = len_a.min(len_b);
        
        for i in 0..min_len {
            let slot_a = self.lisp.arena_index_at_offset(data_a, i)?;
            let char_a = match self.lisp.get(slot_a)? {
                Value::Char(c) => c,
                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
            };
            let slot_b = self.lisp.arena_index_at_offset(data_b, i)?;
            let char_b = match self.lisp.get(slot_b)? {
                Value::Char(c) => c,
                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
            };
            
            match char_a.cmp(&char_b) {
                core::cmp::Ordering::Equal => {}
                ord => return Ok(ord),
            }
        }
        
        // All compared characters are equal, compare lengths
        Ok(len_a.cmp(&len_b))
    }
    
    /// Convert a proper list to an array (vector).
    /// Used by both `(vector obj ...)` and `(list->vector lst)`.
    fn list_to_array(&self, list: ArenaIndex, call_expr: ArenaIndex) -> EvalResult {
        // Count elements
        let mut count = 0usize;
        let mut current = list;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => break,
                Value::Cons { .. } => {
                    count += 1;
                    current = self.lisp.cdr(current)?;
                }
                _ => return Err(self.make_error(ErrorKind::TypeError, call_expr)),
            }
        }
        
        // Create vector and fill
        let placeholder = self.lisp.number(0)?;
        let vec = self.lisp.make_array(count, placeholder)?;
        current = list;
        for i in 0..count {
            let val = self.lisp.car(current)?;
            self.lisp.array_set(vec, i, val)?;
            current = self.lisp.cdr(current)?;
        }
        Ok(vec)
    }

    /// Extract an input port from optional arguments.
    /// Returns STDIN if no argument is provided.
    fn extract_input_port(&self, args: ArenaIndex, call_expr: ArenaIndex) -> Result<grift_parser::PortId, EvalError> {
        if self.lisp.get(args)?.is_nil() {
            Ok(grift_parser::PortId::STDIN)
        } else {
            let port_arg = self.lisp.car(args)?;
            match self.lisp.get(port_arg)? {
                Value::Port(pid) => Ok(pid),
                v => Err(self.type_error(call_expr, "port", v.type_name())),
            }
        }
    }

    /// Implement (read) by reading characters from a port and parsing.
    fn apply_read_builtin(&mut self, pid: grift_parser::PortId, call_expr: ArenaIndex) -> EvalResult {
        // Read characters into a stack buffer until we have a complete S-expression
        let mut buf = [0u8; 2048];
        let mut byte_len = 0;
        let mut paren_depth: i32 = 0;
        let mut in_string = false;
        let mut escape = false;
        let mut got_token = false;

        let io = match &mut self.io {
            Some(io) => io,
            None => return Err(self.make_error(ErrorKind::Generic, call_expr)),
        };

        // Skip leading whitespace
        loop {
            match io.read_char(pid) {
                Ok(c) => {
                    if !c.is_whitespace() {
                        // Put this char into the buffer
                        let enc_len = c.len_utf8();
                        if byte_len + enc_len > buf.len() {
                            return Err(self.make_error(ErrorKind::Generic, call_expr));
                        }
                        c.encode_utf8(&mut buf[byte_len..]);
                        byte_len += enc_len;

                        if c == '(' || c == '[' {
                            paren_depth += 1;
                        } else if c == '"' {
                            in_string = true;
                        } else if paren_depth == 0 {
                            got_token = true;
                        }
                        break;
                    }
                }
                Err(grift_parser::IoErrorKind::Eof) => {
                    return self.lisp.eof().map_err(Into::into);
                }
                Err(_) => return Err(self.make_error(ErrorKind::Generic, call_expr)),
            }
        }

        // Read remaining characters to form a complete expression
        if paren_depth > 0 || in_string {
            loop {
                match io.read_char(pid) {
                    Ok(c) => {
                        let enc_len = c.len_utf8();
                        if byte_len + enc_len > buf.len() {
                            return Err(self.make_error(ErrorKind::Generic, call_expr));
                        }
                        c.encode_utf8(&mut buf[byte_len..]);
                        byte_len += enc_len;

                        if in_string {
                            if escape {
                                escape = false;
                            } else if c == '\\' {
                                escape = true;
                            } else if c == '"' {
                                in_string = false;
                                if paren_depth == 0 { break; }
                            }
                        } else {
                            if c == '(' || c == '[' {
                                paren_depth += 1;
                            } else if c == ')' || c == ']' {
                                paren_depth -= 1;
                                if paren_depth == 0 { break; }
                            } else if c == '"' {
                                in_string = true;
                            }
                        }
                    }
                    Err(grift_parser::IoErrorKind::Eof) => break,
                    Err(_) => return Err(self.make_error(ErrorKind::Generic, call_expr)),
                }
            }
        } else if got_token {
            // Continue reading the token (number, symbol, etc.)
            loop {
                match io.peek_char(pid) {
                    Ok(c) if c.is_whitespace() || c == '(' || c == ')' || c == '[' || c == ']' => break,
                    Ok(c) => {
                        let _ = io.read_char(pid);
                        let enc_len = c.len_utf8();
                        if byte_len + enc_len > buf.len() { break; }
                        c.encode_utf8(&mut buf[byte_len..]);
                        byte_len += enc_len;
                    }
                    Err(_) => break,
                }
            }
        }

        let s = core::str::from_utf8(&buf[..byte_len])
            .map_err(|_| self.make_error(ErrorKind::Generic, call_expr))?;

        if s.is_empty() {
            return self.lisp.eof().map_err(Into::into);
        }

        // Parse the expression
        match grift_parser::parse(self.lisp, s) {
            Ok(expr) => Ok(expr),
            Err(_) => Err(self.make_error(ErrorKind::Generic, call_expr)),
        }
    }

    /// Extract a Scheme string value into a stack-allocated UTF-8 buffer.
    /// Returns a fixed-size array wrapper that can be used as `&str`.
    fn extract_string_arg(&self, idx: ArenaIndex, call_expr: ArenaIndex) -> Result<StackString, EvalError> {
        match self.lisp.get(idx)? {
            Value::String { len, data } => {
                let mut buf = [0u8; 1024];
                let mut byte_len = 0;
                for i in 0..len {
                    let char_slot = self.lisp.arena_index_at_offset(data, i)?;
                    if let Value::Char(c) = self.lisp.get(char_slot)? {
                        let enc_len = c.len_utf8();
                        if byte_len + enc_len > buf.len() { break; }
                        c.encode_utf8(&mut buf[byte_len..]);
                        byte_len += enc_len;
                    }
                }
                Ok(StackString { buf, len: byte_len })
            }
            v => Err(self.type_error(call_expr, "string", v.type_name())),
        }
    }

    /// Extract an exit code from optional arguments.
    /// - No args or #t: 0
    /// - #f: 1
    /// - integer: use directly
    fn extract_exit_code(&self, args: ArenaIndex, call_expr: ArenaIndex) -> Result<i32, EvalError> {
        if self.lisp.get(args)?.is_nil() {
            return Ok(0);
        }
        let arg = self.lisp.car(args)?;
        match self.lisp.get(arg)? {
            Value::True => Ok(0),
            Value::False => Ok(1),
            Value::Number(n) => Ok(n as i32),
            v => Err(self.type_error(call_expr, "integer or boolean", v.type_name())),
        }
    }
}

/// Stack-allocated UTF-8 string buffer for passing to IoProvider methods.
struct StackString {
    buf: [u8; 1024],
    len: usize,
}

impl core::ops::Deref for StackString {
    type Target = str;
    fn deref(&self) -> &str {
        // The buffer was constructed from valid UTF-8 chars
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

/// Adapter to write through an [`IoProvider`] port via [`core::fmt::Write`].
struct IoPortWriter<'a> {
    io: &'a mut dyn grift_parser::IoProvider,
    port: grift_parser::PortId,
    error: bool,
}

impl core::fmt::Write for IoPortWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.io.write_str(self.port, s).is_err() {
            self.error = true;
            Err(core::fmt::Error)
        } else {
            Ok(())
        }
    }
}
