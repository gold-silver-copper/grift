# The grift language

grift follows the Revised⁻¹ Report on the Kernel Programming Language (R-1RK, [05-07.pdf](../05-07.pdf))
in its evaluation model and names, with a much smaller set of combiners. Section numbers below
are R-1RK's.

## Evaluation

- A symbol evaluates to its binding, looked up through the environment and its ancestors; an
  unbound symbol is the error `unbound-symbol` carrying the symbol.
- A pair `(c . operands)` is a combination. `c` is evaluated to a combiner. An **operative**
  receives the operands unevaluated, with the caller's environment. An **applicative** evaluates
  the operands left to right and passes the list of results to the combiner it wraps.
- Everything else evaluates to itself, `()` included.
- A compound operative's body, and `eval`, are evaluated as tail calls: a loop through `$if`,
  `$sequence`, `$cond`, `$let` or `$lambda` bodies runs in constant space.
- There are no limits on time, depth or memory besides those of the allocator and the OS.

## Values

| Value | Written | `type-of` |
|---|---|---|
| the empty list | `()` | `null` |
| booleans | `#t` `#f` | `boolean` |
| inert, the value of forms with nothing to say | `#inert` | `inert` |
| ignore, the formal that binds nothing | `#ignore` | `ignore` |
| integers, 64-bit, checked | `42` `-7` | `integer` |
| symbols, interned, case-sensitive | `foo` `$if` `<?` | `symbol` |
| pairs, immutable | `(1 . 2)` `(1 2 3)` | `pair` |
| strings, immutable atoms; escapes `\\` `\"` `\n` | `"hi"` | `string` |
| environments | `#[environment]` | `environment` |
| operatives | `#[operative (x) env body]`, `#[primitive $vau]`, `#[host name]` | `operative` |
| applicatives | `#[applicative #[operative (x) #ignore (* x x)]]` | `applicative` |

`;` starts a comment to the end of the line.

## Primitives (Rust)

All are applicatives except `$vau`.

| Combiner | Meaning |
|---|---|
| `($vau formals eformal body)` | An operative closing over the current environment (4.10.3). `formals` is a tree of symbols, `#ignore`, `()` and pairs, with no symbol twice; `eformal` is a symbol not in it, or `#ignore`. Checked when the operative is made (`bad-formals`). A call binds them in a new child of that environment and evaluates `body` as a tail call. |
| `(wrap c)`, `(unwrap a)` | The applicative around combiner `c`; the combiner inside applicative `a` (4.10.4–5). |
| `(eval x env)` | Evaluates `x` in `env`, as a tail call (4.8.3). |
| `(make-environment)`, `(make-environment parent)` | A new empty environment, with no parent or one (4.8.4). |
| `(bind! env formals value)` | Matches the tree `formals` to `value` and binds its symbols in `env` itself, replacing bindings there. Returns `#inert`. The matching of `$define!` (4.9.1) as an applicative. |
| `(bindings env)` | The symbols bound in `env` itself, in definition order. |
| `(cons a d)`, `(car p)`, `(cdr p)` | Pairs (4.6.3, 5.4.1). |
| `(eq? a b)` | Identity; integers, symbols and strings compare by value (4.2.1). |
| `(select b x y)` | `x` if `b` is `#t`, `y` if `#f`; a `type-error` otherwise. |
| `(type-of v)` | The symbol naming `v`'s type, as in the table above. |
| `(+ a b)`, `(- a b)`, `(* a b)`, `(div a b)`, `(<? a b)` | Binary integer arithmetic; `div` rounds toward negative infinity (12.5.8). |
| `(error code value ...)` | Stops evaluation; the host gets `Err` with `code` (a symbol) and the values. |

## The library (grift, [`library.grift`](../crates/grift/src/library.grift))

