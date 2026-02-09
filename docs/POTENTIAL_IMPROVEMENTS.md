# Potential Improvements to Grift

This document discusses in detail potential improvements to Grift across several key areas: Unicode support, I/O testing, error handling, data interning and mutation, environment management, garbage collection, macro error reporting, compile-time configuration, stack usage, and variadic functions.

## Table of Contents

1. [Unicode Library System and I/O Testing](#1-unicode-library-system-and-io-testing)
2. [Improved Error Integration and Types](#2-improved-error-integration-and-types)
3. [String, Array, Symbol Interning and Mutation](#3-string-array-symbol-interning-and-mutation)
4. [Environment Lookup, Extension, and Capture Improvements](#4-environment-lookup-extension-and-capture-improvements)
5. [GC Batch Size and Optimizations](#5-gc-batch-size-and-optimizations)
6. [Syntax-Error and Improved Errors and Traces Inside Macros](#6-syntax-error-and-improved-errors-and-traces-inside-macros)
7. [Compile-Time Configuration of Constants, Statics, and Magic Numbers](#7-compile-time-configuration-of-constants-statics-and-magic-numbers)
8. [Stack Usage Optimizations](#8-stack-usage-optimizations)
9. [Variadic Functions](#9-variadic-functions)

---

## 1. Unicode Library System and I/O Testing

### Current State

Grift inherits basic Unicode support through Rust's native `char` type (a Unicode scalar value). Characters in `Value::Char(char)` are full 32-bit Unicode code points. The lexer uses two lookup tables — `SYMBOL_CHAR_TABLE[256]` and `WHITESPACE_TABLE[256]` — that only classify the ASCII range (0–255), which means symbol names are restricted to ASCII characters. String literals can contain arbitrary Unicode characters because they pass through Rust's `char` iterator, but the character classification routines do not participate in Unicode-aware processing.

For I/O, the `IoProvider` trait in `grift_core/src/io.rs` defines a comprehensive interface, and `StdIoProvider` in `grift_std/src/io.rs` provides a standard implementation. Testing currently relies on `NullIoProvider` (a no-op provider) and integration tests that exercise `display`, `write`, and port operations. There is no systematic mechanism to capture and assert on output produced during evaluation, making it difficult to write thorough I/O tests.

### Proposed Improvements

#### 1.1 Full R7RS `(scheme char)` Library

R7RS §6.6 specifies Unicode-aware character operations. Currently missing or incomplete procedures include:

```scheme
;; Character classification (Unicode-aware)
(char-alphabetic? ch)   ; Should use Unicode General Category L*
(char-numeric? ch)      ; Should use Unicode General Category Nd
(char-whitespace? ch)   ; Should include all Unicode Zs, Zl, Zp categories
(char-upper-case? ch)   ; Unicode General Category Lu
(char-lower-case? ch)   ; Unicode General Category Ll

;; Case conversion (Unicode-aware)
(char-upcase ch)        ; Full Unicode case mapping
(char-downcase ch)      ; Full Unicode case mapping
(char-foldcase ch)      ; For case-insensitive comparison

;; Case-insensitive comparison
(char-ci=? a b)
(char-ci<? a b)
(string-ci=? a b)
```

**Implementation approach**: Since this is a `no_std` crate, we cannot depend on the standard library's Unicode tables. Two options:

1. **Minimal inline tables**: Generate compact lookup tables for the Unicode General Categories needed (L, N, Z, Lu, Ll). These can be auto-generated from the Unicode Character Database (UCD) and embedded as `static` arrays. The `unicode-xid` crate demonstrates this pattern at around 12 KB of table data.

2. **Range-based classification**: Store Unicode category ranges as sorted `(start, end)` pairs and use binary search for classification. This trades lookup speed (O(log n) vs O(1)) for smaller memory footprint, which matters in embedded contexts.

For either approach, the tables should be placed in a dedicated module (e.g., `grift_parser::unicode`) with `#[cfg]` gates to allow excluding them on extremely memory-constrained targets:

```rust
// crates/grift_parser/src/unicode.rs
#[cfg(feature = "unicode-tables")]
mod tables {
    // Auto-generated from UCD
    pub static ALPHABETIC_RANGES: &[(u32, u32)] = &[
        (0x0041, 0x005A), // A-Z
        (0x0061, 0x007A), // a-z
        (0x00C0, 0x00D6), // Latin-1 Supplement
        // ... full Unicode ranges
    ];
}

pub fn is_alphabetic(c: char) -> bool {
    #[cfg(feature = "unicode-tables")]
    { binary_search_ranges(c as u32, tables::ALPHABETIC_RANGES) }
    #[cfg(not(feature = "unicode-tables"))]
    { c.is_ascii_alphabetic() }  // Fallback to ASCII-only
}
```

#### 1.2 Unicode-Aware Symbol Names

The lexer's `SYMBOL_CHAR_TABLE` restricts symbol characters to ASCII. To support Unicode identifiers (as permitted by R7RS §7.1.1):

- Extend `is_symbol_char()` to accept Unicode letters and combining marks beyond the ASCII range.
- Maintain the O(1) ASCII fast path for the common case.
- Add a slower Unicode fallback for code points above 127.

```rust
fn is_symbol_char(c: char) -> bool {
    let code = c as u32;
    if code < 128 {
        SYMBOL_CHAR_TABLE[code as usize]  // Existing fast path
    } else {
        // Unicode identifier characters (UAX #31)
        is_xid_continue(c)
    }
}
```

#### 1.3 Structured I/O Testing Framework

The current I/O testing approach uses `NullIoProvider` or captures output through callbacks. A more systematic approach would introduce a `TestIoProvider` that records all output and provides injectable input:

```rust
/// Test-oriented I/O provider that captures all output and provides scripted input
pub struct TestIoProvider {
    /// Recorded output from display/write/newline operations, indexed by port
    output_log: Vec<(PortId, String)>,
    /// Pre-loaded input to return from read operations
    input_queue: VecDeque<String>,
    /// Track port open/close lifecycle
    port_events: Vec<PortEvent>,
}

impl TestIoProvider {
    pub fn assert_output_contains(&self, port: PortId, expected: &str) -> bool { ... }
    pub fn get_all_output(&self, port: PortId) -> String { ... }
    pub fn assert_output_sequence(&self, expected: &[&str]) -> bool { ... }
}
```

This lives in `grift_std` (which can use `std`) and enables tests like:

```rust
#[test]
fn test_display_unicode_string() {
    let io = TestIoProvider::new();
    io.push_input("(display \"héllo wörld\")");
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io(&mut io);
    eval.eval_str("(display \"héllo wörld\")").unwrap();
    assert_eq!(io.get_all_output(STDOUT), "héllo wörld");
}
```

#### 1.4 Port Lifecycle Testing

Current port tests cover basic open/close, but do not thoroughly test:

- Port exhaustion (opening more than `MAX_DYNAMIC_PORTS` = 64 ports)
- Read-after-close behavior
- String port position tracking (cursor management)
- EOF handling across port types
- Interaction between `current-input-port`/`current-output-port` and dynamic parameters

Each of these should have dedicated test cases that exercise the `IoProvider` trait methods directly and through Scheme evaluation.

---

## 2. Improved Error Integration and Types

### Current State

Grift has two main error types:

- **`ParseError`** in `grift_parser`: Contains an error kind, line/column position, and a fixed-size message buffer.
- **`EvalError`** in `grift_eval`: An 88-byte structure with `ErrorKind` (12 variants), a static message string, optional type information (`expected`/`got`), and optional `ArgCountInfo`.

The `ErrorKind` enum covers: `OutOfMemory`, `UnboundVariable`, `NotAFunction`, `WrongArgCount`, `TypeError`, `DivisionByZero`, `Parse`, `UserError`, `StackOverflow`, `NotAPair`, `Generic`, `SyntaxError`.

Errors from the parser must be manually wrapped to propagate through the evaluator. The call stack is recorded in a fixed `[StackFrame; 64]` array, but `StackFrame` only captures `(expr, func)` pairs — there is no source location (line/column) information. User-facing error objects (`Value::ErrorObject`) are distinct from internal `EvalError`, requiring separate handling paths.

### Proposed Improvements

#### 2.1 Unified Error Type Hierarchy

Replace the current flat `ErrorKind` with a structured error hierarchy that distinguishes error categories:

```rust
pub enum ErrorKind {
    // Resource errors (non-recoverable)
    OutOfMemory,
    StackOverflow,

    // Parse errors (with source location)
    Parse(ParseErrorDetail),

    // Evaluation errors (recoverable, catchable)
    Eval(EvalErrorDetail),

    // User errors (from `raise` / `error`)
    User(UserErrorDetail),
}

pub enum EvalErrorDetail {
    UnboundVariable { name: ArenaIndex },
    NotAFunction { value_type: &'static str },
    WrongArgCount { expected: ArgSpec, got: u16 },
    TypeError { expected: &'static str, got: &'static str, position: Option<u16> },
    DivisionByZero,
    NotAPair,
    IndexOutOfBounds { index: usize, length: usize },
    ImmutableModification,
    PortError { kind: PortErrorKind },
}

pub enum ArgSpec {
    Exact(u16),
    AtLeast(u16),
    Between(u16, u16),
}
```

Benefits:
- **Pattern matching**: Each error category can be handled with a single `match` arm.
- **Richer information**: `TypeError` carries the argument position, `WrongArgCount` specifies exact/minimum/range.
- **Extensibility**: New error categories (e.g., `PortError`, `ImmutableModification`) can be added without disturbing the top-level enum.

#### 2.2 Source Location Tracking

Currently, error messages lack line and column information from the original source. Improving this requires:

1. **Annotate parsed expressions with source spans**: The parser should record `(line, column)` for each expression it produces. This can be stored as a side-table (arena-allocated association list mapping `ArenaIndex → (line, col)`) rather than inflating the `Value` enum.

2. **Propagate locations through evaluation**: When the evaluator encounters an error, it looks up the current expression's source location in the side-table.

3. **Include locations in stack traces**: Each `StackFrame` should carry an optional `SourceLocation`:

```rust
pub struct StackFrame {
    pub expr: ArenaIndex,
    pub func: ArenaIndex,
    pub source_loc: Option<SourceLocation>,
}

#[derive(Copy, Clone)]
pub struct SourceLocation {
    pub line: u32,
    pub column: u16,
}
```

This keeps `StackFrame` small (adding only 8 bytes for the optional source location) while dramatically improving error messages:

```
Error: unbound variable 'foo'
  at line 15, column 3
  in (define (bar x) (+ foo x))
  called from line 22, column 1
  in (bar 42)
```

#### 2.3 Unified Internal and User-Facing Errors

Currently, `EvalError` (internal) and `Value::ErrorObject` (user-facing, from R7RS `error` procedure) are separate types. When a user catches an internal error with `guard`, it is converted to an `ErrorObject` at the boundary. This conversion can lose information.

**Proposal**: Make `EvalError` directly representable as a `Value::ErrorObject`, with an arena-allocated wrapper that preserves all internal detail:

```rust
// Store the full error in the arena as an ErrorObject
fn eval_error_to_value(error: &EvalError, arena: &Lisp<N>) -> Result<ArenaIndex, EvalError> {
    let message = arena.alloc_string(error.message)?;
    let irritants = arena.alloc_list(&error.irritants())?;
    let error_type = arena.symbol_from_str(error.kind.type_name())?;
    Ok(arena.alloc(Value::ErrorObject { message, irritants_and_type: ... })?)
}
```

This means `guard` handlers receive error objects with full detail, and `error-object-message` returns useful information even for internally-raised errors.

#### 2.4 Condition System (R7RS §6.11 Extension)

R7RS specifies a minimal condition system. Grift could extend this toward SRFI-35/SRFI-36 style condition types:

- Define a hierarchy of condition types (`&error`, `&message`, `&irritants`, `&who`, `&syntax`)
- Allow `guard` to match on condition type with predicates
- Integrate with the existing exception handler chain

This is a longer-term improvement but would bring Grift closer to production-quality error handling.

---

## 3. String, Array, Symbol Interning and Mutation

### Current State

**Symbol interning** uses an association list stored in arena slot 4. `intern_table_lookup_bytes()` does a linear scan comparing input bytes against each interned string's characters. Once found, the same `ArenaIndex` is returned, enabling O(1) symbol equality via index comparison. New symbols are prepended to the alist with `cons((string_idx, symbol_idx), table)`.

**String interning** uses a separate table in slot 5. `string_from_chars_interned()` checks the string intern table before allocating a new string, deduplicating identical string literals.

**Strings** are represented as `Value::String { len: usize, data: ArenaIndex }` where characters are stored contiguously in the arena (one `Value::Char` per slot). String operations like `string-ref` are O(1) (index into contiguous slots), but `string-append` requires allocating a new string and copying all characters.

**Arrays (vectors)** are `Value::Array { len: usize, data: ArenaIndex }` with similar contiguous storage. `vector-set!` mutates in place via `arena.set()`.

**Mutation**: `set-car!` and `set-cdr!` work via `arena.set()` which replaces the entire `Value` in a slot. String mutation (`string-set!`) is possible because characters occupy individual arena slots. However, there is no immutability enforcement — R7RS specifies that string literals should be immutable, but Grift does not track this.

### Proposed Improvements

#### 3.1 Hash-Based Symbol Intern Table

The current linear-scan intern table is O(n) where n is the number of interned symbols. For programs with many symbols, this becomes a bottleneck during parsing. A hash table would provide amortized O(1) lookup:

**Approach**: Implement a fixed-size open-addressing hash table stored in the arena:

```rust
// Hash table stored as contiguous arena slots
// Each bucket is either Nil (empty) or Cons(key_string, Cons(symbol, next_in_chain))
pub struct InternHashTable {
    buckets_start: ArenaIndex,  // Start of bucket array in arena
    bucket_count: usize,        // Number of buckets (power of 2)
    entry_count: usize,         // Number of entries
}
```

- **Hash function**: FNV-1a or SipHash over the symbol bytes, computed without allocation.
- **Collision resolution**: Chaining via arena-allocated cons cells (compatible with existing GC).
- **Sizing**: A table of 256 or 512 buckets covers most programs well. With a load factor of 0.75, this supports 192–384 symbols before degradation.
- **GC integration**: The bucket array and all chains are arena-allocated, so the GC traces them like any other data.

This would change `intern_table_lookup_bytes()` from O(n) to O(1) amortized, which matters for programs with hundreds of symbols.

#### 3.2 String Representation Optimization

Each character in a string currently occupies one full arena slot (24 bytes on 64-bit). A single-character string consumes 2 slots (header + char), and a 100-character string consumes 101 slots. This is wasteful for ASCII-heavy text.

**Option A: Packed character storage**: Store multiple characters per arena slot using a packed representation:

```rust
// Pack up to 3 ASCII characters (or 1 Unicode char) per slot
Value::PackedChars { chars: [u8; 3], count: u8 }  // For ASCII
Value::WideChar { c: char }                         // For non-ASCII
```

This reduces ASCII string storage by ~3x but complicates indexing (must compute slot offset and position within slot).

**Option B: Byte-string representation**: Store strings as byte vectors using UTF-8 encoding, with `Value::Bytevector` as the backing store:

```rust
// String backed by UTF-8 encoded bytes
Value::String { len: usize, byte_data: ArenaIndex, encoding: Encoding }
```

This is the most memory-efficient option but makes `string-ref` O(n) for non-ASCII strings (requires scanning UTF-8 bytes). A hybrid approach — UTF-8 internally with a cached index table for strings above a threshold length — balances space and access time.

#### 3.3 Immutability Tracking

R7RS §6.7 specifies that string and vector literals are immutable. Currently, Grift does not enforce this, so `(string-set! "hello" 0 #\H)` succeeds silently. Adding immutability tracking requires:

1. **Flag in the Value**: Add an `immutable` bit to `String` and `Array` variants:

```rust
Value::String { len: usize, data: ArenaIndex, immutable: bool }
Value::Array { len: usize, data: ArenaIndex, immutable: bool }
```

2. **Check on mutation**: `string-set!`, `vector-set!` check the flag and raise an error if the value is immutable.

3. **Parser marks literals**: String and vector literals produced by the parser are created with `immutable: true`. Values created by `make-string`, `make-vector`, `string-copy`, etc. are mutable.

This adds 1 byte to the `String` and `Array` variants (which may or may not increase the enum size depending on alignment) but provides correct R7RS semantics.

#### 3.4 Copy-on-Write for Interned Strings

Currently, interned strings and non-interned strings have the same representation. If a user mutates an interned string (via `string-set!`), it affects all references to that string. With immutability tracking (§3.3), interned strings would be immutable, and `string-copy` would produce a mutable copy — which is the correct R7RS behavior.

However, for `string-append` and similar operations that create new strings from existing ones, a copy-on-write optimization could share character data until mutation occurs:

- Track a "shared" flag on string data
- `string-append` initially shares character data from both operands
- On first `string-set!`, copy the shared data to a new arena allocation

This optimization is complex and may not be worthwhile given the arena's allocation model, but it would reduce memory pressure for string-heavy programs.

---

## 4. Environment Lookup, Extension, and Capture Improvements

### Current State

Environments in Grift are association lists: linked lists of `(name . value)` pairs stored as arena-allocated cons cells. The structure is `((name₁ . val₁) . ((name₂ . val₂) . ... Nil))`.

- **Lookup** (`env_lookup`): Linear scan from head to tail, comparing symbol indices. Falls back to the global environment if the local chain reaches `Nil`. Cost: O(d) where d is the depth of the environment chain.
- **Extension** (`env_extend`): Conses a new binding onto the head. Cost: O(1) per binding, but a `let` with k bindings performs k conses.
- **Mutation** (`env_set`): Linear scan to find the binding, then replaces the `cdr` of the binding cons cell. Cost: O(d).
- **Closure capture**: `Value::Lambda { params, body_env }` stores the environment at the point of lambda creation. Since environments are persistent (functional) data structures, this is O(1) — just save the arena index.

### Proposed Improvements

#### 4.1 Frame-Based Environment Representation

The cons-cell environment representation has poor cache locality — each binding lookup follows a chain of arena slot references. A frame-based representation groups bindings from the same scope into contiguous arena slots:

```
Frame layout in arena:
  [Frame header: parent_frame ArenaIndex, binding_count: usize]
  [Binding 0: name ArenaIndex, value ArenaIndex]
  [Binding 1: name ArenaIndex, value ArenaIndex]
  ...
  [Binding N-1: name ArenaIndex, value ArenaIndex]
```

Benefits:
- **Cache locality**: All bindings in the same `let`/`lambda` scope are contiguous in the arena.
- **Faster lookup within a frame**: Linear scan over contiguous memory vs. pointer chasing.
- **Reduced allocation**: One allocation per frame vs. one per binding (saves 1 cons cell per binding).

Implementation sketch:

```rust
// New Value variant for environment frames
Value::EnvFrame {
    parent: ArenaIndex,      // Parent frame (or Nil for global)
    count: u16,              // Number of bindings in this frame
    data: ArenaIndex,        // Start of contiguous binding slots
}

fn env_extend_frame(parent: EnvRef, names: &[ArenaIndex], values: &[ArenaIndex])
    -> Result<EnvRef, EvalError>
{
    let count = names.len();
    let data_start = lisp.alloc_contiguous(count * 2)?;  // name-value pairs
    for i in 0..count {
        lisp.set(data_start + i * 2, Value::Ref(names[i]));
        lisp.set(data_start + i * 2 + 1, values[i]);
    }
    let frame = lisp.alloc(Value::EnvFrame {
        parent: parent.0, count: count as u16, data: data_start
    })?;
    Ok(EnvRef(frame))
}
```

The lookup function would scan each frame's contiguous bindings before following the parent pointer. For typical Scheme programs where most lookups hit the innermost frame, this is significantly faster.

#### 4.2 De Bruijn Index Optimization

For lexically-scoped variables, the compiler/evaluator can determine at parse time the exact position of each variable in the environment chain. Using de Bruijn indices (depth, offset) pairs would make variable lookup O(1):

```scheme
;; Source:
(let ((x 1) (y 2))
  (let ((z 3))
    (+ x z)))

;; After de Bruijn analysis:
;; x = (depth: 1, offset: 0)
;; y = (depth: 1, offset: 1)
;; z = (depth: 0, offset: 0)
;; + = (global: "+")
```

With frame-based environments (§4.1), lookup becomes:

```rust
fn env_lookup_debruijn(env: EnvRef, depth: u16, offset: u16) -> ArenaIndex {
    let mut frame = env;
    for _ in 0..depth {
        frame = get_parent(frame);
    }
    get_binding_value(frame, offset)
}
```

This is a significant change requiring a compilation pass after parsing, but it eliminates symbol comparison during variable lookup entirely.

#### 4.3 Global Environment Hash Table

The global environment uses the same alist structure as local environments, making global variable lookup O(g) where g is the number of global bindings. Since many programs define hundreds of global functions, this is a meaningful bottleneck.

A hash table for the global environment (similar to §3.1) would provide O(1) global lookups:

```rust
pub struct GlobalEnv {
    buckets_start: ArenaIndex,
    bucket_count: usize,
}
```

This benefits all programs because every free variable reference falls through to the global environment.

#### 4.4 Closure Environment Trimming

When a lambda captures its environment, it captures the entire environment chain — including bindings it never references. This keeps unreferenced values alive, preventing GC from collecting them.

**Free variable analysis** at lambda creation time could trim the captured environment to only include referenced variables:

```scheme
;; Original:
(let ((x 1) (y 2) (z 3))
  (lambda (a) (+ a x)))
;; Captures: {x, y, z}  (y and z are wasted)

;; After trimming:
;; Captures: {x}  (only what's needed)
```

Implementation: Walk the lambda body, collect referenced symbols, and create a new environment frame containing only those bindings. This trades O(b) work at lambda creation (where b is the body size) for reduced GC pressure and smaller closures.

---

## 5. GC Batch Size and Optimizations

### Current State

The garbage collector in `grift_arena/src/gc.rs` implements mark-and-sweep with these characteristics:

- **Mark phase**: Iterative depth-first traversal using an explicit mark stack (`[usize; N]` on the Rust stack). Each object is traced via `trace_with_arena()`, which calls a closure for each child reference.
- **Batch processing**: When an object has many children (e.g., a long list), children are processed in batches of **16** to prevent the closure from consuming excessive stack space.
- **Sweep phase**: Single O(N) pass over all arena slots, freeing unmarked occupied slots.
- **Roots**: Up to 16 root indices collected from the evaluator state (global env, macro env, continuation chain, etc.) plus the intern tables (slots 4 and 5).
- **Trigger**: GC runs after major operations when `gc_enabled` is true, or explicitly via `(gc)`.
- **Cost**: Mark phase is O(R) where R is the number of reachable objects. Sweep phase is always O(N) where N is the arena capacity.

### Proposed Improvements

#### 5.1 Adaptive Batch Size

The fixed batch size of 16 is a compromise — too small and the GC spends excessive time on batch management overhead; too large and it risks stack overflow with deeply nested traces. An adaptive approach would scale the batch size based on available information:

```rust
const MIN_BATCH_SIZE: usize = 8;
const MAX_BATCH_SIZE: usize = 64;

fn compute_batch_size<const N: usize>() -> usize {
    // Larger arenas can afford larger batches
    // Each batch entry costs ~8 bytes of stack space
    let target = N / 1000;
    target.clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE)
}
```

Alternatively, the batch size could be a const generic parameter on the arena or GC, allowing users to tune it for their target:

```rust
impl<T: Copy + Trace, const N: usize, const BATCH: usize> Arena<T, N> {
    pub fn collect_garbage_with_batch<const BATCH: usize>(&self, ...) { ... }
}
```

#### 5.2 Generational Collection

The current collector traces all reachable objects on every collection. Many programs exhibit a "generational hypothesis" — most objects die young. A two-generation collector would significantly reduce GC pause times:

**Approach** (compatible with fixed arena):

1. **Divide arena into two regions**: Young (e.g., first 25% of slots) and Old (remaining 75%).
2. **Allocate in Young first**: New objects go into the young generation.
3. **Minor collection**: Only trace and sweep the young generation. Objects that survive a minor collection are promoted to old.
4. **Write barrier**: When an old object is mutated to point to a young object, record it in a remembered set. The remembered set is an arena-allocated list of "dirty" old objects that must be treated as roots during minor collection.
5. **Major collection**: Periodically trace and sweep the entire arena.

The write barrier adds overhead to every `set!`, `set-car!`, `set-cdr!`, and `vector-set!` operation, but minor collections would be much faster (O(Y) where Y is the young generation size, vs O(N) for the full arena).

**Feasibility in `no_std`**: The remembered set can be stored as an arena-allocated list or a fixed-size ring buffer. No heap allocation is needed.

#### 5.3 Incremental/Tri-Color Marking

For real-time or interactive use (the REPL), long GC pauses are undesirable. An incremental collector based on tri-color marking would spread the mark phase across multiple allocation cycles:

- **White**: Not yet visited (potentially garbage)
- **Gray**: Visited but children not yet traced
- **Black**: Visited and all children traced

The gray set is stored in the arena as a work list. Each allocation performs a fixed number of mark steps (e.g., 4–8 objects), amortizing the mark phase over allocations. This requires a write barrier during the mark phase to prevent the mutator from breaking the tri-color invariant (no black-to-white pointers).

This is a significant implementation effort but would eliminate GC pauses proportional to arena size.

#### 5.4 Sweep Phase Optimization

The current sweep phase always scans all N slots. Optimizations:

1. **Bitmap-based sweep**: Maintain a mark bitmap (`[u8; N/8]`) instead of per-slot mark flags. The sweep phase can then use word-level operations to find unmarked regions:

```rust
fn sweep_bitmap(bitmap: &[u64], slots: &[Cell<Slot<T>>]) {
    for (word_idx, &word) in bitmap.iter().enumerate() {
        if word == u64::MAX { continue; }  // All 64 slots are marked, skip
        let base = word_idx * 64;
        let mut unmarked = !word;
        while unmarked != 0 {
            let bit = unmarked.trailing_zeros() as usize;
            free_slot(base + bit);
            unmarked &= unmarked - 1;
        }
    }
}
```

This is significantly faster when most objects survive (common in long-running programs).

2. **Lazy sweep**: Instead of sweeping the entire arena at once, sweep incrementally — check and free slots as they are needed for allocation. This distributes sweep cost over allocation calls:

```rust
fn alloc(&self) -> Result<ArenaIndex, Error> {
    // Try free list first
    if let Some(idx) = self.pop_free() {
        return Ok(idx);
    }
    // Lazy sweep: scan forward from sweep cursor to find unmarked slot
    while self.sweep_cursor < N {
        let idx = self.sweep_cursor;
        self.sweep_cursor += 1;
        if !self.is_marked(idx) && self.is_occupied(idx) {
            self.free_slot(idx);
            return Ok(idx);
        }
    }
    Err(OutOfMemory)
}
```

#### 5.5 GC Statistics and Profiling

Enhance the `(gc)` and `(arena-stats)` built-ins to return more detailed information:

```scheme
(gc-stats)
; => ((collections . 42)
;     (total-marked . 15000)
;     (total-collected . 8000)
;     (avg-pause-us . 150)
;     (peak-live . 3200)
;     (fragmentation . 0.12))
```

This helps users tune arena sizes and GC frequency for their workloads.

---

## 6. Syntax-Error and Improved Errors and Traces Inside Macros

### Current State

Macro expansion errors in Grift report `ErrorKind::SyntaxError` with a static message string. When a `syntax-case` pattern fails to match, or a fender rejects a clause, the error message is generic: "no matching pattern in syntax-case" or "bad syntax". The error does not indicate:

- Which macro was being expanded
- Which pattern was tried and why it failed
- The source location of the macro use site
- The expansion history (which macros led to the current form)

Stack traces during macro expansion show the evaluator's continuation frames, which correspond to the macro transformer's execution — not the user's source code. This makes macro debugging extremely difficult.

### Proposed Improvements

#### 6.1 R7RS `syntax-error` Form

R7RS §4.3.1 specifies `(syntax-error message arg ...)` as a way for macros to report errors at expansion time:

```scheme
(define-syntax check-arg-count
  (syntax-rules ()
    ((check-arg-count (f a b)) #t)
    ((check-arg-count expr)
     (syntax-error "expected exactly 2 arguments" expr))))
```

Implementation:
- The evaluator recognizes `syntax-error` as a special form during macro expansion.
- It evaluates its arguments (message string and irritant expressions) and raises a `SyntaxError` with the expansion context attached.
- The error includes the macro name and the original form that triggered expansion.

#### 6.2 Macro Expansion Trace

Maintain a macro expansion stack — a list of `(macro-name, original-form, expanded-form)` triples — during expansion. When an error occurs, this stack is included in the error report:

```
Syntax error: expected exactly 2 arguments
  in expansion of (my-macro (f a b c))
  expanded from (my-let ((x 1) (y 2)) ...)
  at line 15, column 3
  
Expansion history:
  1. (my-let ((x 1) (y 2)) body)
     → (my-macro (f a b c))
  2. (my-macro (f a b c))
     → syntax-error
```

Implementation sketch:

```rust
// In the evaluator, push/pop macro expansion context
struct MacroExpansionFrame {
    macro_name: ArenaIndex,
    original_form: ArenaIndex,
    expansion_depth: u16,
}

// Stack of active macro expansions (fixed size, similar to call_stack)
macro_expansion_stack: [Option<MacroExpansionFrame>; 32],
macro_expansion_depth: usize,
```

#### 6.3 Pattern Match Failure Diagnostics

When `syntax-case` fails to match, provide detailed diagnostics about why each pattern was rejected:

```
Syntax error: no matching pattern in syntax-case
  Form: (my-macro 1 2 3)
  
  Pattern 1: (my-macro a b)
    Failed: expected 2 subforms after 'my-macro', got 3
  
  Pattern 2: (my-macro a)
    Failed: expected 1 subform after 'my-macro', got 3
    Fender: (symbol? a) → #f (a was 1, a number)
```

This requires tracking rejection reasons during pattern matching. The current implementation uses a simple boolean success/failure; extending it to return a `MatchResult` enum would enable diagnostics:

```rust
enum MatchResult {
    Success(Bindings),
    StructureMismatch { expected_len: usize, got_len: usize },
    LiteralMismatch { expected: ArenaIndex, got: ArenaIndex },
    FenderRejected { fender_expr: ArenaIndex, fender_result: ArenaIndex },
    EllipsisMismatch { min_required: usize, got: usize },
}
```

#### 6.4 Expansion-Time Warnings

Some macro usage patterns are likely errors but not strictly invalid. A warning system could flag:

- Unused pattern variables in templates
- Shadowed macro bindings
- Ellipsis patterns that always match zero elements
- Recursive macros without a base case at the first pattern

Warnings could be collected during expansion and reported after evaluation completes, rather than interrupting execution.

---

## 7. Compile-Time Configuration of Constants, Statics, and Magic Numbers

### Current State

Grift uses numerous hardcoded constants throughout the codebase:

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| `RESERVED_SLOTS` | 6 | `grift_core/src/lisp.rs` | Singleton arena slots |
| `MAX_STACK_DEPTH` | 64 | `grift_eval/src/error.rs` | Call stack trace depth |
| `MAX_DISPLAY_DEPTH` | 100 | `grift_core/src/display.rs` | Display recursion limit |
| `MAX_LIST_ELEMENTS` | 100 | `grift_core/src/display.rs` | List display truncation |
| `EQUAL_MAX_DEPTH` | 10,000 | `grift_eval/src/helpers.rs` | `equal?` recursion limit |
| `STACK_STRING_BUF_SIZE` | 1,024 | `grift_eval/src/evaluator/builtins.rs` | String operation buffer |
| `MAX_STRING_LEN` | 1,024 | `grift_parser/src/lexer.rs` | Max string literal length |
| `MAX_SYMBOL_LEN` | 64 | `grift_parser/src/lexer.rs` | Max symbol name length |
| `MAX_LIST_DEPTH` | 128 | `grift_parser/src/parser.rs` | Max parse nesting |
| `MAX_DYNAMIC_PORTS` | 64 | `grift_std/src/io.rs` | Dynamic I/O port limit |
| `MAX_NATIVE_FUNCTIONS` | 64 | `grift_eval/src/native.rs` | Native function limit |
| GC batch size | 16 | `grift_arena/src/gc.rs` | Children per GC iteration |

Additionally, many buffer sizes appear as raw literals in the code (e.g., `[u8; 48]`, `[ArenaIndex; 64]`, `[char; 256]`).

### Proposed Improvements

#### 7.1 Centralized Configuration Module

Create a configuration module that collects all tunable constants in one place, with documentation explaining each:

```rust
// crates/grift_core/src/config.rs

/// Maximum depth of the call stack for error reporting.
/// Increase for deeply recursive programs; decrease for embedded targets.
pub const MAX_STACK_DEPTH: usize = 64;

/// Maximum number of children processed per GC mark iteration.
/// Larger values reduce GC overhead but use more stack space.
pub const GC_BATCH_SIZE: usize = 16;

/// Maximum length of a string literal in source code.
/// Does not affect runtime string operations.
pub const MAX_STRING_LEN: usize = 1024;

/// Maximum length of a symbol name in source code.
pub const MAX_SYMBOL_LEN: usize = 64;

/// Maximum nesting depth for parsed expressions.
pub const MAX_PARSE_DEPTH: usize = 128;

/// Maximum number of simultaneously open dynamic I/O ports.
pub const MAX_DYNAMIC_PORTS: usize = 64;

/// Maximum number of registered native functions.
pub const MAX_NATIVE_FUNCTIONS: usize = 64;

/// Maximum recursion depth for `equal?` comparisons.
pub const EQUAL_MAX_DEPTH: usize = 10_000;

/// Maximum depth for display/write formatting.
pub const MAX_DISPLAY_DEPTH: usize = 100;

/// Maximum list elements to display before truncation.
pub const MAX_LIST_ELEMENTS: usize = 100;
```

All other modules import from this central location, making it easy to find and modify any constant.

#### 7.2 Const Generic Parameters

Some constants could be elevated to const generic parameters, allowing compile-time customization without modifying source code:

```rust
pub struct Evaluator<'a, const N: usize, const STACK_DEPTH: usize = 64> {
    lisp: &'a Lisp<N>,
    call_stack: [StackFrame; STACK_DEPTH],
    // ...
}

// Default usage:
let eval = Evaluator::<10000>::new(&lisp);

// Embedded usage with smaller stack:
let eval = Evaluator::<2000, 16>::new(&lisp);
```

This is particularly valuable for:
- **Arena size** (already a const generic: `N`)
- **Stack depth** (trade memory for trace depth)
- **GC batch size** (trade stack space for throughput)
- **Native function limit** (trade memory for extensibility)

#### 7.3 Feature-Gated Profiles

Define preset configurations for common use cases:

```toml
# Cargo.toml
[features]
default = ["profile-standard"]
profile-standard = []      # Desktop/server: generous limits
profile-embedded = []      # Microcontroller: minimal limits
profile-wasm = []          # WebAssembly: medium limits
```

```rust
#[cfg(feature = "profile-embedded")]
pub const MAX_STACK_DEPTH: usize = 16;
#[cfg(feature = "profile-embedded")]
pub const MAX_STRING_LEN: usize = 256;
#[cfg(feature = "profile-embedded")]
pub const MAX_NATIVE_FUNCTIONS: usize = 16;

#[cfg(feature = "profile-standard")]
pub const MAX_STACK_DEPTH: usize = 64;
#[cfg(feature = "profile-standard")]
pub const MAX_STRING_LEN: usize = 4096;
#[cfg(feature = "profile-standard")]
pub const MAX_NATIVE_FUNCTIONS: usize = 128;
```

#### 7.4 Eliminate Raw Numeric Literals

Audit the codebase for raw numeric literals used as buffer sizes and replace them with named constants. For example:

```rust
// Before:
let mut buf = [0u8; 48];
let mut elements = [ArenaIndex(0); 64];

// After:
let mut buf = [0u8; config::INT_FORMAT_BUF_SIZE];
let mut elements = [ArenaIndex(0); config::MAX_EXPANSION_ELEMENTS];
```

This improves readability and makes it possible to adjust all related buffer sizes by changing a single constant.

---

## 8. Stack Usage Optimizations

### Current State

Grift's `no_std` design means all temporary data lives on the Rust call stack or in the arena. Key stack consumers:

1. **GC mark bitmap**: `[bool; N]` = N bytes. For N=50,000, this is 50 KB.
2. **GC mark stack**: `[usize; N]` = 8N bytes. For N=50,000, this is 400 KB.
3. **GC batch array**: `[usize; 16]` = 128 bytes.
4. **Error call stack**: `[StackFrame; 64]` = ~1 KB.
5. **Evaluator struct**: Contains all the above plus environment references, continuation state, etc.
6. **Parser buffers**: `[char; MAX_STRING_LEN]` (1,024 chars = 4 KB), `[u8; 256]` for lookup tables.
7. **Format buffers**: Various `[u8; N]` buffers for number-to-string conversion.

Total stack usage for a 50,000-slot arena is approximately **450–500 KB**, which can be problematic for:
- Threads with default 2 MB stack size (uses 25% of stack)
- Embedded targets with limited stack (often 8–64 KB)
- WebAssembly (default 1 MB stack)

### Proposed Improvements

#### 8.1 External Mark Bitmap and Stack

The GC mark bitmap and mark stack are the largest stack consumers. Moving them into the arena itself would dramatically reduce stack usage:

**Option A: Reserve arena space for GC metadata**:

```rust
impl<T: Copy, const N: usize> Arena<T, N> {
    // Reserve first N/8 + N slots for GC metadata
    const GC_BITMAP_SLOTS: usize = (N + 7) / 8 / core::mem::size_of::<T>();
    const GC_STACK_SLOTS: usize = N;  // Worst case: all objects reachable
    const GC_OVERHEAD: usize = Self::GC_BITMAP_SLOTS + Self::GC_STACK_SLOTS;
    // User-visible capacity is N - GC_OVERHEAD
}
```

This trades arena capacity for stack safety. For N=50,000 with 24-byte values, the GC overhead would be ~260 slots for the bitmap and ~16,667 slots for the mark stack — a significant cost.

**Option B: Reuse free slots for GC metadata**:

During GC, free slots are not being used. The mark bitmap and stack could be stored in free slots, which are guaranteed to be available during collection (since we're trying to find more free slots). This is the most space-efficient option but requires careful bookkeeping.

**Option C: Iterative mark without a mark stack**:

Replace the explicit mark stack with a Schorr-Waite pointer-reversal algorithm:

```
// Schorr-Waite: traverse the object graph by reversing pointers as we go
// No mark stack needed — the graph structure itself serves as the stack
fn mark_schorr_waite(root: ArenaIndex) {
    let mut current = root;
    let mut parent = SENTINEL;
    loop {
        mark(current);
        if has_unvisited_child(current) {
            let child = next_unvisited_child(current);
            // Reverse pointer: child's slot temporarily points to parent
            swap(&child_ref, &parent);
            parent = current;
            current = child;
        } else {
            // Backtrack: restore reversed pointer
            if parent == SENTINEL { break; }
            let grandparent = get_reversed_pointer(parent);
            restore_pointer(parent, current);
            current = parent;
            parent = grandparent;
        }
    }
}
```

Schorr-Waite uses O(1) extra space (just two pointers) but is more complex to implement and temporarily modifies the arena during traversal. It is the standard technique for garbage collectors on memory-constrained systems.

#### 8.2 Lazy Evaluator Initialization

The `Evaluator` struct is large due to its fixed-size arrays. Fields like `call_stack` and `macro_expansion_stack` could use a smaller initial allocation with growth on demand:

```rust
pub struct Evaluator<'a, const N: usize> {
    // Instead of [StackFrame; 64], use a counter and arena-allocated frames
    call_stack_head: ArenaIndex,  // Arena-allocated linked list of stack frames
    call_stack_depth: u16,
    // ...
}
```

This moves the call stack into the arena (where space is already allocated) and reduces the evaluator's stack footprint. The trade-off is slightly slower call stack operations (following arena pointers vs. indexing a flat array).

#### 8.3 Split GC Into Separate Stack Frame

If the GC mark stack must remain on the Rust stack, ensure it is allocated in its own stack frame that does not overlap with the evaluator:

```rust
// The GC function should be #[inline(never)] to get its own stack frame
#[inline(never)]
fn collect_garbage(&self) {
    let mut mark_bitmap = [false; N];
    let mut mark_stack = [0usize; N];
    // ... GC work ...
    // mark_bitmap and mark_stack are freed when this function returns
}
```

This ensures the GC's large temporaries do not accumulate with the evaluator's stack usage. The `#[inline(never)]` attribute prevents the compiler from inlining the GC into the evaluation loop, which would keep both stack frames alive simultaneously.

#### 8.4 Configurable Mark Stack Size

The mark stack does not need to be N entries large in practice — the maximum depth is bounded by the longest chain of references, not the total number of objects. A much smaller mark stack (e.g., `N/4` or even `sqrt(N)`) with overflow handling would suffice:

```rust
const MARK_STACK_SIZE: usize = 4096;  // Fixed reasonable size

fn process_mark_stack(&self) {
    let mut stack = [0usize; MARK_STACK_SIZE];
    let mut top = 0;
    // ... push/pop as usual ...
    // On overflow: fall back to re-scanning from roots
    if top >= MARK_STACK_SIZE {
        // Mark stack overflow: restart from a different root
        // This is rare but handled correctly
        top = 0;
        self.restart_mark_from_overflow();
    }
}
```

This bounds the mark stack to a known size regardless of arena capacity, which is critical for embedded targets.

---

## 9. Variadic Functions

### Current State

Grift supports variadic functions through two mechanisms:

1. **Rest parameters** in lambdas: `(lambda (a b . rest) body)` binds `rest` to a list of remaining arguments. Detected at runtime by checking if the parameter list terminates with a symbol rather than `Nil`.

2. **Rest-only lambdas**: `(lambda args body)` binds `args` to the entire argument list. Used by builtins like `+`, `-`, `*`, `list`.

3. **Built-in variadic operations**: Builtins like `+`, `-`, `*` use the `BuiltinForceArg` continuation to evaluate arguments one-by-one and fold them with an accumulator.

**Limitations**:
- No `case-lambda` for dispatching on argument count (available as a standard library feature but not as a core form).
- No optional arguments with default values.
- No keyword arguments.
- Rest parameter collection allocates a fresh list, which increases GC pressure for frequently-called variadic functions.
- The `WrongArgCount` error only reports `(expected, got)` as a pair of `u16` values — it cannot express "at least N" or "between N and M" arguments.

### Proposed Improvements

#### 9.1 Core `case-lambda` Support

`case-lambda` (R7RS §4.2.9) allows a single function to dispatch on argument count:

```scheme
(define add
  (case-lambda
    ((a b) (+ a b))
    ((a b c) (+ a b c))
    ((a b . rest) (apply + a b rest))))
```

Currently available as a standard library macro, but a core implementation would be more efficient:

```rust
Value::CaseLambda {
    clauses: ArenaIndex,  // List of (params . body) pairs
    env: ArenaIndex,      // Captured environment
}
```

The evaluator matches the argument count against each clause's parameter list, selecting the first match. This avoids the overhead of macro-expanding `case-lambda` into nested `lambda` and dispatch code.

#### 9.2 Optional and Default Arguments

A common extension to Scheme is optional arguments with default values:

```scheme
;; Proposed syntax:
(define (greet name (greeting "Hello"))
  (string-append greeting ", " name "!"))

(greet "Alice")           ; => "Hello, Alice!"
(greet "Bob" "Hi")        ; => "Hi, Bob!"
```

Implementation: Desugar optional arguments into a `case-lambda`:

```scheme
;; Desugars to:
(define greet
  (case-lambda
    ((name) (greet name "Hello"))
    ((name greeting) (string-append greeting ", " name "!"))))
```

This can be done as a macro transformation in the parser or evaluator, requiring no changes to the runtime.

#### 9.3 Efficient Rest Parameter Collection

Currently, rest parameters are collected into a fresh cons-cell list during function application. For functions that immediately destructure the rest list (common in variadic arithmetic), this allocation is wasted.

**Optimization: Lazy rest list construction**: Delay the construction of the rest list until it is actually accessed:

```rust
// Instead of eagerly building the rest list:
Value::LazyRestArgs {
    args_start: ArenaIndex,  // Pointer to first unevaluated rest arg
    env: ArenaIndex,         // Environment for evaluation
    cached_list: Cell<Option<ArenaIndex>>,  // Materialized on first access
}
```

When the rest parameter is used with `car`, `cdr`, `length`, etc., the lazy list is materialized. If the rest parameter is only used with `apply`, the argument list can be forwarded directly without materialization.

**Caveat**: This optimization complicates the GC (must trace the unevaluated arguments) and may not be worth the complexity for most programs.

#### 9.4 Improved Argument Count Error Messages

Enhance `WrongArgCount` errors to express variadic expectations:

```rust
pub enum ArgSpec {
    Exact(u16),              // (lambda (a b) ...)     → "expected 2"
    AtLeast(u16),            // (lambda (a . rest) ...) → "expected at least 1"
    Between(u16, u16),       // case-lambda with range  → "expected 2 to 4"
    OneOf(u16, u16, u16),    // case-lambda with fixed  → "expected 1, 2, or 3"
}

// Error message:
// "wrong number of arguments to greet: expected at least 1, got 0"
// "wrong number of arguments to add: expected 2 or 3, got 5"
```

This requires storing the `ArgSpec` in the lambda or `case-lambda` value, adding a small amount of metadata to each function.

#### 9.5 Keyword Arguments (Future Extension)

Keyword arguments are not part of R7RS but are common in Scheme extensions (SRFI-89):

```scheme
;; Proposed syntax:
(define (make-point #:x (x 0) #:y (y 0))
  (cons x y))

(make-point #:x 3 #:y 4)   ; => (3 . 4)
(make-point #:y 5)          ; => (0 . 5)
```

This is a significant extension requiring:
- Parser support for `#:keyword` syntax
- Runtime keyword argument matching
- Integration with `case-lambda` and optional arguments

This is listed as a future extension rather than an immediate improvement, as it goes beyond R7RS.

---

## Summary of Priorities

The improvements above range from straightforward to ambitious. Here is a suggested priority order based on impact and implementation effort:

### High Priority (High impact, moderate effort)

| Improvement | Section | Rationale |
|-------------|---------|-----------|
| Centralized configuration module | §7.1 | Low risk, immediate readability benefit |
| Unified error type hierarchy | §2.1 | Better error messages for users |
| Source location tracking | §2.2 | Critical for debugging |
| `syntax-error` form | §6.1 | Required by R7RS |
| Core `case-lambda` support | §9.1 | Required by R7RS |
| Improved argument count errors | §9.4 | Low effort, better UX |

### Medium Priority (Significant impact, significant effort)

| Improvement | Section | Rationale |
|-------------|---------|-----------|
| Hash-based intern table | §3.1 | Performance improvement for large programs |
| Frame-based environments | §4.1 | Cache locality and allocation savings |
| Global environment hash table | §4.3 | Performance improvement for all programs |
| Macro expansion trace | §6.2 | Critical for macro debugging |
| External GC mark structures | §8.1 | Required for embedded targets |
| Configurable mark stack size | §8.4 | Required for embedded targets |
| R7RS `(scheme char)` library | §1.1 | R7RS conformance |
| Structured I/O testing framework | §1.3 | Development velocity |

### Lower Priority (Ambitious, high effort)

| Improvement | Section | Rationale |
|-------------|---------|-----------|
| Generational GC | §5.2 | Performance for long-running programs |
| Incremental/tri-color marking | §5.3 | Real-time responsiveness |
| De Bruijn index optimization | §4.2 | Requires compilation pass |
| Closure environment trimming | §4.4 | GC pressure reduction |
| Packed string storage | §3.2 | Memory optimization |
| Immutability tracking | §3.3 | R7RS correctness |
| Keyword arguments | §9.5 | Beyond R7RS |
| Schorr-Waite GC | §8.3 | Complexity vs. benefit trade-off |
