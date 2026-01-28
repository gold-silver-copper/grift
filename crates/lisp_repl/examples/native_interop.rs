//! # Lisp-Rust Interop Example
//!
//! This example demonstrates how the Lisp evaluator works with built-in 
//! functions, including the embedded hardware functions like peek/poke.
//!
//! Note: Native functions are now compiled into the evaluator at build time.
//! All functions (including peek, poke, GPIO operations, and bit manipulation)
//! are available as built-in functions without runtime registration.
//!
//! Run with: `cargo run --example native_interop`

use lisp_eval::{Lisp, Evaluator};

fn main() {
    println!("=== Lisp-Rust Interop Example ===\n");
    
    // Create a Lisp context with a 10,000 cell arena
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    println!("All functions are available as built-ins - no registration needed!\n");
    
    // Test basic arithmetic (core builtins)
    let basic_tests = [
        ("(+ 10 20 30)", "Addition"),
        ("(* 5 6)", "Multiplication"),
        ("(- 100 42)", "Subtraction"),
        ("(/ 42 6)", "Division"),
        ("(mod 17 5)", "Modulo"),
    ];
    
    println!("--- Basic Arithmetic ---\n");
    for (expr, desc) in basic_tests {
        run_test(&mut eval, &lisp, expr, desc);
    }
    
    // Test predicates
    let predicate_tests = [
        ("(null? '())", "Is empty list null?"),
        ("(pair? '(1 2))", "Is (1 2) a pair?"),
        ("(number? 42)", "Is 42 a number?"),
        ("(symbol? 'hello)", "Is 'hello a symbol?"),
    ];
    
    println!("\n--- Predicates ---\n");
    for (expr, desc) in predicate_tests {
        run_test(&mut eval, &lisp, expr, desc);
    }
    
    // Test embedded hardware functions (mock implementation)
    let embedded_tests = [
        ("(poke 0 42)", "Write 42 to address 0"),
        ("(peek 0)", "Read from address 0"),
        ("(poke32 4 0x12345678)", "Write 32-bit word"),
        ("(peek32 4)", "Read 32-bit word"),
        ("(gpio-write 0 255)", "Write 255 to GPIO register 0"),
        ("(gpio-read 0)", "Read GPIO register 0"),
        ("(gpio-set 1 0)", "Set bit 0 in GPIO register 1"),
        ("(gpio-read 1)", "Read GPIO register 1"),
    ];
    
    println!("\n--- Embedded Hardware Functions (Mock) ---\n");
    for (expr, desc) in embedded_tests {
        run_test(&mut eval, &lisp, expr, desc);
    }
    
    // Test bit manipulation functions
    let bit_tests = [
        ("(bit-set? 5 0)", "Is bit 0 set in 5? (5 = 0b101)"),
        ("(bit-set? 5 1)", "Is bit 1 set in 5?"),
        ("(bit-set? 5 2)", "Is bit 2 set in 5?"),
        ("(bit-extract 0xFF 4 4)", "Extract high nibble from 0xFF"),
        ("(bit-insert 0 0xF 4 4)", "Insert 0xF at bit position 4"),
    ];
    
    println!("\n--- Bit Manipulation ---\n");
    for (expr, desc) in bit_tests {
        run_test(&mut eval, &lisp, expr, desc);
    }
    
    // Test using builtins in more complex expressions
    let complex_tests = [
        "(define (double x) (* x 2))",
        "(double 21)",
        "(define (square x) (* x x))",
        "(square 8)",
        "(define (clamp val lo hi) (if (< val lo) lo (if (> val hi) hi val)))",
        "(clamp 50 0 100)",
        "(clamp -10 0 100)",
        "(clamp 150 0 100)",
    ];
    
    println!("\n--- User-Defined Functions ---\n");
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

fn run_test<const N: usize>(eval: &mut Evaluator<N>, lisp: &Lisp<N>, expr: &str, desc: &str) {
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
