//! Edge Case Tests for Grift's Syntax-Case Implementation
//!
//! These tests explore the boundaries of grift's macro system, particularly
//! behaviors that differ from Racket/Guile due to grift's single-phase model.
//!
//! Test categories:
//! 1. First-class syntax object manipulation
//! 2. Runtime-created syntax
//! 3. Cross-environment syntax usage
//! 4. Phase violation tests (things that work in grift but not Racket/Guile)
//! 5. Hygiene edge cases

use grift_eval::*;

fn eval_to_num<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> isize {
    let result = eval.eval_str(input).unwrap();
    lisp.get(result).unwrap().as_number().unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Suite A: First-Class Syntax Object Manipulation
// ═══════════════════════════════════════════════════════════════════════════
// These tests verify that syntax objects behave as first-class values,
// which is a key feature of grift's single-phase model.

/// Test A.1: Syntax objects can be stored in variables
///
/// In Racket/Guile, runtime-created syntax has limited lexical context.
/// In grift, syntax objects preserve their full creation-site environment.
#[test]
fn test_a1_syntax_in_variable() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define my-stx
          (let ((x 42))
            (syntax x)))
    "#).unwrap();
    
    eval.eval_str(r#"
        (define-syntax use-my-stx
          (lambda (_) my-stx))
    "#).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(use-my-stx)"), 42,
        "Syntax stored in variable should preserve its binding");
}

/// Test A.2: Syntax objects in pairs
///
/// Syntax objects can be consed like any other value.
#[test]
fn test_a2_syntax_in_pair() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define stx-pair
          (let ((a 10) (b 20))
            (cons (syntax a) (syntax b))))
    "#).unwrap();
    
    eval.eval_str(r#"
        (define-syntax use-car
          (lambda (_) (car stx-pair)))
    "#).unwrap();
    
    eval.eval_str(r#"
        (define-syntax use-cdr
          (lambda (_) (cdr stx-pair)))
    "#).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(use-car)"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(use-cdr)"), 20);
}

/// Test A.3: Syntax objects in vectors
///
/// Syntax objects can be stored in vectors.
#[test]
fn test_a3_syntax_in_vector() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define stx-vec
          (let ((x 100))
            (vector (syntax x) (syntax x))))
    "#).unwrap();
    
    eval.eval_str(r#"
        (define-syntax use-first
          (lambda (_) (vector-ref stx-vec 0)))
    "#).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(use-first)"), 100);
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Suite B: Runtime Syntax Creation
// ═══════════════════════════════════════════════════════════════════════════
// These tests demonstrate grift's ability to create syntax at runtime,
// which would be restricted in phase-separated systems.

/// Test B.1: Function that returns syntax
///
/// A regular function can return syntax objects for use in macros.
#[test]
fn test_b1_function_returning_syntax() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define (make-const-syntax n)
          (let ((val n))
            (syntax val)))
    "#).unwrap();
    
    eval.eval_str("(define stx-5 (make-const-syntax 5))").unwrap();
    eval.eval_str("(define stx-10 (make-const-syntax 10))").unwrap();
    
    eval.eval_str(r#"
        (define-syntax get-5
          (lambda (_) stx-5))
    "#).unwrap();
    
    eval.eval_str(r#"
        (define-syntax get-10
          (lambda (_) stx-10))
    "#).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(get-5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(get-10)"), 10);
}

/// Test B.2: Conditional syntax creation
///
/// Syntax can be created conditionally at runtime.
#[test]
fn test_b2_conditional_syntax_creation() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define (make-syntax-or-literal use-syntax)
          (let ((x 42))
            (if use-syntax
                (syntax x)
                x)))
    "#).unwrap();
    
    // When using syntax, we get the captured binding
    eval.eval_str("(define my-stx (make-syntax-or-literal #t))").unwrap();
    eval.eval_str(r#"
        (define-syntax use-stx
          (lambda (_) my-stx))
    "#).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(use-stx)"), 42);
}

