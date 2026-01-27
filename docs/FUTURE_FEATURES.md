# Future Features: Direct Arena Access

This document outlines potential features that could be added to the Lisp implementation to provide more direct access to the arena storage. These features would give advanced users low-level control over memory layout and allocation patterns.

## Implemented Features

### Arrays (Contiguous Value Storage)

Arrays store values contiguously in the arena, providing O(1) indexed access and mutation:

```lisp
(define arr (make-array 3 0))  ; Create array of 3 zeros
(array-set! arr 1 42)          ; Set index 1 to 42
(array-ref arr 1)              ; => 42
(array-length arr)             ; => 3
(array? arr)                   ; => #t
```

**Memory Layout**: Elements are stored in consecutive arena slots, enabling cache-friendly access patterns.

---

## Potential Future Features

### 1. Raw Arena Access

Direct read/write access to arena slots by raw index:

```lisp
;; Get raw slot index from a value
(arena-index value)           ; => 42 (raw slot number)

;; Read/write by raw index (UNSAFE)
(arena-read 42)               ; => value at slot 42
(arena-write! 42 new-value)   ; Write to slot 42

;; Bounds checking
(arena-capacity)              ; => 50000
(arena-valid-index? 42)       ; => #t
```

**Use Cases**:
- Implementing custom data structures
- Debugging and introspection
- Performance-critical code that bypasses normal indirection

**Risks**: Use-after-free, generation mismatch, memory corruption

### 2. Bulk Allocation

Allocate multiple slots at once for custom data structures:

```lisp
;; Allocate N contiguous slots
(arena-alloc-block 100 default-value)  ; => start-index

;; Free a contiguous block
(arena-free-block start-index count)
```

**Use Cases**:
- Large matrices or tensors
- Custom hash tables with contiguous backing storage
- Memory pools for specialized allocators

### 3. Memory Views / Slices

Create views into existing arrays without copying:

```lisp
;; Create a view into an array
(array-slice arr 2 5)         ; View of elements 2..5

;; Views share storage with original
(define view (array-slice arr 1 3))
(array-set! view 0 99)        ; Modifies arr[1]
```

**Benefits**:
- Zero-copy subarray access
- Efficient matrix row/column access
- Reduced memory allocation pressure

### 4. Typed/Packed Arrays

Arrays with homogeneous types for compact storage:

```lisp
;; Create packed integer array (1 slot per 8 integers)
(make-packed-array 'i64 1000 0)

;; Create byte array (1 slot per 24 bytes on 64-bit)
(make-byte-array 100 0)

;; Access
(packed-ref arr 500)
(packed-set! arr 500 42)
```

**Benefits**:
- 8-24x memory reduction for numeric data
- Better cache utilization
- SIMD-friendly layout

### 5. Arena Regions / Zones

Allocate values in separate regions for bulk deallocation:

```lisp
;; Create a new region
(define region (arena-make-region))

;; Allocate in the region
(arena-with-region region
  (define x (cons 1 2))
  (define y (list 1 2 3))
  ; All allocations go to 'region'
)

;; Free entire region at once
(arena-free-region region)    ; O(1) deallocation of all values
```

**Use Cases**:
- Request-scoped allocation (like web servers)
- Compiler passes that allocate then discard ASTs
- Game frame allocators

### 6. Copy-on-Write Structures

Efficient persistent data structures:

```lisp
;; Create COW array
(define arr (make-cow-array 100 0))

;; Fork creates a logical copy (O(1))
(define arr2 (cow-fork arr))

;; Writes trigger actual copying of affected pages
(array-set! arr2 50 42)       ; Only copies the modified region
```

**Benefits**:
- Efficient immutable data structures
- Safe parallel access
- Undo/redo support

### 7. Generation Introspection

Inspect and manipulate generational indices:

```lisp
;; Get generation of an index
(index-generation idx)        ; => 5

;; Check if index is stale
(index-valid? idx)            ; => #t or #f

;; Compare generations
(index-newer? idx1 idx2)      ; => #t if idx1 is newer
```

**Use Cases**:
- Debugging stale references
- Implementing weak references
- Cache invalidation strategies

### 8. Custom Trace Implementations

User-defined GC tracing for foreign objects:

```lisp
;; Register a custom tracer
(register-tracer! 'my-struct
  (lambda (obj tracer)
    (tracer (my-struct-field1 obj))
    (tracer (my-struct-field2 obj))))

;; Create objects with custom tracing
(make-foreign 'my-struct data)
```

**Use Cases**:
- FFI objects with interior pointers
- Compressed reference encoding
- Specialized graph structures

### 9. Memory-Mapped Arrays

Arrays backed by external memory or files:

```lisp
;; Map a file as an array (std only)
(define mmap (mmap-file "data.bin" 'read-write))

;; Access like normal array
(array-ref mmap 1000)

;; Changes written back to file
(array-set! mmap 1000 42)
(mmap-sync mmap)
```

**Benefits**:
- Work with data larger than arena capacity
- Persistent storage
- Shared memory between processes

### 10. Weak References

References that don't prevent GC:

```lisp
;; Create weak reference
(define weak (make-weak-ref obj))

;; Dereference (may return #f if collected)
(weak-ref weak)               ; => obj or #f

;; Check if still alive
(weak-alive? weak)            ; => #t or #f
```

**Use Cases**:
- Caches that don't cause memory leaks
- Observer patterns
- Finalization callbacks

---

## Implementation Considerations

### Safety vs Performance Tradeoffs

Each feature must balance:
1. **Safety**: Generational indices, bounds checking, type validation
2. **Performance**: Minimal overhead for checked operations
3. **Ergonomics**: Intuitive API that's hard to misuse

### no_std Compatibility

Features should remain compatible with `no_std` where possible:
- Avoid heap allocation in core implementations
- Use fixed-size buffers with configurable limits
- Provide `std`-only extensions for file I/O, mmap, etc.

### GC Integration

New features must integrate with garbage collection:
- Implement `Trace` for new value types
- Handle contiguous allocations correctly
- Consider tracing cost for large structures

### Backward Compatibility

New features should not break existing code:
- Existing Value variants remain unchanged
- New builtins use distinct names
- Error handling consistent with existing patterns
