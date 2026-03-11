# Grift Language Specification

This is the implementation-compatibility specification for Grift as currently
implemented. It is written so that the language can be reimplemented in another
host language, including C, without having to infer behavior from Rust source
during the reimplementation.

This is not an aspirational language design. If a behavior here is surprising,
that means the implementation is surprising and a compatible reimplementation
should preserve it.

Primary conformance sources:

- `crates/grift/src`
- `crates/grift/prelude.grift`
- `crates/grift/tests/lisp_tests.rs`

## 1. Program Model

### 1.1 Interpreter State

One interpreter instance has:

- one arena of runtime objects
- one persistent global environment
- one symbol intern table
- one temporary GC-root stack

Top-level definitions persist across successive `eval` calls on the same
interpreter instance.

### 1.2 Input Unit

A program is zero or more expressions read from a source stream.

Top-level evaluation algorithm:

1. parse the next expression
2. evaluate it immediately in `GLOBAL_ENV`
3. continue until input is exhausted
4. return the value of the last expression

If the input contains no expressions, the result is `#inert`.

Example:

```lisp
1 2 3
```

evaluates to `3`.

### 1.3 Empty Program vs Empty Runtime String

These are distinct:

- empty top-level input => `#inert`
- `(raw-read-string "")` => `()`

## 2. Reader Specification

### 2.1 Two Reader Modes

Grift has two parsing entry points:

- external source text, used by ordinary `eval`
- runtime strings, used by `raw-read-string`

They deliberately do not process input the same way for non-ASCII data.

### 2.2 External Source

The external reader is byte-oriented.

Consequences:

- tokenization operates on bytes cast to `char`
- line and column positions count byte steps, not Unicode scalar values
- non-ASCII input is not guaranteed to behave like a Unicode-aware Lisp reader

### 2.3 Runtime String Reader

`raw-read-string` parses an existing runtime string, which is a linked chain of
Rust `char` values. That path is character-oriented.

### 2.4 Whitespace and Comments

Whitespace characters are:

- space
- tab
- carriage return
- newline

Line comments start with `;` and continue through the next newline or
end-of-input.

There are no block comments.

### 2.5 Syntax Recognized by the Reader

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
- character literals as a separate type
- block comments

### 2.6 Grammar Summary

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

This is only an approximation because `.` is special in exactly one list
position.

### 2.7 Atom Delimiters

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
- `.` is only special when the list parser has already parsed the first list
  element and is checking for dotted-pair syntax

### 2.8 Expression Parsing Algorithm

To parse one expression:

1. skip whitespace and comments
2. if input is exhausted:
   - optional parse returns no expression
   - required parse raises `ParseError`
3. consume one character
4. dispatch:
   - `(` => parse list
   - `'` => parse quote shorthand
   - `"` => parse string
   - `)` => `ParseError`
   - anything else => parse atom

### 2.9 Lists and Dotted Pairs

After reading `(`:

1. skip whitespace/comments
2. if next char is `)`, return `NIL`
3. if next char is `.`:
   - consume it
   - if the following char is a delimiter or end-of-input, raise `ParseError`
   - otherwise treat the dot as part of an atom and continue parsing a normal
     first list element
4. otherwise parse the first element
5. skip whitespace/comments
6. if next char is `.`:
   - consume it
   - if the following char is a delimiter or end-of-input, parse a dotted pair
   - otherwise treat the dot as the first character of another atom
7. otherwise continue parsing the rest of the proper list

Examples:

- `(a b c)` => proper list
- `(a . b)` => dotted pair
- `(a .foo)` => list of `a` and `.foo`
- `(.foo bar)` => list whose first element is `.foo`
- `(. x)` => parse error
- `(a . b c)` => parse error

### 2.10 Quote Shorthand

`'expr` is rewritten to `(quote expr)`.

Examples:

- `'x` => `(quote x)`
- `'(1 2)` => `(quote (1 2))`
- `'` => `ParseError`

### 2.11 Atom Classification

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
- an overflowing integer literal is not a parse error; it becomes a symbol