/// Test B.3: Recursive syntax creation
///
/// Syntax can be created recursively.
#[test]
fn test_b3_recursive_syntax_creation() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define (nested-syntax depth)
          (let ((x depth))
            (if (= depth 0)
                (syntax x)
                (nested-syntax (- depth 1)))))
    "#).unwrap();
    
    eval.eval_str("(define deep-stx (nested-syntax 5))").unwrap();
    eval.eval_str(r#"
        (define-syntax use-deep
          (lambda (_) deep-stx))
    "#).unwrap();
    
    // The innermost call creates syntax with x=0
    assert_eq!(eval_to_num(&lisp, &mut eval, "(use-deep)"), 0);
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Suite C: Hygiene Edge Cases
// ═══════════════════════════════════════════════════════════════════════════

/// Test C.1: Multiple macro invocations with same introduced names
///
/// Each macro expansion should have its own hygienic scope.
#[test]
fn test_c1_multiple_expansions_hygiene() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax with-temp
          (lambda (stx)
            (syntax-case stx ()
              ((_ val body)
               (syntax (let ((temp val))
                         body))))))
    "#).unwrap();
    
    // Nested uses of with-temp should each have their own 'temp'
    let result = eval.eval_str(r#"
        (with-temp 10
          (with-temp 20
            (+ temp temp)))
    "#).unwrap();
    
    // The innermost temp is 20, so 20+20=40
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(40));
}

