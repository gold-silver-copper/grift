# Grift Language Specification

This document specifies the currently implemented Grift language as it exists
in `crates/grift/src`, `crates/grift/prelude.grift`, and
`crates/grift/tests/lisp_tests.rs`.

It is an implementation-compatibility specification, not an idealized design
document. The goal is to make a source-compatible reimplementation possible in
another language such as C, including Grift's current quirks.

## 1. Conformance Target

A conforming implementation must match the observable behavior of:

- the reader
- the evaluator
- environment lookup and mutation
- formal-parameter-tree matching
- builtin operatives and applicatives
- equality predicates
- string/list behavior
- prelude bindings
- `raw-read-string`
- `raw-display-to-string`
- `raw-write-to-string`

It does not need to match Rust-only host APIs such as the exact shape of
`LispOps`, except where host-native values are visible as first-class runtime
values.

## 2. Program Model

A program is zero or more expressions read from a character stream.

- Top-level input is parsed and evaluated sequentially.
- The result of the last expression is the result of the whole program.
- An empty top-level program evaluates to `#inert`.
- Evaluation happens in the global environment unless an explicit environment
  is provided to `eval` or `apply`.
- The global environment persists across evaluations performed on the same
  interpreter instance.

## 3. Reader

### 3.1 Source Model

The reader is byte-oriented. Portable source should therefore be treated as
ASCII source text plus the escape sequences listed below.

Source coordinates are reported as 1-based `(line, col)` pairs for ordinary
top-level parsing. Parsing through `raw-read-string` uses a `CharPair` chain and
reports `(0, 0)` for reader-generated parse errors because source coordinates
are not tracked there.

### 3.2 Whitespace and Comments

The reader treats these characters as whitespace:

- space
- tab
- carriage return
- newline

Line comments begin with `;` and continue to the next newline or end of input.

### 3.3 Surface Forms

The reader recognizes:

- proper lists: `(a b c)`
- dotted pairs: `(a . b)`
- quote shorthand: `'x`
- string literals: `"hello"`
- atoms: booleans, `#inert`, `#ignore`, integers, and symbols

The language does not currently have reader support for:

- quasiquote / unquote
- vectors
- byte strings
- characters as a separate literal type
- block comments

### 3.4 Informal Grammar

The implemented grammar is best described procedurally, but the accepted forms
can be summarized as:

```text
program    := { ws_or_comment expr }*
expr       := list | quote | string | atom
quote      := "'" expr
list       := "(" list_contents
list_contents
           := ")"
           | expr list_tail
list_tail  := ")"
           | ws_or_comment "." ws_or_comment expr ws_or_comment ")"
           | ws_or_comment expr list_tail
string     := '"' { string_char } '"'
atom       := { non_delimiter }+
```

That grammar is not complete by itself because `.` is only special in one
specific list-parsing position; see the dot rules below.

### 3.5 Reader Algorithm

The reader is recursive-descent and behaves as follows:

1. Skip whitespace and `;` comments.
2. Read one character.
3. Dispatch:
   - `(` starts list parsing
   - `'` parses the next expression and rewrites it as `(quote expr)`
   - `"` starts string parsing
   - `)` is a parse error
   - anything else starts atom parsing

At top level, `parse_expr` returns `NIL` when no input remains.

### 3.6 Lists and Dotted Pairs

After reading `(`:

1. Skip whitespace/comments.
2. If the next character is `)`, the result is `NIL`.
3. Otherwise parse the first element.
4. Skip whitespace/comments.
5. If the next character is `.` and that `.` is followed immediately by a
   delimiter or end of input, parse one more expression as the cdr, require a
   closing `)`, and return a dotted pair.
6. Otherwise continue parsing the remaining list elements recursively.

Important dot rules:

- A lone `.` in list-structure position is special.
- `.` is not a general token delimiter.
- `(a . b)` is a dotted pair.
- `(a .foo)` is parsed as the proper list `(a .foo)`.
- `( . x)` is a parse error.
- A list whose first token begins with `.` also follows this rule:
  `( .foo bar)` parses as `(.foo bar)`.

