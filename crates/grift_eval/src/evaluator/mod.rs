//! Main evaluator implementation.
//!
//! This module is split into four files:
//! - `core.rs`: Core evaluation logic (constructor, GC, environment, trampoline)
//! - `builtins.rs`: Builtin function implementations
//! - `forms.rs`: Special form handling (continuations, let, case, do, etc.)
//! - `expand.rs`: Macro expansion (define-syntax, syntax-rules)

mod core;
mod builtins;
mod forms;
mod expand;

use grift_parser::{ArenaIndex, Lisp};

use crate::error::{StackFrame, MAX_STACK_DEPTH};
use crate::native::NativeRegistry;

// ============================================================================
// Evaluator
// ============================================================================

/// The Lisp evaluator with full trampolined TCO.
///
/// This evaluator uses continuation-passing style with an arena-based
/// continuation chain, enabling unlimited recursion depth without Rust
/// stack overflow.
///
/// The continuation stack is stored entirely in the arena as a linked list
/// of `ContFrame` values, enabling O(1) capture for call/cc.
pub struct Evaluator<'a, const N: usize> {
    pub(crate) lisp: &'a Lisp<N>,
    /// Global environment
    global_env: ArenaIndex,
    /// Call stack for error reporting
    call_stack: [StackFrame; MAX_STACK_DEPTH],
    call_stack_depth: usize,
    /// Current continuation - arena-based ContFrame linked list
    /// Points to the head of the continuation chain, or Nil if empty (Done)
    current_cont: ArenaIndex,
    /// Native function registry
    native_registry: NativeRegistry<N>,
    /// Macro environment - stores (name . SyntaxRules) bindings
    /// Separate from value environment to allow shadowing
    macro_env: ArenaIndex,
    /// Counter for generating unique symbols (gensym)
    gensym_counter: usize,
    /// Dynamic-wind chain - arena-based linked list of (before . after) thunk pairs
    /// Each entry is: ((before . after) . parent_chain)
    /// Used to track dynamic extent for proper before/after thunk execution
    /// when entering/exiting dynamic-wind scopes via call/cc
    dynamic_wind_chain: ArenaIndex,
}
