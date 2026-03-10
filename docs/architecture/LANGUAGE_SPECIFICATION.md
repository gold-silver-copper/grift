# Grift Language Specification

This document is the implementation-compatibility specification for Grift as it
exists today. It is written to be concrete enough that the language can be
reimplemented in another host language such as C without reading the Rust
source during implementation.

This is not an aspirational design. If behavior here is strange, uneven, or
implementation-driven, that is intentional: a compatible implementation should
preserve the current behavior.

The conformance source for this document is:

- `crates/grift/src`
- `crates/grift/prelude.grift`
- `crates/grift/tests/lisp_tests.rs`

## 1. Scope

A conforming implementation must match the observable behavior of:

- the reader
- top-level multi-expression execution
- the runtime value model
- environment creation, lookup, definition, and mutation
- formal parameter tree validation and matching
- builtin operatives and applicatives, including current arity quirks
- `eval` and `apply`
- string formatting and parsing helpers
- the bundled prelude functions

It does not need to match Rust-specific APIs, arena layout, or type names at
the source-code level. It does need to preserve first-class runtime categories
and user-visible behavior.

## 2. Program Model

### 2.1 Input Unit

A program is zero or more expressions read from a source stream.

Top-level evaluation is sequential:

1. Read the next expression.
2. Evaluate it immediately in the global environment.
3. Continue until input is exhausted.
4. Return the value of the last expression.

If the program contains no expressions, the result is `#inert`.

Example:

```lisp
1 2 3
```

evaluates to `3`.

### 2.2 Persistent Global State

One interpreter instance has one persistent global environment.

Consequences:

- top-level `define!` survives later `eval` calls on the same interpreter
- prelude entries are installed at startup
- registered host-native functions are installed there

### 2.3 Empty Program vs Empty Reader Result

These are different:

- empty top-level program => `#inert`
- `(raw-read-string "")` => `()`

## 3. Surface Syntax

### 3.1 Source Kinds

Grift has two reader entry points:

- external source text, used by ordinary evaluation
- runtime strings, used by `raw-read-string`

These are intentionally not identical for non-ASCII text.

### 3.2 External Source Model

The external reader is byte-oriented, not Unicode-scalar-oriented.

Consequences:

- source positions count bytes, not Unicode scalar values
- non-ASCII text is not a stable portability target
- a C reimplementation should preserve the current byte-oriented behavior if it
  wants to be maximally compatible

### 3.3 Runtime String Source Model

`raw-read-string` reads from an existing runtime string, which is a linked chain
of `char` values. That path is character-oriented.

This means ordinary source parsing and `raw-read-string` are observably
different for non-ASCII input.

### 3.4 Whitespace and Comments

Whitespace characters are:

- space
- tab
- carriage return
- newline

Line comments start with `;` and continue until newline or end of input.

There are no block comments.

### 3.5 Recognized Surface Forms

The reader directly recognizes:

- proper lists: `(a b c)`
- dotted pairs: `(a . b)`
- quote shorthand: `'x`
- string literals: `"hello"`
- atoms: booleans, `#inert`, `#ignore`, integers, symbols

The reader does not recognize:

- quasiquote
- unquote
- unquote-splicing
- vectors
- byte strings
- character literals as a separate type
- block comments

### 3.6 Grammar Summary

The implementation is recursive-descent. A useful summary grammar is:

```text
program    := { ws_or_comment expr }*
expr       := list | quote | string | atom
quote      := "'" expr
list       := "(" list_body
list_body  := ")"
           | expr list_tail
list_tail  := ")"
           | ws_or_comment "." ws_or_comment expr ws_or_comment ")"
           | ws_or_comment expr list_tail
string     := '"' { string_char } '"'
atom       := { non_delimiter }+
```

This grammar is incomplete by itself because `.` is only special in one list
position.

### 3.7 Expression Parsing Algorithm

To parse one expression:

1. Skip whitespace and comments.
2. If the source is exhausted:
   - optional parsing returns no expression
   - required parsing raises `ParseError`
3. Read one character.
4. Dispatch:
   - `(` => parse list
   - `'` => parse quote shorthand
   - `"` => parse string
   - `)` => `ParseError`
   - anything else => parse atom

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

Notably:

- `.` is not a general delimiter
- `.` is only special when the list parser has already consumed the first list
  element and is checking for dotted-pair syntax

### 3.9 Lists and Dotted Pairs

After reading `(`:

1. Skip whitespace and comments.
2. If the next character is `)`, return `NIL`.
3. If the next character is `.`:
   - if `.` is followed by a delimiter or end of input, raise `ParseError`
   - otherwise treat it as the first character of an atom
