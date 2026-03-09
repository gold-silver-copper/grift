# Language Reference

Grift implements a strict subset of the Kernel programming language (Shutt 2010),
a Lisp dialect based on the vau calculus. This document specifies every type,
every primitive, and the evaluation rules precisely enough to reimplement the
evaluator from scratch.

## Types

| Type | Written as | Self-evaluating | Mutable | `eq?` semantics |
|------|-----------|-----------------|---------|-----------------|
| Nil | `()` | yes | no | value |
| Boolean | `#t`, `#f`, `#true`, `#false` | yes | no | value |
| Number | `42`, `-7`, `+3` | yes | no | value |
| Symbol | `foo`, `define!`, `+` | no (triggers lookup) | no | value |
| Pair | `(1 . 2)`, `(a b c)` | no (triggers combination) | no | identity |
| String | `"hello"` | yes | no | identity |
| Operative | `<operative>` | yes | no | identity |
| Applicative | `<applicative>` | yes | no | identity |
| Builtin | `<builtin>` | yes | no | identity |
| Environment | `<environment>` | yes | yes | identity |
| Inert | `#inert` | yes | no | value |
| Ignore | `#ignore` | yes | no | value |

"Value" under `eq?` semantics means two distinct allocations with the same
content are considered `eq?`. "Identity" means `eq?` compares arena slot
identity — two structurally identical pairs at different slots are not `eq?`.

## Evaluation Rules

### Self-Evaluating Forms

Everything except symbols and pairs evaluates to itself. Numbers, booleans,
strings, nil, inert, ignore, operatives, applicatives, builtins,
and environments all return themselves when evaluated.

### Symbol Lookup

When a symbol is evaluated, the evaluator walks the environment chain:

1. Search the current environment's bindings alist for a matching symbol.
2. If not found and there is a single parent, repeat with the parent.
3. If there are multiple parents, perform depth-first search with cycle detection.
4. If no binding is found in any ancestor, signal `UnboundVariable`.

Symbol identity is compared by `ArenaIndex` equality (interning ensures each
unique name has exactly one symbol index).

### Combination (Function Application)

When a pair `(operator . operands)` is evaluated:

1. Evaluate `operator` to obtain a combiner.
2. Dispatch based on the combiner type:

**Operative** (compound fexpr): Pass `operands` unevaluated and the caller's
environment to the operative. Bind parameters per the parameter tree, optionally
bind the environment parameter, then evaluate the body in the operative's
closed environment extended with those bindings.

**Builtin** (Rust-native operative): Pass `operands` unevaluated and the
caller's environment to the Rust dispatch function.

**Applicative** (wrapper): Evaluate all operands left-to-right in the caller's
environment, then pass the evaluated argument list to the underlying combiner
(which may be an operative, builtin, or another applicative).

### Truthiness

Grift follows strict boolean semantics. The `if`, `and`, `or`, `cond`, and
`not` operatives require their test expressions to evaluate to actual boolean
values (`#t` or `#f`). Non-boolean values in boolean context signal `TypeError`.

### Formal Parameter Trees

Parameter binding uses recursive tree matching (Kernel §4.9.1):

- A **symbol** binds to the corresponding argument value.
- `#ignore` discards the corresponding argument.
- `()` (nil) requires the corresponding argument to also be nil.
- A **pair** requires the argument to be a pair and recursively matches car/cdr.

Parameter trees must be acyclic and contain no duplicate symbols. The
`env-param` of a `vau` form must not duplicate any symbol in the parameter tree.

## Primitive Operatives

Operatives receive their operands unevaluated.

### `quote`

```
(quote expr) → expr
```

Return `expr` without evaluating it.

```lisp
(quote (+ 1 2))   ; → (+ 1 2)  — the list, not 3
(quote hello)      ; → hello    — the symbol
```

### `if`

```
(if test consequent)
(if test consequent alternative)
```

Evaluate `test`. If `#t`, evaluate `consequent` in tail position. If `#f`,
evaluate `alternative` in tail position (or return `()` if omitted). `test`
must evaluate to a boolean.

```lisp
(if #t 1 2)        ; → 1
(if #f 1 2)        ; → 2
(if (< 3 5) 10 20) ; → 10
(if #f 1)           ; → ()
```

### `define!`

```
(define! definiend expression)
```

