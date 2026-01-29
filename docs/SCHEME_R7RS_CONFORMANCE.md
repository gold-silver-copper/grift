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
- ✅ Special forms: `quote`, `if`, `cond`, `case`, `lambda`, `define`, `set!`, `let`, `let*`, `letrec`, `letrec*`, `begin`, `and`, `or`, `when`, `unless`, `do`, `quasiquote`, `eval`, `apply`, `values`

#### Built-in Procedures (Chapter 6)
- ✅ **Equivalence**: `eq?`, `eqv?`, `equal?`
- ✅ **Booleans**: `not`, `boolean?`
- ✅ **Pairs/Lists**: `car`, `cdr`, `cons`, `list`, `null?`, `pair?`, `set-car!`, `set-cdr!`
- ✅ **Numbers**: `+`, `-`, `*`, `/`, `modulo`, `remainder`, `quotient`, `=`, `<`, `>`, `<=`, `>=`, `number?`, `integer?`, `exact?`, `inexact?`, `exact-integer?`
- ✅ **Floats**: Full support for floating-point literals (`3.14`, `1e-15`, `+nan.0`, `+inf.0`, `-inf.0`)
- ✅ **Number Operations**: `abs`, `max`, `min`, `gcd`, `lcm`, `expt`, `square`, `floor`, `ceiling`, `truncate`, `round`
- ✅ **Number Predicates**: `zero?`, `positive?`, `negative?`, `odd?`, `even?`
- ✅ **Type Predicates**: `symbol?`, `procedure?`
- ✅ **Characters**: `char?`, `char=?`, `char<?`, `char>?`, `char<=?`, `char>=?`, `char->integer`, `integer->char`, `char-upcase`, `char-downcase`
- ✅ **Strings**: `string?`, `make-string`, `string`, `string-length`, `string-ref`, `string-set!`, `string=?`, `string<?`, `string>?`, `string<=?`, `string>=?`, `string-append`, `string->list`, `list->string`, `substring`, `string-copy`
- ✅ **I/O**: `display`, `newline`, `error`

