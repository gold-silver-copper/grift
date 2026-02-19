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
//! - **Generic**: Works with any type implementing the `Slotted` trait
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
//! use grift_arena::{Arena, ArenaIndex, Slotted};
//!
//! #[derive(Clone, Copy, Debug, PartialEq)]
//! enum Node {
//!     Free(usize),
//!     Leaf(isize),
//!     Branch(ArenaIndex, ArenaIndex),
//! }
//!
//! impl Slotted for Node {
//!     fn is_free(&self) -> bool { matches!(self, Node::Free(_)) }
//!     fn next_free(&self) -> usize { match self { Node::Free(n) => *n, _ => unreachable!() } }
//!     fn make_free(next: usize) -> Self { Node::Free(next) }
//! }
//!
//! let arena: Arena<Node, 1024> = Arena::new();
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

// — Module Declarations —

pub mod types;
pub mod traits;
pub mod stats;
pub mod arena;
pub mod iter;
pub mod gc;

// — Re-exports —

pub use types::{ArenaIndex, ArenaError, ArenaResult, FREE_LIST_END};
pub use arena::Arena;
pub use traits::{ArenaDelete, ArenaCopy, Trace, Slotted};
pub use stats::{ArenaStats, GcStats};
pub use iter::ArenaIterator;
