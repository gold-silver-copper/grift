//! # Builtin Function Definition Example
//!
//! This example demonstrates how to define custom functions in Lisp using
//! the standard library functions. The `register_native!` macro is used
//! for defining builtin functions at compile time in the lisp_eval crate.
//!
//! Note: Custom Rust functions are now integrated as builtins at compile time
//! rather than registered at runtime. This example shows how to achieve
//! similar functionality using pure Lisp definitions.
//!
//! Run with: `cargo run --example native_interop`

use lisp_eval::{Lisp, Evaluator};

fn main() {
    println!("=== Custom Function Example ===\n");
    
    // Create a Lisp context with a 10,000 cell arena
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define custom functions in Lisp (equivalent to what native functions did)
    // These use the pure Lisp approach instead of Rust native functions
    let _ = eval.eval_str("(define (double x) (* x 2))");
    let _ = eval.eval_str("(define (my-max a b) (if (> a b) a b))");
    let _ = eval.eval_str("(define (even? x) (= (mod x 2) 0))");
    let _ = eval.eval_str("(define (square x) (* x x))");
    let _ = eval.eval_str("(define (clamp value min-val max-val) (cond ((< value min-val) min-val) ((> value max-val) max-val) (else value)))");
    
    println!("Defined functions: double, my-max, even?, square, clamp\n");
    
    // Test the functions
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
    
    println!("\n--- Using custom functions in Lisp expressions ---\n");
    
    // More complex expressions using custom functions
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
    
    println!("\n--- Note on Builtin Functions ---\n");
    println!("To add Rust-implemented builtin functions, use the register_native!");
    println!("macro in the lisp_eval crate and add the function to the Builtin enum.");
    println!("Example:");
    println!("  register_native!(my_builtin, (x: isize, y: isize) -> isize, {{ x + y }});");
    
    println!("\n=== Example Complete ===");
}
