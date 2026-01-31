//! Environment management for the evaluator.
//!
//! This module handles variable binding, lookup, and mutation.

use grift_parser::{ArenaIndex, Value};

use crate::error::{ErrorKind, EvalResult};
use super::Evaluator;

impl<'a, const N: usize> Evaluator<'a, N> {
    // ========================================================================
    // Environment Management
    // ========================================================================
    
    /// Extend an environment with a binding
    #[inline]
    pub(crate) fn env_extend(&self, env: ArenaIndex, name: ArenaIndex, value: ArenaIndex) -> EvalResult {
        let binding = self.lisp.cons(name, value)?;
        self.lisp.cons(binding, env).map_err(Into::into)
    }
    
    /// Look up a variable in an environment
    pub(super) fn env_lookup(&self, env: ArenaIndex, name: ArenaIndex) -> EvalResult {
        let mut current = env;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    // Try global
                    return self.env_lookup_global(name);
                }
                Value::Cons { car, cdr } => {
                    // With inline cons, we get car and cdr directly
                    if let Value::Cons { car: bound_name, cdr: bound_value } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        return Ok(bound_value);
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::Generic, name)),
            }
        }
    }
    
    /// Look up in global environment only
    pub(super) fn env_lookup_global(&self, name: ArenaIndex) -> EvalResult {
        let mut current = self.global_env;
        
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    return Err(self.make_error(ErrorKind::UnboundVariable, name));
                }
                Value::Cons { car, cdr } => {
                    // With inline cons, we get car and cdr directly
                    if let Value::Cons { car: bound_name, cdr: bound_value } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        return Ok(bound_value);
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::Generic, name)),
            }
        }
    }
    
    /// Set a variable in an environment (mutation operation)
    /// Searches both local and global environments
    /// Returns the new value on success
    pub(super) fn env_set(&self, env: ArenaIndex, name: ArenaIndex, value: ArenaIndex) -> EvalResult {
        // First search local environment
        let mut current = env;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    // Not found in local env, try global
                    return self.env_set_global(name, value);
                }
                Value::Cons { car, cdr } => {
                    if let Value::Cons { car: bound_name, .. } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        // Found it - mutate the binding
                        self.lisp.set_cdr(car, value)?;
                        return Ok(value);
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::Generic, name)),
            }
        }
    }
    
    /// Set a variable in global environment only
    pub(super) fn env_set_global(&self, name: ArenaIndex, value: ArenaIndex) -> EvalResult {
        let mut current = self.global_env;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    // Not found anywhere - error
                    return Err(self.make_error(ErrorKind::UnboundVariable, name));
                }
                Value::Cons { car, cdr } => {
                    if let Value::Cons { car: bound_name, .. } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        // Found it - mutate the binding
                        self.lisp.set_cdr(car, value)?;
                        return Ok(value);
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::Generic, name)),
            }
        }
    }
    
    /// Define in global environment (NOTE: only allowed at top-level)
    pub fn define(&mut self, name: ArenaIndex, value: ArenaIndex) -> EvalResult {
        // Check if already defined and update
        let mut current = self.global_env;
        loop {
            match self.lisp.get(current)? {
                Value::Nil => {
                    // Not found, add new binding
                    self.global_env = self.env_extend(self.global_env, name, value)?;
                    return Ok(value);
                }
                Value::Cons { car, cdr } => {
                    if let Value::Cons { car: bound_name, .. } = self.lisp.get(car)?
                        && self.lisp.symbol_eq(bound_name, name)?
                    {
                        // Update existing
                        self.lisp.set_cdr(car, value)?;
                        return Ok(value);
                    }
                    current = cdr;
                }
                _ => return Err(self.make_error(ErrorKind::Generic, name)),
            }
        }
    }
}
