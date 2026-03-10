# Grift Language Specification

This document is the implementation-compatibility specification for the Grift
language as implemented today in:

- `crates/grift/src`
- `crates/grift/prelude.grift`
- `crates/grift/tests/lisp_tests.rs`

It is not an aspirational design document. A compatible reimplementation,
including one written in C, should match the behaviors described here even when
they are surprising, uneven, or clearly implementation-driven.

## 1. Conformance Target

A conforming implementation matches the observable behavior of:

- the reader
- top-level program execution
- runtime value representation as exposed through predicates, equality, and
  formatting
- environment lookup, definition, and mutation
- formal-parameter-tree validation and matching
- builtin operatives and applicatives, including their current arity quirks
- `eval` and `apply`
- `raw-read-string`
- `raw-display-to-string`
- `raw-write-to-string`
- the bundled prelude functions

It does not need to reproduce Rust-only APIs such as trait shapes, but it does
need to preserve all host-visible runtime value kinds, including environments,
prelude entries, and native function values.

## 2. Program Model

### 2.1 Input Unit

A program is zero or more expressions read from a source stream.

Top-level evaluation rules:

1. Expressions are read sequentially.
2. Each expression is evaluated immediately after being read.
3. The program result is the value of the last expression.
4. If the source contains no expressions, the result is `#inert`.

Example:

```lisp
1 2 3
```

This evaluates to `3`.

### 2.2 Evaluation Environment

Top-level evaluation normally happens in the global environment.

The global environment is persistent within one interpreter instance:

- a top-level `define!` survives later `eval` calls on the same interpreter
- prelude bindings are installed there at startup
- registered native functions are bound there

### 2.3 Empty Program Result

The empty top-level program evaluates to `#inert`.

This is different from `raw-read-string`, which returns `()` for empty input.

## 3. Reader

The reader is recursive-descent and has two entry points:

- parsing external source text for `eval`
- parsing a runtime string value for `raw-read-string`

These entry points are similar but not identical in their source model.

### 3.1 Source Model

#### External Source

The ordinary reader is byte-oriented.

Important consequences:

- source positions count bytes, not Unicode scalar values
- portable source should be treated as ASCII plus the documented escapes
- non-ASCII source text is not a stable portability target

#### Runtime String Source

`raw-read-string` parses an existing runtime string, which is a linked chain of
Unicode `char` values. That path is character-oriented, not byte-oriented.

This means the two reader entry points are not perfectly symmetric for
non-ASCII text.

### 3.2 Whitespace and Comments

Whitespace characters are:

- space
- tab
- carriage return
- newline

Line comments begin with `;` and continue until newline or end of input.

There are no block comments.

### 3.3 Surface Forms

The reader recognizes:

- proper lists: `(a b c)`
- dotted pairs: `(a . b)`
- quote shorthand: `'x`
- string literals: `"hello"`
- atoms: booleans, `#inert`, `#ignore`, integers, and symbols

The reader does not recognize:

- quasiquote
- unquote
- unquote-splicing
- vectors
- byte strings
- character literals as a separate type
- block comments

### 3.4 Grammar Summary

The accepted grammar is best understood procedurally, but the implemented
surface language can be summarized as:

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

That grammar is incomplete by itself because `.` is special only in one
particular list position.

### 3.5 Reader Algorithm

To parse one expression:

1. Skip whitespace and comments.
2. If input is exhausted:
   - top-level optional parsing returns no expression
   - required-expression parsing raises `ParseError`
3. Read one character.
4. Dispatch:
   - `(` starts list parsing
   - `'` parses the next expression and rewrites it to `(quote expr)`
   - `"` starts string parsing
   - `)` is a parse error
   - anything else starts atom parsing

### 3.6 Atom Delimiters

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
- `.` is only special when the list parser has already read the car of a list
  and is deciding whether the next token is dotted-pair syntax

### 3.7 Lists and Dotted Pairs

After reading `(`:

1. Skip whitespace/comments.
2. If the next character is `)`, return `NIL`.
3. If the next character is `.`:
   - if `.` is followed by a delimiter or end of input, signal `ParseError`
   - otherwise parse an atom beginning with `.`
4. Otherwise parse the first element.
5. Skip whitespace/comments.
6. If the next character is `.` and that `.` is followed by a delimiter or end
   of input, parse exactly one cdr expression, require `)`, and return a
   dotted pair.
