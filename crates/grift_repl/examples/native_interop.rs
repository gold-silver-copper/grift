//! # Native Function Interop Example
//!
//! This example demonstrates how to register Rust functions that can be
//! called from Lisp code using the native function interop system.
//!
//! Run with: `cargo run --example native_interop`

use grift_eval::{
    Lisp, Evaluator, ArenaIndex, ArenaResult, ToLisp, extract_arg,
};

/// A simple native function that doubles a number.
///
/// This demonstrates the basic pattern for native functions:
/// 1. Extract arguments from the args list
/// 2. Perform computation
/// 3. Return result via ToLisp
fn native_double<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (x, _rest): (isize, _) = extract_arg(lisp, args)?;
    (x * 2).to_lisp(lisp)
}

/// A native function that computes the maximum of two numbers.
fn native_max<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (a, rest): (isize, _) = extract_arg(lisp, args)?;
    let (b, _rest): (isize, _) = extract_arg(lisp, rest)?;
    (if a > b { a } else { b }).to_lisp(lisp)
}

/// A native function that returns whether a number is even.
fn native_evenp<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (x, _rest): (isize, _) = extract_arg(lisp, args)?;
    (x % 2 == 0).to_lisp(lisp)
}

/// A native function that squares a number.
fn native_square<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (x, _rest): (isize, _) = extract_arg(lisp, args)?;
    (x * x).to_lisp(lisp)
}

/// A native function that clamps a value to a range.
fn native_clamp<const N: usize>(lisp: &Lisp<N>, args: ArenaIndex) -> ArenaResult<ArenaIndex> {
    let (value, rest): (isize, _) = extract_arg(lisp, args)?;
    let (min_val, rest): (isize, _) = extract_arg(lisp, rest)?;
    let (max_val, _rest): (isize, _) = extract_arg(lisp, rest)?;
    
    let clamped = if value < min_val {
        min_val
    } else if value > max_val {
        max_val
    } else {
        value
    };
    
    clamped.to_lisp(lisp)
}

fn main() {
    println!("=== Native Function Interop Example ===\n");
    
    // Create a Lisp context with a 10,000 cell arena
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Register native functions
    eval.register_native("double", native_double).unwrap();
    eval.register_native("my-max", native_max).unwrap();
    eval.register_native("even?", native_evenp).unwrap();
    eval.register_native("square", native_square).unwrap();
    eval.register_native("clamp", native_clamp).unwrap();
    
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