Evaluate `expression` in the current environment, then match `definiend`
(a formal parameter tree) against the result, binding symbols in the current
environment. Returns `#inert`. Mutating the ground environment is forbidden.
If a binding for the symbol already exists in the current frame, `define!`
signals an error — use `set!` to update existing bindings.

**Ptree destructuring:** When `definiend` is a pair, standard parameter tree
matching applies.  This includes pairs beginning with symbols, `#ignore`,
nested pairs, or nil.

```lisp
(define! x 42)                           ; simple binding
(define! x 99)                           ; ERROR: x already defined (use set!)
(define! (a b) (list 1 2))               ; ptree destructuring
(define! ((a b) c) (list (list 1 2) 3))  ; nested ptree destructuring
```

### `set!`

```
(set! env-expr symbol expression)
```

Evaluate `env-expr` to get a target environment and `expression` to get a
value, then update the existing binding of `symbol` in the target environment's
own frame. Returns `#inert`.

**Important constraints:**
- The `symbol` must already exist as a binding in the target environment's own
  frame. `set!` does **not** walk the parent chain and does **not** create new
  bindings. If the symbol is not found, `UnboundVariable` is signaled.
- Only single symbol formals are supported (no parameter tree destructuring).
- The target must be an environment and must not be the ground environment.

```lisp
(define! x 1)
(set! (current-environment) x 2)
x                                  ; → 2

(set! (current-environment) y 1)   ; error: UnboundVariable

(define! e (make-environment (current-environment)))
(set! e x 1)                       ; error: UnboundVariable (x is in parent, not e)
```

### `lambda`

```
(lambda params body ...)
```

Create an applicative (arguments are evaluated before binding). Equivalent to
`(wrap (vau params #ignore (begin body ...)))`. The `params` must be a valid
formal parameter tree. Multiple body expressions are implicitly wrapped in
`begin`.

```lisp
(define! add1 (lambda (x) (+ x 1)))
(add1 5)  ; → 6

(define! swap (lambda (a b) (list b a)))
(swap 1 2)  ; → (2 1)
```

### `vau`

```
(vau params env-param body ...)
```

Create an operative (fexpr). `params` is a formal parameter tree matched against
the unevaluated operands. `env-param` is either a symbol (bound to the caller's
environment) or `#ignore`. Multiple body expressions are wrapped in `begin`.

```lisp
;; An operative that returns its operand unevaluated
(define! my-quote (vau (x) #ignore x))
(my-quote (+ 1 2))  ; → (+ 1 2)

;; An operative that evaluates in the caller's environment
(define! my-eval (vau (x) e (eval x e)))
(my-eval (+ 1 2))   ; → 3
```

### `begin`

```
(begin expr1 expr2 ... exprN)
```

Evaluate each expression in order. The last expression is in tail position.
Returns the value of the last expression, or `()` if given no expressions.

```lisp
(begin 1 2 3)  ; → 3
(begin
  (define! x 10)
  (+ x 5))     ; → 15
```

### `cond`

```
(cond (test1 body1 ...)
      (test2 body2 ...)
      ...
      (else bodyN ...))
```

Evaluate tests in order until one returns `#t` (or the `else` clause is
reached), then evaluate the corresponding body expressions (last in tail
position). Returns `()` if no clause matches.

```lisp
(cond
  ((= x 1) 10)
  ((= x 2) 20)
  (else 30))
```

### `and`

```
(and expr1 expr2 ... exprN)
```

Evaluate expressions left-to-right. If any evaluates to `#f`, return `#f`
immediately. Otherwise return the result of the last expression (tail position).
With no arguments, returns `#t`.

```lisp
(and #t #t #t)  ; → #t
(and #t #f #t)  ; → #f
```

### `or`

```
(or expr1 expr2 ... exprN)
```

Evaluate expressions left-to-right. If any evaluates to `#t`, return `#t`
immediately. Otherwise return the result of the last expression (tail position).
With no arguments, returns `#f`.

```lisp
(or #f #f #t)  ; → #t
(or #f #f #f)  ; → #f
```

### `let`

```
(let ((name1 val1) (name2 val2) ...) body ...)
(let name ((param1 init1) (param2 init2) ...) body ...)
```