7. Otherwise continue parsing remaining list elements as a proper list.

Examples:

- `(a b c)` => proper list
- `(a . b)` => dotted pair
- `(a .foo)` => proper list containing the symbol `.foo`
- `(.foo bar)` => proper list whose first element is `.foo`
- `(. x)` => parse error
- `(a . b c)` => parse error

### 3.8 Quote Shorthand

`'expr` rewrites to `(quote expr)`.

Examples:

- `'x` => `(quote x)`
- `'(1 2)` => `(quote (1 2))`
- `'` => `ParseError`

### 3.9 Atom Classification

An atom token is classified in this exact order:

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

### 3.10 Strings

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

Runtime strings are stored as linked `CharPair` nodes, not as flat buffers.

### 3.11 Reader Errors

Reader-visible error classes are:

- `ParseError { line, col }`
- `InvalidArgument` for unknown string escapes

Examples of `ParseError`:

- unexpected `)`
- unterminated list
- unterminated string
- malformed dotted-pair syntax
- bare `'` at end of input
- trailing junk after `raw-read-string` parses one expression

### 3.12 Source Coordinates

Ordinary source parsing reports 1-based line and column positions.

`raw-read-string` parsing reports `(0, 0)` on parse errors because chain-backed
parsing does not track coordinates.

## 4. Runtime Value Model

Grift exposes these first-class runtime categories:

| Category | External/write form |
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

### 4.1 Nil

`NIL` is the empty list.

It is also the empty string at runtime.

This alias is observable and mandatory:

- `()` and `""` are different source forms
- both evaluate to the same runtime object
- `(null? "")` is true
- `(pair? "")` is false
- `raw-write-to-string ""` produces `"()"`
- `raw-display-to-string ""` also produces `"()"`

### 4.2 Booleans

Only booleans participate in boolean contexts.

There is no general truthiness rule.

### 4.3 Numbers

Numbers are signed machine integers equivalent to Rust `isize`.

Behavior:

- literals are parsed only if they fit `isize`
- arithmetic builtins use checked arithmetic
- overflow raises `ArithmeticOverflow`
- division truncates toward zero

### 4.4 Symbols

Symbols are interned by name.

Observable consequences:

- two symbols with the same spelling are pointer-identical
- symbol equality is stable across repeated reads of the same symbol spelling
- environment lookup compares symbol identity, not string contents at lookup
  time

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
- a one-character string is one `CharPair` node with `cdr = NIL`
- strings are not `Cons` cells, even though some operations treat them as
  pair-like

Observable consequences:

- `(car "hello")` returns `"h"` as a freshly allocated one-character string
- `(cdr "hello")` returns `"ello"`
- `(cdr "x")` returns `()`
- `(pair? "hello")` is true
- `(pair? "")` is false
- strings format as `()` when empty, because empty string and nil are the same
  runtime value

### 4.7 Callables

Grift has five callable storage forms:

1. builtin operative cores
2. compound operatives created by `vau`
3. applicative wrappers
4. prelude entries
5. native host functions

Their calling conventions differ:

- builtin operative core => receives raw operand object plus caller environment
- compound operative => receives raw operand object plus caller environment
- applicative => evaluates operands first, then invokes its wrapped value
- prelude entry => normally reached through an applicative wrapper
- native function => normally reached through an applicative wrapper

### 4.8 Environments

Environments are first-class values.

Each environment contains:

- `bindings`: an alist of `(symbol-or-other-key . value)` pairs
- `parents`: a proper list of parent environments

Although ordinary language evaluation only looks up symbols, the implementation
does not strictly enforce that every environment binding key is a symbol.

### 4.9 Inert and Ignore

`#inert` is a singleton used as the return value of side-effect-oriented forms.

`#ignore` is a singleton used specially in formal parameter trees.

Both are self-evaluating.

### 4.10 Prelude Entries

Prelude functions are stored as first-class runtime values of their own kind.

They are usually exposed to user code by wrapping them in applicatives at
startup.

### 4.11 Native Functions

Host-native functions are also first-class runtime values and are usually bound
as applicatives in the global environment.

## 5. Environments

### 5.1 Reserved Environments

The implementation pre-creates:

