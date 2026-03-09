---
title: "Strings"
order: 6
---

# Strings

A character-pair node forming a linked list for strings.
A string is a linked list of `CharPair` nodes terminated by NIL,
exactly as a list is a linked list of `Cons` nodes terminated by NIL.
A single character is `CharPair { ch, cdr: NIL }` — a one-element string.

## Methods

## Contents

- [char_val](#char-val)
- [alloc_string](#alloc-string)
- [string_eq](#string-eq)
- [strings_equal](#strings-equal)
- [prepend_char](#prepend-char)
- [reverse_chain](#reverse-chain)
- [car_char](#car-char)
- [cdr_char](#cdr-char)
- [cadr_char](#cadr-char)
- [eval](#eval)
- [eval_to_index](#eval-to-index)
- [write_value](#write-value)
- [display_value](#display-value)
- [fmt_value](#fmt-value)
- [walk_chars](#walk-chars)
- [parse_string](#parse-string)
- [builtin_cons](#builtin-cons)
- [builtin_raw_read_string](#builtin-raw-read-string)
- [fmt_to_string](#fmt-to-string)
- [builtin_raw_display_to_string](#builtin-raw-display-to-string)
- [builtin_raw_write_to_string](#builtin-raw-write-to-string)
- [type_name](#type-name)

### char_val

Allocate a character (one-element string).

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### alloc_string

Allocate a string value from a `&str`.

Strings are stored as a linked list of `CharPair` nodes.
Each `CharPair { ch, cdr }` points to the next character,
with the final character's `cdr` pointing to `NIL`.
An empty string is represented as `NIL`.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### string_eq

Compare a `CharPair` linked list starting at `char_head` with a `&str`.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### strings_equal

Compare two arena-allocated strings by their `CharPair` chains.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### prepend_char

Prepend a character to the front of a CharPair chain.

Allocates a new `CharPair` node whose `cdr` points to `head`.
Returns the new head. This is purely functional — no existing
arena nodes are mutated.

Build strings by prepending characters in reverse order, then call
[`reverse_chain`](Self::reverse_chain) to flip into
forward order.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### reverse_chain

Reverse a singly-linked chain (Cons list or CharPair string)
in place by swapping `cdr` pointers.

Works with both `Cons` and `CharPair` chains. Returns the new
head (formerly the last node). No new nodes are allocated.

Safe because the chain is freshly built and not yet shared.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### car_char

Get car of a cons cell or CharPair (user-facing).
For CharPair, allocates a fresh one-element string.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### cdr_char

Get cdr of a cons cell or CharPair (user-facing).

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### cadr_char

Get car of cdr (second element of a list, user-facing).

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### eval

Parse and evaluate Lisp expression(s) in a string.

Multiple expressions are evaluated in sequence and the result of the
last one is returned.  The global environment persists across calls
so that bindings made by `define!` survive.

# Errors

Returns [`ArenaError`] on failure. Common variants include:
- [`ParseError`](ArenaError::ParseError) — malformed S-expression.
- [`UnboundVariable`](ArenaError::UnboundVariable) — undefined symbol.
- [`TypeError`](ArenaError::TypeError) — wrong type for an operation.
- [`OutOfMemory`](ArenaError::OutOfMemory) — arena exhausted even after GC.
- [`ArithmeticOverflow`](ArenaError::ArithmeticOverflow) — integer overflow.

# Example

```rust
use grift::{Lisp, Value};

let lisp: Lisp<20000> = Lisp::new();
assert_eq!(lisp.eval("(+ 1 2)"), Ok(Value::Number(3)));
```

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### eval_to_index

Parse and evaluate Lisp expression(s), returning the arena index.

Use with [`write_value`](Self::write_value) to properly display the
result, including walking symbol names, string contents, and lists.

# Errors

Returns [`ArenaError`] on parse or evaluation failure.

# Example

```rust
use grift::Lisp;

let lisp: Lisp<20000> = Lisp::new();
let idx = lisp.eval_to_index("(+ 10 20)").unwrap();
let mut buf = String::new();
lisp.write_value(idx, &mut buf).unwrap();
assert_eq!(buf, "30");
```

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### write_value

Write a human-readable representation of the value at `idx`.

Unlike `Value::Display`, this method has arena access and can walk
`CharPair` chains to display full symbol names and string contents,
and `Cons` chains to display proper/improper lists.

# Example

```rust
use grift::Lisp;

let lisp: Lisp<20000> = Lisp::new();
let idx = lisp.eval_to_index("(list 1 2 3)").unwrap();
let mut buf = String::new();
lisp.write_value(idx, &mut buf).unwrap();
assert_eq!(buf, "(1 2 3)");
```

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### display_value

Human-readable output (like Scheme `display`).

Same as [`write_value`](Self::write_value) except strings are printed
without surrounding quotes or escape sequences.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### fmt_value

Unified value formatter. When `display` is true, strings are printed
without quotes/escapes (like Scheme `display`); otherwise machine-
readable (like Scheme `write`).

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### walk_chars

Walk a CharPair chain, emitting each character via a closure.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### parse_string

Parse a string literal (opening `"` already consumed).

Processes escape sequences: `\n`, `\t`, `\r`, `\\`, `\"`.
Builds a CharPair chain in reverse, then reverses.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### builtin_cons

`(cons a b)` — cons cell construction.
When `a` is a single-character string (CharPair with cdr=NIL),
produces a CharPair node instead, so `(cons (car "h") "ello")` → `"hello"`.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### builtin_raw_read_string

`(raw-read-string str)` — parse one s-expression from a string.
Returns the parsed value or NIL if the string is empty.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### fmt_to_string

Format a value to an arena CharPair chain. Shared by
`raw-display-to-string` and `raw-write-to-string`.

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### builtin_raw_display_to_string

`(raw-display-to-string obj)` — display a value to a string (CharPair chain).

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### builtin_raw_write_to_string

`(raw-write-to-string obj)` — write a value to a string (CharPair chain).

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

### type_name

Returns the type name as a static string (for error messages).

# Example

```rust
use grift::Value;

assert_eq!(Value::Number(42).type_name(), "number");
assert_eq!(Value::Nil.type_name(), "nil");
assert_eq!(Value::Boolean(true).type_name(), "boolean");
```

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

## Builtins

### builtin_raw_display_to_string

**Name:** `raw-display-to-string`

**Signature:** `(raw-display-to-string obj)`

`(raw-display-to-string obj)` — display a value to a string (CharPair chain).

### builtin_raw_read_string

**Name:** `raw-read-string`

**Signature:** `(raw-read-string str)`

`(raw-read-string str)` — parse one s-expression from a string.
Returns the parsed value or NIL if the string is empty.

### builtin_raw_write_to_string

**Name:** `raw-write-to-string`

**Signature:** `(raw-write-to-string obj)`

`(raw-write-to-string obj)` — write a value to a string (CharPair chain).

**Related:** [Types](types.md#charpair) | [Built-ins](builtins.md) | [Examples](examples.md)

