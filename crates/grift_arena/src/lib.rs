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
//! - **Pluggable storage**: Use fixed-size arrays (no-std) or `Vec` (std feature)
//!
//! ## Module Organization
//!
//! The arena allocator is organized into the following modules:
//!
//! - [`types`] - Core types: `ArenaIndex`, `ArenaError`, `ArenaResult`
//! - [`arena`] - Main `Arena` struct and core operations
//! - [`storage`] - Storage backends: `ArenaStorage` trait, `ArrayStorage`, `VecStorage`
//! - [`generic_arena`] - Generic arena that works with any storage backend
//! - [`traits`] - Extension traits: `ArenaDelete`, `ArenaCopy`, `Trace`
//! - [`gc`] - Garbage collection implementation
//! - [`iter`] - Iterator support
//! - [`stats`] - Statistics types: `ArenaStats`, `GcStats`
//!
//! ## Storage Backends
//!
//! The arena supports different storage backends via the [`ArenaStorage`] trait:
//!
//! - **`ArrayStorage<T, N>`**: Fixed-size array storage (default, no-std compatible)
//! - **`VecStorage<T>`**: Vector-based storage (requires `std` feature)
//!
//! The original `Arena<T, N>` type is a convenience alias for the fixed-size case.
//! For dynamic storage, use `GenericArena<T, VecStorage<T>>`.
//!
//! ## Example
//!
//! ```rust
//! use grift_arena::{Arena, ArenaIndex};
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

// Macros module (must be declared before other modules that use the macros)
mod macros;

pub mod types;
pub mod storage;
pub mod generic_arena;
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

// Arena struct (backward compatible)
pub use arena::Arena;

// Generic arena
pub use generic_arena::{GenericArena, GenericArenaIterator};

// Storage backends
pub use storage::{ArenaStorage, ArrayStorage};
#[cfg(feature = "std")]
pub use storage::VecStorage;

// Traits
pub use traits::{ArenaDelete, ArenaCopy, Trace};
pub use traits::{GenericArenaDelete, GenericArenaCopy};

// Statistics
pub use stats::{ArenaStats, GcStats};

// Iterator
pub use iter::ArenaIterator;

// ============================================================================
// Helper Macros
// ============================================================================


