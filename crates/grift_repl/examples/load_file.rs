//! # Loading External Scheme Files
//!
//! This example demonstrates how to load and evaluate external `.scm` files
//! in the Grift Scheme interpreter.
//!
//! ## Running
//!
//! ```bash
//! cargo run -p grift_repl --example load_file
//! ```
//!
//! ## Loading files in the interactive REPL
//!
//! You can also load `.scm` files interactively using the `:load` command:
//!
//! ```text
//! > :load examples/hello.scm
//! Hello, World!
//! 5! = 120
//! Loaded: examples/hello.scm
//! ```

use grift_eval::{Lisp, Evaluator, Value};
use grift_repl::{format_value, value_to_string, format_error};

fn main() {
    println!("=== Loading External .scm Files ===\n");

    // Create a Lisp interpreter with a 20,000-cell arena
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).expect("Failed to create evaluator");

    // --- Method 1: Load from a string embedded at compile time ---
    println!("--- Method 1: Embedded Scheme source ---\n");

    let scheme_source = r#"
        (begin
          (define (square x) (* x x))
          (define (cube x) (* x x x)))
    "#;

    match eval.eval_str(scheme_source) {
        Ok(_) => println!("Loaded embedded definitions."),
        Err(e) => println!("Error: {}", format_error(&lisp, &e)),
    }

    let result = eval.eval_str("(square 7)").unwrap();
    println!("(square 7) => {}", value_to_string(&lisp, result));

    let result = eval.eval_str("(cube 3)").unwrap();
    println!("(cube 3) => {}", value_to_string(&lisp, result));

    // --- Method 2: Load from a file at runtime ---
    println!("\n--- Method 2: Load from .scm file ---\n");

    let file_path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/hello.scm");
    match std::fs::read_to_string(file_path) {
        Ok(contents) => {
            // Set up output callback so display/newline work during eval
            eval.set_output_callback(Some(output_callback));

            // Wrap in (begin ...) to evaluate all top-level forms
            let wrapped = format!("(begin {})", contents);
            match eval.eval_str(&wrapped) {
                Ok(result) => {
                    if !matches!(lisp.get(result), Ok(Value::Void)) {
                        println!("Result: {}", value_to_string(&lisp, result));
                    }
                    println!("Successfully loaded: {}", file_path);
                }
                Err(e) => println!("Error loading file: {}", format_error(&lisp, &e)),
            }
        }
        Err(e) => println!("Cannot read file '{}': {}", file_path, e),
    }

    // Use the definitions from the loaded file
    println!("\n--- Using loaded definitions ---\n");

    let result = eval.eval_str("(fact 10)").unwrap();
    println!("(fact 10) => {}", value_to_string(&lisp, result));

    println!("\n=== Example Complete ===");
}

/// Output callback for display/newline during evaluation.
fn output_callback<const N: usize>(lisp: &Lisp<N>, val: grift_eval::ArenaIndex) {
    use std::io::Write;
    let mut stdout = std::io::stdout();

    if val.is_nil() {
        let _ = stdout.write_all(b"\n");
    } else {
        let mut buf = String::new();
        format_value(lisp, val, &mut buf);
        let _ = stdout.write_all(buf.as_bytes());
    }
    let _ = stdout.flush();
}