4. Otherwise parse the first element.
5. Skip whitespace and comments.
6. If the next character is `.` and that dot is followed by a delimiter or end
   of input:
   - parse exactly one cdr expression
   - skip whitespace/comments
   - require `)`
   - return a dotted pair
7. Otherwise continue parsing remaining elements as a proper list.

Examples:

- `(a b c)` => proper list
- `(a . b)` => dotted pair
- `(a .foo)` => proper list containing the symbol `.foo`
- `(.foo bar)` => proper list whose first element is `.foo`
- `(. x)` => parse error
- `(a . b c)` => parse error

### 3.10 Quote Shorthand

`'expr` is rewritten to `(quote expr)`.

Examples:

- `'x` => `(quote x)`
- `'(1 2)` => `(quote (1 2))`
- `'` => `ParseError`

### 3.11 Atom Classification

Atoms are classified in this exact order:

1. `#t` or `#true` => true
2. `#f` or `#false` => false
3. `#inert` => inert singleton
4. `#ignore` => ignore singleton
5. signed base-10 integer fitting host `isize`
6. otherwise symbol

Consequences:

- `+42` is a number
- `-42` is a number
- `+` is a symbol
- `-` is a symbol
- `#unknown` is a symbol
- an overflowing integer literal is not a parse error; it becomes a symbol

### 3.12 Strings

String literals are delimited by `"`.

Recognized escapes are exactly:

- `\n`
- `\t`
- `\r`
- `\\`
- `\"`

Behavior:

- recognized escapes produce the corresponding character
- any other backslash escape raises `InvalidArgument`
- unterminated strings raise `ParseError`

### 3.13 Reader Errors

User-visible reader errors are:

- `ParseError { line, col }`
- `InvalidArgument` for unrecognized string escapes

Examples of `ParseError`:

- unexpected `)`
- unterminated list
- unterminated string
- malformed dotted-pair syntax
- bare `'` at end of input
- trailing non-whitespace after `raw-read-string` parses one expression

### 3.14 Source Coordinates

External parsing reports 1-based line and column positions.

`raw-read-string` parsing reports `(0, 0)` on parse errors because string-chain
parsing does not track coordinates.

## 4. Runtime Value Model

Grift exposes the following first-class runtime categories:

| Category | Write/display form |
| --- | --- |
| nil | `()` |
| booleans | `#t`, `#f` |
| numbers | decimal integer |
| symbols | symbol name |
| pairs | list or dotted-pair syntax |
| strings | quoted in write mode, raw in display mode |
| compound operatives | `<operative>` |
| applicatives | `<applicative>` |
| builtin operative cores | `<builtin>` |
| environments | `<environment>` |
| inert | `#inert` |
| ignore | `#ignore` |
| prelude entries | `<prelude:name>` |
| native host functions | `<native>` |

Not every runtime category has direct reader syntax. Source code can directly
construct only:

- nil
- booleans
- numbers
- symbols
- pairs/lists
- strings
- `#inert`
- `#ignore`

### 4.1 Nil

`NIL` is:

- the empty list
- the empty string

This alias is mandatory and observable:

- `()` and `""` are different source forms
- both evaluate to the same runtime object
- `(null? "")` is true
- `(pair? "")` is false
- `(equal? "" ())` is true
- `raw-write-to-string ""` produces `"()"`
- `raw-display-to-string ""` also produces `"()"`

### 4.2 Booleans

Only actual booleans participate in boolean contexts.

There is no general truthiness rule.

### 4.3 Numbers

Numbers are signed machine integers equivalent to Rust `isize`.

Behavior:

- literals are parsed only if they fit `isize`
- arithmetic uses checked arithmetic
- overflow raises `ArithmeticOverflow`
- division truncates toward zero

### 4.4 Symbols

Symbols are interned by name.

Consequences:

- two symbols with the same spelling are the same runtime object
- symbol equality is stable across repeated reads
- environment lookup compares symbol identity, not string contents on each
  lookup

### 4.5 Pairs

Pairs are immutable cons cells.

Behavior:

- proper lists are cons chains ending in `NIL`
- improper lists are cons chains whose final cdr is not `NIL`
- there is no `set-car!`
- there is no `set-cdr!`

### 4.6 Strings

Strings are linked `CharPair` chains.

A non-empty string is:

```text
CharPair(ch0, CharPair(ch1, ... CharPair(chn, NIL)))
```

Important properties:

