//! Comprehensive benchmark suite for the Grift Lisp interpreter.
//!
//! Tests naive fib, TCO fib, iterative fib, arithmetic, list operations,
//! higher-order functions, closures, recursion patterns, and more.
//!
//! ```sh
//! cargo run -p grift --example fib_bench --release
//! ```

use grift::{Lisp, Value};

fn main() {
    let builder = std::thread::Builder::new()
        .name("bench".into())
        .stack_size(64 * 1024 * 1024); // 64 MiB
    let handler = builder
        .spawn(run_benchmarks)
        .expect("failed to spawn thread");
    handler.join().expect("benchmark thread panicked");
}

// ── Helpers ──────────────────────────────────────────────────────────────────

struct BenchResult {
    name: &'static str,
    elapsed: std::time::Duration,
    output: String,
    ok: bool,
}

/// Run a single benchmark, returning its result.
fn bench<const N: usize>(
    lisp: &Lisp<N>,
    name: &'static str,
    program: &str,
    expected: Option<Value>,
) -> BenchResult {
    let start = std::time::Instant::now();
    let result = lisp.eval(program);
    let elapsed = start.elapsed();

    match result {
        Ok(val) => {
            let ok = expected.as_ref().map_or(true, |e| values_equal(&val, e));
            let output = format!("{val:?}");
            if !ok {
                eprintln!(
                    "  MISMATCH in {name}: got {output}, expected {:?}",
                    expected.unwrap()
                );
            }
            BenchResult {
                name,
                elapsed,
                output,
                ok,
            }
        }
        Err(e) => BenchResult {
            name,
            elapsed,
            output: format!("ERROR: {e:?}"),
            ok: false,
        },
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::Boolean(x), Value::Boolean(y)) => x == y,
        (Value::Nil, Value::Nil) => true,
        _ => false,
    }
}

fn print_report(results: &[BenchResult]) {
    let max_name = results.iter().map(|r| r.name.len()).max().unwrap_or(0);
    let max_out = results
        .iter()
        .map(|r| r.output.len())
        .max()
        .unwrap_or(0)
        .min(30);

    println!();
    println!("╔{:═<width$}╗", "", width = max_name + max_out + 32);
    println!(
        "║ {:^width$} ║",
        "GRIFT BENCHMARK RESULTS",
        width = max_name + max_out + 30
    );
    println!("╠{:═<width$}╣", "", width = max_name + max_out + 32);
    println!(
        "║ {:<nw$}  {:<ow$}  {:>12}  {:>6} ║",
        "Benchmark",
        "Result",
        "Time",
        "Status",
        nw = max_name,
        ow = max_out
    );
    println!("╠{:─<width$}╣", "", width = max_name + max_out + 32);

    let mut total = std::time::Duration::ZERO;
    let mut pass = 0usize;
    let mut fail = 0usize;

    for r in results {
        total += r.elapsed;
        if r.ok {
            pass += 1;
        } else {
            fail += 1;
        }
        let status = if r.ok { " OK " } else { "FAIL" };
        let truncated: String = r.output.chars().take(max_out).collect();
        println!(
            "║ {:<nw$}  {:<ow$}  {:>12.3?}  {:>6} ║",
            r.name,
            truncated,
            r.elapsed,
            status,
            nw = max_name,
            ow = max_out
        );
    }

    println!("╠{:─<width$}╣", "", width = max_name + max_out + 32);
    println!(
        "║ {:<nw$}  {:<ow$}  {:>12.3?}  {:>6} ║",
        "TOTAL",
        format!("{pass} pass, {fail} fail"),
        total,
        "",
        nw = max_name,
        ow = max_out
    );
    println!("╚{:═<width$}╝", "", width = max_name + max_out + 32);
}

// ── Benchmark Suite ──────────────────────────────────────────────────────────

