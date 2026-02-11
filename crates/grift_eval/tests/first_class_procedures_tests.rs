mod common;

use grift_eval::*;
use common::{eval_to_num, eval_is_true, eval_to_string};

// ============================================================================
// R7RS First-Class Procedure Tests
//
// Per R7RS, all standard library procedures must be first-class values that
// can be: bound to variables, stored in data structures, passed to
// higher-order functions, returned from functions, and compared with eq?.
// ============================================================================

// ============================================================================
// values — must be first-class (R7RS §6.10)
// ============================================================================

#[test]
fn test_values_bind_to_variable() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // (define my-values values) should work
    eval.eval_str("(define my-values values)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval, "(call-with-values (lambda () (my-values 1 2)) +)"),
        3
    );
}

#[test]
fn test_values_store_in_vector() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define v (make-vector 1))").unwrap();
    eval.eval_str("(vector-set! v 0 values)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval,
            "(call-with-values (lambda () ((vector-ref v 0) 10 20)) +)"),
        30
    );
}

#[test]
fn test_values_pass_to_higher_order() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // (apply values '(1 2 3)) should return (1 2 3)
    assert_eq!(
        eval_to_num(&lisp, &mut eval,
            "(call-with-values (lambda () (apply values '(1 2 3))) +)"),
        6
    );
}

#[test]
fn test_values_identity() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(eq? values values)"));
    eval.eval_str("(define v1 values)").unwrap();
    eval.eval_str("(define v2 values)").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(eq? v1 v2)"));
}

#[test]
fn test_values_is_procedure() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(procedure? values)"));
}

// ============================================================================
// call-with-values — must be first-class (R7RS §6.10)
// ============================================================================

#[test]
fn test_call_with_values_bind_to_variable() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define my-cwv call-with-values)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval,
            "(my-cwv (lambda () (values 3 4)) *)"),
        12
    );
}

#[test]
fn test_call_with_values_identity() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(eq? call-with-values call-with-values)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? call-with-values)"));
}

// ============================================================================
// apply — must be first-class (R7RS §6.4)
// ============================================================================

#[test]
fn test_apply_bind_to_variable() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define my-apply apply)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(my-apply + '(1 2 3))"), 6);
}

#[test]
fn test_apply_store_in_vector() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define v (make-vector 1))").unwrap();
    eval.eval_str("(vector-set! v 0 apply)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval, "((vector-ref v 0) + '(10 20))"),
        30
    );
}

#[test]
fn test_apply_identity() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(eq? apply apply)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? apply)"));
}

#[test]
fn test_apply_with_fixed_args() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // (apply + 1 2 '(3)) should work
    assert_eq!(eval_to_num(&lisp, &mut eval, "(apply + 1 2 '(3))"), 6);
}

// ============================================================================
// call/cc — must be first-class (R7RS §6.10)
// ============================================================================

#[test]
fn test_call_cc_bind_to_variable() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define my-cc call/cc)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval,
            "(my-cc (lambda (k) (k 42)))"),
        42
    );
}

#[test]
fn test_call_with_current_continuation_bind() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define my-cc call-with-current-continuation)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval,
            "(my-cc (lambda (k) (k 99)))"),
        99
    );
}

#[test]
fn test_call_cc_store_in_vector() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define v (make-vector 1))").unwrap();
    eval.eval_str("(vector-set! v 0 call/cc)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval,
            "((vector-ref v 0) (lambda (k) (k 77)))"),
        77
    );
}

#[test]
fn test_call_cc_identity() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(eq? call/cc call/cc)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? call/cc)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? call-with-current-continuation)"));
}

// ============================================================================
// dynamic-wind — must be first-class (R7RS §6.10)
// ============================================================================

#[test]
fn test_dynamic_wind_bind_to_variable() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define my-dw dynamic-wind)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval,
            "(my-dw (lambda () #f) (lambda () 42) (lambda () #f))"),
        42
    );
}

#[test]
fn test_dynamic_wind_identity() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(eq? dynamic-wind dynamic-wind)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? dynamic-wind)"));
}

// ============================================================================
// with-exception-handler — must be first-class (R7RS §6.11)
// ============================================================================

#[test]
fn test_with_exception_handler_bind_to_variable() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define my-weh with-exception-handler)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval,
            "(my-weh (lambda (e) 0) (lambda () 42))"),
        42
    );
}

#[test]
fn test_with_exception_handler_identity() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(eq? with-exception-handler with-exception-handler)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? with-exception-handler)"));
}

// ============================================================================
// raise — must be first-class (R7RS §6.11)
// ============================================================================

#[test]
fn test_raise_bind_to_variable() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define my-raise raise)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval,
            "(with-exception-handler (lambda (e) e) (lambda () (my-raise 42)) )"),
        42
    );
}

#[test]
fn test_raise_identity() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(eq? raise raise)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? raise)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? raise-continuable)"));
}

// ============================================================================
// eval — must be first-class (R7RS §6.12)
// ============================================================================

#[test]
fn test_eval_bind_to_variable() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define my-eval eval)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(my-eval '(+ 1 2))"), 3);
}

#[test]
fn test_eval_store_in_vector() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define v (make-vector 1))").unwrap();
    eval.eval_str("(vector-set! v 0 eval)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval, "((vector-ref v 0) '(+ 10 20))"),
        30
    );
}

#[test]
fn test_eval_identity() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(eq? eval eval)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? eval)"));
}

// ============================================================================
// environment — must be first-class (R7RS §6.12)
// ============================================================================

#[test]
fn test_environment_bind_to_variable() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define my-env environment)").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? my-env)"));
}

#[test]
fn test_environment_identity() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(eq? environment environment)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? environment)"));
}

// ============================================================================
// Cross-procedure first-class tests (from problem statement)
// ============================================================================

#[test]
fn test_values_proc_in_hide_pattern() {
    // From the problem statement - the workaround should no longer be needed
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // This pattern from the problem statement should work without workarounds
    eval.eval_str(r#"
        (define (hide r x)
          (call-with-values
           (lambda ()
             (values (vector values (lambda (x) x))
                     (if (< r 100) 0 1)))
           (lambda (v i)
             ((vector-ref v i) x))))
    "#).unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(hide 0 42)"), 42);
}

#[test]
fn test_all_procedures_in_list() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Store all the previously-special-form procedures in a list
    eval.eval_str(r#"
        (define procs (list values call-with-values apply
                       call/cc call-with-current-continuation
                       dynamic-wind with-exception-handler
                       raise raise-continuable eval environment))
    "#).unwrap();

    // All should be procedures
    assert_eq!(
        eval_to_num(&lisp, &mut eval,
            "(length (filter procedure? procs))"),
        11
    );
}

#[test]
fn test_return_from_function() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Return apply from a function, then use it
    eval.eval_str("(define (get-apply) apply)").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval, "((get-apply) + '(1 2 3))"),
        6
    );
}

#[test]
fn test_higher_order_with_apply() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Use apply as argument to a higher-order function
    eval.eval_str("(define (use-it f) (f + '(1 2 3)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(use-it apply)"), 6);
}
