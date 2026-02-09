# Hygienic Macros Without Heap Allocation: The Design and Implementation of Grift's Macro System

**Abstract.** Hygienic macro systems are a cornerstone of modern Scheme implementations, yet they have historically required heap allocation for syntax objects, environment capture, and phase-separated compilation. We present the macro system of Grift, a minimal R7RS-compatible Scheme implementation built entirely on a fixed-size, `no_std`, `no_alloc` arena allocator. Grift implements `syntax-case`-based hygienic macros using the mark-based algorithm of Clinger and Rees (1991), but makes several novel design choices: it operates without phase separation, treats syntax objects as first-class values, and performs all macro expansion during evaluation rather than as a separate compilation phase. We describe the system's architecture, compare it to the macro systems of Racket and Guile, and discuss the trade-offs that arise from unifying macro expansion and evaluation in a constrained-memory environment.

---

## 1. Introduction

Macro systems are among the most powerful features of the Lisp family of languages. They enable programmers to extend the language with new syntactic forms, implement domain-specific languages, and abstract over patterns that cannot be captured by functions alone. Since the introduction of hygienic macros by Kohlbecker et al. (1986), Scheme implementations have steadily improved their macro facilities, culminating in the `syntax-case` system standardized in R6RS (Sperber et al. 2007) and the `syntax-rules` facility of R7RS (Shinn, Cowan, and Gleckler 2013).

However, existing implementations of hygienic macros—most notably those in Racket (Flatt 2002) and Guile (Wingo 2011)—depend heavily on heap allocation. Syntax objects are represented as records containing source expressions, lexical context information, and source locations, all of which are dynamically allocated. Phase separation, which isolates compile-time macro environments from runtime environments, requires maintaining multiple heaps or module registries. These requirements make it difficult to implement hygienic macros in environments where heap allocation is unavailable, such as embedded systems, WebAssembly without WASI, or bare-metal firmware.

Grift is a minimal Scheme implementation that addresses this gap. It provides a fully hygienic `syntax-case` macro system that operates entirely within a fixed-size, stack-allocated arena. The system requires no heap allocation (`no_alloc`), no standard library (`no_std`), and no operating system support. Despite these constraints, Grift supports `syntax-rules`, `syntax-case` with fenders, `bound-identifier=?`, `free-identifier=?`, `datum->syntax`, ellipsis patterns, and first-class syntax objects.

This paper makes the following contributions:

1. We describe the architecture of a hygienic macro system that operates without heap allocation, using a fixed-size arena with mark-and-sweep garbage collection (Section 3).
2. We explain Grift's single-phase evaluation model, in which macro expansion occurs during evaluation rather than as a separate compilation pass, and analyze its implications for expressiveness and safety (Section 4).
3. We compare Grift's design to the macro systems of Racket and Guile, identifying trade-offs in expressiveness, safety, and resource usage (Section 5).
4. We discuss the implications of Grift's design for module systems, embedded deployment, and language experimentation (Section 6).

## 2. Background

### 2.1 Hygienic Macros

A macro is *hygienic* if it automatically prevents unintended variable capture. Consider the classic `swap!` macro:

```scheme
(define-syntax swap!
  (syntax-rules ()
    ((_ a b)
     (let ((temp a))
       (set! a b)
       (set! b temp)))))
```

In a naïve macro system, expanding `(swap! x temp)` would introduce a binding for `temp` that shadows the user's variable of the same name. Hygienic macros guarantee that the `temp` introduced by the macro is distinct from any `temp` in the user's code.

Kohlbecker et al. (1986) introduced the first hygienic macro algorithm based on automatic renaming. Clinger and Rees (1991) refined this with a mark-based approach in their "Macros that Work" algorithm, which forms the basis of most modern implementations. Dybvig, Hieb, and Bruggeman (1992) developed `syntax-case`, a procedural macro facility that provides the power of `defmacro`-style macros with the hygiene guarantees of `syntax-rules`.

### 2.2 Phase Separation

Racket introduced the concept of *phase separation* (Flatt 2002), which divides program execution into distinct levels:

- **Phase 0** (runtime): Normal program execution.
- **Phase 1** (compile-time): Macro transformer execution.
- **Phase 2** and beyond: Macros that define macros.

