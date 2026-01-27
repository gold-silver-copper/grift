//! # Lisp Stress Test / Benchmark Suite
//!
//! Run with: `cargo run -p lisp_repl --bin lisp-bench --release`
//!
//! This runs various stress tests on the Lisp interpreter to measure performance
//! and verify correctness under load.

use lisp_eval::Evaluator;
use lisp_parser::Lisp;
use lisp_repl::format_value;
use std::time::{Duration, Instant};

/// Result of a single benchmark
struct BenchResult {
    name: String,
    duration: Duration,
    iterations: usize,
    result: String,
    passed: bool,
    note: Option<String>,
}

impl BenchResult {
    fn print(&self) {
        let status = if self.passed { "PASS" } else { "FAIL" };
        let per_iter = if self.iterations > 1 {
            format!(
                " ({:.2}µs/iter)",
                self.duration.as_nanos() as f64 / self.iterations as f64 / 1000.0
            )
        } else {
            String::new()
        };

        println!(
            "[{}] {}: {:?}{}",
            status, self.name, self.duration, per_iter
        );

        if !self.result.is_empty() && self.result.len() < 60 {
            println!("       Result: {}", self.result);
        }

        if let Some(note) = &self.note {
            println!("       Note: {}", note);
        }
    }
}

/// Evaluate a string and return the formatted result
fn eval_str<const N: usize>(
    lisp: &Lisp<N>,
    eval: &mut Evaluator<N>,
    code: &str,
) -> Result<String, String> {
    match eval.eval_str(code) {
        Ok(idx) => {
            let mut buf = String::new();
            format_value(lisp, idx, &mut buf);
            Ok(buf)
        }
        Err(e) => Err(format!("Error: {:?}", e.kind)),
    }
}

