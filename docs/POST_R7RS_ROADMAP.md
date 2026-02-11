# Post-R7RS-small Roadmap

Once grift achieves full R7RS-small conformance, there are several meaningful directions to pursue. This document organizes them by impact and alignment with grift's core design philosophy (no_std, no_alloc, arena-based, embedded-first).

---

## 1. External Validation: Run Third-Party Test Suites

Before moving on to new features, prove conformance externally. Grift has a strong internal test suite, but running established community test suites will catch edge cases and build credibility.

**Targets:**
- **Chibi Scheme's R7RS test suite** (`tests/r7rs-tests.scm`) — the de facto conformance test
- **R7RS benchmarks** (from the Larceny project) — functional correctness under load
- **SRFI test suites** for any SRFIs you adopt later

**Why first:** If third-party tests reveal conformance gaps, you want to fix them before building on top of the standard. This also gives you a concrete "100% passing" milestone to advertise.

---

## 2. Performance: Bytecode Compilation

Grift currently tree-walks the AST via a trampolined CPS evaluator. This is elegant and correct, but leaves significant performance on the table. A bytecode compiler would be the single highest-impact performance improvement.

**Approach:**
- Define a flat bytecode instruction set (LOAD, CALL, JUMP, CLOSURE, etc.)
- Compile S-expressions to bytecode arrays stored in the arena
- Replace the tree-walking evaluator with a bytecode VM loop
- Keep the existing evaluator as a fallback/reference implementation

**Constraints to respect:**
- Bytecode and the VM loop must work within the arena (no heap vectors)
- Bytecode arrays could be stored as arena-allocated bytevectors
- The const-generic arena size may need to grow to accommodate compiled code

**Expected impact:** 5-20x speedup for computation-heavy workloads (loops, recursion, arithmetic). The improvement comes from eliminating repeated pattern matching on `Value` variants and reducing continuation frame allocation.

---

## 3. SRFI Support (Selective R7RS-large Libraries)

R7RS-large is modular — you don't need all of it. Pick the SRFIs that embedded/WASM users actually need:

**High value, low effort (implementable in Scheme):**
- **SRFI-1** (List library) — extended list operations, widely used
- **SRFI-26** (Cut/Cute) — partial application via `cut` syntax
- **SRFI-8** (receive) — convenient destructuring of multiple values

**High value, needs native support:**
- **SRFI-9** (Records) — already covered by `define-record-type`, just needs the SRFI wrapper
- **SRFI-69** or **SRFI-125** (Hash tables) — requires a hash table implementation in the arena
- **SRFI-133** (Vectors) — extended vector operations

**Domain-specific for embedded:**
- **SRFI-60** or **SRFI-151** (Bitwise operations) — essential for hardware manipulation
- **SRFI-19** (Time) — useful if targeting real-time embedded systems

**Packaging:** Each SRFI should be a separate `(import (srfi N))` library, following the R7RS library system grift already supports.

---

## 4. Full Unicode Support

The current ASCII-only case operations (`char-upcase`, `char-downcase`, `char-foldcase`, `string-upcase`, etc.) are the main conformance gap. Fixing this is straightforward but has a binary size cost.

**Options:**
- **Lookup tables:** Embed Unicode case-mapping tables as static data. The full Unicode case table is ~20KB compressed. This is feasible even in no_std.
- **Feature-gated:** Put full Unicode behind a `unicode` feature flag. Default to ASCII-only for size-constrained embedded targets.
- **Existing crates:** Consider `unicode-segmentation` or hand-rolled tables to avoid pulling in `std`.

**Scope:** R7RS only requires Unicode codepoint awareness, not full locale-aware collation. The required operations are:
- `char-upcase` / `char-downcase` / `char-foldcase`
- `char-alphabetic?` / `char-numeric?` / `char-whitespace?` / `char-upper-case?` / `char-lower-case?`
- Corresponding `string-*` variants

