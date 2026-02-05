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

/// Function pointer type for output callbacks
/// 
/// This function is called by `display` and `newline` builtins during evaluation,
/// including during macro expansion. The function receives the evaluator's lisp 
/// context and the value to display.
/// 
/// **Special handling for newline**: The `newline` builtin passes `nil` as the
/// value parameter. Callbacks should check `val.is_nil()` to distinguish between
/// newline requests and actual display values.
pub type OutputCallback<const N: usize> = fn(&Lisp<N>, ArenaIndex);

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
    /// Optional output callback for display/newline
    /// 
    /// When set, `display` and `newline` will call this function to produce output.
    /// This enables side effects during macro expansion to be visible.
    output_callback: Option<OutputCallback<N>>,
    /// Current ellipsis symbol for macro expansion
    /// 
    /// Defaults to `...` but can be changed via `with-ellipsis`.
    /// Used during pattern matching and template transcription to
    /// recognize the ellipsis operator.
    current_ellipsis: ArenaIndex,
    /// Saved continuation root for nested evaluations
    /// 
    /// When eval_for_macro creates a nested trampoline evaluation,
    /// the outer continuation chain needs to remain rooted during GC.
    /// This field holds the saved continuation to keep it alive.
    saved_cont_root: ArenaIndex,
}
