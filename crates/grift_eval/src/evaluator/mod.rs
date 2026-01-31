//! Evaluator module - split into logical components.
//!
//! This module contains the Lisp evaluator with full trampolined TCO.

mod env;
mod eval;
mod special_forms;
mod builtins;

use grift_parser::{
    ArenaIndex, GcStats, Lisp, Value, Builtin, StdLib, parse, ParseError, ParseErrorKind,
};

use crate::error::{
    ErrorKind, ErrorMessage, StackFrame, EvalError, EvalResult,
    MAX_STACK_DEPTH, MAX_BACKTRACE,
};
use crate::continuation::{Cont, TrampolineState, is_binary_builtin, MAX_CONT_DEPTH, MAX_DATA_STACK};
use crate::helpers::{
    gcd_helper, int_pow, equal_recursive, case_matches,
};
use crate::native::{NativeRegistry, NativeFn, simple_hash};

// Re-export macros from lib.rs (they're defined there)
use crate::{extract_args, builtin_unary_pred, builtin_numeric_pred, builtin_int_identity, builtin_div_op};

// ============================================================================
// Evaluator
// ============================================================================

/// The Lisp evaluator with full trampolined TCO.
///
/// This evaluator uses continuation-passing style with an explicit stack,
/// enabling unlimited recursion depth without Rust stack overflow.
pub struct Evaluator<'a, const N: usize> {
    pub(crate) lisp: &'a Lisp<N>,
    /// Global environment
    pub(super) global_env: ArenaIndex,
    /// Call stack for error reporting
    pub(super) call_stack: [StackFrame; MAX_STACK_DEPTH],
    pub(super) call_stack_depth: usize,
    /// Continuation stack for full trampolining
    pub(super) cont_stack: [Cont; MAX_CONT_DEPTH],
    pub(super) cont_depth: usize,
    /// Data stack for continuation data (separate from arena for performance)
    /// Stores raw ArenaIndex values without Value::Ref wrapper
    pub(super) data_stack: [ArenaIndex; MAX_DATA_STACK],
    pub(super) data_stack_top: usize,
    /// Native function registry
    pub(super) native_registry: NativeRegistry<N>,
}

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Create a new evaluator with standard environment
    pub fn new(lisp: &'a Lisp<N>) -> Result<Self, EvalError> {
        let mut eval = Evaluator {
            lisp,
            global_env: ArenaIndex::NIL,
            call_stack: [StackFrame::default(); MAX_STACK_DEPTH],
            call_stack_depth: 0,
            cont_stack: [Cont::Done; MAX_CONT_DEPTH],
            cont_depth: 0,
            data_stack: [ArenaIndex::NIL; MAX_DATA_STACK],
            data_stack_top: 0,
            native_registry: NativeRegistry::new(),
        };
        
        // Initialize global environment with builtins
        eval.global_env = lisp.nil()?;
        
        for &builtin in Builtin::ALL {
            let name = lisp.symbol(builtin.name())?;
            let val = lisp.builtin(builtin)?;
            eval.global_env = eval.env_extend(eval.global_env, name, val)?;
        }
        
        // Register standard library functions
        // These are stored in static memory and parsed on-demand
        for &stdlib in StdLib::ALL {
            let name = lisp.symbol(stdlib.name())?;
            let val = lisp.stdlib(stdlib)?;
            eval.global_env = eval.env_extend(eval.global_env, name, val)?;
        }
        
        // Note: In Scheme, only #t and #f are the booleans. 
        // 'true' and 'false' are NOT predefined aliases.
        
        Ok(eval)
    }
    
    /// Get the Lisp context
    pub fn lisp(&self) -> &Lisp<N> {
        self.lisp
    }
    
    /// Get the global environment
    pub fn global_env(&self) -> ArenaIndex {
        self.global_env
    }
    
    /// Register a native Rust function that can be called from Lisp.
    ///
    /// The function will be bound to the given name in the global environment.
    ///
    /// # Example
    ///
    /// ```rust
    /// use grift_eval::{Lisp, Evaluator, EvalResult, ArenaIndex, NativeFn, FromLisp, ToLisp};
    ///
    /// let lisp: Lisp<10000> = Lisp::new();
    /// let mut eval = Evaluator::new(&lisp).unwrap();
    ///
    /// // Register a native function that adds 100 to a number
    /// fn add_hundred<const N: usize>(
    ///     eval: &mut Evaluator<N>,
    ///     args: ArenaIndex,
    /// ) -> EvalResult {
    ///     let n = i32::from_lisp(eval, eval.lisp().car(args)?)?;
    ///     (n + 100).to_lisp(eval)
    /// }
    ///
    /// eval.register_native("add-hundred", add_hundred).unwrap();
    ///
    /// // Call from Lisp
    /// let result = eval.eval_str("(add-hundred 42)").unwrap();
    /// assert_eq!(eval.lisp().get(result).unwrap().as_number(), Some(142));
    /// ```
    pub fn register_native(&mut self, name: &str, func: NativeFn<N>) -> Result<(), EvalError> {
        // Get hash for lookup
        let hash = simple_hash(name);
        
        // Register in native registry
        self.native_registry.register(name, func)?;
        
        // Create a Native value with the hash
        let native_val = self.lisp.native(hash)?;
        
        // Bind to name in global environment
        let name_sym = self.lisp.symbol(name)?;
        self.global_env = self.env_extend(self.global_env, name_sym, native_val)?;
        
        Ok(())
    }
    
    /// Get a reference to the native function registry.
    pub fn native_registry(&self) -> &NativeRegistry<N> {
        &self.native_registry
    }
    
    // ========================================================================
    // Stack Management
    // ========================================================================
    
    pub(super) fn push_frame(&mut self, expr: ArenaIndex, func: ArenaIndex) -> Result<(), EvalError> {
        if self.call_stack_depth >= MAX_STACK_DEPTH {
            return Err(self.make_error(ErrorKind::StackOverflow, expr));
        }
        self.call_stack[self.call_stack_depth] = StackFrame { expr, func };
        self.call_stack_depth += 1;
        Ok(())
    }
    
    pub(super) fn pop_frame(&mut self) {
        if self.call_stack_depth > 0 {
            self.call_stack_depth -= 1;
        }
    }
    
    pub(crate) fn make_error(&self, kind: ErrorKind, expr: ArenaIndex) -> EvalError {
        EvalError::new(kind)
            .with_expr(expr)
            .with_backtrace(&self.call_stack, self.call_stack_depth)
    }
    
    pub(crate) fn type_error(&self, expr: ArenaIndex, expected: &'static str, got: &'static str) -> EvalError {
        self.make_error(ErrorKind::TypeError, expr)
            .with_types(expected, got)
    }
    
    pub(super) fn arg_error(&self, expr: ArenaIndex, expected: usize, got: usize) -> EvalError {
        self.make_error(ErrorKind::WrongArgCount, expr)
            .with_args(expected, got)
    }
    
    // ========================================================================
    // Continuation Stack Management
    // ========================================================================
    
    /// Push a continuation onto the stack
    #[inline]
    pub(super) fn push_cont(&mut self, cont: Cont) -> Result<(), EvalError> {
        if self.cont_depth >= MAX_CONT_DEPTH {
            return Err(EvalError::new(ErrorKind::StackOverflow));
        }
        self.cont_stack[self.cont_depth] = cont;
        self.cont_depth += 1;
        Ok(())
    }
    
    /// Pop a continuation from the stack
    /// Also restores the data stack by popping the continuation's data
    #[inline]
    pub(super) fn pop_cont(&mut self) -> Cont {
        if self.cont_depth == 0 {
            Cont::Done
        } else {
            self.cont_depth -= 1;
            let cont = self.cont_stack[self.cont_depth];
            // Restore data stack - pop this continuation's data
            let data_len = cont.data_len();
            if data_len > 0 {
                self.data_stack_top -= data_len;
            }
            cont
        }
    }
    
    // ========================================================================
    // Data Stack Operations
    // ========================================================================
    
    #[inline]
    pub(super) fn data_push(&mut self, val: ArenaIndex) -> Result<(), EvalError> {
        if self.data_stack_top >= MAX_DATA_STACK {
            return Err(EvalError::new(ErrorKind::StackOverflow));
        }
        self.data_stack[self.data_stack_top] = val;
        self.data_stack_top += 1;
        Ok(())
    }
    
    #[inline]
    pub(super) fn data_push2(&mut self, a: ArenaIndex, b: ArenaIndex) -> Result<(), EvalError> {
        if self.data_stack_top + 1 >= MAX_DATA_STACK {
            return Err(EvalError::new(ErrorKind::StackOverflow));
        }
        self.data_stack[self.data_stack_top] = a;
        self.data_stack[self.data_stack_top + 1] = b;
        self.data_stack_top += 2;
        Ok(())
    }
    
    #[inline]
    pub(super) fn data_push3(&mut self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex) -> Result<(), EvalError> {
        if self.data_stack_top + 2 >= MAX_DATA_STACK {
            return Err(EvalError::new(ErrorKind::StackOverflow));
        }
        self.data_stack[self.data_stack_top] = a;
        self.data_stack[self.data_stack_top + 1] = b;
        self.data_stack[self.data_stack_top + 2] = c;
        self.data_stack_top += 3;
        Ok(())
    }
    
    #[inline]
    pub(super) fn data_push4(&mut self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex) -> Result<(), EvalError> {
        if self.data_stack_top + 3 >= MAX_DATA_STACK {
            return Err(EvalError::new(ErrorKind::StackOverflow));
        }
        self.data_stack[self.data_stack_top] = a;
        self.data_stack[self.data_stack_top + 1] = b;
        self.data_stack[self.data_stack_top + 2] = c;
        self.data_stack[self.data_stack_top + 3] = d;
        self.data_stack_top += 4;
        Ok(())
    }
    
    #[inline]
    pub(super) fn data_push5(&mut self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex, e: ArenaIndex) -> Result<(), EvalError> {
        if self.data_stack_top + 4 >= MAX_DATA_STACK {
            return Err(EvalError::new(ErrorKind::StackOverflow));
        }
        self.data_stack[self.data_stack_top] = a;
        self.data_stack[self.data_stack_top + 1] = b;
        self.data_stack[self.data_stack_top + 2] = c;
        self.data_stack[self.data_stack_top + 3] = d;
        self.data_stack[self.data_stack_top + 4] = e;
        self.data_stack_top += 5;
        Ok(())
    }
    
    #[inline]
    pub(super) fn data_push6(&mut self, a: ArenaIndex, b: ArenaIndex, c: ArenaIndex, d: ArenaIndex, e: ArenaIndex, f: ArenaIndex) -> Result<(), EvalError> {
        if self.data_stack_top + 5 >= MAX_DATA_STACK {
            return Err(EvalError::new(ErrorKind::StackOverflow));
        }
        self.data_stack[self.data_stack_top] = a;
        self.data_stack[self.data_stack_top + 1] = b;
        self.data_stack[self.data_stack_top + 2] = c;
        self.data_stack[self.data_stack_top + 3] = d;
        self.data_stack[self.data_stack_top + 4] = e;
        self.data_stack[self.data_stack_top + 5] = f;
        self.data_stack_top += 6;
        Ok(())
    }
    
    /// Unpack 1 value from data stack (doesn't modify data_stack_top - that's done in pop_cont)
    #[inline]
    pub(super) fn data_unpack1(&self, start: usize) -> ArenaIndex {
        self.data_stack[start]
    }
    
    #[inline]
    pub(super) fn data_unpack2(&self, start: usize) -> (ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1])
    }
    
    #[inline]
    pub(super) fn data_unpack3(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2])
    }
    
    #[inline]
    pub(super) fn data_unpack4(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2], self.data_stack[start + 3])
    }
    
    #[inline]
    pub(super) fn data_unpack5(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2], self.data_stack[start + 3], self.data_stack[start + 4])
    }
    
    #[inline]
    pub(super) fn data_unpack6(&self, start: usize) -> (ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex, ArenaIndex) {
        (self.data_stack[start], self.data_stack[start + 1], self.data_stack[start + 2], self.data_stack[start + 3], self.data_stack[start + 4], self.data_stack[start + 5])
    }
    
    // ========================================================================
    // GC Interface
    // ========================================================================
    
    /// Run garbage collection, preserving the global environment
    pub fn gc(&mut self) -> GcStats {
        self.lisp.gc_with_roots(&[self.global_env])
    }
    
    /// Enable or disable automatic garbage collection
    pub fn set_gc_enabled(&mut self, enabled: bool) {
        self.lisp.set_gc_enabled(enabled);
    }
    
    // ========================================================================
    // Error Helpers
    // ========================================================================
    
    pub(super) fn parse_error_to_eval(err: ParseError) -> EvalError {
        EvalError {
            kind: ErrorKind::Generic,
            message: ErrorMessage::ParseError,
            expr: ArenaIndex::NIL,
            expected_type: "",
            got_type: "",
            expected_args: 0,
            got_args: 0,
            backtrace: [StackFrame::default(); MAX_BACKTRACE],
            backtrace_len: 0,
            parse_error: Some(err),
        }
    }
    
    /// Check if a value is false (ONLY #f is false)
    #[inline]
    pub(super) fn is_false(&self, val: ArenaIndex) -> Result<bool, EvalError> {
        Ok(self.lisp.get(val)?.is_false())
    }
}
