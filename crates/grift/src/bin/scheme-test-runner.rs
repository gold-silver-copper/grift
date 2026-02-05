//! Scheme Test Runner
//! 
//! This program loads and executes Scheme test files from the peroxide test suite.
//! It handles the should-be and test macros to run the full test files.
//! 
//! Usage:
//!   cargo run --bin scheme-test-runner -- tests/scheme/r5rs_pitfall.scm
//!   cargo run --bin scheme-test-runner -- tests/scheme/r5rs-tests.scm

use grift_eval::*;
use grift_parser::Lisp;
use std::env;
use std::fs;
use std::thread;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <test-file.scm>", args[0]);
        eprintln!("\nExamples:");
        eprintln!("  {} tests/scheme/r5rs_pitfall.scm", args[0]);
        eprintln!("  {} tests/scheme/r5rs-tests.scm", args[0]);
        std::process::exit(1);
    }
    
    let test_file = args[1].clone();
    
    // Run with increased stack size to handle complex macro expansions
    // during initialization
    let builder = thread::Builder::new()
        .name("scheme-test-runner".into())
        .stack_size(32 * 1024 * 1024); // 32 MB stack
    
    let handle = builder.spawn(move || run_tests(&test_file)).unwrap();
    
    match handle.join() {
        Ok(_) => {},
        Err(e) => {
            eprintln!("Thread panicked: {:?}", e);
            std::process::exit(1);
        }
    }
}

fn run_tests(test_file: &str) {
    
    println!("═══════════════════════════════════════════════════════════");
    println!("  Grift Scheme Test Runner");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("Test file: {}", test_file);
    println!();
    
    // Read the test file
    let content = match fs::read_to_string(test_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file {}: {}", test_file, e);
            std::process::exit(1);
        }
    };
    
    // Create evaluator with large arena
    // Use Box to allocate on heap to avoid stack overflow
    let lisp: Box<Lisp<100000>> = Box::new(Lisp::new());
    let mut eval = match Evaluator::new(&*lisp) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to create evaluator: {:?}", e);
            std::process::exit(1);
        }
    };
    
    // Set up test infrastructure based on file type
    if test_file.contains("pitfall") {
        run_pitfall_tests(&lisp, &mut eval, &content);
    } else {
        run_r5rs_tests(&lisp, &mut eval, &content);
    }
}

