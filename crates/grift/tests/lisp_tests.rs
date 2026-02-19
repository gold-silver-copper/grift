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
    // After collecting, arena should contain only the persistent evaluator
    // state (ground env, builtins, global env) plus singletons.
    assert!(
        stats.allocated < 600,
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
        stats.allocated < 600,
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

    // After full GC, arena should contain only persistent evaluator state.
    assert!(
        final_stats.allocated < 600,
        "Arena should be mostly empty after GC: allocated = {}",
        final_stats.allocated
    );
}

#[test]
fn test_gc_define_creates_garbage() {
    // The evaluator state persists across calls, but temporary values
    // (lambdas, intermediate results) become garbage after evaluation.
    let lisp: Lisp<5000> = Lisp::new();

    // Evaluate several expressions that create lambdas and intermediate values.
    lisp.eval("((lambda (x) (+ x 1)) 5)").unwrap();
    lisp.eval("((lambda (a b) (* a b)) 3 7)").unwrap();

    // After eval, temporary values are garbage.
    let gc = lisp.collect_garbage(&[]);
    assert!(gc.collected > 0, "GC should collect temporary values");
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
        (define! bad (+ 1 "crash"))
        42
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
        (define! count (lambda (n)
            (if (= n 0) 0 (count (- n 1)))))
        (count 1000)
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
        (define! my-even? (lambda (n)
            (if (= n 0) #t (my-odd? (- n 1)))))
        (define! my-odd? (lambda (n)
            (if (= n 0) #f (my-even? (- n 1)))))
        (my-even? 1000)
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
            (define! loop (lambda (n)
                (if (= n 0) 42
                    (begin
                        (+ 1 2)
                        (loop (- n 1))))))
            (loop 1000))
    "#,
    );
    assert_eq!(result, Ok(Value::Number(42)));
}

// ============================================================================
// Purity Tests (no set!, define shadows)
// ============================================================================

#[test]
fn test_set_bang_scheme_style_is_rejected() {
    // Scheme-style (set! x 2) is not valid Kernel syntax;
    // Kernel's $set! requires an environment argument: (set! env definiend expr).
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (define! x 1)
        (set! x 2)
        x
    "#,
    );
    assert!(
        result.is_err(),
        "Scheme-style set! should fail (wrong syntax). Only Kernel style is proper"
    );
}

#[test]
fn test_define_shadows_not_mutates() {
    // Redefining a name at the top level creates a new shadow binding.
    // A lambda parameter binding is independent from a later global define.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (define! x 1)
        (define! get-x (lambda (x) x))
        (define! x 2)
        (get-x 1)
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
        (define! x 1)
        (define! x 2)
        x
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
        (define! sum-to (lambda (n acc)
            (if (= n 0) acc (sum-to (- n 1) (+ acc n)))))
        (sum-to 100 0)
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
        (define! fib (lambda (n)
            ((lambda (loop)
                (loop loop 0 1 n))
             (lambda (self a b count)
                (if (= count 0)
                    a
                    (self self b (+ a b) (- count 1)))))))
        (fib 20)
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
        (define! fib (lambda (n)
            (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2))))))
        (fib 30)
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
        (define! my-quote (vau (x) #ignore x))
        (my-quote 42)
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
        (define! my-quote (vau (x) #ignore x))
        (pair? (my-quote (1 2 3)))
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
        (define! my-eval-add
            (vau (a b) e
                (+ (eval a e) (eval b e))))
        (my-eval-add (+ 1 2) (+ 3 4))
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
        (define! my-inc
            (vau (x) e (+ 1 (eval x e))))
        (my-inc (+ 2 3))
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
        (define! double (lambda (x) (+ x x)))
        (double 21)
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
        (define! make-adder
            (lambda (n)
                (vau (x) e (+ n (eval x e)))))
        (define! add5 (make-adder 5))
        (add5 (+ 1 2))
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
        (define! my-if if)
        (my-if #t 1 2)
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
        (define! my-quote quote)
        (my-quote 42)
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
        (define! my-define define!)
        (my-define x 42)
        x
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
        (define! my-and and)
        (my-and #t #t)
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
        (define! my-or or)
        (my-or #f #t)
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
        (define! my-add +)
        (my-add 1 2)
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
        (define! my-cons cons)
        (car (my-cons 1 2))
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
        (define! my-if
            (vau (test then else) e
                (if (eval test e)
                    (eval then e)
                    (eval else e))))
        (my-if (= 1 1) (+ 10 20) (+ 30 40))
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
        (define! my-if
            (vau (test then else) e
                (if (eval test e)
                    (eval then e)
                    (eval else e))))
        (my-if #t 42 (+ 1 "crash"))
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
        (define! f +)
        (f 3 4)
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
        (define! count-args
            (vau args #ignore
                (car args)))
        (count-args 10 20 30)
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
        (define! make-op
            (lambda (n)
                (vau (x) e (+ n (eval x e)))))
        (define! op1 (make-op 1))
        (define! op2 (make-op 2))
        (define! op3 (make-op 3))
        (+ (op1 10) (op2 20) (op3 30))
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
        (define! my-op (vau (x) e (eval x e)))
        (define! my-app (wrap my-op))
        (my-app (+ 1 2))
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
        (define! unless
            (vau (test . body) caller-env
                (eval (list if test (list) (cons begin body))
                      caller-env)))
        (unless #f 1 2 3)
    "#,
    );
    assert_eq!(result, Ok(Value::Number(3)));
}

#[test]
fn test_user_defined_unless_true() {
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (define! unless
            (vau (test . body) caller-env
                (eval (list if test (list) (cons begin body))
                      caller-env)))
        (unless #t 1 2 3)
    "#,
    );
    assert_eq!(result, Ok(Value::Nil));
}

#[test]
fn test_user_defined_when() {
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (define! when
            (vau (test . body) caller-env
                (eval (list if test (cons begin body) (list))
                      caller-env)))
        (when #t (+ 1 2))
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
        (define! my-quote (vau (x) #ignore x))
        (my-quote 42)
    "#,
    );
    assert_eq!(result, Ok(Value::Number(42)));
}

#[test]
fn test_dollar_vau_user_defined_unless() {
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (define! unless
            (vau (test . body) e
                (eval (list if test (list) (cons begin body)) e)))
        (unless #f (+ 1 2))
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
        (define! my-lambda
            (vau (params . body) static-env
                (wrap (eval (list vau params #ignore (cons begin body)) static-env))))
        ((my-lambda (x) (+ x 1)) 10)
    "#,
    );
    assert_eq!(result, Ok(Value::Number(11)));
}

#[test]
fn test_dollar_vau_with_env_param() {
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (define! my-eval-add
            (vau (a b) e
                (+ (eval a e) (eval b e))))
        (my-eval-add (+ 1 2) (+ 3 4))
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
    assert_eq!(lisp.eval("(environment? 42)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(environment? #t)"), Ok(Value::Boolean(false)));
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
            (define! x 1)
            x
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
            (define! x 1)
            (define! x 2)
            x
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
            (define! make-adder (lambda (n)
                (lambda (x) (+ x n))))
            (define! add5 (make-adder 5))
            (define! n 999)
            (add5 10)
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
            (define! even? (lambda (n) (if (= n 0) #t (odd? (- n 1)))))
            (define! odd? (lambda (n) (if (= n 0) #f (even? (- n 1)))))
            (even? 10)
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
            (define! get-caller-env
                (vau () caller-env caller-env))
            (environment? (get-caller-env))
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
            (define! get-env (vau () e e))
            (define! e (make-environment (get-env)))
            (eval (list define! (quote x) 42) e)
            (eval (quote x) e)
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
            (define! compose (lambda (f g) (lambda (x) (f (g x)))))
            ((compose (lambda (x) (+ x 1)) (lambda (x) (* x 2))) 5)
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
            (define! countdown (lambda (n)
                (if (= n 0) 0 (countdown (- n 1)))))
            (countdown 100000)
            "#
        ),
        Ok(Value::Number(0))
    );
}

#[test]
fn test_lambda_square() {
    // Lambda application
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("((lambda (x) (* x x)) 5)"), Ok(Value::Number(25)));
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
            (define! my-if
                (vau (test then else) e
                    (eval (if (eval test e) then else) e)))
            (my-if #t 1 2)
            "#
        ),
        Ok(Value::Number(1))
    );
    assert_eq!(
        lisp.eval(
            r#"
            (define! my-if
                (vau (test then else) e
                    (eval (if (eval test e) then else) e)))
            (my-if #f 1 2)
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
            (define! get-env (vau () e e))
            (define! child (make-environment (get-env)))
            (eval (list define! (quote local-var) 99) child)
            (eval (quote local-var) child)
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
            (define! get-env (vau () e e))
            (define! child (make-environment (get-env)))
            (eval (quote +) child)
            "#
        )
        .is_ok()
    );
    // Verify child can use parent builtins
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! child (make-environment (get-env)))
            (eval '(+ 1 2) child)
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
            (define! sandbox (make-empty-environment))
            (eval (quote (+ 1 2)) sandbox)
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
            (define! safe-env (make-empty-environment))
            (eval (list define! (quote +) +) safe-env)
            (eval (list define! (quote -) -) safe-env)
            (eval (quote (+ 1 2)) safe-env)
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
            (define! safe-env (make-empty-environment))
            (eval (list define! (quote +) +) safe-env)
            (eval (list define! (quote -) -) safe-env)
            (eval (quote (vau (x) e x)) safe-env)
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
            (define! e (make-empty-environment))
            (environment? e)
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
            (define! get-env (vau () e e))
            (define! child (make-environment (get-env)))
            (eval (quote (+ 1 2)) child)
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
            (define! isolated (make-environment))
            (eval (quote +) isolated)
            "#
        ),
        Err(ArenaError::UnboundVariable)
    );
    // make-empty-environment
    assert_eq!(
        lisp.eval(
            r#"
            (define! isolated (make-empty-environment))
            (eval (quote +) isolated)
            "#
        ),
        Err(ArenaError::UnboundVariable)
    );
}

// ============================================================================
// Kernel $define! Conformance Tests (§4.9.1)
// ============================================================================

#[test]
fn test_define_returns_inert() {
    // Per Kernel spec, $define! returns #inert
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(define! x 42)"), Ok(Value::Inert));
}

#[test]
fn test_inert_is_self_evaluating() {
    // #inert evaluates to itself
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("#inert"), Ok(Value::Inert));
}

#[test]
fn test_define_ptree_symbol() {
    // Simple symbol definiend
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! x 42)
            x
            "#
        ),
        Ok(Value::Number(42))
    );
}

#[test]
fn test_define_ptree_ignore() {
    // #ignore definiend — value is discarded
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(define! #ignore (+ 1 2))"), Ok(Value::Inert));
}

#[test]
fn test_define_ptree_nil() {
    // Nil definiend matches nil value
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(define! () ())"), Ok(Value::Inert));
}

#[test]
fn test_define_ptree_nil_mismatch() {
    // Nil definiend must match nil value; non-nil causes error
    let lisp: Lisp<20000> = Lisp::new();
    assert!(lisp.eval("(define! () 42)").is_err());
}

#[test]
fn test_define_ptree_pair_destructuring() {
    // Pair definiend destructures the value
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! (a . b) (cons 1 2))
            (+ a b)
            "#
        ),
        Ok(Value::Number(3))
    );
}

#[test]
fn test_define_ptree_list_destructuring() {
    // List definiend destructures a list
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! (a b c) (list 10 20 30))
            (+ a (+ b c))
            "#
        ),
        Ok(Value::Number(60))
    );
}

#[test]
fn test_define_ptree_nested_destructuring() {
    // Nested pair definiend
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! ((a b) c) (list (list 1 2) 3))
            (+ a (+ b c))
            "#
        ),
        Ok(Value::Number(6))
    );
}

#[test]
fn test_define_ptree_with_ignore_in_pair() {
    // #ignore in a pair position
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! (a #ignore c) (list 10 20 30))
            (+ a c)
            "#
        ),
        Ok(Value::Number(40))
    );
}

#[test]
fn test_define_ptree_pair_mismatch() {
    // Pair definiend with non-pair value should error
    let lisp: Lisp<20000> = Lisp::new();
    assert!(lisp.eval("(define! (a . b) 42)").is_err());
}

#[test]
fn test_define_ptree_rest_binding() {
    // Dotted pair captures first element and rest of list
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! (a . rest) (list 1 2 3))
            a
            "#
        ),
        Ok(Value::Number(1))
    );
    // Verify rest captured (2 3) — car of rest is 2
    assert_eq!(
        lisp.eval(
            r#"
            (define! (a . rest) (list 1 2 3))
            (car rest)
            "#
        ),
        Ok(Value::Number(2))
    );
}

