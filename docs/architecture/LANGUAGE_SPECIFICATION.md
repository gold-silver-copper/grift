# Grift Language Specification

This document specifies the currently implemented Grift language as it exists
in `crates/grift/src` and `crates/grift/tests/lisp_tests.rs`.

It is intentionally implementation-facing: the goal is to make a compatible
reimplementation possible in another language such as C, not to describe an
idealized future language.

## 1. Scope and Compatibility Target

A conforming implementation must match the observable behavior of:

- the reader
- the evaluator
- environment lookup and mutation
- the built-in operatives and applicatives
- equality
- string/list behavior
- the printed forms produced by `raw-display-to-string` and `raw-write-to-string`

This spec covers the core language and prelude. It does not attempt to
standardize Rust host APIs such as `register_native`, except where host-native
functions are visible as first-class runtime values.

## 2. Program Model

A program is zero or more expressions read from a character stream.

- Expressions are parsed and evaluated sequentially.
- The result of the last expression is the result of the program.
- An empty program evaluates to `#inert`.
- Top-level evaluation happens in the global environment.
- The global environment persists across evaluations performed on the same
  interpreter instance.

## 3. Lexical Syntax and Reader

### 3.1 Whitespace and Comments

The reader treats these characters as whitespace:

- space
- tab
- carriage return
- newline

Line comments begin with `;` and continue to the next newline or end of input.

### 3.2 Expression Forms

The reader recognizes:

- lists: `(a b c)`
- dotted pairs: `(a . b)`
- quote shorthand: `'x`
- string literals: `"hello"`
- atoms: booleans, `#inert`, `#ignore`, integers, and symbols

There is no quasiquote, unquote, vector syntax, byte string syntax, or
character literal syntax.

### 3.3 Reader Algorithm

The reader is recursive-descent and behaves as follows:

1. Skip whitespace and comments.
2. Read one character.
3. Dispatch:
   - `(` starts list parsing
   - `'` parses the following expression and rewrites it as `(quote expr)`
   - `"` starts string parsing
   - `)` is a parse error
   - anything else starts atom parsing

### 3.4 Lists and Dotted Pairs

After reading `(`:

1. Skip whitespace/comments.
2. If the next character is `)`, the result is `NIL`.
3. Otherwise parse the first element.
4. Skip whitespace/comments.
5. If the next character is `.` and that `.` is followed by a delimiter or end
   of input, parse one more expression as the cdr, require a closing `)`, and
   return a dotted pair.
6. Otherwise continue parsing the remaining list elements recursively.

Important dot rules:

- A lone `.` in list-structure position is special.
- A token beginning with `.` is a normal symbol if the `.` is not followed by a
  delimiter immediately after it is read in dot position.
- `( . x)` is a parse error.
- `(a . b)` is a dotted pair.
- `(a .foo)` is parsed as `(a .foo)`, not as a dotted pair.

### 3.5 Delimiters

The reader stops an atom when it encounters one of:

- space
- tab
- carriage return
- newline
- `(`
- `)`
- `"`
- `;`

Notably, `.` is not a general delimiter. It is only special in the list parser
as described above.

### 3.6 Atoms

An atom token is classified in this order:

1. `#t` or `#true` => boolean true
2. `#f` or `#false` => boolean false
3. `#inert` => inert singleton
4. `#ignore` => ignore singleton
5. a signed base-10 integer that fits in the host signed machine integer type
6. otherwise, a symbol

Implications:

- `+42` and `-42` are numeric literals.
- `+` and `-` by themselves are symbols.
- An integer literal that overflows the host integer type is not an error; it is
  read as a symbol.

### 3.7 Strings

String literals are delimited by `"`.

Recognized escape sequences are exactly:

- `\n`
- `\t`
- `\r`
- `\\`
- `\"`

Any other backslash escape raises `InvalidArgument`, not `ParseError`.

Source-level strings are parsed character by character until the closing `"`.
An unterminated string raises `ParseError`.

### 3.8 Source Character Set

The current reader is byte-oriented rather than full UTF-8 decoding. Portable
source programs should therefore be treated as ASCII source text plus the escape
sequences above.

Runtime strings conceptually store characters, but only ASCII source behavior is
well-defined by the current implementation.

### 3.9 Parse Errors

Malformed syntax raises `ParseError { line, col }`, using 1-based coordinates.

