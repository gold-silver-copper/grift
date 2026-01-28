# Scheme R7RS Conformance Work

This document provides guidance for continuing work on Scheme R7RS conformance for the pwn_arena Lisp implementation.

## Reference Specification

The authoritative R7RS specification is located at:
- **`scheme-spec-r7rs/spec.html`** - The complete Revised^7 Report on the Algorithmic Language Scheme

All conformance work should reference this specification. The spec is organized into:
- **Chapter 1-3**: Overview, lexical conventions, basic concepts
- **Chapter 4**: Expressions (primitive and derived)
- **Chapter 5**: Program structure (programs, libraries, REPL)
- **Chapter 6**: Standard procedures (built-in functions)
- **Chapter 7**: Formal syntax and semantics
- **Appendix A**: Standard libraries and exported identifiers
- **Appendix B**: Optional implementation features

## Current Implementation Status

### ✅ Implemented Features

#### Core Language Features
- ✅ Lexical scoping with closures
- ✅ Proper tail-call optimization (via trampolining)
- ✅ Strict evaluation (call-by-value)
- ✅ Special forms: `quote`, `if`, `cond`, `case`, `lambda`, `define`, `set!`, `let`, `let*`, `begin`, `and`, `or`, `do`, `quasiquote`, `eval`, `apply`
- ✅ Macros: `defmacro` (note: R7RS uses `syntax-rules`, see below)

#### Built-in Procedures (Chapter 6)
- ✅ **Equivalence**: `eq?`, `eqv?`, `equal?`
- ✅ **Booleans**: `not`, `boolean?`
- ✅ **Pairs/Lists**: `car`, `cdr`, `cons`, `list`, `null?`, `pair?`, `set-car!`, `set-cdr!`
- ✅ **Numbers**: `+`, `-`, `*`, `/`, `mod`, `modulo`, `remainder`, `=`, `<`, `>`, `<=`, `>=`, `number?`
- ✅ **Characters**: `char?` (basic support)
- ✅ **Strings**: Basic string operations (via arrays)
- ✅ **Type Predicates**: `atom`, `symbol?`, `procedure?`
- ✅ **I/O**: `print`, `display`, `newline`, `error`
- ✅ **GC Control**: `gc`, `gc-enable`, `gc-disable`, `gc-enabled?`, `arena-stats`

#### Standard Library Functions (`stdlib.scm`)
- ✅ `atom`, `map`, `filter`, `fold`, `length`, `append`, `reverse`, `nth`, `take`, `drop`, `zip`, `member`, `assoc`, `range`, `compose`, `identity`, `constantly`, `flip`, `curry`, `cadr`, `caddr`, `cddr`

### ❌ Missing Critical Features

#### Syntax and Special Forms (Chapter 4)
- ❌ `letrec` - Recursive let binding
- ❌ `letrec*` - Sequential recursive let binding
- ❌ `let-values` / `let*-values` - Multiple value binding
- ❌ `define-values` - Multiple value definitions
- ❌ `case-lambda` - Multiple arity procedures (Chapter 4.2.9)
- ❌ `when` / `unless` - Convenience conditionals
- ❌ `delay` / `force` / `delay-force` - Lazy evaluation (Chapter 4.2.5)
- ❌ `make-promise` / `promise?` - Promise procedures
- ❌ `parameterize` / `make-parameter` - Dynamic bindings (Chapter 4.2.6)
- ❌ `dynamic-wind` - Dynamic extent control
- ❌ `call-with-current-continuation` / `call/cc` - First-class continuations
- ❌ `guard` - Exception handling (Chapter 4.2.7)
- ❌ `raise` / `raise-continuable` - Exception raising
- ❌ `error-object?` / `error-object-message` / `error-object-irritants` - Error objects
- ❌ `syntax-rules` - Hygienic macros (replaces `defmacro` for R7RS conformance)
- ❌ `syntax-error` - Macro error signaling
- ❌ `let-syntax` / `letrec-syntax` - Local syntax bindings
- ❌ `cond-expand` - Conditional expansion

#### Standard Procedures (Chapter 6)

**Numbers (Section 6.2)**
- ❌ `abs` - Absolute value
- ❌ `expt` - Exponentiation
- ❌ `exact?` / `inexact?` - Exactness predicates
- ❌ `exact-integer?` - Integer predicate
- ❌ `finite?` / `infinite?` / `nan?` - Number predicates
- ❌ `max` / `min` - Min/max
- ❌ `quotient` / `remainder` / `modulo` - Integer division (partially implemented)
- ❌ `floor` / `ceiling` / `truncate` / `round` - Rounding
- ❌ `rationalize` - Rational approximation
- ❌ `exp` / `log` - Exponential/logarithm
- ❌ `sin` / `cos` / `tan` / `asin` / `acos` / `atan` - Trigonometry
- ❌ `sqrt` - Square root
- ❌ `square` - Square
- ❌ `exact-integer-sqrt` - Integer square root
- ❌ `number->string` / `string->number` - Number conversion