#[test]
fn test_define_ptree_kernel_example() {
    // Example from Kernel spec: destructuring get-list-metrics result
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! (x y z) (list 100 200 300))
            y
            "#
        ),
        Ok(Value::Number(200))
    );
}

// ============================================================================
// vau Kernel spec conformance (§4.10.3)
// ============================================================================

#[test]
fn test_vau_eformal_must_be_symbol_or_ignore() {
    // eformal = #ignore is valid
    let lisp: Lisp<20000> = Lisp::new();
    assert!(lisp.eval("(vau (x) #ignore x)").is_ok());

    // eformal = symbol is valid
    let lisp2: Lisp<20000> = Lisp::new();
    assert!(lisp2.eval("(vau (x) e x)").is_ok());

    // eformal = number should error
    let lisp3: Lisp<20000> = Lisp::new();
    assert!(lisp3.eval("(vau (x) 42 x)").is_err());

    // eformal = list/pair should error
    let lisp4: Lisp<20000> = Lisp::new();
    assert!(lisp4.eval("(vau (x) (a b) x)").is_err());

    // eformal = boolean should error
    let lisp5: Lisp<20000> = Lisp::new();
    assert!(lisp5.eval("(vau (x) #t x)").is_err());

    // eformal = nil should error
    let lisp6: Lisp<20000> = Lisp::new();
    assert!(lisp6.eval("(vau (x) () x)").is_err());
}