Top-level parsing reports real source coordinates. Parsing through
`raw-read-string` has no line/column tracking and therefore reports `(0, 0)` for
reader-generated parse errors.

## 4. Runtime Value Model

Grift has these runtime value categories:

| Category | Printed form |
| --- | --- |
| nil | `()` |
| booleans | `#t`, `#f` |
| numbers | decimal integer |
| symbols | symbol name |
| pairs | list syntax or dotted pair syntax |
| strings | quoted by write-mode, raw by display-mode |
| operatives | `<operative>` |
| applicatives | `<applicative>` |
| builtin operatives | `<builtin>` |
| environments | `<environment>` |
| inert | `#inert` |
| ignore | `#ignore` |
| prelude entries | `<prelude:name>` |
| native functions | `<native>` |

### 4.1 Nil

`NIL` is the empty list.

`NIL` is also the runtime representation of the empty string. This is the most
important Grift-specific compatibility rule:

- `()` and `""` are distinct reader syntax
- both evaluate to the same runtime object
- `null? ""` is true
- `pair? ""` is false
- printing `""` through write-mode produces `()`, not `""`

A compatible implementation must preserve this observable aliasing.

### 4.2 Numbers

Numbers are signed machine integers with host width equivalent to Rust `isize`.

- Literal recognition is checked during parsing.
- Arithmetic builtins raise `ArithmeticOverflow` on checked overflow.
- Division is integer division truncating toward zero.

### 4.3 Symbols

Symbols are interned by name.

Two symbols with the same spelling are the same symbol for equality and
environment lookup purposes.

### 4.4 Pairs and Lists

Pairs are immutable cons cells. Proper lists are chains of pairs ending in
`NIL`. Improper lists are pairs whose cdr is not a proper list.

There is no `set-car!` or `set-cdr!`.

### 4.5 Strings

Strings are singly linked chains of character nodes. A non-empty string is not a
distinct top-level value kind; it is a chain analogous to a list.

Observable consequences:

- `car` of a non-empty string returns a newly allocated one-character string
- `cdr` of a non-empty string returns the tail string
- `pair?` is true for non-empty strings
- `pair?` is false for the empty string because the empty string is `NIL`
- `cons` can construct strings only in a restricted case described below

There is no separate character type. A single character is represented as a
one-character string.

### 4.6 Callable Values

Grift has five callable storage forms:

- builtin operatives
- compound operatives created by `vau`
- applicatives created by `wrap` or `lambda`
- prelude entries
- native host functions

Operationally:

- builtin operatives and `vau` operatives receive raw operands
- applicatives evaluate operands first
- prelude entries are bound as applicatives
- native functions are bound as applicatives

## 5. Self-Evaluating Values and Truth

### 5.1 Self-Evaluating Values

These values evaluate to themselves:

- `NIL`
- booleans
- numbers
- strings
- operatives
- applicatives
- builtin values
- environments
- `#inert`
- `#ignore`
- prelude values
- native values

Symbols are not self-evaluating. Pairs are not self-evaluating.

### 5.2 Truth

Only booleans are accepted in boolean contexts.

This differs from Scheme. There is no general truthiness rule such as "anything
except false is true."

The following require actual boolean values and raise `TypeError` otherwise:

- `if`
- `cond` test expressions except `else`
- `and`
- `or`
- `not`

## 6. Environments

An environment is a frame plus zero or more parent environments.

### 6.1 Bindings

Each frame stores bindings from symbols to values.

- `define!` adds a new binding to the current frame
- `set!` mutates an existing binding in a specified frame
- lookup searches the current frame first, then parents

### 6.2 Lookup Order

Lookup behavior is observable and must match:

1. Search the current frame.
2. If there is exactly one parent, continue upward iteratively.
3. If there are multiple parents, search them in left-to-right depth-first
   order.
4. During multi-parent search, already-visited environments are skipped to avoid
   cycles.
5. If no binding is found, raise `UnboundVariable`.

### 6.3 No Implicit Global Fallback

Evaluating in a custom environment does not fall back to the global environment
unless that environment is actually in the parent chain.

This matters for `eval`, `apply`, and user code that constructs sandboxed
environments.

### 6.4 Mutation Rules

`define!`:

- affects only the current frame
- rejects rebinding a symbol already defined in that same frame
- raises `AlreadyDefined` on same-frame redefinition