| Combiner | Meaning |
|---|---|
| `($define! formals expression)` | Evaluates `expression` and matches `formals` to it in the current environment (4.9.1). Returns `#inert`. |
| `($quote x)` | `x`, unevaluated. |
| `(list x ...)` | The list of the arguments (5.2.1). |
| `($if test consequent alternative)` | Both branches required; `test` must give a boolean (4.5.2). |
| `($sequence x ...)` | Each in turn; the last one's value, `#inert` for none (5.1.1). |
| `($lambda formals body ...)` | `(wrap ($vau formals #ignore body))`, with several body forms in a `$sequence` (5.3.2). |
| `(not? b)`, `($and? x ...)`, `($or? x ...)` | Booleans; `$and?` and `$or?` stop early and evaluate the last operand as a tail call (6.1). |
| `(=? a b)` | Integer equality (12.5.2). |
| `(apply a args)` | Calls applicative `a`'s combiner on the list `args`, in a new empty environment (5.5.1). |
| `(map a list)` | The list of `a` applied to each element (5.9.1). |
| `($let ((formals expression) ...) body ...)` | Binds in a new environment, then evaluates the body (5.10.1). |
| `($cond (test body ...) ...)` | The body of the first true test; `#inert` if none is true (5.6.1). |
| `(length list)`, `(append list ...)` | List length; concatenation, sharing the last list (6.3.1, 6.3.3). |
| `(equal? a b)` | Structural equality of pairs, `eq?` otherwise (6.6.1). |
| `ground-environment` | The environment holding all of the above. |

Programs run in the **top-level environment**, a child of the ground environment. Defining a
name at top level shadows the library's binding without changing what the library itself uses.

## Errors

Every error stops evaluation and goes to the host as a code and the values involved.

| Code | When | Values |
|---|---|---|
| `unbound-symbol` | a symbol with no binding | the symbol |
| `type-error` | a primitive got the wrong kind of value; an improper operand list | the primitive and the value |
| `arity-error` | a primitive got the wrong number of operands; a formals tree didn't match | the primitive and the operands; or the formals and the value |
| `not-a-combiner` | a combination's head isn't a combiner | the value |
| `bad-formals` | a malformed formals tree, a symbol twice, or a bad environment formal | the tree or formal |
| `overflow` | arithmetic beyond 64 bits | the primitive and the operands |
| `division-by-zero` | `div` by 0 | the operands |
| `out-of-memory` | the allocator refused | none |
| `parse-error` | the text doesn't read; unfinished text fails at its end | line and column |
| `stale-value` | the host used a value the collector reclaimed | none |
| any symbol | a program called `error` | the values given |

## Deviations from R-1RK, and why

- **One special primitive.** `$if`, `$define!` and `$sequence` are primitive in Kernel; here they
  are library code over `$vau`, `select`, `bind!` and `eval`. Everything that can be grift code
  is grift code.
- **No continuations, no error handlers, no `$let/cc`.** Errors go to the host; grift can't catch
  them. Limits and recovery belong to the host.
- **Integers only:** no numeric tower, no rationals or reals, no characters, vectors, ports or
  promises. Overflow is an error, not a bignum.
- **Binary arithmetic and comparison.** `+`, `*`, `<?` and `=?` take two operands; Kernel's take
  any number.
- **`div` floors.** R-1RK defines it for real arguments; for integers it is the floor of the
  quotient, so `(div -7 2)` is `-4`.
- **Strings are immutable atoms,** and `eq?` compares them by content, like integers.
- **`apply` has one form,** `(apply a args)`, with a new empty environment: Kernel's default,
  made the only form so nothing is optional. `map` takes one list.
- **`make-environment` takes at most one parent.**
- **`bind!` and `bindings`** are new: `bind!` is the matching of `$define!` as an applicative, so
  `$define!` can be library code; `bindings` lets a program, or an agent, list what exists.
- **`ground-environment`** names the ground environment so its bindings can be listed.
- **Printing a combiner shows its parameters and body.**
- **No `$` sigil rule:** `$` starting an operative's name is a convention, not checked.
