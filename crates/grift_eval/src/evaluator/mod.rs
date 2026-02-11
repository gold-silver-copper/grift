//! Main evaluator implementation.
//!
//! This module is split into four files:
//! - `core.rs`: Core evaluation logic (constructor, GC, environment, trampoline)
//! - `builtins.rs`: Builtin function implementations
//! - `forms.rs`: Special form handling (continuations, let, case, do, etc.)
//! - `expand.rs`: Macro expansion (define-syntax, syntax-case)

mod core;
mod builtins;
mod forms;
mod expand;

use grift_parser::{ArenaIndex, Lisp, IoProvider};

use crate::continuation::EnvRef;
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

/// Pre-interned keyword symbols for O(1) dispatch.
///
/// Instead of comparing symbol names by string on every list evaluation,
/// these are interned once at startup and compared by ArenaIndex equality.
pub(crate) struct Keywords {
    pub kw_if: ArenaIndex,
    pub kw_quote: ArenaIndex,
    pub kw_define: ArenaIndex,
    pub kw_define_syntax: ArenaIndex,
    pub kw_let_syntax: ArenaIndex,
    pub kw_letrec_syntax: ArenaIndex,
    pub kw_syntax_case: ArenaIndex,
    pub kw_syntax: ArenaIndex,
    pub kw_lambda: ArenaIndex,
    pub kw_set: ArenaIndex,
    pub kw_begin: ArenaIndex,
    pub kw_quasiquote: ArenaIndex,
    pub kw_syntax_error: ArenaIndex,
    pub kw_define_record_type: ArenaIndex,
    pub kw_define_library: ArenaIndex,
    pub kw_import: ArenaIndex,
    pub kw_include: ArenaIndex,
    pub kw_include_ci: ArenaIndex,
}

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
    global_env: EnvRef,
    /// Call stack for error reporting
    call_stack: [StackFrame; MAX_STACK_DEPTH],
    call_stack_depth: usize,
    /// Current continuation - arena-based ContFrame linked list
    /// Points to the head of the continuation chain, or Nil if empty (Done)
    current_cont: ArenaIndex,
    /// Native function registry
    native_registry: NativeRegistry<N>,
    /// Macro environment - stores (name . transformer) bindings
    /// Transformers are Lambda values (procedural macros via syntax-case)
    macro_env: EnvRef,
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
    /// Call-site environment for macro expansion.
    /// 
    /// Set in `apply_macro_trampolined` to the environment at the macro call site.
    /// Used by `match_pattern_syntax` for free-identifier=? literal matching:
    /// when a syntax-rules literal like `else` is locally bound at the call site,
    /// the pattern matcher checks this environment to distinguish the bound identifier
    /// from the unbound literal keyword.
    call_site_env: EnvRef,
    /// Exception handler chain — arena-based linked list of handler closures.
    /// Each entry is: (handler . parent_chain)
    /// Used by `raise` / `raise-continuable` to invoke the current handler.
    exception_handler_chain: ArenaIndex,
    /// Optional I/O provider for port operations (R7RS §6.13).
    ///
    /// When set, port builtins (read-char, write-char, etc.) use this provider.
    /// The provider is borrowed from the caller and must outlive the evaluator.
    io: Option<&'a mut (dyn IoProvider + 'a)>,
    /// Current input port for `current-input-port` (R7RS §6.13.2).
    /// Defaults to STDIN. Changed by `with-input-from-file`.
    current_input_port: grift_parser::PortId,
    /// Current output port for `current-output-port` (R7RS §6.13.2).
    /// Defaults to STDOUT. Changed by `with-output-to-file`.
    current_output_port: grift_parser::PortId,
    /// Library registry — arena-based association list of (name . env) pairs.
    /// Used by `define-library` and `import` (R7RS §5.6).
    library_registry: ArenaIndex,
    /// Libraries currently being loaded — arena-based list of library names.
    /// Used for detecting circular dependencies during auto-loading.
    loading_libraries: ArenaIndex,
    /// Pre-interned keyword symbols for O(1) special form dispatch
    keywords: Keywords,
}