`set!`:

- updates only the target environment's own frame
- does not search parents for a place to mutate
- raises `UnboundVariable` if the symbol is absent from that frame

The language does not expose the builtin-only ground environment directly.

## 7. Formal Parameter Trees

Formal parameter trees are used by:

- `vau`
- `lambda`
- `fn!`
- `define!`

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

Meaning during matching:

- symbol => bind symbol to the matched object
- `#ignore` => ignore the matched object
- `NIL` => matched object must be `NIL`
- pair => matched object must be a pair and both halves are matched recursively

This gives Grift its destructuring behavior.

Examples:

- `(define! (a . b) (cons 1 2))` binds `a = 1`, `b = 2`
- `(define! ((a b) c) (list (list 1 2) 3))` binds `a = 1`, `b = 2`, `c = 3`
- a bare symbol formal such as `args` captures the entire remaining object

Validation rules:

- duplicate symbols inside a formal parameter tree are invalid
- cyclic formal parameter trees are invalid
- `vau` additionally forbids the environment parameter symbol from also
  appearing anywhere in the formal tree

Validation failures raise:

- `InvalidArgument` for duplicate symbols or env-parameter conflicts
- `Cyclic` for cyclic trees
- `TypeError` for structurally invalid trees

## 8. Evaluation Semantics

### 8.1 Symbol Evaluation

Evaluating a symbol performs environment lookup in the current environment.

### 8.2 Combination Evaluation

Evaluating a pair treats it as a combination:

1. Evaluate the operator position.
2. Dispatch on the resulting callable.

Dispatch rules:

- builtin operative => call with raw operand list and caller environment
- compound operative => call with raw operand list and caller environment
- applicative => evaluate operands left-to-right, then call the wrapped value
- anything else => `NotCallable`

### 8.3 Applicative Evaluation Order

Applicatives evaluate their operands:

- strictly
- left to right
- exactly once

The evaluated operand list is then passed to the wrapped callable.

### 8.4 Operative Invocation

Invoking a compound operative created by `vau`:

1. Create a child environment whose parent is the operative's definition
   environment.
2. Match the formal parameter tree against the operand object.
3. If the operative has an environment parameter, bind that symbol to the
   caller's environment.
4. Evaluate the body in the new environment.

This gives lexical scope for the closure and dynamic access to the caller
environment through the explicit env parameter.

### 8.5 Tail Position

The implementation guarantees tail-call behavior for:

- user operatives
- `if`
- `begin`
- `cond`
- `and`
- `or`
- `let`

This matters operationally but does not change value semantics. A compatible
implementation should preserve unbounded tail recursion.

### 8.6 Arity Behavior

Grift does not enforce a uniform exact-arity rule across all builtins.

- Some forms explicitly validate arity and raise `InvalidArgument`.
- Many forms simply read the operands they need and ignore extras.
- Missing operands often surface as `TypeError` from list access rather than as
  a dedicated arity error.

The per-form behavior below is normative.

## 9. Operatives

Unless otherwise stated, an operative receives its operands unevaluated.

### 9.1 `quote`

Signature:

```lisp
(quote expr)
```

Semantics:

- returns `expr` unchanged
- ignores any extra operands

### 9.2 `if`

Signature:

```lisp
(if test then [else])
```

Semantics:

1. Evaluate `test`.
2. `test` must evaluate to a boolean.
3. If true, evaluate and return `then`.
4. Otherwise:
   - if an else operand is present, evaluate and return it
   - if no else operand is present, return `NIL`
5. Extra operands after the else operand are ignored.

Only the selected branch is evaluated.

### 9.3 `define!`

Signature:

```lisp
(define! definiend expression)
```

Semantics:

1. Validate `definiend` as a formal parameter tree.
2. Evaluate `expression` in the current environment.
3. Match `definiend` against that value in the current frame.
4. Return `#inert`.

Important:

- this is destructuring definition, not Scheme's function-definition shorthand
- `(define! (f x) body)` destructures instead of defining a function
- same-frame rebinding raises `AlreadyDefined`

### 9.4 `fn!`

Signature:

```lisp
(fn! name params body...)
```

Semantics:

- equivalent to `(define! name (lambda params (begin body...)))`
- returns `#inert`

