---
title: "Garbage Collection"
order: 7
---

# Garbage Collection

# Fixed-Size Arena Allocator

A minimal `no_std`, `no_alloc` arena allocator with fixed capacity,
designed for embedded and resource-constrained environments where heap
allocation is unavailable or undesirable.

## Features

- **Fixed-size**: All memory pre-allocated at compile time via const generics
- **No-std, no-alloc**: Works in embedded environments with no heap
- **Generic**: Works with any `Copy` type
- **Interior mutability**: Safe access via `Cell` (no runtime borrow checking overhead)
- **O(1) allocation**: Free-list based allocation and deallocation
- **Mark-and-sweep GC**: Trait-based garbage collection via [`Trace`]
- **Zero dependencies**: Only uses `core::cell::Cell`

## Safety Guarantees

This crate uses `#![forbid(unsafe_code)]` — there is no `unsafe` anywhere
in the implementation. Interior mutability is achieved through `Cell<T>`
rather than raw pointers, and all indices are bounds-checked before access.

## Example

```rust
use grift::arena::{Arena, ArenaIndex};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Node {
    Leaf(isize),
    Branch(ArenaIndex, ArenaIndex),
}

let arena: Arena<Node, 1024> = Arena::new(Node::Leaf(0));

// Allocate nodes
let left = arena.alloc(Node::Leaf(1)).unwrap();
let right = arena.alloc(Node::Leaf(2)).unwrap();
let root = arena.alloc(Node::Branch(left, right)).unwrap();

// Access nodes
if let Node::Branch(l, r) = arena.get(root).unwrap() {
    println!("Left: {:?}, Right: {:?}", arena.get(l), arena.get(r));
}

// Free when done
arena.free(root).unwrap();
```

## Singletons

### `GC_ROOTS`

Slot `8`

The GC_ROOTS index - points to slot 8 where the GC root stack head
is stored.  This is a cons cell whose `car` holds the current head
of the GC roots linked list and whose `cdr` is always NIL.

## Methods

## Contents

- [baseline_allocated](#baseline-allocated)
- [collect_garbage](#collect-garbage)
- [collect_with_roots](#collect-with-roots)
- [is_trace_error](#is-trace-error)
- [initialize_roots](#initialize-roots)
- [process_mark_stack](#process-mark-stack)
- [sweep_unmarked](#sweep-unmarked)
- [mark_and_sweep](#mark-and-sweep)
- [collect_garbage_multi](#collect-garbage-multi)
- [gc_roots_head](#gc-roots-head)
- [set_gc_roots_head](#set-gc-roots-head)
- [push_root](#push-root)
- [pop_roots](#pop-roots)
- [eval_collect_garbage](#eval-collect-garbage)
- [eval_collect_garbage_unconditional](#eval-collect-garbage-unconditional)
- [eval_expr](#eval-expr)
- [builtin_gc_collect](#builtin-gc-collect)

### baseline_allocated

Return the number of arena slots consumed by the ground environment
and builtins in a freshly constructed `Lisp` — before any user
expressions are evaluated.

**Note:** this method triggers a garbage collection cycle to measure
the surviving allocation count. It should not be called in
performance-sensitive code paths.

Useful for computing test budgets: `baseline + constant` rather
than a magic number.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### collect_garbage

Perform mark-and-sweep garbage collection.

Starting from the given `roots`, marks all reachable objects by
following `ArenaIndex` references (via the `Trace` trait), then
frees all unmarked (unreachable) objects.

# Algorithm

1. **Mark phase**: Starting from roots, recursively mark all reachable
   objects. Handles cycles correctly by checking if already marked.
2. **Sweep phase**: Iterate through all slots and free any that are
   allocated but not marked.

# Returns

Returns `GcStats` with information about what was collected.

# Complexity

- Time: O(reachable + N) where N is arena capacity
- Space: O(N) for the mark bitmap

# Example

```rust
use grift::arena::{Arena, ArenaIndex, Trace};

#[derive(Clone, Copy)]
struct Node {
    value: isize,
    next: Option<ArenaIndex>,
}

impl<const N: usize> Trace<Node, N> for Node {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        if let Some(next) = self.next {
            tracer(next);
        }
    }
}

let arena: Arena<Node, 10> = Arena::new(Node { value: 0, next: None });

// Create a linked list: root -> n1 -> n2
let n2 = arena.alloc(Node { value: 3, next: None }).unwrap();
let n1 = arena.alloc(Node { value: 2, next: Some(n2) }).unwrap();
let root = arena.alloc(Node { value: 1, next: Some(n1) }).unwrap();

// Create some garbage
let _garbage = arena.alloc(Node { value: -1, next: None }).unwrap();

// Collect with root as the only GC root
let stats = arena.collect_garbage(&[root]);

assert_eq!(stats.collected, 1);
assert_eq!(arena.len(), 3);
```

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### collect_with_roots

Collect garbage, always protecting the macro-generated singleton
root set plus any caller-supplied `extra_roots`.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### is_trace_error

Check if this error is related to garbage collection.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### initialize_roots

Initialize roots into the mark stack.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### process_mark_stack

Process the mark stack using depth-first traversal.

This iteratively processes children in batches to avoid stack overflow
when objects have many children. Uses iterative batching to ensure all
children are processed even when there are more than 16 per object.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### sweep_unmarked

Sweep phase: free all unmarked but allocated slots in a single pass.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### mark_and_sweep

Shared mark-and-sweep implementation.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### collect_garbage_multi

Perform garbage collection with multiple root sets.

This iterates through all provided root sets and marks objects
reachable from any of them.

# Note

For no-alloc compatibility, this method iterates through root sets
sequentially rather than flattening them. This has the same effect
but uses constant stack space.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### gc_roots_head

Read the current head of the GC root stack.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### set_gc_roots_head

Set the head of the GC root stack.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### push_root

Push a value onto the GC root stack so it survives collection.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### pop_roots

Pop `n` values from the GC root stack.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### eval_collect_garbage

Trigger garbage collection using all known live roots. Used for OOM collections.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### eval_collect_garbage_unconditional

Trigger garbage collection unconditionally (ignores gc_enabled flag).
Used by the `gc-collect` builtin for explicit manual collection.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### eval_expr

Evaluate an expression in an environment (with TCO).

Kernel-style dispatch: three combiner types —
`Operative` (compound fexpr), `Applicative` (wrapper that evals args),
and `Builtin` (primitive operative).

Garbage collection is triggered only on allocation failure (OOM):
when any operation returns `OutOfMemory`, the evaluator restores
the GC root stack, collects garbage, and retries.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

### builtin_gc_collect

`(gc-collect)` — manually trigger garbage collection.
Returns the number of objects collected. Always runs unconditionally
(ignores the gc-enabled flag), since the user explicitly requested it.

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

**Related:** [Errors](errors.md) | [Types](types.md) | [Examples](examples.md)