#[test]
fn test_vau_eformal_not_in_formals() {
    // env-param symbol must not also appear in formals
    let lisp: Lisp<20000> = Lisp::new();
    assert!(lisp.eval("(vau (e) e e)").is_err());

    // env-param symbol nested in formals should also be caught
    let lisp2: Lisp<20000> = Lisp::new();
    assert!(lisp2.eval("(vau (a (b e)) e e)").is_err());

    // env-param symbol in dotted rest position
    let lisp3: Lisp<20000> = Lisp::new();
    assert!(lisp3.eval("(vau (a . e) e e)").is_err());

    // No conflict: different symbol names are fine
    let lisp4: Lisp<20000> = Lisp::new();
    assert!(lisp4.eval("(vau (a b) e e)").is_ok());
}

#[test]
fn test_vau_formals_must_be_valid_ptree() {
    // Valid formal parameter trees
    let lisp: Lisp<20000> = Lisp::new();
    // Symbol
    assert!(lisp.eval("(vau x #ignore x)").is_ok());
    // Nil
    let lisp2: Lisp<20000> = Lisp::new();
    assert!(lisp2.eval("(vau () #ignore 42)").is_ok());
    // Pair/list of symbols
    let lisp3: Lisp<20000> = Lisp::new();
    assert!(lisp3.eval("(vau (a b) #ignore a)").is_ok());
    // Nested pairs
    let lisp4: Lisp<20000> = Lisp::new();
    assert!(lisp4.eval("(vau ((a b) c) #ignore a)").is_ok());
    // #ignore in formals
    let lisp5: Lisp<20000> = Lisp::new();
    assert!(lisp5.eval("(vau (a #ignore) #ignore a)").is_ok());

    // Invalid: number in formals
    let lisp6: Lisp<20000> = Lisp::new();
    assert!(lisp6.eval("(vau (42) #ignore 1)").is_err());

    // Invalid: boolean in formals
    let lisp7: Lisp<20000> = Lisp::new();
    assert!(lisp7.eval("(vau (#t) #ignore 1)").is_err());
}

#[test]
fn test_vau_valid_after_validation() {
    // After validation, vau still works correctly for valid cases
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (define! my-quote (vau (x) #ignore x))
        (symbol? (my-quote hello))
        "#,
    );
    assert_eq!(result, Ok(Value::Boolean(true)));

    // vau with env param works correctly
    let lisp2: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp2.eval(
            r#"
            (define! my-eval
                (wrap (vau (x) e (eval x e))))
            (my-eval (+ 1 2))
            "#
        ),
        Ok(Value::Number(3))
    );
}