`name` is intended to be a symbol. The current implementation does not perform a
separate explicit type check for that position.

### 9.5 `set!`

Signature:

```lisp
(set! env-expr symbol value-expr)
```

Semantics:

1. Evaluate `env-expr` in the current environment.
2. The result must be an environment.
3. Evaluate `value-expr` in the current environment.
4. `symbol` must be a symbol literal, not a formal parameter tree.
5. Mutate that symbol's existing binding in the target environment's own frame.
6. Return `#inert`.

Differences from Scheme:

- there is no `(set! x value)` shorthand
- the target environment is explicit
- parent frames are not searched for the binding to mutate

### 9.6 `lambda`

Signature:

```lisp
(lambda params body...)
```

Semantics:

- creates an applicative closure
- equivalent to wrapping a `vau` whose environment parameter is ignored
- closes over the definition environment
- arguments are evaluated before parameter matching

### 9.7 `begin`

Signature:

```lisp
(begin expr...)
```

Semantics:

- evaluate expressions left to right
- result is the result of the last expression
- `(begin)` returns `NIL`

### 9.8 `cond`

Signature:

```lisp
(cond (test expr...) ...)
```

Semantics:

- clauses are tried left to right
- in each clause, `test` is evaluated and must produce a boolean
- a clause whose test is the symbol `else` matches unconditionally
- when a clause matches, its body is evaluated as though wrapped in `begin`
- a matching clause with no body returns `NIL`
- if no clause matches, the result is `NIL`

The symbol `else` is recognized by name, not by binding identity.

### 9.9 `and`

Signature:

```lisp
(and expr1 expr2 ...)
```

Semantics:

- requires at least two operands; fewer raise `InvalidArgument`
- operands are evaluated left to right
- every operand must evaluate to a boolean
- evaluation stops at the first `#f`
- if any operand is `#f`, the result is `#f`
- if all operands are `#t`, the result is `#t`

Unlike Scheme, `and` does not return an arbitrary last truthy value because only
booleans are accepted.

### 9.10 `or`

Signature:

```lisp
(or expr1 expr2 ...)
```

Semantics:

- requires at least two operands; fewer raise `InvalidArgument`
- operands are evaluated left to right
- every operand must evaluate to a boolean
- evaluation stops at the first `#t`
- if any operand is `#t`, the result is `#t`
- if all operands are `#f`, the result is `#f`

### 9.11 `let`

Regular form:

```lisp
(let ((name init) ...) body...)
```

Semantics:

1. Create a child environment of the current environment.
2. Evaluate each `init` in the outer environment, not in the new child.
3. Bind each `name` in the child environment.
4. Evaluate `body...` in the child environment.

This is simultaneous binding, not `let*`.

Named form:

```lisp
(let name ((param init) ...) body...)
```

Semantics:

1. Evaluate all `init` expressions in the outer environment.
2. Create a local environment that is a child of the current environment.
3. Bind `name` in that local environment to a recursive function.
4. Invoke that function with the already-evaluated init values.

This is the supported recursive named-let form.

### 9.12 `vau`

Signature:

```lisp
(vau params env-param body...)
```

Semantics:

- creates a compound operative
- `params` must be a valid formal parameter tree
- `env-param` must be either a symbol or `#ignore`
- if `env-param` is `#ignore`, no caller-environment binding is created
- if `env-param` is a symbol, that symbol must not appear anywhere in `params`
- the operative closes over its definition environment

### 9.13 `current-environment`

Signature:

```lisp
(current-environment)
```

Semantics:

- returns the caller's current environment
- ignores any extra operands

## 10. Applicatives

Applicatives evaluate their arguments before running.

### 10.1 Arithmetic

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

- integer division
- divides the first argument by the second
- division by zero => `DivisionByZero`
- extra operands are ignored

### 10.2 Numeric Comparison

These use only the first two arguments and ignore extras:

- `(= a b)`
- `(< a b)`
- `(> a b)`
- `(<= a b)`
- `(>= a b)`

Both operands must be numbers. The result is a boolean.

### 10.3 Pair and String Operations

#### `cons`

```lisp
(cons a b)
```

Normal behavior:

- constructs a pair `(a . b)`

String-specialized behavior:

- if `a` is a one-character string and `b` is any value, the result is a string
  node whose first character is that character and whose tail is `b`