**Characters (Section 6.3)**
- ❌ `char=?` / `char<?` / `char>?` / `char<=?` / `char>=?` - Character comparison
- ❌ `char-ci=?` / `char-ci<?` / etc. - Case-insensitive comparison
- ❌ `char-alphabetic?` / `char-numeric?` / `char-whitespace?` / `char-upper-case?` / `char-lower-case?` - Character predicates
- ❌ `char->integer` / `integer->char` - Character conversion
- ❌ `char-upcase` / `char-downcase` - Case conversion
- ❌ `digit-value` - Numeric character value

**Strings (Section 6.4)**
- ❌ `string` - String constructor
- ❌ `string-length` - String length
- ❌ `string-ref` / `string-set!` - String access/mutation
- ❌ `string=?` / `string<?` / `string>?` / `string<=?` / `string>=?` - String comparison
- ❌ `string-ci=?` / `string-ci<?` / etc. - Case-insensitive comparison
- ❌ `string-upcase` / `string-downcase` / `string-foldcase` - Case conversion
- ❌ `string-append` - String concatenation
- ❌ `string->list` / `list->string` - String/list conversion
- ❌ `string-copy` / `string-copy!` - String copying
- ❌ `string-fill!` - String filling
- ❌ `substring` - Substring extraction
- ❌ `string-map` / `string-for-each` - String iteration
- ❌ `string->vector` / `vector->string` - String/vector conversion

**Vectors (Section 6.5)**
- ❌ `vector` - Vector constructor
- ❌ `vector?` - Vector predicate
- ❌ `make-vector` - Vector creation
- ❌ `vector-length` - Vector length
- ❌ `vector-ref` / `vector-set!` - Vector access/mutation
- ❌ `vector->list` / `list->vector` - Vector/list conversion
- ❌ `vector-fill!` - Vector filling
- ❌ `vector-map` / `vector-for-each` - Vector iteration
- ❌ `vector-copy` / `vector-copy!` - Vector copying
- ❌ `vector-append` - Vector concatenation

**Bytevectors (Section 6.6)**
- ❌ Entire bytevector type and operations (optional in R7RS)

**Control Features (Section 6.10)**
- ❌ `call-with-current-continuation` / `call/cc` - Continuations
- ❌ `values` - Multiple values
- ❌ `call-with-values` - Multiple value handling
- ❌ `dynamic-wind` - Dynamic extent

**I/O (Section 6.13)**
- ❌ `read` - Read datum from port
- ❌ `write` / `display` / `write-simple` / `write-shared` - Output procedures (partial: `display` exists)
- ❌ `read-char` / `peek-char` - Character input
- ❌ `write-char` / `newline` - Character output (partial: `newline` exists)
- ❌ `read-line` - Line input
- ❌ `eof-object?` - EOF predicate
- ❌ `open-input-file` / `open-output-file` / `close-input-port` / `close-output-port` / `close-port` - File ports
- ❌ `open-input-string` / `open-output-string` / `get-output-string` - String ports
- ❌ `current-input-port` / `current-output-port` - Current ports
- ❌ `with-input-from-file` / `with-output-to-file` - Port redirection
- ❌ `input-port-open?` / `output-port-open?` - Port predicates
- ❌ `flush-output-port` - Output flushing
- ❌ `load` - File loading

**System Interface (Section 6.14)**
- ❌ `file-exists?` / `delete-file` - File operations
- ❌ `command-line` - Command line arguments
- ❌ `exit` / `emergency-exit` - Program termination
- ❌ `get-environment-variable` / `get-environment-variables` - Environment variables

**Time (Section 6.15)**
- ❌ `current-second` - Current time
- ❌ `current-jiffy` / `jiffies-per-second` - High-resolution timing

**Miscellaneous (Section 6.16)**
- ❌ `features` - Implementation features list

#### Program Structure (Chapter 5)
- ❌ Library system (`define-library`, `import`, `export`)
- ❌ Program structure (import declarations, definitions, expressions)
- ❌ `include` / `include-ci` / `include-library-declarations` - File inclusion
- ❌ `cond-expand` - Conditional expansion

#### Lexical Conventions (Chapter 2)
- ❌ Extended identifier characters (full Unicode support optional)
- ❌ Datum labels (`#n=` / `#n#`) - Shared/circular structure
- ❌ Block comments (`#| ... |#`)
- ❌ Datum comment (`#;`)

## Implementation Guidelines

### Priority Order

1. **High Priority** (Core R7RS features):
   - `letrec` / `letrec*` - Required for many standard library functions
   - `values` / `call-with-values` - Multiple values (used by many procedures)
   - `syntax-rules` - Hygienic macros (replaces `defmacro`)
   - Standard library procedures from `(scheme base)` (see Appendix A)
   - Library system (`define-library`, `import`, `export`)