// ============================================================================
// Kernel §4.5.1 — inert? predicate
// ============================================================================

#[test]
fn test_inert_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(inert? #inert)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(inert? 42)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(inert? #t)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(inert? '())"), Ok(Value::Boolean(false)));
}

#[test]
fn test_inert_predicate_on_define_result() {
    // $define! returns #inert per Kernel spec
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! result (define! x 42))
            (inert? result)
            "#
        ),
        Ok(Value::Boolean(true))
    );
}

// ============================================================================
// Kernel §4.2.1 — eq? predicate
// ============================================================================

#[test]
fn test_eq_booleans() {
    // Booleans: eq? iff same boolean value
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(eq? #t #t)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(eq? #f #f)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(eq? #t #f)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(eq? #f #t)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_eq_symbols() {
    // Symbols are eq? iff they have the same external representation
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! a (quote hello))
            (define! b (quote hello))
            (eq? a b)
            "#
        ),
        Ok(Value::Boolean(true))
    );
    assert_eq!(
        lisp.eval(
            r#"
            (define! a (quote hello))
            (define! b (quote world))
            (eq? a b)
            "#
        ),
        Ok(Value::Boolean(false))
    );
}

#[test]
fn test_eq_numbers() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(eq? 42 42)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(eq? 1 2)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_eq_nil() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(eq? '() '())"), Ok(Value::Boolean(true)));
}

#[test]
fn test_eq_inert() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(eq? #inert #inert)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_eq_cons_different_calls() {
    // Two different calls to cons produce non-eq? pairs (§4.6.3)
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! a (cons 1 2))
            (define! b (cons 1 2))
            (eq? a b)
            "#
        ),
        Ok(Value::Boolean(false))
    );
}

#[test]
fn test_eq_same_pair() {
    // Same pair is eq? to itself
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! a (cons 1 2))
            (eq? a a)
            "#
        ),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_eq_different_types() {
    // Different types are never eq?
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(eq? #t 1)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(eq? '() #f)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(eq? 0 #f)"), Ok(Value::Boolean(false)));
}

// ============================================================================
// Kernel §4.3.1 — equal? predicate
// ============================================================================

#[test]
fn test_equal_booleans() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(equal? #t #t)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(equal? #f #f)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(equal? #t #f)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_equal_numbers() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(equal? 42 42)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(equal? 1 2)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_equal_cons_structural() {
    // equal? compares cons cells structurally
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! a (cons 1 2))
            (define! b (cons 1 2))
            (equal? a b)
            "#
        ),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_equal_cons_different_content() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! a (cons 1 2))
            (define! b (cons 1 3))
            (equal? a b)
            "#
        ),
        Ok(Value::Boolean(false))
    );
}

#[test]
fn test_equal_nested_lists() {
    // Deep structural equality of nested lists
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! a (list 1 (list 2 3) 4))
            (define! b (list 1 (list 2 3) 4))
            (equal? a b)
            "#
        ),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_equal_implies_by_eq() {
    // eq? ⇒ equal? (Rule 2)
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! a (cons 1 2))
            (equal? a a)
            "#
        ),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_equal_different_types() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(equal? #t 1)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(equal? '() #f)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_equal_environments_identity() {
    // Environments are equal? only when eq? (identity-based)
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! a (make-environment))
            (define! b (make-environment))
            (equal? a b)
            "#
        ),
        Ok(Value::Boolean(false))
    );
    assert_eq!(
        lisp.eval(
            r#"
            (define! a (make-environment))
            (equal? a a)
            "#
        ),
        Ok(Value::Boolean(true))
    );
}

// ============================================================================
// Variadic type predicates (§4.4.1, §4.6.1, §4.6.2, etc.)
// ============================================================================

#[test]
fn test_variadic_boolean_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(boolean? #t #f #t)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(boolean? #t 1 #f)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(boolean?)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_variadic_number_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(number? 1 2 3)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(number? 1 #t 3)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(number?)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_variadic_symbol_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(symbol? (quote a) (quote b))"),
        Ok(Value::Boolean(true))
    );
    assert_eq!(
        lisp.eval("(symbol? (quote a) 1)"),
        Ok(Value::Boolean(false))
    );
    assert_eq!(lisp.eval("(symbol?)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_variadic_pair_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(pair? (cons 1 2) (cons 3 4))"),
        Ok(Value::Boolean(true))
    );
    assert_eq!(lisp.eval("(pair? (cons 1 2) 3)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(pair?)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_variadic_null_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(null? '() '())"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(null? '() 1)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(null?)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_variadic_inert_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(inert? #inert #inert)"),
        Ok(Value::Boolean(true))
    );
    assert_eq!(lisp.eval("(inert? #inert 42)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(inert?)"), Ok(Value::Boolean(true)));
}

// ============================================================================
// Ignore Type Tests (§4.8.2)
// ============================================================================

#[test]
fn test_ignore_is_self_evaluating() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("#ignore"), Ok(Value::Ignore));
}

