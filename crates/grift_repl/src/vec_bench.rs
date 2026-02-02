#![forbid(unsafe_code)]

//! # Vec-backed Arena Storage Benchmark Suite
//!
//! Run with: `cargo run -p grift_repl --bin grift-vec-bench --release`
//!
//! This benchmark compares performance between ArrayStorage (const-generic, fixed size)
//! and VecStorage (dynamic, heap-allocated) backends for the arena allocator.
//!
//! Both storage backends use identical arena APIs, so this comparison reveals the
//! performance characteristics of each approach for the same workloads.

use grift_arena::{Arena, GenericArena, VecStorage, ArenaIndex};
use grift_parser::Value;
use std::time::{Duration, Instant};

/// Result of a single benchmark comparing both storage backends
struct BenchResult {
    name: String,
    array_duration: Duration,
    vec_duration: Duration,
    iterations: usize,
    passed: bool,
    note: Option<String>,
}

impl BenchResult {
    fn print(&self) {
        let status = if self.passed { "PASS" } else { "FAIL" };
        let array_per_iter = if self.iterations > 1 {
            format!(
                " ({:.2}µs/iter)",
                self.array_duration.as_nanos() as f64 / self.iterations as f64 / 1000.0
            )
        } else {
            String::new()
        };
        let vec_per_iter = if self.iterations > 1 {
            format!(
                " ({:.2}µs/iter)",
                self.vec_duration.as_nanos() as f64 / self.iterations as f64 / 1000.0
            )
        } else {
            String::new()
        };

        // Calculate speedup/slowdown
        let ratio = if self.vec_duration.as_nanos() > 0 {
            self.array_duration.as_nanos() as f64 / self.vec_duration.as_nanos() as f64
        } else {
            1.0
        };

        let comparison = if ratio > 1.05 {
            format!("Vec {:.2}x faster", ratio)
        } else if ratio < 0.95 {
            format!("Array {:.2}x faster", 1.0 / ratio)
        } else {
            "~equal".to_string()
        };

        println!("[{}] {}", status, self.name);
        println!("       Array: {:?}{}", self.array_duration, array_per_iter);
        println!("       Vec:   {:?}{}", self.vec_duration, vec_per_iter);
        println!("       → {}", comparison);

        if let Some(note) = &self.note {
            println!("       Note: {}", note);
        }
    }
}

/// Benchmark helper for ArrayStorage
fn bench_array<F, const N: usize>(arena: &Arena<Value, N>, iterations: usize, f: F) -> Duration
where
    F: Fn(&Arena<Value, N>),
{
    let start = Instant::now();
    for _ in 0..iterations {
        f(arena);
    }
    start.elapsed()
}

/// Benchmark helper for VecStorage
fn bench_vec<F>(arena: &GenericArena<Value, VecStorage<Value>>, iterations: usize, f: F) -> Duration
where
    F: Fn(&GenericArena<Value, VecStorage<Value>>),
{
    let start = Instant::now();
    for _ in 0..iterations {
        f(arena);
    }
    start.elapsed()
}

