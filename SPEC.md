# Grift Language Specification

*Version 1.5 — Reference specification for the Grift dialect, building on the
Kernel Programming Language Revised-1 Report (R-1).*

## 1. Overview

Grift is a strict subset of the Kernel programming language (Shutt, R-1).
It implements first-class operatives (vau calculus / fexprs) in a `no_std`,
`no_alloc`, `no_unsafe` environment suitable for embedded and
resource-constrained systems.

All values live in a fixed-size arena with const-generic capacity.
There is no heap allocation; all dynamic data structures are arena-allocated.

### 1.1 Deviations from Kernel R-1

Grift intentionally omits several Kernel features to remain minimal and
embedded-friendly:

- **No continuations** — `call/cc`, `guard-continuation`, and related forms
  are not provided.
- **No ports / I/O** — Standard Kernel port operations are replaced by
  string-based I/O primitives (`raw-read-string`, `raw-display-to-string`).
- **Fixed-precision integers only** — Numbers are machine-width signed
  integers (`isize`), not arbitrary-precision.  Arithmetic overflow signals
  `ArithmeticOverflow`.
- **No mutation of pairs** — `set-car!` and `set-cdr!` are not provided;
  pairs are immutable once constructed.
- **No `$sequence`** — Use `begin` (equivalent semantics).

Everything that Grift *does* provide follows the Kernel R-1 semantics.

## 2. Types

Grift defines the following first-class types.  Every value is `Copy` and
fits in a single arena slot.

| Type          | Written as               | Self-evaluating? |
|---------------|--------------------------|------------------|
| Nil           | `()`                     | yes              |
| Boolean       | `#t`, `#f`               | yes              |
| Number        | `42`, `-7`, `+3`         | yes              |
| Symbol        | `foo`, `+`, `define!`    | no (lookup)      |
| Pair          | `(1 . 2)`, `(a b c)`    | no (combination) |
| String        | `"hello\n"`              | yes              |
| Operative     | `(vau ...)`              | yes              |
| Applicative   | `(wrap ...)`             | yes              |
| Builtin       | *(not writable)*         | yes              |
| Environment   | *(not writable)*         | yes              |
| Inert         | `#inert`                 | yes              |
| Ignore        | `#ignore`                | yes              |

### 2.1 Nil

The empty list.  Written `()`.  Used as the list terminator in proper lists.

### 2.2 Booleans

Two values: `#t` (true) and `#f` (false).  Aliases `#true` and `#false`
are accepted by the reader.  Boolean contexts (e.g. `if`, `and`, `or`,
`cond`) require a strict boolean — passing a non-boolean signals
`TypeError`.

### 2.3 Numbers

Machine-width signed integers (`isize`).  Arithmetic operations (`+`, `-`,
`*`, `/`) use checked arithmetic; overflow signals `ArithmeticOverflow` and
division by zero signals `DivisionByZero`.

### 2.4 Symbols

Interned identifiers.  Two symbols with the same name share the same arena
index, so symbol equality is pointer equality.  Symbols are case-sensitive.

### 2.5 Pairs

Immutable cons cells with `car` and `cdr`.  A proper list is a chain of
pairs terminated by nil: `(a b c)` ≡ `(a . (b . (c . ())))`.  Dotted
pairs `(a . b)` are supported.

### 2.6 Strings

Linked lists of `CharPair` nodes in the arena.  Each node stores one
Unicode character and a link to the next node (or nil).  Escape sequences
in string literals: `\n`, `\t`, `\r`, `\\`, `\"`.

### 2.7 Operatives

First-class fexprs created by `vau`.  An operative receives its operands
**unevaluated** together with the caller's dynamic environment.

### 2.8 Applicatives

Wrappers created by `wrap`.  An applicative evaluates all operands in the
caller's environment before passing them to the underlying combiner.

### 2.9 Environments

First-class mutable mappings from symbols to values, with a parent chain
for lexical scoping.  The ground environment (containing all builtins)
is immutable; the global environment is its child.

### 2.10 Inert and Ignore

`#inert` is returned by side-effecting forms whose return value is
meaningless (e.g. `define!`).  `#ignore` is used in formal parameter
trees to discard an operand.

## 3. Evaluation Rules

### 3.1 Self-Evaluation

All values except symbols and pairs evaluate to themselves.

### 3.2 Symbol Lookup

A symbol evaluates by searching the current environment's alist for a
matching binding.  If not found, the search continues in parent
environments (depth-first when multiple parents exist).  Failure signals
`UnboundVariable`.

### 3.3 Combination (Pair Evaluation)

When a pair `(operator . operands)` is evaluated:

1. Evaluate `operator` in the current environment.
2. Dispatch based on the combiner type:
   - **Operative / Builtin (operative)**: pass `operands` unevaluated
     and the caller's environment to the combiner.
   - **Applicative**: evaluate each operand left-to-right in the caller's
     environment, then pass the resulting list to the underlying combiner.
3. If the operator is not callable, signal `NotCallable`.

### 3.4 Tail-Call Optimization

The evaluator uses a trampoline loop.  Tail-position expressions in
`if`, `begin`, `cond`, `let`, `and`, `or`, and user-defined combiners
re-enter the loop without growing the Rust call stack.

### 3.5 Garbage Collection

GC is demand-driven: triggered when an allocation returns `OutOfMemory`.
A mark-and-sweep collector traces from the root set (singletons,
GC root stack, intern list).  If collection frees no memory, `OutOfMemory`
propagates.

## 4. Special Forms (Operatives)

All operatives receive unevaluated operands and the caller's environment.

### 4.1 `(quote expr)`

Returns `expr` without evaluating it.  The reader shorthand `'expr`
expands to `(quote expr)`.

### 4.2 `(if test consequent alternative)`

Evaluates `test`.  If the result is `#t`, evaluates `consequent` (in tail
position).  If `#f`, evaluates `alternative` (in tail position).
Non-boolean test values signal `TypeError`.

### 4.3 `(define! definiend expr)`

Binds `definiend` to the result of evaluating `expr` in the current
environment.  `definiend` may be a symbol, `#ignore`, or a parameter
tree (nested pair structure for destructuring).  Returns `#inert`.

The shorthand `(define! (fn name params...) body...)` defines `name`
as `(lambda (params...) body...)`.

### 4.4 `(set! definiend expr)`

Like `define!` but mutates an existing binding rather than creating a
new one.  Searches the environment chain for each symbol in `definiend`.

### 4.5 `(vau params env-param body)`

Creates an operative (fexpr).  When called:
- `params` is a formal parameter tree matched against the **unevaluated**
  operands.
- `env-param` is bound to the **caller's environment** (or `#ignore`
  to discard it).
- `body` is evaluated in the closed-over environment extended with the
  parameter bindings.

### 4.6 `(lambda params body)`

Sugar for `(wrap (vau params #ignore body))`.  Creates an applicative
that evaluates its arguments before binding them.

### 4.7 `(begin expr ...)`

Evaluates expressions sequentially.  Returns the value of the last
expression (in tail position).

### 4.8 `(cond (test body ...) ...)`

Evaluates `test` clauses sequentially.  For the first `#t` test,
evaluates the corresponding `body` forms (last in tail position) and
returns the result.  Returns `#inert` if no test is true.

### 4.9 `(and expr ...)`

Short-circuit conjunction.  Evaluates expressions left-to-right; returns
`#f` on the first false result, otherwise returns the last value (in tail
position).  `(and)` returns `#t`.

### 4.10 `(or expr ...)`

Short-circuit disjunction.  Returns the first true result; otherwise
returns `#f`.  `(or)` returns `#f`.

### 4.11 `(let ((name expr) ...) body ...)`

Creates a child environment, binds each `name` to the result of evaluating
`expr`, and evaluates `body` forms in that environment (last in tail
position).

### 4.12 `(current-environment)`

Returns the current first-class environment object.

## 5. Applicative Builtins

### 5.1 Arithmetic

| Form           | Description                                      |
|----------------|--------------------------------------------------|
| `(+ a ...)`    | Sum.  `(+)` → `0`.                               |
| `(- a ...)`    | Difference.  `(- x)` → negation.                 |
| `(* a ...)`    | Product.  `(*)` → `1`.                            |
| `(/ a b)`      | Truncating division.  Signals `DivisionByZero`.   |

### 5.2 Comparison

| Form           | Description                                      |
|----------------|--------------------------------------------------|
| `(= a b ...)`  | Numeric equality (chained).                      |
| `(< a b ...)`  | Strictly less than (chained).                    |
| `(> a b ...)`  | Strictly greater than (chained).                 |
| `(<= a b ...)` | Less than or equal (chained).                    |
| `(>= a b ...)` | Greater than or equal (chained).                 |

### 5.3 Pair Operations

| Form           | Description                                      |
|----------------|--------------------------------------------------|
| `(cons a b)`   | Construct a pair.                                |
| `(car p)`      | First element of a pair.                         |
| `(cdr p)`      | Second element of a pair.                        |
| `(list a ...)` | Construct a proper list.                         |

### 5.4 Type Predicates

`null?`, `pair?`, `number?`, `symbol?`, `boolean?`, `inert?`,
`ignore?`, `operative?`, `applicative?`, `environment?`

Each returns `#t` if the argument is of the given type, `#f` otherwise.

### 5.5 Equality