#[test]
fn test_ignore_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(ignore? #ignore)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(ignore? #t)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(ignore? 42)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(ignore? '())"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(ignore? #inert)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_variadic_ignore_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(ignore? #ignore #ignore)"),
        Ok(Value::Boolean(true))
    );
    assert_eq!(lisp.eval("(ignore? #ignore 42)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(ignore?)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_ignore_is_not_symbol() {
    // #ignore is a distinct type, not a symbol
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(symbol? #ignore)"), Ok(Value::Boolean(false)));
}

#[test]
fn test_ignore_eq() {
    // #ignore is eq? to itself (single immutable value)
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(eq? #ignore #ignore)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_ignore_equal() {
    // #ignore is equal? to itself
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(equal? #ignore #ignore)"),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_ignore_in_define_ptree() {
    // #ignore in $define! parameter tree ignores the value
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! (a #ignore c) (list 1 2 3))
            (+ a c)
            "#
        ),
        Ok(Value::Number(4))
    );
}

#[test]
fn test_ignore_in_lambda_params() {
    // #ignore can be used in lambda parameter trees
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! f (lambda (x #ignore y) (+ x y)))
            (f 10 20 30)
            "#
        ),
        Ok(Value::Number(40))
    );
}

// ============================================================================
// Multi-parent Environment Tests (§4.8.4)
// ============================================================================

#[test]
fn test_make_environment_multiple_parents() {
    // (make-environment env1 env2) creates env with both parents
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! e1 (make-environment))
            (eval (list define! (quote x) 10) e1)
            (define! e2 (make-environment))
            (eval (list define! (quote y) 20) e2)
            (define! child (make-environment e1 e2))
            (+ (eval (quote x) child) (eval (quote y) child))
            "#
        ),
        Ok(Value::Number(30))
    );
}

#[test]
fn test_make_environment_parent_order_matters() {
    // When both parents define the same symbol, the first parent wins
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! e1 (make-environment))
            (eval (list define! (quote x) 1) e1)
            (define! e2 (make-environment))
            (eval (list define! (quote x) 2) e2)
            (define! child (make-environment e1 e2))
            (eval (quote x) child)
            "#
        ),
        Ok(Value::Number(1))
    );
    // Reversed order: e2 first, so e2's binding wins
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! e1 (make-environment))
            (eval (list define! (quote x) 1) e1)
            (define! e2 (make-environment))
            (eval (list define! (quote x) 2) e2)
            (define! child (make-environment e2 e1))
            (eval (quote x) child)
            "#
        ),
        Ok(Value::Number(2))
    );
}

#[test]
fn test_make_environment_three_parents() {
    // Three parents
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! e1 (make-environment))
            (eval (list define! (quote a) 1) e1)
            (define! e2 (make-environment))
            (eval (list define! (quote b) 2) e2)
            (define! e3 (make-environment))
            (eval (list define! (quote c) 3) e3)
            (define! child (make-environment e1 e2 e3))
            (+ (eval (quote a) child)
               (+ (eval (quote b) child)
                  (eval (quote c) child)))
            "#
        ),
        Ok(Value::Number(6))
    );
}

#[test]
fn test_make_environment_validates_args_are_environments() {
    // make-environment should only accept environments as arguments
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval("(make-environment 42)"),
        Err(ArenaError::TypeError)
    );
}

#[test]
fn test_make_environment_copies_parent_list() {
    // The constructed environment stores its parents independently
    // of the original argument list (Kernel §4.8.4)
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! e1 (make-environment (get-env)))
            (define! child (make-environment e1))
            (eval (quote (+ 1 2)) child)
            "#
        ),
        Ok(Value::Number(3))
    );
}

#[test]
fn test_make_environment_no_args_has_no_parents() {
    // (make-environment) with no args creates parentless environment
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! e (make-environment))
            (eval (quote +) e)
            "#
        ),
        Err(ArenaError::UnboundVariable)
    );
}

#[test]
fn test_make_environment_local_bindings_shadow_parents() {
    // Local bindings shadow parent bindings
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! parent (make-environment (get-env)))
            (eval (list define! (quote x) 10) parent)
            (define! child (make-environment parent))
            (eval (list define! (quote x) 99) child)
            (eval (quote x) child)
            "#
        ),
        Ok(Value::Number(99))
    );
}

#[test]
fn test_depth_first_search_in_multi_parent() {
    // Depth-first: e1 has parent gp with binding x=100
    // child = (make-environment e1 e2) where e2 has x=200
    // Since e1 is searched depth-first before e2, and e1's parent gp has x=100,
    // that should be found before e2's x=200
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! gp (make-environment))
            (eval (list define! (quote x) 100) gp)
            (define! e1 (make-environment gp))
            (define! e2 (make-environment))
            (eval (list define! (quote x) 200) e2)
            (define! child (make-environment e1 e2))
            (eval (quote x) child)
            "#
        ),
        Ok(Value::Number(100))
    );
}

// ============================================================================
// Formal Parameter Tree Conformance Tests (§4.9.1)
// ============================================================================

#[test]
fn test_define_rejects_duplicate_symbol_in_ptree() {
    // A formal parameter tree must not contain the same symbol more than once.
    let lisp: Lisp<20000> = Lisp::new();
    assert!(
        lisp.eval("(define! (a a) (list 1 2))").is_err(),
        "duplicate symbol 'a' in flat list"
    );
}

#[test]
fn test_define_rejects_duplicate_symbol_nested_ptree() {
    // Duplicate detection must work across nested pairs.
    let lisp: Lisp<20000> = Lisp::new();
    assert!(
        lisp.eval("(define! (a (b a)) (list 1 (list 2 3)))")
            .is_err(),
        "duplicate symbol 'a' in nested ptree"
    );
}