- `GROUND_ENV`: builtin-only environment
- `GLOBAL_ENV`: child of `GROUND_ENV`

Builtins live in `GROUND_ENV`.

Prelude bindings and user top-level bindings live in `GLOBAL_ENV`.

### 5.2 Local Bindings

Environment bindings are stored as an alist in newest-first order.

Defining a new binding prepends a new entry.

This makes shadowing within one frame observable by definition order, although
duplicate same-frame definitions are normally rejected.

### 5.3 Lookup Algorithm

Environment lookup is normative and must match the current implementation.

#### Single-Parent Fast Path

If an environment has zero or one parent:

1. search the current frame from newest binding to oldest
2. if not found and there is one parent, continue to that parent
3. if not found and there are no parents, raise `UnboundVariable`

#### Multi-Parent Search

If an environment has multiple parents:

1. search the current frame
2. search parents in left-to-right depth-first order
3. track visited environments to avoid infinite loops
4. skip an already visited environment
5. return the first match found

Consequences:

- lookup is depth-first, not breadth-first
- parent order matters
- an earlier parent's ancestor can shadow a later direct parent
- cycles are tolerated during lookup; they do not loop forever

### 5.4 `define!`

`define!` mutates only the current frame.

Behavior:

- if the frame already contains the same key, raise `AlreadyDefined`
- parent bindings do not matter for this check
- successful definition prepends a new local binding

### 5.5 `set!`

`set!` mutates only the explicitly supplied target environment.

Behavior:

- only the target environment's own frame is searched
- parents are not consulted
- no new binding is created
- missing binding raises `UnboundVariable`

### 5.6 Environment Construction

`make-environment` creates a fresh frame with zero or more supplied parents.

`make-empty-environment` creates a fresh parentless frame.

Child environments do not copy bindings from parents. They inherit only through
links.

This is observable:

- parent bindings defined after child creation are still visible through lookup
- child-local mutations do not write back into parent frames

## 6. Evaluation Semantics

### 6.1 Self-Evaluating Values

These values evaluate to themselves:

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
- prelude values
- native values

Symbols are not self-evaluating.

Pairs are not self-evaluating.

### 6.2 Boolean Contexts

The following require actual boolean values and raise `TypeError` otherwise:

- `if`
- `cond` tests except `else`
- `and`
- `or`
- `not`

There is no Scheme-style "everything except `#f` is true" rule.

### 6.3 Symbol Evaluation

Evaluating a symbol performs environment lookup in the current environment.

### 6.4 Combination Evaluation

To evaluate `(f arg1 arg2 ...)`:

1. evaluate `f`
2. dispatch on the resulting value

Dispatch rules:

- builtin operative core => invoke with raw operand list
- compound operative => invoke with raw operand list
- applicative => evaluate operands left-to-right exactly once, then invoke the
  wrapped value with the evaluated argument list
- anything else => `NotCallable`

### 6.5 Applicative Evaluation Order

Applicative argument evaluation is:

- strict
- left to right
- exactly once

### 6.6 Compound Operative Invocation

Invoking a compound operative created by `vau` performs:

1. create a child environment whose single parent is the closure's definition
   environment
2. match the formal parameter tree against the raw operand object
3. if an environment parameter exists, bind it to the caller environment
4. evaluate the body in the new environment

This yields lexical scope plus an explicit hook into the caller environment.

### 6.7 Tail Positions

The current implementation preserves tail behavior for:

- user operatives
- `if`
- `begin`
- `cond`
- `and`
- `or`
- `let`

A compatible implementation should not consume unbounded host stack for these
tail-recursive cases.

### 6.8 Top-Level Multi-Expression Evaluation

At top level, multiple expressions are evaluated in sequence and only the last
result is returned.

Example:

```lisp
(define! x 1)
(define! y 2)
(+ x y)
```

The result is `3`.

## 7. Formal Parameter Trees

Formal parameter trees are used by:

- `define!`
- `lambda`
- `vau`
- `fn!` indirectly, through `lambda`
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

That means strings are pair-like for `car`, `cdr`, and `pair?`, but not for
parameter-tree destructuring.

### 7.3 Validation Rules

Validation is eager.

The implementation rejects a ptree if:

- a non-symbol/non-`#ignore`/non-`NIL` leaf appears
- a symbol appears more than once
- the tree is cyclic

Error mapping:

- duplicate symbol => `InvalidArgument`
- cyclic tree => `Cyclic`
- invalid leaf type => `TypeError`

### 7.4 `vau` Environment Parameter Validation

For `vau`, `env-param` must be:

- a symbol, or
- `#ignore`

Additionally:

- if `env-param` is `#ignore`, no environment binding is created
- internally this case is normalized to "no env parameter"
- if `env-param` also appears in `params`, `vau` raises `InvalidArgument`

## 8. Builtin Operatives

Unless explicitly stated otherwise, builtin operatives:

- receive operands unevaluated
- ignore extra operands beyond those they use
- surface missing operands via ordinary list-access failures, usually
  `TypeError`

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

1. evaluate `test`
2. `test` must produce a boolean
3. if true, evaluate `then`
4. if false:
   - if an else operand exists, evaluate it
   - otherwise return `NIL`
5. ignore extra operands after the optional else

Only the selected branch is evaluated.

### 8.3 `define!`

Syntax:

```lisp
(define! definiend expression)
```

Behavior:

1. validate `definiend` as a formal parameter tree
2. evaluate `expression` in the current environment
3. match `definiend` against the resulting value in the current frame
4. return `#inert`

Important points:

- this is destructuring definition, not Scheme function-definition shorthand
- `(define! (f x) body)` destructures a value; it does not define a function
- same-frame redefinition raises `AlreadyDefined`
- extra operands are ignored

Examples:

```lisp
(define! (#ignore b c) (list 10 20 30))
```

binds `b = 20`, `c = 30`.

### 8.4 `fn!`

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
- `params` are validated through `lambda`
- redefinition in the same frame raises `AlreadyDefined`

### 8.5 `set!`

Syntax:

```lisp
(set! env-expr symbol value-expr)
```

Behavior:

1. evaluate `env-expr`
2. the result must be an environment
3. `symbol` must be a symbol literal
4. evaluate `value-expr` in the current environment
5. mutate that symbol's existing binding in the target environment's own frame
6. return `#inert`

Differences from Scheme:

- there is no `(set! x value)` shorthand
- the target environment is explicit
- parent environments are not searched for mutation
- extra operands are ignored

### 8.6 `lambda`

Syntax:

```lisp
(lambda params body...)
```

Behavior:

- validates `params`
- captures the current environment lexically
- returns an applicative
- that applicative evaluates arguments before matching them against `params`
- zero body expressions are allowed; the function returns `NIL`

Operationally, `lambda` is derived from `vau` and `wrap`.

### 8.7 `begin`

Syntax:

```lisp
(begin expr...)
```

Behavior:

- evaluate operands left to right
- return the last operand's result
- `(begin)` returns `NIL`

### 8.8 `cond`

Syntax:

```lisp
(cond (test expr...) ...)
```

Behavior:

- evaluate clauses left to right
- each clause's first element is the test
- if the test is the symbol named `else`, the clause matches immediately
- otherwise evaluate the test and require a boolean
- for the first matching clause, evaluate the body as if wrapped in `begin`
- a matching clause with no body returns `NIL`
- if no clause matches, return `NIL`

Quirk:

- `else` is recognized only by symbol name
- it need not be the final clause

### 8.9 `and`

Syntax:

```lisp
(and expr...)
```

Behavior:

- zero operands => `#t`
- evaluate operands left to right
- every evaluated operand must be a boolean
- stop at first `#f`
- if any operand is `#f`, return `#f`
- if all operands are `#t`, return `#t`

Unlike Scheme, `and` never returns a non-boolean last truthy value.

### 8.10 `or`

Syntax:

```lisp
(or expr...)
```

Behavior:

- zero operands => `#f`
- evaluate operands left to right
- every evaluated operand must be a boolean
- stop at first `#t`
- if any operand is `#t`, return `#t`
- if all operands are `#f`, return `#f`

Unlike Scheme, `or` never returns an arbitrary non-boolean truthy value.

### 8.11 `let`

Regular form:

```lisp
(let ((name init) ...) body...)
```

Behavior:

1. create a child environment of the current environment
2. evaluate every `init` in the outer environment, not the child
3. define each `name` in the child
4. evaluate the body in the child

