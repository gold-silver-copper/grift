#![no_std]
#![forbid(unsafe_code)]

//! # Fixed-Size Arena Allocator
//!
//! A minimal no-std, no-alloc arena allocator with fixed capacity.
//!
//! ## Features
//!
//! - **Fixed-size**: All memory pre-allocated at compile time
//! - **No-std, no-alloc**: Works in embedded environments with no heap
//! - **Generic**: Works with any `Copy` type
//! - **Interior mutability**: Safe access via `Cell` (no runtime borrow checking overhead)
//! - **O(1) allocation**: Free-list based allocation and deallocation
//! - **Mark-and-sweep GC**: Trait-based garbage collection via [`Trace`]
//! - **Zero dependencies**: Only uses `core::cell::Cell`
//!
//! ## Module Organization
//!
//! The arena allocator is organized into the following modules:
//!
//! - [`types`] - Core types: `ArenaIndex`, `ArenaError`, `ArenaResult`
//! - [`arena`] - Main `Arena` struct and core operations
//! - [`traits`] - Extension traits: `ArenaDelete`, `ArenaCopy`, `Trace`
//! - [`gc`] - Garbage collection implementation
//! - [`iter`] - Iterator support
//! - [`stats`] - Statistics types: `ArenaStats`, `GcStats`
//!
//! ## Example
//!
//! ```rust
//! use pwn_arena::{Arena, ArenaIndex};
//!
//! #[derive(Clone, Copy, Debug, PartialEq)]
//! enum Node {
//!     Leaf(isize),
//!     Branch(ArenaIndex, ArenaIndex),
//! }
//!
//! let arena: Arena<Node, 1024> = Arena::new(Node::Leaf(0));
//!
//! // Allocate nodes
//! let left = arena.alloc(Node::Leaf(1)).unwrap();
//! let right = arena.alloc(Node::Leaf(2)).unwrap();
//! let root = arena.alloc(Node::Branch(left, right)).unwrap();
//!
//! // Access nodes
//! if let Node::Branch(l, r) = arena.get(root).unwrap() {
//!     println!("Left: {:?}, Right: {:?}", arena.get(l), arena.get(r));
//! }
//!
//! // Free when done
//! arena.free(root).unwrap();
//! ```

// ============================================================================
// Module Declarations
// ============================================================================

pub mod types;
pub mod traits;
pub mod stats;
pub mod arena;
pub mod iter;
pub mod gc;

// ============================================================================
// Re-exports
// ============================================================================

// Core types
pub use types::{ArenaIndex, ArenaError, ArenaResult};

// Arena struct
pub use arena::Arena;

// Traits
pub use traits::{ArenaDelete, ArenaCopy, Trace};

// Statistics
pub use stats::{ArenaStats, GcStats};

// Iterator
pub use iter::ArenaIterator;

// ============================================================================
// Helper Macros
// ============================================================================