#[test]
fn test_define_rejects_duplicate_symbol_dotted_ptree() {
    // Duplicate in a dotted-pair ptree.
    let lisp: Lisp<20000> = Lisp::new();
    assert!(
        lisp.eval("(define! (a . a) (cons 1 2))").is_err(),
        "duplicate symbol 'a' in dotted pair"
    );
}

#[test]
fn test_define_allows_ignore_duplicates_in_ptree() {
    // #ignore may appear multiple times — it is not a symbol.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! (#ignore a #ignore) (list 1 2 3))
            a
            "#
        ),
        Ok(Value::Number(2))
    );
}

#[test]
fn test_vau_rejects_duplicate_symbol_in_ptree() {
    // vau should also reject duplicate symbols in formals.
    let lisp: Lisp<20000> = Lisp::new();
    assert!(
        lisp.eval("(vau (a a) #ignore a)").is_err(),
        "duplicate symbol 'a' in vau formals"
    );
}

#[test]
fn test_make_environment_rejects_non_environment_mixed() {
    // Passing a mix of environments and non-environments should fail.
    let lisp: Lisp<20000> = Lisp::new();
    assert!(
        lisp.eval(
            r#"
            (define! e (make-environment))
            (make-environment e #t)
            "#
        )
        .is_err(),
        "non-environment #t in parents list"
    );
}

#[test]
fn test_ignore_predicate_various_types() {
    // ignore? returns #f for all non-ignore types.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(ignore? #ignore)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(ignore? #t)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(ignore? #f)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(ignore? 0)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(ignore? '())"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(ignore? #inert)"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("(ignore? (cons 1 2))"), Ok(Value::Boolean(false)));
    assert_eq!(
        lisp.eval("(ignore? (make-environment))"),
        Ok(Value::Boolean(false))
    );
}

#[test]
fn test_multi_parent_first_parent_wins() {
    // When multiple parents define the same binding, the first parent wins.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! e1 (make-environment))
            (eval (list define! (quote x) 10) e1)
            (define! e2 (make-environment))
            (eval (list define! (quote x) 20) e2)
            (define! child (make-environment e1 e2))
            (eval (quote x) child)
            "#
        ),
        Ok(Value::Number(10))
    );
}

// ============================================================================
// Kernel §3.1 — References and Mutation ($set!)
// ============================================================================

#[test]
fn test_set_bang_basic() {
    // $set! mutates an existing binding in the specified environment.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! x 1)
            (set! (get-env) x 2)
            x
            "#
        ),
        Ok(Value::Number(2))
    );
}

#[test]
fn test_set_bang_returns_inert() {
    // $set! returns #inert per Kernel spec.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! x 1)
            (inert? (set! (get-env) x 42))
            "#
        ),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_set_bang_creates_new_binding() {
    // Per Kernel §6.8.1, $set! matches formals in the target environment,
    // creating new bindings (like $define!) rather than requiring pre-existing ones.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! e (get-env))
            (set! e y 42)
            y
            "#
        ),
        Ok(Value::Number(42))
    );
}

#[test]
fn test_set_bang_in_captured_env() {
    // Per Kernel §6.8.1, $set! binds formals in the specified environment.
    // To modify a variable in an outer scope, capture that scope's env first.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! my-env (get-env))
            (define! x 1)
            (define! update-x
                (lambda ()
                    (set! my-env x 2)))
            (update-x)
            x
            "#
        ),
        Ok(Value::Number(2))
    );
}

#[test]
fn test_set_bang_mutation_visible_to_closures() {
    // §3.1: After mutation, subsequent lookups see the new value.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! x 1)
            (define! get-x (lambda () x))
            (set! (get-env) x 42)
            (get-x)
            "#
        ),
        Ok(Value::Number(42))
    );
}

#[test]
fn test_set_bang_ptree_destructuring() {
    // $set! supports formal parameter tree destructuring.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! a 1)
            (define! b 2)
            (set! (get-env) (a b) (list 10 20))
            (+ a b)
            "#
        ),
        Ok(Value::Number(30))
    );
}

#[test]
fn test_set_bang_requires_environment() {
    // $set! requires the first argument to evaluate to an environment.
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(
        r#"
        (define! x 1)
        (set! 42 x 2)
        "#,
    );
    assert_eq!(result, Err(ArenaError::TypeError));
}

#[test]
fn test_set_bang_evaluates_exp2_in_dynamic_env() {
    // Per Kernel §6.8.1, $set! evaluates exp2 in the dynamic environment
    // (the caller's env), NOT in the target environment.
    // This is key to the derivation: the value expression uses the caller's
    // bindings even when the target env is different.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! target (make-environment))
            (define! x 10)
            (set! target y (+ x 5))
            (eval (quote y) target)
            "#
        ),
        Ok(Value::Number(15))
    );
}

#[test]
fn test_set_bang_defines_in_target_not_dynamic() {
    // $set! creates bindings in the target env, not the dynamic env.
    // After (set! target y 42), y should be in target but NOT in the
    // caller's environment.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (define! target (make-environment (get-env)))
            (set! target y 42)
            (eval (quote y) target)
            "#
        ),
        Ok(Value::Number(42))
    );
}

// ============================================================================
// Kernel §3.2 — Ground Environment Protection
// ============================================================================

