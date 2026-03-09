---
title: "Examples"
order: 9
---

# Examples

Example content discovered from source files and project examples.

## Contents

- [map](#map)
- [filter](#filter)
- [length](#length)
- [append](#append)
- [Fib Bench](#fib-bench)
- [Prelude](#prelude)

## Prelude Entries

### map

```scheme
(fn! map (f lst)
```

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md)

### filter

```scheme
(fn! filter (pred lst)
```

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md)

### length

```scheme
(fn! length (lst)
```

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md)

### append

```scheme
(fn! append (a b)
```

**Related:** [Built-ins](builtins.md) | [Special Forms](special-forms.md)

## Discovered Example Files

### Fib Bench

**Source:** `fib_bench.rs`

Comprehensive benchmark suite for the Grift Lisp interpreter. Tests naive fib, TCO fib, iterative fib, arithmetic, list operations, higher-order functions, closures, recursion patterns, and more.

```sh
cargo run -p grift --example fib_bench --release
```

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

### Prelude

**Source:** `prelude.grift`

Discovered from project `.grift` source.

```scheme
;; Grift Prelude
;; This file is read at compile time by the include_prelude! proc macro.
;; Function definitions become Prelude statics (parsed on demand).
;; Constants become Rust init code (bound at startup).

;; Functions
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

**Related:** [Built-ins](builtins.md) | [Types](types.md) | [Errors](errors.md)

