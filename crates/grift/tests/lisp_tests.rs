use grift::{Lisp, Value};

// ============================================================================
// Basic Arithmetic Tests
// ============================================================================

#[test]
fn test_addition() {
    let lisp: Lisp<20000> = Lisp::new();
    let three = lisp.eval("(+ 1 2)");
    assert_eq!(three, Ok(Value::Number(3)));
}

#[test]
fn test_addition_multiple() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(+ 1 2 3 4)"), Ok(Value::Number(10)));
}

#[test]
fn test_addition_zero_args() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(+)"), Ok(Value::Number(0)));
}

#[test]
fn test_subtraction() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(- 10 3)"), Ok(Value::Number(7)));
}

#[test]
fn test_subtraction_unary() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(- 5)"), Ok(Value::Number(-5)));
}

#[test]
fn test_multiplication() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(* 3 4)"), Ok(Value::Number(12)));
}

#[test]
fn test_multiplication_zero_args() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(*)"), Ok(Value::Number(1)));
}

#[test]
fn test_division() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(/ 10 3)"), Ok(Value::Number(3)));
}

#[test]
fn test_nested_arithmetic() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(+ (* 2 3) (- 10 4))"), Ok(Value::Number(12)));
}

// ============================================================================
// Comparison Tests
// ============================================================================

#[test]
fn test_numeric_equality() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(= 3 3)"), Ok(Value::True));
    assert_eq!(lisp.eval("(= 3 4)"), Ok(Value::False));
}

#[test]
fn test_less_than() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(< 1 2)"), Ok(Value::True));
    assert_eq!(lisp.eval("(< 2 1)"), Ok(Value::False));
}

#[test]
fn test_greater_than() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(> 2 1)"), Ok(Value::True));
    assert_eq!(lisp.eval("(> 1 2)"), Ok(Value::False));
}

#[test]
fn test_less_equal() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(<= 1 2)"), Ok(Value::True));
    assert_eq!(lisp.eval("(<= 2 2)"), Ok(Value::True));
    assert_eq!(lisp.eval("(<= 3 2)"), Ok(Value::False));
}

#[test]
fn test_greater_equal() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(>= 2 1)"), Ok(Value::True));
    assert_eq!(lisp.eval("(>= 2 2)"), Ok(Value::True));
    assert_eq!(lisp.eval("(>= 1 2)"), Ok(Value::False));
}

// ============================================================================
// Boolean & Special Form Tests
// ============================================================================

#[test]
fn test_booleans() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("#t"), Ok(Value::True));
    assert_eq!(lisp.eval("#f"), Ok(Value::False));
}

#[test]
fn test_if_true() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(if #t 1 2)"), Ok(Value::Number(1)));
}

#[test]
fn test_if_false() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(if #f 1 2)"), Ok(Value::Number(2)));
}

#[test]
fn test_if_no_else() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(if #f 1)"), Ok(Value::Nil));
}

#[test]
fn test_quote() {
    let lisp: Lisp<20000> = Lisp::new();
    // A quoted number should still be a number
    assert_eq!(lisp.eval("'42"), Ok(Value::Number(42)));
}

#[test]
fn test_and() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(and 1 2 3)"), Ok(Value::Number(3)));
    assert_eq!(lisp.eval("(and 1 #f 3)"), Ok(Value::False));
}

#[test]
fn test_or() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(or #f #f 3)"), Ok(Value::Number(3)));
    assert_eq!(lisp.eval("(or #f #f)"), Ok(Value::False));
}

// ============================================================================
// Define and Lambda Tests
// ============================================================================

#[test]
fn test_number_literal() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("42"), Ok(Value::Number(42)));
}

#[test]
fn test_negative_number() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("-7"), Ok(Value::Number(-7)));
}

// ============================================================================
// List Operations Tests
// ============================================================================

#[test]
fn test_cons_car_cdr() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(car (cons 1 2))"), Ok(Value::Number(1)));
    assert_eq!(lisp.eval("(cdr (cons 1 2))"), Ok(Value::Number(2)));
}

#[test]
fn test_list() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(car (list 1 2 3))"), Ok(Value::Number(1)));
}

// ============================================================================
// Type Predicate Tests
// ============================================================================

#[test]
fn test_null_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(null? '())"), Ok(Value::True));
    assert_eq!(lisp.eval("(null? 1)"), Ok(Value::False));
}

#[test]
fn test_not() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(not #f)"), Ok(Value::True));
    assert_eq!(lisp.eval("(not #t)"), Ok(Value::False));
}

#[test]
fn test_pair_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(pair? (cons 1 2))"), Ok(Value::True));
    assert_eq!(lisp.eval("(pair? 1)"), Ok(Value::False));
}

#[test]
fn test_number_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(number? 42)"), Ok(Value::True));
    assert_eq!(lisp.eval("(number? #t)"), Ok(Value::False));
}

#[test]
fn test_boolean_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(boolean? #t)"), Ok(Value::True));
    assert_eq!(lisp.eval("(boolean? 1)"), Ok(Value::False));
}

// ============================================================================
// Garbage Collection / Memory Leak Tests
// ============================================================================