---

## 5. WASM Target Hardening

Grift's no_std design makes it a natural fit for WebAssembly, but there's work needed to make it a polished WASM offering.

**Tasks:**
- **Build and test under `wasm32-unknown-unknown`** — verify the full test suite passes
- **WASM-specific I/O bridge** — R7RS I/O (`read`, `write`, `display`) needs to be wired to JavaScript console/DOM
- **Published WASM package** — npm-publishable package with JS bindings
- **Interactive web playground** — embed grift in a browser-based REPL (great for adoption)
- **Size optimization** — measure and minimize the `.wasm` binary size with `wasm-opt`

---

## 6. Rust FFI / Embedding API

Make grift easy to embed in Rust applications. The current API (`Lisp::new()`, `Evaluator::new()`, `eval_str()`) is functional but minimal.

**Improvements:**
- **Register custom Rust functions as Scheme builtins** — let users extend the language without modifying grift's source
- **Type-safe value extraction** — ergonomic `TryFrom<Value>` impls for common Rust types
- **Callback support** — allow Scheme code to call registered Rust closures
- **Error interop** — map Scheme exceptions to Rust `Result` types
- **`serde` integration** (feature-gated) — serialize/deserialize Scheme values

This is high-impact for adoption. An embeddable Scheme with a good Rust API competes directly with `rlua`, `rhai`, and `mlua`.

---

## 7. Debugger and Tooling

**REPL improvements:**
- Line editing (integrate `rustyline` or similar, behind `std` feature)
- Tab completion for symbols
- `,inspect` / `,trace` / `,step` REPL commands

**Debugging:**
- Step-through execution (leverage the existing continuation-based evaluator — each continuation is effectively a "stack frame")
- Breakpoints on function entry
- Expression-level tracing

**Profiling:**
- Count function calls and GC collections
- Arena allocation heat maps (which code paths allocate the most)

---

## 8. GC Improvements

The current mark-and-sweep GC is correct but has room for improvement:

- **Incremental GC** — spread marking across multiple evaluation steps to avoid GC pauses. Important for real-time embedded use cases.
- **Generational hints** — track allocation age to skip long-lived objects during collection. The arena's slot array makes this feasible with a generation byte per slot.
- **Compaction** — defragment the arena by moving objects. This is hard with the current `ArenaIndex` model (all indices would need updating) but would improve cache locality.
- **Weak references** — useful for caches and observer patterns in Scheme code.

---

## 9. Embedded Hardware Demos

Concrete hardware demonstrations would differentiate grift from other Scheme implementations.

**Targets:**
- **Raspberry Pi Pico (RP2040)** — popular, cheap, good Rust support
- **ESP32** — WiFi-enabled, huge embedded community
- **STM32** — industry-standard ARM Cortex-M

**Demos:**
- Blink an LED from Scheme code
- Read sensor data and process it with Scheme
- Interactive REPL over UART

The `grift_arena_embedded` crate already exists for hardware abstractions — build on it.

---

## Suggested Priority Order

| Priority | Item | Rationale |
|----------|------|-----------|
| 1 | External test suites | Validate conformance before building further |
| 2 | Full Unicode | Closes the last R7RS-small gaps |
| 3 | Rust embedding API | Drives adoption; makes grift useful to others |
| 4 | High-value SRFIs (1, 9, 151) | Practical utility for real programs |
| 5 | WASM target + web playground | Visibility and discoverability |
| 6 | Bytecode compilation | Performance step-change |
| 7 | Debugger/tooling | Developer experience |
| 8 | Embedded hardware demos | Differentiator |
| 9 | GC improvements | Long-term investment |

Items 1-3 are about solidifying the foundation. Items 4-5 are about reach. Items 6-9 are about depth. The order can shift based on your goals — if grift is primarily for embedded, move items 8 and 9 up; if it's for WASM adoption, prioritize item 5.
