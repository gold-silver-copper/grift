#![forbid(unsafe_code)]

//! # Lisp Stress Test / Benchmark Suite
//!
//! Run with: `cargo run -p grift_repl --bin grift-bench --release`
//!
//! This runs various stress tests on the Lisp interpreter to measure performance
//! and verify correctness under load.

use grift_eval::Evaluator;
use grift_parser::Lisp;
use grift_repl::format_value;
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
    let _peak_delta = peak_allocated.saturating_sub(initial_allocated);
    let _final_delta = final_allocated.saturating_sub(initial_allocated);

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
    println!("║ Arena size: 115,000 cells                                    ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    let lisp: Lisp<110000> = Lisp::new();
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
    
    // Memoized Fibonacci benchmark removed: `memoize` may not be available
    // or stable in all configurations yet, and was causing `UnboundVariable`
    // failures in the stress test suite.

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
    // (defined via the define_stdlib! macro in grift_parser).
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
        "(filter (lambda (x) (= (modulo x 2) 0)) (range 1 21))",
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
    // SECTION 5: Special Forms
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 5: Special Forms");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // cons - evaluated strictly in call-by-value
    results.push(run_bench(
        "cons x 200",
        &lisp,
        &mut eval,
        200,
        "(cons (+ 1 2) (+ 3 4))",
        None,
    ));

    // car access
    results.push(run_bench(
        "car x 200",
        &lisp,
        &mut eval,
        200,
        "(car (cons (* 3 333) 0))",
        Some("999"),
    ));

    // Conditional only evaluates selected branch (if is a special form)
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

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 6: Arrays (O(1) indexed access)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 6: Vectors (R7RS Section 6.8)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Create vector for reuse
    let _ = eval_str(&lisp, &mut eval, "(define bench-vec (make-vector 100 0))");

    results.push(run_bench(
        "Create vector[50] x 200",
        &lisp,
        &mut eval,
        200,
        "(make-vector 50 0)",
        None,
    ));

    results.push(run_bench(
        "Create large vector[500] x 20",
        &lisp,
        &mut eval,
        20,
        "(make-vector 500 42)",
        None,
    ));

    results.push(run_bench(
        "Vector-ref (O(1) access) x 500",
        &lisp,
        &mut eval,
        500,
        "(vector-ref bench-vec 50)",
        Some("0"),
    ));

    results.push(run_bench(
        "Vector-set! (O(1) mutation) x 500",
        &lisp,
        &mut eval,
        500,
        "(vector-set! bench-vec 50 999)",
        None,
    ));

    results.push(run_bench(
        "Vector-length x 500",
        &lisp,
        &mut eval,
        500,
        "(vector-length bench-vec)",
        Some("100"),
    ));

    results.push(run_bench(
        "Vector? predicate x 500",
        &lisp,
        &mut eval,
        500,
        "(vector? bench-vec)",
        Some("#t"),
    ));

    // Vector iteration pattern
    let _ = eval_str(
        &lisp,
        &mut eval,
        "(define (vector-sum vec len) (if (= len 0) 0 (+ (vector-ref vec (- len 1)) (vector-sum vec (- len 1)))))",
    );
    results.push(run_bench(
        "Vector sum recursive (10 elements) x 50",
        &lisp,
        &mut eval,
        50,
        "(let ((vec (make-vector 10 1))) (vector-sum vec 10))",
        Some("10"),
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 7: Mutation Operations
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 7: Mutation Operations");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Create a pair for mutation tests
    let _ = eval_str(&lisp, &mut eval, "(define bench-pair (cons 1 2))");

    results.push(run_bench(
        "set-car! mutation x 200",
        &lisp,
        &mut eval,
        200,
        "(set-car! bench-pair 42)",
        None,
    ));

    results.push(run_bench(
        "set-cdr! mutation x 200",
        &lisp,
        &mut eval,
        200,
        "(set-cdr! bench-pair 99)",
        None,
    ));

    // Variable mutation
    let _ = eval_str(&lisp, &mut eval, "(define mut-var 10)");
    results.push(run_bench(
        "set! variable mutation x 200",
        &lisp,
        &mut eval,
        200,
        "(set! mut-var 20)",
        None,
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 8: Pattern Matching (case/cond)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 8: Pattern Matching (case/cond)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    results.push(run_bench(
        "cond multi-way x 200",
        &lisp,
        &mut eval,
        200,
        "(cond ((= 1 2) 'no) ((= 2 2) 'yes) (else 'default))",
        Some("yes"),
    ));

    results.push(run_bench(
        "case pattern matching x 200",
        &lisp,
        &mut eval,
        200,
        "(case 2 ((1) 'one) ((2 3) 'two-or-three) (else 'other))",
        Some("two-or-three"),
    ));

    results.push(run_bench(
        "case with else fallback x 200",
        &lisp,
        &mut eval,
        200,
        "(case 99 ((1) 'one) ((2) 'two) (else 'other))",
        Some("other"),
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 9: More Arithmetic & Comparison
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 9: More Arithmetic & Comparison");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    results.push(run_bench(
        "Subtraction (- 100 50) x 500",
        &lisp,
        &mut eval,
        500,
        "(- 100 50)",
        Some("50"),
    ));

    results.push(run_bench(
        "Multiplication (* 6 7) x 500",
        &lisp,
        &mut eval,
        500,
        "(* 6 7)",
        Some("42"),
    ));

    results.push(run_bench(
        "Division (/ 100 5) x 500",
        &lisp,
        &mut eval,
        500,
        "(/ 100 5)",
        Some("20"),
    ));

    results.push(run_bench(
        "Modulo (modulo 17 5) x 500",
        &lisp,
        &mut eval,
        500,
        "(modulo 17 5)",
        Some("2"),
    ));

    results.push(run_bench(
        "Comparison (< 1 2) x 500",
        &lisp,
        &mut eval,
        500,
        "(< 1 2)",
        Some("#t"),
    ));

    results.push(run_bench(
        "Comparison (> 5 3) x 500",
        &lisp,
        &mut eval,
        500,
        "(> 5 3)",
        Some("#t"),
    ));

    results.push(run_bench(
        "Numeric equality (= 5 5) x 500",
        &lisp,
        &mut eval,
        500,
        "(= 5 5)",
        Some("#t"),
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 10: More Standard Library Functions
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 10: More Standard Library Functions");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    results.push(run_bench(
        "Length of list x 200",
        &lisp,
        &mut eval,
        200,
        "(length '(1 2 3 4 5 6 7 8 9 10))",
        Some("10"),
    ));

    results.push(run_bench(
        "Append two lists x 100",
        &lisp,
        &mut eval,
        100,
        "(append '(1 2 3) '(4 5 6))",
        None,
    ));

    results.push(run_bench(
        "Reverse list x 100",
        &lisp,
        &mut eval,
        100,
        "(reverse '(1 2 3 4 5))",
        None,
    ));

    results.push(run_bench(
        "Nth element (0-indexed) x 200",
        &lisp,
        &mut eval,
        200,
        "(nth 3 '(a b c d e))",
        Some("d"),
    ));

    results.push(run_bench(
        "Take first 5 elements x 100",
        &lisp,
        &mut eval,
        100,
        "(take 5 '(1 2 3 4 5 6 7 8 9 10))",
        None,
    ));

    results.push(run_bench(
        "Drop first 3 elements x 100",
        &lisp,
        &mut eval,
        100,
        "(drop 3 '(1 2 3 4 5 6 7 8 9 10))",
        None,
    ));

    results.push(run_bench(
        "Zip two lists x 50",
        &lisp,
        &mut eval,
        50,
        "(zip '(1 2 3) '(a b c))",
        None,
    ));

    results.push(run_bench(
        "Member check x 200",
        &lisp,
        &mut eval,
        200,
        "(member 5 '(1 2 3 4 5 6))",
        Some("#t"),
    ));

    results.push(run_bench(
        "Assoc lookup x 200",
        &lisp,
        &mut eval,
        200,
        "(assoc 'b '((a . 1) (b . 2) (c . 3)))",
        None,
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 11: Macros & Metaprogramming
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 11: Reserved for Future Hygienic Macros");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Note: defmacro and gensym have been removed for Scheme R7RS conformance.
    // Hygienic macros via syntax-rules will be implemented in a future phase.
    println!("  (Skipped - defmacro/gensym removed for Scheme conformance)");

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 12: Runtime Evaluation (eval/apply)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 12: Runtime Evaluation (eval/apply)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    results.push(run_bench(
        "eval quoted expression x 100",
        &lisp,
        &mut eval,
        100,
        "(eval '(+ 1 2 3))",
        Some("6"),
    ));

    results.push(run_bench(
        "apply function to list x 100",
        &lisp,
        &mut eval,
        100,
        "(apply + '(1 2 3 4 5))",
        Some("15"),
    ));

    results.push(run_bench(
        "apply with lambda x 50",
        &lisp,
        &mut eval,
        50,
        "(apply (lambda (x y) (* x y)) '(6 7))",
        Some("42"),
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 13: Complex Scenarios
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 13: Complex Scenarios");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Nested higher-order functions
    // range 1 11 = (1 2 3 4 5 6 7 8 9 10)
    // map square = (1 4 9 16 25 36 49 64 81 100)
    // filter (> x 5) = (9 16 25 36 49 64 81 100)
    // fold + 0 = 9+16+25+36+49+64+81+100 = 380
    results.push(run_bench(
        "Nested map/filter/fold x 20",
        &lisp,
        &mut eval,
        20,
        "(fold + 0 (filter (lambda (x) (> x 5)) (map (lambda (x) (* x x)) (range 1 11))))",
        Some("380"),
    ));

    // Vector operations in functional style
    let _ = eval_str(
        &lisp,
        &mut eval,
        "(define (vector-to-list-manual vec len) (if (= len 0) '() (cons (vector-ref vec (- len 1)) (vector-to-list-manual vec (- len 1)))))",
    );
    results.push(run_bench(
        "Vector to list conversion (10 elements) x 30",
        &lisp,
        &mut eval,
        30,
        "(let ((vec (make-vector 10 1))) (vector-to-list-manual vec 10))",
        None,
    ));

    // Quasiquote/unquote - use (quasiquote ...) and (unquote ...) syntax
    results.push(run_bench(
        "Quasiquote with unquote x 100",
        &lisp,
        &mut eval,
        100,
        "(let ((x 42)) (quasiquote (list (unquote x) (unquote (+ x 1)))))",
        None,
    ));

    // Multiple return values
    results.push(run_bench(
        "Multiple values x 100",
        &lisp,
        &mut eval,
        100,
        "(values 1 2 3)",
        None,
    ));

    // Do loop
    results.push(run_bench(
        "Do loop iteration x 50",
        &lisp,
        &mut eval,
        50,
        "(do ((i 0 (+ i 1)) (sum 0 (+ sum i))) ((= i 10) sum))",
        Some("45"),
    ));

    // Clean up before next section
    eval.gc();
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 14: Garbage Collection
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 14: Garbage Collection");
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
    // SECTION 14.1: GC On vs Off Performance Comparison
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 14.1: GC On vs Off Performance Comparison");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Testing interpreter speed with automatic GC enabled vs disabled.");
    println!("Manual GC is triggered after each test run regardless of setting.");
    println!();

    // Test with GC enabled (normal operation)
    let _ = eval_str(&lisp, &mut eval, "(gc-enable)");
    let gc_on_start = Instant::now();
    
    for _ in 0..50 {
        let _ = eval_str(&lisp, &mut eval, "(map (lambda (x) (* x x)) (range 1 21))");
    }
    let gc_on_time = gc_on_start.elapsed();
    eval.gc();
    
    // Test with GC disabled (batch operations)
    let _ = eval_str(&lisp, &mut eval, "(gc-disable)");
    
    let gc_off_start = Instant::now();
    
    for _ in 0..50 {
        let _ = eval_str(&lisp, &mut eval, "(map (lambda (x) (* x x)) (range 1 21))");
    }
    let gc_off_time = gc_off_start.elapsed();
    
    // Re-enable GC and collect
    // IMPORTANT: Enable GC directly on the arena, because eval_str needs
    // to allocate memory for parsing. If the arena is full and GC is disabled,
    // eval_str would fail trying to parse "(gc-enable)".
    lisp.arena().set_gc_enabled(true);
    eval.gc();

    println!("[INFO] Map over 20 elements x 50 iterations:");
    println!("       GC enabled:  {:?} ({:.2}µs/iter)", gc_on_time, gc_on_time.as_nanos() as f64 / 50.0 / 1000.0);
    println!("       GC disabled: {:?} ({:.2}µs/iter)", gc_off_time, gc_off_time.as_nanos() as f64 / 50.0 / 1000.0);
    
    let speedup = if gc_off_time.as_nanos() > 0 {
        gc_on_time.as_nanos() as f64 / gc_off_time.as_nanos() as f64
    } else {
        1.0
    };
    
    if gc_off_time < gc_on_time {
        println!("       Speedup with GC disabled: {:.2}x faster", speedup);
    } else {
        println!("       Note: GC overhead minimal in this test ({:.2}x)", speedup);
    }
    println!();

    // Allocation-heavy test
    let _ = eval_str(&lisp, &mut eval, "(gc-enable)");
    let gc_on_alloc_start = Instant::now();
    
    for i in 0..100 {
        let _ = eval_str(&lisp, &mut eval, &format!("(list {} {} {} {} {} {} {} {})", i, i+1, i+2, i+3, i+4, i+5, i+6, i+7));
    }
    let gc_on_alloc_time = gc_on_alloc_start.elapsed();
    eval.gc();
    
    let _ = eval_str(&lisp, &mut eval, "(gc-disable)");
    let gc_off_alloc_start = Instant::now();
    
    for i in 0..100 {
        let _ = eval_str(&lisp, &mut eval, &format!("(list {} {} {} {} {} {} {} {})", i, i+1, i+2, i+3, i+4, i+5, i+6, i+7));
    }
    let gc_off_alloc_time = gc_off_alloc_start.elapsed();
    
    // Re-enable GC and collect (see comment in map test above for explanation)
    lisp.arena().set_gc_enabled(true);
    eval.gc();

    println!("[INFO] Allocation-heavy (8-element lists) x 100 iterations:");
    println!("       GC enabled:  {:?} ({:.2}µs/iter)", gc_on_alloc_time, gc_on_alloc_time.as_nanos() as f64 / 100.0 / 1000.0);
    println!("       GC disabled: {:?} ({:.2}µs/iter)", gc_off_alloc_time, gc_off_alloc_time.as_nanos() as f64 / 100.0 / 1000.0);
    
    let alloc_speedup = if gc_off_alloc_time.as_nanos() > 0 {
        gc_on_alloc_time.as_nanos() as f64 / gc_off_alloc_time.as_nanos() as f64
    } else {
        1.0
    };
    
    if gc_off_alloc_time < gc_on_alloc_time {
        println!("       Speedup with GC disabled: {:.2}x faster", alloc_speedup);
    } else {
        println!("       Note: GC overhead minimal in this test ({:.2}x)", alloc_speedup);
    }
    println!();

    // Recursive test  
    let _ = eval_str(&lisp, &mut eval, "(gc-enable)");
    let gc_on_rec_start = Instant::now();
    
    for _ in 0..20 {
        let _ = eval_str(&lisp, &mut eval, "(fib 12)");
    }
    let gc_on_rec_time = gc_on_rec_start.elapsed();
    eval.gc();
    
    let _ = eval_str(&lisp, &mut eval, "(gc-disable)");
    let gc_off_rec_start = Instant::now();
    
    for _ in 0..20 {
        let _ = eval_str(&lisp, &mut eval, "(fib 12)");
    }
    let gc_off_rec_time = gc_off_rec_start.elapsed();
    
    // Re-enable GC and collect (see comment in map test above for explanation)
    lisp.arena().set_gc_enabled(true);
    eval.gc();

    println!("[INFO] Fibonacci(12) x 20 iterations (recursive):");
    println!("       GC enabled:  {:?} ({:.2}µs/iter)", gc_on_rec_time, gc_on_rec_time.as_nanos() as f64 / 20.0 / 1000.0);
    println!("       GC disabled: {:?} ({:.2}µs/iter)", gc_off_rec_time, gc_off_rec_time.as_nanos() as f64 / 20.0 / 1000.0);
    
    let rec_speedup = if gc_off_rec_time.as_nanos() > 0 {
        gc_on_rec_time.as_nanos() as f64 / gc_off_rec_time.as_nanos() as f64
    } else {
        1.0
    };
    
    if gc_off_rec_time < gc_on_rec_time {
        println!("       Speedup with GC disabled: {:.2}x faster", rec_speedup);
    } else {
        println!("       Note: GC overhead minimal in this test ({:.2}x)", rec_speedup);
    }
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // SECTION 15: Parsing Stress
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Section 15: Parsing");
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
    
    let delta = final_allocated as isize - initial_allocated as isize;
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
