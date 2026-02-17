use grift::{ArenaError, Lisp, Value};

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
    assert_eq!(lisp.eval("(= 3 3)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(= 3 4)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_less_than() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(< 1 2)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(< 2 1)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_greater_than() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(> 2 1)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(> 1 2)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_less_equal() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(<= 1 2)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(<= 2 2)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(<= 3 2)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_greater_equal() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(>= 2 1)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(>= 2 2)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(>= 1 2)"), Ok(Value::Boolean(false)));
}

// ============================================================================
// Boolean & Special Form Tests
// ============================================================================

#[test]
fn test_booleans() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("#t"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("#f"), Ok(Value::Boolean(false)));
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
    assert_eq!(lisp.eval("(and #t #t #t)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(and #t #f #t)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(and #t #t)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(and)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(and 1 2 3)"), Err(ArenaError::TypeError));
    assert_eq!(lisp.eval("(and 1 #f 3)"), Err(ArenaError::TypeError));
}

#[test]
fn test_or() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(or #f #f #t)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(or #f #f)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(or #t #f)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(or)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(or #f #f 3)"), Err(ArenaError::TypeError));
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
    assert_eq!(lisp.eval("(null? '())"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(null? 1)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_not() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(not #f)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(not #t)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_pair_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(pair? (cons 1 2))"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(pair? 1)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_number_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(number? 42)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(number? #t)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_boolean_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(boolean? #t)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(boolean? 1)"), Ok(Value::Boolean(false)));
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

    assert!(
        gc.collected > 0,
        "GC should collect garbage from repeated evals"
    );
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
    assert!(
        gc.collected > 0,
        "GC should collect after evaluators are dropped"
    );
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
// Call-by-Value Strictness Tests
// ============================================================================

#[test]
fn test_strict_unused_arg_evaluated() {
    // In call-by-value, all arguments are evaluated even if unused.
    // The second argument is a type error that will crash.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        ((lambda (x y) x) 1 (+ 1 "crash"))
    "#,
    );
    assert!(
        result.is_err(),
        "Strict evaluation should evaluate all args"
    );
}

#[test]
fn test_if_unused_branch_not_evaluated() {
    // The false branch contains a type error; it must not be evaluated.
    // (if still only evaluates the taken branch)
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(r#"(if #t 42 (+ 1 "crash"))"#),
        Ok(Value::Number(42))
    );
}

#[test]
fn test_strict_define_evaluated() {
    // Define now evaluates immediately, so a type error is caught.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! bad (+ 1 "crash"))
            42)
    "#,
    );
    assert!(
        result.is_err(),
        "Strict define should evaluate RHS immediately"
    );
}

#[test]
fn test_strict_let_evaluated() {
    // Let binding now evaluates immediately, so a type error is caught.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (let ((x (+ 1 "crash")))
            42)
    "#,
    );
    assert!(
        result.is_err(),
        "Strict let should evaluate bindings immediately"
    );
}

// ============================================================================
// Memoization Tests (At-Most-Once)
// ============================================================================

#[test]
fn test_memoization_double() {
    // (double x) uses x twice. With memoization, (+ 1 2) is evaluated once.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        ((lambda (double)
            (double (+ 1 2)))
         (lambda (x) (+ x x)))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(6)));
}

#[test]
fn test_memoization_let_reuse() {
    // x is used twice in let body. It should be evaluated at most once.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (let ((x (+ 10 20)))
            (+ x x))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(60)));
}

#[test]
fn test_answers_through_let_bindings() {
    // Answers can be wrapped in pending bindings (indirections).
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (let ((x 1))
            (let ((y 2))
                (+ x y)))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(3)));
}

// ============================================================================
// Tail-Call Optimization Tests
// ============================================================================

#[test]
fn test_tco_countdown() {
    // Deep recursion that must not overflow the Rust stack.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! (count n)
                (if (= n 0) 0 (count (- n 1))))
            (count 1000))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(0)));
}

#[test]
fn test_tco_mutual_recursion() {
    // Mutual recursion in tail position.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! (my-even? n)
                (if (= n 0) #t (my-odd? (- n 1))))
            (define! (my-odd? n)
                (if (= n 0) #f (my-even? (- n 1))))
            (my-even? 1000))
    "#,
    );
    assert_eq!(result, Ok(Value::Boolean(true)));
}

#[test]
fn test_tco_begin_tail_position() {
    // The last expression in begin is a tail position.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! (loop n)
                (if (= n 0) 42
                    (begin
                        (+ 1 2)
                        (loop (- n 1)))))
            (loop 1000))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(42)));
}

