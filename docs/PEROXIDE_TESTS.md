# Peroxide Scheme Test Suite Integration

## Overview

This document describes the integration of Scheme test files from the [peroxide](https://github.com/MattX/peroxide) repository into the grift project.

## What Was Added

### 1. Test Files (in `tests/scheme/`)

- **r5rs_pitfall.scm** (~330 lines)
  - Tests for R5RS Scheme edge cases and pitfalls
  - Covers advanced features like call/cc, macro hygiene, and scoping edge cases
  - Originally from public forums, placed in the public domain

- **r5rs-tests.scm** (~548 lines)
  - Comprehensive R5RS compliance tests from chibi-scheme
  - Tests core language features: lambda, conditionals, lists, macros, etc.
  - Licensed under BSD-3-Clause (by Alex Shinn)

### 2. Test Infrastructure (in `crates/grift_eval/tests/`)

- **peroxide_pitfalls_tests.rs**
  - 4 test functions covering different pitfall categories
  - Tests specific R5RS compliance requirements
  - Documents known limitations

- **peroxide_r5rs_tests.rs**
  - 6 test functions testing core R5RS features
  - Validates lambda, conditionals, let bindings, and list operations
  - Documents expected behavior vs actual implementation

### 3. Documentation

- **tests/scheme/README.md**
  - Describes the test files and their origins
  - Explains how to run the tests
  - Documents test categories and limitations

## Running the Tests

```bash
# Run all peroxide tests
cargo test --package grift_eval peroxide

# Run specific test suites
cargo test --package grift_eval peroxide_pitfalls
cargo test --package grift_eval peroxide_r5rs

# Run with output
cargo test --package grift_eval peroxide -- --nocapture
```

## Test Results

All 10 tests are currently passing:
- 4 pitfall tests
- 6 r5rs compliance tests

### Pitfall Tests

1. **test_peroxide_pitfalls_documentation** - Documents test categories
2. **test_peroxide_pitfalls_section_4_no_reserved_identifiers** - Tests that identifiers like `lambda` can be used as parameters
3. **test_peroxide_pitfalls_section_5_false_nil_distinctness** - Verifies `#f` and `'()` are distinct
4. **test_peroxide_pitfalls_section_8_miscellaneous** - Tests edge cases like named-let with `-` as loop name

### R5RS Compliance Tests

1. **test_peroxide_r5rs_basic_lambda** - Lambda expressions and variadic arguments
2. **test_peroxide_r5rs_if_cond** - Conditional expressions
3. **test_peroxide_r5rs_and_or** - Boolean operations
4. **test_peroxide_r5rs_let_letrec** - Let bindings (with noted differences)
5. **test_peroxide_r5rs_list_operations** - List length and basic operations
6. **test_peroxide_r5rs_documentation** - Documents the full test suite

## Known Limitations

The test infrastructure documents several known differences from standard R5RS:

1. **Special Form Shadowing**: In grift, special forms like `begin` and `quote` cannot be shadowed as lambda parameters. This is a deliberate simplification.

2. **Let Binding Semantics**: Grift currently implements `let` with sequential binding semantics (like `let*`), rather than parallel binding as specified in R5RS. This means:
   ```scheme
   (let ((x 2) (y 3))
     (let ((x 7) (z (+ x y)))  ; z sees x=7, not x=2
       (* z x)))
   ; Returns 70 instead of 35
   ```

3. **Advanced Features**: Many tests in the .scm files involve advanced features like:
   - `call/cc` (call-with-current-continuation)
   - Complex macro hygiene scenarios
   - `dynamic-wind`
   
   These are documented but not all may be fully implemented.

## Test Strategy

The approach taken was:

1. **Copy Complete Test Files**: Preserve the original test files in their entirety for reference
2. **Selective Testing**: Create Rust tests that verify specific features that should work
3. **Documentation**: Clearly document what's tested, what's not, and why
4. **Progressive Enhancement**: The test infrastructure allows tracking progress as more features are implemented

This allows the project to:
- Track R5RS conformance over time
- Document implementation choices
- Provide a roadmap for future enhancements

## Future Work

The complete test files provide a roadmap for improving R5RS conformance:
- Implement proper parallel binding for `let`
- Add support for more advanced continuation features
- Improve macro hygiene handling
- Support shadowing of special forms where appropriate

## License

The test infrastructure code (Rust files) follows the grift project license (MIT OR Apache-2.0).

The original test files maintain their respective licenses:
- `r5rs_pitfall.scm`: Public domain
- `r5rs-tests.scm`: BSD-3-Clause (chibi-scheme)