- this is how `(cons (car "h") "ello")` constructs `"hello"`
- `b` is not required to be a well-formed string tail, so this rule can create
  char-node structures whose cdr is not another string node or `NIL`

Important limitation:

- only a one-character first argument triggers string construction
- `(cons "ab" "cd")` constructs a pair, not a string

#### `car`

```lisp
(car pair-or-string)
```

- on a pair, returns the pair's car
- on a non-empty string, returns a newly allocated one-character string
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

- returns the already-evaluated argument list as a proper list
- `(list)` returns `NIL`

### 10.4 Predicates

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

Semantics:

- return `#t` iff every supplied argument matches the predicate
- return `#f` as soon as one argument does not match
- with zero arguments, return `#t`

Specific meanings:

- `pair?` is true for cons cells and non-empty strings
- `operative?` is true for compound operatives and builtin operative values
- `applicative?` is true only for applicative wrapper values

#### `not`

```lisp
(not boolean)
```

- boolean negation
- only the first argument is used
- the first argument must be a boolean
- extra operands are ignored

### 10.5 Equality

#### `eq?`

```lisp
(eq? a b)
```

`eq?` is identity-like equality with a few value-based cases.

It returns true if:

- `a` and `b` are the same object, or
- they are immutable values with equal stored representation

For compatibility, the effective behavior is:

- nil, booleans, numbers, symbols, `#inert`, `#ignore`, prelude entries, and
  native functions compare by abstract value
- pairs compare by object identity only
- environments compare by object identity only
- operatives compare by object identity only
- applicatives compare by object identity only
- strings compare by node representation, not full text content

That last rule is important:

- two separately allocated equal multi-character strings are usually not `eq?`
- a shared string tail may be `eq?`
- two separately allocated one-character strings with the same character are
  `eq?`, because their stored representation is the same

#### `equal?`

```lisp
(equal? a b)
```

`equal?` is structural equality.

Rules:

- if `eq?` is true, `equal?` is true
- pairs compare recursively by car and cdr
- strings compare by complete character sequence
- environments are never `equal?` unless they are `eq?`
- all other values use the `eq?` result

### 10.6 Evaluation and Combiner Operations

#### `eval`

```lisp
(eval expr [env])
```

- if `env` is omitted, use the global environment
- otherwise evaluate `expr` in the supplied environment
- extra operands are ignored after the optional environment
- the environment argument is not eagerly type-checked; if `expr` is
  self-evaluating, `(eval expr non-environment)` can still succeed

#### `wrap`

```lisp
(wrap combiner)
```

- returns an applicative that evaluates operands before delegating to
  `combiner`
- only the first argument is used

#### `unwrap`

```lisp
(unwrap applicative)
```

- extracts the wrapped value from an applicative
- only the first argument is used

#### `apply`

```lisp
(apply combiner arg-list [env])
```

- if `env` is omitted, use the global environment
- `arg-list` is passed directly as the combiner's operand object
- for applicatives, `arg-list` therefore contains already-evaluated arguments
- for compound operatives, `arg-list` is treated as raw operands
- the optional environment argument is not eagerly type-checked; errors arise
  only if the invoked combiner actually uses it as an environment

Current implementation limitation:

- `apply` does not support builtin operative values such as `if` or `quote`
- applying such a builtin operative raises `NotCallable`

### 10.7 Environment Constructors

#### `make-environment`

```lisp
(make-environment env ...)
```

- each argument must be an environment
- returns a fresh environment whose parent list is the supplied environments in
  the given order
- the parent list is copied into fresh list structure
- with zero arguments, returns a parentless environment

#### `make-empty-environment`

```lisp
(make-empty-environment)
```

- always returns a parentless environment

### 10.8 Raw Reader/Printer Builtins

#### `raw-read-string`

```lisp
(raw-read-string string)
```

- parses one expression from the supplied string
- if the input string is empty, returns `NIL`
- trailing unread characters after the first expression are ignored
- reader syntax is the same as the top-level reader

#### `raw-display-to-string`

```lisp
(raw-display-to-string obj)
```

- renders `obj` using display-mode formatting
- strings are emitted without quotes or escapes

#### `raw-write-to-string`

```lisp
(raw-write-to-string obj)
```

- renders `obj` using write-mode formatting
- strings are emitted with surrounding quotes and the five supported escapes