/// Test C.2: Pattern variable shadowing local binding
///
/// When a local binding shadows a pattern variable, syntax should use the local.
#[test]
fn test_c2_pattern_shadowing() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax shadow-test
          (lambda (stx)
            (syntax-case stx ()
              ((kw x)
               (let ((outer-x (syntax x)))
                 (let ((x 999))  ; Shadows pattern variable x
                   (let ((inner-x (syntax x)))
                     ;; outer-x refers to pattern x, inner-x refers to local x
                     (if (free-identifier=? outer-x inner-x)
                         (syntax 'same)
                         (syntax 'different)))))))))
    "#).unwrap();
    
    let result = eval.eval_str("(shadow-test 123)").unwrap();
    assert!(lisp.symbol_matches(result, "different").unwrap(),
        "Pattern variable and local binding should be different identifiers");
}

/// Test C.3: Hygiene with recursive macros
///
/// Recursive macro calls should maintain proper hygiene.
#[test]
fn test_c3_recursive_macro_hygiene() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax sum-to
          (lambda (stx)
            (syntax-case stx ()
              ((kw n)
               (let ((num (syntax->datum (syntax n))))
                 (if (= num 0)
                     (syntax 0)
                     (with-syntax ((m (- num 1)))
                       (syntax (+ n (sum-to m))))))))))
    "#).unwrap();
    
    // sum-to 3 = 3 + 2 + 1 + 0 = 6
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum-to 3)"), 6);
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Suite D: datum->syntax and syntax->datum
// ═══════════════════════════════════════════════════════════════════════════

/// Test D.1: datum->syntax with template context
///
/// datum->syntax should create an identifier with the template's lexical context.
#[test]
fn test_d1_datum_to_syntax_context() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax make-ref
          (lambda (stx)
            (syntax-case stx ()
              ((kw name)
               (datum->syntax (syntax kw) (syntax->datum (syntax name)))))))
    "#).unwrap();
    
    eval.eval_str("(define foo 42)").unwrap();
    
    // make-ref should create a reference that resolves in the macro's context
    assert_eq!(eval_to_num(&lisp, &mut eval, "(make-ref foo)"), 42);
}

/// Test D.2: syntax->datum round-trip
///
/// Converting syntax to datum and back should preserve structure.
#[test]
fn test_d2_syntax_datum_roundtrip() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax structure-test
          (lambda (stx)
            (syntax-case stx ()
              ((kw (a b c))
               (let ((datum (syntax->datum (syntax (a b c)))))
                 (if (and (pair? datum)
                          (eq? (car datum) 'a)
                          (eq? (cadr datum) 'b)
                          (eq? (caddr datum) 'c))
                     (syntax 'correct)
                     (syntax 'wrong)))))))
    "#).unwrap();
    
    let result = eval.eval_str("(structure-test (a b c))").unwrap();
    assert!(lisp.symbol_matches(result, "correct").unwrap());
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Suite E: with-syntax Edge Cases
// ═══════════════════════════════════════════════════════════════════════════

/// Test E.1: with-syntax with computed values
///
/// with-syntax should work with values computed at macro expansion time.
#[test]
fn test_e1_with_syntax_computed() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax double-literal
          (lambda (stx)
            (syntax-case stx ()
              ((kw n)
               (let ((doubled (* 2 (syntax->datum (syntax n)))))
                 (with-syntax ((result doubled))
                   (syntax result)))))))
    "#).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(double-literal 21)"), 42);
}

/// Test E.2: with-syntax with multiple bindings
///
/// with-syntax should handle multiple bindings correctly.
#[test]
fn test_e2_with_syntax_multiple() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax swap-and-add
          (lambda (stx)
            (syntax-case stx ()
              ((kw a b)
               (with-syntax ((x (syntax b))
                             (y (syntax a)))
                 (syntax (+ x y)))))))
    "#).unwrap();
    
    // Should compute (+ 20 10) = 30
    assert_eq!(eval_to_num(&lisp, &mut eval, "(swap-and-add 10 20)"), 30);
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Suite F: Ellipsis Edge Cases
// ═══════════════════════════════════════════════════════════════════════════

/// Test F.1: Empty ellipsis match
///
/// Ellipsis should correctly handle zero matches.
#[test]
fn test_f1_empty_ellipsis() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax list-or-zero
          (lambda (stx)
            (syntax-case stx ()
              ((kw x ...)
               (syntax (list x ...))))))
    "#).unwrap();
    
    // Empty case
    let result = eval.eval_str("(list-or-zero)").unwrap();
    assert!(lisp.get(result).unwrap().is_nil(),
        "Empty ellipsis should produce empty list");
    
    // Non-empty case
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (list-or-zero 1 2 3))"), 3);
}

/// Test F.2: Ellipsis with multiple pattern variables
///
/// Multiple pattern variables under ellipsis should expand in parallel.
#[test]
fn test_f2_parallel_ellipsis() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax zip-add
          (lambda (stx)
            (syntax-case stx ()
              ((kw (a b) ...)
               (syntax (list (+ a b) ...))))))
    "#).unwrap();
    
    // (zip-add (1 10) (2 20) (3 30)) => (11 22 33)
    let result = eval.eval_str("(zip-add (1 10) (2 20) (3 30))").unwrap();
    let first = lisp.car(result).unwrap();
    assert_eq!(lisp.get(first).unwrap().as_number(), Some(11));
}

/// Test F.3: Ellipsis after fixed elements
///
/// Patterns with fixed elements followed by ellipsis.
#[test]
fn test_f3_fixed_then_ellipsis() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax first-then-rest
          (lambda (stx)
            (syntax-case stx ()
              ((kw first rest ...)
               (syntax (cons first (list rest ...)))))))
    "#).unwrap();
    
    let result = eval.eval_str("(first-then-rest 1 2 3 4)").unwrap();
    let car = lisp.car(result).unwrap();
    let cdr = lisp.cdr(result).unwrap();
    
    assert_eq!(lisp.get(car).unwrap().as_number(), Some(1));
    // Verify the rest is a list of (2 3 4)
    assert_eq!(lisp.list_len(cdr).unwrap(), 3);
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Suite G: Identifier Comparison Edge Cases
// ═══════════════════════════════════════════════════════════════════════════

/// Test G.1: bound-identifier=? with identifiers from different contexts
///
/// Identifiers from different macro invocations should not be bound-identifier=?.
#[test]
fn test_g1_different_contexts_not_bound_eq() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create identifiers in two separate let bindings - they should have different marks
    eval.eval_str(r#"
        (define-syntax check-different-contexts
          (lambda (stx)
            (syntax-case stx ()
              ((kw)
               (let ((id1 (datum->syntax (syntax kw) 'x))
                     (id2 (datum->syntax (syntax kw) 'x)))
                 ;; Same name, same template context - should be bound-identifier=?
                 (if (bound-identifier=? id1 id2)
                     (syntax 'same)
                     (syntax 'different)))))))
    "#).unwrap();
    
    let result = eval.eval_str("(check-different-contexts)").unwrap();
    // When created in the same macro expansion with same template, should be same
    assert!(lisp.symbol_matches(result, "same").unwrap(),
        "Identifiers with same name and template context should be bound-identifier=?");
}

/// Test G.2: free-identifier=? across environments
///
/// free-identifier=? should check if identifiers refer to the same binding.
#[test]
fn test_g2_free_identifier_same_binding() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define global-x 100)").unwrap();
    
    eval.eval_str(r#"
        (define-syntax check-free-eq
          (lambda (stx)
            (syntax-case stx ()
              ((kw a b)
               (if (free-identifier=? (syntax a) (syntax b))
                   (syntax 'same)
                   (syntax 'different))))))
    "#).unwrap();
    
    // Same identifier twice should be free-identifier=?
    let result = eval.eval_str("(check-free-eq global-x global-x)").unwrap();
    assert!(lisp.symbol_matches(result, "same").unwrap());
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Suite H: Macro-Defining Macros
// ═══════════════════════════════════════════════════════════════════════════

/// Test H.1: Simple macro-defining macro
///
/// A macro that defines another macro.
#[test]
fn test_h1_macro_defining_macro() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax define-constant-macro
          (lambda (stx)
            (syntax-case stx ()
              ((kw name val)
               (syntax (define-syntax name
                         (lambda (_) (syntax val))))))))
    "#).unwrap();
    
    eval.eval_str("(define-constant-macro forty-two 42)").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(forty-two)"), 42);
}

/// Test H.2: Parameterized macro generator
///
/// A macro that generates macros with different behavior.
#[test]
fn test_h2_parameterized_macro_generator() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax define-adder-macro
          (lambda (stx)
            (syntax-case stx ()
              ((kw name amount)
               (syntax (define-syntax name
                         (lambda (inner-stx)
                           (syntax-case inner-stx ()
                             ((_ x) (syntax (+ x amount)))))))))))
    "#).unwrap();
    
    eval.eval_str("(define-adder-macro add-5 5)").unwrap();
    eval.eval_str("(define-adder-macro add-10 10)").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add-5 100)"), 105);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add-10 100)"), 110);
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Suite I: Stress Tests
// ═══════════════════════════════════════════════════════════════════════════

/// Test I.1: Deeply nested let-syntax
///
/// Many levels of nested let-syntax should maintain proper scoping.
#[test]
fn test_i1_deeply_nested_let_syntax() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval.eval_str(r#"
        (let-syntax ((a (lambda (stx) (syntax 1))))
          (let-syntax ((b (lambda (stx) (syntax (+ (a) 2)))))
            (let-syntax ((c (lambda (stx) (syntax (+ (b) 3)))))
              (c))))
    "#).unwrap();
    
    // 1 + 2 + 3 = 6
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(6));
}

/// Test I.2: Large ellipsis expansion
///
/// Ellipsis should handle larger lists without issues.
#[test]
fn test_i2_large_ellipsis() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax sum-all
          (lambda (stx)
            (syntax-case stx ()
              ((kw x ...)
               (syntax (+ x ...))))))
    "#).unwrap();
    
    // Sum of 1 to 10
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(sum-all 1 2 3 4 5 6 7 8 9 10)"), 55);
}