### 3.7 Quote Shorthand

`'expr` rewrites to `(quote expr)`.

Current implementation quirk:

- a bare trailing `'` does not raise `ParseError`
- instead it rewrites to `(quote ())`, because end-of-input is treated as
  `NIL` by `parse_expr`

Examples:

- `'x` => `(quote x)`
- `'(1 2)` => `(quote (1 2))`
- `'` => `(quote ())`

### 3.8 Atom Delimiters

Atom parsing stops at:

- space
- tab
- carriage return
- newline
- `(`
- `)`
- `"`
- `;`

Notably, `.` is not a delimiter outside the list parser's dotted-pair logic.

### 3.9 Atom Classification

An atom token is classified in this order:

1. `#t` or `#true` => boolean true
2. `#f` or `#false` => boolean false
3. `#inert` => inert singleton
4. `#ignore` => ignore singleton
5. a signed base-10 integer that fits the host `isize`
6. otherwise, a symbol

Consequences:

- `+42` and `-42` are numeric literals
- `+` and `-` by themselves are symbols
- `#unknown` is a symbol
- an overflowing integer literal is not a reader error; it becomes a symbol

### 3.10 Strings

String literals are delimited by `"`.

Recognized escape sequences are exactly:

- `\n`
- `\t`
- `\r`
- `\\`
- `\"`

Current behavior:

- any other backslash escape raises `InvalidArgument`, not `ParseError`
- unterminated string literals raise `ParseError`
- the reader stores runtime strings as linked `CharPair` nodes

### 3.11 Reader Errors

Malformed syntax can raise:

- `ParseError { line, col }` for structural reader failures
- `InvalidArgument` for an unknown string escape

Examples of `ParseError` cases:

- unexpected `)`
- unterminated list
- unterminated string
- malformed dotted-pair syntax

## 4. Runtime Value Model

Grift has these runtime value categories:

| Category | External form |
| --- | --- |
| nil | `()` |
| booleans | `#t`, `#f` |
| numbers | decimal integer |
| symbols | symbol name |
| pairs | list syntax or dotted-pair syntax |
| strings | quoted in write mode, raw in display mode |
| compound operatives | `<operative>` |
| applicatives | `<applicative>` |
| builtin operative cores | `<builtin>` |
| environments | `<environment>` |
| inert | `#inert` |
| ignore | `#ignore` |
| prelude entries | `<prelude:name>` |
| native host functions | `<native>` |

### 4.1 Nil and the Empty String

`NIL` is the empty list and also the empty string.

This aliasing is mandatory for compatibility:

- `()` and `""` are distinct reader syntax
- both evaluate to the same runtime object
- `null? ""` is true
- `pair? ""` is false
- `raw-write-to-string ""` produces `"()"`, not `"\"\""`

### 4.2 Numbers

Numbers are signed machine integers with host width equivalent to Rust `isize`.

- reader classification checks whether the literal fits `isize`
- arithmetic builtins use checked arithmetic
- overflow raises `ArithmeticOverflow`
- division truncates toward zero

### 4.3 Symbols

Symbols are interned by name.

- two symbols with the same spelling are the same symbol for `eq?`,
  `equal?`, and environment lookup
- symbol interning is observable because symbol equality becomes pointer-stable

### 4.4 Pairs

Pairs are immutable cons cells.

- proper lists are chains of pairs ending in `NIL`
- improper lists are pairs whose final cdr is not `NIL`
- there is no `set-car!`
- there is no `set-cdr!`

### 4.5 Strings

Strings are singly linked chains of `CharPair` nodes.

- a non-empty string is a chain of `CharPair { ch, cdr }`
- the empty string is exactly `NIL`
- there is no separate character type
- a single character is represented as a one-character string

Observable consequences:

- `(car "hello")` returns `"h"` as a fresh one-character string
- `(cdr "hello")` returns `"ello"`
- `(cdr "x")` returns `()`
- `pair?` is true for non-empty strings
- `pair?` is false for the empty string
- `cons` can construct a string node only when its first argument is a
  one-character string