### 10.9 Other Builtins

#### `gc-collect`

```lisp
(gc-collect)
```

- forces a garbage collection cycle
- returns the number of collected runtime objects

This is observable but implementation-dependent. A compatible
reimplementation should return a nonnegative integer with the same meaning.

#### `error`

```lisp
(error msg)
```

- ignores its argument
- always raises `InvalidArgument`

## 11. Printed Representation

### 11.1 Write-Mode Formatting

Write-mode is used by `raw-write-to-string`.

Formatting rules:

- `NIL` => `()`
- booleans => `#t`, `#f`
- numbers => decimal integer
- symbols => symbol name
- strings => quoted, with escapes for `"`, `\`, newline, tab, and carriage return
- proper lists => `(a b c)`
- improper lists => `(a b . c)`
- `#inert` => `#inert`
- `#ignore` => `#ignore`
- compound operatives => `<operative>`
- applicatives => `<applicative>`
- builtin operatives => `<builtin>`
- environments => `<environment>`
- prelude entry `name` => `<prelude:name>`
- native function => `<native>`

### 11.2 Display-Mode Formatting

Display-mode is used by `raw-display-to-string`.

It is identical to write-mode except that non-empty strings are emitted without
quotes and without escape processing.

### 11.3 Empty String Ambiguity

Because the empty string is `NIL`, both display-mode and write-mode render it as
`()`.

Therefore Grift printing is not injective:

- `()` and `""` print the same way in write-mode
- `raw-read-string` can round-trip the value, but not the original source form

## 12. Prelude

The global environment includes these standard bindings as applicatives:

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

These are semantically ordinary top-level definitions. A compatible
implementation may realize them eagerly or lazily as long as the visible
behavior is the same.

## 13. Error Conditions

User-visible errors in the current language include:

- `ParseError { line, col }`
- `InvalidArgument`
- `TypeError`
- `ArithmeticOverflow`
- `DivisionByZero`
- `UnboundVariable`
- `NotCallable`
- `Cyclic`
- `AlreadyDefined`

Typical causes:

| Error | Typical causes |
| --- | --- |
| `ParseError` | malformed list syntax, unexpected `)`, unterminated string |
| `InvalidArgument` | `(and)`, `(or)`, `(-)`, unknown string escape, duplicate ptree symbol, `(error ...)` |
| `TypeError` | wrong runtime type, non-boolean in boolean context, invalid formal tree shape |
| `ArithmeticOverflow` | checked overflow in `+`, `-`, `*`, unary negation |
| `DivisionByZero` | second operand of `/` is zero |
| `UnboundVariable` | symbol not found in the searched environment chain |
| `NotCallable` | attempt to call a non-combiner, or use `apply` on a builtin operative |
| `Cyclic` | cyclic formal parameter tree during validation |
| `AlreadyDefined` | `define!` in a frame that already contains the same symbol |

Two error variants exist in the Rust implementation but are not part of the
normal source-language surface today:

- `ImmutableEnvironment`
- `TraceError`

## 14. Non-Scheme / Non-Kernel Gotchas

The following are essential for compatibility:

- Only booleans are true/false values in conditionals.
- `""` and `()` are the same runtime value.
- Strings are linked lists, so `car`, `cdr`, and `pair?` work on non-empty strings.
- `define!` is destructuring definition, not Scheme function-definition syntax.
- `set!` takes an explicit environment.
- `let` is simultaneous, not sequential.
- `and` and `or` require at least two operands.
- Type predicates are vacuously true on zero operands.
- Large integer literals that do not fit the machine integer type become symbols.
- `apply` does not support builtin operative values like `if`.

## 15. Minimum Checklist for a Compatible Reimplementation

A reimplementation should verify at least these behaviors:

- parse `'x` as `(quote x)`
- support dotted pairs and the exact dot disambiguation rules
- preserve `NIL == empty string`
- intern symbols
- evaluate applicative operands left-to-right
- implement lexical closures for `lambda` and `vau`
- expose caller environments through `vau` env parameters
- implement formal-parameter-tree destructuring for both `define!` and `vau`
- preserve left-to-right DFS search for multi-parent environments
- make `pair?`, `car`, and `cdr` work on non-empty strings
- preserve the exact `eq?`/`equal?` split, especially for strings
- preserve write/display formatting, including the `()` rendering of empty string