### 2.12 Strings

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

### 2.13 Reader Errors

User-visible reader errors are:

- `ParseError { line, col }`
- `InvalidArgument` for unrecognized string escapes

Examples of `ParseError`:

- unexpected `)`
- unterminated list
- unterminated string
- malformed dotted-pair syntax
- bare `'` at end of input
- trailing non-whitespace/comment data after `raw-read-string` parses one
  expression

### 2.14 Source Coordinates

External parsing reports 1-based line and column positions.

`raw-read-string` parsing reports `(0, 0)` because runtime-string parsing does
not track coordinates.

## 3. Runtime Values

Grift exposes the following first-class runtime categories:

| Category | Write/display form |
| --- | --- |
| nil | `()` |
| booleans | `#t`, `#f` |
| numbers | decimal integer |
| symbols | symbol name |
| cons pairs | list or dotted-pair syntax |
| strings | quoted in write mode, raw in display mode |
| compound operatives | `<operative>` |
| applicatives | `<applicative>` |
| builtin combiner cores | `<builtin>` |
| environments | `<environment>` |
| inert | `#inert` |
| ignore | `#ignore` |
| prelude entries | `<prelude:name>` |
| native host functions | `<native>` |

Only some categories have reader syntax.

### 3.1 Nil

`NIL` is:

- the empty list
- the empty string

This alias is mandatory and observable:

- `()` and `""` are different source forms
- both evaluate to the same runtime object
- `(null? "")` is true
- `(pair? "")` is false
- `(equal? "" ())` is true
- write/display formatting of `""` yields `()`

### 3.2 Booleans

Only actual booleans participate in boolean contexts.

There is no general truthiness rule.

### 3.3 Numbers

Numbers are signed machine integers equivalent to Rust `isize`.

Behavior:

- literals parse only if they fit `isize`
- arithmetic uses checked operations
- overflow raises `ArithmeticOverflow`
- division truncates toward zero

### 3.4 Symbols

Symbols are interned by name.

Consequences:

- symbols with the same spelling are the same runtime object
- environment lookup compares symbol identity
- `eq?` on symbols is value-based through interning

### 3.5 Cons Pairs

Cons cells are immutable pairs.

Behavior:

- proper lists are cons chains ending in `NIL`
- improper lists are cons chains whose final cdr is not `NIL`
- there is no `set-car!`
- there is no `set-cdr!`

### 3.6 Strings

Strings are linked `CharPair` chains:

```text
CharPair(ch0, CharPair(ch1, ... CharPair(chn, NIL)))
```

Important properties:

- empty string is exactly `NIL`
- there is no separate character type
- a one-character string is one `CharPair` with `cdr = NIL`
- well-formed strings are `CharPair` chains terminated by `NIL`

Observable consequences:

- `(car "hello")` returns `"h"` as a fresh one-character string
- `(cdr "hello")` returns `"ello"`
- `(cdr "x")` returns `()`
- `(pair? "hello")` is true
- `(pair? "")` is false

### 3.7 Callables

Grift has five callable storage forms:

1. builtin combiner core
2. compound operative created by `vau`
3. applicative wrapper
4. prelude entry
5. host-native function

Call behavior depends on the outer storage form:

- builtin combiner core => raw operands for builtin operatives, evaluated args
  for builtin applicatives
- compound operative => raw operand object plus caller environment
- applicative => evaluate operands first, then invoke wrapped value
- prelude entry => normally reached through an applicative wrapper
- native host function => normally reached through an applicative wrapper

### 3.8 Environments

Environments are first-class runtime values containing:

- `bindings`: an alist of `(key . value)` pairs
- `parents`: a proper list of parent environments

Ordinary evaluation only looks up symbols, but the representation itself does
not enforce symbol-only keys.

### 3.9 Inert and Ignore

`#inert` is the singleton result of side-effect-oriented forms.

`#ignore` is the singleton wildcard used in formal parameter trees.

Both are self-evaluating.

### 3.10 Prelude and Native Values

Prelude functions and host-native functions are distinct runtime categories and
may appear as first-class values.

