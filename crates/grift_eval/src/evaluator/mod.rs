//! Main evaluator implementation.
//!
//! This module is split into three files:
//! - `core.rs`: Core evaluation logic (constructor, GC, environment, trampoline)
//! - `builtins.rs`: Builtin function implementations
//! - `forms.rs`: Special form handling (continuations, let, case, do, etc.)

mod core;
mod builtins;
mod forms;

use grift_parser::{ArenaIndex, Lisp};

use crate::error::{StackFrame, MAX_STACK_DEPTH};
use crate::continuation::{Cont, MAX_CONT_DEPTH, MAX_DATA_STACK};
use crate::native::NativeRegistry;

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
    global_env: ArenaIndex,
    /// Call stack for error reporting
    call_stack: [StackFrame; MAX_STACK_DEPTH],
    call_stack_depth: usize,
    /// Continuation stack for full trampolining
    cont_stack: [Cont; MAX_CONT_DEPTH],
    cont_depth: usize,
    /// Data stack for continuation data (separate from arena for performance)
    /// Stores raw ArenaIndex values without Value::Ref wrapper
    data_stack: [ArenaIndex; MAX_DATA_STACK],
    data_stack_top: usize,
    /// Native function registry
    native_registry: NativeRegistry<N>,
}
