//! Benchmark: compute fib(30) and print elapsed time.
//!
//! Requires the `std` feature for timing via `std::time::Instant`.
//!
//! ```sh
//! cargo run -p grift --features std --example fib_bench --release
//! ```

use grift::{Lisp, Value};

fn main() {
    // Spawn a thread with a larger stack so the arena (a const-generic
    // array) can be constructed without overflowing the default 8 MiB
    // main-thread stack.
    let builder = std::thread::Builder::new()
        .name("fib-bench".into())
        .stack_size(64 * 1024 * 1024); // 64 MiB

    let handler = builder
        .spawn(run_benchmark)
        .expect("failed to spawn thread");

    handler.join().expect("benchmark thread panicked");
}

fn run_benchmark() {
    let lisp: Lisp<500_000> = Lisp::new();

    // Iterative Fibonacci via self-application (no `define` needed).
    // Computes fib(30) = 832 040 in O(n) steps.
    let program = r#"
        ((lambda (n)
            ((lambda (loop)
                (loop loop 0 1 n))
             (lambda (self a b count)
                (if (= count 0)
                    a
                    (self self b (+ a b) (- count 1))))))
         30)
    "#;

    let start = std::time::Instant::now();
    let result = lisp.eval(program);
    let elapsed = start.elapsed();

    match result {
        Ok(Value::Number(n)) => {
            println!("fib(30) = {n}");
            println!("elapsed: {elapsed:.3?}");
        }
        Ok(other) => {
            println!("unexpected result: {other:?}");
        }
        Err(e) => {
            eprintln!("error: {e:?}");
            std::process::exit(1);
        }
    }
}