Each phase has its own environment, and values cannot cross phase boundaries without explicit lifting. This discipline enables separate compilation, caching of expanded code, and static detection of phase errors.

Guile adopted a similar model via its `psyntax`-based expander (Wingo 2011), which implements phased macro expansion with module-level phase control.

### 2.3 Arena Allocation

Arena (or region-based) allocation is a memory management strategy in which objects are allocated sequentially from a contiguous block of memory and freed collectively (Tofte and Talpin 1997). Arenas provide O(1) allocation, predictable memory usage, and excellent cache locality. They are widely used in systems programming—for example, in compilers, game engines, and network protocol parsers—but are uncommon in language runtime implementations.

## 3. Grift's Macro Architecture

### 3.1 System Overview

Grift is structured as a set of Rust crates, each operating under `no_std` and `no_alloc` constraints (with the sole exception of the REPL, which uses `std` for I/O). The macro system spans two crates:

- **`grift_core`**: Defines the `Value` type, which includes a `Syntax` variant for syntax objects, and contains the prelude of standard macro definitions written in Scheme.
- **`grift_eval`**: Implements the macro expander in the `expand.rs` module, including gensym generation, mark application, pattern matching, and template transcription.

The key insight enabling Grift's design is that all components of a syntax object—the expression, marks, substitution environment, and lexical environment—can be represented as arena indices. Since `ArenaIndex` is a `Copy` type (a lightweight wrapper around `usize`), syntax objects can be stored directly in the arena without heap allocation.

### 3.2 Value Representation

Every Lisp value in Grift is represented by the `Value` enum, which must implement the `Copy` trait for arena storage:

```rust
pub enum Value {
    Nil,
    True,
    False,
    Number(isize),
    Float(fsize),
    Char(char),
    Cons { car: ArenaIndex, cdr: ArenaIndex },
    Symbol(ArenaIndex),
    Lambda { params: ArenaIndex, body_env: ArenaIndex },
    Syntax { expr: ArenaIndex, marks: ArenaIndex,
             subst: ArenaIndex, lex_env: ArenaIndex },
    // ... other variants omitted for brevity
}
```

The `Syntax` variant stores four arena indices:

1. **`expr`**: The underlying S-expression.
2. **`marks`**: A list of marks (gensyms) representing the macro expansion scopes through which the syntax has passed.
3. **`subst`**: A substitution environment mapping identifiers to their renamed forms.
4. **`lex_env`**: The lexical environment captured at the point of syntax creation.

Each of these components is itself stored in the arena as a chain of cons cells, symbols, or other values. No pointer or heap-allocated structure is required.

### 3.3 The Arena Allocator

The arena is parameterized by its capacity at compile time via Rust's const generics:

```rust
pub struct Arena<T: Copy, const N: usize> {
    slots: [Cell<Slot<T>>; N],
    free_head: Cell<usize>,
    len: Cell<usize>,
    gc_enabled: Cell<bool>,
}
```

Each slot is either free (containing a pointer to the next free slot) or occupied (containing a value). Allocation pops from the head of the free list in O(1); deallocation pushes to the head of the free list in O(1). The arena supports mark-and-sweep garbage collection using a fixed-size mark stack (no heap-allocated work list), with batched child processing to bound stack depth during tracing.

This design means that macro expansion—which may allocate many intermediate syntax objects, gensyms, and environment frames—operates within the same bounded memory as the rest of the evaluator. If the arena is exhausted during expansion, the system returns an `OutOfMemory` error rather than invoking undefined behavior.

### 3.4 Mark-Based Hygiene

Grift implements hygiene using the mark-based algorithm of Clinger and Rees (1991). Each macro expansion generates a fresh mark (a unique gensym):

```rust
pub fn mark_syntax(&mut self, stx: ArenaIndex) -> EvalResult {
    let (expr, marks, subst, lex_env) =
        self.lisp.syntax_parts_with_env(stx)?;
    let new_mark = self.gensym_simple()?;
    let new_marks = self.lisp.cons(new_mark, marks)?;
    self.lisp.syntax_with_env(expr, new_marks, subst, lex_env)
}
```