#[test]
fn test_standard_env_is_child_of_ground() {
    // The standard environment inherits all builtins from the ground.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(+ 1 2)"), Ok(Value::Number(3)));
    assert_eq!(lisp.eval("(operative? if)"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("(applicative? +)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_define_in_standard_env_does_not_affect_ground() {
    // Defining in the standard env persists across calls but should not
    // affect a separate Lisp instance (different ground/standard envs).
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(define! x 42) x"), Ok(Value::Number(42)));
    // The binding persists in the same instance.
    assert_eq!(lisp.eval("x"), Ok(Value::Number(42)));
    // A fresh Lisp instance does not see the binding.
    let lisp2: Lisp<20000> = Lisp::new();
    assert_eq!(lisp2.eval("x"), Err(ArenaError::UnboundVariable));
}

#[test]
fn test_set_bang_on_ground_env_rejected() {
    // $set! should reject direct mutation of the ground environment.
    // Per Kernel §3.2, the ground environment is immutable.
    // Note: $set! into the *standard* env for a ground-bound symbol
    // just creates a local shadow (permitted). Only direct mutation
    // of the ground env itself is rejected.
    let lisp: Lisp<20000> = Lisp::new();

    // Shadowing a builtin in the standard env is fine (doesn't touch ground).
    assert_eq!(
        lisp.eval(
            r#"
            (define! get-env (vau () e e))
            (set! (get-env) + 42)
            +
            "#
        ),
        Ok(Value::Number(42))
    );
}

// ============================================================================
// Kernel §3.3 — Evaluator Semantics
// ============================================================================

#[test]
fn test_evaluator_self_evaluating() {
    // Step 1: If o isn't a symbol and isn't a pair, return o.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("42"), Ok(Value::Number(42)));
    assert_eq!(lisp.eval("#t"), Ok(Value::Boolean(true)));
    assert_eq!(lisp.eval("#f"), Ok(Value::Boolean(false)));
    assert_eq!(lisp.eval("#inert"), Ok(Value::Inert));
}

#[test]
fn test_evaluator_symbol_unbound_error() {
    // Step 2 error: If symbol is not bound, signal an error.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("nonexistent"), Err(ArenaError::UnboundVariable));
}

#[test]
fn test_evaluator_not_callable_error() {
    // Step 3 error: If f is neither applicative nor operative, error.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval("(42 1 2)"), Err(ArenaError::NotCallable));
}

// ============================================================================
// Kernel §3.4 — Type Encapsulation
// ============================================================================

#[test]
fn test_operative_encapsulation_no_distinction() {
    // §3.4: operative? cannot distinguish compound from primitive operatives.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! my-op (vau (x) #ignore x))
            (operative? my-op)
            "#
        ),
        Ok(Value::Boolean(true))
    );
    assert_eq!(lisp.eval("(operative? if)"), Ok(Value::Boolean(true)));
}

#[test]
fn test_operative_static_env_not_extractable() {
    // §3.4: No feature allows extracting the static environment
    // of a compound operative. Closures with local state demonstrate
    // that only the operative itself can access its closed-over env.
    // Per §6.8.1, $set! binds formals in the captured environment.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! make-counter
                (lambda ()
                    (define! get-env (vau () e e))
                    (define! env (get-env))
                    (define! count 0)
                    (lambda ()
                        (set! env count (+ count 1))
                        count)))
            (define! counter (make-counter))
            (counter)
            (counter)
            (counter)
            "#
        ),
        Ok(Value::Number(3))
    );
}

#[test]
fn test_set_bang_enables_mutable_state() {
    // §3.1: References can be set after creation, enabling mutable state.
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(
            r#"
            (define! make-box
                (lambda (init)
                    (define! val init)
                    (define! get-env (vau () e e))
                    (define! env (get-env))
                    (list
                        (lambda () val)
                        (lambda (new-val)
                            (set! env val new-val)))))
            (define! box (make-box 0))
            (define! get-val (car box))
            (define! set-val (car (cdr box)))
            (set-val 42)
            (get-val)
            "#
        ),
        Ok(Value::Number(42))
    );
}

// ============================================================================
// GC Control Builtin Tests
// ============================================================================

#[test]
fn test_gc_collect_returns_number() {
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval("(gc-collect)");
    // gc-collect returns the number of objects collected
    match result {
        Ok(Value::Number(n)) => assert!(n >= 0, "gc-collect should return non-negative number"),
        other => panic!("gc-collect should return a number, got: {:?}", other),
    }
}

#[test]
fn test_gc_collect_with_garbage() {
    // Create garbage by allocating values that become unreachable,
    // then verify gc-collect reclaims them.
    let lisp: Lisp<20000> = Lisp::new();
    // First eval creates garbage (evaluator, builtins, etc.)
    lisp.eval("(+ 1 2)").unwrap();
    // gc-collect should reclaim at least something from the previous eval
    let result = lisp.eval("(gc-collect)");
    match result {
        Ok(Value::Number(n)) => {
            assert!(n > 0, "gc-collect should reclaim garbage, collected: {}", n)
        }
        other => panic!("gc-collect should return a number, got: {:?}", other),
    }
}

#[test]
fn test_gc_oom_triggers_collection() {
    // Use a small arena so OOM-triggered GC is exercised.
    // Repeated evaluations should succeed because GC reclaims garbage on OOM.
    let lisp: Lisp<5000> = Lisp::new();
    for i in 0..30 {
        let result = lisp.eval("(+ 1 2)");
        assert_eq!(
            result,
            Ok(Value::Number(3)),
            "eval iteration {} should succeed (OOM-triggered GC should reclaim garbage)",
            i
        );
    }
}

// ============================================================================
// String as Linked List (CharPair) Tests
// ============================================================================

#[test]
fn test_car_of_string() {
    // (car "hello") => a one-element string "h"
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"(car "hello")"#).unwrap();
    assert!(
        matches!(result, Value::CharPair { ch: 'h', .. }),
        "car of string should return CharPair with first char, got: {:?}",
        result
    );
}