## 4. Environments

### 4.1 Predefined Environments

The implementation pre-creates:

- `GROUND_ENV`: builtin-only environment
- `GLOBAL_ENV`: child of `GROUND_ENV`

Builtins live in `GROUND_ENV`.

Prelude bindings, host-native registrations, and ordinary top-level user
definitions live in `GLOBAL_ENV`.

### 4.2 Binding Order

Environment bindings are stored newest-first.

Successful `define!` prepends a binding to the current frame.

### 4.3 Lookup Algorithm

Lookup is normative.

Single-parent path:

1. search the current frame from newest to oldest
2. if found, return the value
3. if there is exactly one parent, continue with that parent
4. if there are no parents, raise `UnboundVariable`

Multi-parent path:

1. search the current frame
2. search parents in left-to-right depth-first order
3. maintain a visited set of environments
4. skip already visited environments
5. return the first match found
6. otherwise raise `UnboundVariable`

Consequences:

- lookup is depth-first, not breadth-first
- parent order matters
- earlier parents can shadow later direct parents through their descendants
- cycles are tolerated during lookup

### 4.4 `define!`

`define!` mutates only the current frame.

Behavior:

- duplicate binding in the same frame => `AlreadyDefined`
- parent bindings do not matter for duplicate detection
- successful definition prepends a new local binding

### 4.5 `set!`

`set!` mutates only the explicitly supplied target environment.

Behavior:

- only the target frame is searched
- parent environments are not searched
- no new binding is created
- missing target binding => `UnboundVariable`

### 4.6 Environment Construction

`make-environment` creates a fresh frame with zero or more parents.

`make-empty-environment` creates a fresh frame with no parents.

New environments do not copy parent bindings. They inherit only through parent
links.

Consequences:

- later parent mutation is visible in children
- child-local definitions do not write back into parents

## 5. Evaluation Semantics

### 5.1 Self-Evaluating Values

These evaluate to themselves:

- nil
- booleans
- numbers
- strings
- builtin combiner cores
- compound operatives
- applicatives
- environments
- `#inert`
- `#ignore`
- prelude entries
- native functions

Symbols are not self-evaluating.

Cons pairs are not self-evaluating.

### 5.2 Boolean Contexts

These require actual booleans and raise `TypeError` otherwise:

- `if`
- `cond` tests other than `else`
- `and`
- `or`
- `not`

There is no Scheme-style truthiness.

### 5.3 Symbol Evaluation

Evaluating a symbol performs environment lookup in the current environment.

### 5.4 Combination Evaluation

To evaluate `(f arg1 arg2 ...)`:

1. evaluate `f`
2. dispatch on the resulting runtime value

Dispatch rules:

- builtin combiner core => call builtin operative path on raw operands
- compound operative => invoke with raw operand object
- applicative => evaluate operands left-to-right exactly once, then invoke the
  wrapped value
- anything else => `NotCallable`

### 5.5 Applicative Evaluation Order

Applicatives evaluate arguments:

- strictly
- left to right
- exactly once

### 5.6 Compound Operative Invocation

Invoking a `vau`-created operative does this:

1. create a child environment whose single parent is the closure's defining
   environment
2. match the formal parameter tree against the raw operand object
3. if an environment parameter exists, bind it to the caller environment
4. evaluate the body in the new environment

### 5.7 Closure Capture

Closures capture environment identity, not snapshots.

Consequences:

- closures observe later `define!` and `set!` on the captured environment
- `(define! f (lambda ...))` supports recursion
- sequential top-level definitions can support mutual recursion
- named `let` recursion works for the same reason

### 5.8 Tail Positions

A compatible implementation should preserve tail behavior for:

- user operatives
- `if`
- `begin`
- `cond`
- `and`
- `or`
- `let`

## 6. Formal Parameter Trees

Formal parameter trees are used by:

- `define!`
- `lambda`
- `vau`
- `fn!` indirectly
- compound operative invocation

### 6.1 Valid Shapes

A valid formal parameter tree is:

