//! # Native Function Interop Example
//!
//! This example demonstrates how to register Rust functions that can be
//! called from Lisp code using the native function interop system.
//!
//! Run with: `cargo run --example native_interop`

use lisp_eval::{
    Lisp, Evaluator, register_native,
};

fn main() {
    println!("=== Native Function Interop Example ===\n");
    
    // Create a Lisp context with a 10,000 cell arena
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Register native functions using the new macro syntax
    register_native!(eval, "double", (x: i64) -> i64 {
        x * 2
    }).unwrap();
    
    register_native!(eval, "my-max", (a: i64, b: i64) -> i64 {
        if a > b { a } else { b }
    }).unwrap();
    
    register_native!(eval, "even?", (x: i64) -> bool {
        x % 2 == 0
    }).unwrap();
    
    register_native!(eval, "square", (x: i64) -> i64 {
        x * x
    }).unwrap();
    
    register_native!(eval, "clamp", (value: i64, min_val: i64, max_val: i64) -> i64 {
        if value < min_val {
            min_val
        } else if value > max_val {
            max_val
        } else {
            value
        }
    }).unwrap();
    
    println!("Registered native functions: double, my-max, even?, square, clamp\n");
    
    // Test the native functions
    let tests = [
        ("(double 21)", "Doubling 21"),
        ("(my-max 5 10)", "Max of 5 and 10"),
        ("(my-max 100 50)", "Max of 100 and 50"),
        ("(even? 4)", "Is 4 even?"),
        ("(even? 7)", "Is 7 even?"),
        ("(square 8)", "Square of 8"),
        ("(clamp 50 0 100)", "Clamp 50 to [0, 100]"),
        ("(clamp -10 0 100)", "Clamp -10 to [0, 100]"),
        ("(clamp 150 0 100)", "Clamp 150 to [0, 100]"),
        ("(+ 10 9)", "Simple addition"),
        ("(double 5)", "Double 5"),
        ("(square 3)", "Square 3"),
    ];
    
    for (expr, desc) in tests {
        match eval.eval_str(expr) {
            Ok(result) => {
                let value = lisp.get(result).unwrap();
                println!("{}: {} => {:?}", desc, expr, value);
            }
            Err(e) => {
                println!("{}: {} => ERROR: {:?}", desc, expr, e.kind);
            }
        }
    }
    
    println!("\n--- Using native functions in Lisp expressions ---\n");
    
    // More complex expressions using native functions
    let complex_tests = [
        "(+ (double 5) (square 3))", // 10 + 9 = 19
        "(if (even? 10) 'yes 'no)",  // yes
        "(my-max (square 5) (double 12))", // max(25, 24) = 25
        "(define (double-then-square x) (square (double x)))",
        "(double-then-square 3)", // square(6) = 36
    ];
    
    for expr in complex_tests {
        match eval.eval_str(expr) {
            Ok(result) => {
                let value = lisp.get(result).unwrap();
                println!("{} => {:?}", expr, value);
            }
            Err(e) => {
                println!("{} => ERROR: {:?}", expr, e.kind);
            }
        }
    }
    
    println!("\n=== Example Complete ===");
}
