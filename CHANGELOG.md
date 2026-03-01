# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `SPEC.md` — formal reference specification for the Grift dialect,
  building on Kernel R-1.
- `CHANGELOG.md` — this file, for release tracking.
- MSRV policy: Rust 1.85 documented in `Cargo.toml` (`rust-version`)
  and tested in CI.
- MSRV CI job using `dtolnay/rust-toolchain@1.85`.
- Source location tracking (line/column) in `ArenaError::ParseError`.
  Parse errors now carry `{ line: u32, col: u32 }` fields and display
  as `"Parse error at line N, column M"`.

### Changed
- `ArenaError::ParseError` is now a struct variant with `line` and `col`
  fields (breaking change for pattern matching).
- Upgraded `#![warn(missing_docs)]` to `#![deny(missing_docs)]` in
  `grift`, `grift_arena`, and `grift_unicode`.
- Enabled `#![warn(clippy::pedantic)]` across all crates with targeted
  allows for accepted patterns.
- CI clippy job now uses `-- -D warnings` to fail on any warning.

### Fixed
- `no-std-check` CI job no longer references non-existent crates
  (`grift_core`, `grift_parser`, `grift_eval`).

## [1.5.0] - 2025-01-01

- Initial public release with vau calculus, arena allocator, mark-and-sweep
  GC, tail-call optimization, and standard library prelude.
