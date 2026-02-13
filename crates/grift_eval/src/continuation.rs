//! Continuation types for arena-based trampolined evaluation.
//!
//! Continuations are stored in the arena as a linked list of `ContFrame` values.
//! Each frame references a `Value::ContType` in the arena to identify its kind,
//! along with associated data and a reference to the parent continuation.
//!
//! This design enables O(1) capture for call/cc (just save the current pointer)
//! and natural structure sharing between continuations.

use grift_parser::{ArenaIndex, Builtin};

// Re-export ContType from grift_parser (originally defined in grift_core)
pub use grift_parser::ContType;

// ============================================================================
// GC Root Tracking
// ============================================================================

/// Trait for types that contain GC roots.
///
/// Implementors should call `tracer` for each [`ArenaIndex`] that represents
/// a live GC root. This ensures the garbage collector does not collect
/// reachable objects.
///
/// By centralizing root enumeration in this trait, adding a new
/// [`ArenaIndex`] field to a type will produce a compile-time reminder
/// (or at least a single, obvious place) to update root tracking, rather
/// than requiring updates in every GC call-site.
pub trait GcRoots {
    /// Call `tracer` once for every [`ArenaIndex`] that is a live GC root.
    fn trace_roots(&self, tracer: &mut dyn FnMut(ArenaIndex));
}

// ============================================================================
// Typed Index Newtypes
// ============================================================================

/// Macro to generate a typed wrapper around [`ArenaIndex`].
///
/// Each generated type provides `index()`, `new()`, and bidirectional `From`
/// conversions, preventing accidentally passing an expression where an
/// environment is expected (or vice versa).
macro_rules! define_index_wrapper {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name(pub(crate) ArenaIndex);

        impl $name {
            /// Get the underlying [`ArenaIndex`].
            pub const fn index(self) -> ArenaIndex {
                self.0
            }

            /// Create from a raw [`ArenaIndex`].
            pub const fn new(idx: ArenaIndex) -> Self {
                $name(idx)
            }
        }

        impl From<ArenaIndex> for $name {
            fn from(idx: ArenaIndex) -> Self {
                $name(idx)
            }
        }

        impl From<$name> for ArenaIndex {
            fn from(r: $name) -> Self {
                r.0
            }
        }
    };
}

define_index_wrapper!(
    /// A typed wrapper around [`ArenaIndex`] representing an environment chain.
    ///
    /// Environments are linked lists of `(name . value)` bindings stored in the
    /// arena. Using a distinct type prevents accidentally passing an expression
    /// where an environment is expected.
    EnvRef
);

define_index_wrapper!(
    /// A typed wrapper around [`ArenaIndex`] representing an expression to evaluate.
    ///
    /// Expressions are S-expressions stored in the arena. Using a distinct type
    /// prevents accidentally passing an environment where an expression is expected.
    ExprRef
);

// ============================================================================
// Continuation Type Enum
// ============================================================================
//
// The `ContType` enum identifies the continuation type stored in each ContFrame.
// Using an enum provides exhaustiveness checking in `step_return()` and makes
// adding new continuation types compiler-checked.
//
// The data field of each ContFrame contains continuation-specific data
// encoded as cons cells in the arena.

/// Trampoline state - what we're currently doing
#[derive(Clone, Copy, Debug)]
pub enum TrampolineState {
    /// Evaluate expression in environment
    Eval { expr: ExprRef, env: EnvRef },
    /// Return a value to the continuation
    Return { val: grift_parser::ArenaIndex },
}

impl GcRoots for TrampolineState {
    fn trace_roots(&self, tracer: &mut dyn FnMut(ArenaIndex)) {
        match self {
            TrampolineState::Eval { expr, env } => {
                tracer(expr.0);
                tracer(env.0);
            }
            TrampolineState::Return { val } => {
                tracer(*val);
            }
        }
    }
}

/// Check if a builtin is a binary operation (exactly 2 args, optimized path)
pub fn is_binary_builtin(builtin: Builtin) -> bool {
    matches!(builtin, 
        Builtin::Add | Builtin::Sub | Builtin::Mul | Builtin::Div | Builtin::Modulo | Builtin::Remainder |
        Builtin::Lt | Builtin::Gt | Builtin::Le | Builtin::Ge | Builtin::NumEq |
        Builtin::EqP | Builtin::EqvP | Builtin::Cons
    )
}