- a symbol
- `#ignore`
- `NIL`
- a cons pair whose car and cdr are both valid parameter trees

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
- `(x x)`
- cyclic structures

### 6.2 Matching Semantics

Matching `ptree` against `obj`:

- if `ptree` is `NIL`, `obj` must be `NIL`
- if `ptree` is `#ignore`, do nothing
- if `ptree` is a symbol, bind it to `obj`
- if `ptree` is a cons pair, `obj` must be a `Cons`, then match car and cdr
  recursively

Important limitation:

- pair-shaped matching requires `obj` to be a `Cons`
- a `CharPair` string node does not count as a pair for parameter matching

### 6.3 Validation Rules

Validation is eager.

Rejected cases:

- invalid leaf type => `TypeError`
- duplicate symbol => `InvalidArgument`
- cyclic tree => `Cyclic`

`#ignore` may appear multiple times because it is not a symbol binding.

### 6.4 `vau` Environment Parameter

For `vau`, `env-param` must be:

- a symbol, or
- `#ignore`

Behavior:

- `#ignore` means no caller-environment binding is created
- internally that case is normalized to “no environment parameter”
- duplication between `env-param` and a symbol in `params` raises
  `InvalidArgument`

## 7. Builtin Operatives

Builtin operatives receive raw operands and the caller environment.

Unless noted otherwise:

- extra operands are ignored
- missing operands typically surface as `TypeError` through list access

### 7.1 `quote`

```lisp
(quote expr)
```

Returns the first operand unchanged.

### 7.2 `if`

```lisp
(if test then [else])
```

Behavior:

1. evaluate `test`
2. require a boolean
3. if true, evaluate `then`
4. if false:
   - evaluate `else` if present
   - otherwise return `NIL`

Only the selected branch is evaluated.

### 7.3 `define!`

```lisp
(define! definiend expression)
```

Behavior:

1. validate `definiend` as a formal parameter tree
2. evaluate `expression` in the current environment
3. match the result against `definiend` in the current frame
4. return `#inert`

Important points:

- this is destructuring definition
- `(define! (f x) value)` destructures a value; it does not define a function
- same-frame redefinition raises `AlreadyDefined`

### 7.4 `fn!`

```lisp
(fn! name params body...)
```

Behavior:

- equivalent to `(define! name (lambda params (begin body...)))`
- `name` must be a symbol
- `params` are validated via `lambda`
- zero body expressions are allowed; such a function returns `NIL`
- same-frame redefinition raises `AlreadyDefined`
- returns `#inert`

### 7.5 `set!`

```lisp
(set! env-expr symbol value-expr)
```

Behavior:

1. evaluate `env-expr`
2. require that result to be an environment
3. require `symbol` to be a symbol literal
4. evaluate `value-expr` in the current dynamic environment
5. mutate the existing local binding of `symbol` in the target environment
6. return `#inert`

Differences from Scheme:

- no `(set! x value)` shorthand
- explicit target environment
- no parent walk for mutation
- no binding creation

### 7.6 `lambda`

```lisp
(lambda params body...)
```

Behavior:

- validate `params`
- capture the current environment
- wrap an underlying operative in an applicative
- zero body expressions are allowed and yield `NIL`

### 7.7 `begin`

```lisp
(begin expr...)
```

Behavior:

- evaluate operands left-to-right
- return the last result
- `(begin)` returns `NIL`

### 7.8 `cond`

```lisp
(cond (test expr...) ...)
```

Behavior:

- inspect clauses left-to-right
- first clause element is the test
- a symbol named `else` matches immediately
- otherwise evaluate the test and require a boolean
- matching clause body is evaluated as if wrapped in `begin`
- matching clause with no body returns `NIL`
- if no clause matches, return `NIL`

`else` is recognized by symbol name only and does not need to be last.

### 7.9 `and`

```lisp
(and expr...)
```

Behavior:

- zero operands => `#t`
- evaluate operands left-to-right
- every evaluated operand must be boolean
- stop at first `#f`
- return `#f` if any operand is false
- return `#t` if all operands are true