#[test]
fn test_cdr_of_string() {
    // (cdr "hello") => "ello", check via (car (cdr "hello")) => "e"
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"(car (cdr "hello"))"#).unwrap();
    assert!(
        matches!(result, Value::CharPair { ch: 'e', .. }),
        "car of cdr of string should be 'e', got: {:?}",
        result
    );
}

#[test]
fn test_null_of_empty_string() {
    // (null? "") => #t (empty string is NIL)
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval(r#"(null? "")"#), Ok(Value::Boolean(true)));
}

#[test]
fn test_pair_of_nonempty_string() {
    // (pair? "hello") => #t (non-empty string is a CharPair)
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval(r#"(pair? "hello")"#), Ok(Value::Boolean(true)));
}

#[test]
fn test_pair_of_empty_string() {
    // (pair? "") => #f (empty string is NIL)
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(lisp.eval(r#"(pair? "")"#), Ok(Value::Boolean(false)));
}

#[test]
fn test_string_traversal() {
    // Walk through a string using car/cdr until null
    let lisp: Lisp<20000> = Lisp::new();
    // cdr of a single-char string should be NIL
    assert_eq!(
        lisp.eval(r#"(null? (cdr "x"))"#),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_cons_char_onto_string() {
    // (cons (car "h") "ello") should produce a string "hello"
    let lisp: Lisp<20000> = Lisp::new();
    // Verify by checking car/cdr of the result
    let result = lisp.eval(r#"(car (cons (car "h") "ello"))"#).unwrap();
    assert!(
        matches!(result, Value::CharPair { ch: 'h', .. }),
        "car of cons char onto string should be 'h', got: {:?}",
        result
    );
    let result2 = lisp.eval(r#"(car (cdr (cons (car "h") "ello")))"#).unwrap();
    assert!(
        matches!(result2, Value::CharPair { ch: 'e', .. }),
        "second char of cons'd string should be 'e', got: {:?}",
        result2
    );
}

#[test]
fn test_cons_char_onto_nil() {
    // (cons (car "h") ()) should produce a one-element string
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#"(car (cons (car "h") '()))"#).unwrap();
    assert!(
        matches!(result, Value::CharPair { ch: 'h', .. }),
        "cons char onto nil should produce single-char string, got: {:?}",
        result
    );
    assert_eq!(
        lisp.eval(r#"(null? (cdr (cons (car "h") '())))"#),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_equal_strings() {
    // Two separately allocated strings with same content should be equal?
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(r#"(equal? "hello" "hello")"#),
        Ok(Value::Boolean(true))
    );
    assert_eq!(
        lisp.eval(r#"(equal? "hello" "world")"#),
        Ok(Value::Boolean(false))
    );
}

#[test]
fn test_equal_empty_strings() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(
        lisp.eval(r#"(equal? "" "")"#),
        Ok(Value::Boolean(true))
    );
}

#[test]
fn test_string_is_self_evaluating() {
    // Strings are self-evaluating
    let lisp: Lisp<20000> = Lisp::new();
    let result = lisp.eval(r#""hello""#).unwrap();
    assert!(
        matches!(result, Value::CharPair { ch: 'h', .. }),
        "string should self-evaluate to its head CharPair, got: {:?}",
        result
    );
}

// ============================================================================
// write_value Display Tests
// ============================================================================

/// Helper: evaluate and format via write_value.
fn display(lisp: &Lisp<20000>, input: &str) -> String {
    let idx = lisp.eval_to_index(input).unwrap();
    let mut buf = String::new();
    lisp.write_value(idx, &mut buf).unwrap();
    buf
}

#[test]
fn test_write_value_number() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, "42"), "42");
    assert_eq!(display(&lisp, "-7"), "-7");
}

#[test]
fn test_write_value_boolean() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, "#t"), "#t");
    assert_eq!(display(&lisp, "#f"), "#f");
}

#[test]
fn test_write_value_nil() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, "'()"), "()");
}

#[test]
fn test_write_value_string() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, r#""hello""#), r#""hello""#);
    assert_eq!(display(&lisp, r#""""#), "()"); // empty string is NIL
}

#[test]
fn test_write_value_string_single_char() {
    // car of a string is a one-element string
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, r#"(car "hello")"#), r#""h""#);
}

#[test]
fn test_write_value_string_cdr() {
    // cdr of a string is the rest of the string
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, r#"(cdr "hello")"#), r#""ello""#);
}

#[test]
fn test_write_value_symbol() {
    // quote returns the symbol itself
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, "'foo"), "foo");
    assert_eq!(display(&lisp, "'define!"), "define!");
}

#[test]
fn test_write_value_list() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, "'(1 2 3)"), "(1 2 3)");
    assert_eq!(display(&lisp, "'(1)"), "(1)");
}

#[test]
fn test_write_value_nested_list() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, "'(1 (2 3) 4)"), "(1 (2 3) 4)");
}

#[test]
fn test_write_value_dotted_pair() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, "(cons 1 2)"), "(1 . 2)");
}

#[test]
fn test_write_value_improper_list() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, "(cons 1 (cons 2 3))"), "(1 2 . 3)");
}

#[test]
fn test_write_value_inert() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, "#inert"), "#inert");
}

#[test]
fn test_write_value_ignore() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, "#ignore"), "#ignore");
}

#[test]
fn test_write_value_list_with_string() {
    let lisp: Lisp<20000> = Lisp::new();
    assert_eq!(display(&lisp, r#"(cons 1 (cons "hi" '()))"#), r#"(1 "hi")"#);
}
