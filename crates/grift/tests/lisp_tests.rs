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


#[test]
fn test_fib_self_apply() {
    let lisp: Lisp<20000> = Lisp::new();
    let program = r#"
        ((lambda (fib-self n)
            (fib-self fib-self n))
         (lambda (self n)
            (if (< n 2)
                n
                (+ (self self (- n 1))
                   (self self (- n 2)))))
         10)
    "#;
    assert_eq!(lisp.eval(program), Ok(Value::Number(55)));
}

// ============================================================================
// Call-by-Need Laziness Tests
// ============================================================================

#[test]
fn test_laziness_unused_arg_not_evaluated() {
    // (const x y) returns x without evaluating y.
    // The second argument is a type error that would crash if evaluated.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        ((lambda (x y) x) 1 (+ 1 "crash"))
    "#);
    assert_eq!(result, Ok(Value::Number(1)));
}

#[test]
fn test_laziness_if_unused_branch() {
    // The false branch contains a type error; it must not be evaluated.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(r#"(if #t 42 (+ 1 "crash"))"#),
        Ok(Value::Number(42))
    );
}

#[test]
fn test_laziness_define_not_forced() {
    // Defining a value that would error if forced, but never using it.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        (begin
            (define bad (+ 1 "crash"))
            42)
    "#);
    assert_eq!(result, Ok(Value::Number(42)));
}

#[test]
fn test_laziness_let_not_forced() {
    // Let binding that would error, but the binding is never used.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        (let ((x (+ 1 "crash")))
            42)
    "#);
    assert_eq!(result, Ok(Value::Number(42)));
}

// ============================================================================
// Memoization Tests (At-Most-Once)
// ============================================================================

#[test]
fn test_memoization_double() {
    // (double x) uses x twice. With memoization, (+ 1 2) is evaluated once.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        ((lambda (double)
            (double (+ 1 2)))
         (lambda (x) (+ x x)))
    "#);
    assert_eq!(result, Ok(Value::Number(6)));
}

#[test]
fn test_memoization_let_reuse() {
    // x is used twice in let body. It should be evaluated at most once.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        (let ((x (+ 10 20)))
            (+ x x))
    "#);
    assert_eq!(result, Ok(Value::Number(60)));
}

#[test]
fn test_answers_through_let_bindings() {
    // Answers can be wrapped in pending bindings (indirections).
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        (let ((x 1))
            (let ((y 2))
                (+ x y)))
    "#);
    assert_eq!(result, Ok(Value::Number(3)));
}

// ============================================================================
// Tail-Call Optimization Tests
// ============================================================================

#[test]
fn test_tco_countdown() {
    // Deep recursion that must not overflow the Rust stack.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        (begin
            (define (count n)
                (if (= n 0) 0 (count (- n 1))))
            (count 1000))
    "#);
    assert_eq!(result, Ok(Value::Number(0)));
}

#[test]
fn test_tco_mutual_recursion() {
    // Mutual recursion in tail position.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        (begin
            (define (my-even? n)
                (if (= n 0) #t (my-odd? (- n 1))))
            (define (my-odd? n)
                (if (= n 0) #f (my-even? (- n 1))))
            (my-even? 1000))
    "#);
    assert_eq!(result, Ok(Value::True));
}

#[test]
fn test_tco_begin_tail_position() {
    // The last expression in begin is a tail position.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        (begin
            (define (loop n)
                (if (= n 0) 42
                    (begin
                        (+ 1 2)
                        (loop (- n 1)))))
            (loop 1000))
    "#);
    assert_eq!(result, Ok(Value::Number(42)));
}

// ============================================================================
// Cycle Detection (Black-Holing) Tests
// ============================================================================

#[test]
fn test_cycle_detection_self_reference() {
    // (define x x) then forcing x → circular dependency error.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        (begin
            (define x x)
            x)
    "#);
    assert!(result.is_err(), "Circular reference should produce an error");
}

#[test]
fn test_cycle_detection_indirect() {
    // Indirect cycle: a → b → a.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        (begin
            (define a b)
            (define b a)
            a)
    "#);
    assert!(result.is_err(), "Indirect cycle should produce an error");
}

// ============================================================================
// Infinite Data Structures Tests
// ============================================================================

#[test]
fn test_infinite_ones() {
    // (define ones (cons 1 ones)) — infinite list of ones.
    let lisp: Lisp<50000> = Lisp::new();
    assert_eq!(
        lisp.eval(r#"
            (begin
                (define ones (cons 1 ones))
                (car ones))
        "#),
        Ok(Value::Number(1))
    );
}

#[test]
fn test_infinite_ones_cdr() {
    let lisp: Lisp<50000> = Lisp::new();
    assert_eq!(
        lisp.eval(r#"
            (begin
                (define ones (cons 1 ones))
                (car (cdr ones)))
        "#),
        Ok(Value::Number(1))
    );
}

#[test]
fn test_infinite_ones_cdr_cdr() {
    let lisp: Lisp<50000> = Lisp::new();
    assert_eq!(
        lisp.eval(r#"
            (begin
                (define ones (cons 1 ones))
                (car (cdr (cdr ones))))
        "#),
        Ok(Value::Number(1))
    );
}

#[test]
fn test_infinite_nats() {
    // Infinite stream of natural numbers: test car, cadr, and caddr.
    let lisp: Lisp<50000> = Lisp::new();
    assert_eq!(
        lisp.eval(r#"
            (begin
                (define (nats-from n) (cons n (nats-from (+ n 1))))
                (define nats (nats-from 0))
                (car nats))
        "#),
        Ok(Value::Number(0))
    );
    assert_eq!(
        lisp.eval(r#"
            (begin
                (define (nats-from n) (cons n (nats-from (+ n 1))))
                (define nats (nats-from 0))
                (car (cdr nats)))
        "#),
        Ok(Value::Number(1))
    );
    assert_eq!(
        lisp.eval(r#"
            (begin
                (define (nats-from n) (cons n (nats-from (+ n 1))))
                (define nats (nats-from 0))
                (car (cdr (cdr nats))))
        "#),
        Ok(Value::Number(2))
    );
}

// ============================================================================
// Laziness + TCO Combined Tests
// ============================================================================

#[test]
fn test_tco_with_lazy_accumulator() {
    // Tail-recursive sum with lazy arguments.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        (begin
            (define (sum-to n acc)
                (if (= n 0) acc (sum-to (- n 1) (+ acc n))))
            (sum-to 100 0))
    "#);
    assert_eq!(result, Ok(Value::Number(5050)));
}

#[test]
fn test_tco_iterative_fib() {
    // Iterative fib via self-application with define.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"
        (begin
            (define (fib n)
                ((lambda (loop)
                    (loop loop 0 1 n))
                 (lambda (self a b count)
                    (if (= count 0)
                        a
                        (self self b (+ a b) (- count 1))))))
            (fib 20))
    "#);
    assert_eq!(result, Ok(Value::Number(6765)));
}
