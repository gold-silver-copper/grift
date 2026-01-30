# Arena Allocator Architecture

This document describes the architecture, design decisions, and implementation details of the `pwn_arena` arena allocator.

## Overview

`pwn_arena` is a fixed-size, `no_std`, `no_alloc` arena allocator designed for use in embedded systems, WebAssembly, and other environments where heap allocation is unavailable or undesirable.

## Core Design Principles

### 1. No Heap Allocation

The arena is entirely stack-allocated or static. All memory is pre-allocated at compile time via const generics:

```rust
// Allocate 1024 slots at compile time
let arena: Arena<Value, 1024> = Arena::new(default_value);
```

### 2. Simple Index Model

An `ArenaIndex` is a lightweight wrapper around a `usize` that directly indexes the arena array:

```rust
pub struct ArenaIndex(usize);
```

This provides O(1) access to any slot without indirection. Invalid indices (out of bounds or pointing to freed slots) return `InvalidIndex` errors.

### 3. O(1) Allocation via Free List

Instead of scanning a bitmap for free slots, we maintain an intrusive free list:

```rust
enum Slot<T: Copy> {
    Free { next_free: usize },
    Occupied { value: T },
}
```

Free slots form a linked list. Allocation pops from the head; freeing pushes to the head. Both are O(1).

### 4. Interior Mutability with Cell

The arena uses `Cell` for interior mutability, allowing it to be used from shared references:

```rust
pub struct Arena<T: Copy, const N: usize> {
    slots: [Cell<Slot<T>>; N],
    free_head: Cell<usize>,
    len: Cell<usize>,
    gc_enabled: Cell<bool>,
}
```

This enables the arena to be stored in static variables and passed by shared reference, which is essential for the Lisp interpreter where many functions need to allocate.

## Memory Layout

For an `Arena<T, N>`:

| Component | Size | Purpose |
|-----------|------|---------|
| `slots` | `N × sizeof(Slot<T>)` | Value storage |
| `free_head` | 8 bytes | Free list head |
| `len` | 8 bytes | Allocated count |
| `gc_enabled` | 1 byte | GC control flag |

Example: `Arena<Value, 50000>` where `sizeof(Value) = 24`:
- Slots: 50,000 × 24 = 1.2 MB
- Total: ~1.2 MB

## Garbage Collection

### Mark-and-Sweep Algorithm

The GC uses a classic mark-and-sweep approach:

1. **Mark Phase**: Starting from root indices, traverse all reachable objects via the `Trace` trait
2. **Sweep Phase**: Iterate through all slots, freeing any that are occupied but not marked

### The Trace Trait

Types stored in the arena must implement `Trace` to participate in GC:

```rust
pub trait Trace<T: Copy, const N: usize> {
    fn trace<F: FnMut(ArenaIndex)>(&self, tracer: F);
}
```

The `trace` method calls the tracer function for each `ArenaIndex` contained in the value.

### Mark Stack (No Heap)

Instead of using recursion or a `Vec` for the mark stack, we use a fixed-size array:

```rust
let mut marked = [false; N];
let mut mark_stack = [0usize; N];
let mut stack_len = 0usize;
```

This keeps GC entirely on the stack, preserving the `no_alloc` property.

### Batched Child Processing

When an object has many children (>16), we process them in batches to avoid stack overflow in the tracer closure:

```rust
loop {
    let mut batch = [0usize; 16];
    let mut batch_count = 0;
    
    value.trace(|child| {
        if batch_count < 16 && !marked[child.raw()] {
            batch[batch_count] = child.raw();
            batch_count += 1;
        }
    });
    
    // Process batch...
    if batch_count == 0 { break; }
}
```

## Design Trade-offs

### Fixed Capacity

**Trade-off**: Capacity must be known at compile time.

**Rationale**: This enables stack/static allocation and eliminates the need for a heap. For embedded systems, this is often required. For general use, you can simply choose a large capacity.

**Mitigation**: The GC builtins allow Lisp code to query capacity and trigger collection when running low.

### Copy Types Only

**Trade-off**: Stored types must implement `Copy`.

**Rationale**: 
- Enables array initialization without `Default` trait
- Avoids issues with destructors (freed slots don't run `Drop`)
- Simplifies the implementation significantly

**Mitigation**: For non-Copy types, wrap them in a newtype with manual memory management.

### Cell vs RefCell

**Trade-off**: Using `Cell` instead of `RefCell`.

**Rationale**: `Cell` provides safe interior mutability without runtime borrow checking overhead. Since stored types must be `Copy`, we can use `get()`/`set()` which are zero-cost.

**Benefit**: No risk of borrow panics, no runtime overhead for borrow tracking.

### O(N) `len()` and `is_empty()`

**Trade-off**: Some operations require scanning all slots.

**Rationale**: We traded O(1) length queries for simpler allocation logic. The arena maintains a count, but operations like iteration still require scanning for occupied slots.

**Mitigation**: Use `stats()` sparingly; prefer `is_full()` which is O(1).

## Error Handling

All fallible operations return `ArenaResult<T>`:

```rust
pub enum ArenaError {
    OutOfMemory,         // Arena is full
    InvalidIndex,        // Index out of bounds or not allocated
    TraceError,          // GC tracing failed
}
```

## Gotchas

### 1. Free List Can Fragment

While allocation is O(1), after many alloc/free cycles, the slots may be interleaved (allocated, free, allocated, free...). This doesn't affect performance but can affect cache locality.

### 2. GC Can Move Nothing

If all objects are reachable from roots, GC returns without collecting anything. Call `arena_stats` first to check if GC is necessary.

## Integration with Lisp

The arena is the foundation of the Lisp interpreter:

- All Lisp values (`Value` enum) are stored in a single arena
- Symbol interning uses reserved slots to ensure symbol equality
- The GC roots include the global environment and current continuation stack
- Lisp code can control GC via builtins: `gc`, `gc-enable`, `gc-disable`, `arena-stats`