/// Run a benchmark on both storage backends
fn run_bench<FA, FV, const N: usize>(
    name: &str,
    array_arena: &Arena<Value, N>,
    vec_arena: &GenericArena<Value, VecStorage<Value>>,
    iterations: usize,
    array_fn: FA,
    vec_fn: FV,
) -> BenchResult
where
    FA: Fn(&Arena<Value, N>),
    FV: Fn(&GenericArena<Value, VecStorage<Value>>),
{
    println!("Running: {} ({} iterations)...", name, iterations);

    let array_duration = bench_array(array_arena, iterations, array_fn);
    let vec_duration = bench_vec(vec_arena, iterations, vec_fn);

    BenchResult {
        name: name.to_string(),
        array_duration,
        vec_duration,
        iterations,
        passed: true,
        note: None,
    }
}

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║     Vec vs Array Storage Arena Benchmark Suite               ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ Comparing: ArrayStorage<60000> vs VecStorage(60000)          ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // Create both arena types with the same capacity
    const CAPACITY: usize = 60000;
    let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
    let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
    let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

    println!("Array arena capacity: {}", array_arena.capacity());
    println!("Vec arena capacity:   {}", vec_arena.capacity());
    println!();

    let mut results: Vec<BenchResult> = Vec::new();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 1: Basic Allocation
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 1: Basic Allocation");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Test single allocation
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        results.push(run_bench(
            "Single Number allocation x 10000",
            &array_arena,
            &vec_arena,
            10000,
            |arena| {
                let _ = arena.alloc(Value::Number(42));
            },
            |arena| {
                let _ = arena.alloc(Value::Number(42));
            },
        ));
    }

    // Test Cons cell allocation (common in Lisp)
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        // Pre-allocate some indices
        let nil_a = array_arena.alloc(Value::Nil).unwrap();
        let nil_v = vec_arena.alloc(Value::Nil).unwrap();

        results.push(run_bench(
            "Cons cell allocation x 10000",
            &array_arena,
            &vec_arena,
            10000,
            |arena| {
                let _ = arena.alloc(Value::Cons { car: nil_a, cdr: nil_a });
            },
            |arena| {
                let _ = arena.alloc(Value::Cons { car: nil_v, cdr: nil_v });
            },
        ));
    }

    // Test Symbol allocation
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        let str_idx_a = array_arena.alloc(Value::String { len: 5, data: ArenaIndex::NIL }).unwrap();
        let str_idx_v = vec_arena.alloc(Value::String { len: 5, data: ArenaIndex::NIL }).unwrap();

        results.push(run_bench(
            "Symbol allocation x 5000",
            &array_arena,
            &vec_arena,
            5000,
            |arena| {
                let _ = arena.alloc(Value::Symbol(str_idx_a));
            },
            |arena| {
                let _ = arena.alloc(Value::Symbol(str_idx_v));
            },
        ));
    }

    // Test Lambda allocation
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        let nil_a = array_arena.alloc(Value::Nil).unwrap();
        let nil_v = vec_arena.alloc(Value::Nil).unwrap();
        let body_env_a = array_arena.alloc(Value::Cons { car: nil_a, cdr: nil_a }).unwrap();
        let body_env_v = vec_arena.alloc(Value::Cons { car: nil_v, cdr: nil_v }).unwrap();

        results.push(run_bench(
            "Lambda allocation x 5000",
            &array_arena,
            &vec_arena,
            5000,
            |arena| {
                let _ = arena.alloc(Value::Lambda { params: nil_a, body_env: body_env_a });
            },
            |arena| {
                let _ = arena.alloc(Value::Lambda { params: nil_v, body_env: body_env_v });
            },
        ));
    }

    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 2: Value Access
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 2: Value Access (get/set)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Test get operations
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        let idx_a = array_arena.alloc(Value::Number(42)).unwrap();
        let idx_v = vec_arena.alloc(Value::Number(42)).unwrap();

        results.push(run_bench(
            "get() single value x 50000",
            &array_arena,
            &vec_arena,
            50000,
            |arena| {
                let _ = arena.get(idx_a);
            },
            |arena| {
                let _ = arena.get(idx_v);
            },
        ));
    }

    // Test set operations
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        let idx_a = array_arena.alloc(Value::Number(0)).unwrap();
        let idx_v = vec_arena.alloc(Value::Number(0)).unwrap();

        results.push(run_bench(
            "set() single value x 50000",
            &array_arena,
            &vec_arena,
            50000,
            |arena| {
                let _ = arena.set(idx_a, Value::Number(42));
            },
            |arena| {
                let _ = arena.set(idx_v, Value::Number(42));
            },
        ));
    }

    // Test modify operations (read-modify-write)
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        let idx_a = array_arena.alloc(Value::Number(0)).unwrap();
        let idx_v = vec_arena.alloc(Value::Number(0)).unwrap();

        results.push(run_bench(
            "modify() value x 20000",
            &array_arena,
            &vec_arena,
            20000,
            |arena| {
                let _ = arena.modify(idx_a, |v| {
                    if let Value::Number(n) = v {
                        *n += 1;
                    }
                });
            },
            |arena| {
                let _ = arena.modify(idx_v, |v| {
                    if let Value::Number(n) = v {
                        *n += 1;
                    }
                });
            },
        ));
    }

    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 3: Contiguous Allocation (Strings/Arrays)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 3: Contiguous Allocation (for Strings/Arrays)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Test small contiguous allocation
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        results.push(run_bench(
            "alloc_contiguous(5) x 2000",
            &array_arena,
            &vec_arena,
            2000,
            |arena| {
                let _ = arena.alloc_contiguous(5, Value::Char('x'));
            },
            |arena| {
                let _ = arena.alloc_contiguous(5, Value::Char('x'));
            },
        ));
    }

    // Test medium contiguous allocation
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        results.push(run_bench(
            "alloc_contiguous(20) x 1000",
            &array_arena,
            &vec_arena,
            1000,
            |arena| {
                let _ = arena.alloc_contiguous(20, Value::Char('x'));
            },
            |arena| {
                let _ = arena.alloc_contiguous(20, Value::Char('x'));
            },
        ));
    }

    // Test large contiguous allocation (for vectors)
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        results.push(run_bench(
            "alloc_contiguous(100) x 200",
            &array_arena,
            &vec_arena,
            200,
            |arena| {
                let _ = arena.alloc_contiguous(100, Value::Number(0));
            },
            |arena| {
                let _ = arena.alloc_contiguous(100, Value::Number(0));
            },
        ));
    }

    // Test index_at_offset (used for array/string access)
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        let base_a = array_arena.alloc_contiguous(100, Value::Number(0)).unwrap();
        let base_v = vec_arena.alloc_contiguous(100, Value::Number(0)).unwrap();

        results.push(run_bench(
            "index_at_offset (array access) x 50000",
            &array_arena,
            &vec_arena,
            50000,
            |arena| {
                for i in 0..10 {
                    let _ = arena.index_at_offset(base_a, i);
                }
            },
            |arena| {
                for i in 0..10 {
                    let _ = arena.index_at_offset(base_v, i);
                }
            },
        ));
    }

    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 4: Free and Reuse
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 4: Free and Reuse");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Test alloc + free cycle
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        results.push(run_bench(
            "alloc + free cycle x 10000",
            &array_arena,
            &vec_arena,
            10000,
            |arena| {
                let idx = arena.alloc(Value::Number(42)).unwrap();
                let _ = arena.free(idx);
            },
            |arena| {
                let idx = arena.alloc(Value::Number(42)).unwrap();
                let _ = arena.free(idx);
            },
        ));
    }

    // Test contiguous alloc + free cycle
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        results.push(run_bench(
            "alloc_contiguous(10) + free_contiguous x 2000",
            &array_arena,
            &vec_arena,
            2000,
            |arena| {
                let start = arena.alloc_contiguous(10, Value::Nil).unwrap();
                let _ = arena.free_contiguous(start, 10);
            },
            |arena| {
                let start = arena.alloc_contiguous(10, Value::Nil).unwrap();
                let _ = arena.free_contiguous(start, 10);
            },
        ));
    }

    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 5: Iteration
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 5: Iteration");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Pre-populate arenas for iteration
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        // Allocate 1000 values in each
        for i in 0..1000 {
            let _ = array_arena.alloc(Value::Number(i as isize));
            let _ = vec_arena.alloc(Value::Number(i as isize));
        }

        results.push(run_bench(
            "iter() over 1000 values x 100",
            &array_arena,
            &vec_arena,
            100,
            |arena| {
                let _count: usize = arena.iter().count();
            },
            |arena| {
                let _count: usize = arena.iter().count();
            },
        ));
    }

    // Test for_each
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        // Allocate 1000 values in each
        for i in 0..1000 {
            let _ = array_arena.alloc(Value::Number(i as isize));
            let _ = vec_arena.alloc(Value::Number(i as isize));
        }

        results.push(run_bench(
            "for_each over 1000 values x 100",
            &array_arena,
            &vec_arena,
            100,
            |arena| {
                let mut sum = 0isize;
                arena.for_each(|_, v| {
                    if let Value::Number(n) = v {
                        sum += n;
                    }
                });
                let _ = sum;
            },
            |arena| {
                let mut sum = 0isize;
                arena.for_each(|_, v| {
                    if let Value::Number(n) = v {
                        sum += n;
                    }
                });
                let _ = sum;
            },
        ));
    }

    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 6: Garbage Collection
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 6: Garbage Collection");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Note: GC is only available for ArrayStorage (Arena<T, N>) because it uses
    // fixed-size arrays [bool; N] for mark bitmaps. GenericArena with VecStorage
    // would need a Vec-based GC implementation.
    println!();
    println!("Note: GC benchmarks only run for ArrayStorage.");
    println!("      GenericArena<VecStorage> does not yet implement collect_garbage.");
    println!("      (Would require Vec-based mark bitmap implementation)");
    println!();

    // Test GC with moderate garbage (ArrayStorage only)
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);

        // Create some values, keeping only a few as roots
        let root_a = array_arena.alloc(Value::Number(1)).unwrap();

        // Allocate garbage
        for i in 0..500 {
            let _ = array_arena.alloc(Value::Number(i as isize));
        }

        println!("Running: GC with moderate garbage x 20 (ArrayStorage only)...");
        println!("       Pre-GC array allocated: {}", array_arena.len());

        let array_start = Instant::now();
        for _ in 0..20 {
            array_arena.collect_garbage(&[root_a]);
        }
        let array_duration = array_start.elapsed();

        println!("       Post-GC array allocated: {}", array_arena.len());

        // For comparison, record Vec as N/A
        results.push(BenchResult {
            name: "GC with moderate garbage x 20 (ArrayStorage only)".to_string(),
            array_duration,
            vec_duration: Duration::ZERO,
            iterations: 20,
            passed: true,
            note: Some("VecStorage GC not implemented".to_string()),
        });
    }

    // Test GC with linked structures (Cons cells) - ArrayStorage only
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);

        // Build a linked list of 100 elements (root)
        let nil_a = array_arena.alloc(Value::Nil).unwrap();

        let mut list_a = nil_a;

        for i in 0..100 {
            let num_a = array_arena.alloc(Value::Number(i as isize)).unwrap();
            list_a = array_arena.alloc(Value::Cons { car: num_a, cdr: list_a }).unwrap();
        }

        // Create garbage (another 200 cons cells not reachable)
        for i in 0..200 {
            let num_a = array_arena.alloc(Value::Number(i as isize)).unwrap();
            let _ = array_arena.alloc(Value::Cons { car: num_a, cdr: nil_a });
        }

        println!("Running: GC with linked structures x 20 (ArrayStorage only)...");
        println!("       Pre-GC array allocated: {}", array_arena.len());

        let array_start = Instant::now();
        for _ in 0..20 {
            array_arena.collect_garbage(&[list_a, nil_a]);
        }
        let array_duration = array_start.elapsed();

        println!("       Post-GC array allocated: {}", array_arena.len());

        results.push(BenchResult {
            name: "GC with linked structures x 20 (ArrayStorage only)".to_string(),
            array_duration,
            vec_duration: Duration::ZERO,
            iterations: 20,
            passed: true,
            note: Some("VecStorage GC not implemented".to_string()),
        });
    }

    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 7: Mixed Workloads (Simulating Lisp Evaluation)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 7: Mixed Workloads (Simulating Lisp Evaluation)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Simulate list building (common in Lisp)
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        let nil_a = array_arena.alloc(Value::Nil).unwrap();
        let nil_v = vec_arena.alloc(Value::Nil).unwrap();

        results.push(run_bench(
            "Build 10-element list x 500",
            &array_arena,
            &vec_arena,
            500,
            |arena| {
                let mut list = nil_a;
                for i in 0..10 {
                    let num = arena.alloc(Value::Number(i)).unwrap();
                    list = arena.alloc(Value::Cons { car: num, cdr: list }).unwrap();
                }
                let _ = list;
            },
            |arena| {
                let mut list = nil_v;
                for i in 0..10 {
                    let num = arena.alloc(Value::Number(i)).unwrap();
                    list = arena.alloc(Value::Cons { car: num, cdr: list }).unwrap();
                }
                let _ = list;
            },
        ));
    }

    // Simulate closure creation (lambda + environment)
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        let nil_a = array_arena.alloc(Value::Nil).unwrap();
        let nil_v = vec_arena.alloc(Value::Nil).unwrap();

        results.push(run_bench(
            "Create closure (lambda + 3-binding env) x 1000",
            &array_arena,
            &vec_arena,
            1000,
            |arena| {
                // Create environment: ((x . 1) (y . 2) (z . 3))
                let x = arena.alloc(Value::Symbol(nil_a)).unwrap();
                let one = arena.alloc(Value::Number(1)).unwrap();
                let binding1 = arena.alloc(Value::Cons { car: x, cdr: one }).unwrap();

                let y = arena.alloc(Value::Symbol(nil_a)).unwrap();
                let two = arena.alloc(Value::Number(2)).unwrap();
                let binding2 = arena.alloc(Value::Cons { car: y, cdr: two }).unwrap();

                let z = arena.alloc(Value::Symbol(nil_a)).unwrap();
                let three = arena.alloc(Value::Number(3)).unwrap();
                let binding3 = arena.alloc(Value::Cons { car: z, cdr: three }).unwrap();

                let env = arena.alloc(Value::Cons { car: binding1, cdr: nil_a }).unwrap();
                let env = arena.alloc(Value::Cons { car: binding2, cdr: env }).unwrap();
                let env = arena.alloc(Value::Cons { car: binding3, cdr: env }).unwrap();

                // Create body_env cons
                let body_env = arena.alloc(Value::Cons { car: nil_a, cdr: env }).unwrap();

                // Create lambda
                let _lambda = arena.alloc(Value::Lambda { params: nil_a, body_env }).unwrap();
            },
            |arena| {
                // Same for vec arena
                let x = arena.alloc(Value::Symbol(nil_v)).unwrap();
                let one = arena.alloc(Value::Number(1)).unwrap();
                let binding1 = arena.alloc(Value::Cons { car: x, cdr: one }).unwrap();

                let y = arena.alloc(Value::Symbol(nil_v)).unwrap();
                let two = arena.alloc(Value::Number(2)).unwrap();
                let binding2 = arena.alloc(Value::Cons { car: y, cdr: two }).unwrap();

                let z = arena.alloc(Value::Symbol(nil_v)).unwrap();
                let three = arena.alloc(Value::Number(3)).unwrap();
                let binding3 = arena.alloc(Value::Cons { car: z, cdr: three }).unwrap();

                let env = arena.alloc(Value::Cons { car: binding1, cdr: nil_v }).unwrap();
                let env = arena.alloc(Value::Cons { car: binding2, cdr: env }).unwrap();
                let env = arena.alloc(Value::Cons { car: binding3, cdr: env }).unwrap();

                let body_env = arena.alloc(Value::Cons { car: nil_v, cdr: env }).unwrap();
                let _lambda = arena.alloc(Value::Lambda { params: nil_v, body_env }).unwrap();
            },
        ));
    }

    // Simulate vector creation (make-vector)
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        results.push(run_bench(
            "Create vector[50] (contiguous + header) x 200",
            &array_arena,
            &vec_arena,
            200,
            |arena| {
                let data = arena.alloc_contiguous(50, Value::Number(0)).unwrap();
                let _vec = arena.alloc(Value::Array { len: 50, data }).unwrap();
            },
            |arena| {
                let data = arena.alloc_contiguous(50, Value::Number(0)).unwrap();
                let _vec = arena.alloc(Value::Array { len: 50, data }).unwrap();
            },
        ));
    }

    // Simulate map operation (allocate new cons cells)
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        let nil_a = array_arena.alloc(Value::Nil).unwrap();
        let nil_v = vec_arena.alloc(Value::Nil).unwrap();

        // Build source lists
        let mut src_a = nil_a;
        let mut src_v = nil_v;
        for i in 0..20 {
            let num_a = array_arena.alloc(Value::Number(i)).unwrap();
            let num_v = vec_arena.alloc(Value::Number(i)).unwrap();
            src_a = array_arena.alloc(Value::Cons { car: num_a, cdr: src_a }).unwrap();
            src_v = vec_arena.alloc(Value::Cons { car: num_v, cdr: src_v }).unwrap();
        }

        results.push(run_bench(
            "Simulate map (x*x) over 20 elements x 200",
            &array_arena,
            &vec_arena,
            200,
            |arena| {
                // Walk list and build new list with squared values
                let mut result = nil_a;
                let mut current = src_a;

                while let Ok(Value::Cons { car, cdr }) = arena.get(current) {
                    if let Ok(Value::Number(n)) = arena.get(car) {
                        let squared = arena.alloc(Value::Number(n * n)).unwrap();
                        result = arena.alloc(Value::Cons { car: squared, cdr: result }).unwrap();
                    }
                    current = cdr;
                }
                let _ = result;
            },
            |arena| {
                let mut result = nil_v;
                let mut current = src_v;

                while let Ok(Value::Cons { car, cdr }) = arena.get(current) {
                    if let Ok(Value::Number(n)) = arena.get(car) {
                        let squared = arena.alloc(Value::Number(n * n)).unwrap();
                        result = arena.alloc(Value::Cons { car: squared, cdr: result }).unwrap();
                    }
                    current = cdr;
                }
                let _ = result;
            },
        ));
    }

    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 8: Large Scale Operations
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 8: Large Scale Operations");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Allocate large number of values
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);
        let vec_storage = VecStorage::<Value>::with_capacity(CAPACITY);
        let vec_arena: GenericArena<Value, VecStorage<Value>> = GenericArena::with_storage(vec_storage);

        println!("Running: Allocate 10000 values sequentially...");

        let array_start = Instant::now();
        for i in 0..10000 {
            let _ = array_arena.alloc(Value::Number(i as isize));
        }
        let array_duration = array_start.elapsed();

        let vec_start = Instant::now();
        for i in 0..10000 {
            let _ = vec_arena.alloc(Value::Number(i as isize));
        }
        let vec_duration = vec_start.elapsed();

        results.push(BenchResult {
            name: "Allocate 10000 values sequentially".to_string(),
            array_duration,
            vec_duration,
            iterations: 10000,
            passed: true,
            note: None,
        });
    }

    // Heavy GC workload (ArrayStorage only)
    {
        let array_arena: Arena<Value, CAPACITY> = Arena::new(Value::Nil);

        let nil_a = array_arena.alloc(Value::Nil).unwrap();

        // Keep building lists and GC'ing
        println!("Running: Repeated allocation + GC cycles x 50 (ArrayStorage only)...");

        let array_start = Instant::now();
        for _ in 0..50 {
            // Build a list
            let mut list = nil_a;
            for i in 0..100 {
                let num = array_arena.alloc(Value::Number(i)).unwrap();
                list = array_arena.alloc(Value::Cons { car: num, cdr: list }).unwrap();
            }
            // GC with list as root
            array_arena.collect_garbage(&[list, nil_a]);
        }
        let array_duration = array_start.elapsed();

        results.push(BenchResult {
            name: "Repeated allocation + GC cycles x 50 (ArrayStorage only)".to_string(),
            array_duration,
            vec_duration: Duration::ZERO,
            iterations: 50,
            passed: true,
            note: Some("100 cons cells per cycle, VecStorage GC not implemented".to_string()),
        });
    }

    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // Summary
    // ═══════════════════════════════════════════════════════════════════════
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                        SUMMARY                               ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    let total_array_time: Duration = results.iter().map(|r| r.array_duration).sum();
    let total_vec_time: Duration = results.iter().map(|r| r.vec_duration).sum();
    let passed = results.iter().filter(|r| r.passed).count();

    for result in &results {
        result.print();
        println!();
    }

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Total: {} tests, {} passed", results.len(), passed);
    println!();
    println!("Total benchmark time:");
    println!("  ArrayStorage: {:?}", total_array_time);
    println!("  VecStorage:   {:?}", total_vec_time);

    let overall_ratio = if total_vec_time.as_nanos() > 0 {
        total_array_time.as_nanos() as f64 / total_vec_time.as_nanos() as f64
    } else {
        1.0
    };

    println!();
    if overall_ratio > 1.05 {
        println!("Overall: VecStorage is {:.2}x faster", overall_ratio);
    } else if overall_ratio < 0.95 {
        println!("Overall: ArrayStorage is {:.2}x faster", 1.0 / overall_ratio);
    } else {
        println!("Overall: Performance is roughly equal");
    }

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Notes:");
    println!("  - ArrayStorage uses compile-time fixed capacity (const generics)");
    println!("  - VecStorage uses runtime heap allocation");
    println!("  - Both use Cell<Slot<T>> internally for interior mutability");
    println!("  - Performance differences typically come from cache locality");
    println!("  - VecStorage may benefit from dynamic capacity sizing");
}