Two identifiers are considered `bound-identifier=?` if and only if they share both the same symbolic name and the same list of marks. This ensures that an identifier introduced by a macro expansion is distinct from a user-supplied identifier of the same name, even if they appear in the same scope after expansion.

The `free-identifier=?` predicate, by contrast, resolves both identifiers in their respective lexical environments and checks whether they refer to the same binding. This enables macros to compare identifiers across different scopes—essential for implementing literal keyword matching in `syntax-case`.

### 3.5 Gensym Generation Without Heap Allocation

Fresh symbols are generated using a monotonically increasing counter and formatted into a fixed-size stack buffer:

```rust
pub fn gensym(&mut self, base: &str) -> EvalResult {
    use core::fmt::Write;
    let mut buf = [0u8; 64];
    let mut cursor = WriteCursor::new(&mut buf);
    write!(cursor, "#:{}{}", base, self.gensym_counter).ok();
    self.gensym_counter += 1;
    // Intern the symbol in the arena
    self.lisp.intern(cursor.as_str().unwrap_or("#:g?"))
}
```

The `WriteCursor` helper formats into a stack-allocated byte array, avoiding any heap allocation. Each gensym is interned in the arena as a new symbol, consuming one arena slot.

### 3.6 Pattern Matching and Template Transcription

The `syntax-case` form performs pattern matching on syntax objects, binding pattern variables to matched sub-expressions. Grift's implementation supports:

- **Literal keywords**: Compared using `free-identifier=?` semantics.
- **Pattern variables**: Bound to matched sub-expressions.
- **Ellipsis patterns**: `(pattern ...)` matches zero or more repetitions.
- **Nested ellipsis**: `((a ...) ...)` for multi-level repetitions.
- **Fenders (guards)**: Optional boolean expressions that must evaluate to true for a clause to match.
- **`datum->syntax`**: Creates syntax objects with specified scopes, enabling controlled hygiene breaking.

Template transcription walks the template expression, replacing pattern variables with their bound values and replicating ellipsis-marked sub-templates according to the length of the matched list.

### 3.7 The `syntax-rules` to `syntax-case` Transformation

Grift implements `syntax-rules` as a macro over `syntax-case`, defined in the Scheme prelude:

```scheme
(define-syntax syntax-rules
  (lambda (form)
    (syntax-case form ()
      ((syntax-rules (lit ...) ((keyword . pattern) template) ...)
       (syntax
         (lambda (x)
           (syntax-case x (lit ...)
             ((dummy . pattern) (syntax template)) ...)))))))
```

This definition uses nested ellipsis to transform an arbitrary number of `syntax-rules` clauses into corresponding `syntax-case` clauses. The resulting lambda serves as the macro transformer. This bootstrapping approach means that only `syntax-case` and `define-syntax` need to be implemented as native special forms; all other macro-definition facilities can be built on top.

## 4. Single-Phase Evaluation

### 4.1 Evaluation-Time Macro Expansion

Unlike Racket and Guile, Grift does not perform macro expansion as a separate compilation phase. Instead, macro expansion occurs during evaluation:

1. When the evaluator encounters a `define-syntax` form, it evaluates the transformer expression in the current environment and binds the result in the macro environment.
2. When the evaluator encounters a form whose operator is bound in the macro environment, it invokes the transformer, passing the entire form as a syntax object.
3. The result of the transformer is re-evaluated in place of the original form.

This cycle is implemented via the `MacroResult` continuation type, which re-enters the evaluator with the expanded expression.

### 4.2 Implications of the Single-Phase Model

The absence of phase separation has several consequences:

**Increased expressiveness.** Syntax objects can capture and refer to runtime bindings. A function can construct a syntax object at runtime and pass it to a macro transformer:

```scheme
(define (make-syntax-getter val)
  (let ((x val))
    (syntax x)))

(define stx1 (make-syntax-getter 100))
(define-syntax test1 (lambda (_) stx1))
(test1)  ;; => 100
```

In Racket, this program would be rejected because `(syntax x)` at phase 0 cannot capture a binding for use at phase 1.

**First-class syntax objects.** Syntax objects in Grift can be stored in data structures, passed to functions, and destructured at runtime:

```scheme
(define (make-stx-list)
  (let ((a 1) (b 2) (c 3))
    (list (syntax a) (syntax b) (syntax c))))
```