| Form             | Description                                    |
|------------------|------------------------------------------------|
| `(eq? a b)`      | Identity/pointer equality.                     |
| `(equal? a b)`   | Structural equality (recursive on pairs).      |

### 5.6 Boolean

| Form        | Description                                         |
|-------------|-----------------------------------------------------|
| `(not x)`   | Boolean negation.  Requires a boolean argument.     |

### 5.7 Combiner Operations

| Form            | Description                                       |
|-----------------|---------------------------------------------------|
| `(eval e env)`  | Evaluate expression `e` in environment `env`.     |
| `(wrap c)`      | Wrap a combiner as an applicative.                |
| `(unwrap a)`    | Extract the underlying combiner of an applicative.|
| `(apply f args)`| Apply applicative `f` to argument list `args`.    |

### 5.8 Environment Operations

| Form                     | Description                              |
|--------------------------|------------------------------------------|
| `(make-environment p)`   | New environment with parent `p`.         |
| `(make-empty-environment)` | New environment with no parent.        |

### 5.9 String / I/O

| Form                         | Description                           |
|------------------------------|---------------------------------------|
| `(raw-read-string s)`        | Parse a string as an S-expression.    |
| `(raw-display-to-string v)`  | Convert a value to its display form.  |
| `(raw-write-to-string v)`    | Convert a value to its written form.  |

### 5.10 System

| Form              | Description                                      |
|-------------------|--------------------------------------------------|
| `(gc-collect)`    | Trigger a garbage collection cycle.              |
| `(error msg)`     | Signal an error with message string.             |

## 6. Standard Library

The following functions are defined in `prelude.grift` and loaded as
lazy `StdLib` entries (parsed on first invocation):

| Function               | Description                                  |
|------------------------|----------------------------------------------|
| `(map f lst)`          | Apply `f` to each element, return new list.  |
| `(filter pred lst)`    | Keep elements where `pred` returns `#t`.     |
| `(length lst)`         | Number of elements in a proper list.         |
| `(append a b)`         | Concatenate two lists.                       |

Additional library functions may be defined in `prelude.grift`.

## 7. Reader Syntax

### 7.1 Atoms

- **Booleans**: `#t`, `#f`, `#true`, `#false`
- **Special values**: `#inert`, `#ignore`
- **Numbers**: Optional sign followed by digits: `42`, `-7`, `+3`
- **Symbols**: Any sequence of non-delimiter characters that is not a
  number or special value.
- **Delimiters**: space, tab, newline, CR, `(`, `)`, `"`, `;`

### 7.2 Lists

- Proper list: `(a b c)` ≡ `(a . (b . (c . ())))`
- Dotted pair: `(a . b)`
- Empty list: `()`

### 7.3 Strings

Delimited by `"`.  Supported escape sequences:

| Escape | Character       |
|--------|-----------------|
| `\n`   | newline         |
| `\t`   | tab             |
| `\r`   | carriage return |
| `\\`   | backslash       |
| `\"`   | double quote    |

### 7.4 Quote Shorthand

`'expr` is expanded by the reader to `(quote expr)`.

### 7.5 Comments

Line comments begin with `;` and extend to end of line.

## 8. Error Conditions

All errors are represented by `ArenaError` variants (no heap allocation):

| Error                  | Description                                    |
|------------------------|------------------------------------------------|
| `OutOfMemory`          | Arena full, even after GC.                     |
| `IndexOutOfBounds`     | Index exceeds arena capacity.                  |
| `IndexNotAllocated`    | Accessing a freed slot.                        |
| `InvalidArgument`      | Invalid argument to an operation.              |
| `TraceError`           | GC tracing failure.                            |
| `Cyclic`               | Cycle detected during traversal.               |
| `TypeError`            | Wrong type for operation.                      |
| `ParseError`           | Malformed S-expression (includes line/column). |
| `ArithmeticOverflow`   | Checked arithmetic overflow.                   |
| `DivisionByZero`       | Division or modulo by zero.                    |
| `UnboundVariable`      | Symbol not found in environment chain.         |
| `NotCallable`          | Attempted to call a non-combiner.              |
| `ImmutableEnvironment` | Mutating the ground environment.               |

## 9. Implementation Constraints

- **Arena capacity**: Fixed at compile time via const generic `N`.
- **No unsafe code**: `#![forbid(unsafe_code)]` is enforced crate-wide.
- **No heap allocation**: `no_std`, `no_alloc`.  Only `core::` types used.
- **MSRV**: Rust 1.85 (edition 2024).
- **Tail-call optimization**: Via trampoline; unbounded tail recursion
  without growing the Rust stack.
- **GC**: Mark-and-sweep, triggered on OOM.  Uses a mark bitmap and
  mark stack on the Rust stack.