fn run_pitfall_tests<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, content: &str) {
    println!("Running R5RS Pitfalls Tests...");
    println!("───────────────────────────────────────────────────────────");
    println!();
    
    // Set up the should-be macro and test tracking
    let setup = r#"
(define test-results '())
(define tests-passed 0)
(define tests-failed 0)

(define (record-test id expected actual)
  (let ((passed (equal? expected actual)))
    (if passed
        (set! tests-passed (+ tests-passed 1))
        (set! tests-failed (+ tests-failed 1)))
    (set! test-results (cons (list id expected actual passed) test-results))))

(define-syntax should-be
  (syntax-rules ()
    ((_ test-id expected-value expression)
     (let ((actual-value expression))
       (record-test 'test-id expected-value actual-value)))))

(define call/cc call-with-current-continuation)
"#;
    
    // Execute the setup
    if let Err(e) = eval.eval_str(setup) {
        eprintln!("Failed to set up test infrastructure: {:?}", e);
        return;
    }
    
    // Split content into smaller chunks to avoid stack overflow
    // Process line by line for better error handling
    let lines: Vec<&str> = content.lines().collect();
    let mut buffer = String::new();
    let mut in_test = false;
    let mut paren_count = 0;
    
    for line in lines {
        let trimmed = line.trim();
        
        // Skip comments and empty lines when not in a test
        if !in_test && (trimmed.is_empty() || trimmed.starts_with(';')) {
            continue;
        }
        
        buffer.push_str(line);
        buffer.push('\n');
        
        // Count parentheses
        for ch in line.chars() {
            match ch {
                '(' => paren_count += 1,
                ')' => paren_count -= 1,
                _ => {}
            }
        }
        
        // If parentheses are balanced and we have content, try to execute
        if paren_count == 0 && !buffer.trim().is_empty() {
            match eval.eval_str(&buffer) {
                Ok(_) => {
                    // Success - continue
                }
                Err(e) => {
                    eprintln!("Warning: Error in test block: {:?}", e);
                    eprintln!("Skipping and continuing...");
                }
            }
            buffer.clear();
            in_test = false;
        } else if paren_count > 0 {
            in_test = true;
        }
    }
    
    println!();
    
    // Get test results
    print_test_results(lisp, eval);
}

fn run_r5rs_tests<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, content: &str) {
    println!("Running R5RS Compliance Tests...");
    println!("───────────────────────────────────────────────────────────");
    println!();
    
    // Set up the test macro and test tracking
    let setup = r#"
(define *tests-run* 0)
(define *tests-passed* 0)
(define *test-results* '())

(define (record-test-result expr expected actual)
  (let ((passed (equal? expected actual)))
    (if passed
        (set! *tests-passed* (+ *tests-passed* 1))
        (begin))
    (set! *test-results* (cons (list expr expected actual passed) *test-results*))))

(define-syntax test
  (syntax-rules ()
    ((test name expect expr)
     (test expect expr))
    ((test expect expr)
     (begin
       (set! *tests-run* (+ *tests-run* 1))
       (let ((res expr))
         (record-test-result '*tests-run* expect res))))))

(define-syntax test-assert
  (syntax-rules ()
    ((test-assert expr) (test #t expr))))

(define (test-begin . name)
  (begin
    (display "Starting test section: ")
    (if (pair? name) (display (car name)) (begin))
    (newline)))

(define (test-end)
  (begin
    (display "Tests run: ")
    (display *tests-run*)
    (newline)
    (display "Tests passed: ")
    (display *tests-passed*)
   (newline)))
"#;
    
    // Execute the setup
    if let Err(e) = eval.eval_str(setup) {
        eprintln!("Failed to set up test infrastructure: {:?}", e);
        return;
    }
    
    // Split content into smaller chunks to avoid stack overflow
    // Try to execute line by line for better error handling
    let lines: Vec<&str> = content.lines().collect();
    let mut buffer = String::new();
    let mut in_test = false;
    let mut paren_count = 0;
    let mut skip_current_form = false;
    
    for line in lines {
        let trimmed = line.trim();
        
        // Skip comments and empty lines when not in a test
        if !in_test && (trimmed.is_empty() || trimmed.starts_with(';')) {
            continue;
        }
        
        // Check if this is a form we should skip (macro definitions that conflict with our setup)
        if paren_count == 0 && !in_test {
            if trimmed.starts_with("(define-syntax test") || 
               trimmed.starts_with("(define-syntax test-assert") ||
               trimmed.starts_with("(define (test-begin") ||
               trimmed.starts_with("(define (test-end") ||
               trimmed.starts_with("(define *tests-run*") ||
               trimmed.starts_with("(define *tests-passed*") ||
               trimmed.starts_with("(define *test-results*") ||
               trimmed.starts_with("(define (record-test") {
                skip_current_form = true;
            }
        }
        
        buffer.push_str(line);
        buffer.push('\n');
        
        // Count parentheses
        for ch in line.chars() {
            match ch {
                '(' => paren_count += 1,
                ')' => paren_count -= 1,
                _ => {}
            }
        }
        
        // If parentheses are balanced and we have content, try to execute
        if paren_count == 0 && !buffer.trim().is_empty() {
            if skip_current_form {
                // Skip this form - it conflicts with our test infrastructure
                skip_current_form = false;
            } else {
                match eval.eval_str(&buffer) {
                    Ok(_) => {
                        // Success - continue
                    }
                    Err(e) => {
                        eprintln!("Warning: Error in test block: {:?}", e);
                        eprintln!("Skipping and continuing...");
                    }
                }
            }
            buffer.clear();
            in_test = false;
        } else if paren_count > 0 {
            in_test = true;
        }
    }
    
    println!();
    
    // Get test results
    print_test_results(lisp, eval);
}

fn print_test_results<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>) {
    // Try to get the test results
    let results_ref = match eval.eval_str("(if (defined? 'test-results) test-results *test-results*)") {
        Ok(r) => r,
        Err(_) => {
            // Try alternative names
            match eval.eval_str("*test-results*") {
                Ok(r) => r,
                Err(_) => match eval.eval_str("test-results") {
                    Ok(r) => r,
                    Err(_) => {
                        eprintln!("Could not retrieve test results");
                        return;
                    }
                }
            }
        }
    };
    
    // Count results
    let mut total = 0;
    let mut passed = 0;
    let mut failed_tests = Vec::new();
    
    let mut current = results_ref;
    while let Ok(val) = lisp.get(current) {
        if !val.is_cons() {
            break;
        }
        
        let test_entry = match lisp.car(current) {
            Ok(e) => e,
            Err(_) => break,
        };
        
        // Extract test info: (id expected actual passed?)
        if let Ok(test_val) = lisp.get(test_entry) {
            if test_val.is_cons() {
                total += 1;
                
                // Get the passed flag (4th element)
                if let Ok(rest1) = lisp.cdr(test_entry) {
                    if let Ok(rest2) = lisp.cdr(rest1) {
                        if let Ok(rest3) = lisp.cdr(rest2) {
                            if let Ok(passed_ref) = lisp.car(rest3) {
                                if let Ok(passed_val) = lisp.get(passed_ref) {
                                    if passed_val.is_true() {
                                        passed += 1;
                                    } else {
                                        // Record failed test
                                        if let Ok(id_ref) = lisp.car(test_entry) {
                                            if let Ok(id_val) = lisp.get(id_ref) {
                                                failed_tests.push(format!("{:?}", id_val));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        current = match lisp.cdr(current) {
            Ok(c) => c,
            Err(_) => break,
        };
    }
    
    let failed = total - passed;
    
    println!("═══════════════════════════════════════════════════════════");
    println!("  Test Results");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("Total tests:  {}", total);
    println!("Passed:       {}", passed);
    println!("Failed:       {}", failed);
    
    if total > 0 {
        let pass_rate = (passed as f64 / total as f64) * 100.0;
        println!("Pass rate:    {:.1}%", pass_rate);
    }
    
    if failed > 0 {
        println!();
        println!("Failed tests:");
        for (i, test_id) in failed_tests.iter().enumerate() {
            if i < 20 {
                println!("  - {}", test_id);
            }
        }
        if failed_tests.len() > 20 {
            println!("  ... and {} more", failed_tests.len() - 20);
        }
    }
    
    println!();
    println!("═══════════════════════════════════════════════════════════");
}