Strings behave like list-shaped values in several places, but they are still a
distinct runtime representation from `Cons`.

### 4.6 Callables

Grift has five callable storage forms:

- builtin operative cores (`Value::Builtin`)
- compound operatives created by `vau`
- applicatives (`Value::Applicative`)
- prelude entries (`Value::Prelude`, normally wrapped in an applicative)
- native host functions (`Value::Native`, normally wrapped in an applicative)

Operationally:

- builtin operative cores receive raw operands
- compound operatives receive raw operands
- applicatives evaluate operands first
- prelude entries are bound as applicatives in the global environment
- native functions are normally bound as applicatives in the global environment

## 5. Environments

An environment is a frame plus zero or more parent environments.

### 5.1 Structure

Each environment stores:

- `bindings`: an alist of `(symbol . value)` pairs
- `parents`: a proper list of parent environments

The implementation also reserves:

- `GROUND_ENV`: builtin-only environment
- `GLOBAL_ENV`: child of `GROUND_ENV`; this is the normal user top level

Builtins live in the ground environment. Prelude bindings and user top-level
definitions live in the global environment.

### 5.2 Definition

`define!`:

- mutates only the current frame
- rejects rebinding a name already present in that same frame
- raises `AlreadyDefined` for same-frame redefinition

### 5.3 Mutation

`set!`:

- mutates only the explicitly supplied environment
- searches only that environment's own frame
- does not walk parents
- raises `UnboundVariable` if the symbol is absent from that frame

### 5.4 Lookup Order

Environment lookup is observable and must match the implementation:

1. Search the current frame's bindings from newest to oldest.
2. If there is exactly one parent, continue upward iteratively.
3. If there are multiple parents, search them in left-to-right depth-first
   order.
4. During multi-parent search, already-visited environments are skipped to
   avoid cycles.
5. If nothing matches, raise `UnboundVariable`.

Consequences:

- children inherit through parent links; they do not copy parent bindings
- a later parent can be shadowed by an earlier parent's ancestor because the
  search is depth-first, not breadth-first
- evaluating in a custom environment never falls back to the global environment
  unless that environment is actually in the parent chain

### 5.5 Environment Construction

`make-environment` creates a fresh frame with the supplied parent list.

`make-empty-environment` always creates a parentless environment.

Child environments start with no local bindings of their own.

## 6. Evaluation Semantics

### 6.1 Self-Evaluating Values

These values evaluate to themselves:

- `NIL`
- booleans
- numbers
- strings
- compound operatives
- applicatives
- builtin operative cores
- environments
- `#inert`
- `#ignore`
- prelude values
- native values

Symbols are not self-evaluating. Pairs are not self-evaluating.

### 6.2 Truth

Only booleans are accepted in boolean contexts.

This differs from Scheme. There is no general truthiness rule.

The following require actual booleans and raise `TypeError` otherwise:

- `if`
- `cond` test expressions except `else`
- `and`
- `or`
- `not`

### 6.3 Symbol Evaluation

Evaluating a symbol performs environment lookup in the current environment.

### 6.4 Combination Evaluation

Evaluating a `Cons` cell treats it as a combination:

1. Evaluate the operator position.
2. Dispatch on the resulting value.

Dispatch rules:

- builtin operative core => call with raw operand list and caller environment
- compound operative => call with raw operand list and caller environment
- applicative => evaluate operands left-to-right exactly once, then call the
  wrapped value
- anything else => `NotCallable`

### 6.5 Applicative Evaluation Order

Applicatives evaluate operands:

- strictly
- left to right
- exactly once

The resulting list of evaluated arguments is then passed to the wrapped value.

### 6.6 Operative Invocation

Invoking a compound operative created by `vau`:

1. Create a child environment whose single parent is the operative's definition
   environment.
2. Match the formal parameter tree against the operand object.
3. If an environment parameter exists, bind it to the caller's environment.
4. Evaluate the body in that new environment.

This yields lexical scope plus explicit dynamic access to the caller
environment.

### 6.7 Tail Positions

