//! Integration tests for the grift crate.
//!
//! These tests verify that the unified grift crate correctly re-exports
//! all functionality from the underlying crates.

use grift::{Lisp, Evaluator, Value, ArenaIndex};

/// Helper to evaluate a string and check the result
fn eval_check<const N: usize>(_lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> ArenaIndex {
    eval.eval_str(input).expect(&format!("Failed to evaluate: {}", input))
}

#[test]
fn test_basic_arithmetic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval_check(&lisp, &mut eval, "(+ 1 2 3)");
    assert!(matches!(lisp.get(result), Ok(Value::Number(6))));
    
    let result = eval_check(&lisp, &mut eval, "(* 2 3 4)");
    assert!(matches!(lisp.get(result), Ok(Value::Number(24))));
    
    let result = eval_check(&lisp, &mut eval, "(- 10 3)");
    assert!(matches!(lisp.get(result), Ok(Value::Number(7))));
}

#[test]
fn test_define_and_call() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval_check(&lisp, &mut eval, "(define (square x) (* x x))");
    let result = eval_check(&lisp, &mut eval, "(square 7)");
    assert!(matches!(lisp.get(result), Ok(Value::Number(49))));
}

#[test]
fn test_factorial() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval_check(&lisp, &mut eval, "(define (factorial n) (if (= n 0) 1 (* n (factorial (- n 1)))))");
    let result = eval_check(&lisp, &mut eval, "(factorial 5)");
    assert!(matches!(lisp.get(result), Ok(Value::Number(120))));
}

#[test]
fn test_list_operations() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test cons
    let result = eval_check(&lisp, &mut eval, "(car (cons 1 2))");
    assert!(matches!(lisp.get(result), Ok(Value::Number(1))));
    
    // Test list
    let result = eval_check(&lisp, &mut eval, "(length '(a b c d e))");
    assert!(matches!(lisp.get(result), Ok(Value::Number(5))));
    
    // Test map
    let result = eval_check(&lisp, &mut eval, "(length (map (lambda (x) (* x x)) '(1 2 3)))");
    assert!(matches!(lisp.get(result), Ok(Value::Number(3))));
}

#[test]
fn test_closures() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval_check(&lisp, &mut eval, "(define (make-adder n) (lambda (x) (+ x n)))");
    eval_check(&lisp, &mut eval, "(define add5 (make-adder 5))");
    let result = eval_check(&lisp, &mut eval, "(add5 10)");
    assert!(matches!(lisp.get(result), Ok(Value::Number(15))));
}

#[test]
fn test_booleans() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Only #f is false
    let result = eval_check(&lisp, &mut eval, "(if #t 1 2)");
    assert!(matches!(lisp.get(result), Ok(Value::Number(1))));
    
    let result = eval_check(&lisp, &mut eval, "(if #f 1 2)");
    assert!(matches!(lisp.get(result), Ok(Value::Number(2))));
    
    // nil is truthy!
    let result = eval_check(&lisp, &mut eval, "(if '() 1 2)");
    assert!(matches!(lisp.get(result), Ok(Value::Number(1))));
}

#[test]
fn test_strings() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval_check(&lisp, &mut eval, "(string-length \"hello\")");
    assert!(matches!(lisp.get(result), Ok(Value::Number(5))));
    
    let result = eval_check(&lisp, &mut eval, "(string? \"hello\")");
    assert!(matches!(lisp.get(result), Ok(Value::True)));
}

#[test]
fn test_gc_control() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // GC control functions should work
    let result = eval_check(&lisp, &mut eval, "(gc-enabled?)");
    assert!(matches!(lisp.get(result), Ok(Value::True)));
    
    eval_check(&lisp, &mut eval, "(gc-disable)");
    let result = eval_check(&lisp, &mut eval, "(gc-enabled?)");
    assert!(matches!(lisp.get(result), Ok(Value::False)));
    
    eval_check(&lisp, &mut eval, "(gc-enable)");
    let result = eval_check(&lisp, &mut eval, "(gc-enabled?)");
    assert!(matches!(lisp.get(result), Ok(Value::True)));
}

#[test]
fn test_import_srfi_64() {
    let lisp: Lisp<50000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Import SRFI-64 via the library system: (import (srfi 64))
    eval_check(&lisp, &mut eval, "(import (srfi 64))");

    // Verify test-begin is available (it should be a procedure)
    let result = eval_check(&lisp, &mut eval, "(procedure? test-begin)");
    assert!(matches!(lisp.get(result), Ok(Value::True)));
    let result = eval_check(&lisp, &mut eval, "(procedure? test-end)");
    assert!(matches!(lisp.get(result), Ok(Value::True)));
    let result = eval_check(&lisp, &mut eval, "(procedure? test-equal)");
    assert!(matches!(lisp.get(result), Ok(Value::True)));
    let result = eval_check(&lisp, &mut eval, "(procedure? test-assert)");
    assert!(matches!(lisp.get(result), Ok(Value::True)));
}
