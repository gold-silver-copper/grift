//! Peroxide R5RS Pitfalls Tests
//!
//! These tests are from the peroxide Scheme implementation:
//! https://github.com/MattX/peroxide/tree/master/tests/scheme
//!
//! The tests check for edge cases and pitfalls in R5RS Scheme implementations.
//!
//! Note: Many of these tests involve advanced features like call/cc that may not
//! be fully implemented. This test file documents the test cases but runs only
//! the ones that can be safely executed.

use grift_eval::*;

#[test]
fn test_peroxide_pitfalls_section_4_no_reserved_identifiers() {
    // Section 4: No identifiers are reserved
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test 4.1: lambda can be used as a parameter name
    // (should-be 4.1 '(x) ((lambda lambda lambda) 'x))
    // This creates a lambda with rest-args named 'lambda' that returns 'lambda' (the args)
    let result = eval.eval_str("((lambda lambda lambda) 'x)").unwrap();
    assert!(lisp.get(result).unwrap().is_cons());
    let first = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(first, "x").unwrap());
    
    // Test 4.2: begin can be used as a parameter
    // (should-be 4.2 '(1 2 3) ((lambda (begin) (begin 1 2 3)) (lambda lambda lambda)))
    // When begin is a parameter, (begin 1 2 3) should call the function stored in begin
    // (lambda lambda lambda) is a function that returns its args as a list
    // NOW FIXED: Special forms can be shadowed by variable bindings
    let result = eval.eval_str("((lambda (begin) (begin 1 2 3)) (lambda lambda lambda))").unwrap();
    assert!(lisp.get(result).unwrap().is_cons());
    let first = lisp.car(result).unwrap();
    assert_eq!(lisp.get(first).unwrap().as_number().unwrap(), 1);
    assert_eq!(lisp.list_len(result).unwrap(), 3);
    
    // Test 4.3: quote can be shadowed
    // (should-be 4.3 #f (let ((quote -)) (eqv? '1 1)))
    // NOW FIXED: Special forms can be shadowed by variable bindings
    let result = eval.eval_str("(let ((quote -)) (eqv? '1 1))").unwrap();
    assert!(lisp.get(result).unwrap().is_false());
}

#[test]
fn test_peroxide_pitfalls_section_5_false_nil_distinctness() {
    // Section 5: #f/() distinctness
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test 5.1: #f and () must be distinct (eq?)
    let result = eval.eval_str("(eq? #f '())").unwrap();
    assert!(lisp.get(result).unwrap().is_false());
    
    // Test 5.2: #f and () must be distinct (eqv?)
    let result = eval.eval_str("(eqv? #f '())").unwrap();
    assert!(lisp.get(result).unwrap().is_false());
    
    // Test 5.3: #f and () must be distinct (equal?)
    let result = eval.eval_str("(equal? #f '())").unwrap();
    assert!(lisp.get(result).unwrap().is_false());
}

#[test]
fn test_peroxide_pitfalls_section_8_miscellaneous() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test 8.1: The Petrofsky let test - named-let with - as loop name
    // (should-be 8.1 -1 (let - ((n (- 1))) n))
    let result = eval.eval_str("(let - ((n (- 1))) n)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number().unwrap(), -1);
    
    // Test 8.3: let-syntax with define
    // (should-be 8.3 1
    //   (let ((x 1))
    //     (let-syntax ((foo (syntax-rules () ((_) 2))))
    //       (define x (foo))
    //       3)
    //     x))
    // Note: This tests scoping of definitions inside let-syntax
}

#[test]
fn test_peroxide_pitfalls_documentation() {
    // This test documents the pitfall tests that are present
    // Many require call/cc which may cause issues
    
    println!("\n=== Peroxide R5RS Pitfalls Test Categories ===");
    println!("Section 1: Proper letrec implementation (tests involve call/cc)");
    println!("Section 2: Proper call/cc and procedure application");
    println!("Section 3: Hygienic macros");
    println!("Section 4: No identifiers are reserved");
    println!("Section 5: #f/() distinctness");
    println!("Section 6: string->symbol case sensitivity");
    println!("Section 7: First class continuations");
    println!("Section 8: Miscellaneous");
    println!("\nMany of these tests involve advanced features like call/cc.");
    println!("Only the safe, non-continuation tests are run automatically.");
}