// ============================================================================
// Purity Tests (no set!, define shadows)
// ============================================================================

#[test]
fn test_set_bang_is_rejected() {
    // set! has been removed; using it should fail (unbound variable).
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! x 1)
            (set! x 2)
            x)
    "#,
    );
    assert!(result.is_err(), "set! should not be recognized");
}

#[test]
fn test_define_shadows_not_mutates() {
    // Redefining a name at the top level creates a new shadow binding.
    // A lambda parameter binding is independent from a later global define.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! x 1)
            (define! get-x (lambda (x) x))
            (define! x 2)
            (get-x 1))
    "#,
    );
    // The lambda receives x=1 as a parameter, so global redefinition doesn't affect it.
    assert_eq!(result, Ok(Value::Number(1)));
}

#[test]
fn test_define_redefinition_returns_new_value() {
    // After redefining, a direct reference to x yields the latest binding.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! x 1)
            (define! x 2)
            x)
    "#,
    );
    assert_eq!(result, Ok(Value::Number(2)));
}

// ============================================================================
// Laziness + TCO Combined Tests
// ============================================================================

#[test]
fn test_tco_with_lazy_accumulator() {
    // Tail-recursive sum with lazy arguments.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! (sum-to n acc)
                (if (= n 0) acc (sum-to (- n 1) (+ acc n))))
            (sum-to 100 0))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(5050)));
}

#[test]
fn test_tco_iterative_fib() {
    // Iterative fib via self-application with define.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! (fib n)
                ((lambda (loop)
                    (loop loop 0 1 n))
                 (lambda (self a b count)
                    (if (= count 0)
                        a
                        (self self b (+ a b) (- count 1))))))
            (fib 20))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(6765)));
}

#[test]
fn test_recursive_fib_30() {
    // Naive recursive fib(30) — previously crashed with OOM.
    // GC during evaluation reclaims intermediate values, allowing completion.
    let lisp: Lisp<100_000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! (fib n)
                (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))
            (fib 30))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(832040)));
}

// ============================================================================
// Vau / Fexpr / First-Class Operative Tests
// ============================================================================

#[test]
fn test_vau_basic_quote() {
    // vau receives unevaluated args: (vau (x) #ignore x) acts like quote.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-quote (vau (x) #ignore x))
            (my-quote 42))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(42)));
}

#[test]
fn test_vau_receives_unevaluated_args() {
    // The vau body can inspect unevaluated args. Here we quote a symbol.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-quote (vau (x) #ignore x))
            (pair? (my-quote (1 2 3))))
    "#,
    );
    assert_eq!(result, Ok(Value::Boolean(true)));
}

#[test]
fn test_vau_with_env_param() {
    // vau captures the caller's environment via env-param and can eval in it.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-eval-add
                (vau (a b) e
                    (+ (eval a e) (eval b e))))
            (my-eval-add (+ 1 2) (+ 3 4)))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(10)));
}

#[test]
fn test_vau_derive_lambda() {
    // Derive a simple applicative from vau: evaluates a single argument
    // in the caller's environment before using it.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-inc
                (vau (x) e (+ 1 (eval x e))))
            (my-inc (+ 2 3)))
    "#,
    );
    // (+ 2 3) is evaluated in caller env → 5, then (+ 1 5) → 6
    assert_eq!(result, Ok(Value::Number(6)));
}

#[test]
fn test_lambda_as_syntactic_sugar_over_vau() {
    // lambda is syntactic sugar: wrap(vau(params, #ignore, body, env))
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! double (lambda (x) (+ x x)))
            (double 21))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(42)));
}

#[test]
fn test_vau_closure() {
    // vau closes over its definition environment.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! make-adder
                (lambda (n)
                    (vau (x) e (+ n (eval x e)))))
            (define! add5 (make-adder 5))
            (add5 (+ 1 2)))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(8)));
}

#[test]
fn test_first_class_if() {
    // `if` is a first-class operative that can be passed as an argument.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-if if)
            (my-if #t 1 2))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(1)));
}

#[test]
fn test_first_class_quote() {
    // `quote` is a first-class operative.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-quote quote)
            (my-quote 42))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(42)));
}