#### Standard Library Functions (`stdlib.scm`)
- ✅ **List Operations**: `map`, `filter`, `fold`, `fold-right`, `reduce`, `length`, `append`, `reverse`, `nth`, `take`, `drop`, `zip`, `list?`, `list-ref`, `list-tail`, `list-copy`, `make-list`, `list-set!`, `last`, `last-pair`
- ✅ **Search Functions**: `member`, `memq`, `memv`, `member-equal`, `assoc`, `assq`, `assv`, `assoc-equal`, `find`
- ✅ **Higher-Order Functions**: `for-each`, `any`, `every`, `filter-map`, `partition`, `remove`, `delete`
- ✅ **Utilities**: `range`, `compose`, `identity`, `constantly`, `flip`, `curry`, `sign`, `boolean-eq`
- ✅ **Car/Cdr Compositions**: Full set of `caar`, `cadr`, `cdar`, `cddr`, `caaar`, `caadr`, `cadar`, `cdaar`, `cdadr`, `cddar`, `caddr`, `cdddr`, `cadddr`, `cddddr`
- ✅ **Math Functions**: `sqrt`, `exp`, `log`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan1`, `atan2`, `sinh`, `cosh`, `tanh`, `log10`, `log2`, `log-base`
- ✅ **Float Predicates**: `nan?`, `infinite?`, `finite?`, `real?`, `rational?`, `complex?`
- ✅ **Type Conversion**: `exact->inexact`, `inexact->exact`
- ✅ **Constants**: `get-pi`, `get-e`, `get-epsilon`
- ✅ **Character Predicates**: `char-alphabetic?`, `char-numeric?`, `char-whitespace?`, `char-upper-case?`, `char-lower-case?`, `digit-value`, `char-foldcase`, `char-ci=?`, `char-ci<?`, `char-ci>?`, `char-ci<=?`, `char-ci>=?`
- ✅ **String Functions**: `string-upcase`, `string-downcase`, `string-foldcase`, `string-ci=?`

### 🔧 Implementation Extensions (Non-R7RS)

These features are intentionally non-R7RS for embedded systems and runtime control:

#### GC Control (Embedded Extension)
- `gc` - Manually trigger garbage collection
- `gc-enable` - Enable automatic garbage collection
- `gc-disable` - Disable automatic garbage collection
- `gc-enabled?` - Check if GC is enabled
- `arena-stats` - Get arena statistics as a list

#### Array Operations (Embedded Extension)
- `make-array` - Create an array with given length and initial value
- `array-ref` - Get element at index (O(1))
- `array-set!` - Set element at index (O(1))
- `array-length` - Get array length (O(1))
- `array?` - Check if value is an array

#### Native Function FFI (Embedded Extension)
- Native Rust functions can be registered and called from Lisp
- Used for hardware access in embedded contexts

---

## Multi-Phase R7RS Conformance Plan

### Phase 1: Core Language Foundation ✅ COMPLETED
**Goal**: Ensure all basic R7RS semantics are correctly implemented

#### 1.1 Binding Constructs ✅
- [x] Implement `letrec` - Recursive let binding (Section 4.2.2)
- [x] Implement `letrec*` - Sequential recursive let binding
- [x] Verify `let` and `let*` follow R7RS semantics exactly

#### 1.2 Core Procedures ✅
- [x] Implement `for-each` - Apply procedure for side effects
- [x] Implement `list-tail` - Return sublist starting at index
- [x] Implement `list-ref` - Return element at index
- [x] Implement `list?` - Check if value is a proper list
- [x] Implement `list-copy` - Create a copy of a list

#### 1.3 Number Operations ✅
- [x] Implement `abs` - Absolute value
- [x] Implement `max` / `min` - Maximum and minimum
- [x] Implement `quotient` - Integer quotient (alias for truncate-quotient)
- [x] Implement `gcd` / `lcm` - Greatest common divisor / least common multiple
- [x] Implement `floor` / `ceiling` / `truncate` / `round` - Rounding operations (works for floats too)
- [x] Implement `expt` - Exponentiation
- [x] Implement `square` - Square of a number
- [x] Implement `zero?` / `positive?` / `negative?` / `odd?` / `even?` - Predicates

#### 1.4 Numerical Tower ✅ (Section 6.2)
- [x] Implement floating-point number parsing (e.g., `3.14`, `1e-15`, `-2.5`)
- [x] Implement special float values (`+nan.0`, `+inf.0`, `-inf.0`)
- [x] Mixed integer/float arithmetic (auto-promotion)
- [x] Float predicates: `nan?`, `infinite?`, `finite?`
- [x] Type predicates: `real?`, `rational?`, `complex?`
- [x] Type conversion: `exact->inexact`, `inexact->exact`

#### 1.5 Transcendental Functions ✅ (Section 6.2.6)
All implemented in `stdlib.scm` using Taylor series and Newton-Raphson methods (no libm dependency):
- [x] Constants: `get-pi`, `get-e`, `get-epsilon`
- [x] Square root: `sqrt` (Newton-Raphson)
- [x] Exponential: `exp` (Taylor series)
- [x] Logarithm: `log` (Newton's method + series)
- [x] Logarithm variants: `log10`, `log2`, `log-base`
- [x] Trigonometric: `sin`, `cos`, `tan` (Taylor series)
- [x] Inverse trig: `asin`, `acos`, `atan1`, `atan2` (Newton's method + series)
- [x] Hyperbolic: `sinh`, `cosh`, `tanh`

#### 1.6 Convenience Conditionals ✅
- [x] Implement `when` - Execute body when test is true
- [x] Implement `unless` - Execute body when test is false

#### 1.7 Additional Stdlib Functions ✅
- [x] Implement `make-list` - Create a list of k elements with fill value
- [x] Implement `list-set!` - Store obj at element k of list
- [x] Implement `last` / `last-pair` - Access last element/pair
- [x] Implement `any` / `every` - Higher-order predicates
- [x] Implement `find` - Find first element matching predicate
- [x] Implement `partition` - Split list by predicate
- [x] Implement `remove` / `delete` - Remove elements from list
- [x] Implement `fold-right` / `reduce` - Right fold operations
- [x] Implement full c...r accessors (up to 4 levels: `cddddr`, `cadddr`)

### Phase 2: String and Character Support ✅ COMPLETED
**Goal**: Full R7RS string and character operations

#### 2.1 Character Literal Parsing
- [x] Character literal syntax: `#\a`, `#\A`, `#\0`, `#\(`, etc.
- [x] Named characters: `#\newline`, `#\space`, `#\tab`, `#\return`, `#\null`, `#\alarm`, `#\backspace`, `#\delete`, `#\escape`
- [x] Hex character literals: `#\x41` (for 'A'), `#\x20` (for space)

