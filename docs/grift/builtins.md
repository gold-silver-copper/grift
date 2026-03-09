---
title: "Built-in Functions"
order: 4
---

# Built-in Functions

Applicatives evaluate arguments before dispatch. The grouping below is derived from the builtin names present in the source tree.

## Contents

- [*](#builtin-mul)
- [+](#builtin-add)
- [-](#builtin-sub)
- [/](#builtin-div)
- [<](#builtin-lt)
- [<=](#builtin-le)
- [=](#builtin-eq)
- [>](#builtin-gt)
- [>=](#builtin-ge)
- [applicative?](#builtin-applicativep)
- [apply](#builtin-apply)
- [boolean?](#builtin-booleanp)
- [car](#builtin-car)
- [cdr](#builtin-cdr)
- [cons](#builtin-cons)
- [environment?](#builtin-environmentp)
- [eq?](#builtin-eqp)
- [equal?](#builtin-equalp)
- [error](#builtin-error)
- [eval](#builtin-eval)
- [gc-collect](#builtin-gc-collect)
- [ignore?](#builtin-ignorep)
- [inert?](#builtin-inertp)
- [list](#builtin-list)
- [make-empty-environment](#builtin-make-empty-env)
- [make-environment](#builtin-make-env)
- [not](#builtin-not)
- [null?](#builtin-nullp)
- [number?](#builtin-numberp)
- [operative?](#builtin-operativep)
- [pair?](#builtin-pairp)
- [raw-display-to-string](#builtin-raw-display-to-string)
- [raw-read-string](#builtin-raw-read-string)
- [raw-write-to-string](#builtin-raw-write-to-string)
- [symbol?](#builtin-symbolp)
- [unwrap](#builtin-unwrap)
- [wrap](#builtin-wrap)

## *

### builtin_mul

**Name:** `*`

**Signature:** `(* ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_mul`

**Tail-call optimized:** No

`(* ...)` — variadic multiplication.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## +

### builtin_add

**Name:** `+`

**Signature:** `(+ ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_add`

**Tail-call optimized:** No

`(+ ...)` — variadic addition.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Other

### builtin_sub

**Name:** `-`

**Signature:** `(- a b ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_sub`

**Tail-call optimized:** No

`(- a b ...)` — subtraction. With one arg, negates.

#### Errors

- `ArenaError::InvalidArgument`

**Related:** [Types](types.md) | [Error](errors.md#invalidargument) | [Examples](examples.md)

## /

### builtin_div

**Name:** `/`

**Signature:** `(/ a b)`

**Kind:** Applicative

**Dispatch method:** `builtin_div`

**Tail-call optimized:** No

`(/ a b)` — integer division.

#### Errors

- `ArenaError::DivisionByZero`

**Related:** [Types](types.md) | [Error](errors.md#divisionbyzero) | [Examples](examples.md)

## <

### builtin_lt

**Name:** `<`

**Signature:** `(< ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_lt`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## <=

### builtin_le

**Name:** `<=`

**Signature:** `(<= ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_le`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## =

### builtin_eq

**Name:** `=`

**Signature:** `(= ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_eq`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## >

### builtin_gt

**Name:** `>`

**Signature:** `(> ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_gt`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## >=

### builtin_ge

**Name:** `>=`

**Signature:** `(>= ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_ge`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Applicative

### builtin_applicativep

**Name:** `applicative?`

**Signature:** `(applicative? ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_applicativep`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Apply

### builtin_apply

**Name:** `apply`

**Signature:** `(apply combiner args)`

**Kind:** Applicative

**Dispatch method:** `builtin_apply`

**Tail-call optimized:** No

`(apply combiner args)` — apply a combiner to a list of arguments.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Boolean

### builtin_booleanp

**Name:** `boolean?`

**Signature:** `(boolean? ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_booleanp`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Car

### builtin_car

**Name:** `car`

**Signature:** `(car ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_car`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Cdr

### builtin_cdr

**Name:** `cdr`

**Signature:** `(cdr ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_cdr`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Cons

### builtin_cons

**Name:** `cons`

**Signature:** `(cons a b)`

**Kind:** Applicative

**Dispatch method:** `builtin_cons`

**Tail-call optimized:** No

`(cons a b)` — cons cell construction.
When `a` is a single-character string (CharPair with cdr=NIL),
produces a CharPair node instead, so `(cons (car "h") "ello")` → `"hello"`.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Environment

### builtin_environmentp

**Name:** `environment?`

**Signature:** `(environment? ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_environmentp`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Eq

### builtin_eqp

**Name:** `eq?`

**Signature:** `(eq? object1 object2)`

**Kind:** Applicative

**Dispatch method:** `builtin_eqp`

**Tail-call optimized:** No

`(eq? object1 object2)` — identity predicate (§4.2.1).

Returns `#t` iff the two objects are effectively the same object.
For immutable, encapsulated types (booleans, nil, inert, symbols),
eq? is determined by value. For mutable/constructed objects (pairs,
environments), eq? compares arena identity.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Equal

### builtin_equalp

**Name:** `equal?`

**Signature:** `(equal? object1 object2)`

**Kind:** Applicative

**Dispatch method:** `builtin_equalp`

**Tail-call optimized:** No

`(equal? object1 object2)` — structural equality predicate (§4.3.1).

Returns `#t` iff the two objects "look" the same as long as nothing
is mutated. Weaker than eq?; equal? returns true whenever eq? would.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Error

### builtin_error

**Name:** `error`

**Signature:** `(error msg)`

**Kind:** Applicative

**Dispatch method:** `builtin_error`

**Tail-call optimized:** No

`(error msg)` — signal an error.

#### Errors

- `ArenaError::InvalidArgument`

**Related:** [Types](types.md) | [Error](errors.md#invalidargument) | [Examples](examples.md)

## Eval

### builtin_eval

**Name:** `eval`

**Signature:** `(eval expr env)`

**Kind:** Applicative

**Dispatch method:** `builtin_eval`

**Tail-call optimized:** No

`(eval expr env)` — evaluate expression in given environment.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Gc

### builtin_gc_collect

**Name:** `gc-collect`

**Signature:** `(gc-collect)`

**Kind:** Applicative

**Dispatch method:** `builtin_gc_collect`

**Tail-call optimized:** No

`(gc-collect)` — manually trigger garbage collection.
Returns the number of objects collected. Always runs unconditionally
(ignores the gc-enabled flag), since the user explicitly requested it.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Ignore

### builtin_ignorep

**Name:** `ignore?`

**Signature:** `(ignore? ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_ignorep`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Inert

### builtin_inertp

**Name:** `inert?`

**Signature:** `(inert? ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_inertp`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## List

### builtin_list

**Name:** `list`

**Signature:** `(list ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_list`

**Tail-call optimized:** No

`(list ...)` — return args as-is (already evaluated).

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Make

### builtin_make_empty_env

**Name:** `make-empty-environment`

**Signature:** `(make-empty-environment)`

**Kind:** Applicative

**Dispatch method:** `builtin_make_empty_env`

**Tail-call optimized:** No

`(make-empty-environment)` — always creates a parentless environment.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### builtin_make_env

**Name:** `make-environment`

**Signature:** `(make-environment . environments)`

**Kind:** Applicative

**Dispatch method:** `builtin_make_env`

**Tail-call optimized:** No

`(make-environment . environments)` — create a new environment with
zero or more parent environments (Kernel §4.8.4).

The parents list is copied so that subsequent mutation of the
argument list does not affect the constructed environment.

#### Errors

- `ArenaError::TypeError`

**Related:** [Types](types.md) | [Error](errors.md#typeerror) | [Examples](examples.md)

## Not

### builtin_not

**Name:** `not`

**Signature:** `(not boolean)`

**Kind:** Applicative

**Dispatch method:** `builtin_not`

**Tail-call optimized:** No

`(not boolean)` — boolean negation (requires exactly one boolean arg).

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Null

### builtin_nullp

**Name:** `null?`

**Signature:** `(null? ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_nullp`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Number

### builtin_numberp

**Name:** `number?`

**Signature:** `(number? ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_numberp`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Operative

### builtin_operativep

**Name:** `operative?`

**Signature:** `(operative? ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_operativep`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Pair

### builtin_pairp

**Name:** `pair?`

**Signature:** `(pair? ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_pairp`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Raw

### builtin_raw_display_to_string

**Name:** `raw-display-to-string`

**Signature:** `(raw-display-to-string obj)`

**Kind:** Applicative

**Dispatch method:** `builtin_raw_display_to_string`

**Tail-call optimized:** No

`(raw-display-to-string obj)` — display a value to a string (CharPair chain).

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### builtin_raw_read_string

**Name:** `raw-read-string`

**Signature:** `(raw-read-string str)`

**Kind:** Applicative

**Dispatch method:** `builtin_raw_read_string`

**Tail-call optimized:** No

`(raw-read-string str)` — parse one s-expression from a string.
Returns the parsed value or NIL if the string is empty.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

### builtin_raw_write_to_string

**Name:** `raw-write-to-string`

**Signature:** `(raw-write-to-string obj)`

**Kind:** Applicative

**Dispatch method:** `builtin_raw_write_to_string`

**Tail-call optimized:** No

`(raw-write-to-string obj)` — write a value to a string (CharPair chain).

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Symbol

### builtin_symbolp

**Name:** `symbol?`

**Signature:** `(symbol? ...)`

**Kind:** Applicative

**Dispatch method:** `builtin_symbolp`

**Tail-call optimized:** No

> No documentation found.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Unwrap

### builtin_unwrap

**Name:** `unwrap`

**Signature:** `(unwrap applicative)`

**Kind:** Applicative

**Dispatch method:** `builtin_unwrap`

**Tail-call optimized:** No

`(unwrap applicative)` — extract the underlying combiner.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

## Wrap

### builtin_wrap

**Name:** `wrap`

**Signature:** `(wrap combiner)`

**Kind:** Applicative

**Dispatch method:** `builtin_wrap`

**Tail-call optimized:** No

`(wrap combiner)` — wrap an operative into an applicative.

**Related:** [Types](types.md) | [Errors](errors.md) | [Examples](examples.md)

