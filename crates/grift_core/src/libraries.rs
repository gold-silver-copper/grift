//! Embedded R7RS standard library source registry.
//!
//! Each entry maps a library name (e.g. `&["scheme", "base"]`) to the
//! embedded Scheme source for its `define-library` form.  These are loaded
//! on-demand by the evaluator when a library is first imported.

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
pub const LIBRARY_SOURCES: &[LibrarySource] = &[
    LibrarySource { name: &["scheme", "base"],            source: include_str!("lib/scheme/base.scm") },
    LibrarySource { name: &["scheme", "case-lambda"],     source: include_str!("lib/scheme/case-lambda.scm") },
    LibrarySource { name: &["scheme", "char"],            source: include_str!("lib/scheme/char.scm") },
    LibrarySource { name: &["scheme", "cxr"],             source: include_str!("lib/scheme/cxr.scm") },
    LibrarySource { name: &["scheme", "eval"],            source: include_str!("lib/scheme/eval.scm") },
    LibrarySource { name: &["scheme", "file"],            source: include_str!("lib/scheme/file.scm") },
    LibrarySource { name: &["scheme", "inexact"],         source: include_str!("lib/scheme/inexact.scm") },
    LibrarySource { name: &["scheme", "lazy"],            source: include_str!("lib/scheme/lazy.scm") },
    LibrarySource { name: &["scheme", "load"],            source: include_str!("lib/scheme/load.scm") },
    LibrarySource { name: &["scheme", "process-context"], source: include_str!("lib/scheme/process-context.scm") },
    LibrarySource { name: &["scheme", "read"],            source: include_str!("lib/scheme/read.scm") },
    LibrarySource { name: &["scheme", "repl"],            source: include_str!("lib/scheme/repl.scm") },
    LibrarySource { name: &["scheme", "time"],            source: include_str!("lib/scheme/time.scm") },
    LibrarySource { name: &["scheme", "write"],           source: include_str!("lib/scheme/write.scm") },
];