**Regular let:** Create a child environment, evaluate each `valN` in the
**outer** environment, bind each `nameN` in the child environment, then
evaluate the body expressions in the child environment (last in tail position).

**Named let:** When the first argument is a symbol, it is treated as a named
let. `(let name ((param init) ...) body...)` creates a recursive function
`name` with the given parameters, evaluates the init expressions in the
**outer** environment, then calls the function with the evaluated init values.
The `name` is visible within the body for recursion, but not outside the `let`.
This is equivalent to:

```lisp
(let ()
  (fn! name (param ...) body...)
  (name init ...))
```

Named let supports tail-call optimization for the recursive calls.

```lisp
;; Regular let
(let ((x 10) (y 20))
  (+ x y))                        ; → 30

;; Named let: loop
(let loop ((i 0))
  (if (< i 10) (loop (+ i 1)) i)) ; → 10

;; Named let: factorial
(let fact ((n 5) (acc 1))
  (if (= n 0) acc
    (fact (- n 1) (* acc n))))     ; → 120

;; Named let: fibonacci
(let fib ((n 10) (a 0) (b 1))
  (if (= n 0) a
    (fib (- n 1) b (+ a b))))     ; → 55
```

## Primitive Applicatives

Applicatives evaluate their arguments before the body executes.

### Arithmetic

| Name | Signature | Behavior |
|------|-----------|----------|
| `+` | `(+ . numbers)` | Sum. Zero arguments returns `0`. |
| `-` | `(- n . rest)` | With one argument: negate. With two+: left fold subtraction. |
| `*` | `(* . numbers)` | Product. Zero arguments returns `1`. |
| `/` | `(/ a b)` | Integer (truncating) division. `DivisionByZero` if `b` is 0. |

All arithmetic uses checked operations. Overflow signals `ArithmeticOverflow`.

```lisp
(+ 1 2 3)    ; → 6
(+)           ; → 0
(- 10 3)      ; → 7
(- 5)         ; → -5
(* 2 3 4)     ; → 24
(*)           ; → 1
(/ 10 3)      ; → 3
```

### Comparison

| Name | Signature | Behavior |
|------|-----------|----------|
| `=` | `(= a b)` | Numeric equality. |
| `<` | `(< a b)` | Less than. |
| `>` | `(> a b)` | Greater than. |
| `<=` | `(<= a b)` | Less than or equal. |
| `>=` | `(>= a b)` | Greater than or equal. |

Both arguments must be numbers. Returns `#t` or `#f`.

### Pair Operations

| Name | Signature | Behavior |
|------|-----------|----------|
| `cons` | `(cons a b)` | Construct a pair. |
| `car` | `(car pair)` | First element of a pair. |
| `cdr` | `(cdr pair)` | Second element of a pair. |
| `list` | `(list . items)` | Return the argument list as-is (already a proper list). |

```lisp
(cons 1 2)         ; → (1 . 2)
(cons 1 (list 2 3)); → (1 2 3)
(car (cons 1 2))   ; → 1
(cdr (cons 1 2))   ; → 2
(list 1 2 3)       ; → (1 2 3)
```

### Type Predicates

All type predicates are variadic: `(pred? . objects)` returns `#t` if and only
if every argument matches the type.

| Name | Returns `#t` when |
|------|-------------------|
| `null?` | All arguments are `()` |
| `pair?` | All arguments are pairs |
| `number?` | All arguments are numbers |
| `symbol?` | All arguments are symbols |
| `boolean?` | All arguments are booleans |
| `inert?` | All arguments are `#inert` |
| `ignore?` | All arguments are `#ignore` |
| `operative?` | All arguments are operatives or builtins |
| `applicative?` | All arguments are applicatives |
| `environment?` | All arguments are environments |

```lisp
(null? ())          ; → #t
(null? 1)           ; → #f
(pair? (cons 1 2))  ; → #t
(number? 1 2 3)     ; → #t
(number? 1 "a")     ; → #f
```

### Boolean Operations

| Name | Signature | Behavior |
|------|-----------|----------|
| `not` | `(not boolean)` | Boolean negation. Argument must be a boolean. |

```lisp
(not #t)  ; → #f
(not #f)  ; → #t
```

### Equality