#[test]
fn test_first_class_begin() {
    // `begin` is a first-class operative.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-begin begin)
            (my-begin 1 2 3))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(3)));
}

#[test]
fn test_first_class_define() {
    // `define!` is a first-class operative.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-define define!)
            (my-define x 42)
            x)
    "#,
    );
    assert_eq!(result, Ok(Value::Number(42)));
}

#[test]
fn test_first_class_and() {
    // `and` is a first-class operative.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-and and)
            (my-and #t #t))
    "#,
    );
    assert_eq!(result, Ok(Value::Boolean(true)));
}

#[test]
fn test_first_class_or() {
    // `or` is a first-class operative.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-or or)
            (my-or #f #t))
    "#,
    );
    assert_eq!(result, Ok(Value::Boolean(true)));
}

#[test]
fn test_first_class_plus() {
    // `+` is a first-class operative stored in the environment.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-add +)
            (my-add 1 2))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(3)));
}

#[test]
fn test_first_class_cons() {
    // `cons` is a first-class operative.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-cons cons)
            (car (my-cons 1 2)))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(1)));
}

#[test]
fn test_eval_builtin() {
    // `eval` can evaluate expressions.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (eval '(+ 1 2))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(3)));
}

#[test]
fn test_vau_if_alternative() {
    // Define a custom if using vau.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-if
                (vau (test then else) e
                    (if (eval test e)
                        (eval then e)
                        (eval else e))))
            (my-if (= 1 1) (+ 10 20) (+ 30 40)))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(30)));
}

#[test]
fn test_vau_short_circuit() {
    // vau-defined if should not evaluate the unused branch.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-if
                (vau (test then else) e
                    (if (eval test e)
                        (eval then e)
                        (eval else e))))
            (my-if #t 42 (+ 1 "crash")))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(42)));
}

#[test]
fn test_operative_is_value() {
    // Operatives (builtins) are values that can be bound and looked up.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! f +)
            (f 3 4))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(7)));
}

#[test]
fn test_vau_rest_params() {
    // vau with a rest parameter binds all unevaluated args.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! count-args
                (vau args #ignore
                    (car args)))
            (count-args 10 20 30))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(10)));
}

#[test]
fn test_vau_gc_stress() {
    // Create and call many vau operatives to test GC.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! make-op
                (lambda (n)
                    (vau (x) e (+ n (eval x e)))))
            (define! op1 (make-op 1))
            (define! op2 (make-op 2))
            (define! op3 (make-op 3))
            (+ (op1 10) (op2 20) (op3 30)))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(66)));
}

// ============================================================================
// Kernel Semantics Tests (wrap/unwrap, operative?/applicative?)
// ============================================================================

#[test]
fn test_vau_creates_operative() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(operative? (vau (x) e x))"),
        Ok(Value::Boolean(true))
    );
    assert_eq!(
        lisp.eval("(applicative? (vau (x) e x))"),
        Ok(Value::Boolean(false))
    );
}

#[test]
fn test_wrap_creates_applicative() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(applicative? (wrap (vau (x) #ignore x)))"),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_unwrap_retrieves_operative() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(operative? (unwrap (wrap (vau (x) #ignore x))))"),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_lambda_is_applicative() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(applicative? (lambda (x) x))"),
        Ok(Value::Boolean(true))
    );
    assert_eq!(
        lisp.eval("(operative? (unwrap (lambda (x) x)))"),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_plus_is_applicative() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(applicative? +)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(operative? +)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_unwrap_plus() {
    // Unwrapping + gives the underlying operative (Builtin).
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(operative? (unwrap +))"),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_if_is_operative() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(applicative? if)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(operative? if)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_wrap_unwrap_roundtrip() {
    // wrap/unwrap round-trip on user operatives.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-op (vau (x) e (eval x e)))
            (define! my-app (wrap my-op))
            (my-app (+ 1 2)))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(3)));
}

#[test]
fn test_user_defined_unless() {
    // User-defined special form: unless
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! unless
                (vau (test . body) caller-env
                    (eval (list if test (list) (cons begin body))
                          caller-env)))
            (unless #f 1 2 3))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(3)));
}

#[test]
fn test_user_defined_unless_true() {
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! unless
                (vau (test . body) caller-env
                    (eval (list if test (list) (cons begin body))
                          caller-env)))
            (unless #t 1 2 3))
    "#,
    );
    assert_eq!(result, Ok(Value::Nil));
}