The implementation performs proper tail-call behavior for:

- user operatives
- `if`
- `begin`
- `cond`
- `and`
- `or`
- `let`

A compatible reimplementation should preserve unbounded tail recursion.

### 6.8 Arity Policy

Grift does not impose a single exact-arity rule across all builtins.

The current language uses four patterns:

- exact arity for a few forms via explicit checks
- "use only the first N operands" with extras ignored
- "consume all remaining operands"
- missing operands surfacing as `TypeError` from list access

This specification treats those exact per-form behaviors as normative.

## 7. Formal Parameter Trees

Formal parameter trees are used in matching and destructuring.

### 7.1 Valid Shapes

A valid formal parameter tree is:

- a symbol
- `#ignore`
- `NIL`
- a pair whose car and cdr are both valid formal parameter trees

Examples:

- `x`
- `#ignore`
- `()`
- `(x y)`
- `(head . tail)`
- `((a b) c)`

### 7.2 Matching Semantics

When a formal parameter tree is matched against an object:

- symbol => bind symbol to the matched object
- `#ignore` => ignore the matched object
- `NIL` => the matched object must be `NIL`
- pair => the matched object must be a `Cons`, then match car and cdr

Important limitation:

- matching requires the object to be a `Cons` when the tree is a pair
- a `CharPair` string node does not count as a pair for parameter matching

### 7.3 Build-Dependent Validation

Formal-parameter-tree validation is currently gated on Rust `debug_assertions`.

In debug builds, `define!` and `vau` perform an eager validation pass:

- duplicate symbols are rejected
- cyclic trees are rejected
- malformed non-symbol leaves are rejected
- `vau` also checks that `env-param` is a symbol or `#ignore`
- `vau` also checks that `env-param` does not appear in `params`

In release builds, that eager validation is skipped:

- `define!` evaluates its right-hand side and then immediately attempts matching
- `vau` stores `params` directly and only normalizes `#ignore` to "no env binding"
- malformed trees therefore fail only when matching or later binding activity
  reaches the bad shape

Consequences:

- debug and release builds can differ on whether malformed `define!` or `vau`
  forms fail at construction time or only later
- `lambda` and `fn!` do not perform eager validation in either build mode

## 8. Operatives

Unless otherwise stated, an operative receives its operands unevaluated.

### 8.1 `quote`

Syntax:

```lisp
(quote expr)
```

Behavior:

- returns the first operand unchanged
- ignores extra operands
- missing operand raises `TypeError`

### 8.2 `if`

Syntax:

```lisp
(if test then [else])
```

Behavior:

1. Evaluate `test`.
2. `test` must evaluate to a boolean.
3. If true, evaluate and return `then`.
4. Otherwise:
   - if an else operand is present, evaluate and return it
   - if no else operand is present, return `NIL`
5. Extra operands after the else operand are ignored.

Only the selected branch is evaluated.

### 8.3 `define!`

Syntax:

```lisp
(define! definiend expression)
```

Behavior:

1. Evaluate `expression` in the current environment.
2. Match `definiend` against the resulting value in the current frame.
3. Return `#inert`.

Important:

- this is destructuring definition, not Scheme function-definition shorthand
- `(define! (f x) body)` destructures instead of defining a function
- same-frame rebinding raises `AlreadyDefined`
- in debug builds, malformed definiends can fail before RHS evaluation due to
  eager validation
- in release builds, malformed definiends fail during matching instead
- extra operands after the expression are ignored

### 8.4 `fn!`

Syntax:

```lisp
(fn! name params body...)
```

Behavior:

- creates a function equivalent to `(lambda params (begin body...))`
- binds it in the current frame under `name`
- returns `#inert`

Current quirks:

- `name` is not explicitly type-checked to be a symbol
- `params` are not eagerly validated as a formal parameter tree
- zero body expressions are allowed; such a function returns `NIL`

### 8.5 `set!`

Syntax:

```lisp
(set! env-expr symbol value-expr)
```

Behavior:

1. Evaluate `env-expr` in the current environment.
2. The result must be an environment.
3. Evaluate `value-expr` in the current environment.
4. `symbol` must be a symbol literal, not a parameter tree.
5. Mutate that symbol's existing binding in the target environment's own frame.
6. Return `#inert`.

Differences from Scheme:

- there is no `(set! x value)` shorthand
- the target environment is explicit
- parent frames are not searched for mutation
- extra operands are ignored after the third used operand

### 8.6 `lambda`

Syntax:

```lisp
(lambda params body...)
```

Behavior:

- creates an applicative closure
- captures the definition environment lexically
- evaluates arguments before matching them against `params`
- zero body expressions are allowed; the function returns `NIL`

Current quirk:

- `params` are not eagerly validated at definition time

### 8.7 `begin`

Syntax:

```lisp
(begin expr...)
```

Behavior:

- evaluate operands left to right
- the result is the result of the last operand
- `(begin)` returns `NIL`

### 8.8 `cond`

Syntax:

```lisp
(cond (test expr...) ...)
```

Behavior:

- clauses are considered left to right
- in each clause, `test` is either:
  - the symbol `else`, which matches unconditionally, or
  - an expression that must evaluate to a boolean
- when a clause matches, its body is evaluated as if wrapped in `begin`
- a matching clause with no body returns `NIL`
- if no clause matches, the result is `NIL`

Current quirk:

- `else` is recognized purely by symbol name
- it does not need to be the final clause
- therefore `(cond (else 1) (#t 2))` evaluates to `1`

### 8.9 `and`

Syntax:

```lisp
(and expr1 expr2 ...)
```

Behavior:

- requires at least two operands; fewer raise `InvalidArgument`
- evaluates operands left to right
- every evaluated operand must be a boolean
- stops at the first `#f`
- returns `#f` if any operand is `#f`
- returns `#t` if all operands are `#t`

Unlike Scheme, it never returns an arbitrary last truthy value.

### 8.10 `or`

Syntax:

```lisp
(or expr1 expr2 ...)
```

Behavior:

- requires at least two operands; fewer raise `InvalidArgument`
- evaluates operands left to right
- every evaluated operand must be a boolean
- stops at the first `#t`
- returns `#t` if any operand is `#t`
- returns `#f` if all operands are `#f`

### 8.11 `let`

Regular form:

```lisp
(let ((name init) ...) body...)
```

Behavior:

1. Create a child environment of the current environment.
2. Evaluate every `init` in the outer environment, not the child.
3. Bind each `name` in the child frame.
4. Evaluate `body...` in the child environment.

This is simultaneous binding, not `let*`.

Named form:

```lisp
(let name ((param init) ...) body...)
```

Behavior:

1. Evaluate all init expressions in the outer environment.
2. Create a child environment.
3. Define `name` in that child as a recursive function.
4. Invoke that function with the already evaluated init values.

Current details:

- named-let init values are not re-evaluated when the helper function is
  invoked
- zero body expressions are allowed; the result is `NIL`

### 8.12 `vau`

Syntax:

```lisp
(vau params env-param body...)
```

Behavior:

- creates a compound operative
- captures the definition environment lexically
- zero body expressions are allowed; the operative body is `NIL`

`env-param` handling:

- if `env-param` is `#ignore`, no caller-environment binding is created
- in debug builds, non-symbol/non-`#ignore` `env-param` values are rejected
- in debug builds, `env-param` is rejected if it also appears in `params`
- in release builds, any non-`#ignore` value is stored as-is and later used as
  the binding key

### 8.13 `current-environment`

Syntax:

```lisp
(current-environment)
```

Behavior:

- returns the caller's current environment
- ignores every supplied operand

## 9. Applicatives

Applicatives evaluate their arguments before running.

### 9.1 Arithmetic

#### `+`

```lisp
(+ n ...)
```

- variadic addition
- zero arguments => `0`
- all arguments must be numbers
- overflow => `ArithmeticOverflow`

#### `-`

```lisp
(- n ...)
```

- zero arguments => `InvalidArgument`
- one argument => arithmetic negation
- multiple arguments => left fold subtraction
- overflow => `ArithmeticOverflow`