| Name | Signature | Behavior |
|------|-----------|----------|
| `eq?` | `(eq? a b)` | Identity equality. For immutable scalar types (nil, booleans, numbers, symbols, inert, ignore), compares by value. For mutable/constructed types (pairs, strings, environments, combiners), compares by arena identity. |
| `equal?` | `(equal? a b)` | Structural equality. Returns `#t` whenever `eq?` would. Additionally compares pairs structurally (recursive car/cdr) and strings character-by-character. Different environments are never `equal?` unless `eq?`. |

```lisp
(eq? 1 1)              ; → #t
(eq? (cons 1 2) (cons 1 2))  ; → #f (different allocations)
(equal? (cons 1 2) (cons 1 2))  ; → #t (structural match)
(equal? "abc" "abc")   ; → #t
```

### Combiner Operations

| Name | Signature | Behavior |
|------|-----------|----------|
| `eval` | `(eval expr)` or `(eval expr env)` | Evaluate `expr` in the given environment (defaults to the standard environment if omitted). |
| `wrap` | `(wrap combiner)` | Wrap a combiner in an applicative (arguments will be evaluated). |
| `unwrap` | `(unwrap applicative)` | Extract the underlying combiner from an applicative. |

```lisp
(eval (list '+ 1 2))  ; → 3
(wrap (vau (x) #ignore x))  ; creates an applicative from an operative
(unwrap (lambda (x) x))     ; extracts the underlying operative
```

### Environment Operations

| Name | Signature | Behavior |
|------|-----------|----------|
| `make-environment` | `(make-environment . envs)` | Create a new environment with the given parents. All arguments must be environments. |
| `make-empty-environment` | `(make-empty-environment)` | Create a new environment with no parents. |
| `current-environment` | `(current-environment)` | Return the caller's dynamic environment. This is an operative (not applicative) because it needs the caller's environment, not evaluated arguments. |

`current-environment` is essential for obtaining a reference to the environment
that can be passed to `set!` for mutation.

```lisp
(environment? (current-environment))  ; → #t

;; Capture environment for mutation
(define! x 1)
(define! e (current-environment))
(set! e x 2)
x                                     ; → 2

;; Different scopes give different environments
(define! outer (current-environment))
(let ()
  (define! inner (current-environment))
  (eq? outer inner))                   ; → #f

;; Mutable cells using current-environment and set!
(fn! make-cell (val)
  (define! env (current-environment))
  (list
    (lambda () val)
    (lambda (new-val) (set! env val new-val))))
(define! cell (make-cell 0))
((car cell))                           ; → 0
((car (cdr cell)) 42)
((car cell))                           ; → 42
```

## Error Conditions

| Error | Condition |
|-------|-----------|
| `OutOfMemory` | Arena full, allocation failed. |
| `TypeError` | Wrong type for operation (e.g., `(car 5)`). |
| `ParseError` | Malformed S-expression syntax. |
| `UnboundVariable` | Symbol not found in environment chain. |
| `NotCallable` | Attempted to apply a non-combiner value. |
| `ArithmeticOverflow` | Checked arithmetic operation overflowed. |
| `DivisionByZero` | Division or modulo by zero. |
| `InvalidArgument` | Bad argument (e.g., duplicate symbol in parameter tree, zero-count argument). |
| `ImmutableEnvironment` | Attempted to mutate the ground environment. |
| `Cyclic` | Cycle detected in parameter tree or structure traversal. |

## Deriving Lambda from Vau

The relationship between `vau`, `wrap`, and `lambda` is central to the
language. `lambda` is not a primitive — it is defined in terms of `vau` and
`wrap`:

```lisp
;; lambda is equivalent to:
;; (define! lambda (vau (params . body) env
;;   (wrap (eval (list vau params #ignore (cons begin body)) env))))

;; But in Grift it's implemented directly as a builtin operative
;; that creates (wrap (vau params #ignore (begin body...)))
```

A `vau` form creates an operative: a combiner that receives its operands
unevaluated. `wrap` creates an applicative: a combiner that evaluates its
operands before passing them to the wrapped combiner. Therefore `lambda` —
which evaluates arguments, then binds them — is simply a wrapped `vau` that
ignores the caller's environment.

This means any applicative can be unwrapped to reveal the operative underneath,
and any operative can be wrapped to become an applicative. The evaluator has
one dispatch mechanism rather than separate function-call and macro-expansion
phases.