Unlike Scheme, `and` never returns an arbitrary non-boolean truthy value.

### 7.10 `or`

```lisp
(or expr...)
```

Behavior:

- zero operands => `#f`
- evaluate operands left-to-right
- every evaluated operand must be boolean
- stop at first `#t`
- return `#t` if any operand is true
- return `#f` if all operands are false

Unlike Scheme, `or` never returns an arbitrary non-boolean truthy value.

### 7.11 `let`

Regular form:

```lisp
(let ((name init) ...) body...)
```

Behavior:

1. create a child environment of the current environment
2. require each binding to be exactly `(symbol init)`
3. evaluate each init in the outer environment, not the child
4. define each symbol in the child
5. evaluate the body in the child

This is simultaneous binding, not `let*`.

Named form:

```lisp
(let name ((param init) ...) body...)
```

Behavior:

1. evaluate all init expressions in the outer environment
2. create a child environment
3. define `name` in that child as a recursive function
4. invoke that function using the already evaluated init values

Important detail:

- named-let init values are not re-evaluated during helper invocation
- zero body expressions are allowed and yield `NIL`

### 7.12 `vau`

```lisp
(vau params env-param body...)
```

Behavior:

- validate `params`
- require `env-param` to be a symbol or `#ignore`
- reject overlap between `params` symbols and `env-param`
- capture the current environment
- if multiple body expressions are present, wrap them in `begin`
- zero body expressions yield `NIL`
- return a compound operative

### 7.13 `current-environment`

```lisp
(current-environment)
```

Returns the caller's current environment and ignores any operands.

## 8. Builtin Applicatives

Builtin applicatives receive already evaluated arguments.

Unless noted otherwise:

- extra operands are ignored
- missing operands usually surface as `TypeError`

### 8.1 `cons`

```lisp
(cons a b)
```

Normal behavior:

- return `(a . b)`

Special string behavior:

- if `a` is a one-character string
- and `b` is a well-formed string chain
- return a new `CharPair` node instead of a `Cons`

Consequences:

- `(cons (car "h") "ello")` => `"hello"`
- `(cons (car "h") ())` => `"h"`
- `(cons (car "a") 42)` => `TypeError`

### 8.2 Arithmetic

`(+ ...)`

- zero args => `0`
- variadic checked addition

`(- a b ...)`

- zero args => `ArityError`
- one arg => checked negation
- more args => left-fold checked subtraction

`(* ...)`

- zero args => `1`
- variadic checked multiplication

`(/ a b)`

- divides first argument by second
- truncates toward zero
- division by zero => `DivisionByZero`
- extra operands are ignored

### 8.3 Numeric Comparisons

`=` `<` `>` `<=` `>=`

Behavior:

- use the first two arguments
- require numbers
- ignore extras
- return a boolean

These are not chain comparisons.

### 8.4 `car` and `cdr`

`car`:

- on `Cons`, return the cons cell's car
- on non-empty strings, return a fresh one-character string
- otherwise `TypeError`

`cdr`:

- on `Cons`, return the cons cell's cdr
- on non-empty strings, return the tail string
- otherwise `TypeError`

### 8.5 `list`

```lisp
(list ...)
```

Returns the evaluated argument list exactly as received. It does not copy the
list.

### 8.6 Type Predicates

These predicates are variadic and return true iff every argument matches:

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

- all of the above return `#t`

Membership rules:

- `null?` => only `NIL`
- `pair?` => `Cons` and non-empty `CharPair` strings
- `operative?` => builtin combiner cores and compound operatives
- `applicative?` => `Applicative` only
- `environment?` => `Environment` only

Notably:

- prelude entries are not `operative?`
- native host functions are not `operative?`
- wrapped operatives are `applicative?`

### 8.7 `not`

```lisp
(not boolean)
```

Reads only the first argument, requires a boolean, and returns its negation.

### 8.8 `eq?`

`eq?` is defined as:

1. if the two objects are the same runtime object, return true
2. if both objects are strings, compare string content
3. otherwise, if both objects are immutable value-like objects, compare their
   stored value
