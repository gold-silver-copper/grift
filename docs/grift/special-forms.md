---
title: "Special Forms"
order: 3
---

# Special Forms

Operatives receive unevaluated arguments. Entries and signatures are derived from the builtin declarations in source.

## Contents

- [quote](#op-quote)
- [if](#op-if)
- [define!](#op-define)
- [fn!](#op-fn-define)
- [set!](#op-set)
- [lambda](#op-lambda)
- [begin](#op-begin)
- [cond](#op-cond)
- [and](#op-and)
- [or](#op-or)
- [let](#op-let)
- [vau](#op-vau)
- [current-environment](#op-current-env)

### op_quote

**Name:** `quote`

**Signature:** `(quote expr)`

**Kind:** Operative

**Dispatch method:** `op_quote`

**Tail-call optimized:** Yes

`(quote expr)` — return the expression unevaluated.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_if

**Name:** `if`

**Signature:** `(if test then else)`

**Kind:** Operative

**Dispatch method:** `op_if`

**Tail-call optimized:** Yes

`(if test then else)` — test is strict, branches are tail positions.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_define

**Name:** `define!`

**Signature:** `($define! definiend expression)`

**Kind:** Operative

**Dispatch method:** `op_define`

**Tail-call optimized:** No

`($define! definiend expression)` — Kernel §4.9.1.

Evaluates `expression` in the dynamic environment and matches `definiend`
(a formal parameter tree) to the result, binding symbols in the dynamic
environment.  Returns `#inert`.

It is an error to define a variable that already has a binding in the
current environment frame. Use `set!` to update an existing binding.

Per Kernel §3.2, mutation of the ground environment or its ancestors
is forbidden.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_fn_define

**Name:** `fn!`

**Signature:** `(fn! name params body...)`

**Kind:** Operative

**Dispatch method:** `op_fn_define`

**Tail-call optimized:** No

`(fn! name params body...)` — define a named function.

Syntactic sugar for `(define! name (lambda params (begin body...)))`.
`name` is a symbol, `params` is a formal parameter tree, and `body`
is one or more body expressions.

Returns `#inert`.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_set

**Name:** `set!`

**Signature:** `($set! exp1 formals exp2)`

**Kind:** Operative

**Dispatch method:** `op_set`

**Tail-call optimized:** Yes

`($set! exp1 formals exp2)` — Kernel §6.8.1.

Evaluates `exp1` and `exp2` in the dynamic environment; call the
results `env` and `obj`.  If `env` is not an environment, an error
is signaled.  Then the operative updates an existing binding of
`formals` (a single symbol) in environment `env`.  The symbol must
already exist in `env`'s own frame — parents are not walked, and a
new binding is never created.  Returns `#inert`.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_lambda

**Name:** `lambda`

**Signature:** `(lambda (params...) body...)`

**Kind:** Operative

**Dispatch method:** `op_lambda`

**Tail-call optimized:** Yes

`(lambda (params...) body...)`.
Derived: `lambda = wrap(vau(params, #ignore, body))`.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_begin

**Name:** `begin`

**Signature:** `(begin expr1 expr2 ...)`

**Kind:** Operative

**Dispatch method:** `op_begin`

**Tail-call optimized:** Yes

`(begin expr1 expr2 ...)` — all but last are non-tail, last is tail.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_cond

**Name:** `cond`

**Signature:** `(cond (test expr...) ...)`

**Kind:** Operative

**Dispatch method:** `op_cond`

**Tail-call optimized:** Yes

`(cond (test expr...) ...)` — tests are strict, last body expr is tail.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_and

**Name:** `and`

**Signature:** `(and expr1 expr2 ...)`

**Kind:** Operative

**Dispatch method:** `op_and`

**Tail-call optimized:** Yes

`(and expr1 expr2 ...)` — strict on tests, last is tail.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_or

**Name:** `or`

**Signature:** `(or expr1 expr2 ...)`

**Kind:** Operative

**Dispatch method:** `op_or`

**Tail-call optimized:** Yes

`(or expr1 expr2 ...)` — strict on tests, last is tail.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_let

**Name:** `let`

**Signature:** `(let ((name val) ...) body...)`

**Kind:** Operative

**Dispatch method:** `op_let`

**Tail-call optimized:** Yes

`(let ((name val) ...) body...)` — bindings are strict, body is tail.

Named let: `(let name ((param init) ...) body...)` desugars to a
recursive function `name` with params bound to evaluated inits.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_vau

**Name:** `vau`

**Signature:** `(vau params env-param body)`

**Kind:** Operative

**Dispatch method:** `op_vau`

**Tail-call optimized:** No

`(vau params env-param body)` — create a fexpr (operative).

Per the Kernel spec (§4.10.3):
- `params` must be a valid formal parameter tree.
- `env-param` must be either a symbol or `#ignore`.
- If `env-param` is a symbol that also occurs in `params`, an error
  is signaled.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### op_current_env

**Name:** `current-environment`

**Signature:** `(current-environment)`

**Kind:** Operative

**Dispatch method:** `op_current_env`

**Tail-call optimized:** No

`(current-environment)` — return the caller's environment.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