- empty string is exactly `NIL`
- there is no separate character type
- a one-character string is a single `CharPair` node with `cdr = NIL`
- strings are not cons cells
- well-formed strings are `CharPair` chains terminated by `NIL`

Observable consequences:

- `(car "hello")` returns `"h"` as a fresh one-character string
- `(cdr "hello")` returns `"ello"`
- `(cdr "x")` returns `()`
- `(pair? "hello")` is true
- `(pair? "")` is false
- strings print as `()` when empty because empty string and nil are identical

### 4.7 Callables

Grift has five callable storage forms:

1. builtin operative cores
2. compound operatives created by `vau`
3. applicative wrappers
4. prelude entries
5. host-native functions

Their calling conventions differ:

- builtin operative core => receives raw operands plus caller environment
- compound operative => receives raw operands plus caller environment
- applicative => evaluates operands first, then calls the wrapped value
- prelude entry => normally reached through an applicative wrapper
- native host function => normally reached through an applicative wrapper

### 4.8 Environments

Environments are first-class values.

Each environment contains:

- `bindings`: an alist of `(key . value)` pairs
- `parents`: a proper list of parent environments

Ordinary evaluation only looks up symbols, but the representation does not
strictly forbid non-symbol keys in environment bindings.

### 4.9 Inert and Ignore

`#inert` is the singleton result of side-effect-oriented forms.

`#ignore` is a singleton used by formal parameter tree matching.

Both are self-evaluating.

### 4.10 Prelude Entries

Prelude functions are first-class runtime values of their own kind. The global
environment normally exposes them by wrapping each entry in an applicative.

### 4.11 Native Functions

Host-native functions are also first-class runtime values and are normally
bound as applicatives in the global environment.

## 5. Environments

### 5.1 Predefined Environments

The implementation pre-creates:

- `GROUND_ENV`: builtin-only environment
- `GLOBAL_ENV`: child of `GROUND_ENV`

Builtins live in `GROUND_ENV`.

Prelude entries and ordinary top-level user bindings live in `GLOBAL_ENV`.

### 5.2 Local Binding Order

Environment bindings are stored newest-first.

Successful definition prepends a new binding to the frame.

### 5.3 Lookup Algorithm

Environment lookup is normative.

#### Single-parent fast path

If an environment has zero or one parent:

1. Search the current frame from newest to oldest.
2. If found, return that value.
3. If there is one parent, continue with that parent.
4. If there are no parents, raise `UnboundVariable`.

#### Multi-parent search

If an environment has multiple parents:

1. Search the current frame.
2. Search parents in left-to-right depth-first order.
3. Track visited environments.
4. Skip already visited environments.
5. Return the first match found.

Consequences:

- lookup is depth-first, not breadth-first
- parent order matters
- an earlier parent's ancestor can shadow a later direct parent
- cycles are tolerated during lookup

### 5.4 `define!`

`define!` mutates only the current frame.

Behavior:

- if the same key already exists in that frame, raise `AlreadyDefined`
- parent bindings do not matter for this check
- successful definition prepends a new local binding

### 5.5 `set!`

`set!` mutates only the explicitly supplied target environment.

Behavior:

- only the target frame is searched
- parent environments are not consulted
- no new binding is created
- missing binding raises `UnboundVariable`

### 5.6 Environment Construction

`make-environment` creates a fresh frame with zero or more parent environments.

`make-empty-environment` creates a fresh parentless frame.

Child environments do not copy parent bindings. They inherit only through
links.

Consequences:

- a binding added to a parent after child creation is still visible in the
  child
- child-local definitions and mutations do not write back into parents

## 6. Evaluation

### 6.1 Self-Evaluating Values

These evaluate to themselves:

- nil
- booleans
- numbers
- strings
- builtin operative cores
- compound operatives
- applicatives
- environments
- `#inert`
- `#ignore`
- prelude entries
- native functions

Symbols are not self-evaluating.

Pairs are not self-evaluating.

### 6.2 Boolean Contexts

The following require real booleans and raise `TypeError` otherwise:

- `if`
- `cond` tests other than `else`
- `and`
- `or`
- `not`

There is no Scheme-style truthiness rule.

### 6.3 Symbol Evaluation

Evaluating a symbol performs environment lookup in the current environment.

### 6.4 Combination Evaluation

To evaluate `(f arg1 arg2 ...)`:

1. Evaluate `f`.
2. Dispatch on the resulting value.

Dispatch rules:

- builtin operative core => call with raw operand list
- compound operative => call with raw operand list
- applicative => evaluate operands left-to-right exactly once, then invoke the
  wrapped value on the evaluated operand list