4. otherwise return false

Immutable value-like categories for `eq?` are:

- nil
- booleans
- numbers
- symbols
- strings
- `#inert`
- `#ignore`
- prelude entries
- native host functions

Identity-based categories for `eq?` are:

- cons pairs
- environments
- compound operatives
- applicatives
- builtin combiner cores

### 8.9 `equal?`

`equal?` extends `eq?`:

1. if `eq?` is true, return true
2. if both values are cons cells, compare car and cdr recursively
3. if both values are strings, compare string content
4. if both values are environments and not `eq?`, return false
5. otherwise return false

Consequences:

- cons cells compare structurally
- strings compare by content
- environments are identity-only

### 8.10 `eval`

```lisp
(eval expr [env])
```

Behavior:

- no explicit environment => evaluate in `GLOBAL_ENV`
- explicit environment => must be an environment
- `expr` is evaluated as the existing runtime object; it is not reparsed

### 8.11 `wrap`

```lisp
(wrap combiner)
```

Returns an applicative wrapping the first argument.

It does not validate callability. Wrapping a non-callable succeeds and only
fails later when the resulting applicative is invoked.

### 8.12 `unwrap`

```lisp
(unwrap applicative)
```

Returns the immediate wrapped value.

Non-applicatives raise `TypeError`.

### 8.13 `make-environment`

```lisp
(make-environment env ...)
```

Behavior:

- every argument must be an environment
- returns a fresh environment whose parent list is a copy of the argument list
- the new environment starts with no local bindings

The copy prevents later list-object mutation from changing the parent list, but
it does not clone parent environments.

### 8.14 `make-empty-environment`

```lisp
(make-empty-environment)
```

Creates a fresh environment with no parents.

Any argument at all => `ArityError`.

### 8.15 `gc-collect`

```lisp
(gc-collect)
```

Forces a collection cycle, ignores operands, and returns the number of
collected objects as a number.

### 8.16 `error`

```lisp
(error msg)
```

Always raises `InvalidArgument`. The message is currently ignored.

### 8.17 `apply`

```lisp
(apply combiner arg-list [env])
```

Behavior:

- no explicit environment => use `GLOBAL_ENV`
- explicit environment => must be an environment
- `arg-list` is passed directly as the operand object
- `apply` does not evaluate the elements of `arg-list`

This is critical:

- if `combiner` is operative, `arg-list` is the raw operand object
- if `combiner` is applicative, `arg-list` is treated as already evaluated
  arguments

So `apply` is not Scheme-style “evaluate the list elements for me”.

### 8.18 `raw-read-string`

```lisp
(raw-read-string string)
```

Behavior:

- argument must be a well-formed runtime string
- empty string => `NIL`
- parse zero or one expression
- reject trailing non-whitespace/comment input
- if one expression is parsed, return that runtime object directly

### 8.19 `raw-display-to-string`

```lisp
(raw-display-to-string object)
```

Behavior:

- validate reachable strings and symbol names
- format using display semantics
- return a runtime string

### 8.20 `raw-write-to-string`

```lisp
(raw-write-to-string object)
```

Behavior:

- validate reachable strings and symbol names
- format using write semantics
- return a runtime string

## 9. Formatting Semantics

Grift has two formatting modes:

- write mode
- display mode

### 9.1 Common Rules

Formatting produces:

- nil => `()`
- booleans => `#t`, `#f`
- numbers => decimal text
- symbols => raw symbol name
- proper lists => list notation
- improper lists => dotted notation
- `#inert`
- `#ignore`
- prelude => `<prelude:name>`
- opaque runtime values => placeholder forms

Opaque placeholder forms:

- compound operative => `<operative>`
- applicative => `<applicative>`
- builtin combiner core => `<builtin>`
- environment => `<environment>`
- native function => `<native>`

### 9.2 Write Mode

Write mode prints strings with quotes and escapes.

Escapes are exactly:

- `"` => `\"`
- `\` => `\\`
- newline => `\n`
- tab => `\t`
- carriage return => `\r`

### 9.3 Display Mode

Display mode prints strings without surrounding quotes or escapes.

### 9.4 Empty String Formatting

Because empty string and nil are the same runtime object:

- write mode prints `""` as `()`
- display mode also prints it as `()`

### 9.5 Formatting Validation

Formatting validates that:

- symbols point to well-formed string chains
- strings are well-formed string chains
- recursively reachable list content is formattable

Malformed reachable string structure causes formatting failure in host APIs and
`TypeError` in raw string helpers.

## 10. Prelude

The bundled prelude defines these functions in `GLOBAL_ENV`:

### 10.1 `map`

```lisp
(fn! map (f lst)
  (if (null? lst) ()
    (cons (f (car lst)) (map f (cdr lst)))))
```

### 10.2 `filter`

```lisp
(fn! filter (pred lst)
  (if (null? lst) ()
    (if (pred (car lst))
      (cons (car lst) (filter pred (cdr lst)))
      (filter pred (cdr lst)))))
```

### 10.3 `length`

```lisp
(fn! length (lst)
  (if (null? lst) 0 (+ 1 (length (cdr lst)))))
```

### 10.4 `append`

```lisp
(fn! append (a b)
  (if (null? a) b
    (cons (car a) (append (cdr a) b))))
```

### 10.5 Invocation Model

Prelude bindings are not cached closures.

For every call:

1. retrieve the stored static lambda source text
2. parse it again
3. evaluate it again in `GLOBAL_ENV`
4. unwrap the resulting applicative to the inner operative
5. invoke that operative

Prelude calls therefore reparse and reevaluate on every invocation.

## 11. Errors

Observable error variants:

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
| `InvalidArgument` | bad string escape, duplicate parameter symbol, `(error ...)` |
| `ArityError` | `(-)`, `(make-empty-environment 1)` |
| `Cyclic` | cyclic formal parameter tree |
| `TypeError` | wrong value kind, non-boolean boolean context, malformed string chain |
| `ArithmeticOverflow` | checked integer overflow |
| `DivisionByZero` | division by zero |
| `UnboundVariable` | missing symbol lookup or `set!` target |
| `NotCallable` | applying a non-combiner |
| `AlreadyDefined` | same-frame redefinition |

## 12. Observable Quirks

These behaviors are surprising but normative:

- empty string is the same runtime object as nil
- strings count as pairs for `pair?`, `car`, and `cdr`, but not for formal
  parameter tree matching
- `eq?` on strings is content-based
- external source parsing is byte-oriented, while `raw-read-string` is
  character-oriented
- `error` ignores its message
- `wrap` accepts non-callables
- `apply` does not evaluate the supplied argument list
- prelude calls reparse their source every time

## 13. Reimplementation Checklist

A compatible implementation should preserve at least these properties:

1. empty top-level input yields `#inert`
2. empty `raw-read-string` yields `()`
3. empty string is the same runtime object as nil
4. only booleans are accepted in boolean contexts
5. symbols are interned
6. strings are linked character chains with pair-like `car`/`cdr`
7. `define!` destructures via formal parameter trees
8. `set!` requires an explicit environment and mutates only that frame
9. multi-parent environment lookup is left-to-right depth-first
10. applicatives evaluate operands exactly once, left-to-right
11. `apply` does not evaluate the supplied argument list
12. `eval` defaults to the global environment
13. regular `let` evaluates initializers in the outer environment
14. named `let` does not re-evaluate initializer values during invocation
15. closures capture live environment objects
16. string-building `cons` requires a well-formed string tail
17. `eq?` is value-based for strings and immutable scalars, identity-based for
    pairs and environments
18. `equal?` is structural for cons cells and strings, but not for distinct
    environments
19. write/display formatting of the empty string matches nil formatting
20. prelude functions `map`, `filter`, `length`, and `append` exist
21. prelude calls reparse and reevaluate on every invocation

If this document and the implementation disagree, the code in `crates/grift/src`
and the behavior covered by `crates/grift/tests/lisp_tests.rs` are authoritative.
