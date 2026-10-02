//! Benchmark suite for grift.
//!
//! The programs are the ones from the previous (arena-based) grift's
//! benchmark, translated to the current language. The "old" column is that
//! implementation's average on the same programs, measured on the `pure`
//! branch, for comparison.
//!
//! ```sh
//! cargo run -p grift --example fib_bench --release
//! ```

use std::time::{Duration, Instant};

use grift::Grift;

const BENCH_RUNS: u32 = 5;

/// (name, program, expected written result, old average in microseconds)
const BENCHMARKS: &[(&str, &str, &str, f64)] = &[
    (
        "fib-naive(30)",
        "($let ()
           ($define! fib ($lambda (n) ($if (<=? n 1) n (+ (fib (- n 1)) (fib (- n 2))))))
           (fib 30))",
        "832040",
        2_542_000.0,
    ),
    (
        "fib-tco(50)",
        "($let ()
           ($define! fib-tco ($lambda (n a b) ($if (=? n 0) a (fib-tco (- n 1) b (+ a b)))))
           (fib-tco 50 0 1))",
        "12586269025",
        40.937,
    ),
    (
        "fib-iter(50)",
        "($let ()
           ($define! fib-iter
             ($lambda (n)
               ($define! loop ($lambda (i a b) ($if (=? i 0) a (loop (- i 1) b (+ a b)))))
               (loop n 0 1)))
           (fib-iter 50))",
        "12586269025",
        80.550,
    ),
    (
        "sum-tco(10000)",
        "($let ()
           ($define! sum ($lambda (n acc) ($if (=? n 0) acc (sum (- n 1) (+ acc n)))))
           (sum 10000 0))",
        "50005000",
        10_543.0,
    ),
    (
        "countdown(100000)",
        "($let ()
           ($define! countdown ($lambda (n) ($if (=? n 0) 0 (countdown (- n 1)))))
           (countdown 100000))",
        "0",
        98_615.0,
    ),
    (
        "factorial-tco(20)",
        "($let ()
           ($define! fact ($lambda (n acc) ($if (=? n 0) acc (fact (- n 1) (* acc n)))))
           (fact 20 1))",
        "2432902008176640000",
        146.191,
    ),
    (
        "ackermann(3,7)",
        "($let ()
           ($define! ack
             ($lambda (m n)
               ($cond ((=? m 0) (+ n 1))
                      ((=? n 0) (ack (- m 1) 1))
                      (#t (ack (- m 1) (ack m (- n 1)))))))
           (ack 3 7))",
        "1021",
        868_800.0,
    ),
    (
        "list-build(1000)",
        "($let ()
           ($define! build ($lambda (n acc) ($if (=? n 0) acc (build (- n 1) (cons n acc)))))
           ($define! lst (build 1000 (list)))
           (car lst))",
        "1",
        1_370.0,
    ),
    (
        "list-length(1000)",
        "($let ()
           ($define! build ($lambda (n acc) ($if (=? n 0) acc (build (- n 1) (cons n acc)))))
           ($define! length
             ($lambda (lst)
               ($define! loop ($lambda (l acc) ($if (null? l) acc (loop (cdr l) (+ acc 1)))))
               (loop lst 0)))
           (length (build 1000 (list))))",
        "1000",
        11_445.0,
    ),
    (
        "list-sum(1000)",
        "($let ()
           ($define! build ($lambda (n acc) ($if (=? n 0) acc (build (- n 1) (cons n acc)))))
           ($define! sum-list
             ($lambda (lst)
               ($define! loop ($lambda (l acc) ($if (null? l) acc (loop (cdr l) (+ acc (car l))))))
               (loop lst 0)))
           (sum-list (build 1000 (list))))",
        "500500",
        9_752.0,
    ),
    (
        "map-double(500)",
        "($let ()
           ($define! build ($lambda (n acc) ($if (=? n 0) acc (build (- n 1) (cons n acc)))))
           ($define! map ($lambda (f lst) ($if (null? lst) (list) (cons (f (car lst)) (map f (cdr lst))))))
           ($define! double ($lambda (x) (* x 2)))
           ($define! result (map double (build 500 (list))))
           (car result))",
        "2",
        2_700.0,
    ),
    (
        "foldl-sum(1000)",
        "($let ()
           ($define! build ($lambda (n acc) ($if (=? n 0) acc (build (- n 1) (cons n acc)))))
           ($define! foldl ($lambda (f init lst) ($if (null? lst) init (foldl f (f init (car lst)) (cdr lst)))))
           ($define! add ($lambda (a b) (+ a b)))
           (foldl add 0 (build 1000 (list))))",
        "500500",
        13_524.0,
    ),
    (
        "pure-counter(10000)",
        "($let ()
           ($define! count ($lambda (n acc) ($if (=? n 0) acc (count (- n 1) (+ acc 1)))))
           (count 10000 0))",
        "10000",
        9_018.0,
    ),
    (
        "mutual-recur(10000)",
        "($let ()
           ($define! (my-even? my-odd?)
             (list ($lambda (n) ($if (=? n 0) #t (my-odd? (- n 1))))
                   ($lambda (n) ($if (=? n 0) #f (my-even? (- n 1))))))
           (my-even? 10000))",
        "#t",
        10_978.0,
    ),
    (
        "nested-closures",
        "($let ()
           ($define! adder ($lambda (x) ($lambda (y) (+ x y))))
           ($define! add5 (adder 5))
           ($define! add10 (adder 10))
           ($define! compose ($lambda (f g) ($lambda (x) (f (g x)))))
           ($define! add15 (compose add5 add10))
           (add15 100))",
        "115",
        30.204,
    ),
    (
        "church-numerals",
        "($let ()
           ($define! church-zero ($lambda (f) ($lambda (x) x)))
           ($define! church-succ ($lambda (n) ($lambda (f) ($lambda (x) (f ((n f) x))))))
           ($define! church-add ($lambda (m n) ($lambda (f) ($lambda (x) ((m f) ((n f) x))))))
           ($define! church->int ($lambda (n) ((n ($lambda (x) (+ x 1))) 0)))
           ($define! c1 (church-succ church-zero))
           ($define! c2 (church-succ c1))
           ($define! c3 (church-succ c2))
           ($define! c5 (church-add c2 c3))
           (church->int c5))",
        "5",
        98.612,
    ),
    (
        "fast-power(2^30)",
        "($let ()
           ($define! fast-pow
             ($lambda (base exp)
               ($cond ((=? exp 0) 1)
                      ((even? exp) ($let ((half (fast-pow base (div exp 2)))) (* half half)))
                      (#t (* base (fast-pow base (- exp 1)))))))
           (fast-pow 2 30))",
        "1073741824",
        38.191,
    ),
    (
        "gcd-euclid",
        "($let ()
           ($define! gcd ($lambda (a b) ($if (=? b 0) a (gcd b (mod a b)))))
           (gcd 1071 462))",
        "21",
        20.387,
    ),
    (
        "gcd-repeat(10000)",
        "($let ()
           ($define! gcd ($lambda (a b) ($if (=? b 0) a (gcd b (mod a b)))))
           ($define! repeat
             ($lambda (n)
               ($if (=? n 0) (gcd 1071 462) ($sequence (gcd 1071 462) (repeat (- n 1))))))
           (repeat 10000))",
        "21",
        67_745.0,
    ),
    (
        "tak(18,12,6)",
        "($let ()
           ($define! tak
             ($lambda (x y z)
               ($if (>=? y x) z (tak (tak (- x 1) y z) (tak (- y 1) z x) (tak (- z 1) x y)))))
           (tak 18 12 6))",
        "7",
        55_738.0,
    ),
    (
        "flatten-nested",
        "($let ()
           ($define! append ($lambda (a b) ($if (null? a) b (cons (car a) (append (cdr a) b)))))
           ($define! flatten
             ($lambda (lst)
               ($cond ((null? lst) (list))
                      ((pair? (car lst)) (append (flatten (car lst)) (flatten (cdr lst))))
                      (#t (cons (car lst) (flatten (cdr lst)))))))
           ($define! nested (list (list 1 2) (list 3 (list 4 5)) (list 6)))
           ($define! flat (flatten nested))
           ($define! sum-list ($lambda (lst) ($if (null? lst) 0 (+ (car lst) (sum-list (cdr lst))))))
           (sum-list flat))",
        "21",
        288.554,
    ),
    (
        "list-nth(5000)",
        "($let ()
           ($define! build ($lambda (n acc) ($if (=? n 0) acc (build (- n 1) (cons n acc)))))
           ($define! nth ($lambda (lst n) ($if (=? n 0) (car lst) (nth (cdr lst) (- n 1)))))
           (nth (build 5000 (list)) 4999))",
        "5000",
        12_864.0,
    ),
    (
        "boolean-logic",
        "($and? (not? #f)
                ($or? #f #t)
                ($and? #t #t)
                (not? ($and? #f #t))
                ($or? ($and? #t #f) ($and? #t #t))
                (=? 1 1)
                (<? 1 2)
                (>? 3 2)
                (<=? 5 5)
                (>=? 10 9))",
        "#t",
        13.612,
    ),
    (
        "cond-classify(10000)",
        "($let ()
           ($define! classify
             ($lambda (n)
               ($cond ((=? (mod n 15) 0) 1)
                      ((=? (mod n 5) 0) 2)
                      ((=? (mod n 3) 0) 3)
                      (#t 4))))
           ($define! run ($lambda (n acc) ($if (=? n 0) acc (run (- n 1) (+ acc (classify n))))))
           (run 10000 0))",
        "32667",
        54_224.0,
    ),
    (
        "let-stress",
        "($let ()
           ($define! compute
             ($lambda (x)
               ($let ((a (+ x 1)) (b (* x 2)) (c (- x 3)))
                 ($let ((d (+ a b)) (e (* b c)))
                   (+ d e)))))
           ($define! run ($lambda (n acc) ($if (=? n 0) acc (run (- n 1) (+ acc (compute n))))))
           (run 10000 0))",
        "666616665000",
        37_527.0,
    ),
    (
        "append-lists(500+500)",
        "($let ()
           ($define! build ($lambda (n acc) ($if (=? n 0) acc (build (- n 1) (cons n acc)))))
           ($define! append ($lambda (a b) ($if (null? a) b (cons (car a) (append (cdr a) b)))))
           ($define! length
             ($lambda (lst)
               ($define! loop ($lambda (l acc) ($if (null? l) acc (loop (cdr l) (+ acc 1)))))
               (loop lst 0)))
           (length (append (build 500 (list)) (build 500 (list)))))",
        "1000",
        17_082.0,
    ),
    (
        "reverse(2000)",
        "($let ()
           ($define! build ($lambda (n acc) ($if (=? n 0) acc (build (- n 1) (cons n acc)))))
           ($define! reverse
             ($lambda (lst)
               ($define! loop ($lambda (l acc) ($if (null? l) acc (loop (cdr l) (cons (car l) acc)))))
               (loop lst (list))))
           (car (reverse (build 2000 (list)))))",
        "2000",
        21_841.0,
    ),
    (
        "collatz-len(837799)",
        "($let ()
           ($define! collatz-len
             ($lambda (n steps)
               ($if (=? n 1)
                    steps
                    ($if (even? n)
                         (collatz-len (div n 2) (+ steps 1))
                         (collatz-len (+ (* 3 n) 1) (+ steps 1))))))
           (collatz-len 837799 0))",
        "524",
        933.312,
    ),
    (
        "count-primes(500)",
        "($let ()
           ($define! divides? ($lambda (d n) (=? (mod n d) 0)))
           ($define! prime?
             ($lambda (n)
               ($define! check
                 ($lambda (d)
                   ($cond ((>? (* d d) n) #t)
                          ((divides? d n) #f)
                          (#t (check (+ d 1))))))
               ($if (<=? n 1) #f (check 2))))
           ($define! count-primes
             ($lambda (n acc)
               ($if (=? n 1) acc (count-primes (- n 1) ($if (prime? n) (+ acc 1) acc)))))
           (count-primes 500 0))",
        "95",
        11_802.0,
    ),
    (
        "variadic-add",
        "(+ 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20)",
        "210",
        53.479,
    ),
    (
        "deep-nesting",
        "(+ 1 (+ 2 (+ 3 (+ 4 (+ 5 (+ 6 (+ 7 (+ 8 (+ 9 (+ 10
           (+ 11 (+ 12 (+ 13 (+ 14 (+ 15 (+ 16 (+ 17 (+ 18
           (+ 19 20)))))))))))))))))))",
        "210",
        19.737,
    ),
    (
        "y-combinator-fib",
        "($let ()
           ($define! Y-fib ($lambda (f n) ($if (<=? n 1) n (+ (f f (- n 1)) (f f (- n 2))))))
           (Y-fib Y-fib 25))",
        "75025",
        312_972.0,
    ),
    (
        "cps-factorial(15)",
        "($let ()
           ($define! fact-cps
             ($lambda (n k) ($if (=? n 0) (k 1) (fact-cps (- n 1) ($lambda (r) (k (* n r)))))))
           (fact-cps 15 ($lambda (x) x)))",
        "1307674368000",
        363.796,
    ),
    (
        "type-predicates",
        "($and? (integer? 42)
                (not? (integer? #t))
                (boolean? #t)
                (boolean? #f)
                (not? (boolean? 0))
                (symbol? (string->symbol \"hello\"))
                (not? (symbol? 42))
                (pair? (cons 1 2))
                (not? (pair? 42))
                (null? (list))
                (not? (null? (list 1))))",
        "#t",
        258.170,
    ),
];

fn fmt(d: Duration) -> String {
    let us = d.as_secs_f64() * 1e6;
    fmt_us(us)
}

fn fmt_us(us: f64) -> String {
    if us >= 1e6 {
        format!("{:.3}s", us / 1e6)
    } else if us >= 1e3 {
        format!("{:.2}ms", us / 1e3)
    } else {
        format!("{us:.1}µs")
    }
}

fn main() {
    let base = Grift::new();
    println!(
        "{:<24} {:>14} {:>12} {:>12} {:>12} {:>8}  status",
        "benchmark", "result", "avg", "min", "old avg", "ratio"
    );
    let mut failures = 0;
    let mut total = Duration::ZERO;
    let mut old_total = 0.0;
    for &(name, program, expected, old_us) in BENCHMARKS {
        let mut sum = Duration::ZERO;
        let mut min = Duration::MAX;
        let mut output = String::new();
        let mut ok = true;
        for _ in 0..BENCH_RUNS {
            let mut g = base.clone();
            let start = Instant::now();
            let result = g.eval(program);
            let elapsed = start.elapsed();
            sum += elapsed;
            min = min.min(elapsed);
            output = match result {
                Ok(v) => g.write(v),
                Err(e) => format!("error: {e}"),
            };
            ok &= output == expected;
        }
        let avg = sum / BENCH_RUNS;
        total += avg;
        old_total += old_us;
        if !ok {
            failures += 1;
        }
        let ratio = avg.as_secs_f64() * 1e6 / old_us;
        let shown: String = output.chars().take(14).collect();
        println!(
            "{name:<24} {shown:>14} {:>12} {:>12} {:>12} {ratio:>7.2}x  {}",
            fmt(avg),
            fmt(min),
            fmt_us(old_us),
            if ok { "ok" } else { "FAIL" }
        );
    }
    println!(
        "{:<24} {:>14} {:>12} {:>12} {:>12} {:>7.2}x  {} failed",
        "total",
        "",
        fmt(total),
        "",
        fmt_us(old_total),
        total.as_secs_f64() * 1e6 / old_total,
        failures
    );
    if failures > 0 {
        std::process::exit(1);
    }
}