- anything else => `NotCallable`

### 6.5 Applicative Evaluation Order

Applicatives evaluate arguments:

- strictly
- left to right
- exactly once

### 6.6 Compound Operative Invocation

Invoking a compound operative created by `vau` does the following:

1. Create a child environment whose only parent is the closure's definition
   environment.
2. Match the operative's formal parameter tree against the raw operand object.
3. If an environment parameter exists, bind it to the caller environment.
4. Evaluate the body in the new environment.

### 6.7 Closure Capture

Closures capture environment identity, not a snapshot.

Consequences:

- closures observe later `define!` or `set!` mutations to the captured
  environment object
- `(define! f (lambda ...))` supports recursion
- sequential top-level function definitions can support mutual recursion
- named `let` recursion works for the same reason

### 6.8 Tail Positions

The current implementation preserves tail behavior for:

- user operatives
- `if`
- `begin`
- `cond`
- `and`
- `or`
- `let`

A compatible implementation should not consume unbounded host stack on these
tail-recursive paths.

## 7. Formal Parameter Trees

Formal parameter trees are used by:

- `define!`
- `lambda`
- `vau`
- `fn!` indirectly
- compound operative invocation

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

Invalid examples:

- `42`
- `#t`
- `(x x)` because of duplicate symbols
- cyclic trees

### 7.2 Matching Semantics

Matching a parameter tree `ptree` against an object `obj` behaves as follows:

- if `ptree` is `NIL`, `obj` must be `NIL`
- if `ptree` is `#ignore`, do nothing
- if `ptree` is a symbol, bind that symbol to `obj`
- if `ptree` is a pair, `obj` must be a `Cons`, then match car and cdr
  recursively

Important limitation:

- matching a pair-shaped ptree requires the object to be a `Cons`
- a `CharPair` string node does not count as a pair for ptree matching

Strings are pair-like for `car`, `cdr`, and `pair?`, but not for ptree
matching.

### 7.3 Validation Rules

Validation is eager.

The implementation rejects a ptree if:

- a non-symbol, non-`#ignore`, non-`NIL` leaf appears
- a symbol appears more than once
- the tree is cyclic

Error mapping:

- duplicate symbol => `InvalidArgument`
- cyclic structure => `Cyclic`
- invalid leaf type => `TypeError`

### 7.4 `vau` Environment Parameter

For `vau`, `env-param` must be:

- a symbol, or
- `#ignore`

Additionally:

- `#ignore` means no caller-environment binding is created
- internally this case is normalized to "no environment parameter"
- if `env-param` also appears in `params`, `vau` raises `InvalidArgument`

## 8. Equality

Grift provides `eq?` and `equal?`.

### 8.1 `eq?`

`eq?` is defined as follows:

1. If the two objects are the same runtime object, return true.
2. If both objects are strings, compare by string content.
3. Otherwise, if both objects are immutable encapsulated values, compare by
   value.
4. Otherwise return false.

Immutable encapsulated values are:

- nil
- booleans
- numbers
- symbols
- strings
- `#inert`
- `#ignore`
- prelude entries
- native host functions

Non-immutable constructed values are identity-based:

- cons cells
- environments
- compound operatives
- applicatives
- builtin operative values

Consequences:

- two separately allocated but textually equal strings are `eq?`
- two separately allocated but structurally equal cons cells are not `eq?`
- two distinct environments are not `eq?`

### 8.2 `equal?`

`equal?` extends `eq?`:

1. If `eq?` is true, `equal?` is true.
2. If both values are cons cells, compare car and cdr recursively.
3. If both values are strings, compare string content.
4. If both values are environments and not `eq?`, return false.
5. Otherwise return false.

Consequences:

- cons cells compare structurally
- strings compare by content
- environments are identity-only
- every `eq?` success is also an `equal?` success

## 9. Builtin Operatives

Builtin operatives receive raw operands and the caller environment.

Unless otherwise stated:

- extra operands are ignored
- missing operands surface through ordinary list access failures, usually
  `TypeError`

### 9.1 `quote`

Syntax:

```lisp
(quote expr)
```

Behavior:

- returns the first operand unchanged
- ignores extra operands

### 9.2 `if`

Syntax:

```lisp
(if test then [else])
```

Behavior:

1. Evaluate `test`.
2. Require a boolean result.
3. If true, evaluate `then`.
4. If false:
   - if an else operand exists, evaluate it
   - otherwise return `NIL`
5. Ignore operands after the optional else.

