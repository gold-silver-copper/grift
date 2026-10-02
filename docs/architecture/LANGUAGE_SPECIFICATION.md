# The grift language

grift is a Lisp in the family of John Shutt's Kernel: operatives created by
`$vau` receive their operands unevaluated together with the caller's
environment, applicatives are wrapped operatives, and environments are
first-class. It differs from Kernel in three deliberate ways:

1. **The core is as small as it can be.** `$vau` is the only primitive
   operative. Everything that can be written in grift is written in grift,
   in the *library* (Kernel's term), including `$if` and `$define!`.
2. **There is no mutation.** No value changes once it has been observed.
3. **Types are strict.** No punning, no implicit conversion, typed errors,
   an enforced sigil for operatives, and contracts with blame.

The Revised⁻¹ Report on Kernel (R-1RK, `05-07.pdf`) is the source of
names, semantics and library derivations for every feature grift keeps.
Section 11 lists every deviation.

## 1. Lexical syntax

| Datum | Syntax |
|---|---|
| Integer | optional sign, decimal digits: `42`, `-7`, `+3`. Must fit in 64 bits. |
| Boolean | `#t`, `#f` (also `#true`, `#false`) |
| Special values | `#inert`, `#ignore`, `()` |
| Character | `#\a`, `#\λ`, `#\space`, `#\newline`, `#\tab`, `#\return`, `#\null`, `#\alarm`, `#\backspace`, `#\delete`, `#\escape`, `#\x3bb` |
| String | `"..."` with escapes `\" \\ \n \t \r \a \b \0 \x3bb;` |
| Symbol | any other run of non-delimiter characters, case-sensitive |
| List | `(a b c)`, dotted `(a b . c)` |
| Comments | `;` to end of line, `#| ... |#` (nesting), `#;` before a datum |

Delimiters are whitespace and `( ) " ;`. There is **no quotation
shorthand**: `'`, `` ` `` and `,` are parse errors, because grift (like
Kernel) has no `$quote`. A one-line operative serves when symbol data is
needed: `($define! $quote ($vau (x) #ignore x))`, or `string->symbol`.

Parse errors are reported as `parse-error` with the line and column.

## 2. Values

| Type | Predicate | Notes |
|---|---|---|
| integer | `integer?` | checked 64-bit; overflow is an error |
| boolean | `boolean?` | `#t`, `#f` |
| null | `null?` | `()`, the empty list; not a boolean |
| inert | `inert?` | `#inert`, the result of forms used only for effect |
| ignore | `ignore?` | `#ignore`, the wildcard of parameter trees |
| char | `char?` | a Unicode scalar value |
| string | `string?` | immutable text; not a list of chars |
| symbol | `symbol?` | interned |
| pair | `pair?` | immutable; `cons`, `car`, `cdr` |
| environment | `environment?` | immutable |
| operative | `operative?` | `$vau` closures, primitive operatives |
| applicative | `applicative?` | `(wrap combiner)` |
| declaration | `declaration?` | see §4.3 |
| prompt | `prompt?` | delimiter tag for delimited control |
| sub-continuation | `sub-continuation?` | a captured, delimited piece of continuation |
| encapsulated | per type | made by `make-encapsulation-type` |

Integers, booleans, `()`, `#inert`, `#ignore`, chars, symbols and
primitive operatives are immediate values. Everything else lives in the
heap.

## 3. Evaluation

`(eval expr env)`:

- A **symbol** evaluates to its binding in `env`. If it has none, the
  result is an `unbound-symbol` error. If the binding exists but its
  recursive initialization has not finished, the result is an
  `uninitialized-binding` error.
- A **pair** `(head . operands)` is a combination:
  1. If `head` is a symbol, it is looked up and the **sigil rule** (§9) is
     checked. If `head` is a combination, it is evaluated. Any other head
     is used as it is (this is how constructed combinations such as
     `(eval (list $if #t 1 2) env)` work).
  2. If the result is an **operative**, it is called with the operand tree
     unevaluated and the current environment.
  3. If it is an **applicative**, the operands, which must form a proper
     list, are evaluated left to right. Its underlying combiner is then
     applied to the list of results. If that combiner is itself an
     applicative, the results are evaluated again, as Kernel specifies.
  4. Anything else is a `type-error`.
- **Anything else** evaluates to itself.

A compound operative `($vau ptree eparam body)` called with operands `o`
from environment `e` evaluates `body` in a fresh child of its static
environment. That child binds `ptree` against `o` and `eparam` to `e`.

**Proper tail calls.** Calling a combiner in tail position does not grow
the continuation. The body of an operative is in tail position, as is the
expression evaluated by a primitive `eval` that is itself in tail position.
Library forms built this way are therefore properly tail-recursive: `$if`,
`$cond`, `$let`, `$sequence`, `$and?`, `$or?`, `$match`, `apply`. The test
suite runs 1,000,000-iteration loops through each of them.

### 3.1 Parameter trees

A parameter tree is a symbol, `#ignore`, `()`, or a pair of parameter
trees. No symbol may occur twice. Matching a tree against a value:

- a symbol binds the value;
- `#ignore` matches anything;
- `()` matches only `()`;
- a pair matches a pair whose car and cdr match.

Trees are validated when an operative is created and by `environment-bind`
and `environment-bind-recursive`. A shape mismatch along the operand list
of a call is an `arity-error`; a mismatch inside an operand (e.g.
`(car 5)`) is a `type-error`. The environment parameter of `$vau` must be a
symbol not in the tree, or `#ignore`.

## 4. Environments, declarations and purity

### 4.1 The purity invariant

No operation ever changes a value that has been observed. Pairs, strings,
environments, combiners, encapsulated values and sub-continuations are
immutable. There is no `set-car!`, no `$set!`, no mutable cell and no
memoizing promise.

There is exactly one internal write: initializing a recursive binding (see
`environment-bind-recursive`). Reading such a binding before it is
initialized raises `uninitialized-binding`. Initializing it a second time,
for example by reinstating a sub-continuation captured during the
initialization, raises `reinitialized-binding`. No program can therefore
observe a binding change value.

Side effects exist only in host-registered natives.

### 4.2 Environments

- `(make-environment)` returns a new empty environment with no parent.
- `(environment-bind env ptree value)` returns a **new child** of `env`
  binding `ptree` against `value`. `env` is unchanged.
- `(environment-bind-recursive env ptree expr)` creates a child `E` of
  `env` in which the symbols of `ptree` are bound but uninitialized. It
  then evaluates `expr` in `E`, matches the result against `ptree` to
  initialize the bindings, and returns `E`. This is `letrec`: mutual
  recursion uses a tree, e.g. `(even? odd?)`.
- There is no way to enumerate an environment or obtain its parent.
  Lookup searches an environment's own bindings, then its parent's.

### 4.3 Declarations

A declaration is an opaque value wrapping an applicative
`f : environment → environment`. It is made with `make-declaration`,
recognized with `declaration?`, and applied with
`(apply-declaration decl env)`, which calls `f` and checks that the result
is an environment.

A declaration has no effect on its own. Two things apply declarations:

- **Bodies.** `$sequence`, and therefore the body of every `$vau`,
  `$lambda`, `$let`, `$cond` clause, `$guard` and so on, evaluates its
  forms left to right. When a non-final form evaluates to a declaration,
  the body applies it to the current environment, and the forms after it
  are evaluated in the environment it returns. The final form is evaluated
  in tail position and its value is the value of the body.
- **The top level.** The host evaluates source text the same way, form by
  form. `Grift::eval`, the REPL and library loading all keep the resulting
  environment for later input.

`$define!`, `$import!`, `$define-record-type!` and `$define/contract!` are
library operatives that return declarations. Users can write their own,
since declarations are first-class.

Consequences:

- **Redefinition shadows.** A later `($define! x 2)` creates a new binding
  in a new child environment. Closures created earlier keep seeing the old
  binding, as at an ML top level.
- **Recursive definitions.** `$define!` is recursive: the definiend is in
  scope in its own initializer. `($define! x (+ x 1))` is therefore an
  `uninitialized-binding` error. Use `$let` to compute from an outer `x`.
- **Mutual recursion.** It needs one definition with a tree, or
  `$letrec`. A closure cannot see definitions made after it was created.
- **Definitions as values.** A definition evaluated somewhere other than
  in a body, e.g. as an argument, is just a declaration value.

## 5. The primitives

The Rust core provides exactly these. `$vau` is the only operative; every
other primitive is bound as an applicative. All of them check their arity
and argument types.

| Primitive | Meaning |
|---|---|
| `($vau ptree eparam body)` | compound operative; exactly one body expression |
| `(wrap combiner)` | applicative over `combiner` (which must be a combiner) |
| `(unwrap applicative)` | the underlying combiner |
| `(eval expr env)` | evaluate `expr` in `env` (tail position) |
| `(make-environment)` | empty environment, no parent |
| `(environment-bind env ptree value)` | new child binding `ptree` |
| `(environment-bind-recursive env ptree expr)` | new child, `letrec` semantics |
| `(make-declaration applicative)` | a declaration |
| `(apply-declaration decl env)` | apply a declaration; result must be an environment |
| `(select bool x y)` | `x` if `bool` is `#t`, `y` if `#f`, else `type-error` |
| `(cons a d)` | a pair |
| `(eq? a b)` | identity (§9 rule 4) |
| `(make-encapsulation-type)` | `(constructor predicate accessor)` for a new type |
| `pair?` `symbol?` `integer?` `char?` `string?` `environment?` `operative?` `applicative?` `declaration?` `prompt?` `sub-continuation?` | unary type predicates |
| `(make-prompt)` | a fresh prompt |
| `(push-prompt p thunk)` | call `thunk` under a frame delimited by `p` |
| `(with-sub-cont p f)` | capture the frames up to the innermost `p`, remove them and that prompt, and call `f` with the sub-continuation |
| `(push-sub-cont k thunk)` | reinstate `k` on the current continuation and call `thunk` in it; may be done any number of times |
| `error-prompt` | the prompt errors are delivered to (a value, not a combiner) |
| `(+ a b)` `(- a b)` `(* a b)` | checked: `overflow` on overflow |
| `(div n d)` | Euclidean quotient: `n = d·q + r`, `0 ≤ r < |d|`; `division-by-zero`, `overflow` |
| `(<? a b)` | integer comparison |
| `string->list` `list->string` `string->symbol` `symbol->string` `char->integer` `integer->char` | text conversions |
| `string=?` `string-length` | performance primitives (derivable via `string->list`, but pervasive) |
| `(read string)` | the single datum written in `string` |
| `(write v)` `(display v)` | the external representation of `v`, as a string |

`apply-declaration` cannot be derived: a declaration is opaque, and both
library bodies and the host need to apply one.

Primitive predicates take one argument; the library defines Kernel's
n-ary versions under the same names. The binary arithmetic primitives are
likewise shadowed by n-ary library versions.

## 6. The library

Loaded in order into the *ground environment*, a child of the environment
holding the primitives. User code runs in a *standard environment*, a
child of the finished ground environment. The primitive environment cannot
be reached from programs.

| File | Contents |
|---|---|
| `00-boot` | `$define!` (built directly from `make-declaration` and `environment-bind-recursive`), `$primitive-vau`, `list`, `list*`, `car`, `cdr` and compositions (via parameter trees), `$if` (over `select`), `$sequence` (declaration-aware), multi-body `$vau`, `$lambda`, `apply` |
| `10-control` | `not?`, `$cond`, `$and?`, `$or?`, `and?`, `or?`, `$let`, `$let*`, `$letrec`, `$letrec*`, `raise-error`, `error`, `$define!` with the definition-time sigil check |
| `20-env` | `get-current-environment`, `$remote-eval`, `$let-redirect`, `$bindings->environment`, `$module`, `$import!` |
| `30-list` | n-ary type predicates, `null?`, `boolean?`, `inert?`, `ignore?`, `combiner?`, `list?`, `length`, `list-tail`, `list-ref`, `append`, `reverse`, `map`, `for-each`, `filter`, `fold-left`, `fold-right`, `reduce`, `equal?`, `assoc`, `assq`, `member?`, `memq?`, `$match` |
| `40-integer` | n-ary `+ * -`, `=? <? <=? >? >=?`, `div`, `mod`, `div-and-mod`, `zero?`, `positive?`, `negative?`, `even?`, `odd?`, `abs`, `max`, `min`, `gcd`, `lcm`, `expt` |
| `50-control` | `reset-at`, `shift-at`, `control0-at`, `reset`, `shift`, `$reset`, `$shift`; error objects, `with-error-handler`, `raise`, `error-object-resume`, `$guard`, `error-kind?`, `$binds?`; `make-parameter`, `parameterize`, `$parameterize`; `yield`, `make-generator`, `generator->list` |
| `60-effects` | `make-effect`, `effect?`, `perform`, `handle`, `$handle` |
| `70-contract` | `make-blame`, `make-contract`, `flat-contract`, `any/c`, `none/c`, `list-of`, `or/c`, `and/c`, `rest/c`, `->`, `check`, `contract`, `$define/contract!`, `$define-record-type!` |
| `80-string` | char comparisons and classification (ASCII), `string-append`, `string`, `string-ref`, `substring`, string comparisons, `string-upcase`, `string-downcase`, `string-join`, `number->string`, `string->number`, `symbol<?` |
| `90-rational` | optional exact rationals: `make-rational`, `rational?`, `rational+ - * /`, `rational=?`, `rational<?`, `rational-numerator`, `rational-denominator`, `rational->string` |
| `99-standard` | `make-kernel-standard-environment`, `$let-safe` |

Where a definition follows R-1RK, its comment cites the section.

Notes on some library forms:

- `$cond` clauses are `(test . body)`. There is no `else`; use `#t`. No
  match yields `#inert`.
- `$and?` and `$or?` evaluate operands left to right until the answer is
  known. The last operand is in tail position and its value is returned
  unchecked (R-1RK §6.1.4).
- `$match` patterns:

  | Pattern | Matches |
  |---|---|
  | `#ignore` | anything |
  | a symbol | anything, and binds it |
  | `(? pred pat ...)` | when `(pred v)` is `#t` and each `pat` matches `v` |
  | `(app f pat)` | when `pat` matches `(f v)` |
  | `(p1 . p2)` | a pair |
  | anything else | a literal, compared with `eq?` (strings with `string=?`) |

  If no clause matches, the result is a `match-failure` error.
- `$module` replaces Kernel's `$provide!`. It evaluates a body
  (declarations included) in a child environment and returns the
  environment the body ends in. `($import! env-expr sym ...)` is a
  declaration binding those symbols to their values in that environment.

## 7. Delimited control

grift has no `call/cc`. Its four control primitives are Dybvig, Peyton
Jones and Sabry's multi-prompt interface (`newPrompt`, `pushPrompt`,
`withSubCont`, `pushSubCont`):

- Prompts are tagged, so independent uses do not interfere. A capture goes
  to the innermost prompt with its tag; other prompts in between are
  captured along.
- `with-sub-cont` has `control0` semantics: it removes both the captured
  frames and the prompt, and `f` runs in the continuation of the
  `push-prompt`.
- Sub-continuations are a distinct type. They are not callable and are
  reinstated only with `push-sub-cont`, any number of times.
- `with-sub-cont` with no matching prompt raises `no-prompt`.

The library derives from these primitives:

- `shift`/`reset`, `control0` and their multi-prompt variants;
- **deep effect handlers** (`handle`, `perform`);
- **generators**: pure lazy streams, each step `()` or `(value . next)`;
- **parameters**: dynamic binding, following Kiselyov, Shan & Sabry.

## 8. Errors

Errors are an effect delivered through `error-prompt`. When a primitive
fails, the machine behaves exactly as:

```lisp
(with-sub-cont error-prompt ($lambda (k) (list k kind message irritants)))
```

Here `kind` is a symbol, `message` a string, and `irritants` a list of the
values involved. `k` lets a handler resume the failed computation.
`raise-error` and `error` (kind `user`) use the same protocol.

`(with-error-handler handler thunk)` pushes `error-prompt` around `thunk`,
wrapping a normal result in a private `ok` record so it cannot be confused
with an error payload. If an error arrives, `handler` is called with an
error object, outside the protected region, and its result is returned.

- **Deep handlers.** `(error-object-resume e v)` continues the failed
  computation as if the failing operation had returned `v`, under the same
  handler.
- **Errors inside a handler** propagate outward.
- **`$guard`.** `($guard (var clause ...) body ...)` binds `var` to the
  error object and tries `$cond`-style clauses. If no clause matches, the
  error is raised again with `raise`.
- **Uncaught errors.** When there is no handler, the host's own error
  prompt receives the error and `Grift::eval` returns
  `Err(Error { kind, message, irritants })`.

Error objects provide `error-object-kind`, `error-object-message`,
`error-object-irritants` and `error-object-continuation`.

### Error kinds

| Kind | Raised by |
|---|---|
| `type-error` | a value of the wrong type; `$if` on a non-boolean; a mismatch inside an operand |
| `arity-error` | wrong number of operands; lists of different lengths in `map` |
| `range-error` | `list-ref`, `string-ref`, `substring`, `integer->char`, `expt` out of range |
| `overflow` | integer overflow |
| `division-by-zero` | `div`, `mod`, `make-rational` |
| `unbound-symbol` | a symbol with no binding |
| `uninitialized-binding` | reading a recursive binding before its initializer finished |
| `reinitialized-binding` | initializing a recursive binding twice (via a reinstated sub-continuation) |
| `sigil-violation` | the sigil rule (§9 rule 5) |
| `no-prompt` | `with-sub-cont`, `perform` or `yield` with no matching prompt |
| `match-failure` | no `$match` clause matches |
| `contract-violation` | a broken contract; irritants are `(blamed-party contract-name value binding-name)` |
| `parse-error` | the reader, `read`, `string->number` |
| `user` | `error` |

## 9. Type discipline

These rules are normative.

1. **No punning or implicit conversion.**
   - `()` is not false, and there is no truthiness.
   - `select`, `$if`, `$cond`, `$and?`, `$or?`, `not?`, `and?` and `or?`
     require booleans (except the tail operand of `$and?`/`$or?`, §6).
   - Strings are not lists and chars are not one-character strings.
   - Integers never change type: overflow is an error, not promotion.
2. **Primitives check everything.** Every primitive checks its arity and
   argument types and raises a typed error. `wrap` requires a combiner,
   `unwrap` an applicative, `eval` an environment.
3. **Purity.** See §4.1.
4. **Abstraction and identity.**
   - User-defined types come only from `make-encapsulation-type`. A value
     of such a type is opaque to anyone without its predicate and
     accessor, and the accessor raises `type-error` on any other value.
   - `eq?` compares identity. Immutable structured data (pairs, strings,
     encapsulated values) has no meaningful identity, so comparing two
     structured values of the same kind is a `type-error`; use `equal?`.
     Comparing values of different kinds answers normally, e.g.
     `(eq? x ())` is how `null?` is defined.
   - `equal?` compares pairs and strings structurally and everything else
     with `eq?`. Two encapsulated values compare structurally only if
     their type opted in (records declared with `(name equal)`).
     Otherwise `equal?` raises `type-error` rather than look through an
     abstraction barrier.
5. **The sigil rule.**
   - **At call sites:** when a combination's head is a symbol, a symbol
     starting with `$` must evaluate to an operative, and any other symbol
     to a non-operative. Violations raise `sigil-violation`, so a call
     site always tells the truth about whether operands are evaluated.
   - **At definition time:** `$define!` (and the library's other defining
     forms) enforce the same rule for every symbol they bind.
   - **Not enforced** for heads that are not symbols, or for formal
     parameters: operatives may be passed around as data under any name.
     Natives are applicatives, so their names may not start with `$`.
6. **Higher-order contracts with blame** (Findler & Felleisen).
   - Contracts are first-class values (`contract?`). Predicates act as
     flat contracts.
   - `(-> dom ... range)` makes a function contract. A trailing
     `(rest/c c)` domain covers any remaining arguments.
   - Applying a function contract returns a new applicative that checks
     every call:
     - arguments are checked with the blame swapped (the caller supplied
       them);
     - the result is checked with the original blame (the function
       supplied it);
     - a wrong argument count blames the caller.
   - Function arguments are wrapped in turn, so blame follows the party
     that actually supplied a bad value.
   - Combinators: `list-of`, `or/c` (chooses by first-order test), `and/c`,
     `any/c`, `none/c`, `flat-contract`, `make-contract`.
   - Applying contracts: `check`, `contract`, and `$define/contract!`
     (blames "the definition of f" or "a caller of f").
7. **Exhaustive matching.** A `$match` with no matching clause raises
   `match-failure`.

## 10. Records

```lisp
($define-record-type! point (make-point x y) point? (x point-x) (y point-y))
($define-record-type! (vec equal) (make-vec x y) vec? (x vec-x) (y vec-y))
```

This declares a new encapsulated type, with:

- a constructor that checks its arity;
- a predicate;
- accessors that check their argument's type.

The `(name equal)` form opts the type into structural `equal?`.

## 11. Deviations from R-1RK

| R-1RK | grift | Why |
|---|---|---|
| `$if`, `$define!`, `$sequence` (primitive operatives) | library operatives over `select`, declarations and `$vau` | one primitive operative |
| `$vau` with several body forms | primitive takes one; library `$vau` adds `$sequence` | minimal core |
| mutable pairs: `set-car!`, `set-cdr!`, `copy-es-immutable`, `encycle!`, `append!` | removed | no mutation |
| cyclic lists, `get-list-metrics`, cycle-aware list operations | removed (cycles cannot be built) | follows from immutability |
| `$define!` mutates the current environment | returns a declaration; bodies and the top level thread environments | no mutation |
| `$set!` | removed | no mutation |
| `$provide!` | replaced by `$module` | no mutation |
| `make-environment` with any number of parents | no parents; children come from `environment-bind` | minimal core, immutability |
| `call/cc`, `extend-continuation`, `guard-continuation`, `continuation->applicative`, `root-continuation`, `error-continuation`, `$let/cc`, `apply-continuation` | multi-prompt delimited control; errors via `error-prompt` | delimited control composes and subsumes them |
| keyed dynamic variables | library parameters over delimited control | derivable |
| keyed static variables | removed | not needed without mutation |
| promises: `$lazy`, `force`, `memoize` | removed; use thunks or generators | memoization is mutation |
| numeric tower: rationals, inexact reals, infinities, bounds, robustness | checked 64-bit integers; rationals as an optional library type | strictness, minimal core |
| integers of unbounded size | 64-bit, overflow is an error | integers never change type |
| ports | none; host natives provide I/O | effects belong to the host |
| `equal?`, `boolean?`, `null?`, `inert?`, `ignore?` (primitive) | library | derivable |
| `eq?` defined on all objects | refuses to compare two structured values of the same kind | immutable data has no identity |
| `$and?`/`$or?`, `$cond` and `$if` accept any test value | require booleans | strict booleans |
| no sigil enforcement | sigil rule enforced at call sites and definitions | call sites tell the truth |
| strings and characters (unspecified in R-1RK) | immutable strings, chars, names from klisp/R7RS | — |
| `error` (unspecified in R-1RK) | `error`, `raise-error`, error objects as in klisp | — |

There is no `$quote`, as in Kernel.

## 12. The host interface

`Grift::new()` loads the library. `eval(src)` folds the forms of `src` as
described in §4.3 and returns the value of the last form that is not a
declaration, or `#inert`. Natives registered with `register_native` are
bound, as applicatives, in a new child of the top-level environment. They
receive evaluated arguments; an error they return is delivered through the
error protocol like a primitive's.