This is simultaneous binding, not `let*`.

Named form:

```lisp
(let name ((param init) ...) body...)
```

Behavior:

1. evaluate all init expressions in the outer environment
2. create a child environment
3. define `name` in that child as a recursive function
4. invoke that function with the already evaluated init values

Important detail:

- named-let init values are not re-evaluated during the helper invocation

Zero body expressions are allowed and yield `NIL`.

### 8.12 `vau`

Syntax:

```lisp
(vau params env-param body...)
```

Behavior:

- validate `params`
- validate `env-param`
- capture the current environment lexically
- create a compound operative
- zero body expressions are allowed and yield `NIL`

`env-param` behavior:

- if it is `#ignore`, no caller-environment binding is created
- otherwise it is bound to the caller environment when the operative runs

### 8.13 `current-environment`

Syntax:

```lisp
(current-environment)
```

Behavior:

- returns the caller's current environment
- ignores all operands

## 9. Builtin Applicatives

Builtin applicatives evaluate their arguments before executing.

Their exact arity policy is irregular and must be matched form by form.

### 9.1 Arithmetic

#### `+`

```lisp
(+ n ...)
```

Behavior:

- variadic addition
- zero arguments => `0`
- all arguments must be numbers
- overflow => `ArithmeticOverflow`

#### `-`

```lisp
(- n ...)
```

Behavior:

- zero arguments => `ArityError`
- one argument => arithmetic negation
- multiple arguments => left-fold subtraction
- all used arguments must be numbers
- overflow => `ArithmeticOverflow`

#### `*`

```lisp
(* n ...)
```

Behavior:

- variadic multiplication
- zero arguments => `1`
- all arguments must be numbers
- overflow => `ArithmeticOverflow`

#### `/`

```lisp
(/ a b)
```

Behavior:

- uses only the first two evaluated arguments
- both used arguments must be numbers
- division truncates toward zero
- division by zero => `DivisionByZero`
- missing operands usually surface as `TypeError`
- extra operands are ignored

### 9.2 Numeric Comparison

The following use only the first two evaluated arguments:

- `(= a b)`
- `(< a b)`
- `(> a b)`
- `(<= a b)`
- `(>= a b)`

Behavior:

- both used arguments must be numbers
- result is a boolean
- missing operands usually raise `TypeError`
- extra operands are ignored

### 9.3 Pair and String Operations

#### `cons`

```lisp
(cons a b)
```

Normal behavior:

- constructs a pair `(a . b)`

String-specialized behavior:

- if `a` is a one-character string, construct a `CharPair` node containing that
  character and `cdr = b`

Important limitations:

- only a one-character first argument triggers string construction
- `(cons "ab" "cd")` creates a pair, not a string
- the second argument is not validated to be a proper string tail

#### `car`

```lisp
(car pair-or-string)
```

Behavior:

- if argument is a pair, return its car
- if argument is a non-empty string, return a fresh one-character string
- otherwise raise `TypeError`
- extra operands are ignored

#### `cdr`

```lisp
(cdr pair-or-string)
```

Behavior:

- if argument is a pair, return its cdr
- if argument is a non-empty string, return the tail string
- otherwise raise `TypeError`
- extra operands are ignored

#### `list`

```lisp
(list obj ...)
```

Behavior:

- return the already evaluated argument list as a proper list
- `(list)` returns `NIL`

### 9.4 Predicates

The following are variadic universal predicates:

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

Behavior:

- return `#t` if every supplied operand matches
- return `#f` as soon as one operand does not match
- with zero operands, return `#t`

Specific meanings:

- `pair?` is true for `Cons` cells and non-empty strings
- `operative?` is true for compound operatives and bare builtin operative cores
- `applicative?` is true only for applicative wrapper values
- raw `Prelude` values are not considered applicatives by `applicative?`, even
  though user-visible prelude bindings are wrapped as applicatives

#### `not`

```lisp
(not boolean)
```

Behavior:

- negates the first used operand
- that operand must be boolean
- missing operand raises `TypeError`
- extra operands are ignored

### 9.5 Equality

#### `eq?`

```lisp
(eq? a b)
```

Algorithm:

1. if `a` and `b` are the same runtime object, return `#t`
2. otherwise, if both values are immutable kinds, compare by value
3. otherwise return `#f`

