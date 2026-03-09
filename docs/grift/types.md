---
title: "Types"
order: 2
---

# Value Types

Lisp value type.

Defines the [`Value`] enum representing all first-class Lisp types,
along with [`BuiltinId`] for identifying primitive operatives.

## Design Constraints

Every `Value` variant fits in a single arena slot and may inline at
most two `ArenaIndex`-sized fields. Larger structures (parameter
trees, environment chains) are built as linked lists of `Cons` or
`CharPair` nodes in the arena.

## Summary

| Type | Fields | Description |
| --- | --- | --- |
| `Nil` | None | The empty list / nil. |
| `Boolean` | 1 field | Boolean value (`#t` or `#f`). |
| `Number` | 1 field | Integer number (machine-width signed integer). |
| `Symbol` | 1 field | A symbol, pointing to the first `CharPair` node of its name. |
| `Cons` | 2 fields | A cons cell (pair) with inline car and cdr. |
| `CharPair` | 2 fields | A character-pair node forming a linked list for strings. |
| `Operative` | 2 fields | Compound operative (vau closure / fexpr). |
| `Applicative` | 1 field | Applicative wrapper: evaluates arguments, then calls inner combiner.
Created by `(wrap combiner)`. |
| `Builtin` | 1 field | Rust-native primitive operative. |
| `Environment` | 2 fields | A first-class environment with lexical parent chain. |
| `Inert` | None | The inert value, written `#inert`.
Returned by combiners whose primary purpose is side-effect (e.g. |
| `Ignore` | None | The ignore value, written `#ignore`. |
| `Prelude` | 1 field | Prelude function (static memory, parsed on demand). |
| `Native` | 1 field | User-registered native function (Rust function pointer).
Always wrapped in an `Applicative` when registered. |

## Contents

- [Nil](#nil)
- [Boolean](#boolean)
- [Number](#number)
- [Symbol](#symbol)
- [Cons](#cons)
- [CharPair](#charpair)
- [Operative](#operative)
- [Applicative](#applicative)
- [Builtin](#builtin)
- [Environment](#environment)
- [Inert](#inert)
- [Ignore](#ignore)
- [Prelude](#prelude)
- [Native](#native)

### Nil

The empty list / nil.

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Boolean

Boolean value (`#t` or `#f`).

| Field | Type | Description |
| --- | --- | --- |
| `_` | `bool` | — |

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Number

Integer number (machine-width signed integer).

| Field | Type | Description |
| --- | --- | --- |
| `_` | `isize` | — |

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Symbol

A symbol, pointing to the first `CharPair` node of its name.

| Field | Type | Description |
| --- | --- | --- |
| `_` | `ArenaIndex` | — |

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Cons

A cons cell (pair) with inline car and cdr.

| Field | Type | Description |
| --- | --- | --- |
| `car` | `ArenaIndex` | Head element of the pair. |
| `cdr` | `ArenaIndex` | Tail element of the pair (next `Cons` node in a proper list, `NIL` at the end, or any value in a dotted pair). |

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### CharPair

A character-pair node forming a linked list for strings.
A string is a linked list of `CharPair` nodes terminated by NIL,
exactly as a list is a linked list of `Cons` nodes terminated by NIL.
A single character is `CharPair { ch, cdr: NIL }` — a one-element string.

| Field | Type | Description |
| --- | --- | --- |
| `ch` | `char` | The Unicode character stored at this position. |
| `cdr` | `ArenaIndex` | Next `CharPair` node, or `NIL` for the last character. |

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Operative

Compound operative (vau closure / fexpr).
Created by `(vau params env-param body)`.

| Field | Type | Description |
| --- | --- | --- |
| `params_envparam` | `ArenaIndex` | Cons cell packing the formal parameter tree and environment parameter. |
| `body_env` | `ArenaIndex` | Cons cell packing the body expression and the closed-over environment. |

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Applicative

Applicative wrapper: evaluates arguments, then calls inner combiner.
Created by `(wrap combiner)`. The inner combiner is any callable:
Operative, Builtin, or even another Applicative.

| Field | Type | Description |
| --- | --- | --- |
| `_` | `ArenaIndex` | — |

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Builtin

Rust-native primitive operative.
Always an operative — receives unevaluated args + caller env.
Applicative primitives (like +) are (wrap (Builtin id)) at init time.

| Field | Type | Description |
| --- | --- | --- |
| `_` | `BuiltinId` | — |

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Environment

A first-class environment with lexical parent chain.

| Field | Type | Description |
| --- | --- | --- |
| `bindings` | `ArenaIndex` | Alist of `(symbol . |
| `parents` | `ArenaIndex` | Cons-list of parent environments, or NIL for top-level. |

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Inert

The inert value, written `#inert`.
Returned by combiners whose primary purpose is side-effect (e.g. `$define!`).

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Ignore

The ignore value, written `#ignore`.
Used specifically for parameter matching in formal parameter trees.

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Prelude

Prelude function (static memory, parsed on demand).

| Field | Type | Description |
| --- | --- | --- |
| `_` | `Prelude` | — |

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

### Native

User-registered native function (Rust function pointer).
Always wrapped in an `Applicative` when registered. The function
pointer is stored directly, with no const generic dependency.

| Field | Type | Description |
| --- | --- | --- |
| `_` | `NativeFn` | — |

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Errors](errors.md)