This treats syntax objects as ordinary values, which simplifies the mental model for users but sacrifices the safety guarantees of phase separation.

**No separate compilation.** Because macro transformers may depend on runtime state, it is not possible to cache or serialize expanded code independently of the runtime environment. Each module must be re-evaluated from source on every load.

### 4.3 Trampolined Continuation-Based Expansion

Grift's evaluator uses a trampolined continuation-passing style to achieve proper tail-call optimization. Macro expansion participates in this mechanism: the `MacroResult` continuation captures the call-site environment and, upon receiving the expanded syntax from the transformer, re-enters the evaluator's main loop. This avoids unbounded stack growth during deeply nested macro expansions—a concern in traditional recursive expanders.

The continuation chain is itself stored in the arena as a linked list of `ContFrame` values, each containing a continuation type tag, data payload, environment reference, and parent link. There are 48 continuation types in total, of which `MacroResult`, `SyntaxCaseMatch`, `SyntaxCaseFender`, and `LetSyntaxBody` are directly related to macro expansion.

## 5. Comparison with Other Macro Systems

### 5.1 Racket

Racket's macro system (Flatt 2002; Flatt et al. 2012) is widely regarded as the most sophisticated in the Scheme family. It introduces several features beyond R6RS `syntax-case`:

| Feature | Grift | Racket |
|---------|-------|--------|
| `syntax-case` | ✅ | ✅ |
| `syntax-rules` | ✅ (macro over `syntax-case`) | ✅ (built-in) |
| Phase separation | ❌ | ✅ (strict, multi-phase) |
| `syntax-local-value` | ❌ | ✅ |
| `local-expand` | ❌ | ✅ |
| Syntax parameters | ❌ | ✅ |
| Syntax properties | ❌ | ✅ |
| Syntax certificates/tamper status | ❌ | ✅ |
| Module-level macros | Partial | ✅ |
| Separate compilation | ❌ | ✅ |
| `no_std` / embedded | ✅ | ❌ |
| First-class syntax at runtime | ✅ | Restricted |

Racket's phase separation enables powerful static guarantees. For example, if a macro accidentally refers to a phase-0 binding from a phase-1 transformer, Racket reports a compile-time error. Grift, by contrast, would silently resolve the reference at runtime, which may or may not be the intended behavior.

However, Racket's phase system introduces significant complexity. Programmers must annotate imports with phase levels (`(for-syntax ...)`, `(for-template ...)`), understand phase-level shifting, and reason about which bindings are visible at which phase. Grift's single-phase model eliminates this complexity entirely.

### 5.2 Guile

Guile (Wingo 2011) implements hygienic macros via `psyntax`, a portable `syntax-case` implementation originally developed by Dybvig, Hieb, and Bruggeman (1992). Guile's approach is closer to Grift's in some respects:

| Feature | Grift | Guile |
|---------|-------|-------|
| `syntax-case` | ✅ | ✅ (via `psyntax`) |
| Phase separation | ❌ | ✅ (module-level) |
| Compilation caching | ❌ | ✅ (.go files) |
| Heap allocation | ❌ (arena only) | ✅ (GC heap) |
| Ellipsis patterns | ✅ | ✅ |
| `datum->syntax` | ✅ | ✅ |
| `no_std` / embedded | ✅ | ❌ |

Guile's `psyntax` expander is a well-tested, mature implementation that has been refined over decades. Its module system integrates tightly with the macro expander, enabling macros to be exported and imported across module boundaries with full hygiene. Grift's module system is still under development and does not yet provide the same level of cross-module macro support.

### 5.3 R7RS `syntax-rules`

The R7RS standard (Shinn, Cowan, and Gleckler 2013) specifies only `syntax-rules` as a required macro facility, leaving `syntax-case` as an optional extension. Many small Scheme implementations (e.g., CHICKEN, Chibi-Scheme) provide `syntax-rules` without full `syntax-case` support.

Grift goes beyond R7RS requirements by providing `syntax-case` as the foundational macro mechanism, with `syntax-rules` defined as a macro on top. This gives Grift users access to procedural macros, fenders, and `datum->syntax` without requiring a separate expander or compiler phase.

### 5.4 Common Lisp's `defmacro`