Only the selected branch is evaluated.

### 9.3 `define!`

Syntax:

```lisp
(define! definiend expression)
```

Behavior:

1. Validate `definiend` as a formal parameter tree.
2. Evaluate `expression` in the current environment.
3. Match `definiend` against that result in the current frame.
4. Return `#inert`.

Important points:

- this is destructuring definition
- `(define! (f x) body)` destructures a value; it does not define a function
- same-frame redefinition raises `AlreadyDefined`
- extra operands are ignored

### 9.4 `fn!`

Syntax:

```lisp
(fn! name params body...)
```

Behavior:

- constructs a function equivalent to `(lambda params (begin body...))`
- binds it in the current frame under `name`
- returns `#inert`

Details:

- zero body expressions are allowed; such a function returns `NIL`
- `name` must be a symbol
- `params` are validated via `lambda`
- same-frame redefinition raises `AlreadyDefined`

### 9.5 `set!`

Syntax:

```lisp
(set! env-expr symbol value-expr)
```

Behavior:

1. Evaluate `env-expr`.
2. Require the result to be an environment.
3. Require `symbol` to be a symbol literal.
4. Evaluate `value-expr` in the current dynamic environment.
5. Mutate that symbol's existing binding in the target environment's own frame.
6. Return `#inert`.

Differences from Scheme:

- there is no `(set! x value)` shorthand
- the target environment is explicit
- parents are not searched for mutation
- no new binding is created
- extra operands are ignored

### 9.6 `lambda`

Syntax:

```lisp
(lambda params body...)
```

Behavior:

- validates `params`
- captures the current environment
- returns an applicative
- the applicative evaluates arguments before matching them against `params`
- zero body expressions are allowed and yield `NIL`

Operationally, `lambda` is derived from `vau` plus `wrap`.

### 9.7 `begin`

Syntax:

```lisp
(begin expr...)
```

Behavior:

- evaluate operands left to right
- return the last result
- `(begin)` returns `NIL`

### 9.8 `cond`

Syntax:

```lisp
(cond (test expr...) ...)
```

Behavior:

- evaluate clauses left to right
- the first element of each clause is the test
- if the test is a symbol named `else`, the clause matches immediately
- otherwise evaluate the test and require a boolean
- the first matching clause evaluates its body as if wrapped in `begin`
- a matching clause with no body returns `NIL`
- if no clause matches, return `NIL`

`else` is recognized by symbol name only and does not need to be last.

### 9.9 `and`

Syntax:

```lisp
(and expr...)
```

Behavior:

- zero operands => `#t`
- evaluate operands left to right
- every evaluated operand must be boolean
- stop at first `#f`
- return `#f` if any operand is false
- return `#t` if all operands are true

Unlike Scheme, `and` never returns an arbitrary non-boolean truthy value.

### 9.10 `or`

Syntax:

```lisp
(or expr...)
```

Behavior:

- zero operands => `#f`
- evaluate operands left to right
- every evaluated operand must be boolean
- stop at first `#t`
- return `#t` if any operand is true
- return `#f` if all operands are false

Unlike Scheme, `or` never returns an arbitrary non-boolean truthy value.

### 9.11 `let`

Grift supports both regular and named `let`.

Regular form:

```lisp
(let ((name init) ...) body...)
```

Behavior:

1. Create a child environment of the current environment.
2. Require each binding to be exactly `(symbol init)`.
3. Evaluate each `init` in the outer environment, not the child.
4. Define each `name` in the child.
5. Evaluate the body in the child.

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

Important detail:

- named-let init values are not re-evaluated during helper invocation
- zero body expressions are allowed and yield `NIL`

### 9.12 `vau`

Syntax:

```lisp
(vau params env-param body...)
```

Behavior:

- validate `params` as a formal parameter tree
- require `env-param` to be a symbol or `#ignore`
- reject overlap between `params` symbols and `env-param`
- wrap multiple body expressions in `begin`
- capture the current environment
- return a compound operative

If there are zero body expressions, the operative body is `NIL`.

### 9.13 `current-environment`

Syntax:

```lisp
(current-environment)
```

Behavior:

- returns the caller's current environment
- ignores all operands

## 10. Builtin Applicatives

Builtin applicatives receive already evaluated arguments.

Unless otherwise stated:

- extra operands are ignored
- missing operands usually surface as `TypeError`

### 10.1 Summary Table