#### `*`

```lisp
(* n ...)
```

- variadic multiplication
- zero arguments => `1`
- all arguments must be numbers
- overflow => `ArithmeticOverflow`

#### `/`

```lisp
(/ a b)
```

- integer division truncating toward zero
- uses only the first two arguments
- division by zero => `DivisionByZero`
- missing operands typically raise `TypeError`
- extra operands are ignored

### 9.2 Numeric Comparison

These forms use only the first two evaluated arguments:

- `(= a b)`
- `(< a b)`
- `(> a b)`
- `(<= a b)`
- `(>= a b)`

Behavior:

- both used operands must be numbers
- result is a boolean
- missing operands typically raise `TypeError`
- extra operands are ignored

### 9.3 Pair and String Operations

#### `cons`

```lisp
(cons a b)
```

Normal behavior:

- constructs a pair `(a . b)`

String-specialized behavior:

- if `a` is a one-character string, the result is a `CharPair` node whose
  character is that character and whose cdr is `b`
- this is how `(cons (car "h") "ello")` constructs `"hello"`

Important limitation:

- only a one-character first argument triggers string construction
- `(cons "ab" "cd")` constructs a pair, not a string
- the resulting cdr is not validated to be a proper string tail

#### `car`

```lisp
(car pair-or-string)
```

- on a pair, returns the pair's car
- on a non-empty string, returns a fresh one-character string
- on anything else, raises `TypeError`
- extra operands are ignored

#### `cdr`

```lisp
(cdr pair-or-string)
```

- on a pair, returns the pair's cdr
- on a non-empty string, returns the tail string
- on anything else, raises `TypeError`
- extra operands are ignored

#### `list`

```lisp
(list obj ...)
```

- returns the already evaluated argument list as a proper list
- `(list)` returns `NIL`

### 9.4 Predicates

These predicates are variadic universal predicates:

- `(null? obj ...)`
- `(pair? obj ...)`
- `(number? obj ...)`
- `(symbol? obj ...)`
- `(boolean? obj ...)`
- `(inert? obj ...)`
- `(ignore? obj ...)`
- `(operative? obj ...)`
- `(applicative? obj ...)`
- `(environment? obj ...)`

Behavior:

- return `#t` iff every supplied operand matches the predicate
- return `#f` as soon as one operand does not match
- with zero operands, return `#t`

Specific meanings:

- `pair?` is true for `Cons` cells and non-empty strings
- `operative?` is true for compound operatives and bare builtin operative cores
- `applicative?` is true only for applicative wrapper values

#### `not`

```lisp
(not boolean)
```

- negates the first used operand
- that operand must be a boolean
- missing operand raises `TypeError`
- extra operands are ignored

### 9.5 Equality

#### `eq?`

```lisp
(eq? a b)
```

`eq?` first checks whether `a` and `b` are the same arena object. If not, it
falls back to value-based comparison only for immutable value kinds.

Effective behavior:

- nil, booleans, numbers, symbols, `#inert`, `#ignore`, prelude entries, and
  native functions compare by abstract value
- pairs compare by object identity only
- environments compare by object identity only
- operatives compare by object identity only
- applicatives compare by object identity only
- strings compare by node representation, not by full text content

String consequence:

- two separately allocated equal multi-character strings are usually not `eq?`
- two separately allocated one-character strings with the same character are
  `eq?`
- shared string tails can be `eq?`

#### `equal?`

```lisp
(equal? a b)
```

`equal?` is structural equality:

- if `eq?` is true, `equal?` is true
- pairs compare recursively by car and cdr
- strings compare character-by-character
- environments are never `equal?` unless they are already `eq?`
- all other values use the `eq?` result

### 9.6 Combiner Operations

#### `eval`

```lisp
(eval expr [env])
```

- if `env` is omitted, the global environment is used
- otherwise `expr` is evaluated in the supplied environment
- only the first two operands are used
- the environment operand is not eagerly type-checked

That last point is observable:

- `(eval 42 not-an-environment)` can succeed because `42` is self-evaluating
- environment type errors appear only if evaluation actually needs environment
  operations