Common Lisp uses `defmacro`, a non-hygienic macro system in which transformers operate on raw S-expressions. While `defmacro` is simpler to implement and understand, it provides no automatic protection against variable capture. Programmers must manually use `gensym` to avoid capture, which is error-prone.

Grift's mark-based hygiene automates this process: macro-introduced bindings are automatically renamed, and user bindings are protected from capture. When intentional capture is needed, `datum->syntax` provides controlled hygiene-breaking, offering the best of both worlds.

### 5.5 Summary of Trade-offs

| Dimension | Grift | Racket | Guile | Common Lisp |
|-----------|-------|--------|-------|-------------|
| Hygiene | Automatic (marks) | Automatic (scopes) | Automatic (marks) | Manual (`gensym`) |
| Expressiveness | High (single-phase) | Very high (multi-phase) | High | High (unrestricted) |
| Safety | Low (no phase checks) | High (phase errors caught) | Medium | Low (no hygiene) |
| Memory model | Fixed arena | GC heap | GC heap | GC heap |
| Compilation | Interpreted | Compiled + cached | Compiled + cached | Compiled |
| Embedded use | ✅ | ❌ | ❌ | Rare |
| Complexity | Low | High | Medium | Low |

## 6. Discussion

### 6.1 Suitability for Embedded Systems

Grift's macro system is, to our knowledge, the first hygienic macro implementation that operates without heap allocation. This makes it suitable for deployment on microcontrollers, in WebAssembly modules, and in other environments where dynamic memory allocation is either unavailable or undesirable. The fixed-size arena provides a hard upper bound on memory usage, which is essential for safety-critical and real-time systems.

The trade-off is that the arena capacity must be chosen at compile time. For macro-heavy programs that generate many intermediate syntax objects, a larger arena is needed. Grift's garbage collector can reclaim unreachable syntax objects during expansion, mitigating this limitation, but the worst-case memory usage is still bounded by the arena size.

### 6.2 Implications for Module Systems

The absence of phase separation creates challenges for module systems. In Racket, modules can export macros with well-defined phase-level semantics: a macro exported at phase 0 is available to importers at phase 1 for compile-time use. In Grift, because there is no phase distinction, exported macros may capture runtime bindings that are not visible to the importer.

Grift's documentation proposes a simple name-based import system in which macros are expanded in the importer's context. This avoids the problem of captured environments at the cost of reduced expressiveness: macros that rely on auxiliary runtime bindings must explicitly export those bindings.

### 6.3 Teaching and Experimentation

Grift's single-phase model may be advantageous for teaching. Students learning about macros often struggle with Racket's phase system, which introduces concepts (phase levels, `for-syntax` imports, phase shifting) that are orthogonal to the core idea of syntactic abstraction. Grift's model—where macros are simply functions that transform syntax—is conceptually simpler and may lower the barrier to understanding hygienic macros.

For language researchers, Grift provides a lightweight platform for experimenting with macro system designs. Its small codebase (the entire macro expander fits in a single file) and absence of external dependencies make it easy to modify and extend.

### 6.4 Limitations

Several limitations of Grift's macro system should be noted:

1. **No `syntax-error`**: The `syntax-error` form specified in R7RS is not yet implemented.
2. **No `letrec-syntax`**: Only `let-syntax` is supported for local macro definitions.
3. **Limited string operations in macros**: Because the core crates are `no_std`, compile-time operations like `string-append` and `string->symbol` are restricted.
4. **Pattern variable scoping**: Pattern variables in `syntax-case` bind to raw symbols rather than full syntax objects with lexical context, which can cause issues in some advanced macro patterns involving mutation of captured syntax.
5. **Fixed arena capacity**: Deeply nested or highly recursive macro expansions may exhaust the arena, requiring the user to increase the arena size.

## 7. Related Work

The design of Grift's macro system draws on a long line of research:

- **Kohlbecker et al. (1986)** introduced hygienic macro expansion via automatic renaming.
- **Clinger and Rees (1991)** developed mark-based hygiene, which Grift directly implements.
- **Dybvig, Hieb, and Bruggeman (1992)** created `syntax-case`, providing the procedural macro interface that Grift adopts.
- **Flatt (2002)** introduced composable and compilable macros with phase separation, forming the foundation of Racket's macro system.
- **Flatt (2016)** proposed the *binding as sets of scopes* model, an alternative to marks that avoids certain hygiene anomalies. Grift currently uses the older mark-based model.
- **Tofte and Talpin (1997)** developed region-based memory management, which inspired the arena allocation strategy used by Grift.

