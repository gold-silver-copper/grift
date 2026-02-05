# Scheme Test Files from Peroxide

This directory contains Scheme test files from the [peroxide](https://github.com/MattX/peroxide) Scheme implementation.

## Files

### r5rs_pitfall.scm
- **Source**: Peroxide test suite
- **Purpose**: Tests for edge cases and pitfalls in R5RS Scheme implementations
- **Size**: ~330 lines
- **Content**: Tests for:
  - Proper letrec implementation with call/cc
  - Call/cc and procedure application
  - Hygienic macros
  - No reserved identifiers
  - #f/() distinctness
  - string->symbol case sensitivity
  - First-class continuations
  - Miscellaneous edge cases

### r5rs-tests.scm
- **Source**: Based on chibi-scheme R5RS test suite (by Alex Shinn)
- **Purpose**: Comprehensive R5RS Scheme compliance tests
- **Size**: ~548 lines
- **Content**: Tests for:
  - Lambda expressions and application
  - Conditionals (if, cond, case)
  - Boolean operations (and, or, not)
  - Let bindings (let, let*, letrec)
  - List operations
  - Numeric operations
  - Quoting and quasiquoting
  - Macros (define-syntax, syntax-rules)
  - Continuations (call/cc, dynamic-wind)
  - And many more R5RS features

## Test Infrastructure

The Rust test infrastructure for running these tests is located in:
- `crates/grift_eval/tests/peroxide_pitfalls_tests.rs`
- `crates/grift_eval/tests/peroxide_r5rs_tests.rs`

These test files:
1. Parse and execute selected tests from the .scm files
2. Verify that basic R5RS compliance is maintained
3. Document known limitations and differences from the standard

## Running the Tests

```bash
# Run all peroxide tests
cargo test --package grift_eval peroxide

# Run just the pitfalls tests
cargo test --package grift_eval peroxide_pitfalls

# Run just the r5rs tests
cargo test --package grift_eval peroxide_r5rs
```

## Notes

**Important**: The .scm test files are preserved as-is from their original sources for reference and documentation purposes. They may contain test syntax or macros that are specific to their original implementations. The actual test execution in grift is done through the Rust test infrastructure which selectively executes individual test expressions that are compatible with grift's implementation.

Many of the tests in these files involve advanced features like:
- `call/cc` (call-with-current-continuation)
- `dynamic-wind`
- Complex macro hygiene scenarios

Not all of these features may be fully implemented in grift. The test infrastructure focuses on:
1. Basic R5RS compliance tests that should work
2. Documenting the presence and categories of more advanced tests
3. Tracking progress on conformance over time

## License

These test files come from open source projects:
- r5rs_pitfall.scm: Public domain (collected from public forums)
- r5rs-tests.scm: BSD-3-Clause (from chibi-scheme)