2. **Medium Priority** (Commonly used):
   - String operations (Section 6.4)
   - Vector operations (Section 6.5)
   - Number operations (Section 6.2)
   - Character operations (Section 6.3)
   - I/O operations (Section 6.13)

3. **Low Priority** (Advanced features):
   - Continuations (`call/cc`)
   - Dynamic bindings (`parameterize`)
   - Exception handling (`guard`, `raise`)
   - Bytevectors (optional)
   - Full Unicode support (optional)

### Implementation Approach

#### For Special Forms

1. **Add to parser** (`crates/lisp_parser/src/lib.rs`):
   - Add syntax recognition in the parser
   - Ensure proper AST representation

2. **Add to evaluator** (`crates/lisp_eval/src/lib.rs`):
   - Add handling in `step_eval` for the new special form
   - Implement semantics according to R7RS spec
   - Add appropriate continuations if needed

3. **Add tests** (`crates/lisp_eval/tests/lib_tests.rs`):
   - Test basic functionality
   - Test edge cases
   - Test conformance with R7RS examples

#### For Standard Procedures

1. **Builtins** (for performance-critical operations):
   - Add variant to `define_builtins!` macro in `crates/lisp_parser/src/lib.rs`
   - Implement in `apply_builtin_with_forced_args` in `crates/lisp_eval/src/lib.rs`
   - Add tests

2. **Standard Library** (for less critical operations):
   - Add definition to `crates/lisp_parser/src/stdlib.scm`
   - The `include_stdlib!` macro will automatically generate the enum variant
   - Add tests

#### For Library System

This is a major feature requiring:
- Parser support for `define-library`, `import`, `export`
- Library resolution and loading mechanism
- Module system integration with evaluator
- See Chapter 5 of the spec for details

### Testing Strategy

1. **Unit Tests**: Test each feature in isolation
2. **Conformance Tests**: Compare behavior with R7RS spec examples
3. **Integration Tests**: Test features working together
4. **Edge Cases**: Test error conditions, boundary cases

### Reference Implementation Notes

- The spec includes many examples - use these as test cases
- Pay attention to error conditions ("it is an error if...")
- Note which features are optional vs. required
- Some features may conflict with current implementation (e.g., `defmacro` vs `syntax-rules`)

## Current Architecture Compatibility

### Compatible Features
- ✅ Trampolined evaluation supports proper tail recursion
- ✅ Arena allocation supports all value types
- ✅ GC integration supports long-running programs
- ✅ Lexical scoping supports closures

### Potential Conflicts

1. **Macros**: Current `defmacro` is non-hygienic. R7RS requires `syntax-rules` (hygienic).
   - **Solution**: Implement `syntax-rules` alongside `defmacro`, or replace it
   
2. **Multiple Values**: Current implementation returns single values.
   - **Solution**: Add `values` and `call-with-values` support
   
3. **Continuations**: Current trampoline design doesn't support `call/cc`.
   - **Solution**: This is a major architectural change - defer to low priority

4. **Library System**: Current implementation has no module system.
   - **Solution**: Implement library system per Chapter 5

## Appendix A Reference

The spec's Appendix A lists all identifiers exported by standard libraries. Focus on `(scheme base)` first, which includes:
- Core syntax (special forms)
- Essential procedures (equivalence, booleans, pairs, numbers, etc.)
- Basic I/O

Other libraries (`scheme char`, `scheme complex`, `scheme cxr`, `scheme eval`, `scheme file`, `scheme inexact`, `scheme lazy`, `scheme load`, `scheme process-context`, `scheme read`, `scheme repl`, `scheme time`, `scheme write`) can be implemented later.

## Next Steps

1. **Review spec.html** for the feature you want to implement
2. **Check current implementation** to see what exists
3. **Design the implementation** considering the architecture
4. **Implement incrementally** - add parser support, then evaluator, then tests
5. **Test thoroughly** - use spec examples as test cases
6. **Document** - update this file as features are completed

## Resources

- **Spec**: `scheme-spec-r7rs/spec.html`
- **Architecture**: `docs/LISP_ARCHITECTURE.md`
- **Arena Architecture**: `docs/ARENA_ARCHITECTURE.md`
- **Current Stdlib**: `crates/lisp_parser/src/stdlib.scm`
- **Parser**: `crates/lisp_parser/src/lib.rs`
- **Evaluator**: `crates/lisp_eval/src/lib.rs`

## Notes

- The implementation uses `no_std` - ensure any new features maintain this constraint
- The arena has fixed capacity - consider memory usage for new features
- GC integration is important - ensure new features properly mark roots
- Performance matters - prefer builtins for hot paths, stdlib for convenience
