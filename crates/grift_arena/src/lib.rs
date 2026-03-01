#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # Fixed-Size Arena Allocator
//!
//! A minimal `no_std`, `no_alloc` arena allocator with fixed capacity,
//! designed for embedded and resource-constrained environments where heap
//! allocation is unavailable or undesirable.
//!
//! ## Features
//!
//! - **Fixed-size**: All memory pre-allocated at compile time via const generics
//! - **No-std, no-alloc**: Works in embedded environments with no heap
//! - **Generic**: Works with any `Copy` type
//! - **Interior mutability**: Safe access via `Cell` (no runtime borrow checking overhead)
//! - **O(1) allocation**: Free-list based allocation and deallocation
//! - **Mark-and-sweep GC**: Trait-based garbage collection via [`Trace`]
//! - **Zero dependencies**: Only uses `core::cell::Cell`
//!
//! ## Safety Guarantees
//!
//! This crate uses `#![forbid(unsafe_code)]` — there is no `unsafe` anywhere
//! in the implementation. Interior mutability is achieved through `Cell<T>`
//! rather than raw pointers, and all indices are bounds-checked before access.
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

// — Module Declarations —

pub mod types;
pub mod traits;
pub mod stats;
pub mod arena;
pub mod iter;
pub mod gc;

// — Re-exports —

pub use types::{ArenaIndex, ArenaError, ArenaResult};
pub use arena::Arena;
pub use traits::{ArenaDelete, ArenaCopy, Trace};
pub use stats::{ArenaStats, GcStats};
pub use iter::ArenaIterator;