Effective value-based `eq?` kinds:

- nil
- booleans
- numbers
- symbols
- strings
- `#inert`
- `#ignore`
- prelude entries
- native functions

Identity-only `eq?` kinds:

- cons cells
- environments
- operatives
- applicatives
- builtin values that are not the same object

This means `eq?` on strings is value-based, not identity-only.

#### `equal?`

```lisp
(equal? a b)
```

Algorithm:

1. if `eq?` is true, return `#t`
2. if both are cons cells, compare car and cdr recursively
3. if both are strings, compare character-by-character
4. if both are environments and not `eq?`, return `#f`
5. otherwise return `#f`

Consequences:

- `equal?` is structural for lists and strings
- `equal?` is identity-based for environments
- `equal?` is not a generic "deep compare every type" predicate

### 9.6 `eval`

Syntax:

```lisp
(eval expr [env])
```

Behavior:

- evaluate `expr` in `env`
- if `env` is omitted, use `GLOBAL_ENV`
- extra operands are ignored

Important detail:

- `expr` is already an evaluated argument because `eval` is applicative
- to evaluate source-like syntax, callers normally quote or construct the
  expression object explicitly

### 9.7 `wrap`

Syntax:

```lisp
(wrap combiner)
```

Behavior:

- create an applicative wrapper around the first evaluated argument
- extra operands are ignored
- the wrapped object is not validated eagerly for callability

This means `(wrap 42)` succeeds but calling the result later raises
`NotCallable`.

### 9.8 `unwrap`

Syntax:

```lisp
(unwrap applicative)
```

Behavior:

- extract the wrapped value from the first evaluated argument
- non-applicatives raise `TypeError`
- extra operands are ignored

### 9.9 Environment Constructors

#### `make-environment`

Syntax:

```lisp
(make-environment env ...)
```

Behavior:

- every supplied argument must already be an environment
- copy the parent list into fresh cons cells
- create a new environment with that copied parent list
- zero arguments are allowed and produce a parentless environment

#### `make-empty-environment`

Syntax:

```lisp
(make-empty-environment)
```

Behavior:

- create a fresh parentless environment
- reject any supplied operands with `ArityError`

### 9.10 `gc-collect`

Syntax:

```lisp
(gc-collect)
```

Behavior:

- trigger garbage collection unconditionally
- return the number of reclaimed objects as a number
- ignore extra operands

### 9.11 `error`

Syntax:

```lisp
(error msg)
```

Behavior:

- always raises `InvalidArgument`
- ignores the supplied message value
- ignores extra operands

### 9.12 `apply`

Syntax:

```lisp
(apply combiner arg-list [env])
```

Behavior:

- `combiner` and `arg-list` are first evaluated because `apply` is applicative
- if `env` is omitted, use `GLOBAL_ENV`
- then invoke `combiner` on `arg-list`

Crucial semantic detail:

- `arg-list` is passed exactly as supplied
- `apply` does not evaluate elements of `arg-list`
- applicatives receiving `arg-list` therefore treat it as an already evaluated
  argument list
- operatives receiving `arg-list` treat it as a raw operand object

Consequences:

- `(apply + (list 1 2 3))` works
- `(apply if (list #t 'x 0) env)` works because `if` is operative and receives
  raw operands plus the explicit caller environment
- `(apply + (list (+ 1 2)))` does not re-evaluate `(+ 1 2)`; `+` sees a cons
  cell where it expected a number

### 9.13 Raw String Helpers

#### `raw-read-string`

Syntax:

```lisp
(raw-read-string stringish)
```

Behavior:

- if the first argument is `NIL`, return `NIL`
- otherwise require the first argument to be a runtime string chain
- otherwise parse exactly one complete expression from the runtime string chain
- if the runtime string is empty, return `NIL`
- if trailing unread input remains, raise `ParseError`

#### `raw-display-to-string`

Syntax:

```lisp
(raw-display-to-string obj)
```

Behavior:

- format the first argument using display semantics
- return the result as a runtime string
- extra operands are ignored

#### `raw-write-to-string`

Syntax:

```lisp
(raw-write-to-string obj)
```

Behavior:

- format the first argument using write semantics
- return the result as a runtime string
- extra operands are ignored

## 10. Formatting Semantics