#### `wrap`

```lisp
(wrap combiner)
```

- returns an applicative that evaluates operands before delegating to
  `combiner`
- only the first operand is used
- `combiner` is not validated eagerly to be callable

#### `unwrap`

```lisp
(unwrap applicative)
```

- extracts the wrapped inner value
- only the first operand is used
- raises `TypeError` if the operand is not an applicative

#### `apply`

```lisp
(apply combiner arg-list [env])
```

- if `env` is omitted, the global environment is used
- `arg-list` is passed directly as the combiner's operand object
- if `combiner` is applicative, `arg-list` therefore contains already evaluated
  arguments
- if `combiner` is operative, `arg-list` is treated as raw operands
- only the first three operands are used

`apply` mirrors ordinary combination dispatch and works with:

- builtin operatives such as `if`, `quote`, and `current-environment`
- user operatives created by `vau`
- applicatives including wrapped builtin operatives
- prelude entries
- native functions

As with `eval`, the optional environment is only type-checked if the call path
actually uses it as an environment.

### 9.7 Environment Constructors

#### `make-environment`

```lisp
(make-environment env ...)
```

- every operand must be an environment
- returns a fresh environment whose parent list is the supplied operands in
  order
- the parent list is copied into fresh list structure
- zero operands produce a parentless environment

#### `make-empty-environment`

```lisp
(make-empty-environment)
```

- always returns a parentless environment
- current implementation ignores extra operands instead of rejecting them

### 9.8 Raw Reader and Printer Builtins

#### `raw-read-string`

```lisp
(raw-read-string string)
```

- parses one expression from the supplied string value
- if the input string is empty, returns `NIL`
- uses the same reader rules as top-level parsing
- only the first operand is used

Important quirk:

- trailing unread characters after the first parsed expression are ignored
- for example, `(raw-read-string "1 2")` returns `1`

#### `raw-display-to-string`

```lisp
(raw-display-to-string obj)
```

- renders `obj` using display-mode formatting
- strings are emitted without quotes or escapes
- only the first operand is used

#### `raw-write-to-string`

```lisp
(raw-write-to-string obj)
```

- renders `obj` using write-mode formatting
- strings are emitted with quotes and the five supported escapes
- only the first operand is used

### 9.9 Other Builtins

#### `gc-collect`

```lisp
(gc-collect)
```

- forces a garbage-collection cycle
- returns the number of collected objects as a nonnegative integer
- ignores all operands

#### `error`

```lisp
(error msg)
```

- ignores all operands
- always raises `InvalidArgument`

## 10. Printed Representation

### 10.1 Write Mode

Write mode is used by `raw-write-to-string`.

Formatting rules:

- `NIL` => `()`
- booleans => `#t`, `#f`
- numbers => decimal integer
- symbols => symbol name
- non-empty strings => quoted, with escapes for `"`, `\`, newline, tab, and
  carriage return
- proper lists => `(a b c)`
- improper lists => `(a b . c)`
- `#inert` => `#inert`
- `#ignore` => `#ignore`
- compound operatives => `<operative>`
- applicatives => `<applicative>`
- builtin operative cores => `<builtin>`
- environments => `<environment>`
- prelude entry `name` => `<prelude:name>`
- native function => `<native>`

### 10.2 Display Mode

Display mode is used by `raw-display-to-string`.

It is identical to write mode except that non-empty strings are emitted:

- without surrounding quotes
- without escape rewriting

Because the empty string is `NIL`, the shared empty value still renders as
`()`.

### 10.3 Non-Injective Printing

Printing is not injective because `NIL` is both empty list and empty string.

Consequences:

- `()` and `""` print the same way in write mode
- `()` and `""` print the same way in display mode
- `raw-read-string` can round-trip the runtime value, but not the original
  source spelling

## 11. Prelude

The global environment includes these prelude bindings as applicatives:

```lisp
(fn! map (f lst)
  (if (null? lst) ()
    (cons (f (car lst)) (map f (cdr lst)))))

(fn! filter (pred lst)
  (if (null? lst) ()
    (if (pred (car lst))
      (cons (car lst) (filter pred (cdr lst)))
      (filter pred (cdr lst)))))

(fn! length (lst)
  (if (null? lst) 0 (+ 1 (length (cdr lst)))))

(fn! append (a b)
  (if (null? a) b
    (cons (car a) (append (cdr a) b))))
```

Compatibility requirements:

- these names must be present in the global environment
- they behave as ordinary applicatives
- their visible behavior must match the source above

The Rust implementation realizes them lazily by storing `Prelude` values and
parsing their lambda source on demand. A C implementation may realize them
eagerly if the behavior is the same.

## 12. Error Conditions

User-visible errors in the current language include:

- `ParseError { line, col }`
- `InvalidArgument`
- `TypeError`
- `ArithmeticOverflow`
- `DivisionByZero`
- `UnboundVariable`
- `NotCallable`
- `AlreadyDefined`
- `Cyclic` in debug builds

Typical causes:

| Error | Typical causes |
| --- | --- |
| `ParseError` | malformed list syntax, unexpected `)`, unterminated string |
| `InvalidArgument` | `(and)`, `(or)`, `(-)`, unknown string escape, duplicate ptree symbol, `(error ...)` |
| `TypeError` | wrong runtime type, non-boolean in boolean context, malformed parameter tree shape, missing operands surfacing through list access |
| `ArithmeticOverflow` | checked overflow in `+`, `-`, `*`, unary negation |
| `DivisionByZero` | second used operand of `/` is zero |
| `UnboundVariable` | symbol not found in the searched environment chain |
| `NotCallable` | attempt to call a non-combiner |
| `AlreadyDefined` | duplicate binding in the same frame |
| `Cyclic` | cyclic formal parameter tree during debug-build eager validation |

Two Rust error variants exist but are not part of the normal source-language
surface today:

- `ImmutableEnvironment`
- `TraceError`

## 13. Compatibility Quirks and Inconsistencies

These are all observable in the current implementation and matter for
behavioral compatibility.

### 13.1 Intentional-Looking Quirks

- only booleans are accepted in boolean contexts
- `""` and `()` are the same runtime value
- strings are list-like enough that `car`, `cdr`, and `pair?` work on non-empty
  strings
- `define!` is destructuring definition, not Scheme function-definition syntax
- `set!` takes an explicit environment
- `let` is simultaneous, not sequential
- `and` and `or` require at least two operands
- type predicates are vacuously true on zero operands
- many builtins ignore extra operands
- large integer literals that do not fit `isize` become symbols

### 13.2 Implementation Inconsistencies Worth Knowing

- eager formal-tree validation for `define!` and `vau` exists only in debug
  builds; release builds skip it
- `cond` accepts `else` in any clause position, not just the last one
- `raw-read-string` parses only the first expression and ignores trailing input
- a bare trailing `'` is accepted and read as `(quote ())`
- `make-empty-environment`, `gc-collect`, `error`, and `current-environment`
  ignore all operands instead of enforcing exact arity

These are not merely documentation notes. A reimplementation that aims for
bug-for-bug source compatibility should reproduce them.

## 14. Reimplementation Checklist

A compatible reimplementation should verify at least these behaviors:

- parse `'x` as `(quote x)`
- preserve the trailing-quote quirk where `'` reads as `(quote ())`
- implement dotted pairs and the exact dot-disambiguation rules
- preserve `NIL == empty string`
- intern symbols
- evaluate applicative operands left-to-right exactly once
- implement lexical closures for `lambda` and `vau`
- expose caller environments through `vau` environment parameters
- decide whether compatibility target means debug-build or release-build
  behavior, because `define!` and `vau` validation now differs by build mode
- preserve left-to-right depth-first search for multi-parent environments
- make `pair?`, `car`, and `cdr` work on non-empty strings
- preserve the exact `eq?` / `equal?` split, especially for strings
- preserve write/display formatting, including `()` for the empty string
- include the prelude bindings `map`, `filter`, `length`, and `append`