#[test]
fn test_user_defined_when() {
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! when
                (vau (test . body) caller-env
                    (eval (list if test (cons begin body) (list))
                          caller-env)))
            (when #t (+ 1 2)))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(3)));
}

// ============================================================================
// vau Syntax and Vau Calculus Examples
// ============================================================================

#[test]
fn test_dollar_vau_syntax() {
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-quote (vau (x) #ignore x))
            (my-quote 42))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(42)));
}

#[test]
fn test_dollar_vau_user_defined_unless() {
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! unless
                (vau (test . body) e
                    (eval (list if test (list) (cons begin body)) e)))
            (unless #f (+ 1 2)))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(3)));
}

#[test]
fn test_operative_predicate_on_if() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(operative? if)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_applicative_predicate_on_plus() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(applicative? +)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_operative_predicate_on_unwrap_plus() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(operative? (unwrap +))"),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_applicative_predicate_on_wrap_if() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(applicative? (wrap if))"),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_derive_lambda_from_dollar_vau() {
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-lambda
                (vau (params . body) static-env
                    (wrap (eval (list vau params #ignore (cons begin body)) static-env))))
            ((my-lambda (x) (+ x 1)) 10))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(11)));
}

#[test]
fn test_dollar_vau_with_env_param() {
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (begin
            (define! my-eval-add
                (vau (a b) e
                    (+ (eval a e) (eval b e))))
            (my-eval-add (+ 1 2) (+ 3 4)))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(10)));
}

// ============================================================================
// First-Class Environment Tests
// ============================================================================

#[test]
fn test_environment_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(environment? (make-environment))"),
        Ok(Value::Boolean(true))
    );
    assert_eq!(
        lisp.eval("(environment? 42)"),
        Ok(Value::Boolean(false))
    );
    assert_eq!(
        lisp.eval("(environment? #t)"),
        Ok(Value::Boolean(false))
    );
}

#[test]
fn test_make_environment_no_parent() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(environment? (make-environment))"),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_define_bang_mutates_environment() {
    // define! mutates the current environment in place
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! x 1)
                x)
            "#
        ),
        Ok(Value::Number(1))
    );
}

#[test]
fn test_define_bang_redefinition() {
    // Redefining with define! updates the same environment
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! x 1)
                (define! x 2)
                x)
            "#
        ),
        Ok(Value::Number(2))
    );
}

#[test]
fn test_let_creates_child_scope() {
    // let creates a child scope; define! inside let is local
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (let ((x 10))
                (define! y 20)
                (+ x y))
            "#
        ),
        Ok(Value::Number(30))
    );
}

#[test]
fn test_lexical_scope_preserved() {
    // Closures see their definition environment, not the call site
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! (make-adder n)
                    (lambda (x) (+ x n)))
                (define! add5 (make-adder 5))
                (define! n 999)
                (add5 10))
            "#
        ),
        Ok(Value::Number(15))
    );
}

#[test]
fn test_mutual_recursion_through_shared_env() {
    // Mutual recursion through shared environment
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! (even? n) (if (= n 0) #t (odd? (- n 1))))
                (define! (odd? n) (if (= n 0) #f (even? (- n 1))))
                (even? 10))
            "#
        ),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_vau_captures_caller_env_as_environment() {
    // Vau's env-param captures the caller's environment as a first-class Environment value
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! get-caller-env
                    (vau () caller-env caller-env))
                (environment? (get-caller-env)))
            "#
        ),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_eval_in_custom_environment() {
    // Create a fresh environment and eval in it
    let lisp: Lisp<20000> = Lisp::new();
    // Note: we pass current env as parent so builtins are accessible
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! get-env (vau () e e))
                (define! e (make-environment (get-env)))
                (eval (list define! (quote x) 42) e)
                (eval (quote x) e))
            "#
        ),
        Ok(Value::Number(42))
    );
}

#[test]
fn test_lambda_higher_order() {
    // Higher-order function: compose
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! (compose f g) (lambda (x) (f (g x))))
                ((compose (lambda (x) (+ x 1)) (lambda (x) (* x 2))) 5))
            "#
        ),
        Ok(Value::Number(11))
    );
}

#[test]
fn test_tco_with_define_bang() {
    // TCO works with define!
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! (countdown n)
                    (if (= n 0) 0 (countdown (- n 1))))
                (countdown 100000))
            "#
        ),
        Ok(Value::Number(0))
    );
}