Several Scheme implementations target constrained environments but do not provide full `syntax-case`: Chibi-Scheme (Shinn 2009) targets small footprints with `syntax-rules` only; uLisp (Johnson 2016) targets microcontrollers but omits macros entirely; s7 (Schottstaedt 2021) provides `define-macro` without hygiene.

## 8. Conclusion

We have presented Grift's macro system, a hygienic `syntax-case` implementation that operates without heap allocation within a fixed-size arena. By unifying macro expansion with evaluation and eliminating phase separation, Grift achieves a simpler programming model at the cost of some static safety guarantees. The system supports the full range of `syntax-case` features—pattern matching with ellipsis, fenders, `datum->syntax`, and `bound-identifier=?`/`free-identifier=?`—while remaining deployable on embedded systems and other constrained environments.

The key finding of this work is that the historical coupling between hygienic macros and heap allocation is not fundamental. By representing syntax objects as tuples of arena indices and using a mark-based hygiene algorithm with stack-allocated gensyms, it is possible to implement a practical hygienic macro system within a fixed memory budget. This opens the door to bringing the benefits of syntactic abstraction to domains—embedded systems, WebAssembly, real-time computing—where they have previously been unavailable.

Future work includes implementing `syntax-error`, `letrec-syntax`, and the sets-of-scopes hygiene model (Flatt 2016); developing a full module system with cross-module macro support; and conducting empirical evaluations of macro expansion performance and memory usage on embedded hardware.

---

## References

1. Clinger, W. and Rees, J. (1991). "Macros that Work." *Proceedings of the 18th ACM SIGPLAN-SIGACT Symposium on Principles of Programming Languages (POPL '91)*, pp. 155–162. ACM.

2. Dybvig, R.K., Hieb, R., and Bruggeman, C. (1992). "Syntactic Abstraction in Scheme." *Lisp and Symbolic Computation*, 5(4), pp. 295–326.

3. Flatt, M. (2002). "Composable and Compilable Macros: You Want It When?" *Proceedings of the 7th ACM SIGPLAN International Conference on Functional Programming (ICFP '02)*, pp. 72–83. ACM.

4. Flatt, M. (2016). "Binding as Sets of Scopes." *Proceedings of the 43rd ACM SIGPLAN-SIGACT Symposium on Principles of Programming Languages (POPL '16)*, pp. 705–717. ACM.

5. Flatt, M., Culpepper, R., Darais, D., and Findler, R.B. (2012). "Macros that Work Together: Compile-Time Bindings, Partial Expansion, and Definition Contexts." *Journal of Functional Programming*, 22(2), pp. 181–216.

6. Johnson, D. (2016). "uLisp: Lisp for Microcontrollers." http://www.ulisp.com/.

7. Kohlbecker, E., Friedman, D.P., Felleisen, M., and Duba, B. (1986). "Hygienic Macro Expansion." *Proceedings of the 1986 ACM Conference on LISP and Functional Programming*, pp. 151–161. ACM.

8. Schottstaedt, B. (2021). "s7: A Scheme Implementation." https://ccrma.stanford.edu/software/snd/snd/s7.html.

9. Shinn, A. (2009). "Chibi-Scheme." https://github.com/ashinn/chibi-scheme.

10. Shinn, A., Cowan, J., and Gleckler, A. (Eds.) (2013). *Revised^7 Report on the Algorithmic Language Scheme (R7RS)*. http://r7rs.org/.

11. Sperber, M., Dybvig, R.K., Flatt, M., van Straaten, A., Findler, R.B., and Matthews, J. (Eds.) (2007). *Revised^6 Report on the Algorithmic Language Scheme (R6RS)*. http://www.r6rs.org/.

12. Tofte, M. and Talpin, J.-P. (1997). "Region-Based Memory Management." *Information and Computation*, 132(2), pp. 109–176.

13. Wingo, A. (2011). "A Brief History of Guile's Macro Expander." https://wingolog.org/.
