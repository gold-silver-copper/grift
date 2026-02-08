#![forbid(unsafe_code)]

//! # Grift Standard Library
//!
//! Standard library features for the Grift R7RS-compliant Scheme implementation
//! that require Rust's `std` (I/O, filesystem, etc.).
//!
//! This crate implements the [`IoProvider`] trait from `grift_core` using
//! Rust's standard I/O, giving the `no_std` evaluator access to real
//! input/output when running on a hosted platform.
//!
//! ## Quick Start
//!
//! ```rust
//! use grift_std::StdIoProvider;
//! use grift_core::IoProvider;
//!
//! // Create a standard I/O provider
//! let mut io = StdIoProvider::new();
//!
//! // Check port types
//! use grift_core::PortId;
//! assert!(io.is_input_port(PortId::STDIN));
//! assert!(io.is_output_port(PortId::STDOUT));
//! ```
//!
//! ## Crate Boundary
//!
//! | Crate | `#![no_std]` | Purpose |
//! |-------|-------------|---------|
//! | `grift_core` | ✅ | Defines [`IoProvider`] trait |
//! | **`grift_std`** | ❌ | Implements [`IoProvider`] with `std::io` |
//! | `grift_eval` | ✅ | Pure evaluator, no I/O |
//! | `grift_repl` | ❌ | Interactive REPL |

mod io;

pub use io::StdIoProvider;

// Re-export the trait so users can access it without a separate grift_core dep.
pub use grift_core::{IoProvider, PortId, IoErrorKind, IoResult, NullIoProvider, DisplayPort};