Grift has two formatting modes exposed through raw string helpers and host API
formatters:

- write mode
- display mode

### 10.1 Common Renderings

Common renderings in both modes:

- nil => `()`
- `#t` => `#t`
- `#f` => `#f`
- numbers => decimal form
- symbols => unquoted symbol name
- cons cells => list or dotted-pair notation
- `#inert` => `#inert`
- `#ignore` => `#ignore`
- prelude value => `<prelude:name>`
- other opaque callable/runtime types => angle-bracket placeholder such as
  `<operative>`, `<applicative>`, `<builtin>`, `<environment>`, `<native>`

### 10.2 Write Mode for Strings

Write mode wraps non-empty strings in quotes and escapes only:

- `"`
- `\`
- newline
- tab
- carriage return

The empty string still renders as `()`, not `""`, because empty string and nil
are the same runtime value.

### 10.3 Display Mode for Strings

Display mode prints non-empty strings without quotes or escapes.

Again, the empty string prints as `()`.

### 10.4 List Formatting

Pairs print as:

- proper list => `(a b c)`
- improper list => `(a b . c)`

Strings are not printed using list notation except for the empty string alias.

## 11. Prelude

The bundled prelude currently exports these global applicative bindings:

- `map`
- `filter`
- `length`
- `append`

Their source definitions are:

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

Implementation detail that is still language-visible:

- each prelude function is stored as source and parsed on demand when invoked
- invocation happens in `GLOBAL_ENV`
- the parsed result is expected to evaluate to an applicative wrapping an
  operative

There are currently no additional prelude constants defined in
`prelude.grift`.

## 12. Error Surface

The implementation exposes at least these relevant language-visible errors:

- `ParseError`
- `ArityError`
- `InvalidArgument`
- `TypeError`
- `UnboundVariable`
- `AlreadyDefined`
- `ArithmeticOverflow`
- `DivisionByZero`
- `NotCallable`
- `Cyclic`
- `OutOfMemory`

This spec describes which operations raise which error when that behavior is
stable and directly observable in the current implementation.

When the implementation obtains an error indirectly from list access or type
projection, the practical result is usually `TypeError`.

## 13. Known Quirks and Inconsistencies

These are current implementation facts that a compatible reimplementation
should preserve unless the language is intentionally changed.

### 13.1 Empty String Is Nil

The empty string and nil are the same runtime object. This affects:

- equality
- predicates
- formatting
- `raw-read-string ""`
- `car`/`cdr` behavior on strings

### 13.2 Strings Are Pair-Like, But Not for Ptree Matching

Strings count as pairs for:

- `pair?`
- `car`
- `cdr`

But they do not count as pairs for formal-parameter-tree matching.

### 13.3 `eq?` on Strings Is Value-Based

Even though cons cells and environments are identity-based for `eq?`, strings
compare by content under `eq?`.

This is implemented, tested, and should be considered normative.

### 13.4 External Reader vs `raw-read-string`

External source parsing is byte-oriented, while `raw-read-string` parses
character chains. Non-ASCII text is therefore not handled identically by the
two entry points.

### 13.5 `error` Ignores Its Message

`(error msg)` does not propagate `msg` into an error payload. It simply raises
`InvalidArgument`.

## 14. Reimplementation Checklist

A C or other-language reimplementation should preserve at least these concrete
properties:

1. Empty top-level input yields `#inert`, while empty `raw-read-string` yields
   `()`.
2. Empty string is the same runtime object as nil.
3. Only booleans are accepted in boolean contexts.
4. Symbols are interned and symbol equality is stable.
5. Strings are linked character chains with pair-like `car`/`cdr` behavior.
6. `define!` destructures via formal parameter trees.
7. `set!` requires an explicit environment and mutates only that frame.
8. Multi-parent environment lookup is left-to-right depth-first with cycle
   detection.
9. Applicatives evaluate operands exactly once, left to right.
10. `apply` receives an already constructed argument list and does not walk it
    to evaluate elements.
11. Prelude functions `map`, `filter`, `length`, and `append` are present.
12. Formatting of strings, especially the empty string, matches current write
    and display behavior.

If this file and the implementation disagree, the code and tests in
`crates/grift/src` and `crates/grift/tests/lisp_tests.rs` remain the final
conformance source.