#[test]
fn test_lambda_square() {
    // Lambda application
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("((lambda (x) (* x x)) 5)"),
        Ok(Value::Number(25))
    );
}

#[test]
fn test_existing_functionality_preserved() {
    // Arithmetic, comparisons, lists
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(+ 1 2 3)"), Ok(Value::Number(6)));
    assert_eq!(lisp.eval("(< 1 2)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(car (list 1 2 3))"), Ok(Value::Number(1)));
    assert_eq!(lisp.eval("(null? (list))"), Ok(Value::Boolean(true)));
}

#[test]
fn test_vau_custom_if() {
    // Custom if using vau/operatives
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! my-if
                    (vau (test then else) e
                        (eval (if (eval test e) then else) e)))
                (my-if #t 1 2))
            "#
        ),
        Ok(Value::Number(1))
    );
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! my-if
                    (vau (test then else) e
                        (eval (if (eval test e) then else) e)))
                (my-if #f 1 2))
            "#
        ),
        Ok(Value::Number(2))
    );
}

#[test]
fn test_make_environment_with_parent() {
    // Child of current env can access parent bindings via eval
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! get-env (vau () e e))
                (define! child (make-environment (get-env)))
                (eval (list define! (quote local-var) 99) child)
                (eval (quote local-var) child))
            "#
        ),
        Ok(Value::Number(99))
    );
}

#[test]
fn test_child_env_inherits_from_parent() {
    // A child env can look up bindings from its parent
    let lisp: Lisp<20000> = Lisp::new();
    // Verify + is accessible through parent chain (just check no error)
    assert!(
        lisp.eval(
            r#"
            (begin
                (define! get-env (vau () e e))
                (define! child (make-environment (get-env)))
                (eval (quote +) child))
            "#
        )
        .is_ok()
    );
    // Verify child can use parent builtins
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! get-env (vau () e e))
                (define! child (make-environment (get-env)))
                (eval '(+ 1 2) child))
            "#
        ),
        Ok(Value::Number(3))
    );
}

// ============================================================================
// Sandboxed / Restricted Environment Tests
// ============================================================================

#[test]
fn test_sandboxed_eval_no_access_to_builtins() {
    // Sandboxed eval — no access to define!, vau, eval, or anything else
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! sandbox (make-empty-environment))
                (eval (quote (+ 1 2)) sandbox))
            "#
        ),
        Err(ArenaError::UnboundVariable)
    );
}

#[test]
fn test_selective_exposure_arithmetic_only() {
    // Selective exposure — only arithmetic, no metaprogramming
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! safe-env (make-empty-environment))
                (eval (list define! (quote +) +) safe-env)
                (eval (list define! (quote -) -) safe-env)
                (eval (quote (+ 1 2)) safe-env))
            "#
        ),
        Ok(Value::Number(3))
    );
}

#[test]
fn test_selective_exposure_vau_is_unbound() {
    // vau should be unbound in the selective environment
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! safe-env (make-empty-environment))
                (eval (list define! (quote +) +) safe-env)
                (eval (list define! (quote -) -) safe-env)
                (eval (quote (vau (x) e x)) safe-env))
            "#
        ),
        Err(ArenaError::UnboundVariable)
    );
}

#[test]
fn test_make_empty_environment_is_truly_empty() {
    // make-empty-environment creates a truly empty scope
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! e (make-empty-environment))
                (environment? e))
            "#
        ),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_make_environment_with_current_env_has_builtins() {
    // Users who want builtins available write (make-environment (current-environment))
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! get-env (vau () e e))
                (define! child (make-environment (get-env)))
                (eval (quote (+ 1 2)) child))
            "#
        ),
        Ok(Value::Number(3))
    );
}

#[test]
fn test_no_global_fallback_in_eval() {
    // Without parent chain to global, symbols in global are unreachable
    let lisp: Lisp<20000> = Lisp::new();
    // make-environment with no args
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! isolated (make-environment))
                (eval (quote +) isolated))
            "#
        ),
        Err(ArenaError::UnboundVariable)
    );
    // make-empty-environment
    assert_eq!(
        lisp.eval(
            r#"
            (begin
                (define! isolated (make-empty-environment))
                (eval (quote +) isolated))
            "#
        ),
        Err(ArenaError::UnboundVariable)
    );
}
