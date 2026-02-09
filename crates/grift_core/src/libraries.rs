//! Embedded R7RS standard library source registry.
//!
//! Each entry maps a library name (e.g. `&["scheme", "base"]`) to the
//! embedded Scheme source for its `define-library` form.  These are loaded
//! on-demand by the evaluator when a library is first imported.
//!
//! ## Feature-gated library inclusion
//!
//! Each library is gated behind a corresponding Cargo feature flag.
//! By default, the `all-libraries` feature enables every library,
//! preserving backward compatibility.  For resource-constrained embedded
//! targets, disable default features and select only the libraries you
//! need:
//!
//! ```toml
//! [dependencies.grift_core]
//! version = "..."
//! default-features = false
//! features = ["scheme-base", "scheme-write"]
//! ```
//!
//! ### Available feature flags
//!
//! | Feature                | Library                     |
//! |------------------------|-----------------------------|
//! | `scheme-base`          | `(scheme base)`             |
//! | `scheme-case-lambda`   | `(scheme case-lambda)`      |
//! | `scheme-char`          | `(scheme char)`             |
//! | `scheme-cxr`           | `(scheme cxr)`              |
//! | `scheme-eval`          | `(scheme eval)`             |
//! | `scheme-file`          | `(scheme file)`             |
//! | `scheme-inexact`       | `(scheme inexact)`          |
//! | `scheme-lazy`          | `(scheme lazy)`             |
//! | `scheme-load`          | `(scheme load)`             |
//! | `scheme-process-context` | `(scheme process-context)` |
//! | `scheme-read`          | `(scheme read)`             |
//! | `scheme-repl`          | `(scheme repl)`             |
//! | `scheme-time`          | `(scheme time)`             |
//! | `scheme-write`         | `(scheme write)`            |
//! | `all-libraries`        | All of the above            |

/// A single entry in the embedded library source registry.
#[derive(Debug, Clone, Copy)]
pub struct LibrarySource {
    /// Library name as a slice of path components (e.g. `&["scheme", "base"]`).
    pub name: &'static [&'static str],
    /// The Scheme source for the `define-library` form.
    pub source: &'static str,
}

/// All embedded standard library sources.
///
/// The evaluator consults this table when `import` refers to a library that
/// has not yet been loaded into the library registry.
///
/// The contents of this slice depend on which `scheme-*` feature flags are
/// enabled at compile time.  With the default `all-libraries` feature every
/// library is included.
pub const LIBRARY_SOURCES: &[LibrarySource] = &[
    #[cfg(feature = "scheme-base")]
    LibrarySource { name: &["scheme", "base"],            source: include_str!("lib/scheme/base.scm") },
    #[cfg(feature = "scheme-case-lambda")]
    LibrarySource { name: &["scheme", "case-lambda"],     source: include_str!("lib/scheme/case-lambda.scm") },
    #[cfg(feature = "scheme-char")]
    LibrarySource { name: &["scheme", "char"],            source: include_str!("lib/scheme/char.scm") },
    #[cfg(feature = "scheme-cxr")]
    LibrarySource { name: &["scheme", "cxr"],             source: include_str!("lib/scheme/cxr.scm") },
    #[cfg(feature = "scheme-eval")]
    LibrarySource { name: &["scheme", "eval"],            source: include_str!("lib/scheme/eval.scm") },
    #[cfg(feature = "scheme-file")]
    LibrarySource { name: &["scheme", "file"],            source: include_str!("lib/scheme/file.scm") },
    #[cfg(feature = "scheme-inexact")]
    LibrarySource { name: &["scheme", "inexact"],         source: include_str!("lib/scheme/inexact.scm") },
    #[cfg(feature = "scheme-lazy")]
    LibrarySource { name: &["scheme", "lazy"],            source: include_str!("lib/scheme/lazy.scm") },
    #[cfg(feature = "scheme-load")]
    LibrarySource { name: &["scheme", "load"],            source: include_str!("lib/scheme/load.scm") },
    #[cfg(feature = "scheme-process-context")]
    LibrarySource { name: &["scheme", "process-context"], source: include_str!("lib/scheme/process-context.scm") },
    #[cfg(feature = "scheme-read")]
    LibrarySource { name: &["scheme", "read"],            source: include_str!("lib/scheme/read.scm") },
    #[cfg(feature = "scheme-repl")]
    LibrarySource { name: &["scheme", "repl"],            source: include_str!("lib/scheme/repl.scm") },
    #[cfg(feature = "scheme-time")]
    LibrarySource { name: &["scheme", "time"],            source: include_str!("lib/scheme/time.scm") },
    #[cfg(feature = "scheme-write")]
    LibrarySource { name: &["scheme", "write"],           source: include_str!("lib/scheme/write.scm") },
];
