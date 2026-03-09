---
title: "Error Types"
order: 8
---

# Error Types

Errors that can occur during arena and Lisp operations.

Each variant captures a specific failure mode, enabling precise
diagnostics without heap-allocated error messages.

## Summary

| Variant | Raised when | Raised in |
| --- | --- | --- |
| `OutOfMemory` | Arena is full, cannot allocate more cells. | `alloc`, `alloc_string`, `eval_expr` |
| `IndexOutOfBounds` | Index exceeds the arena's capacity (>= N). | `get`, `validate_index` |
| `IndexNotAllocated` | Index refers to a slot that has been freed or was never allocated. | `get`, `validate_index` |
| `InvalidArgument` | An argument to an arena operation was invalid (e.g., zero-length contiguous allocation). | `builtin_error`, `builtin_sub`, `parse_string`, `validate_ptree_inner` |
| `TraceError` | An error occurred during garbage collection tracing. | — |
| `Cyclic` | Cycle detected during structure traversal (e.g., graph traversal | `validate_ptree_inner` |
| `TypeError` | A value had the wrong type for the requested operation | `builtin_make_env`, `car`, `car_char`, `cdr`, `cdr_char` |
| `ParseError` | A parse error occurred while reading an S-expression. | — |
| `ArithmeticOverflow` | Checked arithmetic overflowed (e.g., addition, negation). | — |
| `DivisionByZero` | Division or modulo by zero. | `builtin_div` |
| `UnboundVariable` | A variable was not found in the current or global environment. | `env_lookup`, `env_lookup_dfs`, `env_lookup_dfs_parents`, `env_set` |
| `NotCallable` | Attempted to call a value that is not a function (lambda or builtin). | `apply_combiner`, `eval_step` |
| `ImmutableEnvironment` | Attempted to mutate an immutable environment (e.g., the ground environment). | — |
| `AlreadyDefined` | Attempted to define a variable that already has a binding in the current frame. | `env_define` |

## Contents

- [OutOfMemory](#outofmemory)
- [IndexOutOfBounds](#indexoutofbounds)
- [IndexNotAllocated](#indexnotallocated)
- [InvalidArgument](#invalidargument)
- [TraceError](#traceerror)
- [Cyclic](#cyclic)
- [TypeError](#typeerror)
- [ParseError](#parseerror)
- [ArithmeticOverflow](#arithmeticoverflow)
- [DivisionByZero](#divisionbyzero)
- [UnboundVariable](#unboundvariable)
- [NotCallable](#notcallable)
- [ImmutableEnvironment](#immutableenvironment)
- [AlreadyDefined](#alreadydefined)

### OutOfMemory

Arena is full, cannot allocate more cells.

**Raised in:**

- `alloc`
- `alloc_string`
- `eval_expr`

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### IndexOutOfBounds

Index exceeds the arena's capacity (>= N).

**Raised in:**

- `get`
- `validate_index`

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### IndexNotAllocated

Index refers to a slot that has been freed or was never allocated.

**Raised in:**

- `get`
- `validate_index`

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### InvalidArgument

An argument to an arena operation was invalid (e.g., zero-length contiguous allocation).

**Raised in:**

- `builtin_error`
- `builtin_sub`
- `parse_string`
- `validate_ptree_inner`

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### TraceError

An error occurred during garbage collection tracing.
This can happen if the mark stack overflows or roots are invalid.

> No call-sites found for this error variant.

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### Cyclic

Cycle detected during structure traversal (e.g., graph traversal
or recursive data structure operations).

**Raised in:**

- `validate_ptree_inner`

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### TypeError

A value had the wrong type for the requested operation
(e.g., expected a Number but found a Cons).

**Raised in:**

- `builtin_make_env`
- `car`
- `car_char`
- `cdr`
- `cdr_char`
- `env_define`
- `env_lookup`
- `env_lookup_dfs`
- `env_set`
- `extract_arg`
- `from_lisp`
- `match_ptree`
- `reverse_chain`
- `strings_equal`
- `validate_ptree_inner`
- `vau_parts`

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### ParseError

A parse error occurred while reading an S-expression.

Carries the 1-based line and column where the error was detected.

| Field | Type | Description |
| --- | --- | --- |
| `line` | `u32` | 1-based line number in the source text. |
| `col` | `u32` | 1-based column number in the source text. |

> No call-sites found for this error variant.

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### ArithmeticOverflow

Checked arithmetic overflowed (e.g., addition, negation).

> No call-sites found for this error variant.

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### DivisionByZero

Division or modulo by zero.

**Raised in:**

- `builtin_div`

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### UnboundVariable

A variable was not found in the current or global environment.

**Raised in:**

- `env_lookup`
- `env_lookup_dfs`
- `env_lookup_dfs_parents`
- `env_set`

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### NotCallable

Attempted to call a value that is not a function (lambda or builtin).

**Raised in:**

- `apply_combiner`
- `eval_step`

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### ImmutableEnvironment

Attempted to mutate an immutable environment (e.g., the ground environment).

> No call-sites found for this error variant.

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

### AlreadyDefined

Attempted to define a variable that already has a binding in the current frame.

**Raised in:**

- `env_define`

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md) | [Types](types.md)