fn run_benchmarks() {
    let lisp: Lisp<500000> = Lisp::new();
    let mut results: Vec<BenchResult> = Vec::new();

    // ── 1. Naive recursive Fibonacci ─────────────────────────────────────
    results.push(bench(
        &lisp,
        "fib-naive(30)",
        r#"
        (begin
          (define! (fib n)
            (if (<= n 1) n
              (+ (fib (- n 1)) (fib (- n 2)))))
          (fib 30))
        "#,
        Some(Value::Number(832040)),
    ));

    // ── 2. TCO Fibonacci (tail-recursive with accumulator) ───────────────
    results.push(bench(
        &lisp,
        "fib-tco(50)",
        r#"
        (begin
          (define! (fib-tco n a b)
            (if (= n 0) a
              (fib-tco (- n 1) b (+ a b))))
          (fib-tco 50 0 1))
        "#,
        Some(Value::Number(12586269025)),
    ));

    // ── 3. Iterative Fibonacci via let-loop ──────────────────────────────
    results.push(bench(
        &lisp,
        "fib-iter(50)",
        r#"
        (begin
          (define! (fib-iter n)
            (define! (loop i a b)
              (if (= i 0) a
                (loop (- i 1) b (+ a b))))
            (loop n 0 1))
          (fib-iter 50))
        "#,
        Some(Value::Number(12586269025)),
    ));

    // ── 4. Summation: sum 1..10000 (tail-recursive) ─────────────────────
    results.push(bench(
        &lisp,
        "sum-tco(10000)",
        r#"
        (begin
          (define! (sum n acc)
            (if (= n 0) acc
              (sum (- n 1) (+ acc n))))
          (sum 10000 0))
        "#,
        Some(Value::Number(50005000)),
    ));

    // ── 5. Countdown: TCO recursion depth stress-test ────────────────────
    results.push(bench(
        &lisp,
        "countdown(100000)",
        r#"
        (begin
          (define! (countdown n)
            (if (= n 0) 0
              (countdown (- n 1))))
          (countdown 100000))
        "#,
        Some(Value::Number(0)),
    ));

    // ── 6. Factorial (tail-recursive) ────────────────────────────────────
    results.push(bench(
        &lisp,
        "factorial-tco(20)",
        r#"
        (begin
          (define! (fact n acc)
            (if (= n 0) acc
              (fact (- n 1) (* acc n))))
          (fact 20 1))
        "#,
        Some(Value::Number(2432902008176640000)),
    ));

    // ── 7. Ackermann function (deeply recursive) ─────────────────────────
    results.push(bench(
        &lisp,
        "ackermann(3,7)",
        r#"
        (begin
          (define! (ack m n)
            (cond
              ((= m 0) (+ n 1))
              ((= n 0) (ack (- m 1) 1))
              (#t      (ack (- m 1) (ack m (- n 1))))))
          (ack 3 7))
        "#,
        Some(Value::Number(1021)),
    ));

    // ── 8. List building: build list of 1000 elements ────────────────────
    results.push(bench(
        &lisp,
        "list-build(1000)",
        r#"
        (begin
          (define! (build n acc)
            (if (= n 0) acc
              (build (- n 1) (cons n acc))))
          (define! lst (build 1000 (list)))
          (car lst))
        "#,
        Some(Value::Number(1)),
    ));

    // ── 9. List length (recursive traversal) ─────────────────────────────
    results.push(bench(
        &lisp,
        "list-length(1000)",
        r#"
        (begin
          (define! (build n acc)
            (if (= n 0) acc
              (build (- n 1) (cons n acc))))
          (define! (length lst)
            (define! (loop l acc)
              (if (null? l) acc
                (loop (cdr l) (+ acc 1))))
            (loop lst 0))
          (length (build 1000 (list))))
        "#,
        Some(Value::Number(1000)),
    ));

    // ── 10. List sum: sum all elements ───────────────────────────────────
    results.push(bench(
        &lisp,
        "list-sum(1000)",
        r#"
        (begin
          (define! (build n acc)
            (if (= n 0) acc
              (build (- n 1) (cons n acc))))
          (define! (sum-list lst)
            (define! (loop l acc)
              (if (null? l) acc
                (loop (cdr l) (+ acc (car l)))))
            (loop lst 0))
          (sum-list (build 1000 (list))))
        "#,
        Some(Value::Number(500500)),
    ));

    // ── 11. Map: apply function over list ────────────────────────────────
    results.push(bench(
        &lisp,
        "map-double(500)",
        r#"
        (begin
          (define! (build n acc)
            (if (= n 0) acc
              (build (- n 1) (cons n acc))))
          (define! (map f lst)
            (if (null? lst) (list)
              (cons (f (car lst)) (map f (cdr lst)))))
          (define! (double x) (* x 2))
          (define! result (map double (build 500 (list))))
          (car result))
        "#,
        Some(Value::Number(2)),
    ));

    // ── 13. Fold-left (reduce) ───────────────────────────────────────────
    results.push(bench(
        &lisp,
        "foldl-sum(1000)",
        r#"
        (begin
          (define! (build n acc)
            (if (= n 0) acc
              (build (- n 1) (cons n acc))))
          (define! (foldl f init lst)
            (if (null? lst) init
              (foldl f (f init (car lst)) (cdr lst))))
          (define! (add a b) (+ a b))
          (foldl add 0 (build 1000 (list))))
        "#,
        Some(Value::Number(500500)),
    ));

    // ── 14. Pure functional counter (accumulator) ──────────────────────
    results.push(bench(
        &lisp,
        "pure-counter(10000)",
        r#"
        (begin
          (define! (count n acc)
            (if (= n 0) acc
              (count (- n 1) (+ acc 1))))
          (count 10000 0))
        "#,
        Some(Value::Number(10000)),
    ));

    // ── 15. Mutual recursion: even?/odd? ─────────────────────────────────
    results.push(bench(
        &lisp,
        "mutual-recur(10000)",
        r#"
        (begin
          (define! (my-even? n)
            (if (= n 0) #t
              (my-odd? (- n 1))))
          (define! (my-odd? n)
            (if (= n 0) #f
              (my-even? (- n 1))))
          (my-even? 10000))
        "#,
        Some(Value::Boolean(true)),
    ));

    // ── 16. Nested closures / higher-order ───────────────────────────────
    results.push(bench(
        &lisp,
        "nested-closures",
        r#"
        (begin
          (define! (adder x)
            (lambda (y) (+ x y)))
          (define! add5 (adder 5))
          (define! add10 (adder 10))
          (define! (compose f g)
            (lambda (x) (f (g x))))
          (define! add15 (compose add5 add10))
          (add15 100))
        "#,
        Some(Value::Number(115)),
    ));

    // ── 17. Church numerals (lambda calculus encoding) ────────────────────
    results.push(bench(
        &lisp,
        "church-numerals",
        r#"
        (begin
          (define! (church-zero f) (lambda (x) x))
          (define! (church-succ n)
            (lambda (f) (lambda (x) (f ((n f) x)))))
          (define! (church-add m n)
            (lambda (f) (lambda (x) ((m f) ((n f) x)))))
          (define! (church->int n)
            ((n (lambda (x) (+ x 1))) 0))
          (define! c1 (church-succ church-zero))
          (define! c2 (church-succ c1))
          (define! c3 (church-succ c2))
          (define! c5 (church-add c2 c3))
          (church->int c5))
        "#,
        Some(Value::Number(5)),
    ));

    // ── 18. Exponentiation by squaring (fast power) ──────────────────────
    results.push(bench(
        &lisp,
        "fast-power(2^30)",
        r#"
        (begin
          (define! (mod a b) (- a (* b (/ a b))))
          (define! (even? n) (= (mod n 2) 0))
          (define! (fast-pow base exp)
            (cond
              ((= exp 0) 1)
              ((even? exp) (let ((half (fast-pow base (/ exp 2))))
                             (* half half)))
              (#t (* base (fast-pow base (- exp 1))))))
          (fast-pow 2 30))
        "#,
        Some(Value::Number(1073741824)),
    ));

    // ── 19. GCD via Euclidean algorithm ──────────────────────────────────
    results.push(bench(
        &lisp,
        "gcd-euclid",
        r#"
        (begin
          (define! (mod a b) (- a (* b (/ a b))))
          (define! (gcd a b)
            (if (= b 0) a
              (gcd b (mod a b))))
          (gcd 1071 462))
        "#,
        Some(Value::Number(21)),
    ));

    // ── 20. Repeated GCD (stress TCO) ────────────────────────────────────
    results.push(bench(
        &lisp,
        "gcd-repeat(10000)",
        r#"
        (begin
          (define! (mod a b) (- a (* b (/ a b))))
          (define! (gcd a b)
            (if (= b 0) a
              (gcd b (mod a b))))
          (define! (repeat n)
            (if (= n 0) (gcd 1071 462)
              (begin (gcd 1071 462) (repeat (- n 1)))))
          (repeat 10000))
        "#,
        Some(Value::Number(21)),
    ));

    // ── 21. Tak function (Takeuchi benchmark) ────────────────────────────
    results.push(bench(
        &lisp,
        "tak(18,12,6)",
        r#"
        (begin
          (define! (tak x y z)
            (if (>= y x) z
              (tak (tak (- x 1) y z)
                   (tak (- y 1) z x)
                   (tak (- z 1) x y))))
          (tak 18 12 6))
        "#,
        Some(Value::Number(7)),
    ));

    // ── 22. Flatten nested list ──────────────────────────────────────────
    results.push(bench(
        &lisp,
        "flatten-nested",
        r#"
        (begin
          (define! (append a b)
            (if (null? a) b
              (cons (car a) (append (cdr a) b))))
          (define! (flatten lst)
            (cond
              ((null? lst) (list))
              ((pair? (car lst))
               (append (flatten (car lst)) (flatten (cdr lst))))
              (#t (cons (car lst) (flatten (cdr lst))))))
          (define! nested (list (list 1 2) (list 3 (list 4 5)) (list 6)))
          (define! flat (flatten nested))
          (define! (sum-list lst)
            (if (null? lst) 0
              (+ (car lst) (sum-list (cdr lst)))))
          (sum-list flat))
        "#,
        Some(Value::Number(21)),
    ));

    // ── 23. Nth element access ───────────────────────────────────────────
    results.push(bench(
        &lisp,
        "list-nth(5000)",
        r#"
        (begin
          (define! (build n acc)
            (if (= n 0) acc
              (build (- n 1) (cons n acc))))
          (define! (nth lst n)
            (if (= n 0) (car lst)
              (nth (cdr lst) (- n 1))))
          (nth (build 5000 (list)) 4999))
        "#,
        Some(Value::Number(5000)),
    ));

    // ── 24. Boolean logic gauntlet ───────────────────────────────────────
    results.push(bench(
        &lisp,
        "boolean-logic",
        r#"
        (begin
          (and
            (not #f)
            (or #f #t)
            (and #t #t)
            (not (and #f #t))
            (or (and #t #f) (and #t #t))
            (= 1 1)
            (< 1 2)
            (> 3 2)
            (<= 5 5)
            (>= 10 9)))
        "#,
        Some(Value::Boolean(true)),
    ));

    // ── 25. Deep cond chains ─────────────────────────────────────────────
    results.push(bench(
        &lisp,
        "cond-classify(10000)",
        r#"
        (begin
          (define! (mod a b) (- a (* b (/ a b))))
          (define! (classify n)
            (cond
              ((= (mod n 15) 0) 1)
              ((= (mod n 5)  0) 2)
              ((= (mod n 3)  0) 3)
              (#t                4)))
          (define! (run n acc)
            (if (= n 0) acc
              (run (- n 1) (+ acc (classify n)))))
          (run 10000 0))
        "#,
        None, // just checking it completes without error
    ));

    // ── 26. Let binding stress ───────────────────────────────────────────
    results.push(bench(
        &lisp,
        "let-stress",
        r#"
        (begin
          (define! (compute x)
            (let ((a (+ x 1))
                  (b (* x 2))
                  (c (- x 3)))
              (let ((d (+ a b))
                    (e (* b c)))
                (+ d e))))
          (define! (run n acc)
            (if (= n 0) acc
              (run (- n 1) (+ acc (compute n)))))
          (run 10000 0))
        "#,
        None,
    ));

    // ── 27. Append two large lists ───────────────────────────────────────
    results.push(bench(
        &lisp,
        "append-lists(500+500)",
        r#"
        (begin
          (define! (build n acc)
            (if (= n 0) acc
              (build (- n 1) (cons n acc))))
          (define! (append a b)
            (if (null? a) b
              (cons (car a) (append (cdr a) b))))
          (define! (length lst)
            (define! (loop l acc)
              (if (null? l) acc
                (loop (cdr l) (+ acc 1))))
            (loop lst 0))
          (length (append (build 500 (list)) (build 500 (list)))))
        "#,
        Some(Value::Number(1000)),
    ));

    // ── 28. Reverse a list ───────────────────────────────────────────────
    results.push(bench(
        &lisp,
        "reverse(2000)",
        r#"
        (begin
          (define! (build n acc)
            (if (= n 0) acc
              (build (- n 1) (cons n acc))))
          (define! (reverse lst)
            (define! (loop l acc)
              (if (null? l) acc
                (loop (cdr l) (cons (car l) acc))))
            (loop lst (list)))
          (car (reverse (build 2000 (list)))))
        "#,
        Some(Value::Number(2000)),
    ));

    // ── 29. Collatz conjecture (sequence length) ─────────────────────────
    results.push(bench(
        &lisp,
        "collatz-len(837799)",
        r#"
        (begin
          (define! (mod a b) (- a (* b (/ a b))))
          (define! (even? n) (= (mod n 2) 0))
          (define! (collatz-len n steps)
            (if (= n 1) steps
              (if (even? n)
                (collatz-len (/ n 2) (+ steps 1))
                (collatz-len (+ (* 3 n) 1) (+ steps 1)))))
          (collatz-len 837799 0))
        "#,
        Some(Value::Number(524)),
    ));

    // ── 30. Sieve-like: count primes via trial division ──────────────────
    results.push(bench(
        &lisp,
        "count-primes(500)",
        r#"
        (begin
          (define! (mod a b) (- a (* b (/ a b))))
          (define! (divides? d n) (= (mod n d) 0))
          (define! (prime? n)
            (define! (check d)
              (cond
                ((> (* d d) n) #t)
                ((divides? d n) #f)
                (#t (check (+ d 1)))))
            (if (<= n 1) #f (check 2)))
          (define! (count-primes n acc)
            (if (= n 1) acc
              (count-primes (- n 1) (if (prime? n) (+ acc 1) acc))))
          (count-primes 500 0))
        "#,
        Some(Value::Number(95)),
    ));

    // ── 31. Variadic arithmetic: sum many args ───────────────────────────
    results.push(bench(
        &lisp,
        "variadic-add",
        r#"
        (+ 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20)
        "#,
        Some(Value::Number(210)),
    ));

    // ── 32. Deeply nested expressions ────────────────────────────────────
    results.push(bench(
        &lisp,
        "deep-nesting",
        r#"
        (+ 1 (+ 2 (+ 3 (+ 4 (+ 5 (+ 6 (+ 7 (+ 8 (+ 9 (+ 10
          (+ 11 (+ 12 (+ 13 (+ 14 (+ 15 (+ 16 (+ 17 (+ 18
          (+ 19 20)))))))))))))))))))
        "#,
        Some(Value::Number(210)),
    ));

    // ── 33. Y-combinator style recursion ─────────────────────────────────
    results.push(bench(
        &lisp,
        "y-combinator-fib",
        r#"
        (begin
          (define! (Y-fib f n)
            (if (<= n 1) n
              (+ (f f (- n 1)) (f f (- n 2)))))
          (Y-fib Y-fib 25))
        "#,
        Some(Value::Number(75025)),
    ));

    // ── 34. CPS (continuation-passing style) factorial ───────────────────
    results.push(bench(
        &lisp,
        "cps-factorial(15)",
        r#"
        (begin
          (define! (fact-cps n k)
            (if (= n 0) (k 1)
              (fact-cps (- n 1) (lambda (r) (k (* n r))))))
          (fact-cps 15 (lambda (x) x)))
        "#,
        Some(Value::Number(1307674368000)),
    ));

    // ── 35. Type predicate gauntlet ──────────────────────────────────────
    results.push(bench(
        &lisp,
        "type-predicates",
        r#"
        (begin
          (and
            (number? 42)
            (not (number? #t))
            (boolean? #t)
            (boolean? #f)
            (not (boolean? 0))
            (symbol? (quote hello))
            (not (symbol? 42))
            (pair? (cons 1 2))
            (not (pair? 42))
            (null? (list))
            (not (null? (list 1)))))
        "#,
        Some(Value::Boolean(true)),
    ));

    // ── Print results ────────────────────────────────────────────────────
    print_report(&results);
}
