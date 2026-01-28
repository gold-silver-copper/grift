# Extension Design: Builtin Migration, Rust Interop, and Embedded Access

## Overview

This document designs three major extensions to the Lisp interpreter:

1. **Builtin → Stdlib Migration**: Moving functions from builtins to stdlib.lisp
2. **Rust Function Interop**: Calling native Rust functions from Lisp
3. **External Data Access**: Accessing registers, memory, and hardware from embedded contexts

---

## 1. Builtin → Stdlib Migration

### Current State

**Builtins** (implemented in Rust):
- List operations: `car`, `cdr`, `cons`, `list`
- Predicates: `atom`, `eq`, `null?`, `pair?`, `number?`, `boolean?`, `procedure?`, `symbol?`
- Arithmetic: `+`, `-`, `*`, `/`, `mod`
- Comparison: `<`, `>`, `<=`, `>=`, `=`
- Boolean: `not`
- I/O: `print`, `display`, `newline`
- Error: `error`
- Memoization: `memoize`
- Symbol generation: `gensym`
- Mutation: `set-car!`, `set-cdr!`
- Arrays: `make-array`, `array-ref`, `array-set!`, `array-length`, `array?`
- GC: `gc`, `gc-enable`, `gc-disable`, `gc-enabled?`, `arena-stats`

**Stdlib** (implemented in Lisp):
- List operations: `map`, `filter`, `fold`, `length`, `append`, `reverse`, `nth`, `take`, `drop`, `zip`, `member`, `assoc`
- Utilities: `range`, `compose`, `identity`, `constantly`, `flip`, `curry`
- Accessors: `cadr`, `caddr`, `cddr`

### Migration Strategy

#### Phase 1: Safe Migrations (Pure Functions)

These can be moved to stdlib.lisp without breaking core functionality:

**Arithmetic Operations** → Can be moved if we add variadic support:
```lisp
;;; (+ a b ...) - Addition (variadic)
(define (+ . args) 
  (if (null? args) 
      0 
      (if (null? (cdr args))
          (car args)
          (fold (lambda (acc x) (+ acc x)) 0 args))))

;;; (- a b ...) - Subtraction (variadic)
(define (- . args)
  (if (null? args)
      0
      (if (null? (cdr args))
          (- 0 (car args))
          (fold (lambda (acc x) (- acc x)) (car args) (cdr args)))))
```

**Comparison Operations** → Can be moved:
```lisp
;;; (< a b) - Less than
(define (< a b) (< a b))  ; Still needs builtin for actual comparison

;;; Actually, comparisons need type checking and number coercion
;;; So they should stay as builtins for now
```

**Boolean Operations**:
```lisp
;;; (not x) - Boolean negation
(define (not x) (if x #f #t))
```

#### Phase 2: Core Primitives (Must Stay Builtins)

These **cannot** be moved because they:
- Require direct arena access
- Need special evaluation semantics (non-strict)
- Are fundamental primitives

**Must Stay Builtins**:
- `car`, `cdr`, `cons`, `list` - Core list primitives (non-strict)
- `null?`, `pair?`, `number?`, `boolean?`, `procedure?`, `symbol?`, `array?` - Type predicates
- `eq` - Reference equality (needs arena internals)
- `set-car!`, `set-cdr!` - Direct mutation
- `make-array`, `array-ref`, `array-set!`, `array-length` - Array primitives
- `gc`, `gc-enable`, `gc-disable`, `gc-enabled?`, `arena-stats` - Memory management
- `error` - Error handling (needs evaluator context)
- `memoize` - Memoization (needs evaluator context)
- `gensym` - Symbol generation (needs global state)

#### Phase 3: I/O Operations