| Builtin | Behavior summary |
| --- | --- |
| `cons` | pair constructor, with special string-building behavior |
| `+` | variadic checked addition |
| `-` | unary negate or left fold subtraction |
| `*` | variadic checked multiplication |
| `/` | two-argument integer division |
| `=` `<` `>` `<=` `>=` | numeric binary comparisons |
| `car` `cdr` | pair/string accessors |
| `list` | return evaluated arguments as a list |
| `null?` `pair?` `number?` `symbol?` `boolean?` `inert?` `ignore?` `operative?` `applicative?` `environment?` | variadic "all arguments satisfy predicate" predicates |
| `not` | boolean negation of first argument |
| `eq?` `equal?` | equality predicates |
| `eval` | evaluate an expression, optionally in an explicit environment |
| `wrap` `unwrap` | applicative construction/destruction |
| `make-environment` | make new environment with copied parent list |
| `make-empty-environment` | make parentless environment |
| `gc-collect` | force GC, return collected count |
| `error` | always raises `InvalidArgument` |
| `apply` | apply a combiner to an already constructed operand object |
| `raw-read-string` | parse exactly one expression from a runtime string |
| `raw-display-to-string` | display-format a value into a runtime string |
| `raw-write-to-string` | write-format a value into a runtime string |

### 10.2 `cons`

Syntax:

```lisp
(cons a b)
```

Normal behavior:

- returns `(a . b)`

Special string behavior:

- if `a` is a one-character string
- and `b` is a well-formed string chain
- then `cons` returns a new string whose first character is `a` and whose tail
  is `b`

Consequences:

- `(cons (car "h") "ello")` => `"hello"`
- `(cons (car "h") ())` => `"h"`
- `(cons (car "a") 42)` => `TypeError`

### 10.3 Arithmetic

`(+ ...)`

- zero args => `0`
- variadic checked addition

`(- a b ...)`

- zero args => `ArityError`
- one arg => checked negation
- more args => left fold subtraction

`(* ...)`

- zero args => `1`
- variadic checked multiplication

`(/ a b)`

- divides the first argument by the second
- integer truncation toward zero
- division by zero => `DivisionByZero`
- extra operands are ignored

### 10.4 Numeric Comparisons

`=` `<` `>` `<=` `>=`

Behavior:

- consume the first two arguments
- require both to be numbers
- ignore extra operands
- return a boolean

These are not chain comparisons.

### 10.5 `car` and `cdr`

`car`:

- for cons cells, returns the cons cell's car
- for non-empty strings, returns a fresh one-character string
- otherwise `TypeError`

`cdr`:

- for cons cells, returns the cons cell's cdr
- for non-empty strings, returns the tail string
- otherwise `TypeError`

### 10.6 `list`

`(list ...)` returns the evaluated argument list exactly as received.

It does not allocate a second copy.

### 10.7 Type Predicates

The following predicates are variadic and return true iff every argument
matches:

- `null?`
- `pair?`
- `number?`
- `symbol?`
- `boolean?`
- `inert?`
- `ignore?`
- `operative?`
- `applicative?`
- `environment?`

Zero-argument behavior:

- all of these return `#t`

Membership rules:

- `null?` => only `NIL`
- `pair?` => `Cons` and non-empty `CharPair` strings
- `operative?` => builtin operative cores and compound operatives
- `applicative?` => applicative wrappers only
- `environment?` => environments only

Notably:

- prelude entries are not `operative?`
- native host functions are not `operative?`
- wrapped builtin operatives are `applicative?` because the wrapper is an
  applicative value

### 10.8 `not`

Syntax:

```lisp
(not boolean)
```

Behavior:

- reads only the first argument
- requires a boolean
- returns its negation

### 10.9 `eq?` and `equal?`

These follow the equality rules from section 8.

Both consume their first two operands and ignore extras.

### 10.10 `eval`

Syntax:

```lisp
(eval expr [env])
```

Behavior:

- if no explicit environment is supplied, evaluate in `GLOBAL_ENV`
- if an explicit environment is supplied, it must be an environment
- the supplied expression object is evaluated as-is; it is not reparsed

### 10.11 `wrap`

Syntax:

```lisp
(wrap combiner)
```

Behavior:

- returns an applicative wrapping the first argument
- does not validate that the wrapped value is actually callable

A wrapped non-callable value becomes an applicative that fails with
`NotCallable` when applied.

### 10.12 `unwrap`

Syntax:

```lisp
(unwrap applicative)
```

Behavior:

- returns the immediate inner wrapped value
- non-applicatives raise `TypeError`

`unwrap` unwraps only one layer.

### 10.13 `make-environment`

Syntax:

```lisp
(make-environment env ...)
```

Behavior:

- all arguments must be environments
- returns a fresh environment whose parent list is a copy of the argument list
- the environment starts with no local bindings

The copy matters only for independence from later mutation of the original
argument list object. It does not clone parent environments.

### 10.14 `make-empty-environment`

Syntax:

```lisp
(make-empty-environment)
```

Behavior:

- creates a fresh environment with no parents
- any argument at all => `ArityError`

### 10.15 `gc-collect`

Syntax:

```lisp
(gc-collect)
```

Behavior:

- forces a garbage-collection cycle
- ignores operands
- returns the number of collected objects as a number

### 10.16 `error`

Syntax:

```lisp
(error msg)
```

Behavior:

- always raises `InvalidArgument`
- the message is currently ignored

### 10.17 `apply`

Syntax:

```lisp
(apply combiner arg-list [env])
```

Behavior:

- if no explicit environment is supplied, use `GLOBAL_ENV`
- if an explicit environment is supplied, it must be an environment
- `arg-list` is passed directly as the operand object
- `apply` does not walk `arg-list` evaluating its elements

This point is critical:

- when the combiner is operative, `arg-list` is the raw operand object
- when the combiner is applicative, `arg-list` is treated as already evaluated
  arguments

Therefore `apply` is not Scheme-style "evaluate the list's elements for me."

This is observable:

- `(apply + (list 1 2 3))` works because numbers are self-evaluating
- `(apply if (list #t 'x 0) env)` works because `if` is operative and receives
  the raw operands

### 10.18 `raw-read-string`

Syntax:

```lisp
(raw-read-string string)
```

Behavior:

- the argument must be a well-formed runtime string
- empty string => `NIL`
- parse exactly zero or one expression
- reject trailing non-whitespace/comment input
- if one expression is parsed, return that runtime object directly

This reader path uses character chains, not byte-oriented source text.

### 10.19 `raw-display-to-string`

Syntax:

```lisp
(raw-display-to-string object)
```

Behavior:

- validates reachable strings/symbol names for formatting
- formats using display semantics
- returns a runtime string

### 10.20 `raw-write-to-string`

Syntax:

```lisp
(raw-write-to-string object)
```

Behavior:

- validates reachable strings/symbol names for formatting
- formats using write semantics
- returns a runtime string

## 11. Formatting Semantics

Grift has two formatting modes:

- write mode
- display mode

### 11.1 Common Formatting Rules

Formatting outputs:

- nil => `()`
- `#t` / `#f`
- numbers in decimal
- symbols by symbol name
- proper lists in list notation
- improper lists in dotted notation
- `#inert`
- `#ignore`
- prelude entries as `<prelude:name>`
- other opaque runtime categories by placeholder type name

Opaque forms currently print as:

- compound operative => `<operative>`
- applicative => `<applicative>`
- builtin operative core => `<builtin>`
- environment => `<environment>`
- native function => `<native>`

### 11.2 Write Mode

Write mode prints strings with quotes and escapes.

Escapes used are exactly:

- `"` => `\"`
- `\` => `\\`
- newline => `\n`
- tab => `\t`
- carriage return => `\r`

Other characters print as themselves.

Examples:

- `"hello"` => `"hello"`
- a string containing a real newline prints with `\n`

### 11.3 Display Mode

Display mode prints strings without surrounding quotes or escapes.

Examples:

- `"hello"` => `hello`
- a string containing a real newline prints a real newline

### 11.4 Empty String Formatting

Because empty string and nil are the same runtime object:

- write mode prints empty string as `()`
- display mode also prints empty string as `()`

### 11.5 Formatting Validation

Formatting validates that:

- symbols point to well-formed string chains
- strings are well-formed string chains
- lists recursively contain formattable values

Malformed reachable string structure raises `TypeError` in raw string helpers
and formatting failure in host-side formatting APIs.

This validation requirement applies uniformly to all public formatting entry
points:

- `write_value`
- `display_value`
- native/host formatting through the erased `LispOps` interface
- `raw-write-to-string`
- `raw-display-to-string`

## 12. Prelude

The implementation installs the following prelude functions in the global
environment:

### 12.1 `map`

```lisp
(fn! map (f lst)
  (if (null? lst) ()
    (cons (f (car lst)) (map f (cdr lst)))))
```

### 12.2 `filter`

```lisp
(fn! filter (pred lst)
  (if (null? lst) ()
    (if (pred (car lst))
      (cons (car lst) (filter pred (cdr lst)))
      (filter pred (cdr lst)))))