#[test]
fn test_gc_collects_eval_garbage() {
    let lisp: Lisp<2000> = Lisp::new();

    // Evaluate an expression; intermediate values become garbage.
    lisp.eval("(+ 1 2)").unwrap();
    let before = lisp.stats();

    // GC with no roots should collect all values except the pre-allocated nil.
    let gc = lisp.collect_garbage(&[]);
    let after = lisp.stats();

    assert!(gc.collected > 0, "GC should collect some garbage");
    assert!(
        after.allocated <= before.allocated,
        "Allocated count should not increase after GC"
    );
}

#[test]
fn test_gc_repeated_eval_no_leak() {
    let lisp: Lisp<5000> = Lisp::new();

    // Evaluate many expressions; each creates temporary values.
    for _ in 0..50 {
        lisp.eval("(+ 1 2)").unwrap();
    }

    // GC should free the accumulated garbage.
    let gc = lisp.collect_garbage(&[]);
    let stats = lisp.stats();

    assert!(gc.collected > 0, "GC should collect garbage from repeated evals");
    // After collecting, arena should be mostly empty (only nil slot remains).
    assert!(
        stats.allocated < 50,
        "Arena should not keep growing without roots: allocated = {}",
        stats.allocated
    );
}

#[test]
fn test_gc_nested_expressions_no_leak() {
    let lisp: Lisp<5000> = Lisp::new();

    // Deeply nested expression creates many intermediate values.
    lisp.eval("(+ (* 2 3) (- 10 (+ 1 2)))").unwrap();
    lisp.eval("(if (> 5 3) (+ 1 (* 2 3)) (- 10 4))").unwrap();
    lisp.eval("(car (cons (+ 1 2) (list 4 5 6)))").unwrap();

    let before_gc = lisp.stats();
    let gc = lisp.collect_garbage(&[]);
    let after_gc = lisp.stats();

    assert!(gc.collected > 0, "GC should collect nested-expr garbage");
    assert!(
        after_gc.allocated < before_gc.allocated,
        "Allocated should decrease after GC: {} -> {}",
        before_gc.allocated,
        after_gc.allocated
    );
}

#[test]
fn test_gc_lambda_garbage() {
    let lisp: Lisp<5000> = Lisp::new();

    // Create and call a lambda; the lambda and its environment become garbage.
    lisp.eval("((lambda (x) (+ x 1)) 5)").unwrap();
    lisp.eval("((lambda (x y) (* x y)) 3 4)").unwrap();

    let gc = lisp.collect_garbage(&[]);
    assert!(gc.collected > 0, "GC should collect lambda garbage");
}

#[test]
fn test_gc_list_operations_no_leak() {
    let lisp: Lisp<5000> = Lisp::new();

    // Create lists, extract elements, create garbage.
    for _ in 0..20 {
        lisp.eval("(car (list 1 2 3))").unwrap();
        lisp.eval("(cdr (cons 1 2))").unwrap();
    }

    let gc = lisp.collect_garbage(&[]);
    let stats = lisp.stats();

    assert!(gc.collected > 0, "GC should collect list garbage");
    assert!(
        stats.allocated < 100,
        "Arena should not grow unbounded: allocated = {}",
        stats.allocated
    );
}

#[test]
fn test_gc_stress_many_evals() {
    let lisp: Lisp<10000> = Lisp::new();

    // Run many evaluations without GC, then collect.
    for i in 0..100 {
        lisp.eval("(+ 1 2)").unwrap();
        lisp.eval("(* 3 4)").unwrap();
        lisp.eval("(list 1 2 3)").unwrap();

        // Periodic GC every 25 iterations to prevent OOM.
        if (i + 1) % 25 == 0 {
            let _ = lisp.collect_garbage(&[]);
        }
    }

    let _gc = lisp.collect_garbage(&[]);
    let final_stats = lisp.stats();

    // After full GC, arena should be mostly empty.
    assert!(
        final_stats.allocated < 200,
        "Arena should be mostly empty after GC: allocated = {}",
        final_stats.allocated
    );
}

#[test]
fn test_gc_define_creates_garbage() {
    // Each call to lisp.eval() creates a fresh Evaluator, so defines don't
    // persist across calls. This test verifies that GC collects the garbage
    // from evaluator setup (builtins, symbols, etc.).
    let lisp: Lisp<5000> = Lisp::new();

    // Evaluate several expressions that create lambdas and intermediate values.
    lisp.eval("((lambda (x) (+ x 1)) 5)").unwrap();
    lisp.eval("((lambda (a b) (* a b)) 3 7)").unwrap();

    // After eval, the evaluators are dropped; all values are garbage.
    let gc = lisp.collect_garbage(&[]);
    assert!(gc.collected > 0, "GC should collect after evaluators are dropped");
}

#[test]
fn test_gc_stats_accuracy() {
    let lisp: Lisp<2000> = Lisp::new();

    let initial = lisp.stats();
    assert!(initial.allocated > 0, "Nil slot should be pre-allocated");

    lisp.eval("(+ 1 2)").unwrap();
    let after_eval = lisp.stats();
    assert!(
        after_eval.allocated > initial.allocated,
        "Eval should allocate arena slots"
    );

    let gc = lisp.collect_garbage(&[]);
    let after_gc = lisp.stats();

    // GC stats should report correct numbers.
    assert_eq!(
        gc.total_before, after_eval.allocated,
        "total_before should match pre-GC allocation count"
    );
    assert!(
        after_gc.allocated <= after_eval.allocated,
        "GC should not increase allocations"
    );
}