I/O operations could potentially be moved, but they need access to stdout/stderr:
- Keep as builtins for now (they're simple wrappers)
- Could be replaced with Rust interop functions later

### Implementation Plan

1. **Add variadic argument support to stdlib functions**
   - Extend `define_stdlib!` to support `(define (f . args) body)`
   - Update parser to handle variadic definitions

2. **Migrate arithmetic operations**
   - Move `+`, `-`, `*`, `/`, `mod` to stdlib.lisp
   - Keep type checking in builtins or add type predicates

3. **Migrate boolean operations**
   - Move `not` to stdlib.lisp

4. **Update benchmarks and tests**
   - Ensure performance is acceptable
   - Verify correctness

### Trade-offs

**Pros of Migration**:
- ✅ Smaller core interpreter
- ✅ More functions user-modifiable
- ✅ Easier to extend with new arithmetic operations
- ✅ Better separation of concerns

**Cons of Migration**:
- ❌ Performance: stdlib functions parse on first call (cached after)
- ❌ Type safety: Builtins can do type checking, stdlib relies on predicates
- ❌ Error messages: Builtins have better error context

**Recommendation**: Migrate arithmetic and boolean operations, keep core primitives as builtins.

---

## 2. Rust Function Interop

### Design Goals

Allow embedding Rust functions that can be called from Lisp, enabling:
- Hardware access (registers, memory-mapped I/O)
- System calls
- Native performance for critical paths
- Integration with existing Rust codebases

### Architecture

#### 2.1 Function Registration

```rust
// In lisp_eval or new lisp_ffi crate

pub trait LispCallable {
    fn call(&self, args: &[ArenaIndex], lisp: &Lisp<N>, eval: &mut Evaluator<N>) 
        -> Result<ArenaIndex, EvalError>;
}

pub struct NativeFunction {
    name: &'static str,
    arity: Option<usize>,  // None = variadic
    func: Box<dyn LispCallable>,
}

impl<'a, const N: usize> Evaluator<'a, N> {
    /// Register a native Rust function
    pub fn register_native<F>(&mut self, name: &str, func: F) -> Result<(), EvalError>
    where
        F: LispCallable + 'static,
    {
        // Add to global environment
        let name_sym = self.lisp.symbol(name)?;
        let native_val = self.lisp.native_function(/* ... */)?;
        self.global_env = self.env_extend(self.global_env, name_sym, native_val)?;
        Ok(())
    }
}
```

#### 2.2 Value Conversion

```rust
// Helper traits for conversion

pub trait FromLisp {
    fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> Result<Self, EvalError>;
}

pub trait ToLisp {
    fn to_lisp(self, lisp: &Lisp<N>) -> Result<ArenaIndex, ArenaError>;
}

impl FromLisp for i64 {
    fn from_lisp(lisp: &Lisp<N>, idx: ArenaIndex) -> Result<Self, EvalError> {
        match lisp.get(idx)? {
            Value::Number(n) => Ok(n),
            _ => Err(EvalError::new(ErrorKind::TypeError)
                .with_message("Expected number")),
        }
    }
}

impl ToLisp for i64 {
    fn to_lisp(self, lisp: &Lisp<N>) -> Result<ArenaIndex, ArenaError> {
        lisp.number(self)
    }
}

// Similar for bool, String, Vec, etc.
```

#### 2.3 Macro for Easy Registration

```rust
#[macro_export]
macro_rules! define_native {
    (
        $(#[$attr:meta])*
        $name:ident => $lisp_name:literal,
        $arity:expr,
        |$lisp:ident, $eval:ident, $args:ident| $body:block
    ) => {
        // Generate LispCallable implementation
    };
}

// Usage:
define_native! {
    /// Read CPU register
    read_register => "read-register",
    1,
    |lisp, eval, args| {
        let reg_name: String = FromLisp::from_lisp(lisp, args[0])?;
        let value = read_cpu_register(&reg_name)?;
        ToLisp::to_lisp(value, lisp)
    }
}
```

#### 2.4 Calling Convention

```rust
// In apply_function_trampolined:

match self.lisp.get(val)? {
    Value::NativeFunction { func, arity } => {
        // Force all arguments
        let forced_args = self.force_argument_list(args)?;
        
        // Convert to slice
        let arg_slice = self.arena_index_list_to_slice(forced_args)?;
        
        // Call native function
        func.call(&arg_slice, self.lisp, self)?  // Returns ArenaIndex
    }
    // ...
}
```

### Example Use Cases

#### Hardware Register Access

```rust
// Register a function to read CPU registers
eval.register_native("read-register", |args, lisp, _| {
    let reg_name: String = FromLisp::from_lisp(lisp, args[0])?;
    let value = unsafe { read_register(&reg_name) };
    ToLisp::to_lisp(value, lisp)
})?;

// In Lisp:
(define rax (read-register "rax"))
(define rbx (read-register "rbx"))
(+ rax rbx)
```

#### Memory-Mapped I/O

```rust
eval.register_native("mmio-read", |args, lisp, _| {
    let addr: u64 = FromLisp::from_lisp(lisp, args[0])?;
    let value = unsafe { read_mmio(addr) };
    ToLisp::to_lisp(value as i64, lisp)
})?;

eval.register_native("mmio-write", |args, lisp, _| {
    let addr: u64 = FromLisp::from_lisp(lisp, args[0])?;
    let value: u64 = FromLisp::from_lisp(lisp, args[1])?;
    unsafe { write_mmio(addr, value) };
    lisp.nil()
})?;
```

#### System Integration

```rust
eval.register_native("get-time", |_args, lisp, _| {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    ToLisp::to_lisp(now as i64, lisp)
})?;
```

### Safety Considerations

1. **Unsafe Code**: Native functions may use `unsafe` for hardware access
2. **Error Handling**: Convert Rust errors to `EvalError`
3. **Memory Safety**: Native functions must not hold references to arena values
4. **Recursion**: Native functions can call back into Lisp (via `eval` parameter)

### Implementation Phases

**Phase 1**: Basic function registration and calling
- Add `NativeFunction` value type
- Implement `LispCallable` trait
- Add registration to `Evaluator`

**Phase 2**: Value conversion helpers
- Implement `FromLisp`/`ToLisp` for common types
- Add macro for easy registration

**Phase 3**: Advanced features
- Variadic native functions
- Callback support (Lisp → Rust → Lisp)
- Async/await support (if needed)

---

## 3. External Data Access for Embedded Use

### Design Goals

Enable Lisp to access external data structures for embedded systems:
- CPU registers
- Memory-mapped I/O
- Hardware state
- External buffers
- Sensor data

### Architecture

#### 3.1 External Reference Type

```rust
// New value type in lisp_parser

pub enum Value {
    // ... existing variants ...
    
    /// External reference - points to data outside the arena
    /// The data is accessed via a callback function
    External {
        /// Opaque pointer to external data (not managed by GC)
        data: *const u8,
        /// Type tag for the external data
        tag: ExternalTag,
        /// Optional size (for arrays/buffers)
        size: Option<usize>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalTag {
    Register,      // CPU register
    Mmio,          // Memory-mapped I/O
    Buffer,        // External buffer
    Sensor,        // Sensor reading
    Custom(u32),   // User-defined
}
```

#### 3.2 Accessor Functions

```rust
// Builtin functions for external access

define_builtins! {
    // ... existing builtins ...
    
    /// read-external - Read value from external reference
    ReadExternal => "read-external",
    
    /// write-external - Write value to external reference
    WriteExternal => "write-external",
    
    /// external? - Check if value is external reference
    Externalp => "external?",
}
```

#### 3.3 Register Access

```rust
// Register accessor implementation

pub struct RegisterAccessor {
    registers: &'static [Register],
}

pub struct Register {
    name: &'static str,
    value: *mut u64,  // Pointer to actual register
}

impl<'a, const N: usize> Evaluator<'a, N> {
    pub fn register_cpu_registers(&mut self, accessor: RegisterAccessor) -> Result<(), EvalError> {
        for reg in accessor.registers {
            // Create external reference
            let ext_val = self.lisp.external(
                reg.value as *const u8,
                ExternalTag::Register,
                Some(8),  // 64-bit register
            )?;
            
            // Bind to symbol
            let name_sym = self.lisp.symbol(reg.name)?;
            self.global_env = self.env_extend(self.global_env, name_sym, ext_val)?;
        }
        Ok(())
    }
}
```

#### 3.4 Memory-Mapped I/O

```rust
pub struct MmioRegion {
    base: u64,
    size: usize,
}

impl<'a, const N: usize> Evaluator<'a, N> {
    pub fn register_mmio(&mut self, name: &str, region: MmioRegion) -> Result<(), EvalError> {
        let ext_val = self.lisp.external(
            region.base as *const u8,
            ExternalTag::Mmio,
            Some(region.size),
        )?;
        
        let name_sym = self.lisp.symbol(name)?;
        self.global_env = self.env_extend(self.global_env, name_sym, ext_val)?;
        Ok(())
    }
}
```

#### 3.5 Lisp API

```lisp
;;; Read CPU register
(define rax (read-external %rax))

;;; Write to register
(write-external %rax 42)

;;; Memory-mapped I/O
(define gpio-base (read-external %gpio-base))
(write-external gpio-base 0x1234)

;;; Check if value is external
(if (external? %rax)
    (print "This is an external reference")
    (print "This is not external"))
```

### Implementation Details

#### 3.6 Garbage Collection

External references are **not** garbage collected:
- They point to data outside the arena
- They're treated as roots during GC
- They don't need tracing (no internal pointers)

```rust
impl Trace for Value {
    fn trace(&self, tracer: &mut Tracer) {
        match self {
            // ... existing cases ...
            Value::External { .. } => {
                // External references are roots - don't trace
            }
        }
    }
}
```

#### 3.7 Type Safety

```rust
// Type checking for external operations

fn read_external(&mut self, ext: ArenaIndex, call_expr: ArenaIndex) -> EvalResult {
    match self.lisp.get(ext)? {
        Value::External { data, tag, size } => {
            match tag {
                ExternalTag::Register => {
                    // Read 64-bit value
                    let value = unsafe { *(data as *const u64) };
                    self.lisp.number(value as i64)
                }
                ExternalTag::Mmio => {
                    // Read from MMIO (size-dependent)
                    // ...
                }
                _ => Err(self.make_error(ErrorKind::TypeError, call_expr)),
            }
        }
        _ => Err(self.make_error(ErrorKind::TypeError, call_expr)
            .with_message("Expected external reference")),
    }
}
```

### Example: Embedded System Integration

```rust
// Setup for embedded system

let mut eval = Evaluator::new(&lisp)?;

// Register CPU registers
let registers = RegisterAccessor::new(&[
    Register::new("rax", &mut cpu_state.rax),
    Register::new("rbx", &mut cpu_state.rbx),
    Register::new("rcx", &mut cpu_state.rcx),
    // ...
]);
eval.register_cpu_registers(registers)?;

// Register MMIO regions
eval.register_mmio("gpio", MmioRegion::new(0x40000000, 0x1000))?;
eval.register_mmio("uart", MmioRegion::new(0x40001000, 0x1000))?;

// Now Lisp can access hardware:
eval.eval_str("(write-external %gpio 0x42)")?;
eval.eval_str("(define uart-data (read-external %uart))")?;
```

### Safety Considerations

1. **Unsafe Access**: External data access requires `unsafe` for raw pointers
2. **Lifetime Management**: External references must outlive the evaluator
3. **Concurrency**: External data may be modified by other threads/hardware
4. **Bounds Checking**: MMIO access should validate addresses

---

## Implementation Priority

### Phase 1: Foundation (High Priority)
1. ✅ Rust function interop (basic)
2. ✅ External reference type
3. ✅ Register access

### Phase 2: Migration (Medium Priority)
1. ✅ Move arithmetic to stdlib
2. ✅ Move boolean operations to stdlib
3. ✅ Add variadic argument support

### Phase 3: Advanced Features (Lower Priority)
1. ✅ MMIO support
2. ✅ Buffer access
3. ✅ Callback support (Rust → Lisp)

---

## API Summary

### Rust Interop

```rust
// Register a native function
eval.register_native("my-func", |args, lisp, _| {
    let x: i64 = FromLisp::from_lisp(lisp, args[0])?;
    let result = my_rust_function(x);
    ToLisp::to_lisp(result, lisp)
})?;
```

### External Data

```rust
// Register CPU registers
eval.register_cpu_registers(RegisterAccessor::new(&registers))?;

// Register MMIO
eval.register_mmio("gpio", MmioRegion::new(0x40000000, 0x1000))?;
```

### Lisp Usage

```lisp
;;; Call native function
(my-func 42)

;;; Access external data
(read-external %rax)
(write-external %gpio 0x42)
```

---

## Open Questions

1. **Performance**: How much overhead does stdlib parsing add? (Currently cached after first call)
2. **Type System**: Should we add type annotations for native functions?
3. **Error Handling**: How to handle hardware faults in external access?
4. **Concurrency**: How to handle concurrent access to external data?
5. **Memory Safety**: How to prevent use-after-free for external references?

---

## Next Steps

1. Create `lisp_ffi` crate for Rust interop
2. Implement `NativeFunction` value type
3. Add external reference support
4. Migrate arithmetic operations to stdlib
5. Write tests and benchmarks
6. Document API usage
