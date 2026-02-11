mod common;

use grift_eval::*;
use common::eval_to_num;

// ============================================================================
// interaction-environment (R7RS §6.12)
// ============================================================================

#[test]
fn test_interaction_environment_returns_environment() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // interaction-environment should return an environment object
    let result = eval.eval_str("(interaction-environment)").unwrap();
    assert!(
        matches!(lisp.get(result).unwrap(), Value::Environment { mutable: true, .. }),
        "interaction-environment should return a mutable environment"
    );
}

#[test]
fn test_eval_with_interaction_environment() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // eval expression in the interaction environment
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval '(+ 1 2) (interaction-environment))"), 3);
}

#[test]
fn test_eval_with_interaction_environment_sees_definitions() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Define a variable, then access it via eval + interaction-environment
    eval.eval_str("(define x 42)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval 'x (interaction-environment))"), 42);
}

#[test]
fn test_eval_1_arg_still_works() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Existing 1-arg eval should still work
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval '(+ 10 20))"), 30);
}

#[test]
fn test_eval_2_arg_with_expression() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // 2-arg eval with a more complex expression
    assert_eq!(
        eval_to_num(&lisp, &mut eval, "(eval '(* 3 (+ 2 5)) (interaction-environment))"),
        21
    );
}

// ============================================================================
// environment (R7RS §6.12)
// ============================================================================

#[test]
fn test_environment_with_library() {
    let lisp: Lisp<40000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // First define a library
    eval.eval_str("(define-library (test math) (export add1) (begin (define (add1 x) (+ x 1))))").unwrap();

    // Create environment from library
    let result = eval.eval_str("(environment '(test math))").unwrap();
    assert!(
        matches!(lisp.get(result).unwrap(), Value::Environment { mutable: false, .. }),
        "environment should return an immutable environment"
    );
}

#[test]
fn test_eval_in_library_environment() {
    let lisp: Lisp<40000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Define a library with add1
    eval.eval_str("(define-library (test math) (export add1) (begin (define (add1 x) (+ x 1))))").unwrap();

    // Create env and eval in it
    eval.eval_str("(define math-env (environment '(test math)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval '(add1 10) math-env)"), 11);
}

#[test]
fn test_environment_with_quoted_spec() {
    let lisp: Lisp<40000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Define a library
    eval.eval_str("(define-library (test greet) (export hi) (begin (define hi 99)))").unwrap();

    // environment with quoted import spec should also work
    eval.eval_str("(define env (environment '(test greet)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval 'hi env)"), 99);
}

#[test]
fn test_environment_multiple_libraries() {
    let lisp: Lisp<40000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Define two libraries
    eval.eval_str("(define-library (lib a) (export x) (begin (define x 10)))").unwrap();
    eval.eval_str("(define-library (lib b) (export y) (begin (define y 20)))").unwrap();

    // Combine both
    eval.eval_str("(define combined-env (environment '(lib a) '(lib b)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval 'x combined-env)"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval 'y combined-env)"), 20);
}

#[test]
fn test_environment_display() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Environment should display as #<environment>
    let result = eval.eval_str("(interaction-environment)").unwrap();
    let display = format!("{}", lisp.display(result));
    assert_eq!(display, "#<environment>");
}

#[test]
fn test_eval_non_environment_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Passing a non-environment value as second arg should error
    let result = eval.eval_str("(eval '(+ 1 2) 42)");
    assert!(result.is_err(), "eval with non-environment second arg should error");
}

// ============================================================================
// Large environment tests (validates merge_environments fix)
// ============================================================================

#[test]
fn test_let_with_many_bindings() {
    // Validates fix: merge_environments previously used a 64-element stack array.
    // A let with >64 bindings should work correctly with arena cons list.
    let lisp: Lisp<50000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Generate a let with 100 bindings, sum them all
    let mut bindings = String::new();
    let mut sum_expr = String::from("(+");
    for i in 0..100 {
        bindings.push_str(&format!(" (v{} {})", i, i));
        sum_expr.push_str(&format!(" v{}", i));
    }
    sum_expr.push(')');
    let expr = format!("(let ({}) {})", bindings, sum_expr);
    // Sum of 0..99 = 4950
    assert_eq!(eval_to_num(&lisp, &mut eval, &expr), 4950);
}