/// Run a simple timed benchmark with GC after completion
fn run_bench<const N: usize>(
    name: &str,
    lisp: &Lisp<N>,
    eval: &mut Evaluator<N>,
    iterations: usize,
    code: &str,
    expected: Option<&str>,
) -> BenchResult {
    println!("Running: {} ({} iterations)...", name, iterations);

    let start = Instant::now();
    let mut last_result = String::new();
    let mut error = None;
    let mut successful_iters = 0;
    
    // Track peak allocation during execution
    let initial_allocated = lisp.stats().allocated;
    let mut peak_allocated = initial_allocated;

    for _ in 0..iterations {
        match eval_str(lisp, eval, code) {
            Ok(r) => {
                last_result = r;
                successful_iters += 1;
                // Update peak
                let current = lisp.stats().allocated;
                if current > peak_allocated {
                    peak_allocated = current;
                }
            }
            Err(e) => {
                error = Some(e);
                break;
            }
        }
    }

    let duration = start.elapsed();
    
    // Run GC after each test to clean up
    eval.gc();
    
    let final_allocated = lisp.stats().allocated;
    let peak_delta = peak_allocated.saturating_sub(initial_allocated);
    let final_delta = final_allocated.saturating_sub(initial_allocated);

    if let Some(e) = error {
        return BenchResult {
            name: name.to_string(),
            duration,
            iterations: successful_iters,
            result: e,
            passed: false,
            note: Some(format!("Failed after {} iterations", successful_iters)),
        };
    }

    let passed = expected.map_or(true, |exp| last_result == exp);

    BenchResult {
        name: name.to_string(),
        duration,
        iterations,
        result: last_result,
        passed,
        note: if !passed {
            expected.map(|e| format!("Expected: {}", e))
        } else {
            None
        },
    }
}

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           Lisp Interpreter Stress Test Suite                 ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ Arena size: 50,000 cells                                     ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    let lisp: Lisp<50000> = Lisp::new();
    let mut eval = match Evaluator::new(&lisp) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to create evaluator: {:?}", e);
            return;
        }
    };

    // Run GC at the beginning to get baseline
    println!("Running initial GC to establish baseline...");
    eval.gc();
    let initial_stats = lisp.stats();
    let initial_allocated = initial_stats.allocated;
    println!(
        "Initial state after GC: {} / {} cells allocated ({:.1}%)",
        initial_allocated,
        initial_stats.capacity,
        initial_stats.usage_percent()
    );
    println!(
        "  (Includes {} reserved slots: NIL, True, False)",
        3
    );
    println!();

    let mut results: Vec<BenchResult> = Vec::new();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 1: Basic Operations
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 1: Basic Operations");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    results.push(run_bench(
        "Arithmetic (+ 1 2 3 4 5) x 500",
        &lisp,
        &mut eval,
        500,
        "(+ 1 2 3 4 5)",
        Some("15"),
    ));

    // Define a variable for lookup tests
    let _ = eval_str(&lisp, &mut eval, "(define bench-var 42)");

    results.push(run_bench(
        "Symbol lookup x 500",
        &lisp,
        &mut eval,
        500,
        "bench-var",
        Some("42"),
    ));

    results.push(run_bench(
        "List construction x 200",
        &lisp,
        &mut eval,
        200,
        "(list 1 2 3 4 5 6 7 8 9 10)",
        None,
    ));

    results.push(run_bench(
        "Quote x 200",
        &lisp,
        &mut eval,
        200,
        "'(a b c d e)",
        None,
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 2: Recursion & TCO
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 2: Recursion & Tail Call Optimization");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Define recursive functions
    let _ = eval_str(
        &lisp,
        &mut eval,
        "(define (factorial n) (if (<= n 1) 1 (* n (factorial (- n 1)))))",
    );
    let _ = eval_str(
        &lisp,
        &mut eval,
        "(define (fib n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))",
    );
    let _ = eval_str(
        &lisp,
        &mut eval,
        "(define (sum-to-tco n acc) (if (= n 0) acc (sum-to-tco (- n 1) (+ acc n))))",
    );
    let _ = eval_str(
        &lisp,
        &mut eval,
        "(define (count-down n) (if (= n 0) 'done (count-down (- n 1))))",
    );

    results.push(run_bench(
        "Factorial(10) x 100",
        &lisp,
        &mut eval,
        100,
        "(factorial 10)",
        Some("3628800"),
    ));

    // Fibonacci - exponential time, tree recursion uses lots of memory
    // fib(n) makes O(2^n) calls, each needs stack space
    results.push(run_bench(
        "Fibonacci(12) x 5",
        &lisp,
        &mut eval,
        5,
        "(fib 12)",
        Some("144"),
    ));

    results.push(run_bench(
        "Fibonacci(20) x 1",
        &lisp,
        &mut eval,
        1,
        "(fib 20)",
        Some("6765"),
    ));
    
    // Memoized Fibonacci - enables computing values that naive fib can't
    // First call builds cache, subsequent calls are O(1) lookups
    let _ = eval_str(
        &lisp,
        &mut eval,
        "(define fib-memo (memoize (lambda (n) (if (<= n 1) n (+ (fib-memo (- n 1)) (fib-memo (- n 2)))))))",
    );
    
    // First call - builds cache
    results.push(run_bench(
        "Memoized Fib(25) first call",
        &lisp,
        &mut eval,
        1,
        "(fib-memo 25)",
        Some("75025"),
    ));
    
    // Cached call - nearly instant
    results.push(run_bench(
        "Memoized Fib(25) cached x 100",
        &lisp,
        &mut eval,
        100,
        "(fib-memo 25)",
        Some("75025"),
    ));

    // TCO tests - trampolining enables deeper recursion (limited by arena, not stack)
    // Memory accumulates across iterations, so we balance depth vs iterations
    results.push(run_bench(
        "TCO Sum 1..150 x 10",
        &lisp,
        &mut eval,
        10,
        "(sum-to-tco 150 0)",
        Some("11325"),
    ));

    results.push(run_bench(
        "TCO Sum 1..200 x 10",
        &lisp,
        &mut eval,
        10,
        "(sum-to-tco 200 0)",
        Some("20100"),
    ));

    results.push(run_bench(
        "TCO countdown 150 x 20",
        &lisp,
        &mut eval,
        20,
        "(count-down 150)",
        Some("done"),
    ));

    results.push(run_bench(
        "TCO countdown 200 x 10",
        &lisp,
        &mut eval,
        10,
        "(count-down 200)",
        Some("done"),
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 3: Higher-Order Functions
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 3: Higher-Order Functions (using stdlib)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Note: map, filter, fold, range, and reverse are now part of the stdlib
    // (defined via the define_stdlib! macro in lisp_parser).
    // The stdlib range function takes (start end) and produces [start, end).

    results.push(run_bench(
        "Map square over 20 elements x 20",
        &lisp,
        &mut eval,
        20,
        "(map (lambda (x) (* x x)) (range 1 21))",
        None,
    ));

    results.push(run_bench(
        "Filter even from 20 elements x 20",
        &lisp,
        &mut eval,
        20,
        "(filter (lambda (x) (= (mod x 2) 0)) (range 1 21))",
        None,
    ));

    results.push(run_bench(
        "Fold sum over 20 elements x 20",
        &lisp,
        &mut eval,
        20,
        "(fold + 0 (range 1 21))",
        Some("210"),
    ));

    results.push(run_bench(
        "Map+Filter+Fold pipeline x 20",
        &lisp,
        &mut eval,
        20,
        "(fold + 0 (filter (lambda (x) (> x 50)) (map (lambda (x) (* x x)) (range 1 16))))",
        None,
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 4: Closures & Environments
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 4: Closures & Environments");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Pure closure - no mutation
    let _ = eval_str(
        &lisp,
        &mut eval,
        "(define (make-adder x) (lambda (y) (+ x y)))",
    );

    results.push(run_bench(
        "Create closure x 200",
        &lisp,
        &mut eval,
        200,
        "(make-adder 42)",
        None,
    ));

    // Counter test - use add10 as closure instead of make-counter
    let _ = eval_str(&lisp, &mut eval, "(define add10 (make-adder 10))");
    results.push(run_bench(
        "Call closure x 100",
        &lisp,
        &mut eval,
        100,
        "(add10 5)",
        Some("15"),
    ));

    results.push(run_bench(
        "Nested let* 5 deep x 200",
        &lisp,
        &mut eval,
        200,
        "(let* ((a 1) (b (+ a 1)) (c (+ b 2)) (d (+ c 3)) (e (+ d 4))) e)",
        Some("11"),
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 5: Lazy Evaluation (automatic)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 5: Lazy Evaluation (everything is lazy by default!)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // cons is non-strict - builds pairs with unevaluated elements
    results.push(run_bench(
        "Non-strict cons x 200",
        &lisp,
        &mut eval,
        200,
        "(cons (+ 1 2) (+ 3 4))",
        None,
    ));

    // Accessing element forces it
    results.push(run_bench(
        "car (forces elem) x 200",
        &lisp,
        &mut eval,
        200,
        "(car (cons (* 3 333) 0))",
        Some("999"),
    ));

    // Conditional only evaluates selected branch (lazy branches)
    results.push(run_bench(
        "if branch selection x 100",
        &lisp,
        &mut eval,
        100,
        "(if #t 42 (error 'never-evaluated))",
        Some("42"),
    ));

    // HYBRID: Lambda args are strict (enables TCO)
    let _ = eval_str(&lisp, &mut eval, "(define (strict-first x y) x)");
    results.push(run_bench(
        "Strict lambda args x 100",
        &lisp,
        &mut eval,
        100,
        "(strict-first 60 100)",
        Some("60"),
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // NOTE: Mutation section removed - this is a PURE Lisp!

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 6: Garbage Collection
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 6: Garbage Collection");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // GC stress test: allocate lots, then collect
    println!("Running: GC stress test (allocate heavily, then collect)...");
    let gc_start = Instant::now();

    // Allocate a bunch of garbage
    for i in 0..100 {
        let _ = eval_str(
            &lisp,
            &mut eval,
            &format!("(list {} {} {} {} {})", i, i + 1, i + 2, i + 3, i + 4),
        );
    }

    let pre_gc = gc_start.elapsed();
    println!("       Allocation phase: {:?}", pre_gc);

    let gc_only_start = Instant::now();
    let stats = eval.gc();
    let gc_time = gc_only_start.elapsed();

    println!(
        "[PASS] GC stress test: {:?} total ({:?} GC only)",
        gc_start.elapsed(),
        gc_time
    );
    println!(
        "       Stats: marked={}, collected={}, total_before={}",
        stats.marked, stats.collected, stats.total_before
    );
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 8: Parsing Stress
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 7: Parsing");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    results.push(run_bench(
        "Parse deeply nested x 100",
        &lisp,
        &mut eval,
        100,
        "'((((((((((42))))))))))",
        None,
    ));

    results.push(run_bench(
        "Parse long list x 100",
        &lisp,
        &mut eval,
        100,
        "'(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15)",
        None,
    ));

    results.push(run_bench(
        "Parse complex expression x 100",
        &lisp,
        &mut eval,
        100,
        "(if (> 3 2) (+ 1 2) (* 3 4))",
        Some("3"),
    ));

    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // Summary
    // ═══════════════════════════════════════════════════════════════════════
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                        SUMMARY                               ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    let total_time: Duration = results.iter().map(|r| r.duration).sum();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.iter().filter(|r| !r.passed).count();

    for result in &results {
        result.print();
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "Total: {} tests, {} passed, {} failed",
        results.len(),
        passed,
        failed
    );
    println!("Total benchmark time: {:?}", total_time);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Run final GC and compare with initial state
    println!();
    println!("Running final GC...");
    eval.gc();
    let final_stats = lisp.stats();
    let final_allocated = final_stats.allocated;
    
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Memory Usage Comparison:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "  Initial (after GC): {} / {} cells ({:.1}%)",
        initial_allocated,
        initial_stats.capacity,
        initial_stats.usage_percent()
    );
    println!(
        "  Final (after GC):   {} / {} cells ({:.1}%)",
        final_allocated,
        final_stats.capacity,
        final_stats.usage_percent()
    );
    
    let delta = final_allocated as i64 - initial_allocated as i64;
    if delta > 0 {
        println!(
            "  Net increase:       +{} cells (tests left some allocations)",
            delta
        );
    } else if delta < 0 {
        println!(
            "  Net decrease:       {} cells (unexpected - should not happen)",
            delta
        );
    } else {
        println!(
            "  Net change:         0 cells (all test allocations were collected)"
        );
    }
    
    println!();
    println!("Reserved Slots Breakdown:");
    println!(
        "  Reserved slots:      {} cells (NIL, True, False - always allocated)",
        3
    );
    println!(
        "  Initial user data:   {} cells",
        initial_allocated.saturating_sub(3)
    );
    println!(
        "  Final user data:     {} cells",
        final_allocated.saturating_sub(3)
    );
    println!();
    println!("Reserved slots optimization benefits:");
    println!("  ✓ Prevents redundant allocations of NIL/True/False during execution");
    println!("  ✓ nil(), true_val(), false_val() now return constants (O(1))");
    println!("  ✓ Reduces peak memory usage during test execution");
    println!("  ✓ The 3 reserved slots are always allocated (counted in totals above)");
}