```

### 12.3 `length`

```lisp
(fn! length (lst)
  (if (null? lst) 0 (+ 1 (length (cdr lst)))))
```

### 12.4 `append`

```lisp
(fn! append (a b)
  (if (null? a) b
    (cons (car a) (append (cdr a) b))))
```

### 12.5 Prelude Invocation Model

Prelude bindings are not precompiled closures.

Current behavior:

1. Each prelude binding stores static source text for a lambda expression.
2. Each call reparses that source text.
3. The parsed lambda is evaluated in `GLOBAL_ENV`.
4. The resulting applicative is unwrapped to its inner operative.
5. That operative is invoked.

This means prelude calls are reparsed and reevaluated on every invocation.

## 13. Errors

The observable error variants are:

- `OutOfMemory`
- `InvalidArgument`
- `ArityError`
- `Cyclic`
- `TypeError`
- `ParseError { line, col }`
- `ArithmeticOverflow`
- `DivisionByZero`
- `UnboundVariable`
- `NotCallable`
- `AlreadyDefined`

Typical triggers:

| Error | Typical causes |
| --- | --- |
| `ParseError` | malformed syntax, trailing input in `raw-read-string` |
| `InvalidArgument` | duplicate ptree symbol, bad string escape, `(error ...)` |
| `ArityError` | `(-)`; `(make-empty-environment 1)` |
| `Cyclic` | cyclic formal parameter tree validation |
| `TypeError` | wrong value kind, non-boolean boolean context, malformed string chain |
| `ArithmeticOverflow` | checked integer overflow |
| `DivisionByZero` | division by zero |
| `UnboundVariable` | missing symbol lookup or `set!` target |
| `NotCallable` | calling a non-combiner |
| `AlreadyDefined` | same-frame redefinition with `define!` or `fn!` |

## 14. Observable Quirks

These behaviors are surprising but normative.

### 14.1 Empty String Is Nil

The empty string and nil are the same runtime object. This affects:

- equality
- predicates
- formatting
- `raw-read-string ""`
- string traversal

### 14.2 Strings Are Pair-Like, But Only Partially

Strings count as pairs for:

- `pair?`
- `car`
- `cdr`

But they do not count as pairs for formal parameter tree matching.

### 14.3 `eq?` on Strings Is Content-Based

Unlike cons cells and environments, strings compare by content under `eq?`.

### 14.4 External Reader and `raw-read-string` Differ

External source parsing is byte-oriented.

`raw-read-string` parses runtime character chains.

The two entry points are therefore not perfectly symmetric.

### 14.5 `error` Ignores Its Message

The argument to `error` is currently unused. The builtin simply raises
`InvalidArgument`.

### 14.6 `wrap` Does Not Validate Callability

`wrap` can create an applicative around a non-callable value. The resulting
object fails only when invoked.

### 14.7 `apply` Does Not Evaluate Its Argument List

`apply` expects the caller to provide the operand object in the correct form.
This is especially important for applicatives.

### 14.8 Prelude Calls Reparse on Every Call

Prelude entries are invoked from stored source text rather than from persistent
compiled closures.

## 15. Reimplementation Checklist

A compatible C or other-language implementation should preserve at least these
properties:

1. Empty top-level input yields `#inert`, while empty `raw-read-string` yields
   `()`.
2. Empty string is the same runtime object as nil.
3. Only booleans are accepted in boolean contexts.
4. Symbols are interned.
5. Strings are linked character chains with pair-like `car`/`cdr`.
6. `define!` destructures via formal parameter trees.
7. `set!` requires an explicit environment and mutates only that frame.
8. Multi-parent environment lookup is left-to-right depth-first with cycle
   detection.
9. Applicatives evaluate operands exactly once, left to right.
10. `apply` does not evaluate the supplied argument list.
11. `eval` defaults to the global environment when no explicit environment is
    supplied.
12. `regular let` evaluates initializers in the outer environment.
13. Named `let` does not re-evaluate its initializer values during invocation.
14. Closures capture live environment objects, not snapshots.
15. String construction via `cons` requires a well-formed string tail.
16. `eq?` is value-based for strings and immutable scalars, identity-based for
    pairs and environments.
17. `equal?` is structural for cons cells and strings, but not for distinct
    environments.
18. Write/display formatting of strings, especially the empty string, matches
    current behavior exactly.
19. Prelude functions `map`, `filter`, `length`, and `append` are present.
20. Prelude calls reparse their source on each invocation.

If this document and the implementation disagree, the code and tests remain the
final conformance source.