#### 2.2 String Literal Parsing  
- [x] Basic string literals: `"hello"`, `""` (empty string)
- [x] Escape sequences: `\n`, `\t`, `\r`, `\"`, `\\`, `\a`, `\b`, `\|`
- [x] Hex escapes: `\x41;` (note the terminating semicolon per R7RS)
- [x] Line continuation: `\` followed by whitespace and newline

#### 2.3 Character Operations (Section 6.6)
- [x] Implement `char?` predicate (builtin)
- [x] Implement `char=?` / `char<?` / `char>?` / `char<=?` / `char>=?` - Comparison (builtins)
- [x] Implement `char->integer` / `integer->char` - Conversion (builtins)
- [x] Implement `char-upcase` / `char-downcase` - Case conversion (builtins)
- [x] Implement character predicates (stdlib): `char-alphabetic?`, `char-numeric?`, `char-whitespace?`, `char-upper-case?`, `char-lower-case?`
- [x] Implement `digit-value` - Get numeric value of digit character (stdlib)
- [x] Implement `char-foldcase` - Unicode simple case-folding (stdlib)
- [x] Implement case-insensitive comparisons (stdlib): `char-ci=?`, `char-ci<?`, `char-ci>?`, `char-ci<=?`, `char-ci>=?`

#### 2.4 String Operations (Section 6.7)
- [x] Implement `string?` predicate (builtin)
- [x] Implement `make-string` - Create string with fill character (builtin)
- [x] Implement `string` constructor - Create string from characters (builtin)
- [x] Implement `string-length` (builtin)
- [x] Implement `string-ref` / `string-set!` - Access and mutation (builtins)
- [x] Implement `string=?` / `string<?` / `string>?` / `string<=?` / `string>=?` - Comparison (builtins)
- [x] Implement `string-append` - Concatenation (builtin)
- [x] Implement `string->list` / `list->string` - Conversion (builtins)
- [x] Implement `substring` - Substring extraction (builtin)
- [x] Implement `string-copy` - String copying (builtin)
- [x] Implement `string-upcase` / `string-downcase` / `string-foldcase` - Case conversion (stdlib)
- [x] Implement `string-ci=?` - Case-insensitive equality (stdlib)

### Phase 3: Vector Support
**Goal**: R7RS vector operations (distinct from arrays)

#### 3.1 Vector Operations (Section 6.8)
- [ ] Implement `vector` constructor
- [ ] Implement `vector?` predicate
- [ ] Implement `make-vector` - Create vector with optional fill
- [ ] Implement `vector-length`
- [ ] Implement `vector-ref` / `vector-set!` - Access and mutation
- [ ] Implement `vector->list` / `list->vector` - Conversion
- [ ] Implement `vector-fill!` - Fill vector with value
- [ ] Implement `vector-copy` - Copy vector

### Phase 4: Multiple Values
**Goal**: Full multiple value support

#### 4.1 Multiple Values (Section 6.10)
- [ ] Verify `values` implementation
- [ ] Implement `call-with-values` - Receive multiple values
- [ ] Implement `let-values` / `let*-values` - Bind multiple values
- [ ] Implement `define-values` - Define multiple values

### Phase 5: Hygienic Macros
**Goal**: R7RS-compliant macro system

#### 5.1 Syntax-Rules (Section 4.3.2)
- [ ] Implement `syntax-rules` - Pattern-based macros
- [ ] Implement `let-syntax` / `letrec-syntax` - Local syntax bindings
- [ ] Implement `define-syntax` - Top-level syntax definitions
- [ ] Implement `syntax-error` - Macro error signaling

### Phase 6: Control Features
**Goal**: Advanced control flow

#### 6.1 Conditionals
- [ ] Implement `when` / `unless` - Convenience conditionals
- [ ] Implement `cond-expand` - Feature-based conditional expansion
- [ ] Implement `case-lambda` - Multiple-arity procedures

#### 6.2 Exception Handling (Section 6.11)
- [ ] Implement `guard` - Exception handling syntax
- [ ] Implement `raise` / `raise-continuable` - Exception raising
- [ ] Implement `with-exception-handler` - Exception handler installation
- [ ] Implement `error-object?` / `error-object-message` / `error-object-irritants`

#### 6.3 Dynamic Bindings (Section 4.2.6)
- [ ] Implement `make-parameter` - Create parameter object
- [ ] Implement `parameterize` - Dynamic binding

### Phase 7: I/O System
**Goal**: R7RS I/O operations

#### 7.1 Ports (Section 6.13)
- [ ] Implement port types and predicates
- [ ] Implement `current-input-port` / `current-output-port` / `current-error-port`
- [ ] Implement `open-input-string` / `open-output-string` / `get-output-string`
- [ ] Implement `read-char` / `peek-char` / `write-char`
- [ ] Implement `read-line` / `read-string`
- [ ] Implement `write` / `write-simple` - Datum output
- [ ] Implement `read` - Datum input

### Phase 8: Library System
**Goal**: R7RS module system

#### 8.1 Libraries (Section 5.6)
- [ ] Implement `define-library` syntax
- [ ] Implement `import` declarations
- [ ] Implement `export` declarations
- [ ] Implement library name resolution
- [ ] Implement `include` / `include-ci` - File inclusion

### Phase 9: Advanced Features (Optional)
**Goal**: Complete R7RS conformance

#### 9.1 Continuations
- [ ] Implement `call-with-current-continuation` / `call/cc`
- [ ] Implement `dynamic-wind`

#### 9.2 Lazy Evaluation (scheme lazy library)
- [ ] Implement `delay` / `force` / `delay-force`
- [ ] Implement `make-promise` / `promise?`

#### 9.3 Environments
- [ ] Implement `environment` - Create evaluation environment
- [ ] Implement `scheme-report-environment`
- [ ] Implement `null-environment`

---

## Implementation Guidelines

### For Special Forms

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

### For Standard Procedures

1. **Builtins** (for performance-critical operations):
   - Add variant to `define_builtins!` macro in `crates/lisp_parser/src/lib.rs`
   - Implement in `apply_builtin` in `crates/lisp_eval/src/lib.rs`
   - Add tests

2. **Standard Library** (for less critical operations):
   - Add definition to `crates/lisp_parser/src/stdlib.scm`
   - The `include_stdlib!` macro will automatically generate the enum variant
   - Add tests

### Testing Strategy

1. **Unit Tests**: Test each feature in isolation
2. **Conformance Tests**: Compare behavior with R7RS spec examples
3. **Integration Tests**: Test features working together
4. **Edge Cases**: Test error conditions, boundary cases

---

## Architecture Notes

### Compatible Features
- ✅ Trampolined evaluation supports proper tail recursion
- ✅ Arena allocation supports all value types
- ✅ GC integration supports long-running programs
- ✅ Lexical scoping supports closures

### Potential Challenges

1. **Continuations**: Current trampoline design doesn't support `call/cc`.
   - **Impact**: Low priority - defer to Phase 9
   
2. **Multiple Values**: Partially implemented - needs `call-with-values`.
   - **Impact**: Medium priority - Phase 4

3. **Library System**: No module system yet.
   - **Impact**: Required for full conformance - Phase 8

---

## Resources

- **Spec**: `scheme-spec-r7rs/spec.html`
- **Architecture**: `docs/LISP_ARCHITECTURE.md`
- **Arena Architecture**: `docs/ARENA_ARCHITECTURE.md`
- **Current Stdlib**: `crates/lisp_parser/src/stdlib.scm`
- **Parser**: `crates/lisp_parser/src/lib.rs`
- **Evaluator**: `crates/lisp_eval/src/lib.rs`

---

## Notes

- The implementation uses `no_std` - ensure any new features maintain this constraint
- The arena has fixed capacity - consider memory usage for new features
- GC integration is important - ensure new features properly mark roots
- Performance matters - prefer builtins for hot paths, stdlib for convenience
- Extensions (GC control, arrays, FFI) are intentionally kept for embedded use
