---
title: "Environments"
order: 5
---

# Environments

The `Lisp` struct: arena wrapper with symbol interning and convenience methods.

[`Lisp`] is the top-level entry point for the interpreter. It owns the
fixed-size [`Arena`](crate::arena::Arena), pre-allocates singleton values
and environments, manages symbol interning, and exposes the public
[`eval`](Lisp::eval) API.

## Slot Layout

Slots 0–9 are reserved at construction time for well-known values
whose [`ArenaIndex`](crate::arena::ArenaIndex) constants are compile-time
values:

| Slot | Contents |
|------|----------|
| 0 | `Nil` |
| 1 | `Boolean(true)` |
| 2 | `Boolean(false)` |
| 3 | `Inert` |
| 4 | `Ignore` |
| 5 | Ground environment |
| 6 | Parents cons cell (ground → global) |
| 7 | Global environment |
| 8 | GC root stack head |
| 9 | Symbol intern list head |

## Methods

## Contents

- [make_env](#make-env)
- [make_child_env](#make-child-env)
- [env_define](#env-define)
- [env_set](#env-set)
- [env_lookup](#env-lookup)
- [env_lookup_dfs](#env-lookup-dfs)
- [env_lookup_dfs_parents](#env-lookup-dfs-parents)
- [op_current_env](#op-current-env)
- [builtin_make_env](#builtin-make-env)
- [builtin_make_empty_env](#builtin-make-empty-env)

### make_env

Create a new environment with a parents list (cons-list of parent
environments, or NIL for a root environment).

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

### make_child_env

Create a child environment with the given single parent.

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

### env_define

Define a binding in an environment (mutates in place via arena.set).
Returns `AlreadyDefined` if a binding for `name` already exists in this frame.

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

### env_set

Set an existing binding in an environment's own frame.
Does NOT walk parents. Returns `UnboundVariable` if not found.

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

### env_lookup

Look up a symbol in an environment.

Fast path: walks single-parent chains with zero arena allocation.
Falls back to DFS with cycle detection only for multi-parent
environments (created by `make-environment`).

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

### env_lookup_dfs

Depth-first lookup with a visited-set (cons-list of env indices
already searched).  Only used for multi-parent environments.

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

### env_lookup_dfs_parents

Search a list of parent environments via DFS.

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

### op_current_env

`(current-environment)` — return the caller's environment.

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

### builtin_make_env

`(make-environment . environments)` — create a new environment with
zero or more parent environments (Kernel §4.8.4).

The parents list is copied so that subsequent mutation of the
argument list does not affect the constructed environment.

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

### builtin_make_empty_env

`(make-empty-environment)` — always creates a parentless environment.

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

## Builtins

### op_current_env

**Name:** `current-environment`

**Signature:** `(current-environment)`

`(current-environment)` — return the caller's environment.

### builtin_environmentp

**Name:** `environment?`

**Signature:** `(environment? ...)`

### builtin_make_empty_env

**Name:** `make-empty-environment`

**Signature:** `(make-empty-environment)`

`(make-empty-environment)` — always creates a parentless environment.

### builtin_make_env

**Name:** `make-environment`

**Signature:** `(make-environment . environments)`

`(make-environment . environments)` — create a new environment with
zero or more parent environments (Kernel §4.8.4).

The parents list is copied so that subsequent mutation of the
argument list does not affect the constructed environment.

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

